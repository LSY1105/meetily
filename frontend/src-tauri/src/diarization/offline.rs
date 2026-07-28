//! Offline diarization pass: re-embeds `audio.wav` (or reuses realtime
//! windows), clusters, and writes stable `speaker` labels to SQLite.
//!
//! Triggered by `RecordingSaver::finalize()` after both `audio.wav` and
//! `audio.mp4` are written. Errors degrade silently (warn + skip) so a
//! missing model or bad wav does not break the recording itself.

use super::clustering::{remap_by_first_appearance, spectral_cluster};
use super::embedding::{ensure_loaded, extract_embedding};
use super::{WindowedEmbedding, EMBEDDING_DIM};
use anyhow::Result;
use hound::WavReader;
use sqlx::SqlitePool;
use std::path::Path;

pub async fn commit_speaker_labels(
    pool: &SqlitePool,
    meeting_id: &str,
    audio_wav: Option<&Path>,
    realtime_windows: Vec<WindowedEmbedding>,
    min_speakers: usize,
    max_speakers: usize,
) -> Result<usize> {
    let status = super::status();
    if !status.enabled {
        log::info!("diarization offline: disabled in settings; skipping");
        return Ok(0);
    }
    let min_speakers = status.min_speakers.max(2);
    let max_speakers = status.max_speakers.max(min_speakers);
    let _ = (min_speakers, max_speakers);
    if ensure_loaded().is_err() {
        log::warn!("diarization offline: model unavailable; skipping");
        return Ok(0);
    }
    let mut windows = realtime_windows;
    if windows.len() < min_speakers.max(2) {
        if let Some(path) = audio_wav {
            match reembed_wav(path) {
                Ok(w) if !w.is_empty() => windows = w,
                Ok(_) => {}
                Err(e) => {
                    log::warn!("diarization offline: re-embed wav failed ({})", e);
                }
            }
        }
    }
    if windows.is_empty() {
        log::warn!("diarization offline: no windows to cluster");
        return Ok(0);
    }
    let k = choose_k(windows.len(), min_speakers, max_speakers);
    let embeds: Vec<Vec<f32>> = windows.iter().map(|w| w.vec.clone()).collect();
    let raw = spectral_cluster(&embeds, k);
    let labels = remap_by_first_appearance(&raw);

    let segments = crate::database::repositories::transcript::TranscriptsRepository::fetch_segment_times(
        pool, meeting_id,
    )
    .await?;
    let mut mapping: Vec<(String, String)> = Vec::with_capacity(segments.len());
    for (seg_id, seg_start, seg_end) in segments {
        let mid = (seg_start + seg_end) / 2.0;
        let mut best_idx = 0usize;
        let mut best_dist = f64::MAX;
        for (i, w) in windows.iter().enumerate() {
            let d = (mid - (w.audio_start + w.audio_end) / 2.0).abs();
            if d < best_dist {
                best_dist = d;
                best_idx = i;
            }
        }
        let speaker = format!("Speaker {}", labels[best_idx] + 1);
        mapping.push((seg_id, speaker));
    }
    crate::database::repositories::transcript::TranscriptsRepository::update_segment_speakers(
        pool, &mapping,
    )
    .await?;
    Ok(mapping.len())
}

fn choose_k(n: usize, min_k: usize, max_k: usize) -> usize {
    let lo = min_k.max(2);
    let hi = max_k.max(lo);
    let guess = (n / 50).clamp(lo, hi);
    guess.min(n)
}

fn reembed_wav(path: &Path) -> Result<Vec<WindowedEmbedding>> {
    let mut reader = WavReader::open(path)?;
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / 32768.0)
        .collect();
    let sr = reader.spec().sample_rate;
    if sr != 16_000 {
        anyhow::bail!("expected 16 kHz wav, got {}", sr);
    }
    let win = (1.5 * sr as f32) as usize;
    let hop = (0.75 * sr as f32) as usize;
    let mut out = Vec::new();
    let mut start = 0usize;
    while start + win <= samples.len() {
        let end_sample = start + win;
        let emb = extract_embedding(&samples[start..end_sample], sr)?;
        if emb.len() != EMBEDDING_DIM {
            break;
        }
        let start_t = start as f64 / sr as f64;
        let end_t = end_sample as f64 / sr as f64;
        out.push(WindowedEmbedding {
            audio_start: start_t,
            audio_end: end_t,
            vec: emb,
        });
        start += hop;
    }
    Ok(out)
}
