use async_trait::async_trait;
use murmur_context::ShellContext;
use murmur_protocol::{CompletionItem, CompletionKind, CompletionRequest};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::{Provider, ProviderConfig, ProviderError};

const DEFAULT_MODEL: &str = "codellama:7b";
const DEFAULT_ENDPOINT: &str = "http://localhost:11434";

pub struct OllamaProvider {
    client: Client,
    model: String,
    endpoint: String,
    timeout: std::time::Duration,
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

impl OllamaProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, ProviderError> {
        Ok(Self {
            client: Client::new(),
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

    fn build_prompt(&self, request: &CompletionRequest, context: &ShellContext) -> String {
        let mut prompt = format!(
            "You are a shell command autocomplete engine.\n\
             Shell: {}\n\
             CWD: {}\n\
             Partial command: `{}`\n\n\
             Suggest up to 5 completions as a JSON array of objects with \"text\" and \"description\" fields.\n\
             Respond ONLY with the JSON array, no other text.",
            request.shell.as_deref().unwrap_or("unknown"),
            request.cwd,
            request.input,
        );

        if !context.history.is_empty() {
            prompt.push_str("\nRecent history:\n");
            for cmd in context.history.iter().rev().take(5) {
                prompt.push_str(&format!("  {cmd}\n"));
            }
        }

        prompt
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
        context: &ShellContext,
    ) -> Result<Vec<CompletionItem>, ProviderError> {
        let prompt = self.build_prompt(request, context);

        debug!(model = %self.model, input = %request.input, "Requesting completion from Ollama");

        let url = format!("{}/api/generate", self.endpoint);
        let body = OllamaRequest {
            model: self.model.clone(),
            prompt,
            stream: false,
        };

        let response = self
            .client
            .post(&url)
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

        let api_response: OllamaResponse = response.json().await?;

        // Parse response same way as Anthropic
        #[derive(Deserialize)]
        struct Suggestion {
            text: String,
            description: Option<String>,
        }

        let json_str = api_response
            .response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        match serde_json::from_str::<Vec<Suggestion>>(json_str) {
            Ok(suggestions) => Ok(suggestions
                .into_iter()
                .enumerate()
                .map(|(i, s)| CompletionItem {
                    text: s.text,
                    description: s.description,
                    kind: CompletionKind::FullCommand,
                    score: 1.0 - (i as f64 * 0.1),
                })
                .collect()),
            Err(e) => {
                warn!(error = %e, "Failed to parse Ollama response");
                Ok(vec![])
            }
        }
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        let url = format!("{}/api/tags", self.endpoint);
        self.client
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await?;
        Ok(())
    }
}
