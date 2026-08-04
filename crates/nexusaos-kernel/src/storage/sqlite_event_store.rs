use std::path::PathBuf;

use async_trait::async_trait;

use crate::{
    error::{NexusError, StorageError},
    events::Event,
    storage::EventStore,
};

/// SQLite-backed event store.
pub struct SqliteEventStore {
    db_path: PathBuf,
}

impl SqliteEventStore {
    /// Open or create a SQLite event store at the given path.
    pub async fn open(path: PathBuf) -> Result<Self, NexusError> {
        let db_path = path.join("events.db");
        let db_path_clone = db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path_clone)
                .map_err(|e| NexusError::Storage(StorageError::Database(e)))?;
            conn.execute(
                "CREATE TABLE IF NOT EXISTS events (
                    id TEXT PRIMARY KEY,
                    task_id TEXT,
                    sequence INTEGER,
                    idempotency_key TEXT UNIQUE,
                    data TEXT NOT NULL
                )",
                (),
            )
            .map_err(|e| NexusError::Storage(StorageError::Database(e)))?;
            // Migrate existing default-sequence rows to 0
            conn.execute(
                "UPDATE events SET sequence = 0 WHERE sequence IS NULL OR sequence = 0",
                (),
            )
            .ok();
            Ok::<_, NexusError>(())
        })
        .await
        .map_err(|e| {
            NexusError::Storage(StorageError::Io(std::io::Error::other(e.to_string())))
        })??;
        Ok(Self { db_path })
    }

    /// Read all events in sequence order.
    pub async fn read_all(&self) -> Result<Vec<Event>, StorageError> {
        self.spawn_query(move |conn| {
            let mut stmt = conn
                .prepare("SELECT data FROM events ORDER BY sequence ASC")
                .map_err(StorageError::Database)?;
            let rows = stmt
                .query_map([], |row| {
                    let data: String = row.get(0)?;
                    serde_json::from_str(&data).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })
                })
                .map_err(StorageError::Database)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(StorageError::Database)
        })
        .await
    }

    /// Read events for a specific task.
    pub async fn read_for_task(
        &self,
        task_id: &crate::task::TaskId,
    ) -> Result<Vec<Event>, StorageError> {
        let task_id_str = task_id.0.to_string();
        self.spawn_query(move |conn| {
            let mut stmt = conn
                .prepare("SELECT data FROM events WHERE task_id = ?1 ORDER BY sequence ASC")
                .map_err(StorageError::Database)?;
            let rows = stmt
                .query_map([&task_id_str], |row| {
                    let data: String = row.get(0)?;
                    serde_json::from_str(&data).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })
                })
                .map_err(StorageError::Database)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(StorageError::Database)
        })
        .await
    }

    /// Read events since a given sequence number.
    pub async fn read_since(&self, sequence: u64) -> Result<Vec<Event>, StorageError> {
        self.spawn_query(move |conn| {
            let mut stmt = conn
                .prepare("SELECT data FROM events WHERE sequence >= ?1 ORDER BY sequence ASC")
                .map_err(StorageError::Database)?;
            let rows = stmt
                .query_map([&sequence], |row| {
                    let data: String = row.get(0)?;
                    serde_json::from_str(&data).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })
                })
                .map_err(StorageError::Database)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(StorageError::Database)
        })
        .await
    }

    /// Get total event count.
    pub async fn count(&self) -> Result<u64, StorageError> {
        self.spawn_query(move |conn| {
            let mut stmt =
                conn.prepare("SELECT COUNT(*) FROM events").map_err(StorageError::Database)?;
            let count: i64 =
                stmt.query_row([], |row| row.get(0)).map_err(StorageError::Database)?;
            Ok(count as u64)
        })
        .await
    }

    /// Helper: run a SQLite query on a new connection in a blocking task.
    async fn spawn_query<F, R>(&self, query: F) -> Result<R, StorageError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<R, StorageError> + Send + 'static,
        R: Send + 'static,
    {
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).map_err(StorageError::Database)?;
            query(&conn)
        })
        .await
        .map_err(|e| StorageError::Io(std::io::Error::other(e.to_string())))?
    }
}

