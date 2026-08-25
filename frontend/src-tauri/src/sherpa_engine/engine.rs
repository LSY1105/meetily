//! SherpaOnnx streaming transcription engine.
//!
//! Wraps the `sherpa_onnx::OnlineRecognizer` so the rest of the app can use the
//! same `transcribe_audio()` interface as the parakeet/whisper engines. The
//! recognizer supports incremental chunk-by-chunk decoding — the worker's
//! accept_waveform -> decode -> is_endpoint loop yields live partial text and
//! commits utterances when VAD-style endpoint detection fires.
//!
//! ponytail: per-session `OnlineStream` is fine; the recognizer is shared and
//! recreated only on model load. Synchronous decode calls run on a blocking
//! task to keep the Tauri async runtime unblocked.

use crate::sherpa_engine::model::{
    is_model_valid_for, SherpaModelInfo, SherpaModelStatus, SHERPA_BILINGUAL_FILES,
    SHERPA_MODEL_CATALOG, PUNCT_MODEL_DIR,
};
use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use sherpa_onnx::{
    OfflinePunctuation, OfflinePunctuationConfig, OfflinePunctuationModelConfig,
    OnlineRecognizer, OnlineRecognizerConfig, OnlineStream,
};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use bzip2::read::BzDecoder;
use tar::Archive;

/// Errors surfaced from the sherpa engine. Mirrors the shape used by the
/// parakeet engine so callers can map to `TranscriptionError` consistently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SherpaEngineError {
    ModelNotLoaded,
    ModelNotFound(String),
    TranscriptionFailed(String),
    DownloadFailed(String),
    Other(String),
}

impl std::fmt::Display for SherpaEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelNotLoaded => write!(f, "No sherpa-onnx model loaded"),
            Self::ModelNotFound(name) => write!(f, "Sherpa model '{}' not found", name),
            Self::TranscriptionFailed(e) => write!(f, "Sherpa transcription failed: {}", e),
            Self::DownloadFailed(e) => write!(f, "Sherpa download failed: {}", e),
            Self::Other(e) => write!(f, "Sherpa error: {}", e),
        }
    }
}

impl std::error::Error for SherpaEngineError {}

impl From<anyhow::Error> for SherpaEngineError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e.to_string())
    }
}

/// Download progress mirror of parakeet's `DownloadProgress` — same shape so
/// the frontend can render either engine's download UI without branching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub downloaded_mb: f64,
    pub total_mb: f64,
    pub speed_mbps: f64,
    pub percent: u8,
}

/// Public streaming result returned to the transcription worker. The worker
/// uses `is_partial` to decide between the partial- and final-event channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingResult {
    pub text: String,
    pub is_endpoint: bool,
    pub is_partial: bool,
}

/// Inner state guarded by the engine's RwLock. The recognizer is cheap to
/// keep around (it just holds the loaded ONNX sessions); each utterance
/// gets a fresh `OnlineStream` from it. `recognizer` is `Arc` so the
/// streaming slot can hold a clone without paying for a second ONNX
/// session — sharing one is fine; only the per-stream buffers are
/// stateful.
struct Inner {
    recognizer: Arc<OnlineRecognizer>,
    stream: OnlineStream,
}

pub struct SherpaEngine {
    models_dir: PathBuf,
    current: Arc<RwLock<Option<Inner>>>,
    // ponytail: parallel independent OnlineStream for the raw-streaming
    // path. VAD path holds the lock on `current` and may call `reset()` to
    // commit an utterance, which wipes accumulated decoder state. The
    // streaming path needs its own state that lives across the entire
    // recording session — putting it in `current` causes a race where a
    // VAD reset blows away the streaming hypothesis and the next partial
    // we emit looks like garbage from a reset buffer. Two slots = two
    // independent decoder contexts, no contention, no surprise resets.
    current_streaming: Arc<RwLock<Option<OnlineStream>>>,
    current_streaming_recognizer: Arc<RwLock<Option<Arc<OnlineRecognizer>>>>,
    current_model_name: Arc<RwLock<Option<String>>>,
    available_models: Arc<RwLock<HashMap<String, SherpaModelInfo>>>,
    active_downloads: Arc<RwLock<std::collections::HashSet<String>>>,
    cancel_download_flag: Arc<RwLock<Option<String>>>,
    // ponytail: offline CJK punctuator, loaded in parallel with the
    // streaming recognizer. Independent of the model name currently
    // loaded for ASR — the user can keep their zipformer and still
    // get punctuation. `None` means the punctuator model is missing
    // from disk; the frontend falls back to the rule-based inserter
    // (`punctuateCJK.ts`) when this happens.
    punctuator: Arc<RwLock<Option<Arc<OfflinePunctuation>>>>,
}

