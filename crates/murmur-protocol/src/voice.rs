use serde::{Deserialize, Serialize};

/// Voice input mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMode {
    /// Convert speech to a shell command.
    Command,
    /// Convert speech to clean prose (for commit messages, comments, etc.).
    Natural,
}

/// Request to start voice capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceStartRequest {
    /// Which mode to use for restructuring.
    pub mode: VoiceMode,
    /// Current working directory (for context).
    pub cwd: String,
    /// Shell type.
    #[serde(default)]
    pub shell: Option<String>,
}

/// Request to process audio data through STT + restructuring.
/// Audio should be base64-encoded WAV (16kHz mono 16-bit PCM).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceProcessRequest {
    /// Base64-encoded WAV audio data.
    pub audio_data: String,
    /// Which mode to use for restructuring.
    pub mode: VoiceMode,
    /// Current working directory (for context).
    pub cwd: String,
    /// Shell type.
    #[serde(default)]
    pub shell: Option<String>,
}

/// Response after voice capture and processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceResult {
    /// Raw transcript from speech-to-text.
    pub transcript: String,
    /// Restructured output (shell command or clean prose).
    pub output: String,
    /// Voice mode used.
    pub mode: VoiceMode,
    /// Confidence score from STT (0.0 to 1.0).
    pub confidence: f64,
    /// STT engine used.
    pub engine: String,
    /// Total processing time in milliseconds.
    pub latency_ms: u64,
}

/// Status of the voice engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceStatus {
    /// Whether voice is currently capturing.
    pub capturing: bool,
    /// Available STT engines.
    pub available_engines: Vec<String>,
    /// Currently active engine.
    pub active_engine: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_mode_serialization() {
        let mode = VoiceMode::Command;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"command\"");

        let mode: VoiceMode = serde_json::from_str("\"natural\"").unwrap();
        assert_eq!(mode, VoiceMode::Natural);
    }

    #[test]
    fn voice_result_roundtrip() {
        let result = VoiceResult {
            transcript: "list all docker containers".to_string(),
            output: "docker ps -a".to_string(),
            mode: VoiceMode::Command,
            confidence: 0.92,
            engine: "whisper".to_string(),
            latency_ms: 450,
        };
        let json = serde_json::to_string(&result).unwrap();
        let roundtrip: VoiceResult = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.output, "docker ps -a");
    }
}
