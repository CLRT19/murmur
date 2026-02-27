//! Voice restructuring — converts raw transcripts into shell commands or clean prose.
//!
//! Uses an LLM to interpret the user's spoken intent and produce:
//! - **Command mode:** A valid shell command (e.g., "list all docker containers" → `docker ps -a`)
//! - **Natural mode:** Clean, well-formatted prose (for commit messages, comments, etc.)

use murmur_protocol::VoiceMode;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::VoiceError;

/// Voice restructurer that uses an LLM to convert transcripts.
pub struct VoiceRestructurer {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
}

#[derive(Serialize)]
struct RestructureRequest {
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
struct RestructureResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

impl VoiceRestructurer {
    /// Create a new restructurer using an Anthropic-compatible API.
    pub fn new(api_key: String, model: Option<String>, endpoint: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model: model.unwrap_or_else(|| "claude-haiku-4-5-20251001".to_string()),
            endpoint: endpoint
                .unwrap_or_else(|| "https://api.anthropic.com/v1/messages".to_string()),
        }
    }

    /// Convert a voice transcript into structured output based on mode.
    pub async fn restructure(
        &self,
        transcript: &str,
        mode: &VoiceMode,
        cwd: &str,
        shell: Option<&str>,
    ) -> Result<String, VoiceError> {
        let system = self.build_system_prompt(mode, cwd, shell);
        let user_msg = self.build_user_prompt(transcript, mode);

        debug!(
            mode = ?mode,
            transcript = %transcript,
            "Restructuring voice transcript"
        );

        let body = RestructureRequest {
            model: self.model.clone(),
            max_tokens: 256,
            messages: vec![Message {
                role: "user".to_string(),
                content: user_msg,
            }],
            system,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .timeout(std::time::Duration::from_secs(10))
            .json(&body)
            .send()
            .await
            .map_err(|e| VoiceError::RestructureError(format!("LLM request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(VoiceError::RestructureError(format!(
                "LLM API error {status}: {message}"
            )));
        }

        let api_response: RestructureResponse = response.json().await.map_err(|e| {
            VoiceError::RestructureError(format!("Failed to parse LLM response: {e}"))
        })?;

        let output = api_response
            .content
            .first()
            .map(|b| b.text.trim().to_string())
            .unwrap_or_default();

        if output.is_empty() {
            warn!("LLM returned empty restructured output");
            return Ok(transcript.to_string());
        }

        debug!(output = %output, "Restructuring complete");
        Ok(output)
    }

    fn build_system_prompt(&self, mode: &VoiceMode, cwd: &str, shell: Option<&str>) -> String {
        match mode {
            VoiceMode::Command => {
                let shell_name = shell.unwrap_or("bash");
                format!(
                    "You are a voice-to-command translator. The user spoke a command using their voice. \
                     Convert their spoken intent into a valid {shell_name} shell command.\n\n\
                     Rules:\n\
                     - Output ONLY the shell command, nothing else\n\
                     - No explanations, no markdown, no backticks\n\
                     - If the intent is ambiguous, pick the most common interpretation\n\
                     - Use standard flags and options\n\
                     - The current directory is: {cwd}\n\n\
                     Examples:\n\
                     \"list all docker containers\" → docker ps -a\n\
                     \"find all python files modified today\" → find . -name '*.py' -mtime 0\n\
                     \"show git log last five commits\" → git log --oneline -5\n\
                     \"kill process on port 3000\" → lsof -ti:3000 | xargs kill\n\
                     \"make a new directory called tests\" → mkdir tests"
                )
            }
            VoiceMode::Natural => {
                "You are a voice-to-text assistant. The user spoke naturally. \
                 Clean up their speech into well-formatted text.\n\n\
                 Rules:\n\
                 - Fix grammar and punctuation\n\
                 - Remove filler words (um, uh, like, you know)\n\
                 - Preserve the original meaning and tone\n\
                 - Output ONLY the cleaned text, nothing else\n\
                 - Keep it concise\n\n\
                 Examples:\n\
                 \"um fix the uh authentication bug in the login flow\" → Fix the authentication bug in the login flow\n\
                 \"so basically we need to add like a retry mechanism for failed API calls\" → Add a retry mechanism for failed API calls"
                    .to_string()
            }
        }
    }

    fn build_user_prompt(&self, transcript: &str, _mode: &VoiceMode) -> String {
        transcript.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_system_prompt_includes_shell() {
        let restructurer = VoiceRestructurer::new("test".to_string(), None, None);
        let prompt =
            restructurer.build_system_prompt(&VoiceMode::Command, "/home/user", Some("zsh"));
        assert!(prompt.contains("zsh"));
        assert!(prompt.contains("/home/user"));
    }

    #[test]
    fn natural_system_prompt_has_examples() {
        let restructurer = VoiceRestructurer::new("test".to_string(), None, None);
        let prompt = restructurer.build_system_prompt(&VoiceMode::Natural, "/home/user", None);
        assert!(prompt.contains("filler words"));
    }
}
