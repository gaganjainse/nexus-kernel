use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    error::ProviderError,
    model::types::{CompletionRequest, CompletionResponse},
    state::ModelRole,
};

/// Trait that all model providers must implement.
/// This is the contract the kernel uses to talk to models.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Human-readable provider name.
    fn name(&self) -> &str;

    /// What role this provider serves.
    fn role(&self) -> ModelRole;

    /// Maximum context length in tokens.
    fn max_context(&self) -> usize;

    /// Whether this provider supports vision/image input.
    fn supports_vision(&self) -> bool;

    /// Check if the provider's backend is healthy and reachable.
    async fn health_check(&self) -> Result<bool, ProviderError>;

    /// Generate a completion (non-streaming).
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError>;

    /// Generate a completion with streaming token callback.
    async fn complete_stream(
        &self,
        request: CompletionRequest,
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send + Sync),
    ) -> Result<CompletionResponse, ProviderError> {
        let resp = self.complete(request).await?;
        on_token(&resp.content);
        Ok(resp)
    }

    /// Stream completion tokens as they arrive.
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<String>, ProviderError> {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let mut on_token = |s: &str| {
            let _ = tx.try_send(s.to_string());
        };
        self.complete_stream(request, &mut on_token).await?;
        Ok(rx)
    }

    /// Provider capabilities descriptor.
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_streaming: true,
            supports_vision: self.supports_vision(),
            max_context: self.max_context(),
        }
    }

    /// Warm up the provider (load model into memory, etc.).
    async fn warmup(&self) -> Result<(), ProviderError> {
        Ok(())
    }

    /// Unload the provider (free memory, close connections).
    async fn unload(&self) -> Result<(), ProviderError> {
        Ok(())
    }

    /// Cancel any in-flight request.
    async fn cancel(&self) -> Result<(), ProviderError>;
}

/// Describes what a provider is capable of.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub supports_streaming: bool,
    pub supports_vision: bool,
    pub max_context: usize,
}
