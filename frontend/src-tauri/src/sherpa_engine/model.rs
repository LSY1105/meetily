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
// ponytail: switched the streaming default to Paraformer. Zipformer
// doesn't do on-device punctuation or number normalization — every
// final segment comes out as a run-on string of Chinese characters,
// which is what users have been complaining about. Paraformer is
// FunASR-derived and includes both, so "二零二六年" lands as "2026年"
// with proper punctuation. ~15% slower than zipformer on the same
// hardware but the per-utterance error rate drops significantly.
// Users who prefer zipformer can still pick it from the model
// selector; this just changes the *default* a fresh install lands on.
pub const DEFAULT_SHERPA_MODEL: &str = PARAFORMER_BILINGUAL_MODEL;

/// Asset filename for the bilingual zipformer model.
pub const SHERPA_BILINGUAL_ASSET: &str =
    "sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2";

/// Download URL for the bilingual zipformer model.
pub const SHERPA_BILINGUAL_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2";

/// Approximate on-disk size in MB (after extraction; the tarball is ~488 MB).
pub const SHERPA_BILINGUAL_SIZE_MB: u64 = 350;

// ponytail: catalog identity for the zipformer. Previously the catalog
// listed `DEFAULT_SHERPA_MODEL` (which is the Paraformer name) with
// the Zipformer description — so the UI got "Paraformer name +
// Zipformer description" in one row and a separate "Paraformer name
// + Paraformer description" row, both ending up at the same friendly
// name. Add this constant and use it in the catalog so the picker
// shows two distinct ASR models.
pub const SHERPA_BILINGUAL_MODEL: &str =
    "sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20";

/// Streaming Paraformer (bilingual Chinese+English) — k2-fsa re-export of
/// the FunASR Paraformer architecture, repackaged for the streaming
/// OnlineRecognizer. Smaller (encoder int8 + decoder fp16) than the
/// zipformer bilingual, with comparable or better Chinese accuracy and
/// cleaner punctuation. ~245 MB after extraction.
pub const PARAFORMER_BILINGUAL_MODEL: &str = "sherpa-onnx-streaming-paraformer-bilingual-zh-en";

/// Asset filename for the streaming Paraformer bilingual model.
pub const PARAFORMER_BILINGUAL_ASSET: &str =
    "sherpa-onnx-streaming-paraformer-bilingual-zh-en.tar.bz2";

/// Download URL for the streaming Paraformer bilingual model.
pub const PARAFORMER_BILINGUAL_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-paraformer-bilingual-zh-en.tar.bz2";

/// Approximate on-disk size in MB after extraction.
pub const PARAFORMER_BILINGUAL_SIZE_MB: u64 = 245;

/// Required files for the streaming Paraformer bilingual model after
/// extraction. Only encoder + decoder + tokens; no joiner (Paraformer is
/// a non-transducer architecture).
pub const PARAFORMER_BILINGUAL_FILES: &[&str] = &[
    "encoder.int8.onnx",
    "decoder.int8.onnx",
    "tokens.txt",
];

/// Required files after extraction. The 2023-02-20 zipformer bilingual model
/// uses the standard k2-fsa file layout (see sherpa-onnx repo).
pub const SHERPA_BILINGUAL_FILES: &[&str] = &[
    "encoder-epoch-99-avg-1.onnx",
    "decoder-epoch-99-avg-1.onnx",
    "joiner-epoch-99-avg-1.onnx",
    "tokens.txt",
];

// ponytail: offline CJK punctuation model, used by the streaming
// emit path (worker.rs) to insert `，。？！` into the raw sherpa
// hypothesis. Loaded alongside the streaming recognizer; sherpa-onnx
// 1.13.4 ships `OfflinePunctuation` under the `static` feature.
//
// Asset URL verified live against
// k2-fsa.github.io/sherpa/onnx/punctuation/pretrained_models.html
// AND by inspecting the downloaded tarball: the archive extracts
// to a top-level dir whose name matches the *asset* filename
// (`...-int8`), and the ONNX/tokenizer files inside are named
// `model.int8.onnx` and `tokens.json` (NOT `model.onnx` and
// `tokens.txt` as the non-int8 variant uses). Earlier revisions
// of this file used the non-int8 name which is what produced the
// "Downloading → Missing" loop — files landed in a nested dir and
// the post-extract validator cleaned the empty target dir.
pub const PUNCT_MODEL: &str =
    "sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12-int8";
