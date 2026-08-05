//! `nexusaos mcp` — Run the MCP protocol server standalone.

use std::sync::Arc;

use nexusaos_kernel::{
    capability::{Capability, CapabilitySet, Scope},
    config::AppConfig,
    error::NexusError,
    model::{openai_compat::OpenAiCompatProvider, registry::ProviderRegistry},
    policy::{PolicyEngine, PolicyRule, TrustTier},
    tools::{broker::ToolBroker, filesystem::FilesystemTool, git::GitTool, terminal::TerminalTool},
};
use tracing::info;

/// Run the MCP server.
pub fn run(config_path: &str) -> Result<(), NexusError> {
    info!("Starting MCP server");

    let config = AppConfig::load(config_path)?;
    let data_dir = config.resolved_data_dir();

    let rt = tokio::runtime::Runtime::new().map_err(|e| {
        NexusError::Config(nexusaos_kernel::error::ConfigError::Invalid { message: e.to_string() })
    })?;
    rt.block_on(async {
        let rules = vec![PolicyRule {
            name: "allow-all".to_string(),
            action_pattern: "*".to_string(),
            decision: "allow".to_string(),
            trust_tier: 0,
            description: None,
        }];

        let policy = PolicyEngine::new(rules, TrustTier::Basic);
        let policy_arc = Arc::new(policy);

        let mut registry = ProviderRegistry::new();
        for p_cfg in &config.model_providers {
            if let Ok(provider) = OpenAiCompatProvider::new(p_cfg) {
                registry.register(Box::new(provider));
            }
        }

        let mut broker = ToolBroker::new(policy_arc.clone());
        let allowed_paths = vec![data_dir.clone()];
        broker.register(Arc::new(FilesystemTool::new(
            allowed_paths,
            config.tools.filesystem.denied_patterns.clone(),
        )));
        if config.tools.git.enabled {
            broker.register(Arc::new(GitTool::new(data_dir.clone())));
        }
        broker.register(Arc::new(TerminalTool::new(
            config.tools.terminal.timeout_secs,
            config.tools.terminal.denied_prefixes.clone(),
        )));
        let broker = Arc::new(broker);

        let mut capabilities = CapabilitySet::new();
        for tool_name in broker.available_tools() {
            capabilities.grant(
                Capability {
                    name: format!("tool.{}", tool_name),
                    scope: Scope::Global,
                    description: format!("Grant {} tool access", tool_name),
                },
                "cli".to_string(),
                None,
            );
        }
        let capabilities = Arc::new(capabilities);

        let mcp_config = nexusaos_mcp::McpServerConfig {
            socket_path: "/tmp/nexusaos-mcp.sock".to_string(),
            max_connections: 16,
        };
        let mcp_server =
            nexusaos_mcp::server::McpServer::new(mcp_config, broker, policy_arc, capabilities);

        info!("MCP server starting on /tmp/nexusaos-mcp.sock");
        mcp_server.run().await?;
        Ok::<(), NexusError>(())
    })
}
