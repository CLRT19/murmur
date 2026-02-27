//! Murmur MCP Server — Exposes Murmur daemon functionality as MCP tools.
//!
//! This binary reads JSON-RPC from stdin and writes responses to stdout,
//! implementing the Model Context Protocol (MCP) specification.
//!
//! Tools provided:
//! - murmur_complete: Get AI-powered shell command completions
//! - murmur_status: Get daemon status and active providers
//! - murmur_record_command: Record a command execution into cross-tool history
//! - murmur_get_history: Get cross-tool command history

use anyhow::Result;
use murmur_protocol::{JsonRpcRequest, JsonRpcResponse, RequestId};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::debug;

const PROTOCOL_VERSION: &str = "2025-11-25";
const SERVER_NAME: &str = "murmur-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> Result<()> {
    // Log to stderr only (stdout is for MCP protocol)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        let response = handle_message(trimmed).await;

        if let Some(response) = response {
            let json = serde_json::to_string(&response)?;
            stdout.write_all(json.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }

        line.clear();
    }

    Ok(())
}

async fn handle_message(raw: &str) -> Option<Value> {
    let msg: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Some(make_error(-32700, "Parse error", Value::Null)),
    };

    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = msg.get("id").cloned();

    debug!(method = method, "MCP request");

    match method {
        "initialize" => {
            let result = serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": { "listChanged": false }
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION
                }
            });
            Some(make_result(result, id))
        }
        "notifications/initialized" => {
            // Notification — no response
            None
        }
        "tools/list" => {
            let tools = serde_json::json!({
                "tools": [
                    {
                        "name": "murmur_complete",
                        "description": "Get AI-powered shell command completions from the Murmur daemon. Suggests completions for partial shell commands using context from history, git state, and project type.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "input": {
                                    "type": "string",
                                    "description": "Partial command to complete (e.g., 'git co', 'docker r')"
                                },
                                "cwd": {
                                    "type": "string",
                                    "description": "Current working directory for context"
                                },
                                "shell": {
                                    "type": "string",
                                    "description": "Shell type: zsh, bash, or fish"
                                }
                            },
                            "required": ["input", "cwd"]
                        }
                    },
                    {
                        "name": "murmur_status",
                        "description": "Get the current status of the Murmur daemon including active providers, cache size, voice engine status, and history count.",
                        "inputSchema": {
                            "type": "object",
                            "additionalProperties": false
                        }
                    },
                    {
                        "name": "murmur_record_command",
                        "description": "Record a command execution into Murmur's cross-tool command history. This helps Murmur provide better completions based on commands run from AI tools.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "command": {
                                    "type": "string",
                                    "description": "The shell command that was executed"
                                },
                                "cwd": {
                                    "type": "string",
                                    "description": "Directory where the command was run"
                                },
                                "exit_code": {
                                    "type": "integer",
                                    "description": "Exit code of the command (0 = success)"
                                },
                                "source": {
                                    "type": "string",
                                    "description": "The tool that ran the command (e.g., 'codex', 'claude-code')"
                                }
                            },
                            "required": ["command", "cwd", "exit_code"]
                        }
                    },
                    {
                        "name": "murmur_get_history",
                        "description": "Get recent cross-tool command history from Murmur. Returns commands from all sources (terminal, Claude Code, Codex) in reverse chronological order.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "cwd": {
                                    "type": "string",
                                    "description": "Filter history by working directory (optional)"
                                },
                                "limit": {
                                    "type": "integer",
                                    "description": "Maximum entries to return (default: 50)"
                                }
                            }
                        }
                    }
                ]
            });
            Some(make_result(tools, id))
        }
        "tools/call" => {
            let tool_name = msg
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let arguments = msg
                .get("params")
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            let result = handle_tool_call(tool_name, arguments).await;
            Some(make_result(result, id))
        }
        _ => {
            // Unknown method
            if id.is_some() {
                Some(make_error(
                    -32601,
                    "Method not found",
                    id.unwrap_or(Value::Null),
                ))
            } else {
                // Unknown notification — ignore
                None
            }
        }
    }
}

async fn handle_tool_call(tool_name: &str, arguments: Value) -> Value {
    match tool_name {
        "murmur_complete" => {
            let input = arguments
                .get("input")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let cwd = arguments.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");
            let shell = arguments.get("shell").and_then(|v| v.as_str());

            let params = serde_json::json!({
                "input": input,
                "cursor_pos": input.len(),
                "cwd": cwd,
                "shell": shell,
            });

            match send_to_daemon("complete", Some(params)).await {
                Ok(response) => tool_result(response, false),
                Err(e) => tool_error(&format!("Failed to connect to Murmur daemon: {e}")),
            }
        }
        "murmur_status" => match send_to_daemon("status", None).await {
            Ok(response) => tool_result(response, false),
            Err(e) => tool_error(&format!("Failed to connect to Murmur daemon: {e}")),
        },
        "murmur_record_command" => {
            let params = serde_json::json!({
                "source": arguments.get("source").and_then(|v| v.as_str()).unwrap_or("mcp"),
                "command": arguments.get("command").and_then(|v| v.as_str()).unwrap_or(""),
                "cwd": arguments.get("cwd").and_then(|v| v.as_str()).unwrap_or("."),
                "exit_code": arguments.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(0),
            });

            match send_to_daemon("context/update", Some(params)).await {
                Ok(response) => tool_result(response, false),
                Err(e) => tool_error(&format!("Failed to connect to Murmur daemon: {e}")),
            }
        }
        "murmur_get_history" => {
            let params = serde_json::json!({
                "cwd": arguments.get("cwd").and_then(|v| v.as_str()),
                "limit": arguments.get("limit").and_then(|v| v.as_u64()).unwrap_or(50),
            });

            match send_to_daemon("history/list", Some(params)).await {
                Ok(response) => tool_result(response, false),
                Err(e) => tool_error(&format!("Failed to connect to Murmur daemon: {e}")),
            }
        }
        _ => tool_error(&format!("Unknown tool: {tool_name}")),
    }
}

/// Send a JSON-RPC request to the Murmur daemon via Unix socket.
async fn send_to_daemon(method: &str, params: Option<Value>) -> Result<Value> {
    let socket_path = "/tmp/murmur.sock";

    let stream = tokio::net::UnixStream::connect(socket_path).await?;
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

    if let Some(result) = response.result {
        Ok(result)
    } else if let Some(error) = response.error {
        anyhow::bail!("Daemon error: {}", error.message)
    } else {
        Ok(Value::Null)
    }
}

fn tool_result(data: Value, is_error: bool) -> Value {
    serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&data).unwrap_or_else(|_| data.to_string())
            }
        ],
        "isError": is_error
    })
}

fn tool_error(message: &str) -> Value {
    serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": message
            }
        ],
        "isError": true
    })
}

fn make_result(result: Value, id: Option<Value>) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result
    })
}

fn make_error(code: i32, message: &str, id: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}
