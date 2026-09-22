//! ASR (Automatic Speech Recognition) client.
//!
//! Talks to the Python `qmeetily-sidecar-asr` subprocess over HTTP.
//! Wire-compatible with OpenAI's `/v1/audio/transcriptions`.
//!
//! The sidecar process is spawned by `AsrSidecar::start` during app
//! startup and reused across sessions; the HTTP client built here just
//! dials whatever URL the lifecycle manager exposes.

pub mod client;
pub mod sidecar;

pub use client::{AsrClient, TranscribeChunk, TranscribeResult};
pub use sidecar::AsrSidecar;

use crate::error::Result;

const DEFAULT_ASR_MODEL: &str = "Qwen/Qwen3-ASR-0.6B";

pub fn build_client(sidecar_url: &str, model: Option<String>) -> Result<AsrClient> {
    AsrClient::new(sidecar_url, model.unwrap_or_else(|| DEFAULT_ASR_MODEL.to_string()))
}
