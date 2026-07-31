use super::{EmbeddingBuffer, WindowedEmbedding, EMBEDDING_DIM};
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static MODEL_STATUS: AtomicUsize = AtomicUsize::new(0); // 0=missing 1=loaded 2=failed

pub fn model_path() -> PathBuf {
    let mut p = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("meetily/speaker-camplus.onnx");
    p
}

/// Lazy hook reserved for the real sherpa-onnx loader.
///
/// In this PR-44a scaffold we only surface a status flag; the offline pass
/// (PR-44b) and the realtime caller share the same error path so that
/// `transientSpeaker` always degrades to `None` on failure.
pub fn ensure_loaded() -> Result<()> {
    match MODEL_STATUS.load(Ordering::SeqCst) {
        1 => Ok(()),
        2 => Err(anyhow!("speaker model failed to load earlier")),
        _ => {
            let path = model_path();
            if !path.exists() {
                MODEL_STATUS.store(2, Ordering::SeqCst);
                return Err(anyhow!(
                    "speaker model missing at {}; see docs/diarization_zh.md",
                    path.display()
                ));
            }
            // Real loader is added in PR-44b once the model is bundled. Until
            // then the realtime caller treats this as `Err` and skips the chip.
            MODEL_STATUS.store(1, Ordering::SeqCst);
            Ok(())
        }
    }
}

/// Stand-in embedding function used by tests and degraded realtime paths.
///
/// Real sherpa-onnx inference is wired in PR-44b; this deterministic stub
/// keeps callers compilable and gives us a unit-testable code path now.
#[allow(unreachable_code, unused_variables)]
pub fn extract_embedding(samples: &[f32], sample_rate: u32) -> Result<Vec<f32>> {
    // ponytail: stub embedder returns RMS-only vectors. All embeddings
    // land on the same axis, so cosine affinity is uniform and speaker
    // labels are arbitrary. Disabled until a real model is wired.
    return Err(anyhow!("Speaker embedding not yet implemented"));

    if samples.is_empty() {
        return Err(anyhow!("empty audio"));
    }
    if sample_rate != 16_000 {
        return Err(anyhow!("expected 16 kHz, got {}", sample_rate));
    }
    if ensure_loaded().is_err() {
        return Err(anyhow!("speaker model unavailable"));
    }
    // Deterministic stub: collapse RMS into a length-192 vector. Used only
    // when a developer explicitly stages a fake model file at the cache path.
    let mut vec = vec![0f32; EMBEDDING_DIM];
    let rms: f32 = (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt();
    vec[0] = rms;
    Ok(vec)
}

pub fn push_window(
    buffer: &EmbeddingBuffer,
    samples: &[f32],
    sample_rate: u32,
    start: f64,
    end: f64,
) -> bool {
    match extract_embedding(samples, sample_rate) {
        Ok(vec) => {
            buffer.push(WindowedEmbedding { audio_start: start, audio_end: end, vec });
            true
        }
        Err(e) => {
            log::debug!("push_window: diarization disabled: {}", e);
            false
        }
    }
}
