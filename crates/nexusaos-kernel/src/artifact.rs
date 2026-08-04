//! Execution artifacts for NexusAOS.
//!
//! Stores execution artifacts (tool outputs, file changes, etc.)
//! separately from the event log for audit completeness.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::{error::NexusError, task::TaskId};

/// Types of execution artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactKind {
    /// Tool output.
    ToolOutput,
    /// File change.
    FileChange,
    /// Command output.
    CommandOutput,
    /// Model response.
    ModelResponse,
    /// Custom artifact.
    Custom(String),
}

/// An execution artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub task_id: TaskId,
    pub kind: ArtifactKind,
    pub content: String,
    pub metadata: serde_json::Value,
    pub size_bytes: usize,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl Artifact {
    /// Create a new artifact.
    pub fn new(
        task_id: TaskId,
        kind: ArtifactKind,
        content: String,
        metadata: serde_json::Value,
    ) -> Self {
        let now = Utc::now();
        let size_bytes = content.len();
        Self {
            id: Uuid::new_v4().to_string(),
            task_id,
            kind,
            content,
            metadata,
            size_bytes,
            created_at: now,
            expires_at: None,
        }
    }

    /// Create an artifact with an expiration time.
    pub fn new_with_ttl(
        task_id: TaskId,
        kind: ArtifactKind,
        content: String,
        metadata: serde_json::Value,
        ttl_seconds: u64,
    ) -> Self {
        let mut artifact = Self::new(task_id, kind, content, metadata);
        artifact.expires_at = Some(Utc::now() + chrono::Duration::seconds(ttl_seconds as i64));
        artifact
    }

    /// Check if the artifact has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            Utc::now() > expires
        } else {
            false
        }
    }
}

/// Artifact storage with cleanup policy.
pub struct ArtifactStore {
    artifacts: Arc<RwLock<Vec<Artifact>>>,
    max_age_seconds: u64,
    max_size_bytes: u64,
}

impl ArtifactStore {
    /// Create a new artifact store.
    pub fn new(max_age_seconds: u64, max_size_bytes: u64) -> Self {
        Self { artifacts: Arc::new(RwLock::new(Vec::new())), max_age_seconds, max_size_bytes }
    }

    /// Store an artifact.
    pub async fn store(&self, artifact: Artifact) -> Result<(), NexusError> {
        let mut artifacts = self.artifacts.write().await;

        // Check size limit
        let total_size: usize = artifacts.iter().map(|a| a.size_bytes).sum();
        if total_size + artifact.size_bytes > self.max_size_bytes as usize {
            // Remove oldest artifacts to make room
            artifacts.sort_by_key(|a| a.created_at);
            while total_size + artifact.size_bytes > self.max_size_bytes as usize
                && !artifacts.is_empty()
            {
                let removed = artifacts.remove(0);
                info!(artifact = %removed.id, "Artifact removed due to size limit");
            }
        }

        artifacts.push(artifact);
        Ok(())
    }

    /// Get an artifact by ID.
    pub async fn get(&self, id: &str) -> Option<Artifact> {
        let artifacts = self.artifacts.read().await;
        artifacts.iter().find(|a| a.id == id && !a.is_expired()).cloned()
    }

    /// Get all artifacts for a task.
    pub async fn get_by_task(&self, task_id: &TaskId) -> Vec<Artifact> {
        let artifacts = self.artifacts.read().await;
        artifacts.iter().filter(|a| a.task_id == *task_id && !a.is_expired()).cloned().collect()
    }

    /// Get all non-expired artifacts.
    pub async fn list_all(&self) -> Vec<Artifact> {
        let artifacts = self.artifacts.read().await;
        artifacts.iter().filter(|a| !a.is_expired()).cloned().collect()
    }

    /// Remove expired artifacts.
    pub async fn cleanup_expired(&self) {
        let mut artifacts = self.artifacts.write().await;
        let before = artifacts.len();
        artifacts.retain(|a| !a.is_expired());
        let removed = before - artifacts.len();
        if removed > 0 {
            info!(count = removed, "Expired artifacts cleaned up");
        }
    }

