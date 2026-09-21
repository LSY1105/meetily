//! Typed AppError. NO unwrap, NO panic on hot paths.
//!
//! All fallible functions return Result<T, AppError>. AppError implements
//! From for common causes (sqlx, reqwest, serde_json, etc).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("audio: {0}")]
    Audio(String),

    #[error("ASR: {0}")]
    Asr(String),

    #[error("LLM: {0}")]
    Llm(String),

    #[error("sidecar: {0}")]
    Sidecar(String),

    #[error("database: {0}")]
    Database(#[from] sqlx::Error),

    #[error("HTTP: {0}")]
    Http(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("config: {0}")]
    Config(String),

    #[error("not initialized: {0}")]
    NotInitialized(&'static str),

    #[error("recording already in progress")]
    AlreadyRecording,

    #[error("recording not in progress")]
    NotRecording,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, AppError>;

// Ergonomic conversions
impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Http(e.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Other(anyhow::anyhow!("rusqlite: {e}"))
    }
}

// Tauri requires errors to be Serialize for commands.
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
