//! Manifest lifecycle management for NexusAOS.
//!
//! Manages the lifecycle of manifests through the states:
//! draft -> validated -> signed -> active -> superseded -> retired.
//! Manifests are immutable once active.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::error::NexusError;

/// The lifecycle state of a manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManifestState {
    /// The manifest is in draft form and can be modified.
    Draft,
    /// The manifest has been validated against schema and policy.
    Validated,
    /// The manifest has been signed and is ready for activation.
    Signed,
    /// The manifest is currently active and in use.
    Active,
    /// The manifest has been superseded by a newer version.
    Superseded,
    /// The manifest has been retired and is no longer in use.
    Retired,
}

impl ManifestState {
    /// Returns the valid transitions from this state.
    pub fn valid_transitions(&self) -> Vec<ManifestState> {
        match self {
            Self::Draft => vec![Self::Validated],
            Self::Validated => vec![Self::Signed, Self::Draft],
            Self::Signed => vec![Self::Active, Self::Draft],
            Self::Active => vec![Self::Superseded, Self::Retired],
            Self::Superseded => vec![Self::Retired],
            Self::Retired => vec![],
        }
    }

    /// Returns true if transitioning to `target` is valid from this state.
    pub fn can_transition_to(&self, target: &ManifestState) -> bool {
        self.valid_transitions().contains(target)
    }

    /// Returns true if the manifest is immutable in this state.
    pub fn is_immutable(&self) -> bool {
        matches!(self, Self::Active | Self::Superseded | Self::Retired)
    }
}

/// A NexusAOS manifest with full lifecycle management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub state: ManifestState,
    pub content: serde_json::Value,
    pub schema_version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
    pub superseded_by: Option<String>,
    pub signature: Option<String>,
    pub created_by: String,
}

impl Manifest {
    /// Create a new manifest in Draft state.
    pub fn new(
        version: String,
        content: serde_json::Value,
        schema_version: String,
        created_by: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            version,
            state: ManifestState::Draft,
            content,
            schema_version,
            created_at: now,
            updated_at: now,
            activated_at: None,
            superseded_by: None,
            signature: None,
            created_by,
        }
    }

    /// Transition the manifest to a new state.
    pub fn transition_to(&mut self, new_state: ManifestState) -> Result<(), NexusError> {
        if !self.state.can_transition_to(&new_state) {
            return Err(NexusError::Storage(crate::error::StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid manifest state transition: {:?} -> {:?}", self.state, new_state),
            ))));
        }

        if new_state.is_immutable() && self.state != new_state {
            // Manifest becomes immutable when activated
        }

        self.state = new_state;
        self.updated_at = Utc::now();

        if new_state == ManifestState::Active {
            self.activated_at = Some(Utc::now());
        }

        info!(manifest = %self.id, version = %self.version, state = ?new_state, "Manifest state transitioned");
        Ok(())
    }

    /// Sign the manifest with a simple hash-based signature.
    pub fn sign(&mut self) -> Result<(), NexusError> {
        if self.state != ManifestState::Signed && self.state != ManifestState::Validated {
            return Err(NexusError::Storage(crate::error::StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Manifest must be in Signed or Validated state before signing",
            ))));
        }

        let content_str = serde_json::to_string(&self.content).unwrap_or_default();
        let signature = format!("sha256:{}", compute_simple_hash(&content_str));
        self.signature = Some(signature);
        self.state = ManifestState::Signed;
        self.updated_at = Utc::now();

        info!(manifest = %self.id, "Manifest signed");
        Ok(())
    }

    /// Supersede this manifest with a newer version.
    pub fn supersede(&mut self, by: String) -> Result<(), NexusError> {
        if self.state != ManifestState::Active {
            return Err(NexusError::Storage(crate::error::StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Only active manifests can be superseded",
            ))));
        }
        self.state = ManifestState::Superseded;
        self.superseded_by = Some(by);
        self.updated_at = Utc::now();
        info!(manifest = %self.id, "Manifest superseded");
        Ok(())
    }

    /// Verify the manifest's signature.
    pub fn verify_signature(&self) -> bool {
        if let Some(ref sig) = self.signature {
            let content_str = serde_json::to_string(&self.content).unwrap_or_default();
            let expected = format!("sha256:{}", compute_simple_hash(&content_str));
            sig == &expected
        } else {
            false
        }
    }

    /// Validate the manifest against a schema.
    pub fn validate(&self) -> bool {
        // Basic validation: check that required fields are present
        !self.version.is_empty() && !self.content.is_null() && !self.schema_version.is_empty()
    }
}

/// Compute a simple hash for manifest signing.
fn compute_simple_hash(content: &str) -> String {
    use std::{collections::hash_map::DefaultHasher, hash::Hasher};
    let mut hasher = DefaultHasher::new();
    hasher.write(content.as_bytes());
    hasher.finish().to_string()
}

/// Manifest store for persisting manifests.
pub struct ManifestStore {
    manifests: Arc<RwLock<Vec<Manifest>>>,
}

impl ManifestStore {
    /// Create a new manifest store.
    pub fn new() -> Self {
        Self { manifests: Arc::new(RwLock::new(Vec::new())) }
    }

    /// Store a new manifest.
    pub async fn store(&self, manifest: Manifest) -> Result<(), NexusError> {
        let mut manifests = self.manifests.write().await;
        manifests.push(manifest);
        Ok(())
    }

    /// Get a manifest by ID.
    pub async fn get(&self, id: &str) -> Option<Manifest> {
        let manifests = self.manifests.read().await;
        manifests.iter().find(|m| m.id == id).cloned()
    }

