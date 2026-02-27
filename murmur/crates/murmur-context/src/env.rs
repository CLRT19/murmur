/// Collects relevant environment variables for context.
pub struct EnvContext;

/// Environment variables that are useful for completion context.
const RELEVANT_VARS: &[&str] = &[
    "EDITOR",
    "VISUAL",
    "SHELL",
    "TERM",
    "LANG",
    "VIRTUAL_ENV",
    "CONDA_DEFAULT_ENV",
    "NODE_ENV",
    "RUST_LOG",
    "GOPATH",
    "CARGO_HOME",
    "NVM_DIR",
    "PYENV_VERSION",
    "RBENV_VERSION",
];

impl EnvContext {
    /// Collect relevant environment variables (not secrets).
    pub fn collect_relevant() -> Vec<(String, String)> {
        RELEVANT_VARS
            .iter()
            .filter_map(|&var| std::env::var(var).ok().map(|val| (var.to_string(), val)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_env_does_not_panic() {
        let vars = EnvContext::collect_relevant();
        // Should return a vec, possibly empty
        assert!(vars.len() <= RELEVANT_VARS.len());
    }

    #[test]
    fn does_not_collect_secrets() {
        // Ensure we don't accidentally collect API keys
        let vars = EnvContext::collect_relevant();
        for (key, _) in &vars {
            assert!(!key.contains("API_KEY"));
            assert!(!key.contains("SECRET"));
            assert!(!key.contains("TOKEN"));
            assert!(!key.contains("PASSWORD"));
        }
    }
}
