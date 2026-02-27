//! Murmur Voice — Audio capture, speech-to-text, and voice restructuring.
//!
//! This crate provides:
//! - STT engine abstraction (`SttEngine` trait)
//! - Apple Speech framework integration (macOS, via Swift helper)
//! - Deepgram cloud STT integration
//! - Voice restructuring pipeline (transcript → LLM → command/prose)
//! - Audio utilities for WAV encoding

mod apple;
mod claude_cli;
mod deepgram;
mod restructure;

pub use apple::AppleEngine;
pub use claude_cli::ClaudeCliRestructurer;
pub use deepgram::DeepgramEngine;
pub use restructure::VoiceRestructurer;

use async_trait::async_trait;
use murmur_protocol::{VoiceMode, VoiceResult, VoiceStatus};
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, info, warn};

#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("Voice engine not available: {0}")]
    NotAvailable(String),

    #[error("Audio capture error: {0}")]
    CaptureError(String),

    #[error("STT error: {0}")]
    SttError(String),

    #[error("Restructuring error: {0}")]
    RestructureError(String),

    #[error("Timeout: capture exceeded {0}ms")]
    Timeout(u64),

    #[error("Low confidence: {0:.2} (threshold: {1:.2})")]
    LowConfidence(f64, f64),
}

/// Result from speech-to-text processing.
#[derive(Debug, Clone)]
pub struct SttResult {
    /// The transcribed text.
    pub transcript: String,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f64,
}

/// Trait for speech-to-text engines.
#[async_trait]
pub trait SttEngine: Send + Sync {
    /// Engine name (e.g., "deepgram", "whisper", "apple").
    fn name(&self) -> &str;

    /// Check if this engine is available/configured.
    fn is_available(&self) -> bool;

    /// Transcribe audio data (WAV format, 16kHz mono 16-bit).
    async fn transcribe(&self, audio_data: &[u8]) -> Result<SttResult, VoiceError>;
}

/// Configuration for the voice engine.
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    pub enabled: bool,
    pub engine: String,
    pub language: String,
    pub confidence_threshold: f64,
    pub capture_timeout_ms: u64,
    pub deepgram_api_key: Option<String>,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: "deepgram".to_string(),
            language: "en".to_string(),
            confidence_threshold: 0.5,
            capture_timeout_ms: 30000,
            deepgram_api_key: None,
        }
    }
}

/// Wrapper enum for the two restructurer backends.
pub enum Restructurer {
    /// HTTP API-based restructurer (requires Anthropic API key).
    Api(VoiceRestructurer),
    /// Local Claude CLI-based restructurer (uses `claude -p`).
    ClaudeCli(ClaudeCliRestructurer),
}

/// The main voice engine that coordinates capture, STT, and restructuring.
pub struct VoiceEngine {
    config: VoiceConfig,
    engines: Vec<Box<dyn SttEngine>>,
    restructurer: Option<Restructurer>,
}

impl VoiceEngine {
    pub fn new(config: VoiceConfig) -> Self {
        let mut engines: Vec<Box<dyn SttEngine>> = Vec::new();

        // Initialize Deepgram if API key is available
        if let Some(ref key) = config.deepgram_api_key {
            let engine = DeepgramEngine::new(key.clone(), config.language.clone());
            info!("Deepgram STT engine initialized");
            engines.push(Box::new(engine));
        }

        // Apple Speech engine (macOS only, via swift helper)
        #[cfg(target_os = "macos")]
        {
            let apple = AppleEngine::new(config.language.clone());
            if apple.is_available() {
                info!("Apple Speech STT engine initialized");
                engines.push(Box::new(apple));
            } else {
                debug!("Apple STT helper not found — run swift-helpers/build.sh to enable");
            }
        }

        Self {
            config,
            engines,
            restructurer: None,
        }
    }

    /// Set the voice restructurer backend.
    pub fn set_restructurer(&mut self, restructurer: Restructurer) {
        self.restructurer = Some(restructurer);
    }

    /// Get current voice engine status.
    pub fn status(&self) -> VoiceStatus {
        let available = self.detect_engines();
        let active = self.engines.first().map(|e| e.name().to_string());

        VoiceStatus {
            capturing: false,
            available_engines: available,
            active_engine: active,
        }
    }

