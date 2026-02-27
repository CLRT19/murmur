//! Speculative pre-fetching for common command continuations.
//!
//! After a user requests completions for "git c", we can predict that
//! "git co", "git ch", "git cl" are likely next inputs and pre-fetch those
//! completions in the background, making the next request instant.

use std::collections::HashMap;

/// Common command prefixes and their likely continuations.
/// Each entry maps a prefix pattern to a list of likely next characters/words.
struct PrefetchRules {
    rules: HashMap<&'static str, Vec<&'static str>>,
}

impl PrefetchRules {
    fn new() -> Self {
        let mut rules = HashMap::new();

        // Git commands
        rules.insert("git", vec![
            "git commit", "git checkout", "git push", "git pull", "git status",
            "git diff", "git log", "git branch", "git stash", "git merge",
            "git rebase", "git add", "git reset",
        ]);
        rules.insert("git c", vec![
            "git commit", "git checkout", "git cherry-pick", "git clone",
        ]);
        rules.insert("git s", vec![
            "git status", "git stash", "git show",
        ]);
        rules.insert("git p", vec![
            "git push", "git pull",
        ]);
        rules.insert("git b", vec![
            "git branch", "git bisect",
        ]);

        // Cargo commands
        rules.insert("cargo", vec![
            "cargo build", "cargo test", "cargo run", "cargo clippy",
            "cargo fmt", "cargo check", "cargo bench",
        ]);
        rules.insert("cargo t", vec![
            "cargo test", "cargo tree",
        ]);
        rules.insert("cargo b", vec![
            "cargo build", "cargo bench",
        ]);

        // npm commands
        rules.insert("npm", vec![
            "npm install", "npm run", "npm test", "npm start",
            "npm build", "npm publish",
        ]);
        rules.insert("npm r", vec![
            "npm run", "npm run dev", "npm run build", "npm run test",
        ]);

        // Docker commands
        rules.insert("docker", vec![
            "docker ps", "docker compose", "docker build", "docker run",
            "docker images", "docker logs",
        ]);
        rules.insert("docker c", vec![
            "docker compose up", "docker compose down", "docker compose logs",
        ]);

        // kubectl
        rules.insert("kubectl", vec![
            "kubectl get", "kubectl describe", "kubectl apply", "kubectl logs",
            "kubectl delete", "kubectl exec",
        ]);

        Self { rules }
    }
}

/// Determine which inputs to pre-fetch based on the current input.
/// Returns a list of predicted next inputs that should be pre-fetched.
pub fn predict_next_inputs(input: &str) -> Vec<String> {
    let rules = PrefetchRules::new();
    let input_trimmed = input.trim();

    // Look for the longest matching prefix
    let mut best_match: Option<(&str, &Vec<&str>)> = None;
    for (prefix, continuations) in &rules.rules {
        if input_trimmed.starts_with(prefix) || *prefix == input_trimmed {
            match best_match {
                None => best_match = Some((prefix, continuations)),
                Some((current_best, _)) if prefix.len() > current_best.len() => {
                    best_match = Some((prefix, continuations));
                }
                _ => {}
            }
        }
    }

    match best_match {
        Some((_, continuations)) => continuations
            .iter()
            .filter(|c| {
                // Only predict inputs that are longer than what user typed
                // and start with what they typed
                c.len() > input_trimmed.len() && c.starts_with(input_trimmed)
            })
            .map(|c| c.to_string())
            .collect(),
        None => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predict_git_continuations() {
        let predictions = predict_next_inputs("git c");
        assert!(!predictions.is_empty());
        assert!(predictions.contains(&"git commit".to_string()));
        assert!(predictions.contains(&"git checkout".to_string()));
    }

    #[test]
    fn predict_cargo_continuations() {
        let predictions = predict_next_inputs("cargo t");
        assert!(predictions.contains(&"cargo test".to_string()));
        assert!(predictions.contains(&"cargo tree".to_string()));
    }

    #[test]
    fn no_predictions_for_unknown() {
        let predictions = predict_next_inputs("zzz unknown");
        assert!(predictions.is_empty());
    }

    #[test]
    fn no_predictions_for_exact_match() {
        // If user already typed "git commit", don't predict "git commit" again
        let predictions = predict_next_inputs("git commit");
        assert!(!predictions.iter().any(|p| p == "git commit"));
    }

    #[test]
    fn predict_npm_run() {
        let predictions = predict_next_inputs("npm r");
        assert!(predictions.contains(&"npm run".to_string()));
    }
}