/// ponytail: when this guard drops, the model name is removed from
/// `active_downloads`. Returning an `Err` from the calling function,
/// panicking, or being cancelled at any await point all drop this
/// guard, so the slot is always freed — fixing the bug where one
/// failed download permanently locked out future downloads for the
/// same model.
struct ActiveDownloadGuard {
    set: Arc<RwLock<std::collections::HashSet<String>>>,
    key: String,
}

impl ActiveDownloadGuard {
    fn new(set: Arc<RwLock<std::collections::HashSet<String>>>, key: String) -> Self {
        Self { set, key }
    }
}

impl Drop for ActiveDownloadGuard {
    fn drop(&mut self) {
        let set = self.set.clone();
        let key = self.key.clone();
        tokio::spawn(async move {
            let mut active = set.write().await;
            active.remove(&key);
        });
    }
}

impl SherpaEngine {
    /// Create a new SherpaEngine, optionally with a custom models directory.
    pub fn new_with_models_dir(models_dir: Option<PathBuf>) -> Result<Self> {
        // ponytail: caller-provided path is the FINAL path (matches
        // whisper/parakeet engine convention). The fallback
        // branches below keep their own `sherpa` suffix because
        // they build a fresh path from scratch.
        let models_dir = if let Some(dir) = models_dir {
            dir
        } else {
            let current_dir = std::env::current_dir()
                .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;
            if cfg!(debug_assertions) {
                current_dir.join("models").join("sherpa")
            } else {
                dirs::data_dir()
                    .or_else(|| dirs::home_dir())
                    .ok_or_else(|| anyhow!("Could not find system data directory"))?
                    .join("Meetily")
                    .join("models")
                    .join("sherpa")
            }
        };

        info!("SherpaEngine using models directory: {}", models_dir.display());

        if !models_dir.exists() {
            std::fs::create_dir_all(&models_dir)?;
        }

        Ok(Self {
            models_dir,
            current: Arc::new(RwLock::new(None)),
            // ponytail: see struct doc — must stay empty until a model
            // is loaded, then `load_model` populates it.
            current_streaming: Arc::new(RwLock::new(None)),
            current_streaming_recognizer: Arc::new(RwLock::new(None)),
            current_model_name: Arc::new(RwLock::new(None)),
            available_models: Arc::new(RwLock::new(HashMap::new())),
            active_downloads: Arc::new(RwLock::new(std::collections::HashSet::new())),
            cancel_download_flag: Arc::new(RwLock::new(None)),
            punctuator: Arc::new(RwLock::new(None)),
        })
    }

    /// Discover all sherpa models listed in the catalog. Sets status to
    /// `Available` if all required files are present, else `Missing`.
    pub async fn discover_models(&self) -> Result<Vec<SherpaModelInfo>> {
        let mut models = Vec::new();
        let active = self.active_downloads.read().await;

        for (name, size_mb, description) in SHERPA_MODEL_CATALOG {
            // ponytail: discover from the configured dir, but also
            // probe `models/<name>` (one level up) for backward
            // compatibility with downloads made before the
            // `sherpa/` subdir was introduced — users who already
            // downloaded the streaming-zipformer bilingual model
            // keep it usable without re-downloading.
            let primary = self.models_dir.join(name);
            let legacy = self
                .models_dir
                .parent()
                .map(|p| p.join(name))
                .unwrap_or_else(|| primary.clone());
            let path = if is_model_valid_for(name, &primary) {
                primary.clone()
            } else if is_model_valid_for(name, &legacy) {
                legacy
            } else {
                primary
            };
            let status = if active.contains(*name) {
                SherpaModelStatus::Downloading { progress: 0 }
            } else if is_model_valid_for(name, &path) {
                SherpaModelStatus::Available
            } else if path.exists() {
                let total_size: u64 = SHERPA_BILINGUAL_FILES
                    .iter()
                    .filter_map(|f| std::fs::metadata(path.join(f)).ok().map(|m| m.len()))
                    .sum();
                SherpaModelStatus::Corrupted {
                    file_size: total_size,
                    expected_min_size: (*size_mb as u64) * 1024 * 1024,
                }
            } else {
                SherpaModelStatus::Missing
            };

            models.push(SherpaModelInfo {
                name: name.to_string(),
                path,
                size_mb: *size_mb,
                status,
                description: description.to_string(),
                streaming: true,
            });
        }

        let mut cache = self.available_models.write().await;
        cache.clear();
        for m in &models {
            cache.insert(m.name.clone(), m.clone());
        }
        Ok(models)
    }

