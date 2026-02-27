use murmur_protocol::*;
use murmur_providers::{
    AnthropicProvider, CodestralProvider, OllamaProvider, Provider, ProviderRouter, RouteDecision,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::cache::CompletionCache;
use crate::config::Config;

/// Handles incoming JSON-RPC requests.
pub struct RequestHandler {
    config: Arc<Config>,
    cache: Arc<Mutex<CompletionCache>>,
    providers: Providers,
}

/// Holds initialized provider instances.
struct Providers {
    anthropic: Option<AnthropicProvider>,
    codestral: Option<CodestralProvider>,
    ollama: Option<OllamaProvider>,
}

impl Providers {
    fn from_config(config: &Config) -> Self {
        let anthropic = config
            .providers
            .get("anthropic")
            .filter(|c| c.enabled)
            .and_then(|c| match AnthropicProvider::new(c) {
                Ok(p) => {
                    info!("Anthropic provider initialized");
                    Some(p)
                }
                Err(e) => {
                    warn!(error = %e, "Failed to initialize Anthropic provider");
                    None
                }
            });

        let codestral = config
            .providers
            .get("codestral")
            .filter(|c| c.enabled)
            .and_then(|c| match CodestralProvider::new(c) {
                Ok(p) => {
                    info!("Codestral provider initialized");
                    Some(p)
                }
                Err(e) => {
                    warn!(error = %e, "Failed to initialize Codestral provider");
                    None
                }
            });

        let ollama = config
            .providers
            .get("ollama")
            .filter(|c| c.enabled)
            .and_then(|c| match OllamaProvider::new(c) {
                Ok(p) => {
                    info!("Ollama provider initialized");
                    Some(p)
                }
                Err(e) => {
                    warn!(error = %e, "Failed to initialize Ollama provider");
                    None
                }
            });

        Self {
            anthropic,
            codestral,
            ollama,
        }
    }

    /// Get an ordered list of providers to try for the given route decision.
    /// Returns primary first, then fallbacks. Enables automatic failover.
    fn get_chain(&self, decision: &RouteDecision) -> Vec<&dyn Provider> {
        let mut chain: Vec<&dyn Provider> = Vec::new();
        match decision {
            RouteDecision::Shell => {
                if let Some(ref p) = self.anthropic {
                    chain.push(p);
                }
                if let Some(ref p) = self.ollama {
                    chain.push(p);
                }
            }
            RouteDecision::Code => {
                if let Some(ref p) = self.codestral {
                    chain.push(p);
                }
                if let Some(ref p) = self.anthropic {
                    chain.push(p);
                }
                if let Some(ref p) = self.ollama {
                    chain.push(p);
                }
            }
            RouteDecision::Local => {
                if let Some(ref p) = self.ollama {
                    chain.push(p);
                }
                if let Some(ref p) = self.anthropic {
                    chain.push(p);
                }
            }
        }
        chain
    }

    fn names(&self) -> Vec<&str> {
        let mut names = vec![];
        if self.anthropic.is_some() {
            names.push("anthropic");
        }
        if self.codestral.is_some() {
            names.push("codestral");
        }
        if self.ollama.is_some() {
            names.push("ollama");
        }
        names
    }
}

impl RequestHandler {
    pub fn new(config: Arc<Config>, cache: Arc<Mutex<CompletionCache>>) -> Self {
        let providers = Providers::from_config(&config);
        Self {
            config,
            cache,
            providers,
        }
    }

    /// Process a JSON-RPC request and return a response.
    pub async fn handle(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        debug!(method = %request.method, "Handling request");

        match request.method.as_str() {
            methods::COMPLETE => self.handle_complete(request).await,
            methods::STATUS => self.handle_status(request).await,
            methods::SHUTDOWN => self.handle_shutdown(request).await,
            methods::VOICE_START => self.handle_voice_start(request).await,
            _ => JsonRpcResponse::error(
                METHOD_NOT_FOUND,
                format!("Unknown method: {}", request.method),
                request.id,
            ),
        }
    }

    async fn handle_complete(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params: CompletionRequest = match request.params {
            Some(params) => match serde_json::from_value(params) {
                Ok(p) => p,
                Err(e) => {
                    return JsonRpcResponse::error(
                        INVALID_PARAMS,
                        format!("Invalid params: {e}"),
                        request.id,
                    )
                }
            },
            None => return JsonRpcResponse::error(INVALID_PARAMS, "Missing params", request.id),
        };

        let start = std::time::Instant::now();

        // Check cache first
        let cache_key = CompletionCache::cache_key(
            &params.input,
            &params.cwd,
            params.shell.as_deref().unwrap_or("unknown"),
        );

        {
            let mut cache = self.cache.lock().await;
            if let Some(mut cached) = cache.get(cache_key) {
                cached.cached = true;
                cached.latency_ms = start.elapsed().as_millis() as u64;
                info!(input = %params.input, latency_ms = cached.latency_ms, "Cache hit");
                return JsonRpcResponse::success(
                    serde_json::to_value(&cached).unwrap(),
                    request.id,
                );
            }
        }

        // Collect context
        let shell = params.shell.as_deref().unwrap_or("zsh");
        let context =
            murmur_context::collect_context(&params.cwd, shell, self.config.context.history_lines)
                .await;

        // Route to provider chain and try with failover
        let decision = ProviderRouter::route(&params, &context);
        let chain = self.providers.get_chain(&decision);
        debug!(route = ?decision, chain_len = chain.len(), input = %params.input, "Provider routing decision");

        let (items, provider_name) = if chain.is_empty() {
            debug!("No providers configured, returning empty completions");
            (vec![], "none".to_string())
        } else {
            let mut result_items = vec![];
            let mut result_provider = "none".to_string();

            for (i, provider) in chain.iter().enumerate() {
                let is_fallback = i > 0;
                if is_fallback {
                    debug!(provider = provider.name(), "Trying fallback provider");
                }

                match provider.complete(&params, &context).await {
                    Ok(items) => {
                        info!(
                            provider = provider.name(),
                            count = items.len(),
                            latency_ms = start.elapsed().as_millis() as u64,
                            fallback = is_fallback,
                            "Completions received"
                        );
                        result_items = items;
                        result_provider = provider.name().to_string();
                        break;
                    }
                    Err(e) => {
                        warn!(
                            provider = provider.name(),
                            error = %e,
                            remaining = chain.len() - i - 1,
                            "Provider failed, trying next"
                        );
                    }
                }
            }

            (result_items, result_provider)
        };

        let response = CompletionResponse {
            items,
            provider: provider_name,
            latency_ms: start.elapsed().as_millis() as u64,
            cached: false,
        };

        // Cache the response
        {
            let mut cache = self.cache.lock().await;
            cache.put(cache_key, response.clone());
        }

        JsonRpcResponse::success(serde_json::to_value(&response).unwrap(), request.id)
    }

    async fn handle_status(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let cache_len = self.cache.lock().await.len();
        let status = serde_json::json!({
            "status": "running",
            "cache_entries": cache_len,
            "voice_enabled": self.config.voice.enabled,
            "providers_configured": self.config.providers.keys().collect::<Vec<_>>(),
            "providers_active": self.providers.names(),
        });
        JsonRpcResponse::success(status, request.id)
    }

    async fn handle_shutdown(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        info!("Shutdown requested");
        JsonRpcResponse::success(Value::String("shutting down".to_string()), request.id)
    }

    async fn handle_voice_start(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        JsonRpcResponse::error(
            INTERNAL_ERROR,
            "Voice input not yet implemented (Phase 3)",
            request.id,
        )
    }
}
