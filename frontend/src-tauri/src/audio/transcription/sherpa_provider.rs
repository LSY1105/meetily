// audio/transcription/sherpa_provider.rs
//
// Sherpa-onnx streaming transcription provider implementation.

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use log::warn;
use std::sync::Arc;

/// Sherpa-onnx streaming transcription provider (wraps `SherpaEngine`).
///
/// Uses the engine's `accept_waveform` API under the hood so each call to
/// `transcribe` returns the latest partial result for the chunk. The worker
/// tags the result as `is_partial = true`; final-commit detection happens
/// when `is_endpoint` fires inside the engine and the worker calls
/// `engine.reset()`.
pub struct SherpaProvider {
    engine: Arc<crate::sherpa_engine::SherpaEngine>,
}

impl SherpaProvider {
    pub fn new(engine: Arc<crate::sherpa_engine::SherpaEngine>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl TranscriptionProvider for SherpaProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        // Sherpa bilingual zipformer handles zh/en internally; warn but accept
        // any explicit language hint.
        if let Some(ref lang) = language {
            warn!(
                "Sherpa bilingual model auto-detects language; ignoring hint '{}'",
                lang
            );
        }

        // The audio is 16kHz mono (the worker already resamples to 16k).
        let result = self
            .engine
            .accept_waveform(audio, 16_000)
            .await
            .map_err(|e| TranscriptionError::EngineFailed(e.to_string()))?;

        // If this chunk hit an endpoint, commit and reset for the next utterance.
        // The worker treats `is_endpoint` as a hint to flush; we still return
        // the latest text so the caller can emit a final segment.
        if result.is_endpoint {
            if let Err(e) = self.engine.reset().await {
                warn!("sherpa reset after endpoint failed: {}", e);
            }
        }

        Ok(TranscriptResult {
            text: result.text,
            confidence: None, // Streaming zipformer doesn't expose a confidence score.
            is_partial: result.is_partial,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        self.engine.is_model_loaded().await
    }

    async fn get_current_model(&self) -> Option<String> {
        self.engine.get_current_model().await
    }

    fn provider_name(&self) -> &'static str {
        "SherpaOnnx"
    }
}
