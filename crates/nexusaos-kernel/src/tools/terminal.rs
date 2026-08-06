use std::time::Duration;

use async_trait::async_trait;
use tokio::{process::Command, time::timeout};

use super::executor::{ToolExecutor, ToolRequest, ToolResult};
use crate::error::ToolError;

/// Sandboxed terminal command execution.
pub struct TerminalTool {
    timeout_secs: u64,
    denied_prefixes: Vec<String>,
    /// If true, refuse to run commands when bwrap is unavailable.
    require_sandbox: bool,
}

impl TerminalTool {
    pub fn new(timeout_secs: u64, denied_prefixes: Vec<String>) -> Self {
        Self { timeout_secs, denied_prefixes, require_sandbox: true }
    }

    pub fn without_sandbox(mut self) -> Self {
        self.require_sandbox = false;
        self
    }

    /// Resolve the bwrap binary through PATH.
    fn resolve_bwrap() -> Option<std::path::PathBuf> {
        if let Ok(path) = std::env::var("PATH") {
            for dir in path.split(':') {
                let candidate = std::path::Path::new(dir).join("bwrap");
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    fn is_command_denied(&self, command: &str) -> bool {
        self.denied_prefixes.iter().any(|prefix| command.starts_with(prefix))
    }
}

#[async_trait]
impl ToolExecutor for TerminalTool {
    fn name(&self) -> &str {
        "terminal"
    }

    fn description(&self) -> &str {
        "Sandboxed terminal command execution"
    }

    fn is_destructive(&self) -> bool {
        true
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, ToolError> {
        let command =
            request.arguments.get("command").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::ExecutionFailed {
                    name: self.name().to_string(),
                    reason: "Missing 'command' argument".to_string(),
                }
            })?;

        if self.is_command_denied(command) {
            return Err(ToolError::CommandDenied { command: command.to_string() });
        }

        let bwrap_path = Self::resolve_bwrap();
        let has_bwrap = bwrap_path.is_some();

        if self.require_sandbox && !has_bwrap {
            return Err(ToolError::ExecutionFailed {
                name: self.name().to_string(),
                reason: "Sandbox required but bwrap not found on PATH".to_string(),
            });
        }

        let mut cmd = if has_bwrap {
            let mut bwrap = Command::new("bwrap");
            bwrap
                .arg("--ro-bind")
                .arg("/")
                .arg("/")
                .arg("--dev")
                .arg("/dev")
                .arg("--proc")
                .arg("/proc")
                .arg("--tmpfs")
                .arg("/tmp")
                .arg("--unshare-all")
                .arg("--share-net")
                .arg("--")
                .arg("sh")
                .arg("-c")
                .arg(command);
            bwrap
        } else {
            let mut sh = Command::new("sh");
            sh.arg("-c").arg(command);
            sh
        };

        let future = cmd.output();

        match timeout(Duration::from_secs(self.timeout_secs), future).await {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

                Ok(ToolResult {
                    success: output.status.success(),
                    output: if output.status.success() { stdout } else { stderr },
                    data: None,
                })
            }
            Ok(Err(e)) => Err(ToolError::Io(e)),
            Err(_) => Err(ToolError::Timeout {
                name: self.name().to_string(),
                timeout_secs: self.timeout_secs,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn test_terminal_tool() -> Result<(), Box<dyn std::error::Error>> {
        let tool = TerminalTool::new(5, vec!["rm -rf".to_string()]).without_sandbox();

        let req_denied = ToolRequest {
            tool_name: "terminal".to_string(),
            arguments: json!({ "command": "rm -rf /" }),
        };

        let Err(err) = tool.execute(&req_denied).await else {
            return Err("expected the terminal tool to fail".into());
        };
        match err {
            ToolError::CommandDenied { command } => assert_eq!(command, "rm -rf /"),
            other => return Err(format!("expected CommandDenied, got {other:?}").into()),
        }

        let req_allowed = ToolRequest {
            tool_name: "terminal".to_string(),
            arguments: json!({ "command": "echo test" }),
        };

        let res = tool.execute(&req_allowed).await?;
        assert!(res.success);
        assert_eq!(res.output.trim(), "test");
        Ok(())
    }

    #[tokio::test]
    async fn test_terminal_missing_command() -> Result<(), Box<dyn std::error::Error>> {
        let tool = TerminalTool::new(5, vec![]).without_sandbox();

        let req = ToolRequest { tool_name: "terminal".to_string(), arguments: json!({}) };
        let Err(err) = tool.execute(&req).await else {
            return Err("expected the terminal tool to fail".into());
        };
        match err {
            ToolError::ExecutionFailed { reason, .. } => {
                assert!(reason.contains("Missing 'command' argument"))
            }
            other => return Err(format!("expected ExecutionFailed, got {other:?}").into()),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_terminal_command_denied_prefix() -> Result<(), Box<dyn std::error::Error>> {
        let tool =
            TerminalTool::new(5, vec!["sudo".to_string(), "dd".to_string()]).without_sandbox();

        let req = ToolRequest {
            tool_name: "terminal".to_string(),
            arguments: json!({ "command": "sudo apt update" }),
        };
        let Err(err) = tool.execute(&req).await else {
            return Err("expected the terminal tool to fail".into());
        };
        match err {
            ToolError::CommandDenied { command } => assert_eq!(command, "sudo apt update"),
            other => return Err(format!("expected CommandDenied, got {other:?}").into()),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_terminal_command_denied_partial_match() -> Result<(), Box<dyn std::error::Error>>
    {
        let tool = TerminalTool::new(5, vec!["rm -rf".to_string()]).without_sandbox();

        let req = ToolRequest {
            tool_name: "terminal".to_string(),
            arguments: json!({ "command": "rm -rf /tmp/foo" }),
        };
        let Err(err) = tool.execute(&req).await else {
            return Err("expected the terminal tool to fail".into());
        };
        match err {
            ToolError::CommandDenied { command } => assert_eq!(command, "rm -rf /tmp/foo"),
            other => return Err(format!("expected CommandDenied, got {other:?}").into()),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_terminal_no_denied_prefixes() -> Result<(), Box<dyn std::error::Error>> {
        let tool = TerminalTool::new(5, vec![]).without_sandbox();

        let req = ToolRequest {
            tool_name: "terminal".to_string(),
            arguments: json!({ "command": "echo hello" }),
        };
        let res = tool.execute(&req).await?;
        assert!(res.success);
        Ok(())
    }

    #[tokio::test]
    async fn test_terminal_timeout() -> Result<(), Box<dyn std::error::Error>> {
        let tool = TerminalTool::new(1, vec![]).without_sandbox();

        let req = ToolRequest {
            tool_name: "terminal".to_string(),
            arguments: json!({ "command": "sleep 10" }),
        };
        let Err(err) = tool.execute(&req).await else {
            return Err("expected the terminal tool to fail".into());
        };
        match err {
            ToolError::Timeout { timeout_secs, .. } => assert_eq!(timeout_secs, 1),
            other => return Err(format!("expected Timeout, got {other:?}").into()),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_terminal_command_fails_nonzero_exit() -> Result<(), Box<dyn std::error::Error>> {
        let tool = TerminalTool::new(5, vec![]).without_sandbox();

        let req = ToolRequest {
            tool_name: "terminal".to_string(),
            arguments: json!({ "command": "exit 1" }),
        };
        let res = tool.execute(&req).await?;
        assert!(!res.success);
        Ok(())
    }

    #[tokio::test]
    async fn test_terminal_empty_output() -> Result<(), Box<dyn std::error::Error>> {
        let tool = TerminalTool::new(5, vec![]).without_sandbox();

        let req = ToolRequest {
            tool_name: "terminal".to_string(),
            arguments: json!({ "command": "true" }),
        };
        let res = tool.execute(&req).await?;
        assert!(res.success);
        assert!(res.output.is_empty() || res.output.trim().is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_terminal_with_stderr() -> Result<(), Box<dyn std::error::Error>> {
        let tool = TerminalTool::new(5, vec![]).without_sandbox();

        let req = ToolRequest {
            tool_name: "terminal".to_string(),
            arguments: json!({ "command": "echo out" }),
        };
        let res = tool.execute(&req).await?;
        assert!(res.success);
        assert!(res.output.contains("out"));
        Ok(())
    }

    #[tokio::test]
    async fn test_terminal_no_match_denied_prefix() -> Result<(), Box<dyn std::error::Error>> {
        let tool = TerminalTool::new(5, vec!["rm -rf /".to_string()]).without_sandbox();

        let req = ToolRequest {
            tool_name: "terminal".to_string(),
            arguments: json!({ "command": "echo safe" }),
        };
        let res = tool.execute(&req).await?;
        assert!(res.success);
        assert_eq!(res.output.trim(), "safe");
        Ok(())
    }

    #[tokio::test]
    async fn test_terminal_exact_denied_prefix() -> Result<(), Box<dyn std::error::Error>> {
        let tool = TerminalTool::new(5, vec!["ls".to_string()]).without_sandbox();

        let req = ToolRequest {
            tool_name: "terminal".to_string(),
            arguments: json!({ "command": "ls" }),
        };
        let Err(err) = tool.execute(&req).await else {
            return Err("expected the terminal tool to fail".into());
        };
        match err {
            ToolError::CommandDenied { command } => assert_eq!(command, "ls"),
            other => return Err(format!("expected CommandDenied, got {other:?}").into()),
        }
        Ok(())
    }
}
