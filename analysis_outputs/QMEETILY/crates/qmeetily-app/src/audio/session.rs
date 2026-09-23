//! Audio session — owns a cpal capture stream for the duration of a meeting.
//!
//! The cpal Stream type is `!Send` on Windows (it pins to the audio thread
//! via COM). Storing an `AudioCapture` directly in `AppState` would prevent
//! `AppState: Send`, which Tauri requires for `app.manage(state)`.
//!
//! `AudioSession` keeps the capture and the WAV writer inside a
//! `spawn_blocking` worker; the only Send-safe handle exposed to `AppState`
//! is the `JoinHandle` and a one-shot stop signal. `stop()` sends the
//! signal and awaits the worker, which drops the cpal Stream (audio halts)
//! and finalises the WAV file before exiting.
//!
//! If an `AsrPipeline` is supplied, every captured chunk is forwarded to
//! it via `push_samples`. The ASR upload + DB insert runs entirely in
//! async tasks, so the cpal capture thread never blocks on I/O.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crossbeam::channel::RecvTimeoutError;
use tauri::async_runtime::JoinHandle;
use tokio::sync::oneshot;

use crate::audio::{self as audio_mod, AsrPipeline, AudioCapture, CaptureConfig};

pub struct AudioSession {
    handle: JoinHandle<()>,
    stop_tx: oneshot::Sender<()>,
}

impl AudioSession {
    pub fn start(audio_path: PathBuf, asr: Option<Arc<AsrPipeline>>) -> anyhow::Result<Self> {
        let (stop_tx, mut stop_rx) = oneshot::channel();
        let path_for_worker = audio_path.clone();
        let asr_for_worker = asr.clone();
        let handle = tauri::async_runtime::spawn_blocking(move || {
            let cap = match AudioCapture::microphone(CaptureConfig::default()) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("audio capture open failed: {e}");
                    return;
                }
            };
            let sample_rate = cap.sample_rate();
            let rx = cap.receiver.clone();
            if let Some(a) = asr_for_worker.as_ref() {
                a.set_sample_rate(sample_rate);
            }

            // Hold the capture in this scope so the cpal stream stays alive
            // until we exit; the crossbeam receiver closes when `cap` drops
            // and the WAV writer's loop then terminates on the next recv.
            let result = (|| -> std::io::Result<()> {
                let mut w = audio_mod::wav::create(&path_for_worker, sample_rate, 1)?;
                loop {
                    match rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(chunk) => {
                            w.write_samples(&chunk)?;
                            if let Some(a) = asr_for_worker.as_ref() {
                                a.push_samples(&chunk);
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            if stop_rx.try_recv().is_ok() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                w.finalize()
            })();

            drop(cap);
            if let Err(e) = result {
                tracing::error!("audio worker exit: {e}");
            }
        });

        Ok(Self { handle, stop_tx })
    }

    /// Signal the worker to stop, then await it so the WAV header is
    /// finalised before this method returns.
    pub async fn stop(self) {
        let _ = self.stop_tx.send(());
        let _ = self.handle.await;
    }
}
