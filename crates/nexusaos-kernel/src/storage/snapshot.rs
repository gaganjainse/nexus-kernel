use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::error::StorageError;

/// A serializable snapshot of projection state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub snapshot_id: String,
    pub created_at: DateTime<Utc>,
    pub last_sequence: u64,
    pub data: serde_json::Value,
}

/// Manages snapshot persistence.
pub struct SnapshotStore {
    path: PathBuf,
}

impl SnapshotStore {
    /// Create a new SnapshotStore at the given path.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Save a snapshot to the filesystem using atomic writes.
    pub async fn save(&self, snapshot: &Snapshot) -> Result<(), StorageError> {
        if !self.path.exists() {
            fs::create_dir_all(&self.path).await?;
        }

        let filename = format!("snapshot_{}.json", snapshot.created_at.timestamp());
        let file_path = self.path.join(filename);
        let temp_path = file_path.with_extension("json.tmp");

        let json = serde_json::to_string_pretty(snapshot)?;
        fs::write(&temp_path, json).await?;
        fs::rename(&temp_path, &file_path).await?;

        Ok(())
    }

    /// Load the most recent snapshot by timestamp in the filename.
    pub async fn load_latest(&self) -> Result<Option<Snapshot>, StorageError> {
        if !self.path.exists() {
            return Ok(None);
        }

        let mut latest_path = None;
        let mut latest_ts = 0;

        let mut entries = fs::read_dir(&self.path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();
            if !name_str.starts_with("snapshot_") || !name_str.ends_with(".json") {
                continue;
            }
            let ts_part = name_str.trim_start_matches("snapshot_").trim_end_matches(".json");
            let Ok(ts) = ts_part.parse::<i64>() else {
                continue;
            };
            if ts >= latest_ts {
                latest_ts = ts;
                latest_path = Some(entry.path());
            }
        }

        if let Some(path) = latest_path {
            let content = fs::read_to_string(path).await?;
            let snapshot: Snapshot = serde_json::from_str(&content)?;
            Ok(Some(snapshot))
        } else {
            Ok(None)
        }
    }

