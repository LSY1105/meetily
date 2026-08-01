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
    is_valid_model_dir, SherpaModelInfo, SherpaModelStatus, SHERPA_BILINGUAL_FILES,
    SHERPA_MODEL_CATALOG,
};
use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineStream};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

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
/// gets a fresh `OnlineStream` from it.
struct Inner {
    recognizer: OnlineRecognizer,
    stream: OnlineStream,
}

pub struct SherpaEngine {
    models_dir: PathBuf,
    current: Arc<RwLock<Option<Inner>>>,
    current_model_name: Arc<RwLock<Option<String>>>,
    available_models: Arc<RwLock<HashMap<String, SherpaModelInfo>>>,
    active_downloads: Arc<RwLock<std::collections::HashSet<String>>>,
    cancel_download_flag: Arc<RwLock<Option<String>>>,
}

impl SherpaEngine {
    /// Create a new SherpaEngine, optionally with a custom models directory.
    pub fn new_with_models_dir(models_dir: Option<PathBuf>) -> Result<Self> {
        let models_dir = if let Some(dir) = models_dir {
            dir.join("sherpa")
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
            current_model_name: Arc::new(RwLock::new(None)),
            available_models: Arc::new(RwLock::new(HashMap::new())),
            active_downloads: Arc::new(RwLock::new(std::collections::HashSet::new())),
            cancel_download_flag: Arc::new(RwLock::new(None)),
        })
    }

    /// Discover all sherpa models listed in the catalog. Sets status to
    /// `Available` if all required files are present, else `Missing`.
    pub async fn discover_models(&self) -> Result<Vec<SherpaModelInfo>> {
        let mut models = Vec::new();
        let active = self.active_downloads.read().await;

        for (name, size_mb, description) in SHERPA_MODEL_CATALOG {
            let path = self.models_dir.join(name);
            let status = if active.contains(*name) {
                SherpaModelStatus::Downloading { progress: 0 }
            } else if is_valid_model_dir(&path) {
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
        if !is_valid_model_dir(&model_path) {
            return Err(anyhow!(
                "Sherpa model '{}' files missing in {}",
                model_name,
                model_path.display()
            ));
        }

        // If the same model is already loaded, no-op.
        if let Some(name) = self.current_model_name.read().await.as_ref() {
            if name == model_name {
                info!("Sherpa model '{}' already loaded", model_name);
                return Ok(());
            }
            info!(
                "Unloading previous sherpa model '{}' before loading '{}'",
                name, model_name
            );
        }

        let encoder = model_path.join("encoder-epoch-99-avg-1.onnx");
        let decoder = model_path.join("decoder-epoch-99-avg-1.onnx");
        let joiner = model_path.join("joiner-epoch-99-avg-1.onnx");
        let tokens = model_path.join("tokens.txt");

        // Build the config outside the blocking closure so the file path
        // borrow doesn't escape the await boundary.
        let mut config = OnlineRecognizerConfig::default();
        config.model_config.transducer.encoder = Some(encoder.to_string_lossy().to_string());
        config.model_config.transducer.decoder = Some(decoder.to_string_lossy().to_string());
        config.model_config.transducer.joiner = Some(joiner.to_string_lossy().to_string());
        config.model_config.tokens = Some(tokens.to_string_lossy().to_string());
        config.model_config.num_threads = 2;
        config.model_config.provider = Some("cpu".into());
        config.enable_endpoint = true;
        config.decoding_method = Some("greedy_search".into());
        // Smaller chunks = lower latency for live captions. Endpoint rules
        // are flat fields on OnlineRecognizerConfig (no nested endpoint_config).
        config.rule1_min_trailing_silence = 1.2;
        config.rule2_min_trailing_silence = 0.8;
        config.rule3_min_utterance_length = 8.0;

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

        *self.current.write().await = Some(Inner {
            recognizer,
            stream,
        });
        *self.current_model_name.write().await = Some(model_name.to_string());

        info!(
            "Loaded sherpa-onnx model '{}' from {}",
            model_name,
            self.models_dir.join(model_name).display()
        );
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
    /// For now this delegates to a single reqwest stream + tar::Archive; the
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
        {
            let mut cache = self.available_models.write().await;
            if let Some(m) = cache.get_mut(model_name) {
                m.status = SherpaModelStatus::Downloading { progress: 0 };
            }
        }

        let url = crate::sherpa_engine::model::SHERPA_BILINGUAL_URL;
        let target_dir = self.models_dir.join(model_name);
        if target_dir.exists() {
            // Best-effort cleanup; bail on failure to keep state consistent.
            if let Err(e) = tokio::fs::remove_dir_all(&target_dir).await {
                let mut active = self.active_downloads.write().await;
                active.remove(model_name);
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
            // Final 100% tick.
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

            // Extract the .tar.bz2 synchronously (cheap relative to download).
            let tmp_path_sync = tmp_path.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let file = std::fs::File::open(&tmp_path_sync)?;
                let bz = bzip2::read::BzDecoder::new(file);
                let mut archive = tar::Archive::new(bz);
                archive.unpack(target_dir.clone())?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow!("Extract task panicked: {}", e))??;

            // Remove the archive; final files remain.
            let _ = tokio::fs::remove_file(&tmp_path).await;
            Ok(())
        }
        .await;

        // Always clear active flag + mark status before returning.
        {
            let mut active = self.active_downloads.write().await;
            active.remove(model_name);
        }
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
