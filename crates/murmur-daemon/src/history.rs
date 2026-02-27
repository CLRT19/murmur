use murmur_protocol::HistoryEntry;
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

/// Cross-tool command history store.
///
/// Stores command executions from all sources (terminal, Claude Code, Codex, etc.)
/// in a bounded ring buffer. Newest entries are at the front.
pub struct CommandHistory {
    entries: VecDeque<HistoryEntry>,
    max_entries: usize,
}

impl CommandHistory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    /// Record a new command execution.
    pub fn record(&mut self, command: String, cwd: String, source: String, exit_code: i32) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let entry = HistoryEntry {
            command,
            cwd,
            source,
            exit_code,
            timestamp,
        };

        self.entries.push_front(entry);

        // Trim to max size
        while self.entries.len() > self.max_entries {
            self.entries.pop_back();
        }
    }

    /// List recent entries, optionally filtered by cwd.
    pub fn list(&self, cwd: Option<&str>, limit: usize) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|e| match cwd {
                Some(dir) => e.cwd == dir,
                None => true,
            })
            .take(limit)
            .collect()
    }

    /// Number of entries stored.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the history is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_list() {
        let mut history = CommandHistory::new(100);
        history.record(
            "git status".to_string(),
            "/tmp".to_string(),
            "terminal".to_string(),
            0,
        );
        history.record(
            "cargo test".to_string(),
            "/tmp".to_string(),
            "claude-code".to_string(),
            0,
        );

        let all = history.list(None, 10);
        assert_eq!(all.len(), 2);
        // Newest first
        assert_eq!(all[0].command, "cargo test");
        assert_eq!(all[1].command, "git status");
    }

    #[test]
    fn filter_by_cwd() {
        let mut history = CommandHistory::new(100);
        history.record(
            "ls".to_string(),
            "/home".to_string(),
            "terminal".to_string(),
            0,
        );
        history.record(
            "pwd".to_string(),
            "/tmp".to_string(),
            "terminal".to_string(),
            0,
        );

        let filtered = history.list(Some("/tmp"), 10);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].command, "pwd");
    }

    #[test]
    fn respects_max_entries() {
        let mut history = CommandHistory::new(3);
        for i in 0..5 {
            history.record(
                format!("cmd {i}"),
                "/tmp".to_string(),
                "test".to_string(),
                0,
            );
        }
        assert_eq!(history.len(), 3);
        let entries = history.list(None, 10);
        assert_eq!(entries[0].command, "cmd 4");
    }

    #[test]
    fn respects_limit() {
        let mut history = CommandHistory::new(100);
        for i in 0..10 {
            history.record(
                format!("cmd {i}"),
                "/tmp".to_string(),
                "test".to_string(),
                0,
            );
        }
        let entries = history.list(None, 3);
        assert_eq!(entries.len(), 3);
    }
}
