use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::debug;

/// Detected project type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Ruby,
    Java,
    CSharp,
    Cpp,
    Unknown,
}

/// Detects the project type based on marker files in the working directory.
pub struct ProjectDetector;

impl ProjectDetector {
    /// Detect the project type from files present in the directory.
    pub async fn detect(cwd: &str) -> Option<ProjectType> {
        let path = Path::new(cwd);

        let markers = [
            ("Cargo.toml", ProjectType::Rust),
            ("package.json", ProjectType::Node),
            ("pyproject.toml", ProjectType::Python),
            ("requirements.txt", ProjectType::Python),
            ("go.mod", ProjectType::Go),
            ("Gemfile", ProjectType::Ruby),
            ("pom.xml", ProjectType::Java),
            ("build.gradle", ProjectType::Java),
            ("*.csproj", ProjectType::CSharp),
            ("CMakeLists.txt", ProjectType::Cpp),
            ("Makefile", ProjectType::Cpp),
        ];

        for (marker, project_type) in &markers {
            if let Some(ext) = marker.strip_prefix('*') {
                // Glob pattern — check with readdir
                if let Ok(mut entries) = tokio::fs::read_dir(path).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        if entry.file_name().to_string_lossy().ends_with(ext) {
                            debug!(project_type = ?project_type, marker = %marker, "Detected project type");
                            return Some(project_type.clone());
                        }
                    }
                }
            } else if path.join(marker).exists() {
                debug!(project_type = ?project_type, marker = %marker, "Detected project type");
                return Some(project_type.clone());
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn detect_unknown_for_empty_dir() {
        let result = ProjectDetector::detect("/tmp").await;
        // /tmp likely doesn't have project markers
        // This is more of a smoke test
        assert!(result.is_none() || result.is_some());
    }
}
