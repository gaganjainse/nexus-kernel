use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::net::UnixStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::info;

use nexusaos_kernel::error::NexusError;

/// MCP protocol version.
pub const MCP_VERSION: &str = "2024-10-01";

/// An MCP client that connects to an MCP server over a Unix socket.
#[derive(Debug, Clone)]
pub struct McpClient {
    socket_path: String,
}

/// A request sent to an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: Option<String>,
}

/// A response from an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<McpError>,
    pub id: Option<String>,
}

/// An MCP error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub code: i64,
    pub message: String,
}

/// Tools listed by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolList {
    pub tools: Vec<McpToolInfo>,
}

/// Information about a tool exposed by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

impl McpClient {
    /// Create a new MCP client pointing at the given socket path.
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self { socket_path: socket_path.into() }
    }

    /// Connect to the MCP server and list available tools.
    pub async fn list_tools(&self) -> Result<McpToolList, NexusError> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            method: "tools/list".to_string(),
            params: None,
            id: Some("list-tools".to_string()),
        };
        let req_json = serde_json::to_string(&req)?;
        writer.write_all(req_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let resp: McpResponse = serde_json::from_str(line.trim())?;

        match resp.result {
            Some(val) => {
                let tools: McpToolList = serde_json::from_value(val)?;
                info!(count = tools.tools.len(), "MCP tools listed");
                Ok(tools)
            }
            None => {
                let msg = resp.error.map(|e| e.message).unwrap_or_else(|| "unknown error".into());
                Err(NexusError::Provider(nexusaos_kernel::error::ProviderError::Unavailable { name: msg }))
            }
        }
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, NexusError> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": tool_name,
                "arguments": arguments,
            })),
            id: Some(tool_name),
        };
        let req_json = serde_json::to_string(&req)?;
        writer.write_all(req_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let resp: McpResponse = serde_json::from_str(line.trim())?;

        match resp.result {
            Some(val) => Ok(val),
            None => {
                let msg = resp.error.map(|e| e.message).unwrap_or_else(|| "unknown error".into());
                Err(NexusError::Provider(nexusaos_kernel::error::ProviderError::Unavailable { name: msg }))
            }
        }
    }

    /// Send a ping to the MCP server.
    pub async fn ping(&self) -> Result<bool, NexusError> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        let req = McpRequest {
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
        let resp: McpResponse = serde_json::from_str(line.trim())?;

        Ok(resp.result.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_request_serde() {
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            method: "tools/list".to_string(),
            params: None,
            id: Some("1".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: McpRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.method, "tools/list");
    }

    #[test]
    fn test_mcp_response_serde() {
        let resp = McpResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(serde_json::json!({"tools": []})),
            error: None,
            id: Some("1".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: McpResponse = serde_json::from_str(&json).unwrap();
        assert!(back.result.is_some());
    }

    #[test]
    fn test_mcp_client_new() {
        let client = McpClient::new("/tmp/test.sock");
        assert_eq!(client.socket_path, "/tmp/test.sock");
    }
}