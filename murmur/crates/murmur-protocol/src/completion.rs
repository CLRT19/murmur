use serde::{Deserialize, Serialize};

/// Request for shell command completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// The current command line input (what the user has typed so far).
    pub input: String,
    /// Cursor position within the input.
    pub cursor_pos: usize,
    /// Current working directory.
    pub cwd: String,
    /// Recent shell history lines for context.
    #[serde(default)]
    pub history: Vec<String>,
    /// Shell type (zsh, bash, fish).
    #[serde(default)]
    pub shell: Option<String>,
}

/// A single completion suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionItem {
    /// The completion text to insert.
    pub text: String,
    /// Human-readable description of what this completion does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The type of completion (command, argument, path, etc.).
    pub kind: CompletionKind,
    /// Confidence score from 0.0 to 1.0.
    #[serde(default = "default_score")]
    pub score: f64,
}

fn default_score() -> f64 {
    1.0
}

/// Type of completion suggestion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompletionKind {
    /// A shell command (e.g., `git commit`).
    Command,
    /// A command argument or flag (e.g., `--force`).
    Argument,
    /// A file or directory path.
    Path,
    /// A full command line suggestion.
    FullCommand,
    /// A code snippet (from FIM completion).
    Code,
}

/// Response containing completion suggestions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// List of completion suggestions, ordered by relevance.
    pub items: Vec<CompletionItem>,
    /// Which provider generated these completions.
    pub provider: String,
    /// Time taken to generate completions (milliseconds).
    pub latency_ms: u64,
    /// Whether this result came from cache.
    pub cached: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_completion_request() {
        let req = CompletionRequest {
            input: "git c".to_string(),
            cursor_pos: 5,
            cwd: "/home/user/project".to_string(),
            history: vec!["git status".to_string(), "git add .".to_string()],
            shell: Some("zsh".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let roundtrip: CompletionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.input, "git c");
        assert_eq!(roundtrip.cursor_pos, 5);
    }

    #[test]
    fn serialize_completion_response() {
        let resp = CompletionResponse {
            items: vec![
                CompletionItem {
                    text: "git commit -m \"".to_string(),
                    description: Some("Commit staged changes".to_string()),
                    kind: CompletionKind::FullCommand,
                    score: 0.95,
                },
                CompletionItem {
                    text: "git checkout".to_string(),
                    description: Some("Switch branches".to_string()),
                    kind: CompletionKind::Command,
                    score: 0.8,
                },
            ],
            provider: "anthropic".to_string(),
            latency_ms: 120,
            cached: false,
        };
        let json = serde_json::to_string_pretty(&resp).unwrap();
        assert!(json.contains("git commit"));
        assert!(json.contains("\"provider\": \"anthropic\""));
    }
}
