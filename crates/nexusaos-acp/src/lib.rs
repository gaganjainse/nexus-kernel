pub mod client;
pub mod session;

use std::sync::Arc;

use nexusaos_kernel::{
    capability::{CapabilityLease, CapabilitySet, Scope},
    error::NexusError,
    policy::{PolicyDecision, PolicyEngine},
    state::ModelRole,
};
use tracing::info;

/// Result type for ACP operations.
pub type AcpResult<T> = Result<T, NexusError>;

/// An ACP agent descriptor.
#[derive(Debug, Clone)]
pub struct AcpAgent {
    pub id: String,
    pub name: String,
    pub capabilities: Arc<CapabilitySet>,
}

/// ACP session configuration.
#[derive(Debug, Clone)]
pub struct AcpSessionConfig {
    pub max_sessions: usize,
    pub default_ttl_seconds: u64,
}

impl Default for AcpSessionConfig {
    fn default() -> Self {
        Self {
            max_sessions: 100,
            default_ttl_seconds: 3600,
        }
    }
}

/// Validates an ACP request through the policy engine.
pub async fn validate_acp_request(
    policy: &PolicyEngine,
    agent_id: &str,
    action: &str,
) -> PolicyDecision {
    let full_action = format!("acp.{}.{}", agent_id, action);
    let decision = policy.evaluate(&full_action);
    info!(agent = %agent_id, action = %action, decision = ?decision, "ACP request validated through policy");
    decision
}

/// Checks if the agent's capability set permits the requested operation.
pub fn check_acp_capabilities(
    agent: &AcpAgent,
    scope: &Scope,
) -> bool {
    for lease in &agent.capabilities.leases {
        if !lease.is_valid() {
            continue;
        }
        match scope {
            Scope::Path(p) => {
                if lease.covers_path(p) {
                    return true;
                }
            }
            Scope::Command(cmd) => {
                if lease.covers_command(cmd) {
                    return true;
                }
            }
            Scope::Model(m) => {
                if let Scope::Model(agent_model) = &lease.capability.scope {
                    if agent_model == m {
                        return true;
                    }
                }
            }
            Scope::Tool(t) => {
                if let Scope::Tool(agent_tool) = &lease.capability.scope {
                    if agent_tool == t {
                        return true;
                    }
                }
            }
            Scope::Global => return true,
        }
    }
    false
}

/// Grants a capability to an ACP agent.
pub fn grant_agent_capability(
    agent: &mut AcpAgent,
    capability: nexusaos_kernel::capability::Capability,
    granted_by: String,
    ttl: Option<std::time::Duration>,
) -> &CapabilityLease {
    agent.capabilities.grant(capability, granted_by, ttl)
}