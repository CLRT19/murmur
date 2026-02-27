use thiserror::Error;

/// Protocol-level errors.
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Failed to serialize message: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid JSON-RPC request: {0}")]
    InvalidRequest(String),

    #[error("Unknown method: {0}")]
    UnknownMethod(String),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
}
