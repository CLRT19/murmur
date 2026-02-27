use crate::GitInfo;
use thiserror::Error;
use tokio::process::Command;
use tracing::debug;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("Not a git repository")]
    NotARepo,
    #[error("Git command failed: {0}")]
    CommandFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Collects git context for a directory.
pub struct GitContext {
    cwd: String,
}

impl GitContext {
    pub fn new(cwd: &str) -> Self {
        Self {
            cwd: cwd.to_string(),
        }
    }

    /// Collect git information for the current directory.
    pub async fn collect(&self) -> Result<GitInfo, GitError> {
        // Check if we're in a git repo
        let repo_root = self.git_output(&["rev-parse", "--show-toplevel"]).await?;
        if repo_root.is_empty() {
            return Err(GitError::NotARepo);
        }

        debug!(repo_root = %repo_root, "Collecting git context");

        let branch = self
            .git_output(&["rev-parse", "--abbrev-ref", "HEAD"])
            .await
            .unwrap_or_else(|_| "unknown".to_string());

        let status = self.git_output(&["status", "--porcelain"]).await?;
        let dirty = !status.is_empty();

        let log_output = self
            .git_output(&["log", "--oneline", "-5", "--no-decorate"])
            .await
            .unwrap_or_default();
        let recent_commits: Vec<String> = log_output
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();

        Ok(GitInfo {
            branch,
            dirty,
            recent_commits,
            repo_root,
        })
    }

    async fn git_output(&self, args: &[&str]) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.cwd)
            .output()
            .await?;

        if !output.status.success() {
            return Err(GitError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
