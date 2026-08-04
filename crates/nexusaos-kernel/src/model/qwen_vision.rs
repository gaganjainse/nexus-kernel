//! Qwen3.5 9B Vision Provider for NexusAOS Kernel.
//!
//! This provider implements the ModelProvider trait for the Qwen3.5 9B
//! vision-capable model. It produces structured observations rather than
//! direct actions, as required by §6.8 of the architecture brief.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{
    error::ProviderError,
    model::{
        provider::ModelProvider,
        types::{ChatMessage, CompletionRequest, CompletionResponse},
    },
    state::ModelRole,
};

/// Qwen3.5 9B vision provider.
pub struct QwenVisionProvider {
    name: String,
    role: ModelRole,
    base_url: String,
    model_id: String,
    api_key: String,
    max_context: usize,
    client: Client,
}

impl QwenVisionProvider {
    /// Create a new Qwen3.5 9B vision provider.
    pub fn new(
        model_id: String,
        base_url: String,
        api_key: String,
        max_context: usize,
    ) -> Result<Self, ProviderError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        Ok(Self {
            name: format!("qwen-vision-{}", model_id),
            role: ModelRole::Vision,
            base_url: base_url.trim_end_matches('/').to_string(),
            model_id,
            api_key,
            max_context,
            client,
        })
    }
}

#[async_trait]
impl ModelProvider for QwenVisionProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn role(&self) -> ModelRole {
        self.role
    }

    fn max_context(&self) -> usize {
        self.max_context
    }

    fn supports_vision(&self) -> bool {
        true
    }

    async fn health_check(&self) -> Result<bool, ProviderError> {
        let health_url = format!("{}/health", self.base_url);
        let response = self.client.get(&health_url).bearer_auth(&self.api_key).send().await;

        match response {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let qwen_req = QwenChatRequest {
            model: &self.model_id,
            messages: &request.messages,
            max_tokens: request.max_tokens as i64,
            temperature: request.temperature,
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&qwen_req)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::Api(format!("Qwen API error: {}", response.status())));
        }

        let body: QwenChatResponse =
            response.json().await.map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        let content = body
            .choices
            .first()
            .and_then(|c| c.message.content.as_deref())
            .unwrap_or("")
            .to_string();

        Ok(CompletionResponse {
            content,
            finish_reason: body.choices.first().and_then(|c| c.finish_reason.clone()),
            prompt_tokens: body.usage.as_ref().map(|u| u.prompt_tokens),
            completion_tokens: body.usage.as_ref().map(|u| u.completion_tokens),
            model: self.model_id.clone(),
        })
    }

    async fn cancel(&self) -> Result<(), ProviderError> {
        Ok(())
    }
}

/// Qwen chat request body.
#[derive(Serialize)]
struct QwenChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    max_tokens: i64,
    temperature: f32,
    stream: bool,
}

/// Qwen chat response body.
#[derive(Deserialize)]
struct QwenChatResponse {
    choices: Vec<QwenChoice>,
    usage: Option<QwenUsage>,
}

/// A single choice in a Qwen response.
#[derive(Deserialize)]
struct QwenChoice {
    message: QwenMessage,
    finish_reason: Option<String>,
}

/// A message in a Qwen response.
#[derive(Deserialize)]
struct QwenMessage {
    content: Option<String>,
}

/// Token usage statistics from Qwen.
#[derive(Deserialize)]
struct QwenUsage {
    prompt_tokens: usize,
    completion_tokens: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::ChatRole;

    #[test]
    fn test_provider_creation() {
        let provider = QwenVisionProvider::new(
            "qwen3.5-9b".to_string(),
            "http://localhost:8000".to_string(),
            "test-key".to_string(),
            32768,
        );
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert_eq!(p.role(), ModelRole::Vision);
        assert!(p.supports_vision());
        assert_eq!(p.max_context(), 32768);
    }

    #[test]
    fn test_provider_name() {
        let provider = QwenVisionProvider::new(
            "qwen3.5-9b".to_string(),
            "http://localhost:8000".to_string(),
            "test-key".to_string(),
            32768,
        )
        .unwrap();
        assert!(provider.name().starts_with("qwen-vision-"));
    }

    #[test]
    fn test_chat_message_with_images() {
        let msg = ChatMessage {
            role: ChatRole::User,
            content: "Describe this image".to_string(),
            images: Some(vec!["base64data".to_string()]),
        };
        assert_eq!(msg.role, ChatRole::User);
        assert!(msg.images.is_some());
    }
}