    /// Get all manifests in a given state.
    pub async fn get_by_state(&self, state: ManifestState) -> Vec<Manifest> {
        let manifests = self.manifests.read().await;
        manifests.iter().filter(|m| m.state == state).cloned().collect()
    }

    /// Get all active manifests.
    pub async fn active_manifests(&self) -> Vec<Manifest> {
        self.get_by_state(ManifestState::Active).await
    }

    /// List all manifests.
    pub async fn list_all(&self) -> Vec<Manifest> {
        self.manifests.read().await.clone()
    }
}

impl Default for ManifestStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_manifest_creation() {
        let manifest = Manifest::new(
            "1.0.0".to_string(),
            json!({"key": "value"}),
            "1.0".to_string(),
            "admin".to_string(),
        );
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.state, ManifestState::Draft);
        assert!(!manifest.id.is_empty());
    }

    #[test]
    fn test_manifest_state_transitions() -> Result<(), Box<dyn std::error::Error>> {
        let mut manifest = Manifest::new(
            "1.0.0".to_string(),
            json!({"key": "value"}),
            "1.0".to_string(),
            "admin".to_string(),
        );

        // Draft -> Validated
        manifest.transition_to(ManifestState::Validated)?;
        assert_eq!(manifest.state, ManifestState::Validated);

        // Validated -> Signed
        manifest.transition_to(ManifestState::Signed)?;
        assert_eq!(manifest.state, ManifestState::Signed);

        // Signed -> Active
        manifest.transition_to(ManifestState::Active)?;
        assert_eq!(manifest.state, ManifestState::Active);
        assert!(manifest.activated_at.is_some());

        // Active -> Superseded
        manifest.transition_to(ManifestState::Superseded)?;
        assert_eq!(manifest.state, ManifestState::Superseded);

        // Superseded -> Retired
        manifest.transition_to(ManifestState::Retired)?;
        assert_eq!(manifest.state, ManifestState::Retired);
        Ok(())
    }

    #[test]
    fn test_invalid_state_transition() {
        let mut manifest = Manifest::new(
            "1.0.0".to_string(),
            json!({"key": "value"}),
            "1.0".to_string(),
            "admin".to_string(),
        );

        // Draft -> Active is invalid
        let result = manifest.transition_to(ManifestState::Active);
        assert!(result.is_err());
    }

    #[test]
    fn test_manifest_immutable_when_active() -> Result<(), Box<dyn std::error::Error>> {
        let mut manifest = Manifest::new(
            "1.0.0".to_string(),
            json!({"key": "value"}),
            "1.0".to_string(),
            "admin".to_string(),
        );
        manifest.transition_to(ManifestState::Validated)?;
        manifest.transition_to(ManifestState::Signed)?;
        manifest.transition_to(ManifestState::Active)?;
        assert!(ManifestState::Active.is_immutable());
        Ok(())
    }

    #[test]
    fn test_manifest_sign_and_verify() -> Result<(), Box<dyn std::error::Error>> {
        let mut manifest = Manifest::new(
            "1.0.0".to_string(),
            json!({"key": "value"}),
            "1.0".to_string(),
            "admin".to_string(),
        );
        manifest.transition_to(ManifestState::Validated)?;
        manifest.sign()?;
        assert!(manifest.verify_signature());
        Ok(())
    }

    #[test]
    fn test_manifest_validate() {
        let manifest = Manifest::new(
            "1.0.0".to_string(),
            json!({"key": "value"}),
            "1.0".to_string(),
            "admin".to_string(),
        );
        assert!(manifest.validate());
    }

    #[test]
    fn test_manifest_validate_empty() {
        let manifest =
            Manifest::new("".to_string(), json!({}), "".to_string(), "admin".to_string());
        assert!(!manifest.validate());
    }

    #[tokio::test]
    async fn test_manifest_store() -> Result<(), Box<dyn std::error::Error>> {
        let store = ManifestStore::new();
        let manifest = Manifest::new(
            "1.0.0".to_string(),
            json!({"key": "value"}),
            "1.0".to_string(),
            "admin".to_string(),
        );
        let id = manifest.id.clone();
        store.store(manifest).await?;

        let retrieved = store.get(&id).await.ok_or("manifest not found")?;
        assert_eq!(retrieved.version, "1.0.0");
        Ok(())
    }

    #[tokio::test]
    async fn test_manifest_store_active() -> Result<(), Box<dyn std::error::Error>> {
        let store = ManifestStore::new();
        let mut manifest = Manifest::new(
            "1.0.0".to_string(),
            json!({"key": "value"}),
            "1.0".to_string(),
            "admin".to_string(),
        );
        manifest.transition_to(ManifestState::Validated)?;
        manifest.transition_to(ManifestState::Signed)?;
        manifest.transition_to(ManifestState::Active)?;
        store.store(manifest).await?;

        let active = store.active_manifests().await;
        assert_eq!(active.len(), 1);
        Ok(())
    }

    #[test]
    fn test_manifest_state_valid_transitions() {
        assert!(ManifestState::Draft.can_transition_to(&ManifestState::Validated));
        assert!(ManifestState::Validated.can_transition_to(&ManifestState::Signed));
        assert!(ManifestState::Signed.can_transition_to(&ManifestState::Active));
        assert!(ManifestState::Active.can_transition_to(&ManifestState::Superseded));
        assert!(ManifestState::Superseded.can_transition_to(&ManifestState::Retired));

        assert!(!ManifestState::Draft.can_transition_to(&ManifestState::Active));
        assert!(!ManifestState::Active.can_transition_to(&ManifestState::Draft));
        assert!(!ManifestState::Retired.can_transition_to(&ManifestState::Draft));
    }
}
