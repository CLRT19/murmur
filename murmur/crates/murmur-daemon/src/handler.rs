use murmur_protocol::*;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::cache::CompletionCache;
use crate::config::Config;

/// Handles incoming JSON-RPC requests.
pub struct RequestHandler {
    config: Arc<Config>,
    cache: Arc<Mutex<CompletionCache>>,
}

impl RequestHandler {
    pub fn new(config: Arc<Config>, cache: Arc<Mutex<CompletionCache>>) -> Self {
        Self { config, cache }
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
        let _context =
            murmur_context::collect_context(&params.cwd, shell, self.config.context.history_lines)
                .await;

        // TODO: Route to provider and get completions
        // For now, return empty completions
        let response = CompletionResponse {
            items: vec![],
            provider: "none".to_string(),
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
            "providers": self.config.providers.keys().collect::<Vec<_>>(),
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
