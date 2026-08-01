//! SherpaOnnx streaming speech recognition engine module.
//!
//! Wraps the k2-fsa `sherpa-onnx` crate to provide real-time streaming
//! Chinese+English ASR via the `OnlineRecognizer` transducer API. The engine
//! is a peer of `parakeet_engine` and `whisper_engine`; the transcription
//! worker can select between them via the `provider` field in
//! `TranscriptConfig`.
//!
//! # Module structure
//!
//! - `engine` — wraps `OnlineRecognizer`, exposes `accept_waveform` / `reset` /
//!   `is_endpoint` so the worker can do live incremental decoding.
//! - `model` — catalog, download URL, expected file names, status types.
//! - `commands` — Tauri command surface for the frontend.

pub mod engine;
pub mod model;
pub mod commands;

pub use engine::{
    DownloadProgress, SherpaEngine, SherpaEngineError, StreamingResult,
};
pub use model::{SherpaModelInfo, SherpaModelStatus, DEFAULT_SHERPA_MODEL};
pub use commands::*;
