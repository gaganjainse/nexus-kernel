// src/events.rs - Event sourcing types for NexusAOS
// All types derive Debug, Clone, Serialize, Deserialize

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::task::TaskId;

/// Unique event identifier (UUIDv7)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId(pub Uuid);

impl EventId {
    /// Create a new EventId using UUIDv7
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Monotonically increasing sequence number within the event store
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SequenceNumber(pub u64);

/// Categories of events
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    // Task lifecycle
    TaskCreated,
    TaskClassified,
    TaskStateChanged,
    // Model interactions
    ModelRequested,
    ModelResponded,
    ModelFailed,
    // Tool interactions
    ToolRequested,
    ToolCompleted,
    ToolFailed,
    // Policy
    PolicyChecked,
    PolicyDenied,
    PolicyDecision,
    ConfirmationRequested,
    ConfirmationGranted,
    ConfirmationDenied,
    // System
    CheckpointCreated,
    SnapshotCreated,
    SystemStarted,
    SystemShutdown,
    Error,
    // MCP
    McpRequest,
    McpResponse,
    // ACP
    AcpSessionCreated,
    AcpSessionTerminated,
    AcpCapabilityGranted,
    AcpCapabilityRevoked,
    // Resource
    ResourceBudgetExceeded,
    ResourceBudgetChecked,
    // Manifest
    ManifestCreated,
    ManifestValidated,
    ManifestSigned,
    ManifestActivated,
    ManifestSuperseded,
    ManifestRetired,
    // Artifact
    ArtifactRecorded,
    // Summary
    ProjectSummaryUpdated,
}

/// The payload of an event — what actually happened
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventPayload {
    TaskCreated { request: serde_json::Value },
    StateChanged { from: String, to: String },
    ModelRequest { role: String, prompt_tokens: usize, context_budget: usize },
    ModelResponse { role: String, response_tokens: usize, content: String },
    ModelFailure { role: String, error: String },
    ToolCall { tool_name: String, arguments: serde_json::Value },
    ToolResult { tool_name: String, success: bool, output: String },
    PolicyCheck { action: String, decision: String, reason: Option<String> },
    PolicyDecision { action: String, decision: String, reason: String, trust_tier: u8 },
    Checkpoint { snapshot_path: String },
    SystemEvent { message: String },
    ErrorEvent { message: String, details: Option<String> },
    // MCP
    McpRequest { agent_id: String, tool_name: String, arguments: serde_json::Value },
    McpResponse { tool_name: String, success: bool, output: String },
    // ACP
    AcpSessionCreated { session_id: String, agent_id: String },
    AcpSessionTerminated { session_id: String, agent_id: String },
    AcpCapabilityGranted { agent_id: String, capability: String, scope: String },
    AcpCapabilityRevoked { agent_id: String, capability: String },
    // Resource
    ResourceBudgetExceeded { resource: String, requested: u64, limit: u64 },
    ResourceBudgetChecked { resource: String, available: u64, limit: u64 },
    // Manifest
    ManifestCreated { manifest_id: String, version: String },
    ManifestValidated { manifest_id: String, valid: bool },
    ManifestSigned { manifest_id: String, signature: String },
    ManifestActivated { manifest_id: String },
    ManifestSuperseded { manifest_id: String, by: String },
    ManifestRetired { manifest_id: String },
    // Artifact
    ArtifactRecorded { artifact_id: String, task_id: String, kind: String },
    // Summary
    ProjectSummaryUpdated { project_id: String, summary: String },
}

/// Metadata attached to every event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMetadata {
    pub source: String,
    pub correlation_id: Option<String>,
}

/// A single event — the atomic unit of the event store
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub task_id: Option<TaskId>,
    pub sequence: SequenceNumber,
    pub kind: EventKind,
    pub payload: EventPayload,
    pub metadata: EventMetadata,
    pub timestamp: DateTime<Utc>,
    pub checksum: String,
}

