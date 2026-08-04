use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

use nexusaos_kernel::{
    capability::{Capability, CapabilityLease, CapabilitySet, Scope},
    error::NexusError,
    policy::{PolicyDecision, PolicyEngine},
};

use crate::{AcpAgent, AcpResult};

/// The state of an ACP session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcpSessionState {
    /// Session has been created but not yet authenticated.
    Pending,
    /// Session is authenticated and active.
    Active,
    /// Session has been suspended (e.g., capability revoked).
    Suspended,
    /// Session has been terminated.
    Terminated,
}

/// An ACP session with an agent.
#[derive(Clone)]
pub struct AcpSession {
    pub session_id: String,
    pub agent: AcpAgent,
    pub state: AcpSessionState,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub granted_capabilities: Arc<RwLock<CapabilitySet>>,
}

impl AcpSession {
    /// Create a new ACP session for an agent.
    pub fn new(agent: AcpAgent, default_ttl_seconds: u64) -> Self {
        let now = Utc::now();
        let expires_at = if default_ttl_seconds > 0 {
            Some(now + chrono::Duration::seconds(default_ttl_seconds as i64))
        } else {
            None
        };

        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            agent,
            state: AcpSessionState::Pending,
            created_at: now,
            expires_at,
            granted_capabilities: Arc::new(RwLock::new(CapabilitySet::new())),
        }
    }

    /// Authenticate the session, transitioning from Pending to Active.
    pub async fn authenticate(&mut self) -> AcpResult<()> {
        if self.state != AcpSessionState::Pending {
            return Err(NexusError::Policy(nexusaos_kernel::error::PolicyError::InvalidRule {
                message: format!("Cannot authenticate session in state {:?}", self.state),
            }));
        }
        self.state = AcpSessionState::Active;
        info!(session = %self.session_id, "ACP session authenticated");
        Ok(())
    }

    /// Grant a capability to the session's agent.
    pub async fn grant_capability(
        &self,
        capability: Capability,
        granted_by: String,
        ttl: Option<std::time::Duration>,
    ) -> AcpResult<CapabilityLease> {
        if self.state != AcpSessionState::Active {
            return Err(NexusError::Policy(nexusaos_kernel::error::PolicyError::Denied {
                reason: "Session is not active".into(),
            }));
        }
        let mut caps = self.granted_capabilities.write().await;
        let lease = caps.grant(capability, granted_by, ttl);
        Ok(lease.clone())
    }

    /// Check if the session has a specific capability.
    pub async fn has_capability(&self, name: &str) -> bool {
        let caps = self.granted_capabilities.read().await;
        caps.has_capability(name)
    }

    /// Check if the session's capabilities cover a given scope.
    pub async fn covers_scope(&self, scope: &Scope) -> bool {
        let caps = self.granted_capabilities.read().await;
        match scope {
            Scope::Path(p) => caps.check_path(p),
            Scope::Command(cmd) => caps.check_command(cmd),
            Scope::Global => true,
            _ => caps.leases.iter().any(|l| l.is_valid() && l.capability.scope == *scope),
        }
    }

    /// Suspend the session (e.g., when capabilities are revoked).
    pub async fn suspend(&mut self) {
        self.state = AcpSessionState::Suspended;
        info!(session = %self.session_id, "ACP session suspended");
    }

    /// Terminate the session.
    pub async fn terminate(&mut self) {
        self.state = AcpSessionState::Terminated;
        info!(session = %self.session_id, "ACP session terminated");
    }

    /// Returns true if the session is active and not expired.
    pub fn is_active(&self) -> bool {
        if self.state != AcpSessionState::Active {
            return false;
        }
        if let Some(expires) = self.expires_at {
            return Utc::now() < expires;
        }
        true
    }
}

/// ACP session manager that handles multiple sessions.
pub struct AcpSessionManager {
    sessions: Arc<RwLock<Vec<AcpSession>>>,
    max_sessions: usize,
    default_ttl_seconds: u64,
    policy: Arc<PolicyEngine>,
}

