//! `nexusaos acp` — Run the ACP protocol server standalone.

use std::sync::Arc;

use tracing::info;

use nexusaos_kernel::{
    config::AppConfig,
    error::NexusError,
    policy::{PolicyEngine, PolicyRule, TrustTier},
};

/// Run the ACP server.
pub fn run(config_path: &str) -> Result<(), NexusError> {
    info!("Starting ACP server");

    let _config = AppConfig::load(config_path)?;

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

        let session_manager = Arc::new(nexusaos_acp::session::AcpSessionManager::new(
            100,
            3600,
            policy_arc,
        ));
        let acp_config = nexusaos_acp::server::AcpServerConfig {
            socket_path: "/tmp/nexusaos-acp.sock".to_string(),
            max_connections: 16,
        };
        let acp_server = nexusaos_acp::server::AcpServer::new(acp_config, session_manager);

        info!("ACP server starting on /tmp/nexusaos-acp.sock");
        acp_server.run().await?;
        Ok::<(), NexusError>(())
    })
}
