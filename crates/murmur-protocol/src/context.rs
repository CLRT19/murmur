use serde::{Deserialize, Serialize};

/// Request to record a command execution from an external tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUpdateRequest {
    /// Source of the command (e.g., "claude-code", "codex", "terminal").
    pub source: String,
    /// The command that was executed.
    pub command: String,
    /// Working directory where the command ran.
    pub cwd: String,
    /// Exit code (0 = success).
    #[serde(default)]
    pub exit_code: i32,
    /// Optional session ID for grouping commands.
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Request to list cross-tool command history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryListRequest {
    /// Filter by working directory (optional).
    #[serde(default)]
    pub cwd: Option<String>,
    /// Maximum number of entries to return.
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

/// A single entry in the cross-tool command history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// The command that was executed.
    pub command: String,
    /// Working directory.
    pub cwd: String,
    /// Source tool.
    pub source: String,
    /// Exit code.
    pub exit_code: i32,
    /// Unix timestamp (seconds since epoch).
    pub timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_update_roundtrip() {
        let req = ContextUpdateRequest {
            source: "claude-code".to_string(),
            command: "git commit -m 'test'".to_string(),
            cwd: "/home/user/project".to_string(),
            exit_code: 0,
            session_id: Some("abc123".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ContextUpdateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.source, "claude-code");
        assert_eq!(parsed.exit_code, 0);
    }

    #[test]
    fn history_list_defaults() {
        let json = r#"{"cwd": "/tmp"}"#;
        let req: HistoryListRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.limit, 50);
    }
}