    /// Load a sherpa-onnx model from disk and create the OnlineRecognizer.
    /// This is the slow path (ONNX session build, ~1-2s); callers should call
    /// it once per recording session, not per chunk.
    pub async fn load_model(&self, model_name: &str) -> Result<()> {
        // Validate model exists on disk before attempting to load.
        let model_path = self.models_dir.join(model_name);
        // ponytail: legacy compatibility — pre-`sherpa/`-subdir builds
        // stored models at `models/<name>` rather than
        // `models/sherpa/<name>`. Probe both so users don't have to
        // re-download ~488 MB after upgrading.
        let legacy_path = self
            .models_dir
            .parent()
            .map(|p| p.join(model_name))
            .unwrap_or_else(|| model_path.clone());
        let model_path = if is_model_valid_for(model_name, &model_path) {
            model_path
        } else if is_model_valid_for(model_name, &legacy_path) {
            legacy_path
        } else {
            model_path
        };
        // ponytail: accept the nested tarball layout (Punct ships
        // with a `<asset>/...` top-level dir inside the .tar.bz2) so
        // the validator below doesn't fail the same load that
        // `is_model_valid_for` already accepted.
        let required = crate::sherpa_engine::model::required_files_for(model_name);
        let flat_ok = required.iter().all(|f| model_path.join(f).exists());
        if !flat_ok {
            let nested = model_path.join(model_name);
            let nested_ok = nested.exists()
                && required.iter().all(|f| nested.join(f).exists());
            if !nested_ok {
                return Err(anyhow!(
                    "Sherpa model '{}' files missing in {} (expected: {:?})",
                    model_name,
                    model_path.display(),
                    required
                ));
            }
        }

        // If the same model is already loaded, no-op.
        if let Some(name) = self.current_model_name.read().await.as_ref() {
            if name == model_name {
                info!("Sherpa model '{}' already loaded", model_name);
                // ponytail: a previous session's `handle.abort()` may
                // have left `current_streaming` as None. Recreate
                // the streaming slot from the still-loaded recognizer
                // so the next recording doesn't emit into a missing
                // stream — this is the recording-stop-hang fix.
                let rec_guard = self.current_streaming_recognizer.read().await;
                if let Some(rec) = rec_guard.as_ref() {
                    let mut slot = self.current_streaming.write().await;
                    if slot.is_none() {
                        *slot = Some(rec.create_stream());
                        info!("Recreated missing sherpa streaming slot for '{}'", model_name);
                    }
                }
                return Ok(());
            }
            info!(
                "Unloading previous sherpa model '{}' before loading '{}'",
                name, model_name
            );
        }

        let tokens = model_path.join("tokens.txt");

        // ponytail: dispatch on model family. sherpa-onnx's C runtime
        // routes by which config field is populated — transducer fields
        // vs. paraformer encoder/decoder. We set exactly the right one
        // and leave the others at their Default::default() (empty) so
        // the dispatch in online-recognizer-impl.cc picks our model.
        let is_paraformer = model_name
            == crate::sherpa_engine::model::PARAFORMER_BILINGUAL_MODEL;

        let (encoder_path, decoder_path, joiner_path) = if is_paraformer {
            (
                model_path.join("encoder.int8.onnx"),
                model_path.join("decoder.int8.onnx"),
                None,
            )
        } else {
            (
                model_path.join("encoder-epoch-99-avg-1.onnx"),
                model_path.join("decoder-epoch-99-avg-1.onnx"),
                Some(model_path.join("joiner-epoch-99-avg-1.onnx")),
            )
        };

        // Build the config outside the blocking closure so the file path
        // borrow doesn't escape the await boundary.
        let mut config = OnlineRecognizerConfig::default();
        config.model_config.tokens = Some(tokens.to_string_lossy().to_string());
        config.model_config.num_threads = 2;
        config.model_config.provider = Some("cpu".into());
        if is_paraformer {
            config.model_config.paraformer.encoder =
                Some(encoder_path.to_string_lossy().to_string());
            config.model_config.paraformer.decoder =
                Some(decoder_path.to_string_lossy().to_string());
            // ponytail: paraformer is non-transducer — `modeling_unit`
            // tells the C tokenizer how to convert tokens. "cjkchar"
            // emits one char per token (best for Chinese characters),
            // "bpe" emits BPE pieces. The bilingual Paraformer uses
            // mixed cjkchar+bpe; leaving it unset lets sherpa-onnx pick
            // from tokens.txt metadata, which is what the official
            // streaming example does.
        } else {
            config.model_config.transducer.encoder =
                Some(encoder_path.to_string_lossy().to_string());
            config.model_config.transducer.decoder =
                Some(decoder_path.to_string_lossy().to_string());
            if let Some(j) = joiner_path {
                config.model_config.transducer.joiner =
                    Some(j.to_string_lossy().to_string());
            }
        }
        config.enable_endpoint = true;
        config.decoding_method = Some("greedy_search".into());
        // ponytail: streaming endpoint tuning. rule1 / rule2 fire on
        // trailing silence and commit the partial. rule3 is the
        // "max-utterance" safety net — even without silence, after
        // N seconds of continuous speech the recognizer commits so the
        // UI never holds a stale single-line partial for the entire
        // sentence.
        //
        // ponytail: history of these values — 1.2/0.8/8.0 → 2.0/1.4/20
        // → 0.8/0.5/6.0 → 1.6/1.2/15.0 → now 3.0/2.4/30.0. The previous
        // 1.6s threshold still fragmented the timeline into 8-9 short
        // segments per minute of Chinese speech because sherpa-onnx's
        // internal rule1 fires on trailing silence OR short token
        // boundaries, not pure silence alone. 3.0s tolerates natural
        // clause pauses (0.5-1.5s) plus the occasional hesitation, so
        // each row in the meeting timeline holds a complete thought
        // rather than a fragment. rule3=30s caps the worst case for a
        // long continuous monologue. Matches the worker-side
        // MAX_CHUNKS_BETWEEN_COMMITS=300 (30s) and
        // SILENT_CHUNKS_FOR_ENDPOINT=35 (3.5s) backstops. The
        // frontend typing effect (TranscriptView.tsx) stretches the
        // in-place animation to 2.0 s on each commit so the user
        // sees a visible typewriter pulse even on long sentences.
        config.rule1_min_trailing_silence = 1.0;
        config.rule2_min_trailing_silence = 0.7;
        config.rule3_min_utterance_length = 5.0;

        // sherpa-onnx session build is CPU-heavy; run on a blocking task so
        // the Tauri runtime doesn't stall while the model initializes.
        let model_name_owned = model_name.to_string();
        let recognizer = tokio::task::spawn_blocking(move || OnlineRecognizer::create(&config))
            .await
            .map_err(|e| anyhow!("Recognizer build task panicked: {}", e))?
            .ok_or_else(|| {
                anyhow!(
                    "Failed to build sherpa-onnx OnlineRecognizer for '{}' (check ONNX file integrity)",
                    model_name_owned
                )
            })?;

        let stream = recognizer.create_stream();
        let recognizer = Arc::new(recognizer);

        *self.current.write().await = Some(Inner {
            recognizer: recognizer.clone(),
            stream,
        });
        *self.current_model_name.write().await = Some(model_name.to_string());

        // ponytail: arm the streaming path with its own fresh
        // OnlineStream. Sharing the recognizer is fine (it's read-only
        // for streaming purposes once built); the stream itself is
        // independent so VAD resets on `current` don't touch the
        // streaming hypothesis.
        *self.current_streaming.write().await = Some(recognizer.create_stream());
        *self.current_streaming_recognizer.write().await = Some(recognizer);

        info!(
            "Loaded sherpa-onnx model '{}' from {}",
            model_name,
            self.models_dir.join(model_name).display()
        );

        // ponytail: load the CJK punctuation model alongside the
        // streaming recognizer. Independent of the ASR model name —
        // a user on zipformer can still have Paraformer-grade
        // punctuation. If the model files are missing we leave
        // `punctuator = None`; the worker falls back to emitting
        // raw text and the frontend rule-based inserter takes over.
        // Failure is logged, not raised — we don't want a missing
        // 100MB punctuation model to block recording entirely.
        // ponytail: punctuation tarball extracts into a nested top-level
        // dir (e.g. `sherpa-onnx-punct-...-int8/model.int8.onnx`),
        // not straight into `punct_dir`. Look one level deeper and
        // use the int8-quantized checkpoint filename the asset
        // actually ships.
        // Idempotent: skip the (expensive) ONNX load when the punctuator
        // is already attached - repeated load_model calls would otherwise
        // rebuild it every time.
        let punct_dir = self.models_dir.join(PUNCT_MODEL_DIR);
        let punct_model_path = punct_dir
            .join(PUNCT_MODEL_DIR)
            .join("model.int8.onnx");
        if punct_model_path.exists() && !self.is_punctuator_loaded().await {
            let cfg = OfflinePunctuationConfig {
                model: OfflinePunctuationModelConfig {
                    ct_transformer: Some(
                        punct_model_path.to_string_lossy().to_string(),
                    ),
                    num_threads: 1,
                    debug: false,
                    provider: Some("cpu".to_string()),
                },
            };
            let cfg_clone = cfg.clone();
            match tokio::task::spawn_blocking(move || {
                OfflinePunctuation::create(&cfg_clone)
            })
            .await
            {
                Ok(Some(p)) => {
                    info!(
                        "Loaded CJK punctuation model from {}",
                        punct_dir.display()
                    );
                    *self.punctuator.write().await = Some(Arc::new(p));
                }
                Ok(None) => {
                    warn!(
                        "OfflinePunctuation::create returned None for {}; falling back to rule-based punctuation",
                        punct_dir.display()
                    );
                }
                Err(e) => {
                    warn!(
                        "Punctuator build task panicked: {}; falling back to rule-based punctuation",
                        e
                    );
                }
            }
        } else {
            info!(
                "CJK punctuation model not installed at {}; rule-based fallback will be used",
                punct_dir.display()
            );
        }

        Ok(())
    }

