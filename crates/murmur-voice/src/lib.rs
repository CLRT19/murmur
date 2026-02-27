//! Murmur Voice — Audio capture, speech-to-text, and voice restructuring.
//!
//! This crate will integrate:
//! - `cpal` for audio capture
//! - `whisper-rs` for local STT
//! - Apple SpeechAnalyzer via Swift helper (macOS)
//! - Deepgram for cloud STT
//!
//! Phase 3 implementation — stubbed for now.

use murmur_protocol::{VoiceMode, VoiceResult, VoiceStatus};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("Voice engine not available: {0}")]
    NotAvailable(String),

    #[error("Audio capture error: {0}")]
    CaptureError(String),

    #[error("STT error: {0}")]
    SttError(String),
}

/// Voice engine state.
pub struct VoiceEngine {
    _enabled: bool,
}

impl VoiceEngine {
    pub fn new(enabled: bool) -> Self {
        Self { _enabled: enabled }
    }

    /// Get current voice engine status.
    pub fn status(&self) -> VoiceStatus {
        VoiceStatus {
            capturing: false,
            available_engines: self.detect_engines(),
            active_engine: None,
        }
    }

    /// Start voice capture (stub).
    pub async fn start_capture(
        &self,
        _mode: VoiceMode,
        _cwd: &str,
    ) -> Result<VoiceResult, VoiceError> {
        Err(VoiceError::NotAvailable(
            "Voice input not yet implemented (Phase 3)".to_string(),
        ))
    }

    fn detect_engines(&self) -> Vec<String> {
        let mut engines = vec!["whisper".to_string()];

        #[cfg(target_os = "macos")]
        engines.push("apple".to_string());

        engines
    }
}