impl AcpSessionManager {
    /// Create a new ACP session manager.
    pub fn new(
        max_sessions: usize,
        default_ttl_seconds: u64,
        policy: Arc<PolicyEngine>,
    ) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(Vec::new())),
            max_sessions,
            default_ttl_seconds,
            policy,
        }
    }

    /// Create a new session for an agent.
    pub async fn create_session(&self, agent: AcpAgent) -> AcpResult<AcpSession> {
        let mut sessions = self.sessions.write().await;
        if sessions.len() >= self.max_sessions {
            return Err(NexusError::Policy(nexusaos_kernel::error::PolicyError::Denied {
                reason: "Maximum session count reached".into(),
            }));
        }
        let mut session = AcpSession::new(agent, self.default_ttl_seconds);
        session.authenticate().await?;
        sessions.push(session.clone());
        Ok(session)
    }

    /// Get a session by ID.
    pub async fn get_session(&self, session_id: &str) -> Option<AcpSession> {
        let sessions = self.sessions.read().await;
        sessions.iter().find(|s| s.session_id == session_id).cloned()
    }

    /// List all active sessions.
    pub async fn active_sessions(&self) -> Vec<AcpSession> {
        let sessions = self.sessions.read().await;
        sessions.iter().filter(|s| s.is_active()).cloned().collect()
    }

    /// Terminate a session by ID.
    pub async fn terminate_session(&self, session_id: &str) -> AcpResult<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.iter_mut().find(|s| s.session_id == session_id) {
            session.terminate().await;
            Ok(())
        } else {
            Err(NexusError::Policy(nexusaos_kernel::error::PolicyError::InvalidRule {
                message: format!("Session not found: {}", session_id),
            }))
        }
    }

    /// Evaluate a policy decision for an ACP action.
    pub async fn evaluate_policy(
        &self,
        agent_id: &str,
        action: &str,
    ) -> PolicyDecision {
        let full_action = format!("acp.{}.{}", agent_id, action);
        self.policy.evaluate(&full_action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexusaos_kernel::capability::{Capability, Scope};

    fn test_agent() -> AcpAgent {
        AcpAgent {
            id: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            capabilities: Arc::new(CapabilitySet::new()),
        }
    }

    #[tokio::test]
    async fn test_session_create() {
        let policy = PolicyEngine::deny_all();
        let manager = AcpSessionManager::new(10, 3600, Arc::new(policy));
        let agent = test_agent();
        let session = manager.create_session(agent).await.unwrap();
        assert_eq!(session.state, AcpSessionState::Active);
        assert!(session.is_active());
    }

    #[tokio::test]
    async fn test_session_grant_capability() {
        let policy = PolicyEngine::deny_all();
        let manager = AcpSessionManager::new(10, 3600, Arc::new(policy));
        let agent = test_agent();
        let session = manager.create_session(agent).await.unwrap();

        let cap = Capability {
            name: "fs_read".to_string(),
            scope: Scope::Path(std::path::PathBuf::from("/tmp")),
            description: "read /tmp".to_string(),
        };
        let lease = session.grant_capability(cap, "admin".to_string(), None).await.unwrap();
        assert!(!lease.revoked);
    }

    #[tokio::test]
    async fn test_session_terminate() {
        let policy = PolicyEngine::deny_all();
        let manager = AcpSessionManager::new(10, 3600, Arc::new(policy));
        let agent = test_agent();
        let session = manager.create_session(agent).await.unwrap();
        let session_id = session.session_id.clone();

        manager.terminate_session(&session_id).await.unwrap();
        let updated = manager.get_session(&session_id).await.unwrap();
        assert_eq!(updated.state, AcpSessionState::Terminated);
    }

    #[tokio::test]
    async fn test_session_max_sessions() {
        let policy = PolicyEngine::deny_all();
        let manager = AcpSessionManager::new(2, 3600, Arc::new(policy));

        let agent1 = AcpAgent {
            id: "agent-1".to_string(),
            name: "Agent 1".to_string(),
            capabilities: Arc::new(CapabilitySet::new()),
        };
        let agent2 = AcpAgent {
            id: "agent-2".to_string(),
            name: "Agent 2".to_string(),
            capabilities: Arc::new(CapabilitySet::new()),
        };
        let agent3 = AcpAgent {
            id: "agent-3".to_string(),
            name: "Agent 3".to_string(),
            capabilities: Arc::new(CapabilitySet::new()),
        };

        manager.create_session(agent1).await.unwrap();
        manager.create_session(agent2).await.unwrap();
        let result = manager.create_session(agent3).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_is_active() {
        let policy = PolicyEngine::deny_all();
        let manager = AcpSessionManager::new(10, 3600, Arc::new(policy));
        let agent = test_agent();
        let session = manager.create_session(agent).await.unwrap();
        assert!(session.is_active());
    }
}