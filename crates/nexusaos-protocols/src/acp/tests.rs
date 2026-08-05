//! Unit tests for ACP session management.

use std::sync::Arc;

use nexusaos_kernel::{
    capability::{Capability, CapabilitySet, Scope},
    policy::{PolicyEngine, PolicyRule, TrustTier},
};
use tokio::sync::RwLock;

use crate::acp::session::{AcpSessionManager, AcpSessionState};
use crate::acp::AcpAgent;

fn make_policy() -> Arc<PolicyEngine> {
    let rules = vec![PolicyRule {
        name: "allow-all".to_string(),
        action_pattern: "*".to_string(),
        decision: "allow".to_string(),
        trust_tier: 0,
        description: None,
    }];
    Arc::new(PolicyEngine::new(rules, TrustTier::Basic))
}

fn make_agent(id: &str) -> AcpAgent {
    AcpAgent {
        id: id.to_string(),
        name: id.to_string(),
        capabilities: Arc::new(RwLock::new(CapabilitySet::new())),
    }
}

#[cfg(test)]
mod acp_session_tests {

    use super::*;

    #[tokio::test]
    async fn test_session_creation() -> Result<(), Box<dyn std::error::Error>> {
        let manager = AcpSessionManager::new(10, 3600, make_policy());
        let agent = make_agent("test-agent");
        let session = manager.create_session(agent).await?;
        assert_eq!(session.state, AcpSessionState::Active);
        assert!(session.is_active());
        Ok(())
    }

    #[tokio::test]
    async fn test_session_termination() -> Result<(), Box<dyn std::error::Error>> {
        let manager = AcpSessionManager::new(10, 3600, make_policy());
        let agent = make_agent("test-agent");
        let session = manager.create_session(agent).await?;
        let session_id = session.session_id.clone();
        manager.terminate_session(&session_id).await?;

        let updated = manager.get_session(&session_id).await.ok_or("session not found")?;
        assert_eq!(updated.state, AcpSessionState::Terminated);
        Ok(())
    }

    #[tokio::test]
    async fn test_session_not_found() -> Result<(), Box<dyn std::error::Error>> {
        let manager = AcpSessionManager::new(10, 3600, make_policy());
        let result = manager.get_session("nonexistent").await;
        assert!(result.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_terminate_nonexistent_session() -> Result<(), Box<dyn std::error::Error>> {
        let manager = AcpSessionManager::new(10, 3600, make_policy());
        let result = manager.terminate_session("nonexistent").await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_active_sessions_filtering() -> Result<(), Box<dyn std::error::Error>> {
        let manager = AcpSessionManager::new(2, 3600, make_policy());
        manager.create_session(make_agent("agent-1")).await?;
        manager.create_session(make_agent("agent-2")).await?;
        let result = manager.create_session(make_agent("agent-3")).await;
        assert!(result.is_err());

        let active = manager.active_sessions().await;
        assert_eq!(active.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_session_isolation() -> Result<(), Box<dyn std::error::Error>> {
        let manager = AcpSessionManager::new(10, 3600, make_policy());
        let session1 = manager.create_session(make_agent("agent-1")).await?;
        let session2 = manager.create_session(make_agent("agent-2")).await?;

        assert_ne!(session1.session_id, session2.session_id);

        manager.terminate_session(&session1.session_id).await?;

        let session2_after =
            manager.get_session(&session2.session_id).await.ok_or("session not found")?;
        assert_eq!(session2_after.state, AcpSessionState::Active);
        assert!(session2_after.is_active());
        Ok(())
    }

    #[tokio::test]
    async fn test_max_sessions() -> Result<(), Box<dyn std::error::Error>> {
        let manager = AcpSessionManager::new(2, 3600, make_policy());
        manager.create_session(make_agent("agent-1")).await?;
        manager.create_session(make_agent("agent-2")).await?;
        let result = manager.create_session(make_agent("agent-3")).await;
        assert!(result.is_err());
        Ok(())
    }
}

#[cfg(test)]
mod acp_capability_tests {

    use super::*;

    #[tokio::test]
    async fn test_grant_capability() -> Result<(), Box<dyn std::error::Error>> {
        let manager = AcpSessionManager::new(10, 3600, make_policy());
        let agent = make_agent("test-agent");
        let session = manager.create_session(agent).await?;

        let cap = Capability {
            name: "fs_read".to_string(),
            scope: Scope::Path(std::path::PathBuf::from("/tmp")),
            description: "read /tmp".to_string(),
        };
        let lease = session.grant_capability(cap, "admin".to_string(), None).await?;
        assert!(!lease.revoked);
        Ok(())
    }

    #[tokio::test]
    async fn test_grant_capability_on_terminated_session_denied(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let manager = AcpSessionManager::new(10, 3600, make_policy());
        let agent = make_agent("test-agent");
        let session = manager.create_session(agent).await?;
        let session_id = session.session_id.clone();

        manager.terminate_session(&session_id).await?;

        let cap = Capability {
            name: "fs_read".to_string(),
            scope: Scope::Path(std::path::PathBuf::from("/tmp")),
            description: "read /tmp".to_string(),
        };
        let updated = manager.get_session(&session_id).await.ok_or("session not found")?;
        let result = updated.grant_capability(cap, "admin".to_string(), None).await;
        assert!(result.is_err());
        Ok(())
    }
}
