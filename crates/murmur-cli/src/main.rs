use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use murmur_daemon::config::Config;
use murmur_daemon::server::{self, Server};
use murmur_protocol::{methods, JsonRpcRequest, JsonRpcResponse, RequestId, VoiceMode};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Parser)]
#[command(
    name = "murmur",
    about = "AI-powered terminal autocomplete with voice input"
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Murmur daemon
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,
        /// Path to config file
        #[arg(long)]
        config: Option<String>,
    },
    /// Stop the running daemon
    Stop,
    /// Show daemon status
    Status,
    /// Print shell integration script
    Setup {
        /// Shell to generate setup for (zsh, bash, fish)
        shell: String,
    },
    /// Run diagnostic checks
    Doctor,
    /// Voice input commands
    Voice {
        #[command(subcommand)]
        action: VoiceAction,
    },
}

#[derive(Subcommand)]
enum VoiceAction {
    /// Test voice input (process a WAV file or generate test audio)
    Test {
        /// Path to a WAV file (16kHz mono 16-bit PCM). If omitted, uses a built-in test.
        #[arg(long)]
        file: Option<String>,
        /// Voice mode: "command" (speech → shell command) or "natural" (speech → clean prose)
        #[arg(long, default_value = "command")]
        mode: String,
    },
    /// Show voice engine status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { foreground, config } => cmd_start(foreground, config).await,
        Commands::Stop => cmd_stop().await,
        Commands::Status => cmd_status().await,
        Commands::Setup { shell } => cmd_setup(&shell),
        Commands::Doctor => cmd_doctor().await,
        Commands::Voice { action } => match action {
            VoiceAction::Test { file, mode } => cmd_voice_test(file, mode).await,
            VoiceAction::Status => cmd_voice_status().await,
        },
    }
}

async fn cmd_start(foreground: bool, config_path: Option<String>) -> Result<()> {
    // Check if already running
    if is_daemon_running() {
        println!("Murmur daemon is already running.");
        return Ok(());
    }

    let config = match &config_path {
        Some(path) => Config::load_from(Path::new(path))?,
        None => Config::load()?,
    };

    if foreground {
        server::init_tracing(&config.daemon.log_level);
        println!("Starting Murmur daemon (foreground)...");
        let server = Server::new(config);
        server.run().await?;
    } else {
        // Spawn as background process
        let exe = std::env::current_exe()?;
        let mut args = vec!["start".to_string(), "--foreground".to_string()];
        if let Some(path) = config_path {
            args.push("--config".to_string());
            args.push(path);
        }

        let child = std::process::Command::new(exe)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("Failed to spawn daemon process")?;

        println!("Murmur daemon started (PID: {})", child.id());
    }

    Ok(())
}

async fn cmd_stop() -> Result<()> {
    if !is_daemon_running() {
        println!("Murmur daemon is not running.");
        return Ok(());
    }

    // Send shutdown via socket
    let config = Config::load().unwrap_or_default();
    match send_request(&config.daemon.socket_path, methods::SHUTDOWN, None).await {
        Ok(_) => println!("Murmur daemon stopped."),
        Err(_) => {
            // Fallback: kill via PID
            if let Ok(pid_str) = std::fs::read_to_string(Config::pid_path()) {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    unsafe {
                        libc::kill(pid, libc::SIGTERM);
                    }
                    let _ = std::fs::remove_file(Config::pid_path());
                    println!("Murmur daemon stopped (via signal).");
                }
            }
        }
    }

    Ok(())
}

