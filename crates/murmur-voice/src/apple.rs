use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, warn};

use crate::{encode_wav, SttEngine, SttResult, VoiceError};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Apple Speech framework STT engine (macOS only).
///
/// Invokes the `murmur-transcribe` Swift helper binary which uses
/// SFSpeechRecognizer for on-device speech recognition. The helper
/// reads a WAV file and outputs JSON: `{"transcript": "...", "confidence": 0.95}`
pub struct AppleEngine {
    language: String,
    helper_path: Option<String>,
}

impl AppleEngine {
    pub fn new(language: String) -> Self {
        // Try to find the helper binary
        let helper_path = find_helper_binary();
        if helper_path.is_some() {
            debug!("Apple STT helper found");
        } else {
            debug!("Apple STT helper not found (run swift-helpers/build.sh to build)");
        }
        Self {
            language,
            helper_path,
        }
    }
}

/// Search for the murmur-transcribe binary in common locations.
fn find_helper_binary() -> Option<String> {
    let candidates = [
        // Next to the murmur binary
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("murmur-transcribe")))
            .map(|p| p.to_string_lossy().to_string()),
        // In the swift-helpers directory (development)
        Some("swift-helpers/murmur-transcribe".to_string()),
        // In PATH
        which_murmur_transcribe(),
    ];

    candidates
        .into_iter()
        .flatten()
        .find(|candidate| std::path::Path::new(candidate).exists())
}

fn which_murmur_transcribe() -> Option<String> {
    std::process::Command::new("which")
        .arg("murmur-transcribe")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[async_trait]
impl SttEngine for AppleEngine {
    fn name(&self) -> &str {
        "apple"
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos") && self.helper_path.is_some()
    }

    async fn transcribe(&self, audio_data: &[u8]) -> Result<SttResult, VoiceError> {
        let helper = self.helper_path.as_ref().ok_or_else(|| {
            VoiceError::NotAvailable(
                "Apple STT helper not found. Run: cd swift-helpers && ./build.sh".to_string(),
            )
        })?;

        // Write audio data to a temp file (unique per concurrent call)
        let tmp_dir = std::env::temp_dir();
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_path = tmp_dir.join(format!("murmur-stt-{}-{}.wav", std::process::id(), counter));
        let tmp_path_str = tmp_path.to_string_lossy().to_string();

        // If the data doesn't start with RIFF header, encode it as WAV
        let wav_data = if audio_data.len() >= 4 && &audio_data[..4] == b"RIFF" {
            audio_data.to_vec()
        } else {
            // Assume raw 16-bit PCM samples at 16kHz
            let samples: Vec<i16> = audio_data
                .chunks_exact(2)
                .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            encode_wav(&samples, 16000)?
        };

        std::fs::write(&tmp_path, &wav_data)
            .map_err(|e| VoiceError::SttError(format!("Failed to write temp audio file: {e}")))?;

        // Spawn the helper
        debug!(helper = %helper, audio_file = %tmp_path_str, "Invoking Apple STT helper");

        let output = tokio::process::Command::new(helper)
            .arg(&tmp_path_str)
            .arg("--language")
            .arg(&self.language)
            .output()
            .await;

        // Always clean up temp file, even on error
        let _ = std::fs::remove_file(&tmp_path);

        let output = output
            .map_err(|e| VoiceError::SttError(format!("Failed to run Apple STT helper: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Try to parse error from stdout JSON
            if let Ok(err) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if let Some(msg) = err.get("error").and_then(|e| e.as_str()) {
                    return Err(VoiceError::SttError(format!("Apple STT: {msg}")));
                }
            }
            warn!(stderr = %stderr, exit_code = ?output.status.code(), "Apple STT helper failed");
            return Err(VoiceError::SttError(format!(
                "Apple STT helper exited with code {:?}: {}",
                output.status.code(),
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| VoiceError::SttError(format!("Failed to parse Apple STT output: {e}")))?;

        let transcript = result["transcript"].as_str().unwrap_or("").to_string();
        let confidence = result["confidence"].as_f64().unwrap_or(0.0);

        Ok(SttResult {
            transcript,
            confidence,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_name() {
        let engine = AppleEngine::new("en-US".to_string());
        assert_eq!(engine.name(), "apple");
    }

    #[test]
    fn availability_depends_on_helper() {
        let engine = AppleEngine {
            language: "en-US".to_string(),
            helper_path: None,
        };
        // Without helper, should not be available (even on macOS)
        assert!(!engine.is_available());
    }

    #[test]
    fn availability_with_helper() {
        let engine = AppleEngine {
            language: "en-US".to_string(),
            helper_path: Some("/usr/bin/true".to_string()),
        };
        if cfg!(target_os = "macos") {
            assert!(engine.is_available());
        } else {
            assert!(!engine.is_available());
        }
    }
}