    /// List all snapshot IDs.
    pub async fn list(&self) -> Result<Vec<String>, StorageError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let mut ids = Vec::new();
        let mut entries = fs::read_dir(&self.path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with("snapshot_") || !name.ends_with(".json") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path).await else {
                continue;
            };
            let Ok(snapshot) = serde_json::from_str::<Snapshot>(&content) else {
                continue;
            };
            ids.push(snapshot.snapshot_id);
        }

        Ok(ids)
    }

    /// Retain only the latest N snapshots, deleting older ones.
    pub async fn retain_latest(&self, max_count: usize) -> Result<(), StorageError> {
        if !self.path.exists() {
            return Ok(());
        }

        let mut snapshots: Vec<(i64, std::path::PathBuf)> = Vec::new();
        let mut entries = fs::read_dir(&self.path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with("snapshot_") || !name.ends_with(".json") {
                continue;
            }
            let ts_part = name.trim_start_matches("snapshot_").trim_end_matches(".json");
            if let Ok(ts) = ts_part.parse::<i64>() {
                snapshots.push((ts, path));
            }
        }

        snapshots.sort_unstable_by_key(|(ts, _)| *ts);
        let delete_count = snapshots.len().saturating_sub(max_count);
        for (_, path) in snapshots.into_iter().take(delete_count) {
            let _ = fs::remove_file(path).await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_snapshot_store() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let store = SnapshotStore::new(temp_dir.path().to_path_buf());

        let snapshot = Snapshot {
            snapshot_id: "snap-1".to_string(),
            created_at: Utc::now(),
            last_sequence: 10,
            data: serde_json::json!({"key": "value"}),
        };

        store.save(&snapshot).await?;

        let latest = store.load_latest().await?.ok_or("snapshot should exist")?;
        assert_eq!(latest.snapshot_id, "snap-1");
        assert_eq!(latest.last_sequence, 10);

        let ids = store.list().await?;
        assert_eq!(ids, vec!["snap-1".to_string()]);
        Ok(())
    }

    #[tokio::test]
    async fn test_snapshot_store_load_latest_empty() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let store = SnapshotStore::new(temp_dir.path().to_path_buf());
        let result = store.load_latest().await?;
        assert!(result.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_snapshot_store_list_empty() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let store = SnapshotStore::new(temp_dir.path().to_path_buf());
        let ids = store.list().await?;
        assert!(ids.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_snapshot_store_multiple_snapshots() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let store = SnapshotStore::new(temp_dir.path().to_path_buf());

        let snap1 = Snapshot {
            snapshot_id: "snap-1".to_string(),
            created_at: Utc::now(),
            last_sequence: 5,
            data: serde_json::json!({"v": 1}),
        };
        // Create snap2 with a slightly later timestamp
        let snap2 = Snapshot {
            snapshot_id: "snap-2".to_string(),
            created_at: Utc::now() + chrono::Duration::seconds(1),
            last_sequence: 10,
            data: serde_json::json!({"v": 2}),
        };

        store.save(&snap1).await?;
        store.save(&snap2).await?;

        let ids = store.list().await?;
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"snap-1".to_string()));
        assert!(ids.contains(&"snap-2".to_string()));
        Ok(())
    }

    #[tokio::test]
    async fn test_snapshot_store_creates_directory() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let new_path = temp_dir.path().join("nested").join("dir");
        let store = SnapshotStore::new(new_path.clone());

        let snapshot = Snapshot {
            snapshot_id: "snap-1".to_string(),
            created_at: Utc::now(),
            last_sequence: 1,
            data: serde_json::json!({}),
        };

        store.save(&snapshot).await?;
        assert!(new_path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_snapshot_store_ignores_non_snapshot_files(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let store = SnapshotStore::new(temp_dir.path().to_path_buf());

        // Create a non-snapshot file
        let other_file = temp_dir.path().join("other.txt");
        tokio::fs::write(&other_file, "not a snapshot").await?;

        let result = store.list().await?;
        assert!(result.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_snapshot_store_invalid_json_ignored_in_list(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let store = SnapshotStore::new(temp_dir.path().to_path_buf());

        // Create a file that looks like a snapshot but has invalid JSON
        let bad_snapshot = temp_dir.path().join("snapshot_9999999999.json");
        tokio::fs::write(&bad_snapshot, "not json {{{").await?;

        let ids = store.list().await?;
        assert!(ids.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_snapshot_store_roundtrip_data() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let store = SnapshotStore::new(temp_dir.path().to_path_buf());

        let original_data = serde_json::json!({
            "tasks": {
                "task-1": {"state": "Completed", "output": "done"}
            },
            "sequence": 42
        });

        let snapshot = Snapshot {
            snapshot_id: "roundtrip".to_string(),
            created_at: Utc::now(),
            last_sequence: 42,
            data: original_data.clone(),
        };

        store.save(&snapshot).await?;
        let loaded = store.load_latest().await?.ok_or("snapshot should exist")?;
        assert_eq!(loaded.data, original_data);
        assert_eq!(loaded.snapshot_id, "roundtrip");
        assert_eq!(loaded.last_sequence, 42);
        Ok(())
    }

    #[tokio::test]
    async fn test_snapshot_store_load_latest_picks_newest() -> Result<(), Box<dyn std::error::Error>>
    {
        let temp_dir = TempDir::new()?;
        let store = SnapshotStore::new(temp_dir.path().to_path_buf());

        let old_snap = Snapshot {
            snapshot_id: "old".to_string(),
            created_at: Utc::now() - chrono::Duration::days(1),
            last_sequence: 1,
            data: serde_json::json!({"old": true}),
        };
        let new_snap = Snapshot {
            snapshot_id: "new".to_string(),
            created_at: Utc::now(),
            last_sequence: 2,
            data: serde_json::json!({"new": true}),
        };

        store.save(&old_snap).await?;
        store.save(&new_snap).await?;

        let latest = store.load_latest().await?.ok_or("snapshot should exist")?;
        assert_eq!(latest.snapshot_id, "new");
        Ok(())
    }
}