impl Event {
    fn compute_checksum(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.id.0.to_string().as_bytes());
        if let Some(task_id) = self.task_id {
            hasher.update(task_id.0.to_string().as_bytes());
        }
        hasher.update(self.sequence.0.to_string().as_bytes());
        hasher.update(serde_json::to_string(&self.kind).unwrap_or_default().as_bytes());
        hasher.update(serde_json::to_string(&self.payload).unwrap_or_default().as_bytes());
        hasher.update(self.metadata.source.as_bytes());
        if let Some(correlation_id) = &self.metadata.correlation_id {
            hasher.update(correlation_id.as_bytes());
        }
        hasher.update(self.timestamp.to_rfc3339().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Creates a new event associated with a task
    pub fn new(task_id: TaskId, kind: EventKind, payload: EventPayload, source: String) -> Self {
        let mut event = Self {
            id: EventId::new(),
            task_id: Some(task_id),
            sequence: SequenceNumber(0),
            kind,
            payload,
            metadata: EventMetadata { source, correlation_id: None },
            timestamp: Utc::now(),
            checksum: String::new(),
        };
        event.checksum = event.compute_checksum();
        event
    }

    /// Creates a new system-level event without a task
    pub fn system(kind: EventKind, payload: EventPayload, source: String) -> Self {
        let mut event = Self {
            id: EventId::new(),
            task_id: None,
            sequence: SequenceNumber(0),
            kind,
            payload,
            metadata: EventMetadata { source, correlation_id: None },
            timestamp: Utc::now(),
            checksum: String::new(),
        };
        event.checksum = event.compute_checksum();
        event
    }

    /// Verify the event's checksum against its current content.
    pub fn verify_checksum(&self) -> bool {
        self.checksum == self.compute_checksum()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_event_id_display() {
        let id = EventId::new();
        let s = id.to_string();
        assert_eq!(s.len(), 36); // UUID string format length
    }

    #[test]
    fn test_sequence_ordering() {
        let s1 = SequenceNumber(1);
        let s2 = SequenceNumber(2);
        assert!(s1 < s2);
        assert_eq!(s1, SequenceNumber(1));
    }

    #[test]
    fn test_event_creation() {
        let task_id = TaskId::new();
        let event = Event::new(
            task_id,
            EventKind::TaskCreated,
            EventPayload::TaskCreated { request: json!({ "prompt": "test" }) },
            "test_source".to_string(),
        );

        assert_eq!(event.task_id, Some(task_id));
        assert_eq!(event.metadata.source, "test_source");
        assert_eq!(event.sequence, SequenceNumber(0));
    }

    #[test]
    fn test_system_event_creation() {
        let event = Event::system(
            EventKind::SystemStarted,
            EventPayload::SystemEvent { message: "started".to_string() },
            "sys".to_string(),
        );

        assert_eq!(event.task_id, None);
        assert_eq!(event.metadata.source, "sys");
    }

    #[test]
    fn test_serde_round_trip() {
        let payloads = vec![
            EventPayload::TaskCreated { request: json!({"k": "v"}) },
            EventPayload::StateChanged { from: "A".to_string(), to: "B".to_string() },
            EventPayload::ModelRequest {
                role: "user".to_string(),
                prompt_tokens: 10,
                context_budget: 100,
            },
            EventPayload::ModelResponse {
                role: "assistant".to_string(),
                response_tokens: 20,
                content: "ok".to_string(),
            },
            EventPayload::ModelFailure { role: "system".to_string(), error: "timeout".to_string() },
            EventPayload::ToolCall { tool_name: "ls".to_string(), arguments: json!({}) },
            EventPayload::ToolResult {
                tool_name: "ls".to_string(),
                success: true,
                output: ".".to_string(),
            },
            EventPayload::PolicyCheck {
                action: "read".to_string(),
                decision: "allow".to_string(),
                reason: None,
            },
            EventPayload::PolicyDecision {
                action: "mcp.fs.read".to_string(),
                decision: "allow".to_string(),
                reason: "matched rule".to_string(),
                trust_tier: 1,
            },
            EventPayload::Checkpoint { snapshot_path: "/tmp/a".to_string() },
            EventPayload::SystemEvent { message: "msg".to_string() },
            EventPayload::ErrorEvent {
                message: "err".to_string(),
                details: Some("dbg".to_string()),
            },
            EventPayload::McpRequest {
                agent_id: "agent-1".to_string(),
                tool_name: "fs.read".to_string(),
                arguments: json!({"path": "/tmp"}),
            },
            EventPayload::McpResponse {
                tool_name: "fs.read".to_string(),
                success: true,
                output: "content".to_string(),
            },
            EventPayload::AcpSessionCreated {
                session_id: "sess-1".to_string(),
                agent_id: "agent-1".to_string(),
            },
            EventPayload::AcpSessionTerminated {
                session_id: "sess-1".to_string(),
                agent_id: "agent-1".to_string(),
            },
            EventPayload::AcpCapabilityGranted {
                agent_id: "agent-1".to_string(),
                capability: "fs.read".to_string(),
                scope: "path:/tmp".to_string(),
            },
            EventPayload::AcpCapabilityRevoked {
                agent_id: "agent-1".to_string(),
                capability: "fs.read".to_string(),
            },
            EventPayload::ResourceBudgetExceeded {
                resource: "ram".to_string(),
                requested: 16000,
                limit: 16000,
            },
            EventPayload::ResourceBudgetChecked {
                resource: "ram".to_string(),
                available: 8000,
                limit: 16000,
            },
            EventPayload::ManifestCreated {
                manifest_id: "man-1".to_string(),
                version: "1.0.0".to_string(),
            },
            EventPayload::ManifestValidated { manifest_id: "man-1".to_string(), valid: true },
            EventPayload::ManifestSigned {
                manifest_id: "man-1".to_string(),
                signature: "sig-1".to_string(),
            },
            EventPayload::ManifestActivated { manifest_id: "man-1".to_string() },
            EventPayload::ManifestSuperseded {
                manifest_id: "man-1".to_string(),
                by: "man-2".to_string(),
            },
            EventPayload::ManifestRetired { manifest_id: "man-1".to_string() },
            EventPayload::ArtifactRecorded {
                artifact_id: "art-1".to_string(),
                task_id: "task-1".to_string(),
                kind: "tool_output".to_string(),
            },
            EventPayload::ProjectSummaryUpdated {
                project_id: "proj-1".to_string(),
                summary: "summary".to_string(),
            },
        ];

        for payload in payloads {
            let serialized = serde_json::to_string(&payload).unwrap();
            let deserialized: EventPayload = serde_json::from_str(&serialized).unwrap();
            assert_eq!(payload, deserialized);
        }
    }

    #[test]
    fn test_event_id_default() {
        let id = EventId::default();
        assert_eq!(id.to_string().len(), 36);
    }

    #[test]
    fn test_event_id_new_unique() {
        let id1 = EventId::new();
        let id2 = EventId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_event_id_ordering() {
        let id1 = EventId::new();
        // Sleep briefly to ensure different timestamps for UUIDv7
        std::thread::sleep(std::time::Duration::from_millis(10));
        let id2 = EventId::new();
        assert!(id1 < id2);
    }

    #[test]
    fn test_event_metadata_correlation_id() {
        let task_id = TaskId::new();
        let event = Event::new(
            task_id,
            EventKind::TaskCreated,
            EventPayload::TaskCreated { request: json!({}) },
            "src".to_string(),
        );
        // Default correlation_id is None
        assert!(event.metadata.correlation_id.is_none());
    }

    #[test]
    fn test_event_kind_variants() {
        // Ensure all EventKind variants are constructible and debuggable
        let kinds = vec![
            EventKind::TaskCreated,
            EventKind::TaskClassified,
            EventKind::TaskStateChanged,
            EventKind::ModelRequested,
            EventKind::ModelResponded,
            EventKind::ModelFailed,
            EventKind::ToolRequested,
            EventKind::ToolCompleted,
            EventKind::ToolFailed,
            EventKind::PolicyChecked,
            EventKind::PolicyDenied,
            EventKind::PolicyDecision,
            EventKind::ConfirmationRequested,
            EventKind::ConfirmationGranted,
            EventKind::ConfirmationDenied,
            EventKind::CheckpointCreated,
            EventKind::SnapshotCreated,
            EventKind::SystemStarted,
            EventKind::SystemShutdown,
            EventKind::Error,
            EventKind::McpRequest,
            EventKind::McpResponse,
            EventKind::AcpSessionCreated,
            EventKind::AcpSessionTerminated,
            EventKind::AcpCapabilityGranted,
            EventKind::AcpCapabilityRevoked,
            EventKind::ResourceBudgetExceeded,
            EventKind::ResourceBudgetChecked,
            EventKind::ManifestCreated,
            EventKind::ManifestValidated,
            EventKind::ManifestSigned,
            EventKind::ManifestActivated,
            EventKind::ManifestSuperseded,
            EventKind::ManifestRetired,
            EventKind::ArtifactRecorded,
            EventKind::ProjectSummaryUpdated,
        ];
        for kind in kinds {
            let debug = format!("{:?}", kind);
            assert!(!debug.is_empty());
        }
    }

    #[test]
    fn test_event_policy_check_with_reason() {
        let payload = EventPayload::PolicyCheck {
            action: "filesystem.write".to_string(),
            decision: "deny".to_string(),
            reason: Some("not allowed".to_string()),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let back: EventPayload = serde_json::from_str(&json).unwrap();
        match back {
            EventPayload::PolicyCheck { reason, .. } => {
                assert_eq!(reason, Some("not allowed".to_string()))
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_event_error_event_with_details() {
        let payload = EventPayload::ErrorEvent { message: "fatal".to_string(), details: None };
        let json = serde_json::to_string(&payload).unwrap();
        let back: EventPayload = serde_json::from_str(&json).unwrap();
        match back {
            EventPayload::ErrorEvent { message, details } => {
                assert_eq!(message, "fatal");
                assert!(details.is_none());
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_event_sequence_number_default() {
        let seq = SequenceNumber(0);
        assert_eq!(seq, SequenceNumber(0));
        assert!(seq < SequenceNumber(1));
    }

    #[test]
    fn test_event_kind_serde() {
        let kind = EventKind::SystemStarted;
        let json = serde_json::to_string(&kind).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, back);
    }

    #[test]
    fn test_event_new_with_all_fields() {
        let task_id = TaskId::new();
        let event = Event::new(
            task_id,
            EventKind::ModelRequested,
            EventPayload::ModelRequest {
                role: "planner".to_string(),
                prompt_tokens: 100,
                context_budget: 4096,
            },
            "kernel".to_string(),
        );
        assert_eq!(event.task_id, Some(task_id));
        assert_eq!(event.kind, EventKind::ModelRequested);
        assert_eq!(event.sequence, SequenceNumber(0));
        assert_eq!(event.metadata.source, "kernel");
    }

    #[test]
    fn test_event_system_new_with_all_fields() {
        let event = Event::system(
            EventKind::SystemShutdown,
            EventPayload::SystemEvent { message: "bye".to_string() },
            "kernel".to_string(),
        );
        assert!(event.task_id.is_none());
        assert_eq!(event.kind, EventKind::SystemShutdown);
        assert_eq!(event.metadata.source, "kernel");
    }
}
