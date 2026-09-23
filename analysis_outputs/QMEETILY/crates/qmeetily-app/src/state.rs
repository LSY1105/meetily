//! Single source of truth: Arc<AppState>.
//!
//! No globals, no static AtomicBool, no LazyLock<Mutex>.

use std::sync::Arc;
use std::time::Duration;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::asr::{AsrClient, AsrSidecar};
use crate::audio::{AsrPipeline, AudioSession};
use crate::db::Db;
use crate::error::Result;

pub struct AppState {
    recording: RwLock<RecordingState>,
    db: Arc<Db>,
    config: RwLock<AppConfig>,
    asr: RwLock<Option<Arc<AsrClient>>>,
    asr_sidecar: Option<Arc<AsrSidecar>>,
    audio_session: parking_lot::Mutex<Option<AudioSession>>,
    audio_asr: parking_lot::Mutex<Option<Arc<AsrPipeline>>>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordingState {
    #[default]
    Idle,
    Recording,
    Stopping,
    Paused,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub models_dir: std::path::PathBuf,
    pub data_dir: std::path::PathBuf,
    pub preferred_llm_model: String,
    /// URL of the Qwen3-ASR sidecar (Python). None = ASR disabled.
    pub asr_url: Option<String>,
    /// ASR model name (sent to sidecar).
    pub asr_model: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("QMeetily");
        Self {
            models_dir: data_dir.join("models"),
            data_dir: data_dir.clone(),
            preferred_llm_model: "qwen3.5:2b".into(),
            asr_url: std::env::var("QMEETILY_ASR_URL").ok()
                .or_else(|| Some("http://127.0.0.1:11436".into())),
            asr_model: Some("Qwen/Qwen3-ASR-0.6B".into()),
        }
    }
}

impl AppState {
    pub async fn new<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<Self> {
        let config = AppConfig::default();
        std::fs::create_dir_all(&config.models_dir)?;
        std::fs::create_dir_all(&config.data_dir)?;

        let db = Arc::new(Db::open(&config.data_dir.join("qmeetily.db")).await?);
        let _ = app;

        // Spawn the ASR Python sidecar and wait for it to answer /health
        // before letting the app finish initialising. If it fails to start
        // or never becomes ready we propagate the error so the user sees
        // a startup failure instead of a silently-broken recording flow.
        let sidecar = AsrSidecar::start().and_then(|s| {
            // wait_ready is async; we have to drive it from a blocking
            // context. The future is cheap (HTTP GET + sleep loop) so
            // tauri::async_runtime::block_on is fine here.
            let s_for_block = s.clone();
            let ready = tauri::async_runtime::block_on(async move {
                s_for_block.wait_ready(Duration::from_secs(60)).await
            });
            ready.map(|_| s)
        })?;
        tracing::info!("ASR sidecar ready at {}", sidecar.url());

        Ok(Self {
            recording: RwLock::new(RecordingState::Idle),
            db,
            config: RwLock::new(config),
            asr: RwLock::new(None),
            asr_sidecar: Some(sidecar),
            audio_session: parking_lot::Mutex::new(None),
            audio_asr: parking_lot::Mutex::new(None),
        })
    }

    pub async fn new_standalone() -> Result<Self> {
        let config = AppConfig::default();
        std::fs::create_dir_all(&config.models_dir)?;
        std::fs::create_dir_all(&config.data_dir)?;
        let db = Arc::new(Db::open(&config.data_dir.join("qmeetily.db")).await?);
        Ok(Self {
            recording: RwLock::new(RecordingState::Idle),
            db,
            config: RwLock::new(config),
            asr: RwLock::new(None),
            asr_sidecar: None,
            audio_session: parking_lot::Mutex::new(None),
            audio_asr: parking_lot::Mutex::new(None),
        })
    }

    pub fn is_recording(&self) -> bool {
        matches!(*self.recording.read(), RecordingState::Recording)
    }

    pub fn set_recording(&self, state: RecordingState) -> Result<()> {
        let mut current = self.recording.write();
        match (*current, state) {
            (RecordingState::Idle, RecordingState::Recording) => *current = state,
            (RecordingState::Recording, RecordingState::Stopping)
            | (RecordingState::Recording, RecordingState::Idle)
            | (RecordingState::Stopping, RecordingState::Idle)
            | (RecordingState::Recording, RecordingState::Paused)
            | (RecordingState::Paused, RecordingState::Recording)
            | (RecordingState::Paused, RecordingState::Idle) => *current = state,
            (RecordingState::Recording, RecordingState::Recording) => {
                return Err(crate::error::AppError::AlreadyRecording);
            }
            (RecordingState::Idle, RecordingState::Idle) => {
                return Err(crate::error::AppError::NotRecording);
            }
            _ => {
                return Err(crate::error::AppError::Config(format!(
                    "invalid transition: {:?} -> {:?}",
                    *current, state
                )));
            }
        }
        Ok(())
    }

    pub fn db(&self) -> &Arc<Db> { &self.db }

    pub fn config(&self) -> AppConfig { self.config.read().clone() }

    pub fn update_config<F>(&self, f: F) where F: FnOnce(&mut AppConfig) {
        f(&mut self.config.write());
    }

    pub fn set_asr(&self, client: Arc<AsrClient>) {
        *self.asr.write() = Some(client);
    }

    pub fn asr(&self) -> Option<Arc<AsrClient>> {
        self.asr.read().clone()
    }

    pub fn clear_asr(&self) {
        *self.asr.write() = None;
    }

    pub fn asr_sidecar(&self) -> Option<&Arc<AsrSidecar>> {
        self.asr_sidecar.as_ref()
    }

    pub fn set_audio_session(&self, s: Option<AudioSession>) {
        *self.audio_session.lock() = s;
    }

    pub fn take_audio_session(&self) -> Option<AudioSession> {
        self.audio_session.lock().take()
    }

    pub fn set_audio_asr(&self, asr: Option<Arc<AsrPipeline>>) {
        *self.audio_asr.lock() = asr;
    }

    pub fn take_audio_asr(&self) -> Option<Arc<AsrPipeline>> {
        self.audio_asr.lock().take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recording_state_machine() {
        let s = AppState::new_standalone().await.unwrap();

        s.set_recording(RecordingState::Recording).unwrap();
        assert!(s.is_recording());

        s.set_recording(RecordingState::Paused).unwrap();
        assert!(!s.is_recording());

        s.set_recording(RecordingState::Recording).unwrap();
        assert!(s.is_recording());

        s.set_recording(RecordingState::Stopping).unwrap();
        s.set_recording(RecordingState::Idle).unwrap();
        assert!(!s.is_recording());

        assert!(s.set_recording(RecordingState::Recording).is_ok());

        assert!(matches!(
            s.set_recording(RecordingState::Recording),
            Err(crate::error::AppError::AlreadyRecording)
        ));
    }
}