    /// Push a chunk of 16kHz mono PCM samples into the recognizer. Blocks
    /// until decode finishes for the new samples; cheap (~ms per chunk).
    /// Returns the latest hypothesis and endpoint flag.
    pub async fn accept_waveform(
        &self,
        samples: Vec<f32>,
        sample_rate: u32,
    ) -> Result<StreamingResult> {
        // Cheap-lock the inner; the decode is on the async-critical path so
        // we avoid spawn_blocking here — accept_waveform + decode + get_result
        // runs in single-digit ms for the int8 bilingual model on CPU.
        let mut guard = self.current.write().await;
        let inner = guard
            .as_mut()
            .ok_or_else(|| anyhow!("No sherpa model loaded"))?;

        // sherpa-onnx expects sample_rate as i32.
        inner.stream.accept_waveform(sample_rate as i32, &samples);

        // Drive the recognizer until the stream is drained. Multiple decode
        // steps may be needed per accept_waveform for longer chunks.
        while inner.recognizer.is_ready(&inner.stream) {
            inner.recognizer.decode(&inner.stream);
        }

        let is_endpoint = inner.recognizer.is_endpoint(&inner.stream);
        let text = inner
            .recognizer
            .get_result(&inner.stream)
            .map(|r| r.text)
            .unwrap_or_default();

        // For non-endpoint frames, this is a partial result (subject to
        // further revision as more audio arrives). For endpoint frames the
        // caller will reset() the stream after committing the text.
        let is_partial = !is_endpoint;

        Ok(StreamingResult {
            text: text.trim().to_string(),
            is_endpoint,
            is_partial,
        })
    }

