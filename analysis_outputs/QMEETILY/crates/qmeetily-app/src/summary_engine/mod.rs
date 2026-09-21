//! LLM summary engine — borrowed from meetily, simplified.
//!
//! What's reused verbatim:
//!   - `models` — ModelDef registry (Qwen3.5 2B/4B, Gemma 3 1B/4B)
//!   - `model_manager` — GGUF download + cache
//!   - `sidecar` — llama-helper process lifecycle (JSON over stdin/stdout)
//!
//! What's rewritten:
//!   - `client` — removed global SIDECAR_MANAGER + Lazy<RwLock>; now uses
//!     instance-based SidecarManager passed in by the caller.
//!   - `commands` — REMOVED. Tauri command handlers live in
//!     `crate::commands`, not here. This module exposes pure functions and
//!     types only.
//!
//! Integration with qmeetily-app:
//!   - `crate::commands::generate_summary` instantiates `SidecarManager`
//!     from `AppState.config.data_dir`, runs the request, persists to DB.

pub mod models;
pub mod model_manager;
pub mod sidecar;
pub mod client;

// Pure data types — no global state, safe to re-export.
pub use models::{get_available_models, get_default_model, get_model_by_name, ModelDef, SamplingParams};
pub use model_manager::{DownloadProgress, ModelInfo, ModelManager, ModelStatus};
pub use sidecar::SidecarManager;
pub use client::{LlmClient, Request, Response};