async fn cmd_status() -> Result<()> {
    if !is_daemon_running() {
        println!("Murmur daemon is not running.");
        return Ok(());
    }

    let config = Config::load().unwrap_or_default();
    match send_request(&config.daemon.socket_path, methods::STATUS, None).await {
        Ok(response) => {
            if let Some(result) = response.result {
                println!("Murmur daemon status:");
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        }
        Err(e) => {
            println!("Failed to get status: {e}");
        }
    }

    Ok(())
}

fn cmd_setup(shell: &str) -> Result<()> {
    match shell {
        "zsh" => {
            // Output the zsh integration script
            let script = include_str!("../../../shell-integration/zsh/murmur.zsh");
            println!("{script}");
        }
        "bash" => {
            let script = include_str!("../../../shell-integration/bash/murmur.bash");
            println!("{script}");
        }
        "fish" => {
            let script = include_str!("../../../shell-integration/fish/murmur.fish");
            println!("{script}");
        }
        other => {
            anyhow::bail!("Unsupported shell: {other}. Supported: zsh, bash, fish");
        }
    }
    Ok(())
}

async fn cmd_doctor() -> Result<()> {
    println!("Murmur Doctor");
    println!("=============\n");

    let mut all_ok = true;

    // 1. Check config file
    let config_path = Config::config_path();
    if config_path.exists() {
        match Config::load() {
            Ok(config) => {
                println!("[OK] Config file: {}", config_path.display());

                // Check providers
                if config.providers.is_empty() {
                    println!("[WARN] No providers configured in config.toml");
                    println!("       Add at least one provider (e.g., [providers.anthropic])");
                    all_ok = false;
                } else {
                    for (name, provider_cfg) in &config.providers {
                        if !provider_cfg.enabled {
                            println!("[SKIP] Provider '{name}': disabled");
                            continue;
                        }
                        if provider_cfg.api_key.is_none() && name != "ollama" {
                            println!("[WARN] Provider '{name}': no api_key set");
                            all_ok = false;
                        } else {
                            println!("[OK] Provider '{name}': configured");
                        }
                    }
                }
            }
            Err(e) => {
                println!("[FAIL] Config file parse error: {e}");
                all_ok = false;
            }
        }
    } else {
        println!("[WARN] Config file not found: {}", config_path.display());
        println!(
            "       Copy config.example.toml to {}",
            config_path.display()
        );
        all_ok = false;
    }

    println!();

    // 2. Check daemon
    if is_daemon_running() {
        println!("[OK] Daemon is running");

        let config = Config::load().unwrap_or_default();
        if let Ok(response) = send_request(&config.daemon.socket_path, methods::STATUS, None).await
        {
            if let Some(result) = response.result {
                let active: Vec<String> = result["providers_active"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                if active.is_empty() {
                    println!("[WARN] No active providers (completions will be empty)");
                    all_ok = false;
                } else {
                    println!("[OK] Active providers: {}", active.join(", "));
                }

                let cache = result["cache_entries"].as_u64().unwrap_or(0);
                println!("[INFO] Cache entries: {cache}");
            }
        }
    } else {
        println!("[WARN] Daemon is not running");
        println!("       Run: murmur start");
        all_ok = false;
    }

    println!();

    // 3. Check shell integration
    let shell = std::env::var("SHELL").unwrap_or_default();
    let shell_name = shell.rsplit('/').next().unwrap_or("unknown");
    println!("[INFO] Current shell: {shell_name}");

    let home = std::env::var("HOME").unwrap_or_default();
    let rc_file = match shell_name {
        "zsh" => format!("{home}/.zshrc"),
        "bash" => format!("{home}/.bashrc"),
        "fish" => format!("{home}/.config/fish/config.fish"),
        _ => String::new(),
    };

    if !rc_file.is_empty() {
        if let Ok(content) = std::fs::read_to_string(&rc_file) {
            if content.contains("murmur") {
                println!("[OK] Shell integration found in {rc_file}");
            } else {
                println!("[WARN] Shell integration not found in {rc_file}");
                println!("       Add: eval \"$(murmur setup {shell_name})\"");
                all_ok = false;
            }
        }
    }

    println!();

    // 4. Check socket connectivity
    let socket_path = Config::load()
        .map(|c| c.daemon.socket_path)
        .unwrap_or_else(|_| "/tmp/murmur.sock".to_string());
    if std::path::Path::new(&socket_path).exists() {
        println!("[OK] Socket exists: {socket_path}");
    } else {
        println!("[INFO] Socket not found: {socket_path} (daemon not running)");
    }

    println!();

    // Summary
    if all_ok {
        println!("All checks passed! Murmur is ready to use.");
    } else {
        println!("Some checks need attention. See warnings above.");
    }

    Ok(())
}

async fn cmd_voice_test(file: Option<String>, mode: String) -> Result<()> {
    let voice_mode = match mode.as_str() {
        "command" => VoiceMode::Command,
        "natural" => VoiceMode::Natural,
        other => anyhow::bail!("Unknown voice mode: {other}. Use 'command' or 'natural'."),
    };

    if !is_daemon_running() {
        println!("Murmur daemon is not running. Start it with: murmur start");
        return Ok(());
    }

    let audio_data = match file {
        Some(path) => {
            let data = std::fs::read(&path)
                .with_context(|| format!("Failed to read audio file: {path}"))?;
            println!("Loaded audio file: {path} ({} bytes)", data.len());
            data
        }
        None => {
            // Generate a minimal silent WAV for testing the pipeline
            println!("No audio file provided. Generating silent test audio...");
            println!("(For real testing, use: murmur voice test --file recording.wav)");
            murmur_voice::encode_wav(&vec![0i16; 16000], 16000)?
        }
    };

    // Base64 encode the audio
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&audio_data);

    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let params = serde_json::json!({
        "audio_data": encoded,
        "mode": voice_mode,
        "cwd": cwd,
        "shell": std::env::var("SHELL").ok(),
    });

    println!("Sending audio to daemon for processing (mode: {mode})...");

    let config = Config::load().unwrap_or_default();
    match send_request(
        &config.daemon.socket_path,
        methods::VOICE_PROCESS,
        Some(params),
    )
    .await
    {
        Ok(response) => {
            if let Some(result) = response.result {
                println!("\nVoice Processing Result:");
                println!(
                    "  Transcript: {}",
                    result["transcript"].as_str().unwrap_or("(none)")
                );
                println!(
                    "  Output:     {}",
                    result["output"].as_str().unwrap_or("(none)")
                );
                println!(
                    "  Mode:       {}",
                    result["mode"].as_str().unwrap_or("unknown")
                );
                println!(
                    "  Confidence: {:.1}%",
                    result["confidence"].as_f64().unwrap_or(0.0) * 100.0
                );
                println!(
                    "  Engine:     {}",
                    result["engine"].as_str().unwrap_or("unknown")
                );
                println!(
                    "  Latency:    {}ms",
                    result["latency_ms"].as_u64().unwrap_or(0)
                );
            } else if let Some(error) = response.error {
                println!("Voice processing error: {}", error.message);
            }
        }
        Err(e) => {
            println!("Failed to communicate with daemon: {e}");
        }
    }

    Ok(())
}

async fn cmd_voice_status() -> Result<()> {
    if !is_daemon_running() {
        println!("Murmur daemon is not running. Start it with: murmur start");
        return Ok(());
    }

    let config = Config::load().unwrap_or_default();
    match send_request(&config.daemon.socket_path, methods::VOICE_STATUS, None).await {
        Ok(response) => {
            if let Some(result) = response.result {
                println!("Voice Engine Status:");
                println!(
                    "  Capturing:  {}",
                    result["capturing"].as_bool().unwrap_or(false)
                );
                let engines = result["available_engines"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                println!(
                    "  Engines:    {}",
                    if engines.is_empty() {
                        "(none)".to_string()
                    } else {
                        engines.join(", ")
                    }
                );
                println!(
                    "  Active:     {}",
                    result["active_engine"].as_str().unwrap_or("(none)")
                );
            }
        }
        Err(e) => {
            println!("Failed to get voice status: {e}");
        }
    }

    Ok(())
}

/// Send a JSON-RPC request to the daemon and return the response.
async fn send_request(
    socket_path: &str,
    method: &str,
    params: Option<serde_json::Value>,
) -> Result<JsonRpcResponse> {
    let stream = UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();

    let request = JsonRpcRequest::new(method, params, RequestId::Number(1));
    let json = serde_json::to_string(&request)?;

    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response: JsonRpcResponse = serde_json::from_str(&line)?;
    Ok(response)
}

fn is_daemon_running() -> bool {
    let pid_path = Config::pid_path();
    if !pid_path.exists() {
        return false;
    }

    if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            // Check if process is alive
            unsafe { libc::kill(pid, 0) == 0 }
        } else {
            false
        }
    } else {
        false
    }
}
