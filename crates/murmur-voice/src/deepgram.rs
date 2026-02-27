//! Deepgram cloud speech-to-text engine.
//!
//! Uses Deepgram's REST API for transcription. Requires an API key.
//! Docs: https://developers.deepgram.com/reference/listen-file

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tracing::debug;

use crate::{SttEngine, SttResult, VoiceError};

const DEEPGRAM_API_URL: &str = "https://api.deepgram.com/v1/listen";

pub struct DeepgramEngine {
    client: Client,
    api_key: String,
    language: String,
}

#[derive(Deserialize)]
struct DeepgramResponse {
    results: DeepgramResults,
}

#[derive(Deserialize)]
struct DeepgramResults {
    channels: Vec<DeepgramChannel>,
}

#[derive(Deserialize)]
struct DeepgramChannel {
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Deserialize)]
struct DeepgramAlternative {
    transcript: String,
    confidence: f64,
}

impl DeepgramEngine {
    pub fn new(api_key: String, language: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            language,
        }
    }
}

#[async_trait]
impl SttEngine for DeepgramEngine {
    fn name(&self) -> &str {
        "deepgram"
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn transcribe(&self, audio_data: &[u8]) -> Result<SttResult, VoiceError> {
        debug!(
            audio_bytes = audio_data.len(),
            language = %self.language,
            "Sending audio to Deepgram"
        );

        let url = format!(
            "{}?model=nova-2&language={}&punctuate=true&smart_format=true",
            DEEPGRAM_API_URL, self.language
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Token {}", self.api_key))
            .header("Content-Type", "audio/wav")
            .timeout(std::time::Duration::from_secs(30))
            .body(audio_data.to_vec())
            .send()
            .await
            .map_err(|e| VoiceError::SttError(format!("Deepgram request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(VoiceError::SttError(format!(
                "Deepgram API error {status}: {body}"
            )));
        }

        let result: DeepgramResponse = response
            .json()
            .await
            .map_err(|e| VoiceError::SttError(format!("Failed to parse Deepgram response: {e}")))?;

        let alternative = result
            .results
            .channels
            .first()
            .and_then(|c| c.alternatives.first())
            .ok_or_else(|| VoiceError::SttError("No transcription results".to_string()))?;

        debug!(
            transcript = %alternative.transcript,
            confidence = alternative.confidence,
            "Deepgram transcription complete"
        );

        Ok(SttResult {
            transcript: alternative.transcript.clone(),
            confidence: alternative.confidence,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_name() {
        let engine = DeepgramEngine::new("test-key".to_string(), "en".to_string());
        assert_eq!(engine.name(), "deepgram");
    }

    #[test]
    fn not_available_without_key() {
        let engine = DeepgramEngine::new(String::new(), "en".to_string());
        assert!(!engine.is_available());
    }

    #[test]
    fn available_with_key() {
        let engine = DeepgramEngine::new("dg-test-key".to_string(), "en".to_string());
        assert!(engine.is_available());
    }
}
