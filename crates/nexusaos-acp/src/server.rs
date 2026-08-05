use std::sync::Arc;

use nexusaos_kernel::error::NexusError;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};
use tracing::info;

use crate::{
    client::{AcpRequest, AcpResponse},
    session::AcpSessionManager,
    AcpResult,
};

/// ACP server configuration.
#[derive(Debug, Clone)]
pub struct AcpServerConfig {
    pub socket_path: String,
    pub max_connections: usize,
}

impl Default for AcpServerConfig {
    fn default() -> Self {
        Self { socket_path: "/tmp/nexusaos-acp.sock".to_string(), max_connections: 16 }
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
        let listener = UnixListener::bind(&self.config.socket_path).map_err(NexusError::Io)?;

        info!(socket = %self.config.socket_path, "ACP server listening");

        loop {
            let (stream, _addr) = listener.accept().await.map_err(NexusError::Io)?;
            let manager = self.session_manager.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, manager).await {
                    tracing::error!(error = %e, "ACP connection error");
                }
            });
        }
    }
}

async fn handle_connection(stream: UnixStream, manager: Arc<AcpSessionManager>) -> AcpResult<()> {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await.map_err(NexusError::Io)?;
        if bytes_read == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: AcpRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let resp = AcpResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(crate::client::AcpError {
                        code: -32700,
                        message: format!("Parse error: {}", e),
                    }),
                    id: None,
                };
                let resp_json = serde_json::to_string(&resp).unwrap_or_default();
                let _ = writer.write_all(resp_json.as_bytes()).await;
                let _ = writer.write_all(b"\n").await;
                continue;
            }
        };

        let resp = handle_request(&req, &manager).await;
        let resp_json = serde_json::to_string(&resp).unwrap_or_default();
        writer.write_all(resp_json.as_bytes()).await.map_err(NexusError::Io)?;
        writer.write_all(b"\n").await.map_err(NexusError::Io)?;
        writer.flush().await.map_err(NexusError::Io)?;
    }

    Ok(())
}

async fn handle_request(req: &AcpRequest, manager: &AcpSessionManager) -> AcpResponse {
    match req.method.as_str() {
        "authenticate" => {
            let agent_id = req
                .params
                .as_ref()
                .and_then(|p| p.get("agent_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let decision =
                crate::validate_acp_request(manager.policy(), agent_id, "authenticate").await;
            if !matches!(decision, nexusaos_kernel::policy::PolicyDecision::Allow) {
                return AcpResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(crate::client::AcpError {
                        code: -32000,
                        message: format!("Policy denied: {:?}", decision),
                    }),
                    id: req.id.clone(),
                };
            }

            let agent = crate::AcpAgent {
                id: agent_id.to_string(),
                name: agent_id.to_string(),
                capabilities: Arc::new(tokio::sync::RwLock::new(
                    nexusaos_kernel::capability::CapabilitySet::new(),
                )),
            };

            match manager.create_session(agent.clone()).await {
                Ok(session) => {
                    let result = serde_json::json!({
                        "id": session.session_id,
                        "agent_id": agent.id,
                        "name": agent.name,
                        "state": "Active",
                    });
                    AcpResponse {
                        jsonrpc: "2.0".to_string(),
                        result: Some(result),
                        error: None,
                        id: req.id.clone(),
                    }
                }
                Err(e) => AcpResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(crate::client::AcpError { code: -32001, message: e.to_string() }),
                    id: req.id.clone(),
                },
            }
        }
        "capability/grant" => {
            let agent_id = req
                .params
                .as_ref()
                .and_then(|p| p.get("agent_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let decision =
                crate::validate_acp_request(manager.policy(), agent_id, "capability_grant").await;
            if !matches!(decision, nexusaos_kernel::policy::PolicyDecision::Allow) {
                return AcpResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(crate::client::AcpError {
                        code: -32000,
                        message: format!("Policy denied: {:?}", decision),
                    }),
                    id: req.id.clone(),
                };
            }

            let scope_json = req
                .params
                .as_ref()
                .and_then(|p| p.get("scope"))
                .cloned()
                .unwrap_or(serde_json::json!({"type": "Global"}));

            let scope: nexusaos_kernel::capability::Scope = match serde_json::from_value(scope_json)
            {
                Ok(s) => s,
                Err(e) => {
                    return AcpResponse {
                        jsonrpc: "2.0".to_string(),
                        result: None,
                        error: Some(crate::client::AcpError {
                            code: -32002,
                            message: format!("Invalid scope: {}", e),
                        }),
                        id: req.id.clone(),
                    }
                }
            };

            let capability = nexusaos_kernel::capability::Capability {
                name: format!("acp.{}", agent_id),
                scope,
                description: "ACP granted capability".to_string(),
            };

            let ttl_seconds =
                req.params.as_ref().and_then(|p| p.get("ttl_seconds")).and_then(|v| v.as_u64());

            let agent = crate::AcpAgent {
                id: agent_id.to_string(),
                name: agent_id.to_string(),
                capabilities: Arc::new(tokio::sync::RwLock::new(
                    nexusaos_kernel::capability::CapabilitySet::new(),
                )),
            };

            match manager.create_session(agent).await {
                Ok(session) => {
                    let ttl = ttl_seconds.map(std::time::Duration::from_secs);
                    match session.grant_capability(capability, "acp-server".to_string(), ttl).await
                    {
                        Ok(lease) => {
                            let result = serde_json::json!({
                                "granted": true,
                                "lease_id": lease.id.to_string(),
                                "expires_at": lease.expires_at.map(|d| d.to_rfc3339()),
                            });
                            AcpResponse {
                                jsonrpc: "2.0".to_string(),
                                result: Some(result),
                                error: None,
                                id: req.id.clone(),
                            }
                        }
                        Err(e) => AcpResponse {
                            jsonrpc: "2.0".to_string(),
                            result: None,
                            error: Some(crate::client::AcpError {
                                code: -32003,
                                message: e.to_string(),
                            }),
                            id: req.id.clone(),
                        },
                    }
                }
                Err(e) => AcpResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(crate::client::AcpError { code: -32001, message: e.to_string() }),
                    id: req.id.clone(),
                },
            }
        }
        "ping" => AcpResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(serde_json::json!({"pong": true})),
            error: None,
            id: req.id.clone(),
        },
        _ => AcpResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(crate::client::AcpError {
                code: -32601,
                message: format!("Method not found: {}", req.method),
            }),
            id: req.id.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use nexusaos_kernel::policy::PolicyEngine;

    use super::*;

    #[tokio::test]
    async fn test_handle_request_ping() -> Result<(), Box<dyn std::error::Error>> {
        let manager =
            Arc::new(AcpSessionManager::new(10, 3600, Arc::new(PolicyEngine::deny_all())));
        let req = AcpRequest {
            jsonrpc: "2.0".to_string(),
            method: "ping".to_string(),
            params: None,
            id: Some("1".to_string()),
        };
        let resp = handle_request(&req, &manager).await;
        assert_eq!(
            resp.result.ok_or("response result was None")?,
            serde_json::json!({"pong": true})
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_handle_request_unknown_method() -> Result<(), Box<dyn std::error::Error>> {
        let manager =
            Arc::new(AcpSessionManager::new(10, 3600, Arc::new(PolicyEngine::deny_all())));
        let req = AcpRequest {
            jsonrpc: "2.0".to_string(),
            method: "unknown".to_string(),
            params: None,
            id: Some("1".to_string()),
        };
        let resp = handle_request(&req, &manager).await;
        assert!(resp.error.is_some());
        Ok(())
    }
}
