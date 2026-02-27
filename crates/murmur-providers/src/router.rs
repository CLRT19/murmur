use murmur_context::ShellContext;
use murmur_protocol::CompletionRequest;

/// Decision about which provider to route a request to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    /// Route to the shell command provider (e.g., Anthropic Haiku).
    Shell,
    /// Route to the code completion provider (e.g., Codestral).
    Code,
    /// Route to the local/offline provider (e.g., Ollama).
    Local,
}

/// Routes completion requests to the appropriate provider.
pub struct ProviderRouter;

impl ProviderRouter {
    /// Decide which provider to use based on the request and context.
    pub fn route(request: &CompletionRequest, context: &ShellContext) -> RouteDecision {
        // If we detect a code-heavy context, route to code provider
        if Self::is_code_context(request, context) {
            return RouteDecision::Code;
        }

        // Default to shell provider
        RouteDecision::Shell
    }

    fn is_code_context(request: &CompletionRequest, context: &ShellContext) -> bool {
        let input = &request.input;

        // Check if the input looks like it's editing a file with code-related tools
        let code_commands = [
            "vim",
            "nvim",
            "nano",
            "code",
            "cat",
            "python",
            "node",
            "cargo run",
        ];
        for cmd in &code_commands {
            if input.starts_with(cmd) {
                return true;
            }
        }

        // Check project type for code-heavy contexts
        matches!(
            context.project,
            Some(
                murmur_context::ProjectType::Rust
                    | murmur_context::ProjectType::Node
                    | murmur_context::ProjectType::Python
                    | murmur_context::ProjectType::Go
            )
        ) && Self::looks_like_code_input(input)
    }

    fn looks_like_code_input(input: &str) -> bool {
        // Heuristic: contains code-like patterns
        input.contains("fn ")
            || input.contains("def ")
            || input.contains("function ")
            || input.contains("class ")
            || input.contains("import ")
            || input.contains("const ")
            || input.contains("let ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_git_to_shell() {
        let request = CompletionRequest {
            input: "git c".to_string(),
            cursor_pos: 5,
            cwd: "/home/user".to_string(),
            history: vec![],
            shell: Some("zsh".to_string()),
        };
        let context = ShellContext::default();
        assert_eq!(
            ProviderRouter::route(&request, &context),
            RouteDecision::Shell
        );
    }

    #[test]
    fn route_vim_to_code() {
        let request = CompletionRequest {
            input: "vim src/main.rs".to_string(),
            cursor_pos: 15,
            cwd: "/home/user/project".to_string(),
            history: vec![],
            shell: Some("zsh".to_string()),
        };
        let context = ShellContext::default();
        assert_eq!(
            ProviderRouter::route(&request, &context),
            RouteDecision::Code
        );
    }
}
