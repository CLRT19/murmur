//! Murmur Context — Collects shell and project context for better completions.

mod env;
mod git;
mod history;
mod project;

pub use env::EnvContext;
pub use git::GitContext;
pub use history::HistoryCollector;
pub use project::{ProjectDetector, ProjectType};

use serde::{Deserialize, Serialize};

/// Aggregated context for a completion request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShellContext {
    /// Recent shell history lines.
    pub history: Vec<String>,
    /// Current working directory.
    pub cwd: String,
    /// Current shell type.
    pub shell: String,
    /// Git context (if in a repo).
    pub git: Option<GitInfo>,
    /// Detected project type.
    pub project: Option<ProjectType>,
    /// Relevant environment variables.
    pub env_vars: Vec<(String, String)>,
}

/// Git repository information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    /// Current branch name.
    pub branch: String,
    /// Whether there are uncommitted changes.
    pub dirty: bool,
    /// Recent commit messages.
    pub recent_commits: Vec<String>,
    /// Repository root path.
    pub repo_root: String,
}

/// Collects all context for a given working directory.
pub async fn collect_context(cwd: &str, shell: &str, history_lines: usize) -> ShellContext {
    let history = HistoryCollector::new(shell)
        .collect(history_lines)
        .await
        .unwrap_or_default();

    let git = GitContext::new(cwd).collect().await.ok();
    let project = ProjectDetector::detect(cwd).await;
    let env_vars = EnvContext::collect_relevant();

    ShellContext {
        history,
        cwd: cwd.to_string(),
        shell: shell.to_string(),
        git,
        project,
        env_vars,
    }
}
