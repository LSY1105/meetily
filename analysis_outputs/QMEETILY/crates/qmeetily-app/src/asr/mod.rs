//! ASR (Automatic Speech Recognition) client.
//!
//! Talks to the Python `qmeetily-sidecar-asr` subprocess over HTTP.
//! Wire-compatible with OpenAI's `/v1/audio/transcriptions`.
//!
//! Lifecycle is owned by `commands::start_recording` / `stop_recording`.
//! We do NOT spawn the sidecar in this crate — that's the Tauri command's job,
//! so the binary can be reused between sessions without restart.

pub mod client;

pub use client::{AsrClient, TranscribeChunk, TranscribeResult};

use crate::error::Result;

const DEFAULT_ASR_MODEL: &str = "Qwen/Qwen3-ASR-0.6B";

pub fn build_client(sidecar_url: &str, model: Option<String>) -> Result<AsrClient> {
    AsrClient::new(sidecar_url, model.unwrap_or_else(|| DEFAULT_ASR_MODEL.to_string()))
}
