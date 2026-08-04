use async_trait::async_trait;
use tracing::info;

use super::executor::{ToolExecutor, ToolRequest, ToolResult};
use crate::error::ToolError;

/// Search and fetch tool for retrieving external content.
pub struct SearchFetchTool {
    allowed_domains: Vec<String>,
    denied_patterns: Vec<String>,
}

impl SearchFetchTool {
    pub fn new(allowed_domains: Vec<String>, denied_patterns: Vec<String>) -> Self {
        Self { allowed_domains, denied_patterns }
    }

    fn is_url_allowed(&self, url: &str) -> bool {
        let host = match url::Url::parse(url) {
            Ok(u) => u.host_str().unwrap_or("").to_string(),
            Err(_) => return false,
        };

        for pattern in &self.denied_patterns {
            if host == pattern.as_str() {
                return false;
            }
        }
        for domain in &self.allowed_domains {
            if host == domain.as_str() {
                return true;
            }
        }
        false
    }
}

#[async_trait]
impl ToolExecutor for SearchFetchTool {
    fn name(&self) -> &str {
        "search_fetch"
    }

    fn description(&self) -> &str {
        "Search and fetch content from allowed URLs"
    }

    fn is_destructive(&self) -> bool {
        false
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, ToolError> {
        let url = request.arguments.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::ExecutionFailed {
                name: self.name().to_string(),
                reason: "Missing 'url' argument".to_string(),
            }
        })?;

        if !self.is_url_allowed(url) {
            return Err(ToolError::PathDenied { path: url.to_string() });
        }

        let query = request.arguments.get("query").and_then(|v| v.as_str()).unwrap_or("");

        info!(url = %url, query = %query, "Search/fetch tool executing");

        let client = reqwest::Client::new();
        let response = client.get(url).query(&[("q", query)]).send().await.map_err(|e| {
            ToolError::ExecutionFailed {
                name: self.name().to_string(),
                reason: format!("Fetch failed: {}", e),
            }
        })?;

        let status = response.status();
        let body = response.text().await.map_err(|e| ToolError::ExecutionFailed {
            name: self.name().to_string(),
            reason: format!("Failed to read response: {}", e),
        })?;

        let truncated = if body.len() > 10_000 {
            format!("{}... [truncated, {} bytes total]", &body[..10_000], body.len())
        } else {
            body
        };

        Ok(ToolResult {
            success: status.is_success(),
            output: truncated,
            data: Some(serde_json::json!({
                "url": url,
                "status": status.as_u16(),
                "query": query,
            })),
        })
    }
}
