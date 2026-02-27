use async_trait::async_trait;
use murmur_context::ShellContext;
use murmur_protocol::{CompletionItem, CompletionKind, CompletionRequest};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::{Provider, ProviderConfig, ProviderError};

const DEFAULT_MODEL: &str = "codestral-latest";
const DEFAULT_ENDPOINT: &str = "https://codestral.mistral.ai/v1/fim/completions";

pub struct CodestralProvider {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
    timeout: std::time::Duration,
}

#[derive(Serialize)]
struct FimRequest {
    model: String,
    prompt: String,
    suffix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Deserialize)]
struct FimResponse {
    choices: Vec<FimChoice>,
}

#[derive(Deserialize)]
struct FimChoice {
    message: FimMessage,
}

#[derive(Deserialize)]
struct FimMessage {
    content: String,
}

impl CodestralProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, ProviderError> {
        let api_key = config
            .api_key
            .clone()
            .ok_or_else(|| ProviderError::NotConfigured("codestral: api_key required".into()))?;

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

    /// Build FIM prompt from command context.
    /// For shell commands, the "prefix" is what the user typed and the "suffix" is empty.
    /// For code-like inputs, we can provide surrounding context.
    fn build_prompt(
        &self,
        request: &CompletionRequest,
        context: &ShellContext,
    ) -> (String, String) {
        let shell = request.shell.as_deref().unwrap_or("bash");
        let mut prefix = String::new();

        // Add context as comments
        prefix.push_str(&format!("# Shell: {shell}\n"));
        prefix.push_str(&format!("# CWD: {}\n", request.cwd));

        if let Some(ref git) = context.git {
            prefix.push_str(&format!("# Git branch: {}\n", git.branch));
        }

        if let Some(ref project) = context.project {
            prefix.push_str(&format!("# Project: {project:?}\n"));
        }

        // Add recent history as context
        if !context.history.is_empty() {
            prefix.push_str("# Recent commands:\n");
            for cmd in context.history.iter().rev().take(5) {
                prefix.push_str(&format!("# $ {cmd}\n"));
            }
        }

        prefix.push_str("$ ");
        prefix.push_str(&request.input);

        // Suffix is empty — we want completions after the cursor
        let suffix = String::from("\n");

        (prefix, suffix)
    }

    fn parse_fim_completions(&self, text: &str, input: &str) -> Vec<CompletionItem> {
        // FIM returns the completion text (what comes after the cursor)
        let completion = text.trim();
        if completion.is_empty() {
            return vec![];
        }

        // Split by newlines — each line could be a separate command suggestion
        let mut items = Vec::new();
        let full_command = format!("{}{}", input, completion.lines().next().unwrap_or(""));

        if !full_command.trim().is_empty() && full_command != input {
            items.push(CompletionItem {
                text: full_command.trim().to_string(),
                description: Some("Code completion (Codestral)".to_string()),
                kind: CompletionKind::Code,
                score: 1.0,
            });
        }

        // If there are multiple lines, add them as additional suggestions
        for (i, line) in completion.lines().skip(1).take(4).enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Strip the `$ ` prefix if present
            let cmd = line.strip_prefix("$ ").unwrap_or(line);
            if !cmd.is_empty() {
                items.push(CompletionItem {
                    text: cmd.to_string(),
                    description: Some("Follow-up command (Codestral)".to_string()),
                    kind: CompletionKind::Code,
                    score: 0.8 - (i as f64 * 0.1),
                });
            }
        }

        items
    }
}

#[async_trait]
impl Provider for CodestralProvider {
    fn name(&self) -> &str {
        "codestral"
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
        context: &ShellContext,
    ) -> Result<Vec<CompletionItem>, ProviderError> {
        let (prompt, suffix) = self.build_prompt(request, context);

        debug!(model = %self.model, input = %request.input, "Requesting FIM completion from Codestral");

        let body = FimRequest {
            model: self.model.clone(),
            prompt,
            suffix,
            stop: Some(vec!["\n\n".to_string(), "$ ".to_string()]),
            max_tokens: 256,
            temperature: 0.2,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
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

        let api_response: FimResponse = response.json().await?;
        let text = api_response
            .choices
            .first()
            .map(|c| c.message.content.as_str())
            .unwrap_or("");

        Ok(self.parse_fim_completions(text, &request.input))
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        debug!("Codestral health check");
        // Simple check — verify the API key works by making a minimal request
        let body = FimRequest {
            model: self.model.clone(),
            prompt: "echo ".to_string(),
            suffix: "\n".to_string(),
            stop: Some(vec!["\n".to_string()]),
            max_tokens: 1,
            temperature: 0.0,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .timeout(std::time::Duration::from_secs(5))
            .json(&body)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(ProviderError::Api {
                status: response.status().as_u16(),
                message: "Health check failed".to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider() -> CodestralProvider {
        CodestralProvider {
            client: Client::new(),
            api_key: "test".to_string(),
            model: "test".to_string(),
            endpoint: "test".to_string(),
            timeout: std::time::Duration::from_secs(5),
        }
    }

    #[test]
    fn parse_single_completion() {
        let provider = make_provider();
        let items = provider.parse_fim_completions("ommit -m \"fix: resolve issue\"", "git c");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "git commit -m \"fix: resolve issue\"");
        assert_eq!(items[0].kind, CompletionKind::Code);
    }

    #[test]
    fn parse_multi_line_completion() {
        let provider = make_provider();
        let text = "ommit -m \"fix bug\"\n$ git push origin main\n$ git log --oneline -5";
        let items = provider.parse_fim_completions(text, "git c");
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text, "git commit -m \"fix bug\"");
        assert_eq!(items[1].text, "git push origin main");
        assert_eq!(items[2].text, "git log --oneline -5");
    }

    #[test]
    fn parse_empty_completion() {
        let provider = make_provider();
        let items = provider.parse_fim_completions("", "git");
        assert!(items.is_empty());
    }
}
