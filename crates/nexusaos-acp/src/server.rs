use std::sync::Arc;

use tokio::net::UnixListener;
use tracing::info;

use nexusaos_kernel::error::NexusError;

use crate::{AcpAgent, AcpResult, AcpSessionManager};

/// ACP server configuration.
#[derive(Debug, Clone)]
pub struct AcpServerConfig {
    pub socket_path: String,
    pub max_connections: usize,
}

impl Default for AcpServerConfig {
    fn default() -> Self {
        Self {
            socket_path: "/tmp/nexusaos-acp.sock".to_string(),
            max_connections: 16,
        }
    }
}

/// An ACP server that listens for incoming ACP client connections.
pub struct AcpServer {
    config: AcpServerConfig,
    session_manager: Arc<AcpSessionManager>,
}

impl AcpServer {
    /// Create a new ACP server.
    pub fn new(config: AcpServerConfig, session_manager: Arc<AcpSessionManager>) -> Self {
        Self { config, session_manager }
    }

    /// Run the ACP server, listening for connections on the configured Unix socket.
    pub async fn run(&self) -> AcpResult<()> {
        tokio::fs::remove_file(&self.config.socket_path).await.ok();
        let listener = UnixListener::bind(&self.config.socket_path)
            .map_err(|e| NexusError::Io(e))?;

        info!(socket = %self.config.socket_path, "ACP server listening");

        loop {
            let (stream, _addr) = listener.accept().await.map_err(|e| NexusError::Io(e))?;
            let manager = self.session_manager.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, manager).await {
                    tracing::error!(error = %e, "ACP connection error");
                }
            });
        }
    }
}

async fn handle_connection(
    _stream: tokio::net::UnixStream,
    _manager: Arc<AcpSessionManager>,
) -> AcpResult<()> {
    Ok(())
}
