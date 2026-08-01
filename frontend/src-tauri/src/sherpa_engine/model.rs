//! SherpaOnnx model catalog and download metadata.
//!
//! Mirrors the layout used by `parakeet_engine::model`. The actual ONNX model
//! itself is loaded by `engine::SherpaEngine` via the `sherpa-onnx` crate;
//! this module only holds the URL, expected files, and size constants used by
//! the discovery / download code in `engine.rs` and `commands.rs`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Default streaming-zipformer bilingual Chinese+English model.
///
/// This is the de-facto k2-fsa streaming model that supports incremental
/// decoding (OnlineRecognizer) for real-time Chinese ASR. ~350 MB.
pub const DEFAULT_SHERPA_MODEL: &str = "sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20";

/// Asset filename for the bilingual zipformer model.
pub const SHERPA_BILINGUAL_ASSET: &str =
    "sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2";

/// Download URL for the bilingual zipformer model.
pub const SHERPA_BILINGUAL_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2";

/// Approximate on-disk size in MB (after extraction; the tarball is ~488 MB).
pub const SHERPA_BILINGUAL_SIZE_MB: u64 = 350;

/// Required files after extraction. The 2023-02-20 zipformer bilingual model
/// uses the standard k2-fsa file layout (see sherpa-onnx repo).
pub const SHERPA_BILINGUAL_FILES: &[&str] = &[
    "encoder-epoch-99-avg-1.onnx",
    "decoder-epoch-99-avg-1.onnx",
    "joiner-epoch-99-avg-1.onnx",
    "tokens.txt",
];

/// ONNX file presence (any of these can be int8 or fp32 — the bilingual
/// 2023-02-20 release ships fp32 onnx files; int8 variants are common too).
pub fn is_valid_model_dir(model_dir: &PathBuf) -> bool {
    SHERPA_BILINGUAL_FILES
        .iter()
        .all(|f| model_dir.join(f).exists())
}

/// Information about a sherpa-onnx model exposed to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SherpaModelInfo {
    pub name: String,
    pub path: PathBuf,
    pub size_mb: u32,
    pub status: SherpaModelStatus,
    pub description: String,
    pub streaming: bool,
}

/// Mirrors `parakeet_engine::ModelStatus` — kept as a separate type so the
/// sherpa engine can evolve independently of the parakeet/whisper catalogs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SherpaModelStatus {
    Available,
    Missing,
    Downloading { progress: u8 },
    Error(String),
    Corrupted { file_size: u64, expected_min_size: u64 },
}

/// Catalog entry used by `SherpaEngine::discover_models`.
/// Format: (name, size_mb, description)
pub const SHERPA_MODEL_CATALOG: &[(&str, u32, &str)] = &[
    (
        DEFAULT_SHERPA_MODEL,
        SHERPA_BILINGUAL_SIZE_MB as u32,
        "Streaming Zipformer (Chinese + English) - real-time ASR with endpoint detection",
    ),
];