    /// Remove artifacts older than max_age_seconds.
    pub async fn cleanup_by_age(&self) {
        let mut artifacts = self.artifacts.write().await;
        let cutoff = Utc::now() - chrono::Duration::seconds(self.max_age_seconds as i64);
        let before = artifacts.len();
        artifacts.retain(|a| a.created_at > cutoff);
        let removed = before - artifacts.len();
        if removed > 0 {
            info!(count = removed, "Aged artifacts cleaned up");
        }
    }
}

impl Default for ArtifactStore {
    fn default() -> Self {
        Self::new(86400, 100 * 1024 * 1024) // 24h max age, 100MB max size
    }
}

/// Record an artifact from a tool execution result.
pub fn record_artifact_from_tool_result(
    task_id: TaskId,
    tool_name: &str,
    result: &crate::tools::executor::ToolResult,
) -> Artifact {
    let kind = ArtifactKind::ToolOutput;
    let metadata = serde_json::json!({
        "tool_name": tool_name,
        "success": result.success,
    });
    Artifact::new(task_id, kind, result.output.clone(), metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::executor::ToolResult;

    #[test]
    fn test_artifact_creation() {
        let artifact = Artifact::new(
            TaskId::new(),
            ArtifactKind::ToolOutput,
            "output content".to_string(),
            serde_json::json!({"tool": "fs"}),
        );
        assert_eq!(artifact.kind, ArtifactKind::ToolOutput);
        assert_eq!(artifact.size_bytes, 14);
    }

    #[test]
    fn test_artifact_with_ttl() {
        let artifact = Artifact::new_with_ttl(
            TaskId::new(),
            ArtifactKind::FileChange,
            "file content".to_string(),
            serde_json::json!({"path": "/tmp/test"}),
            3600,
        );
        assert!(artifact.expires_at.is_some());
    }

    #[test]
    fn test_artifact_not_expired() {
        let artifact = Artifact::new(
            TaskId::new(),
            ArtifactKind::ToolOutput,
            "output".to_string(),
            serde_json::json!({}),
        );
        assert!(!artifact.is_expired());
    }

    #[tokio::test]
    async fn test_artifact_store() {
        let store = ArtifactStore::new(86400, 100_000);
        let artifact = Artifact::new(
            TaskId::new(),
            ArtifactKind::ToolOutput,
            "test output".to_string(),
            serde_json::json!({"tool": "test"}),
        );
        let id = artifact.id.clone();
        store.store(artifact).await.unwrap();

        let retrieved = store.get(&id).await.unwrap();
        assert_eq!(retrieved.content, "test output");
    }

    #[tokio::test]
    async fn test_artifact_store_get_by_task() {
        let store = ArtifactStore::new(86400, 100_000);
        let task_id = TaskId::new();
        let artifact = Artifact::new(
            task_id,
            ArtifactKind::ToolOutput,
            "test output".to_string(),
            serde_json::json!({"tool": "test"}),
        );
        store.store(artifact).await.unwrap();

        let artifacts = store.get_by_task(&task_id).await;
        assert_eq!(artifacts.len(), 1);
    }

    #[tokio::test]
    async fn test_artifact_store_cleanup_expired() {
        let store = ArtifactStore::new(0, 100_000);
        let artifact = Artifact::new_with_ttl(
            TaskId::new(),
            ArtifactKind::ToolOutput,
            "test output".to_string(),
            serde_json::json!({"tool": "test"}),
            0,
        );
        store.store(artifact).await.unwrap();
        store.cleanup_expired().await;

        let all = store.list_all().await;
        assert!(all.is_empty());
    }

    #[test]
    fn test_record_artifact_from_tool_result() {
        let result = ToolResult {
            success: true,
            output: "tool output".to_string(),
            data: Some(serde_json::json!({"bytes": 1024})),
        };
        let artifact = record_artifact_from_tool_result(TaskId::new(), "fs.read", &result);
        assert_eq!(artifact.kind, ArtifactKind::ToolOutput);
        assert!(artifact.content.contains("tool output"));
    }
}
