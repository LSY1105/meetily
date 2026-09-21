//! Single source of truth: Arc<AppState>.
//!
//! Borrowed from meetily, simplified: no globals, no static AtomicBool,
//! no LazyLock<Mutex>. The llama-helper sidecar (LLM) is spawned on demand
//! from `commands::generate_summary` via `summary_engine::sidecar::SidecarManager`.

use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::Result;

pub struct AppState {
    recording: RwLock<RecordingState>,
    db: Arc<Db>,
    config: RwLock<AppConfig>,
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
}

impl Default for AppConfig {
    fn default() -> Self {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("QMeetily");
        Self {
            models_dir: data_dir.join("models"),
            data_dir: data_dir.clone(),
            // Default to qwen3.5:2b — small, fast, 1.2GB; works on 8GB RAM.
            // User can switch to qwen3.5:4b (2.6GB) in Settings.
            preferred_llm_model: "qwen3.5:2b".into(),
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

        Ok(Self {
            recording: RwLock::new(RecordingState::Idle),
            db,
            config: RwLock::new(config),
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
