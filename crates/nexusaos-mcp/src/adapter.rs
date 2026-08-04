use std::sync::Arc;

use async_trait::async_trait;
use tracing::info;

use nexusaos_kernel::{
    tools::executor::{ToolExecutor, ToolRequest, ToolResult},
    capability::CapabilitySet,
    error::ToolError,
};

/// An MCP tool adapter that wraps a ToolExecutor for MCP protocol exposure.
///
/// This adapter bridges the MCP protocol with the existing ToolExecutor trait,
/// adding capability checks and policy validation before execution.
pub struct McpToolAdapter {
    inner: Arc<dyn ToolExecutor>,
    capabilities: Arc<CapabilitySet>,
}

impl McpToolAdapter {
    /// Create a new MCP tool adapter wrapping an existing ToolExecutor.
    pub fn new(
        inner: Arc<dyn ToolExecutor>,
        capabilities: Arc<CapabilitySet>,
    ) -> Self {
        Self { inner, capabilities }
    }

    /// Returns the inner tool executor.
    pub fn inner(&self) -> &dyn ToolExecutor {
        self.inner.as_ref()
    }
}

#[async_trait]
impl ToolExecutor for McpToolAdapter {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn is_destructive(&self) -> bool {
        self.inner.is_destructive()
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, ToolError> {
        info!(tool = %request.tool_name, "MCP tool adapter executing");

        if let Some(path) = request.arguments.get("path").and_then(|v| v.as_str()) {
            if !self.capabilities.check_path(std::path::Path::new(path)) {
                return Err(ToolError::PathDenied { path: path.to_string() });
            }
        }

        if let Some(cmd) = request.arguments.get("command").and_then(|v| v.as_str()) {
            if !self.capabilities.check_command(cmd) {
                return Err(ToolError::CommandDenied { command: cmd.to_string() });
            }
        }

        self.inner.execute(request).await
    }
}