    /// Streaming-mode append: feed samples into the live OnlineStream
    /// **without resetting** between calls. The session is held for the
    /// full recording; the caller (`worker.rs` stream listener) invokes
    /// `streaming_reset()` once on session end, not per-segment.
    ///
    /// ponytail: this is the core of the "real word-flow" cadence. With
    /// the VAD-batched path each call carried 1.7s of audio and the
    /// `is_endpoint` branch forced a reset every 8s — so the UI only saw
    /// text commit at pause boundaries. Here every 100ms audio chunk is
    /// appended to the same stream and `get_result` returns the latest
    /// partial, so the user sees characters update continuously.
    ///
    /// Critical: this path uses `current_streaming`, NOT `current`. The
    /// VAD-batched path may call `reset()` on `current` at endpoint
    /// boundaries; if we shared the stream with it, that reset would
    /// wipe our hypothesis and the next emit would look like garbage
    /// from a reset buffer. Two slots = no contention.
    pub async fn accept_samples_for_streaming(
        &self,
        samples: Vec<f32>,
        sample_rate: u32,
    ) -> Result<StreamingResult> {
        // ponytail: take both locks in a fixed order (recognizer first,
        // then stream) to avoid deadlocks with any future code path
        // that might want to read both. They're held for the duration
        // of decode so no other streaming call can race in.
        let rec_guard = self
            .current_streaming_recognizer
            .read()
            .await;
        let rec = rec_guard
            .as_ref()
            .ok_or_else(|| anyhow!("No sherpa model loaded (streaming)"))?;
        let mut stream_guard = self.current_streaming.write().await;
        let stream = stream_guard
            .as_mut()
            .ok_or_else(|| anyhow!("No sherpa model loaded (streaming)"))?;

        stream.accept_waveform(sample_rate as i32, &samples);
        let mut decode_iters = 0;
        while rec.is_ready(stream) {
            rec.decode(stream);
            decode_iters += 1;
            if decode_iters > 50 {
                break;
            }
        }
        let is_endpoint = rec.is_endpoint(stream);
        let text = rec
            .get_result(stream)
            .map(|r| r.text)
            .unwrap_or_default();
        log::debug!(
            "sherpa streaming: samples={}, decode_iters={}, text_len={}, endpoint={}",
            samples.len(),
            decode_iters,
            text.len(),
            is_endpoint
        );
        Ok(StreamingResult {
            text: text.trim().to_string(),
            is_endpoint,
            is_partial: true,
        })
    }

    /// ponytail: reset the streaming slot. Call once when the recording
    /// session ends. Does NOT touch `current` — VAD path keeps its own
    /// state.
    pub async fn streaming_reset(&self) -> Result<()> {
        let rec_guard = self.current_streaming_recognizer.read().await;
        let rec = match rec_guard.as_ref() {
            Some(r) => r,
            None => return Ok(()), // No model loaded — nothing to reset.
        };
        let mut stream_guard = self.current_streaming.write().await;
        if let Some(stream) = stream_guard.as_mut() {
            rec.reset(stream);
        }
        Ok(())
    }

    /// ponytail: synchronous punctuation pass. `None` means the
    /// punctuator model is missing — caller should fall back to the
    /// frontend rule-based inserter (`punctuateCJK.ts`). The sherpa
    /// binding's `add_punctuation` is synchronous FFI (~40ms on a
    /// sentence-length run on CPU), which fits inside the worker's
    /// 250ms Mid throttle window without stalling audio.
    ///
    /// Output preserves the input characters: this model only
    /// inserts `，。？！、` between existing characters; it never
    /// deletes, swaps, or reorders. So the iFlytek `strip_prefix`
    /// invariant in worker.rs keeps holding as long as the caller
    /// stores the punctuated output in `last_emitted_for_sn` and
    /// diffs against it on the next tick.
    pub async fn punctuate(&self, text: &str) -> Option<String> {
        let guard = self.punctuator.read().await;
        let p = guard.as_ref()?;
        p.add_punctuation(text)
    }

