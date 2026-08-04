//! NexusAOS isolated worker process.
//!
//! This binary receives ToolRequest over stdin, executes the tool,
//! and emits ToolResult over stdout. It runs as a separate process
//! to provide isolation between the kernel and tool execution.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use async_trait::async_trait;
use clap::Parser;
use nexusaos_kernel::{
    error::ToolError,
    tools::executor::{ToolExecutor, ToolRequest, ToolResult},
};
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Worker ID for identification.
    #[arg(long)]
    worker_id: String,

    /// Working directory for the worker.
    #[arg(long)]
    working_dir: String,
}

/// Simple tool executor that runs commands directly.
/// In production, this would be replaced with proper tool implementations.
struct WorkerToolExecutor;

#[async_trait]
impl ToolExecutor for WorkerToolExecutor {
    fn name(&self) -> &str {
        "worker-tool-executor"
    }

    fn description(&self) -> &str {
        "Executes tools in the worker process"
    }

    fn is_destructive(&self) -> bool {
        true
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, ToolError> {
        match request.tool_name.as_str() {
            "fs.read" => {
                let path = request
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::ExecutionFailed {
                        name: "fs.read".to_string(),
                        reason: "missing path argument".to_string(),
                    })?;

                let output = std::fs::read_to_string(path)
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "fs.read".to_string(),
                        reason: format!("failed to read file: {}", e),
                    })?;

                Ok(ToolResult {
                    success: true,
                    output,
                    data: None,
                })
            }
            "fs.write" => {
                let path = request
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::ExecutionFailed {
                        name: "fs.write".to_string(),
                        reason: "missing path argument".to_string(),
                    })?;

                let content = request
                    .arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::ExecutionFailed {
                        name: "fs.write".to_string(),
                        reason: "missing content argument".to_string(),
                    })?;

                std::fs::write(path, content)
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "fs.write".to_string(),
                        reason: format!("failed to write file: {}", e),
                    })?;

                Ok(ToolResult {
                    success: true,
                    output: format!("Wrote to {}", path),
                    data: None,
                })
            }
            "terminal.exec" => {
                let command = request
                    .arguments
                    .get("command")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::ExecutionFailed {
                        name: "terminal.exec".to_string(),
                        reason: "missing command argument".to_string(),
                    })?;

                let output = Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "terminal.exec".to_string(),
                        reason: format!("failed to execute command: {}", e),
                    })?;

                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let combined = if stderr.is_empty() {
                    stdout
                } else {
                    format!("{}\n{}", stdout, stderr)
                };

                Ok(ToolResult {
                    success: output.status.success(),
                    output: combined,
                    data: None,
                })
            }
            _ => Err(ToolError::ExecutionFailed {
                name: request.tool_name.clone(),
                reason: format!("unknown tool: {}", request.tool_name),
            }),
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    info!(worker_id = %args.worker_id, working_dir = %args.working_dir, "Worker started");

    let executor = WorkerToolExecutor;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let request: ToolRequest = match serde_json::from_str(trimmed) {
                    Ok(r) => r,
                    Err(e) => {
                        error!(error = %e, "Failed to parse tool request");
                        let result = ToolResult {
                            success: false,
                            output: format!("Parse error: {}", e),
                            data: None,
                        };
                        let json = serde_json::to_string(&result).unwrap_or_default();
                        let _ = writer.write_all(json.as_bytes());
                        let _ = writer.write_all(b"\n");
                        let _ = writer.flush();
                        continue;
                    }
                };

                info!(tool = %request.tool_name, "Executing tool");
                let result = executor.execute(&request).await;
                let result = match result {
                    Ok(r) => r,
                    Err(e) => ToolResult {
                        success: false,
                        output: format!("Error: {}", e),
                        data: None,
                    },
                };

                let json = serde_json::to_string(&result).unwrap_or_default();
                let _ = writer.write_all(json.as_bytes());
                let _ = writer.write_all(b"\n");
                let _ = writer.flush();
            }
            Err(e) => {
                error!(error = %e, "Failed to read from stdin");
                break;
            }
        }
    }

    info!("Worker shutting down");
}
