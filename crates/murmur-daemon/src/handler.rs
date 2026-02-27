use murmur_protocol::*;
use murmur_providers::{
    AnthropicProvider, CodestralProvider, OllamaProvider, Provider, ProviderRouter, RouteDecision,
};
use murmur_voice::VoiceEngine;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::cache::CompletionCache;
use crate::config::Config;
use crate::history::CommandHistory;

/// Handles incoming JSON-RPC requests.
pub struct RequestHandler {
    config: Arc<Config>,
    cache: Arc<Mutex<CompletionCache>>,
    history: Arc<Mutex<CommandHistory>>,
    providers: Providers,
    voice: VoiceEngine,
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
    pub fn new(
        config: Arc<Config>,
        cache: Arc<Mutex<CompletionCache>>,
        history: Arc<Mutex<CommandHistory>>,
    ) -> Self {
        let providers = Providers::from_config(&config);

        // Initialize voice engine
        let voice_config = murmur_voice::VoiceConfig {
            enabled: config.voice.enabled,
            engine: config.voice.engine.clone(),
            language: config.voice.language.clone(),
            confidence_threshold: config.voice.confidence_threshold,
            capture_timeout_ms: config.voice.capture_timeout_ms,
            deepgram_api_key: config.voice.deepgram_api_key.clone(),
        };
        let mut voice = VoiceEngine::new(voice_config);

        // Set up voice restructurer using the Anthropic provider (if available)
        if let Some(anthropic_config) = config.providers.get("anthropic").filter(|c| c.enabled) {
            if let Some(ref api_key) = anthropic_config.api_key {
                let restructurer = murmur_voice::VoiceRestructurer::new(
                    api_key.clone(),
                    anthropic_config.model.clone(),
                    anthropic_config.endpoint.clone(),
                );
                voice.set_restructurer(restructurer);
                info!("Voice restructurer initialized with Anthropic provider");
            }
        }

        Self {
            config,
            cache,
            history,
            providers,
            voice,
        }
    }

    /// Get the configured socket path (for cleanup on shutdown).
    pub fn socket_path(&self) -> &str {
        &self.config.daemon.socket_path
    }