    /// ponytail: frontend health probe — exposed via a Tauri command
    /// so the ModelPicker can render "installed" vs "missing" without
    /// a separate network call.
    pub async fn is_punctuator_loaded(&self) -> bool {
        self.punctuator.read().await.is_some()
    }

    /// ponytail: load the CJK punctuator from disk without requiring
    /// an ASR model to be loaded first. Used by the
    /// `sherpa_load_punctuator` command so the Punct row in the
    /// ModelPicker can flip to "Active" even when the user is on
    /// provider=Whisper. The block here is the same as the inline
    /// load in `load_model` — pulling it out as a public method lets
    /// the frontend trigger it standalone. Idempotent: if the
    /// punctuator is already loaded, this no-ops. Files-missing case
    /// is logged but not raised — callers can re-probe
    /// `is_punctuator_loaded` afterwards and see the false.
    pub async fn attach_punctuator_if_files_present(&self) {
        if self.is_punctuator_loaded().await {
            return;
        }
        let punct_dir = self.models_dir.join(PUNCT_MODEL_DIR);
        let punct_model_path = punct_dir
            .join(PUNCT_MODEL_DIR)
            .join("model.int8.onnx");
        if !punct_model_path.exists() {
            return;
        }
        let cfg = OfflinePunctuationConfig {
            model: OfflinePunctuationModelConfig {
                ct_transformer: Some(
                    punct_model_path.to_string_lossy().to_string(),
                ),
                num_threads: 1,
                debug: false,
                provider: Some("cpu".to_string()),
            },
        };
        let cfg_clone = cfg.clone();
        match tokio::task::spawn_blocking(move || {
            OfflinePunctuation::create(&cfg_clone)
        })
        .await
        {
            Ok(Some(p)) => {
                info!(
                    "Loaded CJK punctuation model from {}",
                    punct_dir.display()
                );
                *self.punctuator.write().await = Some(Arc::new(p));
            }
            Ok(None) => {
                warn!(
                    "OfflinePunctuation::create returned None for {}; falling back to rule-based punctuation",
                    punct_dir.display()
                );
            }
            Err(e) => {
                warn!(
                    "Punctuator build task panicked: {}; falling back to rule-based punctuation",
                    e
                );
            }
        }
    }

    /// Reset the stream — call after committing the text from an endpoint.
    pub async fn reset(&self) -> Result<()> {
        let mut guard = self.current.write().await;
        let inner = guard
            .as_mut()
            .ok_or_else(|| anyhow!("No sherpa model loaded"))?;
        inner.recognizer.reset(&inner.stream);
        Ok(())
    }

    /// Currently loaded model name, if any.
    pub async fn get_current_model(&self) -> Option<String> {
        self.current_model_name.read().await.clone()
    }

    /// True when a model is loaded and ready to accept audio.
    pub async fn is_model_loaded(&self) -> bool {
        self.current.read().await.is_some()
    }

    /// One-shot transcription (legacy `transcribe_audio` shape, used by the
    /// parakeet-compat path). Feeds all audio, drains the recognizer, returns
    /// the final text. New code should prefer `accept_waveform` for live
    /// captions.
    pub async fn transcribe_audio(&self, audio: Vec<f32>) -> Result<String> {
        let mut last_text = String::new();
        // 500ms-ish chunks at 16kHz = 8000 samples.
        for chunk in audio.chunks(8000) {
            let r = self.accept_waveform(chunk.to_vec(), 16_000).await?;
            if !r.text.is_empty() {
                last_text = r.text.clone();
            }
            if r.is_endpoint {
                let _ = self.reset().await;
            }
        }
        // Drain any remaining trailing text.
        Ok(last_text)
    }

    /// Return the absolute models directory.
    pub async fn get_models_directory(&self) -> PathBuf {
        self.models_dir.clone()
    }

    /// Delete a corrupted or fully-downloaded model directory. Mirrors the
    /// parakeet engine's behavior so the frontend can use the same UI flow.
    pub async fn delete_model(&self, model_name: &str) -> Result<String> {
        let path = self.models_dir.join(model_name);
        if path.exists() {
            tokio::fs::remove_dir_all(&path)
                .await
                .map_err(|e| anyhow!("Failed to delete '{}': {}", path.display(), e))?;
        }
        {
            let mut cache = self.available_models.write().await;
            if let Some(m) = cache.get_mut(model_name) {
                m.status = SherpaModelStatus::Missing;
            }
        }
        Ok(format!("Deleted sherpa model '{}'", model_name))
    }