pub const PUNCT_MODEL_ASSET: &str =
    "sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12-int8.tar.bz2";
pub const PUNCT_MODEL_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/punctuation-models/sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12-int8.tar.bz2";
pub const PUNCT_MODEL_DIR: &str = PUNCT_MODEL;
pub const PUNCT_MODEL_SIZE_MB: u64 = 60;
pub const PUNCT_MODEL_FILES: &[&str] = &[
    "model.int8.onnx",
    "tokens.json",
];

/// ponytail: dispatch the file-presence check to the right set of
/// expected filenames. Zipformer needs four files (encoder/decoder/
/// joiner/tokens); Paraformer needs only three (no joiner). Calling
/// `is_valid_model_dir` with a name that doesn't match either set
/// returns an error that already names the missing model.
pub fn is_valid_model_dir(model_dir: &PathBuf) -> bool {
    // ponytail: keep backward-compat — treat the path as the legacy
    // zipformer layout (single file layout, no model name context).
    SHERPA_BILINGUAL_FILES
        .iter()
        .all(|f| model_dir.join(f).exists())
}

/// ponytail: model-name-aware validity check. Callers that have the
/// catalog name pass it here so Paraformer doesn't get mis-reported as
/// "corrupted" (which would happen if the engine probed for zipformer's
/// `joiner-*.onnx` and didn't find it on a Paraformer dir).
pub fn is_model_valid_for(model_name: &str, model_dir: &PathBuf) -> bool {
    if required_files_for(model_name)
        .iter()
        .all(|f| model_dir.join(f).exists())
    {
        return true;
    }
    // ponytail: sherpa-onnx punctuation asset tarballs ship with a
    // top-level directory whose name matches the model id (e.g.
    // `sherpa-onnx-punct-...-int8/`), so `BzDecoder::unpack` lands the
    // ONNX/tokenizer inside `<target_dir>/<PUNCT_MODEL>/` rather than
    // at `<target_dir>/`. Accept that nested layout too so users
    // don't hit the "downloaded archive did not contain expected
    // files" toast on otherwise-successful downloads.
    if model_name == PUNCT_MODEL {
        let nested = model_dir.join(PUNCT_MODEL);
        return required_files_for(model_name)
            .iter()
            .all(|f| nested.join(f).exists());
    }
    false
}

/// Required-file set for a specific model name. Returns the slice
/// sherpa_engine::engine uses to load the recognizer.
pub fn required_files_for(model_name: &str) -> &'static [&'static str] {
    if model_name == PARAFORMER_BILINGUAL_MODEL {
        PARAFORMER_BILINGUAL_FILES
    } else if model_name == PUNCT_MODEL {
        PUNCT_MODEL_FILES
    } else {
        SHERPA_BILINGUAL_FILES
    }
}

/// ponytail: catalog drives the frontend model picker. The format is
/// `(name, size_mb, description)`. Paraformer is added as a second
/// option for Chinese-forward meeting audio; the zipformer remains the
/// default since it is the most-tested streaming model.
pub const SHERPA_MODEL_CATALOG: &[(&str, u32, &str)] = &[
    (
        SHERPA_BILINGUAL_MODEL,
        SHERPA_BILINGUAL_SIZE_MB as u32,
        "Streaming Zipformer (Chinese + English) - real-time ASR with endpoint detection",
    ),
    (
        PARAFORMER_BILINGUAL_MODEL,
        PARAFORMER_BILINGUAL_SIZE_MB as u32,
        "Streaming Paraformer (Chinese + English) - FunASR-derived streaming ASR, stronger Chinese punctuation and number normalization",
    ),
    (
        PUNCT_MODEL,
        PUNCT_MODEL_SIZE_MB as u32,
        "Offline CJK punctuation (CT-Transformer zh-en) - inserts ，。？！ into raw ASR text in real time; no Chinese ITN",
    ),
];

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

// ponytail: catalog now centralised above (`SHERPA_MODEL_CATALOG`).
// Keeping the legacy constant alias here for any downstream reader that
// still imports the old name.
