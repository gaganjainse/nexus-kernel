use std::sync::{atomic::AtomicUsize, Arc};

use nexusaos_kernel::{
    capability::CapabilitySet,
    error::NexusError,
    policy::{PolicyDecision, PolicyEngine},
    tools::broker::ToolBroker,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    net::UnixListener,
};
use tracing::info;

use crate::{
    client::{McpRequest, McpResponse},
    McpResult, McpServerConfig,
};

/// An MCP server that exposes NexusAOS tools via the MCP protocol.
pub struct McpServer {
    config: McpServerConfig,
    tool_broker: Arc<ToolBroker>,
    policy: Arc<PolicyEngine>,
    capabilities: Arc<CapabilitySet>,
    active_connections: Arc<AtomicUsize>,
}

impl McpServer {
    /// Create a new MCP server.
    pub fn new(
        config: McpServerConfig,
        tool_broker: Arc<ToolBroker>,
        policy: Arc<PolicyEngine>,
        capabilities: Arc<CapabilitySet>,
    ) -> Self {
        Self {
            config,
            tool_broker,
            policy,
            capabilities,
            active_connections: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Run the MCP server, listening for connections on the configured Unix socket.
    pub async fn run(&self) -> McpResult<()> {
        tokio::fs::remove_file(&self.config.socket_path).await.ok();
        let listener = UnixListener::bind(&self.config.socket_path).map_err(NexusError::Io)?;

        info!(socket = %self.config.socket_path, "MCP server listening");

        loop {
            let (stream, _addr) = listener.accept().await.map_err(NexusError::Io)?;
            let current = self.active_connections.load(std::sync::atomic::Ordering::Relaxed);
            if current >= self.config.max_connections {
                tracing::warn!(
                    max = self.config.max_connections,
                    "MCP connection limit reached, rejecting"
                );
                continue;
            }
            self.active_connections.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let broker = self.tool_broker.clone();
            let policy = self.policy.clone();
            let caps = self.capabilities.clone();
            let active_connections = self.active_connections.clone();

            tokio::spawn(async move {
                let result = handle_connection(stream, broker, policy, caps).await;
                active_connections.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                if let Err(e) = result {
                    tracing::error!(error = %e, "MCP connection error");
                }
            });
        }
    }
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    tool_broker: Arc<ToolBroker>,
    policy: Arc<PolicyEngine>,
    capabilities: Arc<CapabilitySet>,
) -> McpResult<()> {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = tokio::io::BufReader::new(reader);
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

        let req: McpRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let resp = McpResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(crate::client::McpError {
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

        let resp = handle_request(&req, &tool_broker, &policy, &capabilities).await;
        let resp_json = serde_json::to_string(&resp).unwrap_or_default();
        writer.write_all(resp_json.as_bytes()).await.map_err(NexusError::Io)?;
        writer.write_all(b"\n").await.map_err(NexusError::Io)?;
        writer.flush().await.map_err(NexusError::Io)?;
    }

    Ok(())
}

async fn handle_request(
    req: &McpRequest,
    broker: &ToolBroker,
    policy: &PolicyEngine,
    caps: &CapabilitySet,
) -> McpResponse {
    match req.method.as_str() {
        "tools/list" => {
            let tools = broker.available_tools();
            let tool_infos: Vec<crate::client::McpToolInfo> = tools
                .iter()
                .map(|name| crate::client::McpToolInfo {
                    name: name.clone(),
                    description: None,
                    input_schema: serde_json::json!({"type": "object"}),
                })
                .collect();
            McpResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(
                    serde_json::to_value(crate::client::McpToolList { tools: tool_infos })
                        .unwrap_or_default(),
                ),
                error: None,
                id: req.id.clone(),
            }
        }
        "tools/call" => {
            let params = req.params.as_ref();
            let tool_name =
                params.and_then(|p| p.get("name")).and_then(|v| v.as_str()).unwrap_or("unknown");
            let arguments =
                params.and_then(|p| p.get("arguments")).cloned().unwrap_or(serde_json::json!({}));

            let decision = super::validate_mcp_request(policy, tool_name, &arguments).await;
            match decision {
                PolicyDecision::Deny(reason) => McpResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(crate::client::McpError { code: -32000, message: reason }),
                    id: req.id.clone(),
                },
                PolicyDecision::RequireConfirmation(reason) => McpResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(crate::client::McpError { code: -32001, message: reason }),
                    id: req.id.clone(),
                },
                PolicyDecision::Allow => {
                    if !super::check_mcp_capabilities(caps, tool_name, &arguments) {
                        McpResponse {
                            jsonrpc: "2.0".to_string(),
                            result: None,
                            error: Some(crate::client::McpError {
                                code: -32002,
                                message: "Capability check failed".into(),
                            }),
                            id: req.id.clone(),
                        }
                    } else {
                        match broker
                            .execute(&nexusaos_kernel::tools::executor::ToolRequest {
                                tool_name: tool_name.to_string(),
                                arguments: arguments.clone(),
                            })
                            .await
                        {
                            Ok(nexusaos_kernel::tools::broker::BrokerResult::Completed(result)) => {
                                McpResponse {
                                    jsonrpc: "2.0".to_string(),
                                    result: Some(serde_json::json!({
                                        "content": result.output,
                                        "success": result.success,
                                    })),
                                    error: None,
                                    id: req.id.clone(),
                                }
                            }
                            Ok(nexusaos_kernel::tools::broker::BrokerResult::Denied(reason)) => {
                                McpResponse {
                                    jsonrpc: "2.0".to_string(),
                                    result: None,
                                    error: Some(crate::client::McpError {
                                        code: -32003,
                                        message: reason,
                                    }),
                                    id: req.id.clone(),
                                }
                            }
                            Ok(
                                nexusaos_kernel::tools::broker::BrokerResult::RequiresConfirmation(
                                    reason,
                                ),
                            ) => McpResponse {
                                jsonrpc: "2.0".to_string(),
                                result: None,
                                error: Some(crate::client::McpError {
                                    code: -32004,
                                    message: reason,
                                }),
                                id: req.id.clone(),
                            },
                            Err(e) => McpResponse {
                                jsonrpc: "2.0".to_string(),
                                result: None,
                                error: Some(crate::client::McpError {
                                    code: -32005,
                                    message: e.to_string(),
                                }),
                                id: req.id.clone(),
                            },
                        }
                    }
                }
            }
        }
        "ping" => McpResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(serde_json::json!({"pong": true})),
            error: None,
            id: req.id.clone(),
        },
        _ => McpResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(crate::client::McpError {
                code: -32601,
                message: format!("Method not found: {}", req.method),
            }),
            id: req.id.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_request_ping() {
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            method: "ping".to_string(),
            params: None,
            id: Some("1".to_string()),
        };
        let broker = ToolBroker::new(Arc::new(PolicyEngine::deny_all()));
        let policy = PolicyEngine::deny_all();
        let caps = CapabilitySet::new();
        let resp = handle_request(&req, &broker, &policy, &caps).await;
        assert_eq!(resp.result.unwrap(), serde_json::json!({"pong": true}));
    }

    #[tokio::test]
    async fn test_handle_request_tools_list() {
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            method: "tools/list".to_string(),
            params: None,
            id: Some("1".to_string()),
        };
        let broker = ToolBroker::new(Arc::new(PolicyEngine::deny_all()));
        let policy = PolicyEngine::deny_all();
        let caps = CapabilitySet::new();
        let resp = handle_request(&req, &broker, &policy, &caps).await;
        assert!(resp.result.is_some());
    }

    #[tokio::test]
    async fn test_handle_request_unknown_method() {
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            method: "unknown".to_string(),
            params: None,
            id: Some("1".to_string()),
        };
        let broker = ToolBroker::new(Arc::new(PolicyEngine::deny_all()));
        let policy = PolicyEngine::deny_all();
        let caps = CapabilitySet::new();
        let resp = handle_request(&req, &broker, &policy, &caps).await;
        assert!(resp.error.is_some());
    }

    #[test]
    fn test_mcp_server_new() {
        let broker = Arc::new(ToolBroker::new(Arc::new(PolicyEngine::deny_all())));
        let policy = Arc::new(PolicyEngine::deny_all());
        let caps = Arc::new(CapabilitySet::new());
        let config = McpServerConfig::default();
        let server = McpServer::new(config, broker, policy, caps);
        assert_eq!(server.config.socket_path, "/tmp/nexusaos-mcp.sock");
    }
}