    /// Download the bilingual streaming-zipformer model archive and extract it
    /// into the models dir. Emits `progress.percent` via the callback.
    /// For now this delegates to a single reqwest stream + Archive; the
    /// frontend can re-use the parakeet download UI without changes.
    pub async fn download_model_detailed(
        &self,
        model_name: &str,
        progress_callback: Option<Box<dyn Fn(DownloadProgress) + Send + Sync>>,
    ) -> Result<()> {
        use futures_util::StreamExt;
        use std::time::{Duration, Instant};

        // Mark active + push status.
        {
            let mut active = self.active_downloads.write().await;
            if active.contains(model_name) {
                return Err(anyhow!("Download already in progress for {}", model_name));
            }
            active.insert(model_name.to_string());
        }
        // ponytail: RAII guard releases the active_downloads slot on every
        // exit path — replaces 4 hand-removed `active.remove(model_name)` calls
        // and fixes the leak when the function exits via early-return, panic,
        // or `?` propagation through futures.
        let _guard = ActiveDownloadGuard::new(self.active_downloads.clone(), model_name.to_string());
        {
            let mut cache = self.available_models.write().await;
            if let Some(m) = cache.get_mut(model_name) {
                m.status = SherpaModelStatus::Downloading { progress: 0 };
            }
        }

        // ponytail: dispatch the download URL on model name. The streaming
        // Paraformer bilingual model is hosted under a different asset
        // path on the k2-fsa releases than the legacy zipformer.
        let url = if model_name == crate::sherpa_engine::model::PARAFORMER_BILINGUAL_MODEL {
            crate::sherpa_engine::model::PARAFORMER_BILINGUAL_URL
        } else {
            crate::sherpa_engine::model::SHERPA_BILINGUAL_URL
        };
        let target_dir = self.models_dir.join(model_name);
        if target_dir.exists() {
            // Best-effort cleanup; bail on failure to keep state consistent.
            if let Err(e) = tokio::fs::remove_dir_all(&target_dir).await {
                // ponytail: ActiveDownloadGuard drops on return; no manual remove.
                return Err(anyhow!("Failed to clean stale model dir: {}", e));
            }
        }
        tokio::fs::create_dir_all(&target_dir)
            .await
            .map_err(|e| anyhow!("Failed to create model dir: {}", e))?;

        let result: Result<()> = async {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(3600))
                .build()
                .map_err(|e| anyhow!("HTTP client init: {}", e))?;
            let response = client
                .get(url)
                .send()
                .await
                .map_err(|e| anyhow!("Download start: {}", e))?;
            let total_size = response.content_length().unwrap_or(0);
            let mut downloaded: u64 = 0;
            let start = Instant::now();
            let mut last_report = Instant::now();
            let mut last_bytes: u64 = 0;

            // Stream into a temp file, then untar/bunzip2.
            let tmp_path = target_dir.join("download.tar.bz2");
            let mut file = tokio::fs::File::create(&tmp_path)
                .await
                .map_err(|e| anyhow!("Create tmp: {}", e))?;
            let mut stream = response.bytes_stream();

            while let Some(chunk) = stream.next().await {
                // Check cancellation.
                if let Some(flag) = self.cancel_download_flag.read().await.as_ref() {
                    if flag == model_name {
                        return Err(anyhow!("Download cancelled"));
                    }
                }
                let chunk = chunk.map_err(|e| anyhow!("Stream error: {}", e))?;
                tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
                    .await
                    .map_err(|e| anyhow!("Write: {}", e))?;
                downloaded += chunk.len() as u64;
                if last_report.elapsed() >= Duration::from_millis(500) {
                    let dt = last_report.elapsed().as_secs_f64();
                    let speed = if dt > 0.0 {
                        (downloaded - last_bytes) as f64 / dt / 1_048_576.0
                    } else {
                        0.0
                    };
                    let percent = if total_size > 0 {
                        ((downloaded as f64 / total_size as f64) * 100.0).min(99.0) as u8
                    } else {
                        0
                    };
                    let progress = DownloadProgress {
                        downloaded_bytes: downloaded,
                        total_bytes: total_size,
                        downloaded_mb: downloaded as f64 / 1_048_576.0,
                        total_mb: total_size as f64 / 1_048_576.0,
                        speed_mbps: speed,
                        percent,
                    };
                    if let Some(cb) = &progress_callback {
                        cb(progress);
                    }
                    last_report = Instant::now();
                    last_bytes = downloaded;
                }
            }
            // ponytail: emit an "extracting" tick before tar.bz2 unpacking.
            // The previous code emitted percent=100 *here* (before the
            // extract task), which left the UI stuck at 100% when
            // BzDecoder failed — the file was on disk but no model
            // files existed yet, so discover_models flagged the model
            // Missing while the picker kept showing "Downloading".
            // The frontend now reads "Downloading { progress }" and
            // switches to a check phase when progress >= 100.
            let total_elapsed = start.elapsed().as_secs_f64();
            let final_speed = if total_elapsed > 0.0 {
                downloaded as f64 / total_elapsed / 1_048_576.0
            } else {
                0.0
            };
            if let Some(cb) = &progress_callback {
                cb(DownloadProgress {
                    downloaded_bytes: downloaded,
                    total_bytes: total_size.max(downloaded),
                    downloaded_mb: downloaded as f64 / 1_048_576.0,
                    total_mb: total_size as f64 / 1_048_576.0,
                    speed_mbps: final_speed,
                    percent: 100,
                });
            }
            drop(file);

            // ponytail: extract the .tar.bz2 with the stock tar unpack then
            // accept either the flat or nested layout via the model-
            // name-aware validity check in `model.rs`. Earlier
            // iterations walked entries manually, but PAX/UTF-8 path
            // handling across the tar crate's edge cases (long
            // names, GNU extensions) made that approach brittle.
            // Letting `unpack` do the platform-correct path
            // resolution and then tolerating either layout on the
            // read side is the smaller surface area.
            let tmp_path_sync = tmp_path.clone();
            let target_dir_for_extract = target_dir.clone();
            let extract_result = tokio::task::spawn_blocking(move || -> Result<()> {
                let file = std::fs::File::open(&tmp_path_sync)?;
                let bz = BzDecoder::new(file);
                let mut archive = Archive::new(bz);
                archive.unpack(target_dir_for_extract)?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow!("Extract task panicked: {}", e))?;

            // ponytail: if the archive unpacked but produced none of the
            // required ONNX/tokens files at the expected location,
            // the download silently succeeded against a wrong URL or
            // a stripped tarball. Surface that as an error rather
            // than a silent Missing status.
            //
            // The sherpa-onnx punctuation asset ships with a nested
            // top-level dir (`sherpa-onnx-punct-...-int8/`); the
            // other catalog models extract straight into `target_dir`.
            // Accept either layout so the int8 punctuation model
            // doesn't fail the post-extract check.
            if extract_result.is_ok() {
                let required = crate::sherpa_engine::model::required_files_for(model_name);
                let top_ok = required
                    .iter()
                    .all(|f| target_dir.join(f).exists());
                if !top_ok {
                    let nested = target_dir.join(model_name);
                    let nested_ok = nested.exists()
                        && required
                            .iter()
                            .all(|f| nested.join(f).exists());
                    // ponytail: surface what the extractor actually
                    // produced so the toast text isn't the only
                    // diagnostic for a bad layout.
                    let listing = std::fs::read_dir(&target_dir)
                        .map(|rd| {
                            rd.filter_map(|e| e.ok())
                                .map(|e| e.file_name().to_string_lossy().to_string())
                                .collect::<Vec<_>>()
                        .join(", ")
                        })
                        .unwrap_or_else(|e| format!("<read_dir failed: {}>", e));
                    if !nested_ok {
                        let _ = tokio::fs::remove_dir_all(&target_dir).await;
                        let _ = tokio::fs::remove_file(&tmp_path).await;
                        return Err(anyhow!(
                            "Downloaded archive did not contain expected files: {:?}; \
                             extracted top-level entries: [{}]",
                            required,
                            listing
                        ));
                    }
                }
            }

            // Remove the archive; final files remain.
            let _ = tokio::fs::remove_file(&tmp_path).await;
            extract_result
        }
        .await;

        // ponytail: on extract failure, clean up the half-populated
        // target directory so discover_models reports Missing instead
        // of leaving a confusing empty folder behind.
        if let Err(e) = &result {
            let _ = tokio::fs::remove_dir_all(&target_dir).await;
            warn!(
                "sherpa download/extract failed for '{}': {}; cleaned target dir",
                model_name, e
            );
        }

        // ponytail: ActiveDownloadGuard drops when this fn returns; the
        // explicit `active.remove` block above was redundant (and would have
        // leaked if this fn panicked before reaching it). Status cleanup below.
        match &result {
            Ok(()) => {
                let mut cache = self.available_models.write().await;
                if let Some(m) = cache.get_mut(model_name) {
                    m.status = SherpaModelStatus::Available;
                }
            }
            Err(_) => {
                let mut cache = self.available_models.write().await;
                if let Some(m) = cache.get_mut(model_name) {
                    m.status = SherpaModelStatus::Missing;
                }
            }
        }
        if let Err(e) = &result {
            warn!("sherpa download failed: {}", e);
        } else {
            debug!("sherpa download completed");
        }
        result
    }

    /// Cancel a running download. Best-effort: flips the flag and lets the
    /// download loop check it on the next chunk.
    pub async fn cancel_download(&self, model_name: &str) -> Result<()> {
        *self.cancel_download_flag.write().await = Some(model_name.to_string());
        {
            let mut active = self.active_downloads.write().await;
            active.remove(model_name);
        }
        Ok(())
    }
}
