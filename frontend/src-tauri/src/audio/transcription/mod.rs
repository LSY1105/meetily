// audio/transcription/mod.rs
//
// Transcription module: Provider abstraction, engine management, and worker pool.

pub mod provider;
pub mod whisper_provider;
pub mod parakeet_provider;
// Sherpa-onnx streaming provider. Same cfg-gate as the `sherpa_engine`
// crate: compiled out on aarch64-pc-windows-msvc where upstream
// prebuilt libs are missing.
#[cfg(not(all(target_os = "windows", target_arch = "aarch64")))]
pub mod sherpa_provider;
pub mod engine;
pub mod worker;

// Re-export commonly used types
pub use provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
pub use whisper_provider::WhisperProvider;
pub use parakeet_provider::ParakeetProvider;
#[cfg(not(all(target_os = "windows", target_arch = "aarch64")))]
pub use sherpa_provider::SherpaProvider;
pub use engine::{
    TranscriptionEngine,
    validate_transcription_model_ready,
    get_or_init_transcription_engine,
    get_or_init_whisper
};
pub use worker::{
    start_transcription_task,
    start_streaming_task,
    reset_speech_detected_flag,
    TranscriptUpdate
};
