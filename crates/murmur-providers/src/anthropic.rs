use async_trait::async_trait;
use murmur_context::ShellContext;
use murmur_protocol::{CompletionItem, CompletionKind, CompletionRequest};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::{Provider, ProviderConfig, ProviderError};

const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
const DEFAULT_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
    timeout: std::time::Duration,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
    system: String,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

impl AnthropicProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, ProviderError> {
        let api_key = config
            .api_key
            .clone()
            .ok_or_else(|| ProviderError::NotConfigured("anthropic: api_key required".into()))?;

        Ok(Self {
            client: Client::new(),
            api_key,
            model: config
                .model
                .clone()
                .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            endpoint: config
                .endpoint
                .clone()
                .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string()),
            timeout: std::time::Duration::from_millis(config.timeout_ms),
        })
    }

    fn build_system_prompt(&self, context: &ShellContext) -> String {
        let mut prompt = String::from(
            "You are a shell command autocomplete engine. Given the user's partial command \
             and context, suggest completions. Respond ONLY with a JSON array of objects, \
             each with \"text\" (the completion) and \"description\" (brief explanation). \
             Order by relevance. Maximum 5 suggestions.",
        );

        if let Some(ref git) = context.git {
            prompt.push_str(&format!(
                "\n\nGit context: branch={}, dirty={}",
                git.branch, git.dirty
            ));
        }

        if let Some(ref project) = context.project {
            prompt.push_str(&format!("\nProject type: {project:?}"));
        }

        if !context.env_vars.is_empty() {
            let vars: Vec<String> = context
                .env_vars
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            prompt.push_str(&format!("\nEnvironment: {}", vars.join(", ")));
        }

        prompt
    }

    fn build_user_prompt(&self, request: &CompletionRequest, context: &ShellContext) -> String {
        let mut prompt = format!(
            "Shell: {}\nCWD: {}\nPartial command: `{}`",
            request.shell.as_deref().unwrap_or("unknown"),
            request.cwd,
            request.input,
        );

        if !context.history.is_empty() {
            let recent: Vec<&String> = context.history.iter().rev().take(10).collect();
            prompt.push_str("\n\nRecent history:\n");
            for cmd in recent.iter().rev() {
                prompt.push_str(&format!("  {cmd}\n"));
            }
        }

        prompt
    }

    fn parse_completions(&self, text: &str) -> Vec<CompletionItem> {
        // Try to parse the response as JSON array
        #[derive(Deserialize)]
        struct Suggestion {
            text: String,
            description: Option<String>,
        }

        // Extract JSON from the response (might be wrapped in markdown code blocks)
        let json_str = text
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        match serde_json::from_str::<Vec<Suggestion>>(json_str) {
            Ok(suggestions) => suggestions
                .into_iter()
                .enumerate()
                .map(|(i, s)| CompletionItem {
                    text: s.text,
                    description: s.description,
                    kind: CompletionKind::FullCommand,
                    score: 1.0 - (i as f64 * 0.1),
                })
                .collect(),
            Err(e) => {
                warn!(error = %e, "Failed to parse completion response as JSON");
                vec![]
            }
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
        context: &ShellContext,
    ) -> Result<Vec<CompletionItem>, ProviderError> {
        let system = self.build_system_prompt(context);
        let user = self.build_user_prompt(request, context);

        debug!(model = %self.model, input = %request.input, "Requesting completion from Anthropic");

        let body = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 512,
            messages: vec![Message {
                role: "user".to_string(),
                content: user,
            }],
            system,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .timeout(self.timeout)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let api_response: AnthropicResponse = response.json().await?;
        let text = api_response
            .content
            .first()
            .map(|b| b.text.as_str())
            .unwrap_or("[]");

        Ok(self.parse_completions(text))
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        // Simple connectivity check — just hit the API with minimal request
        debug!("Anthropic health check");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_completions() {
        let provider = AnthropicProvider {
            client: Client::new(),
            api_key: "test".to_string(),
            model: "test".to_string(),
            endpoint: "test".to_string(),
            timeout: std::time::Duration::from_secs(5),
        };

        let text = r#"[
            {"text": "git commit -m \"fix: resolve issue\"", "description": "Commit staged changes"},
            {"text": "git checkout -b feature/new", "description": "Create and switch to new branch"}
        ]"#;

        let completions = provider.parse_completions(text);
        assert_eq!(completions.len(), 2);
        assert!(completions[0].text.contains("git commit"));
        assert!(completions[0].score > completions[1].score);
    }

    #[test]
    fn parse_markdown_wrapped_json() {
        let provider = AnthropicProvider {
            client: Client::new(),
            api_key: "test".to_string(),
            model: "test".to_string(),
            endpoint: "test".to_string(),
            timeout: std::time::Duration::from_secs(5),
        };

        let text = "```json\n[{\"text\": \"ls -la\", \"description\": \"List all files\"}]\n```";
        let completions = provider.parse_completions(text);
        assert_eq!(completions.len(), 1);
    }
}
