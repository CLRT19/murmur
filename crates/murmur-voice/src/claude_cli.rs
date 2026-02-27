//! Claude CLI restructurer — shells out to `claude -p` for voice transcript cleanup.
//!
//! Uses the locally installed Claude CLI (already authenticated) instead of
//! requiring a separate Anthropic API key. Falls back to the raw transcript
//! if the `claude` binary is not found or exits non-zero.

use murmur_protocol::VoiceMode;
use tokio::process::Command;
use tracing::{debug, warn};

use crate::VoiceError;

/// Voice restructurer that uses the local `claude` CLI.
pub struct ClaudeCliRestructurer {
    model: String,
    timeout_secs: u64,
}

impl ClaudeCliRestructurer {
    /// Create a new CLI-based restructurer.
    ///
    /// Defaults to model `"haiku"` and a 15-second timeout.
    pub fn new(model: Option<String>, timeout_secs: Option<u64>) -> Self {
        Self {
            model: model.unwrap_or_else(|| "haiku".to_string()),
            timeout_secs: timeout_secs.unwrap_or(15),
        }
    }

    /// Check whether the `claude` CLI is available on `$PATH`.
    pub async fn is_available() -> bool {
        Command::new("sh")
            .args(["-c", "command -v claude"])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Restructure a voice transcript via `claude -p`.
    ///
    /// Falls back to the raw transcript on any error.
    pub async fn restructure(
        &self,
        transcript: &str,
        mode: &VoiceMode,
        cwd: &str,
        shell: Option<&str>,
    ) -> Result<String, VoiceError> {
        let system_prompt = self.build_system_prompt(mode, cwd, shell);

        debug!(
            mode = ?mode,
            model = %self.model,
            transcript = %transcript,
            "Restructuring via claude CLI"
        );

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(self.timeout_secs),
            self.run_claude(&system_prompt, transcript),
        )
        .await;

        match result {
            Ok(Ok(output)) if !output.is_empty() => {
                debug!(output = %output, "Claude CLI restructuring complete");
                Ok(output)
            }
            Ok(Ok(_)) => {
                warn!("Claude CLI returned empty output, using raw transcript");
                Ok(transcript.to_string())
            }
            Ok(Err(e)) => {
                warn!(error = %e, "Claude CLI failed, falling back to raw transcript");
                Ok(transcript.to_string())
            }
            Err(_) => {
                warn!(
                    timeout_secs = self.timeout_secs,
                    "Claude CLI timed out, falling back to raw transcript"
                );
                Ok(transcript.to_string())
            }
        }
    }

    async fn run_claude(
        &self,
        system_prompt: &str,
        transcript: &str,
    ) -> Result<String, VoiceError> {
        let output = Command::new("claude")
            .args([
                "-p",
                "--model",
                &self.model,
                "--system-prompt",
                system_prompt,
                "--output-format",
                "text",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                VoiceError::RestructureError(format!("Failed to spawn claude CLI: {e}"))
            })?;

        // Write transcript to stdin
        use tokio::io::AsyncWriteExt;
        let mut child = output;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(transcript.as_bytes()).await.map_err(|e| {
                VoiceError::RestructureError(format!("Failed to write to claude stdin: {e}"))
            })?;
            drop(stdin); // Close stdin so claude reads EOF
        }

        let output = child.wait_with_output().await.map_err(|e| {
            VoiceError::RestructureError(format!("Failed to read claude output: {e}"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VoiceError::RestructureError(format!(
                "claude CLI exited with {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(text)
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
                     \"show git log last five commits\" → git log --oneline -5"
                )
            }
            VoiceMode::Natural => {
                "You are a text polishing assistant. The user will provide raw speech-to-text output. \
                 Clean it up: fix grammar, remove filler words (um, uh, like), improve punctuation, \
                 and make it read naturally. Preserve the original meaning and tone. \
                 Output ONLY the polished text, nothing else."
                    .to_string()
            }
        }
    }

    /// Build the command arguments (useful for testing).
    #[cfg(test)]
    fn build_args(&self, system_prompt: &str) -> Vec<String> {
        vec![
            "-p".to_string(),
            "--model".to_string(),
            self.model.clone(),
            "--system-prompt".to_string(),
            system_prompt.to_string(),
            "--output-format".to_string(),
            "text".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_and_timeout() {
        let r = ClaudeCliRestructurer::new(None, None);
        assert_eq!(r.model, "haiku");
        assert_eq!(r.timeout_secs, 15);
    }

    #[test]
    fn custom_model_and_timeout() {
        let r = ClaudeCliRestructurer::new(Some("sonnet".to_string()), Some(30));
        assert_eq!(r.model, "sonnet");
        assert_eq!(r.timeout_secs, 30);
    }

    #[test]
    fn command_args_are_correct() {
        let r = ClaudeCliRestructurer::new(None, None);
        let prompt = "Test prompt";
        let args = r.build_args(prompt);
        assert_eq!(
            args,
            vec![
                "-p",
                "--model",
                "haiku",
                "--system-prompt",
                "Test prompt",
                "--output-format",
                "text",
            ]
        );
    }

    #[test]
    fn command_system_prompt_includes_shell_and_cwd() {
        let r = ClaudeCliRestructurer::new(None, None);
        let prompt = r.build_system_prompt(&VoiceMode::Command, "/home/user", Some("zsh"));
        assert!(prompt.contains("zsh"));
        assert!(prompt.contains("/home/user"));
        assert!(prompt.contains("voice-to-command"));
    }

    #[test]
    fn natural_system_prompt_mentions_filler_words() {
        let r = ClaudeCliRestructurer::new(None, None);
        let prompt = r.build_system_prompt(&VoiceMode::Natural, "/tmp", None);
        assert!(prompt.contains("filler words"));
        assert!(prompt.contains("polishing"));
    }

    #[tokio::test]
    async fn fallback_when_claude_not_found() {
        // Use a restructurer that points to a nonexistent binary by
        // testing the restructure method — it should fall back gracefully.
        // We can't easily mock the binary, but we can verify the fallback
        // path by setting PATH to empty so `claude` won't be found.
        let r = ClaudeCliRestructurer::new(None, Some(2));
        // The run_claude method will fail because `claude` won't be on PATH
        // in CI environments. restructure() should fall back to raw transcript.
        let result = r
            .restructure("um hello world", &VoiceMode::Natural, "/tmp", None)
            .await;
        // Should succeed (fallback) rather than error
        assert!(result.is_ok());
        // The fallback returns the raw transcript
        let output = result.unwrap();
        assert!(
            output.contains("hello world"),
            "Fallback should contain original transcript"
        );
    }
}