    /// Process a JSON-RPC request and return a response.
    pub async fn handle(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        debug!(method = %request.method, "Handling request");

        match request.method.as_str() {
            methods::COMPLETE => self.handle_complete(request).await,
            methods::STATUS => self.handle_status(request).await,
            methods::SHUTDOWN => self.handle_shutdown(request).await,
            methods::VOICE_START => self.handle_voice_start(request).await,
            methods::VOICE_PROCESS => self.handle_voice_process(request).await,
            methods::VOICE_STATUS => self.handle_voice_status(request).await,
            methods::CONTEXT_UPDATE => self.handle_context_update(request).await,
            methods::HISTORY_LIST => self.handle_history_list(request).await,
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
        let history_len = self.history.lock().await.len();
        let voice_status = self.voice.status();
        let status = serde_json::json!({
            "status": "running",
            "cache_entries": cache_len,
            "history_entries": history_len,
            "voice_enabled": self.config.voice.enabled,
            "voice_engines": voice_status.available_engines,
            "voice_active_engine": voice_status.active_engine,
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
        let params: VoiceStartRequest = match request.params {
            Some(params) => match serde_json::from_value(params) {
                Ok(p) => p,
                Err(e) => {
                    return JsonRpcResponse::error(
                        INVALID_PARAMS,
                        format!("Invalid voice params: {e}"),
                        request.id,
                    )
                }
            },
            None => {
                return JsonRpcResponse::error(INVALID_PARAMS, "Missing voice params", request.id)
            }
        };

        if !self.config.voice.enabled {
            return JsonRpcResponse::error(
                INTERNAL_ERROR,
                "Voice input is disabled. Set voice.enabled = true in config.",
                request.id,
            );
        }

        info!(mode = ?params.mode, cwd = %params.cwd, "Voice start requested");

        // For now, voice/start requires audio data in the params.
        // In a full implementation, the daemon would capture audio via cpal.
        // This endpoint is designed for clients that capture audio themselves
        // and send WAV data to the daemon for STT + restructuring.
        JsonRpcResponse::error(
            INTERNAL_ERROR,
            "Voice capture requires audio data. Use voice/process with audio_data field, \
             or use the CLI: murmur voice test",
            request.id,
        )
    }

    async fn handle_voice_process(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params: VoiceProcessRequest = match request.params {
            Some(params) => match serde_json::from_value(params) {
                Ok(p) => p,
                Err(e) => {
                    return JsonRpcResponse::error(
                        INVALID_PARAMS,
                        format!("Invalid voice/process params: {e}"),
                        request.id,
                    )
                }
            },
            None => {
                return JsonRpcResponse::error(
                    INVALID_PARAMS,
                    "Missing voice/process params",
                    request.id,
                )
            }
        };

        if !self.config.voice.enabled {
            return JsonRpcResponse::error(
                INTERNAL_ERROR,
                "Voice input is disabled. Set voice.enabled = true in config.",
                request.id,
            );
        }

        // Decode base64 audio data
        use base64::Engine;
        let audio_data = match base64::engine::general_purpose::STANDARD.decode(&params.audio_data)
        {
            Ok(data) => data,
            Err(e) => {
                return JsonRpcResponse::error(
                    INVALID_PARAMS,
                    format!("Invalid base64 audio_data: {e}"),
                    request.id,
                )
            }
        };

        info!(
            mode = ?params.mode,
            audio_bytes = audio_data.len(),
            "Processing voice audio"
        );

        match self
            .voice
            .process_audio(
                &audio_data,
                params.mode,
                &params.cwd,
                params.shell.as_deref(),
            )
            .await
        {
            Ok(result) => {
                info!(
                    engine = %result.engine,
                    latency_ms = result.latency_ms,
                    confidence = result.confidence,
                    "Voice processing complete"
                );
                JsonRpcResponse::success(serde_json::to_value(&result).unwrap(), request.id)
            }
            Err(e) => JsonRpcResponse::error(INTERNAL_ERROR, e.to_string(), request.id),
        }
    }

    async fn handle_voice_status(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let status = self.voice.status();
        JsonRpcResponse::success(serde_json::to_value(&status).unwrap(), request.id)
    }

    async fn handle_context_update(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params: ContextUpdateRequest = match request.params {
            Some(params) => match serde_json::from_value(params) {
                Ok(p) => p,
                Err(e) => {
                    return JsonRpcResponse::error(
                        INVALID_PARAMS,
                        format!("Invalid context/update params: {e}"),
                        request.id,
                    )
                }
            },
            None => {
                return JsonRpcResponse::error(
                    INVALID_PARAMS,
                    "Missing context/update params",
                    request.id,
                )
            }
        };

        info!(
            source = %params.source,
            command = %params.command,
            cwd = %params.cwd,
            exit_code = params.exit_code,
            "Recording cross-tool command"
        );

        {
            let mut history = self.history.lock().await;
            history.record(params.command, params.cwd, params.source, params.exit_code);
        }

        JsonRpcResponse::success(serde_json::json!({"recorded": true}), request.id)
    }

    async fn handle_history_list(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params: HistoryListRequest = match request.params {
            Some(params) => match serde_json::from_value(params) {
                Ok(p) => p,
                Err(e) => {
                    return JsonRpcResponse::error(
                        INVALID_PARAMS,
                        format!("Invalid history/list params: {e}"),
                        request.id,
                    )
                }
            },
            None => HistoryListRequest {
                cwd: None,
                limit: 50,
            },
        };

        let history = self.history.lock().await;
        let entries = history.list(params.cwd.as_deref(), params.limit);
        let entries: Vec<_> = entries.into_iter().cloned().collect();

        JsonRpcResponse::success(serde_json::to_value(&entries).unwrap(), request.id)
    }
}
