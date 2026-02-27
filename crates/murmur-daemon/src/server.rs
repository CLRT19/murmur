use anyhow::Result;
use murmur_protocol::{CompletionRequest, JsonRpcRequest, RequestId};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::cache::CompletionCache;
use crate::config::Config;
use crate::handler::RequestHandler;
use crate::prefetch;

/// The main daemon server.
pub struct Server {
    config: Arc<Config>,
    handler: Arc<RequestHandler>,
}

impl Server {
    pub fn new(config: Config) -> Self {
        let config = Arc::new(config);
        let cache = Arc::new(Mutex::new(CompletionCache::new(config.daemon.cache_size)));
        let handler = Arc::new(RequestHandler::new(config.clone(), cache));

        Self { config, handler }
    }

    /// Run the daemon server, listening on Unix socket.
    pub async fn run(&self) -> Result<()> {
        let socket_path = &self.config.daemon.socket_path;

        // Clean up stale socket file
        if std::path::Path::new(socket_path).exists() {
            std::fs::remove_file(socket_path)?;
        }

        let listener = UnixListener::bind(socket_path)?;
        info!(socket = %socket_path, "Murmur daemon listening");

        // Write PID file
        let pid = std::process::id();
        std::fs::write(Config::pid_path(), pid.to_string())?;
        info!(pid = pid, "PID file written");

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let handler = self.handler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, handler).await {
                            error!(error = %e, "Connection handler error");
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "Failed to accept connection");
                }
            }
        }
    }
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    handler: Arc<RequestHandler>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        let response = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
            Ok(request) => {
                let is_shutdown = request.method == murmur_protocol::methods::SHUTDOWN;
                let is_complete = request.method == murmur_protocol::methods::COMPLETE;

                // Extract params for pre-fetching before handling consumes them
                let prefetch_params = if is_complete {
                    request
                        .params
                        .as_ref()
                        .and_then(|p| serde_json::from_value::<CompletionRequest>(p.clone()).ok())
                } else {
                    None
                };

                let response = handler.handle(request).await;

                if is_shutdown {
                    let json = serde_json::to_string(&response)?;
                    writer.write_all(json.as_bytes()).await?;
                    writer.write_all(b"\n").await?;
                    writer.flush().await?;

                    // Clean up and exit
                    info!("Shutting down");
                    let _ = std::fs::remove_file(Config::pid_path());
                    std::process::exit(0);
                }

                // Spawn speculative pre-fetch for predicted next inputs
                if let Some(params) = prefetch_params {
                    let handler = handler.clone();
                    tokio::spawn(async move {
                        prefetch_completions(&handler, &params).await;
                    });
                }

                response
            }
            Err(e) => {
                warn!(error = %e, "Failed to parse request");
                murmur_protocol::JsonRpcResponse::error(
                    murmur_protocol::PARSE_ERROR,
                    format!("Parse error: {e}"),
                    murmur_protocol::RequestId::Number(0),
                )
            }
        };

        let json = serde_json::to_string(&response)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    Ok(())
}

/// Speculatively pre-fetch completions for predicted next inputs.
/// This runs in the background after a completion request is served.
async fn prefetch_completions(handler: &RequestHandler, original: &CompletionRequest) {
    let predictions = prefetch::predict_next_inputs(&original.input);
    if predictions.is_empty() {
        return;
    }

    debug!(
        count = predictions.len(),
        input = %original.input,
        "Pre-fetching predicted completions"
    );

    for predicted_input in predictions {
        // Build a synthetic request for the predicted input
        let request = JsonRpcRequest::new(
            murmur_protocol::methods::COMPLETE,
            Some(serde_json::to_value(&CompletionRequest {
                input: predicted_input.clone(),
                cursor_pos: predicted_input.len(),
                cwd: original.cwd.clone(),
                history: original.history.clone(),
                shell: original.shell.clone(),
            }).unwrap()),
            RequestId::Number(0), // internal request, ID doesn't matter
        );

        // This will populate the cache for the predicted input
        let _ = handler.handle(request).await;
        debug!(input = %predicted_input, "Pre-fetched completion");
    }
}

/// Initialize tracing subscriber.
pub fn init_tracing(log_level: &str) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
