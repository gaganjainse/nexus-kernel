use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::net::UnixStream;
use tokio::sync::RwLock;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::info;

use nexusaos_kernel::error::NexusError;

use crate::{AcpAgent, AcpAgentInfo, AcpResult, CapabilitySet};

/// ACP protocol version.
pub const ACP_VERSION: &str = "2024-10-01";

/// An ACP client that connects to an ACP server over a Unix socket.
#[derive(Debug, Clone)]
pub struct AcpClient {
    socket_path: String,
    #[allow(dead_code)]
    agent_id: String,
}

/// An ACP request message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: Option<String>,
}

/// An ACP response message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<AcpError>,
    pub id: Option<String>,
}

/// An ACP error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpError {
    pub code: i64,
    pub message: String,
}

/// Capability grant request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityGrantRequest {
    pub capability: serde_json::Value,
    pub ttl_seconds: Option<u64>,
}

/// Capability grant response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityGrantResponse {
    pub granted: bool,
    pub lease_id: Option<String>,
    pub expires_at: Option<String>,
}

impl AcpClient {
    /// Create a new ACP client.
    pub fn new(socket_path: impl Into<String>, agent_id: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
            agent_id: agent_id.into(),
        }
    }

    /// Authenticate with the ACP server and receive initial capabilities.
    pub async fn authenticate(&self, credentials: serde_json::Value) -> AcpResult<AcpAgent> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        let req = AcpRequest {
            jsonrpc: "2.0".to_string(),
            method: "authenticate".to_string(),
            params: Some(credentials),
            id: Some("auth".to_string()),
        };
        let req_json = serde_json::to_string(&req)?;
        writer.write_all(req_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let resp: AcpResponse = serde_json::from_str(line.trim())?;

        match resp.result {
            Some(val) => {
                let agent_info: AcpAgentInfo = serde_json::from_value(val)?;
                let agent = AcpAgent {
                    id: agent_info.id,
                    name: agent_info.name,
                    capabilities: Arc::new(RwLock::new(CapabilitySet::new())),
                };
                info!(agent = %agent.id, "ACP agent authenticated");
                Ok(agent)
            }
            None => {
                let msg = resp.error.map(|e| e.message).unwrap_or_else(|| "authentication failed".into());
                Err(NexusError::Policy(nexusaos_kernel::error::PolicyError::Denied { reason: msg }))
            }
        }
    }

    /// Request a capability grant from the ACP server.
    pub async fn request_capability(
        &self,
        agent: &AcpAgent,
        scope: &nexusaos_kernel::capability::Scope,
        ttl_seconds: Option<u64>,
    ) -> AcpResult<AcpCapabilityGrant> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        let req = AcpRequest {
            jsonrpc: "2.0".to_string(),
            method: "capability/grant".to_string(),
            params: Some(serde_json::json!({
                "agent_id": agent.id,
                "scope": scope,
                "ttl_seconds": ttl_seconds,
            })),
            id: Some("grant-cap".to_string()),
        };
        let req_json = serde_json::to_string(&req)?;
        writer.write_all(req_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let resp: AcpResponse = serde_json::from_str(line.trim())?;

        match resp.result {
            Some(val) => {
                let grant: AcpCapabilityGrant = serde_json::from_value(val)?;
                info!(agent = %agent.id, granted = grant.granted, "ACP capability grant result");
                Ok(grant)
            }
            None => {
                let msg = resp.error.map(|e| e.message).unwrap_or_else(|| "capability grant failed".into());
                Err(NexusError::Policy(nexusaos_kernel::error::PolicyError::Denied { reason: msg }))
            }
        }
    }

    /// Send a ping to the ACP server.
    pub async fn ping(&self) -> AcpResult<bool> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        let req = AcpRequest {
            jsonrpc: "2.0".to_string(),
            method: "ping".to_string(),
            params: None,
            id: Some("ping".to_string()),
        };
        let req_json = serde_json::to_string(&req)?;
        writer.write_all(req_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let resp: AcpResponse = serde_json::from_str(line.trim())?;

        Ok(resp.result.is_some())
    }
}

/// The result of a capability grant request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpCapabilityGrant {
    pub granted: bool,
    pub lease_id: Option<String>,
    pub expires_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acp_request_serde() {
        let req = AcpRequest {
            jsonrpc: "2.0".to_string(),
            method: "authenticate".to_string(),
            params: None,
            id: Some("1".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: AcpRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.method, "authenticate");
    }

    #[test]
    fn test_acp_response_serde() {
        let resp = AcpResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(serde_json::json!({"granted": true})),
            error: None,
            id: Some("1".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: AcpResponse = serde_json::from_str(&json).unwrap();
        assert!(back.result.is_some());
    }

    #[test]
    fn test_acp_client_new() {
        let client = AcpClient::new("/tmp/test.sock", "agent-1");
        assert_eq!(client.socket_path, "/tmp/test.sock");
        assert_eq!(client.agent_id, "agent-1");
    }
}