#[async_trait]
impl EventStore for SqliteEventStore {
    async fn append(&self, event: Event) -> Result<(), NexusError> {
        let data = serde_json::to_string(&event).map_err(NexusError::Serde)?;
        let idempotency_key = event.id.0.to_string();
        let result = self
            .spawn_query(move |conn| {
                conn.execute(
                    "INSERT INTO events (id, task_id, sequence, idempotency_key, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                    (
                        event.id.0.to_string(),
                        event.task_id.map(|id| id.0.to_string()),
                        event.sequence.0,
                        idempotency_key,
                        data,
                    ),
                )
                .map_err(StorageError::Database)?;
                Ok(())
            })
            .await;
        result.map_err(NexusError::Storage)?;
        Ok(())
    }

    async fn get_all_events(&self) -> Result<Vec<Event>, NexusError> {
        Self::read_all(self).await.map_err(NexusError::Storage)
    }

    async fn get_task_events(
        &self,
        task_id: &crate::task::TaskId,
    ) -> Result<Vec<Event>, NexusError> {
        Self::read_for_task(self, task_id).await.map_err(NexusError::Storage)
    }

    async fn read_since(&self, sequence: u64) -> Result<Vec<Event>, NexusError> {
        Self::read_since(self, sequence).await.map_err(NexusError::Storage)
    }

    async fn current_sequence(&self) -> Result<u64, NexusError> {
        let seq = self
            .spawn_query(|conn| {
                let mut stmt = conn.prepare("SELECT MAX(sequence) FROM events")?;
                let result = stmt.query_row([], |row| row.get::<_, i64>(0));
                match result {
                    Ok(max_seq) => Ok(max_seq.max(0) as u64),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
                    Err(e) => Err(StorageError::Database(e)),
                }
            })
            .await;
        seq.map_err(NexusError::Storage)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::{
        events::{EventKind, EventPayload, SequenceNumber},
        task::TaskId,
    };

    #[tokio::test]
    async fn test_open() {
        let temp_dir = TempDir::new().unwrap();
        let store = SqliteEventStore::open(temp_dir.path().to_path_buf()).await.unwrap();
        let events = store.read_all().await.unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn test_append() {
        let temp_dir = TempDir::new().unwrap();
        let store = SqliteEventStore::open(temp_dir.path().to_path_buf()).await.unwrap();
        let task_id = TaskId::new();
        let mut event = Event::new(
            task_id,
            EventKind::TaskCreated,
            EventPayload::TaskCreated { request: serde_json::json!({}) },
            "test".to_string(),
        );
        event.sequence = SequenceNumber(1);
        store.append(event.clone()).await.unwrap();
        let events = store.read_all().await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, event.id);
    }

    #[tokio::test]
    async fn test_read_for_task() {
        let temp_dir = TempDir::new().unwrap();
        let store = SqliteEventStore::open(temp_dir.path().to_path_buf()).await.unwrap();

        let task_id = TaskId::new();
        let other_task_id = TaskId::new();

        let mut event1 = Event::new(
            task_id,
            EventKind::TaskCreated,
            EventPayload::TaskCreated { request: serde_json::json!({}) },
            "test".to_string(),
        );
        event1.sequence = SequenceNumber(1);
        store.append(event1).await.unwrap();

        let mut event2 = Event::new(
            other_task_id,
            EventKind::TaskCreated,
            EventPayload::TaskCreated { request: serde_json::json!({}) },
            "test".to_string(),
        );
        event2.sequence = SequenceNumber(2);
        store.append(event2).await.unwrap();

        let task_events = store.read_for_task(&task_id).await.unwrap();
        assert_eq!(task_events.len(), 1);
        assert_eq!(task_events[0].task_id, Some(task_id));

        let other_events = store.read_for_task(&other_task_id).await.unwrap();
        assert_eq!(other_events.len(), 1);
        assert_eq!(other_events[0].task_id, Some(other_task_id));
    }

    #[tokio::test]
    async fn test_read_since() {
        let temp_dir = TempDir::new().unwrap();
        let store = SqliteEventStore::open(temp_dir.path().to_path_buf()).await.unwrap();

        let mut e1 = Event::new(
            TaskId::new(),
            EventKind::TaskCreated,
            EventPayload::TaskCreated { request: serde_json::json!({}) },
            "test".to_string(),
        );
        e1.sequence = SequenceNumber(1);
        store.append(e1).await.unwrap();

        let mut e2 = Event::new(
            TaskId::new(),
            EventKind::TaskCreated,
            EventPayload::TaskCreated { request: serde_json::json!({}) },
            "test".to_string(),
        );
        e2.sequence = SequenceNumber(2);
        store.append(e2).await.unwrap();

        let mut e3 = Event::new(
            TaskId::new(),
            EventKind::TaskCreated,
            EventPayload::TaskCreated { request: serde_json::json!({}) },
            "test".to_string(),
        );
        e3.sequence = SequenceNumber(3);
        store.append(e3).await.unwrap();

        let since_2 = store.read_since(2).await.unwrap();
        assert_eq!(since_2.len(), 2);

        let since_3 = store.read_since(3).await.unwrap();
        assert_eq!(since_3.len(), 1);

        let since_4 = store.read_since(4).await.unwrap();
        assert!(since_4.is_empty());
    }

    #[tokio::test]
    async fn test_count() {
        let temp_dir = TempDir::new().unwrap();
        let store = SqliteEventStore::open(temp_dir.path().to_path_buf()).await.unwrap();

        assert_eq!(store.count().await.unwrap(), 0);

        let task_id = TaskId::new();
        let mut event = Event::new(
            task_id,
            EventKind::TaskCreated,
            EventPayload::TaskCreated { request: serde_json::json!({}) },
            "test".to_string(),
        );
        event.sequence = SequenceNumber(1);
        store.append(event).await.unwrap();

        assert_eq!(store.count().await.unwrap(), 1);

        let mut event2 = Event::new(
            TaskId::new(),
            EventKind::TaskCreated,
            EventPayload::TaskCreated { request: serde_json::json!({}) },
            "test".to_string(),
        );
        event2.sequence = SequenceNumber(2);
        store.append(event2).await.unwrap();

        assert_eq!(store.count().await.unwrap(), 2);
    }
}
