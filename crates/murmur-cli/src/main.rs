use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use murmur_daemon::config::Config;
use murmur_daemon::server::{self, Server};
use murmur_protocol::{methods, JsonRpcRequest, JsonRpcResponse, RequestId};
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { foreground, config } => cmd_start(foreground, config).await,
        Commands::Stop => cmd_stop().await,
        Commands::Status => cmd_status().await,
        Commands::Setup { shell } => cmd_setup(&shell),
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
