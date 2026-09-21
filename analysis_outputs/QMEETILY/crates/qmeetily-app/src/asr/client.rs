//! ASR HTTP client — OpenAI-compatible multipart upload.
//!
//! Wire format: POST /v1/audio/transcriptions with `file=@wav` multipart form.
//! Sidecar lives at `QMEETILY_ASR_URL` (default http://127.0.0.1:11436).
//!
//! Streaming ASR is intentionally not implemented in v0.1 — every audio chunk
//! is uploaded as a complete WAV. Qwen3-ASR streaming via the official Python
//! API requires per-utterance state management that's brittle; whole-utterance
//! ASR is a clear v0.1 feature and can be replaced with streaming in v0.2.

use std::time::Duration;

use reqwest::multipart;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct AsrClient {
    base_url: String,
    model: String,
    http: Client,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscribeChunk {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub language_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranscribeResult {
    pub text: String,
    pub language: Option<String>,
    pub confidence: Option<f32>,
}

impl AsrClient {
    pub fn new(base_url: &str, model: String) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| AppError::Other(anyhow::anyhow!("http client: {e}")))?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
            http,
        })
    }

    pub async fn health(&self) -> Result<()> {
        let r = self
            .http
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!("ASR sidecar /health: {e}")))?;
        if !r.status().is_success() {
            return Err(AppError::Other(anyhow::anyhow!(
                "ASR sidecar /health returned {}",
                r.status()
            )));
        }
        Ok(())
    }

    pub async fn transcribe(&self, chunk: &TranscribeChunk) -> Result<TranscribeResult> {
        // Encode f32 PCM as WAV in-memory for multipart upload.
        let wav = pcm_f32_to_wav(&chunk.samples, chunk.sample_rate);

        let mut form = multipart::Form::new().text("model", self.model.clone());
        if let Some(lang) = &chunk.language_hint {
            form = form.text("language", lang.clone());
        }
        form = form.part(
            "file",
            multipart::Part::bytes(wav)
                .file_name("chunk.wav")
                .mime_str("audio/wav")
                .map_err(|e| AppError::Other(anyhow::anyhow!("mime: {e}")))?,
        );

        let resp = self
            .http
            .post(format!("{}/v1/audio/transcriptions", self.base_url))
            .multipart(form)
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!("ASR POST: {e}")))?;

        if !resp.status().is_success() {
            let s = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Other(anyhow::anyhow!(
                "ASR sidecar error {s}: {body}"
            )));
        }

        let result: TranscribeResult = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!("ASR JSON: {e}")))?;
        Ok(result)
    }
}

/// Convert f32 PCM samples to a minimal WAV byte stream (mono).
pub fn pcm_f32_to_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(44 + samples.len() * 2);
    // RIFF header
    buf.extend_from_slice(b"RIFF");
    let data_size = (samples.len() * 2) as u32;
    buf.extend_from_slice(&(36 + data_size).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * 2;
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}
