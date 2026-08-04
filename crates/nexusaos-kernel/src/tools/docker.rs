use async_trait::async_trait;
use tracing::info;

use super::executor::{ToolExecutor, ToolRequest, ToolResult};
use crate::error::ToolError;

/// Docker container operations tool.
pub struct DockerTool {
    allowed_images: Vec<String>,
    denied_images: Vec<String>,
}

impl DockerTool {
    pub fn new(allowed_images: Vec<String>, denied_images: Vec<String>) -> Self {
        Self {
            allowed_images,
            denied_images,
        }
    }

    /// Parse a Docker image reference into normalized components.
    /// Returns (registry, name, tag) where registry may be empty.
    fn parse_image(image: &str) -> (&str, &str, Option<&str>) {
        let (image, tag) = if let Some(idx) = image.rfind(':') {
            let (name, tag_part) = image.split_at(idx);
            let tag = tag_part.strip_prefix(':').filter(|t| !t.is_empty() && !t.contains('/'));
            (name, tag)
        } else {
            (image, None)
        };

        let (registry, name) = if let Some(idx) = image.find('/') {
            image.split_at(idx)
        } else {
            ("", image)
        };

        (registry, name, tag)
    }

    fn is_image_allowed(&self, image: &str) -> bool {
        let normalized = Self::normalize_image(image);
        for pattern in &self.denied_images {
            let pattern_normalized = Self::normalize_image(pattern);
            if normalized == pattern_normalized {
                return false;
            }
        }
        for allowed in &self.allowed_images {
            let allowed_normalized = Self::normalize_image(allowed);
            if normalized == allowed_normalized {
                return true;
            }
        }
        false
    }

    /// Normalize an image reference for comparison.
    /// Adds implicit library/ prefix for official Docker Hub images.
    fn normalize_image(image: &str) -> String {
        let (registry, name, tag) = Self::parse_image(image);
        let name = if registry.is_empty() && !name.starts_with("library/") {
            format!("library/{}", name)
        } else {
            name.to_string()
        };
        match tag {
            Some(t) => format!("{}{}:{}", registry, name, t),
            None => format!("{}{}", registry, name),
        }
    }
}

#[async_trait]
impl ToolExecutor for DockerTool {
    fn name(&self) -> &str {
        "docker"
    }

    fn description(&self) -> &str {
        "Manage Docker containers (run, stop, inspect)"
    }

    fn is_destructive(&self) -> bool {
        true
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, ToolError> {
        let action = request
            .arguments
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::ExecutionFailed {
                name: self.name().to_string(),
                reason: "Missing 'action' argument".to_string(),
            })?;

        let image = request
            .arguments
            .get("image")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !image.is_empty() && !self.is_image_allowed(image) {
            return Err(ToolError::ExecutionFailed {
                name: self.name().to_string(),
                reason: format!("Docker image not allowed: {}", image),
            });
        }

        info!(action = %action, image = %image, "Docker tool executing");

        let output = match action {
            "run" => {
                let cmd = format!("docker run --rm {}", image);
                self.run_docker_command(&cmd).await?
            }
            "stop" => {
                let container_id = request
                    .arguments
                    .get("container_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let cmd = format!("docker stop {}", container_id);
                self.run_docker_command(&cmd).await?
            }
            "inspect" => {
                let container_id = request
                    .arguments
                    .get("container_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let cmd = format!("docker inspect {}", container_id);
                self.run_docker_command(&cmd).await?
            }
            "ps" => {
                self.run_docker_command("docker ps").await?
            }
            _ => {
                return Err(ToolError::ExecutionFailed {
                    name: self.name().to_string(),
                    reason: format!("Unknown Docker action: {}", action),
                });
            }
        };

        Ok(ToolResult {
            success: true,
            output,
            data: Some(serde_json::json!({
                "action": action,
                "image": image,
            })),
        })
    }
}

impl DockerTool {
    async fn run_docker_command(&self, cmd: &str) -> Result<String, ToolError> {
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: self.name().to_string(),
                reason: format!("Docker command failed: {}", e),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(ToolError::ExecutionFailed {
                name: self.name().to_string(),
                reason: format!("Docker command failed: {}", stderr),
            });
        }

        Ok(if stdout.is_empty() {
            format!("Command succeeded (no output). stderr: {}", stderr)
        } else {
            stdout
        })
    }
}