//! Project state summaries for NexusAOS.
//!
//! Stores derived summaries separately from raw events for
//! performance optimization and audit completeness.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

use crate::events::{Event, EventPayload};

/// A derived project summary computed from the event log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub project_id: String,
    pub summary: String,
    pub task_count: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub last_updated: DateTime<Utc>,
    pub ttl_seconds: u64,
}

impl ProjectSummary {
    /// Create a new project summary.
    pub fn new(project_id: String, ttl_seconds: u64) -> Self {
        Self {
            project_id,
            summary: String::new(),
            task_count: 0,
            completed_tasks: 0,
            failed_tasks: 0,
            last_updated: Utc::now(),
            ttl_seconds,
        }
    }

    /// Check if the summary has expired based on its TTL.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.last_updated + chrono::Duration::seconds(self.ttl_seconds as i64)
    }

    /// Update the summary with new event data.
    pub fn update(&mut self, events: &[Event]) {
        let mut task_count = 0;
        let mut completed_tasks = 0;
        let mut failed_tasks = 0;

        for event in events {
            if let EventPayload::StateChanged { from: _from, to } = &event.payload {
                task_count += 1;
                if to == "Completed" {
                    completed_tasks += 1;
                } else if to == "Failed" {
                    failed_tasks += 1;
                }
            }
        }

        self.task_count = task_count;
        self.completed_tasks = completed_tasks;
        self.failed_tasks = failed_tasks;
        self.last_updated = Utc::now();

        let summary = format!(
            "Project {}: {} tasks, {} completed, {} failed",
            self.project_id, self.task_count, self.completed_tasks, self.failed_tasks
        );
        self.summary = summary;

        info!(project = %self.project_id, "Project summary updated");
    }
}

/// Project summary store with TTL-based caching.
pub struct ProjectSummaryStore {
    summaries: Arc<RwLock<Vec<ProjectSummary>>>,
    default_ttl_seconds: u64,
}

impl ProjectSummaryStore {
    /// Create a new project summary store.
    pub fn new(default_ttl_seconds: u64) -> Self {
        Self {
            summaries: Arc::new(RwLock::new(Vec::new())),
            default_ttl_seconds,
        }
    }

    /// Get or create a summary for a project.
    pub async fn get_or_create(&self, project_id: &str) -> ProjectSummary {
        let mut summaries = self.summaries.write().await;
        if let Some(summary) = summaries.iter_mut().find(|s| s.project_id == project_id) {
            if summary.is_expired() {
                summary.update(&[]);
            }
            return summary.clone();
        }

        let mut summary = ProjectSummary::new(project_id.to_string(), self.default_ttl_seconds);
        summary.update(&[]);
        summaries.push(summary.clone());
        summary
    }

    /// Update a project summary with new events.
    pub async fn update_summary(&self, project_id: &str, events: &[Event]) -> ProjectSummary {
        let mut summaries = self.summaries.write().await;
        if let Some(summary) = summaries.iter_mut().find(|s| s.project_id == project_id) {
            summary.update(events);
            return summary.clone();
        }

        let mut summary = ProjectSummary::new(project_id.to_string(), self.default_ttl_seconds);
        summary.update(events);
        summaries.push(summary.clone());
        summary
    }

    /// Get all summaries.
    pub async fn list_all(&self) -> Vec<ProjectSummary> {
        self.summaries.read().await.clone()
    }

    /// Remove expired summaries.
    pub async fn cleanup_expired(&self) {
        let mut summaries = self.summaries.write().await;
        summaries.retain(|s| !s.is_expired());
    }
}

impl Default for ProjectSummaryStore {
    fn default() -> Self {
        Self::new(3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Event, EventId, EventKind, EventPayload, SequenceNumber};
    use crate::task::TaskId;

    #[test]
    fn test_project_summary_creation() {
        let summary = ProjectSummary::new("proj-1".to_string(), 3600);
        assert_eq!(summary.project_id, "proj-1");
        assert_eq!(summary.task_count, 0);
        assert!(!summary.is_expired());
    }

    #[test]
    fn test_project_summary_update() {
        let mut summary = ProjectSummary::new("proj-1".to_string(), 3600);
        let events = vec![
            Event {
                id: EventId::new(),
                task_id: Some(TaskId::new()),
                sequence: SequenceNumber(1),
                kind: EventKind::TaskStateChanged,
                payload: EventPayload::StateChanged {
                    from: "Received".to_string(),
                    to: "Completed".to_string(),
                },
                metadata: crate::events::EventMetadata {
                    source: "test".to_string(),
                    correlation_id: None,
                },
                timestamp: Utc::now(),
            },
        ];
        summary.update(&events);
        assert_eq!(summary.completed_tasks, 1);
        assert!(summary.summary.contains("Completed"));
    }

    #[test]
    fn test_project_summary_expiry() {
        let mut summary = ProjectSummary::new("proj-1".to_string(), 0);
        summary.last_updated = Utc::now() - chrono::Duration::hours(2);
        assert!(summary.is_expired());
    }

    #[tokio::test]
    async fn test_summary_store_get_or_create() {
        let store = ProjectSummaryStore::new(3600);
        let summary = store.get_or_create("proj-1").await;
        assert_eq!(summary.project_id, "proj-1");
    }

    #[tokio::test]
    async fn test_summary_store_update() {
        let store = ProjectSummaryStore::new(3600);
        let events = vec![
            Event {
                id: EventId::new(),
                task_id: Some(TaskId::new()),
                sequence: SequenceNumber(1),
                kind: EventKind::TaskStateChanged,
                payload: EventPayload::StateChanged {
                    from: "Received".to_string(),
                    to: "Completed".to_string(),
                },
                metadata: crate::events::EventMetadata {
                    source: "test".to_string(),
                    correlation_id: None,
                },
                timestamp: Utc::now(),
            },
        ];
        let summary = store.update_summary("proj-1", &events).await;
        assert_eq!(summary.completed_tasks, 1);
    }

    #[tokio::test]
    async fn test_summary_store_cleanup() {
        let store = ProjectSummaryStore::new(0);
        store.get_or_create("proj-1").await;
        store.cleanup_expired().await;
        let summaries = store.list_all().await;
        assert!(summaries.is_empty());
    }
}