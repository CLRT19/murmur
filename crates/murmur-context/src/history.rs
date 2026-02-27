use std::path::PathBuf;
use thiserror::Error;
use tracing::debug;

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("Failed to read history file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Could not determine history file path for shell: {0}")]
    UnknownShell(String),
}

/// Collects shell history for context.
pub struct HistoryCollector {
    shell: String,
}

impl HistoryCollector {
    pub fn new(shell: &str) -> Self {
        Self {
            shell: shell.to_string(),
        }
    }

    /// Returns the path to the shell history file.
    fn history_path(&self) -> Result<PathBuf, HistoryError> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        match self.shell.as_str() {
            "zsh" => Ok(PathBuf::from(format!("{home}/.zsh_history"))),
            "bash" => Ok(PathBuf::from(format!("{home}/.bash_history"))),
            "fish" => Ok(PathBuf::from(format!(
                "{home}/.local/share/fish/fish_history"
            ))),
            other => Err(HistoryError::UnknownShell(other.to_string())),
        }
    }

    /// Collect the last `n` lines from shell history.
    pub async fn collect(&self, n: usize) -> Result<Vec<String>, HistoryError> {
        let path = self.history_path()?;
        debug!(path = %path.display(), n, "Reading shell history");

        let content = tokio::fs::read_to_string(&path).await?;
        let lines: Vec<String> = content
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                // Handle zsh extended history format: ": timestamp:duration;command"
                if line.starts_with(": ") {
                    if let Some(idx) = line.find(';') {
                        line[idx + 1..].to_string()
                    } else {
                        line.to_string()
                    }
                } else {
                    line.to_string()
                }
            })
            .collect();

        let start = lines.len().saturating_sub(n);
        Ok(lines[start..].to_vec())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_zsh_history_line() {
        let line = ": 1234567890:0;git status";
        if let Some(idx) = line.find(';') {
            assert_eq!(&line[idx + 1..], "git status");
        }
    }

    #[test]
    fn parse_zsh_extended_history_preserves_commands() {
        // Simulate what the collect() logic does for extended history lines
        let lines = [
            ": 1700000001:0;ls -la",
            ": 1700000002:0;git commit -m \"fix\"",
            "plain command",
            "",
        ];
        let result: Vec<String> = lines
            .iter()
            .filter(|line| !line.is_empty())
            .map(|line| {
                if line.starts_with(": ") {
                    if let Some(idx) = line.find(';') {
                        line[idx + 1..].to_string()
                    } else {
                        line.to_string()
                    }
                } else {
                    line.to_string()
                }
            })
            .collect();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "ls -la");
        assert_eq!(result[1], "git commit -m \"fix\"");
        assert_eq!(result[2], "plain command");
    }
}
