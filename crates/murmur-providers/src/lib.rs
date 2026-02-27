//! Murmur Providers — LLM provider abstraction with failover support.

mod anthropic;
mod codestral;
mod ollama;
mod router;

pub use anthropic::AnthropicProvider;
pub use codestral::CodestralProvider;
pub use ollama::OllamaProvider;
pub use router::{ProviderRouter, RouteDecision};

use async_trait::async_trait;
use murmur_context::ShellContext;
use murmur_protocol::{CompletionItem, CompletionRequest};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error: {status} — {message}")]
    Api { status: u16, message: String },

    #[error("Provider not configured: {0}")]
    NotConfigured(String),

    #[error("All providers failed")]
    AllFailed,

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Timeout")]
    Timeout,
}

/// Configuration for a provider.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    5000
}

/// Trait that all LLM providers must implement.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Provider name (e.g., "anthropic", "ollama").
    fn name(&self) -> &str;

    /// Generate completion suggestions.
    async fn complete(
        &self,
        request: &CompletionRequest,
        context: &ShellContext,
    ) -> Result<Vec<CompletionItem>, ProviderError>;

    /// Check if the provider is healthy/reachable.
    async fn health_check(&self) -> Result<(), ProviderError>;
}
