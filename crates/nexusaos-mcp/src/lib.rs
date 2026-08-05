pub mod adapter;
pub mod client;
pub mod server;

use nexusaos_kernel::{
    capability::CapabilitySet,
    error::NexusError,
    policy::{PolicyDecision, PolicyEngine},
};
use tracing::info;

/// Result type for MCP operations.
pub type McpResult<T> = Result<T, NexusError>;

/// An MCP tool descriptor that wraps a ToolExecutor for MCP protocol exposure.
#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// MCP server configuration.
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub socket_path: String,
    pub max_connections: usize,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self { socket_path: "/tmp/nexusaos-mcp.sock".to_string(), max_connections: 16 }
    }
}

/// Validates an MCP request through the policy engine before tool execution.
pub async fn validate_mcp_request(policy: &PolicyEngine, tool_name: &str) -> PolicyDecision {
    let action = format!("mcp.{}.execute", tool_name);
    let decision = policy.evaluate(&action);
    info!(tool = %tool_name, decision = ?decision, "MCP request validated through policy");
    decision
}

/// Checks if the given capability set permits the MCP tool operation.
pub fn check_mcp_capabilities(
    capabilities: &CapabilitySet,
    tool_name: &str,
    arguments: &serde_json::Value,
) -> bool {
    if !capabilities.has_capability(tool_name) {
        return false;
    }
    if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
        if capabilities.check_path(std::path::Path::new(path)) {
            return true;
        }
    }
    if let Some(cmd) = arguments.get("command").and_then(|v| v.as_str()) {
        if capabilities.check_command(cmd) {
            return true;
        }
    }
    if let Some(image) = arguments.get("image").and_then(|v| v.as_str()) {
        if capabilities.check_path(std::path::Path::new(image)) {
            return true;
        }
    }
    if let Some(url) = arguments.get("url").and_then(|v| v.as_str()) {
        if capabilities.check_path(std::path::Path::new(url)) {
            return true;
        }
    }
    false
}