    /// Process audio data through STT and restructuring.
    ///
    /// Takes raw WAV audio bytes (16kHz mono 16-bit PCM).
    /// Returns the voice result with transcript and restructured output.
    pub async fn process_audio(
        &self,
        audio_data: &[u8],
        mode: VoiceMode,
        cwd: &str,
        shell: Option<&str>,
    ) -> Result<VoiceResult, VoiceError> {
        if !self.config.enabled {
            return Err(VoiceError::NotAvailable(
                "Voice input is disabled in config".to_string(),
            ));
        }

        let start = Instant::now();

        // Try each STT engine in order
        let stt_result = self.run_stt(audio_data).await?;

        info!(
            engine = stt_result.1,
            transcript = %stt_result.0.transcript,
            confidence = stt_result.0.confidence,
            "STT completed"
        );

        // Check confidence threshold
        if stt_result.0.confidence < self.config.confidence_threshold {
            return Err(VoiceError::LowConfidence(
                stt_result.0.confidence,
                self.config.confidence_threshold,
            ));
        }

        // Restructure the transcript
        let output = match &self.restructurer {
            Some(Restructurer::Api(restructurer)) => {
                debug!(mode = ?mode, "Restructuring transcript via API");
                restructurer
                    .restructure(&stt_result.0.transcript, &mode, cwd, shell)
                    .await?
            }
            Some(Restructurer::ClaudeCli(restructurer)) => {
                debug!(mode = ?mode, "Restructuring transcript via claude CLI");
                restructurer
                    .restructure(&stt_result.0.transcript, &mode, cwd, shell)
                    .await?
            }
            None => {
                // No restructurer — return raw transcript
                stt_result.0.transcript.clone()
            }
        };

        Ok(VoiceResult {
            transcript: stt_result.0.transcript,
            output,
            mode,
            confidence: stt_result.0.confidence,
            engine: stt_result.1,
            latency_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Run STT across available engines with failover.
    async fn run_stt(&self, audio_data: &[u8]) -> Result<(SttResult, String), VoiceError> {
        if self.engines.is_empty() {
            return Err(VoiceError::NotAvailable(
                "No STT engines configured. Set deepgram_api_key in [voice] config.".to_string(),
            ));
        }

        for engine in &self.engines {
            if !engine.is_available() {
                continue;
            }

            match engine.transcribe(audio_data).await {
                Ok(result) => {
                    return Ok((result, engine.name().to_string()));
                }
                Err(e) => {
                    warn!(engine = engine.name(), error = %e, "STT engine failed, trying next");
                }
            }
        }

        Err(VoiceError::SttError("All STT engines failed".to_string()))
    }

    fn detect_engines(&self) -> Vec<String> {
        let mut available = Vec::new();

        // Deepgram (cloud)
        if self.config.deepgram_api_key.is_some() {
            available.push("deepgram".to_string());
        }

        // Whisper (local) — always "available" as an option
        available.push("whisper".to_string());

        // Apple SpeechAnalyzer (macOS only)
        #[cfg(target_os = "macos")]
        {
            available.push("apple".to_string());
        }

        available
    }
}

/// Encode raw PCM audio samples as WAV bytes.
///
/// Useful for converting captured audio to the WAV format expected by STT engines.
pub fn encode_wav(samples: &[i16], sample_rate: u32) -> Result<Vec<u8>, VoiceError> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::new(&mut buffer, spec)
        .map_err(|e| VoiceError::CaptureError(format!("Failed to create WAV writer: {e}")))?;

    for &sample in samples {
        writer
            .write_sample(sample)
            .map_err(|e| VoiceError::CaptureError(format!("Failed to write sample: {e}")))?;
    }

    writer
        .finalize()
        .map_err(|e| VoiceError::CaptureError(format!("Failed to finalize WAV: {e}")))?;

    Ok(buffer.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = VoiceConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.engine, "deepgram");
        assert_eq!(config.confidence_threshold, 0.5);
    }

    #[test]
    fn engine_not_available_when_disabled() {
        let config = VoiceConfig::default();
        let engine = VoiceEngine::new(config);
        let status = engine.status();
        assert!(!status.capturing);
        assert!(status.active_engine.is_none());
    }

    #[test]
    fn detect_engines_includes_whisper() {
        let config = VoiceConfig::default();
        let engine = VoiceEngine::new(config);
        let status = engine.status();
        assert!(status.available_engines.contains(&"whisper".to_string()));
    }

    #[test]
    fn encode_wav_produces_valid_output() {
        // Generate a simple sine wave
        let sample_rate = 16000;
        let samples: Vec<i16> = (0..sample_rate)
            .map(|i| {
                let t = i as f64 / sample_rate as f64;
                (f64::sin(2.0 * std::f64::consts::PI * 440.0 * t) * 16000.0) as i16
            })
            .collect();

        let wav = encode_wav(&samples, sample_rate as u32).unwrap();

        // WAV files start with "RIFF"
        assert_eq!(&wav[0..4], b"RIFF");
        // Should have reasonable size (header + samples)
        assert!(wav.len() > 44); // WAV header is 44 bytes
    }

    #[tokio::test]
    async fn process_audio_fails_when_disabled() {
        let config = VoiceConfig::default(); // enabled = false
        let engine = VoiceEngine::new(config);
        let result = engine
            .process_audio(b"fake audio", VoiceMode::Command, "/tmp", None)
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("disabled in config"));
    }

    #[tokio::test]
    async fn process_audio_fails_without_stt_engine() {
        let config = VoiceConfig {
            enabled: true,
            ..VoiceConfig::default()
        };
        let engine = VoiceEngine::new(config);
        let result = engine
            .process_audio(b"fake audio", VoiceMode::Command, "/tmp", None)
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No STT engines configured"));
    }
}
