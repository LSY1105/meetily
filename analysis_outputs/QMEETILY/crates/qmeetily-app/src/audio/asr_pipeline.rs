//! ASR per-chunk pipeline.
//!
//! `AudioSession`'s spawn_blocking worker (which owns the `!Send` cpal
//! stream) hands captured samples into `push_samples`. This pipeline
//! drains the buffer on a 5-second `tokio::time::interval`, uploads the
//! buffered audio to the Python ASR sidecar's
//! `/v1/audio/transcriptions` (OpenAI-compatible multipart) and inserts
//! each chunk's text into the `transcripts` table.
//!
//! All HTTP and DB I/O run on async tasks in the tauri/tokio runtime;
//! the cpal capture thread never blocks on network or DB roundtrips and
//! never holds the cpal stream across an await.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Deserialize;
use tauri::async_runtime::JoinHandle;
use tokio::sync::oneshot;

use crate::audio::WavWriter;
use crate::audio::resample::{resample_to_16k, ASR_SAMPLE_RATE};
use crate::db::Db;

const FLUSH_INTERVAL_SECS: u64 = 5;

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

pub struct AsrPipeline {
    handle: JoinHandle<()>,
    stop_tx: oneshot::Sender<()>,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: Arc<AtomicU32>,
}

impl AsrPipeline {
    pub fn start(db: Arc<Db>, asr_url: String, asr_model: String, meeting_id: i64) -> Self {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let sample_rate = Arc::new(AtomicU32::new(0));
        let (stop_tx, mut stop_rx) = oneshot::channel();
        let buf_clone = buffer.clone();
        let sr_clone = sample_rate.clone();

        let handle = tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(FLUSH_INTERVAL_SECS));
            let mut seq: i64 = 0;
            loop {
                tokio::select! {
                    _ = &mut stop_rx => {
                        // Final flush so the user gets the trailing utterance.
                        flush_once(
                            &buf_clone,
                            &sr_clone,
                            &asr_url,
                            &asr_model,
                            meeting_id,
                            &mut seq,
                            &db,
                        )
                        .await;
                        break;
                    }
                    _ = interval.tick() => {
                        flush_once(
                            &buf_clone,
                            &sr_clone,
                            &asr_url,
                            &asr_model,
                            meeting_id,
                            &mut seq,
                            &db,
                        )
                        .await;
                    }
                }
            }
        });

        Self {
            handle,
            stop_tx,
            buffer,
            sample_rate,
        }
    }

    /// Called from the audio capture thread on every captured chunk.
    /// Cheap: parking_lot lock + memcpy.
    pub fn push_samples(&self, samples: &[f32]) {
        self.buffer.lock().extend_from_slice(samples);
    }

    /// Called once the AudioCapture has been opened and we know the
    /// actual native sample rate; required before any ASR upload works.
    pub fn set_sample_rate(&self, sr: u32) {
        self.sample_rate.store(sr, Ordering::Relaxed);
    }

    /// Signal stop, drain the final buffer (handled inside the loop's
    /// stop arm), then await the task so no `INSERT` is still in flight.
    /// Takes `Arc<Self>` so the call site can invoke it on the owned
    /// Arc without first moving the inner fields out.
    pub async fn stop(self: Arc<Self>) {
        let inner = match Arc::try_unwrap(self) {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!("AsrPipeline::stop called while Arc is shared");
                return;
            }
        };
        let _ = inner.stop_tx.send(());
        let _ = inner.handle.await;
        // inner (and its buffer/sample_rate Arcs) drops here.
    }
}

async fn flush_once(
    buffer: &Arc<Mutex<Vec<f32>>>,
    sample_rate: &Arc<AtomicU32>,
    asr_url: &str,
    asr_model: &str,
    meeting_id: i64,
    seq: &mut i64,
    db: &Arc<Db>,
) {
    let samples = {
        let mut buf = buffer.lock();
        if buf.is_empty() {
            return;
        }
        std::mem::take(&mut *buf)
    };
    let sr = sample_rate.load(Ordering::Relaxed);
    if sr == 0 {
        tracing::warn!("ASR pipeline: sample_rate not set yet; dropping chunk");
        return;
    }

    // The cpal stream runs at its native rate (often 48 kHz); the sidecar
    // expects 16 kHz mono PCM, so resample before serialising the WAV.
    let asr_samples = match resample_to_16k(&samples, sr) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("ASR pipeline: resample: {e}");
            return;
        }
    };

    let wav_bytes = match build_wav_bytes(&asr_samples, ASR_SAMPLE_RATE) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("ASR pipeline: build_wav: {e}");
            return;
        }
    };

    let text = match upload(&wav_bytes, asr_url, asr_model).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("ASR pipeline: upload: {e}");
            return;
        }
    };
    if text.trim().is_empty() {
        return;
    }

    let start_ms = (*seq * (FLUSH_INTERVAL_SECS as i64) * 1000) as i32;
    let end_ms = start_ms + (FLUSH_INTERVAL_SECS as i32) * 1000;
    let now = chrono::Utc::now().timestamp_millis();
    let result = sqlx::query(
        "INSERT INTO transcripts (meeting_id, sequence_id, start_ms, end_ms, text, is_partial, created_at) \
         VALUES (?, ?, ?, ?, ?, 0, ?)",
    )
    .bind(meeting_id)
    .bind(*seq)
    .bind(start_ms)
    .bind(end_ms)
    .bind(&text)
    .bind(now)
    .execute(db.pool())
    .await;
    match result {
        Ok(_) => *seq += 1,
        Err(e) => tracing::warn!("ASR pipeline: insert transcript: {e}"),
    }
}

fn build_wav_bytes(samples: &[f32], sr: u32) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    // WavWriter::finalize() patches the header at offsets 4 and 40, so it
    // needs a Seek sink. `Cursor<&mut Vec<u8>>` gives us Write + Seek over
    // the heap buffer without round-tripping through a temp file.
    let mut cursor = std::io::Cursor::new(&mut out);
    let mut w = WavWriter::new(&mut cursor, sr, 1)?;
    w.write_samples(samples)?;
    w.finalize()?;
    Ok(out)
}

async fn upload(wav: &[u8], url: &str, model: &str) -> anyhow::Result<String> {
    let part = reqwest::multipart::Part::bytes(wav.to_vec()).file_name("chunk.wav");
    let form = reqwest::multipart::Form::new()
        .text("model", model.to_string())
        .part("file", part);
    let resp = reqwest::Client::new()
        .post(format!("{url}/v1/audio/transcriptions"))
        .multipart(form)
        .send()
        .await?;
    let parsed: TranscriptionResponse = resp.json().await?;
    Ok(parsed.text)
}

