use murmur_daemon::config::{Config, DaemonConfig};
use murmur_daemon::server::Server;
use murmur_protocol::*;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Create a test config with a unique socket path.
fn test_config(socket_path: &str) -> Config {
    Config {
        daemon: DaemonConfig {
            socket_path: socket_path.to_string(),
            cache_size: 100,
            log_level: "warn".to_string(),
        },
        ..Config::default()
    }
}

/// Send a JSON-RPC request and read the response.
async fn send_request(
    socket_path: &str,
    method: &str,
    params: Option<serde_json::Value>,
) -> JsonRpcResponse {
    let stream = UnixStream::connect(socket_path).await.unwrap();
    let (reader, mut writer) = stream.into_split();

    let request = JsonRpcRequest::new(method, params, RequestId::Number(1));
    let json = serde_json::to_string(&request).unwrap();

    writer.write_all(json.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();

    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();

    serde_json::from_str(&line).unwrap()
}

/// Start a daemon server in the background for testing.
async fn start_test_server(config: Config) {
    let server = Server::new(config);
    tokio::spawn(async move {
        let _ = server.run().await;
    });
    // Give the server a moment to bind
    tokio::time::sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_status_request() {
    let socket = format!("/tmp/murmur-test-status-{}.sock", std::process::id());
    let config = test_config(&socket);

    start_test_server(config).await;

    let response = send_request(&socket, methods::STATUS, None).await;

    assert!(response.error.is_none(), "Status should not return error");
    let result = response.result.unwrap();
    assert_eq!(result["status"], "running");
    assert!(result["cache_entries"].is_number());

    // Clean up
    let _ = std::fs::remove_file(&socket);
}

#[tokio::test]
async fn test_complete_request() {
    let socket = format!("/tmp/murmur-test-complete-{}.sock", std::process::id());
    let config = test_config(&socket);

    start_test_server(config).await;

    let params = serde_json::json!({
        "input": "git c",
        "cursor_pos": 5,
        "cwd": "/tmp",
        "shell": "zsh"
    });

    let response = send_request(&socket, methods::COMPLETE, Some(params)).await;

    assert!(
        response.error.is_none(),
        "Complete should not return error: {:?}",
        response.error
    );

    let result = response.result.unwrap();
    // Without a configured provider, items should be empty but response valid
    assert!(result["items"].is_array());
    assert!(result["latency_ms"].is_number());
    assert_eq!(result["cached"], false);

    // Clean up
    let _ = std::fs::remove_file(&socket);
}

#[tokio::test]
async fn test_complete_cache_hit() {
    let socket = format!("/tmp/murmur-test-cache-{}.sock", std::process::id());
    let config = test_config(&socket);

    start_test_server(config).await;

    let params = serde_json::json!({
        "input": "ls -",
        "cursor_pos": 4,
        "cwd": "/tmp",
        "shell": "zsh"
    });

    // First request — cache miss
    let response1 = send_request(&socket, methods::COMPLETE, Some(params.clone())).await;
    assert_eq!(response1.result.as_ref().unwrap()["cached"], false);

    // Second request — should be cache hit
    let response2 = send_request(&socket, methods::COMPLETE, Some(params)).await;
    assert_eq!(response2.result.as_ref().unwrap()["cached"], true);

    // Clean up
    let _ = std::fs::remove_file(&socket);
}

#[tokio::test]
async fn test_invalid_method() {
    let socket = format!("/tmp/murmur-test-method-{}.sock", std::process::id());
    let config = test_config(&socket);

    start_test_server(config).await;

    let response = send_request(&socket, "nonexistent/method", None).await;

    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap().code, METHOD_NOT_FOUND);

    // Clean up
    let _ = std::fs::remove_file(&socket);
}

#[tokio::test]
async fn test_complete_missing_params() {
    let socket = format!("/tmp/murmur-test-params-{}.sock", std::process::id());
    let config = test_config(&socket);

    start_test_server(config).await;

    let response = send_request(&socket, methods::COMPLETE, None).await;

    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap().code, INVALID_PARAMS);

    // Clean up
    let _ = std::fs::remove_file(&socket);
}

#[tokio::test]
async fn test_multiple_requests_same_connection() {
    let socket = format!("/tmp/murmur-test-multi-{}.sock", std::process::id());
    let config = test_config(&socket);

    start_test_server(config).await;

    // Open a single connection and send multiple requests
    let stream = UnixStream::connect(&socket).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Request 1: status
    let req1 = JsonRpcRequest::new(methods::STATUS, None, RequestId::Number(1));
    let json1 = serde_json::to_string(&req1).unwrap();
    writer.write_all(json1.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();

    let mut line1 = String::new();
    reader.read_line(&mut line1).await.unwrap();
    let resp1: JsonRpcResponse = serde_json::from_str(&line1).unwrap();
    assert!(resp1.error.is_none());

    // Request 2: complete
    let params = serde_json::json!({
        "input": "echo ",
        "cursor_pos": 5,
        "cwd": "/tmp",
        "shell": "bash"
    });
    let req2 = JsonRpcRequest::new(methods::COMPLETE, Some(params), RequestId::Number(2));
    let json2 = serde_json::to_string(&req2).unwrap();
    writer.write_all(json2.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();

    let mut line2 = String::new();
    reader.read_line(&mut line2).await.unwrap();
    let resp2: JsonRpcResponse = serde_json::from_str(&line2).unwrap();
    assert!(resp2.error.is_none());

    // Clean up
    let _ = std::fs::remove_file(&socket);
}
