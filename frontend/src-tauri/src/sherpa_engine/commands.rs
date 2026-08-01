//! Tauri command surface for the sherpa-onnx engine.
//!
//! Mirrors `parakeet_engine::commands` so the frontend can treat sherpa as
//! a drop-in local-transcription provider alongside whisper and parakeet.

use crate::sherpa_engine::engine::{DownloadProgress, SherpaEngine};
use crate::sherpa_engine::model::{SherpaModelInfo, SherpaModelStatus, DEFAULT_SHERPA_MODEL};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{command, AppHandle, Emitter, Manager, Runtime};

// Global sherpa engine. `OnceLock` would be slightly tidier but parakeet uses
// `Mutex<Option<Arc<...>>>` and the rest of the app already matches that
// style — staying consistent.
pub static SHERPA_ENGINE: Mutex<Option<Arc<SherpaEngine>>> = Mutex::new(None);

static MODELS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Sets the root models directory. Called from `lib.rs` at app startup.
pub fn set_models_directory<R: Runtime>(app: &AppHandle<R>) {
    let app_data_dir = match app.path().app_data_dir() {
        Ok(p) => p,
        Err(e) => {
            log::error!("Failed to resolve app_data_dir for sherpa: {}", e);
            return;
        }
    };
    let models_dir = app_data_dir.join("models");
    if !models_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&models_dir) {
            log::error!("Failed to create models dir: {}", e);
            return;
        }
    }
    log::info!("Sherpa models directory set to: {}", models_dir.display());
    *MODELS_DIR.lock().unwrap() = Some(models_dir);
}

fn get_models_directory() -> Option<PathBuf> {
    MODELS_DIR.lock().unwrap().clone()
}

#[command]
pub async fn sherpa_init() -> Result<(), String> {
    let mut guard = SHERPA_ENGINE.lock().unwrap();
    if guard.is_some() {
        return Ok(());
    }
    let models_dir = get_models_directory();
    let engine = SherpaEngine::new_with_models_dir(models_dir)
        .map_err(|e| format!("Failed to init sherpa engine: {}", e))?;
    *guard = Some(Arc::new(engine));
    Ok(())
}

#[command]
pub async fn sherpa_get_available_models() -> Result<Vec<SherpaModelInfo>, String> {
    let engine = {
        SHERPA_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Sherpa engine not initialized".to_string())?
    };
    engine
        .discover_models()
        .await
        .map_err(|e| format!("sherpa discover_models: {}", e))
}

#[command]
pub async fn sherpa_load_model<R: Runtime>(
    app_handle: AppHandle<R>,
    model_name: String,
) -> Result<(), String> {
    let engine = {
        SHERPA_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Sherpa engine not initialized".to_string())?
    };

    let _ = app_handle.emit(
        "sherpa-model-loading-started",
        serde_json::json!({ "modelName": model_name }),
    );

    let result = engine.load_model(&model_name).await;

    if result.is_ok() {
        let _ = app_handle.emit(
            "sherpa-model-loading-completed",
            serde_json::json!({ "modelName": model_name }),
        );
        Ok(())
    } else {
        let err = result.unwrap_err();
        let _ = app_handle.emit(
            "sherpa-model-loading-failed",
            serde_json::json!({ "modelName": model_name, "error": err.to_string() }),
        );
        Err(format!("Failed to load sherpa model: {}", err))
    }
}

#[command]
pub async fn sherpa_get_current_model() -> Result<Option<String>, String> {
    let engine = {
        SHERPA_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Sherpa engine not initialized".to_string())?
    };
    Ok(engine.get_current_model().await)
}

#[command]
pub async fn sherpa_is_model_loaded() -> Result<bool, String> {
    let engine = {
        SHERPA_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Sherpa engine not initialized".to_string())?
    };
    Ok(engine.is_model_loaded().await)
}

#[command]
pub async fn sherpa_has_available_models() -> Result<bool, String> {
    let engine = {
        SHERPA_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Sherpa engine not initialized".to_string())?
    };
    let models = engine
        .discover_models()
        .await
        .map_err(|e| format!("sherpa discover: {}", e))?;
    Ok(models
        .iter()
        .any(|m| matches!(m.status, SherpaModelStatus::Available)))
}

/// Internal validation function used by the worker. Mirrors
/// `parakeet_validate_model_ready_with_config` so the worker can stay
/// provider-agnostic.
pub async fn sherpa_validate_model_ready_with_config<R: tauri::Runtime>(
    _app: &tauri::AppHandle<R>,
) -> Result<String, String> {
    let engine = {
        SHERPA_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Sherpa engine not initialized".to_string())?
    };

    if engine.is_model_loaded().await {
        if let Some(name) = engine.get_current_model().await {
            return Ok(name);
        }
    }

    let models = engine
        .discover_models()
        .await
        .map_err(|e| format!("sherpa discover: {}", e))?;
    let available: Vec<_> = models
        .iter()
        .filter(|m| matches!(m.status, SherpaModelStatus::Available))
        .collect();
    if available.is_empty() {
        return Err("No sherpa-onnx models available. Download one to enable real-time streaming ASR.".to_string());
    }
    let name = available
        .first()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| DEFAULT_SHERPA_MODEL.to_string());
    engine
        .load_model(&name)
        .await
        .map_err(|e| format!("Failed to load sherpa model {}: {}", name, e))?;
    Ok(name)
}

#[command]
pub async fn sherpa_transcribe_audio(audio_data: Vec<f32>) -> Result<String, String> {
    let engine = {
        SHERPA_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Sherpa engine not initialized".to_string())?
    };
    engine
        .transcribe_audio(audio_data)
        .await
        .map_err(|e| format!("sherpa transcribe: {}", e))
}

#[command]
pub async fn sherpa_get_models_directory() -> Result<String, String> {
    let engine = {
        SHERPA_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Sherpa engine not initialized".to_string())?
    };
    Ok(engine.get_models_directory().await.to_string_lossy().to_string())
}

#[command]
pub async fn sherpa_download_model<R: Runtime>(
    app_handle: AppHandle<R>,
    model_name: String,
) -> Result<(), String> {
    let engine = {
        SHERPA_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Sherpa engine not initialized".to_string())?
    };

    let app_handle_clone = app_handle.clone();
    let model_name_clone = model_name.clone();
    // Wrap in Arc<Mutex<>> to satisfy the `Send + Sync` bound on the engine
    // callback type — the engine forwards the callback to a `spawn_blocking`
    // task that has a `Sync` future bound.
    let cb: Box<dyn Fn(DownloadProgress) + Send + Sync> = {
        let app_handle = app_handle_clone;
        let model_name = model_name_clone;
        Box::new(move |p: DownloadProgress| {
            let _ = app_handle.emit(
                "sherpa-model-download-progress",
                serde_json::json!({
                    "modelName": model_name,
                    "progress": p.percent,
                    "downloaded_bytes": p.downloaded_bytes,
                    "total_bytes": p.total_bytes,
                    "downloaded_mb": p.downloaded_mb,
                    "total_mb": p.total_mb,
                    "speed_mbps": p.speed_mbps,
                    "status": if p.percent == 100 { "completed" } else { "downloading" }
                }),
            );
        })
    };

    // Re-discover so the cache is fresh before download.
    let _ = engine.discover_models().await;

    let result = engine.download_model_detailed(&model_name, Some(cb)).await;

    match result {
        Ok(()) => {
            let _ = app_handle.emit(
                "sherpa-model-download-complete",
                serde_json::json!({ "modelName": model_name }),
            );
            Ok(())
        }
        Err(e) => {
            let _ = app_handle.emit(
                "sherpa-model-download-error",
                serde_json::json!({ "modelName": model_name, "error": e.to_string() }),
            );
            Err(format!("sherpa download failed: {}", e))
        }
    }
}

#[command]
pub async fn sherpa_cancel_download<R: Runtime>(
    app_handle: AppHandle<R>,
    model_name: String,
) -> Result<(), String> {
    let engine = {
        SHERPA_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Sherpa engine not initialized".to_string())?
    };
    engine
        .cancel_download(&model_name)
        .await
        .map_err(|e| format!("sherpa cancel: {}", e))?;
    let _ = app_handle.emit(
        "sherpa-model-download-progress",
        serde_json::json!({
            "modelName": model_name,
            "progress": 0,
            "status": "cancelled"
        }),
    );
    Ok(())
}

#[command]
pub async fn sherpa_delete_corrupted_model(model_name: String) -> Result<String, String> {
    let engine = {
        SHERPA_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Sherpa engine not initialized".to_string())?
    };
    engine
        .delete_model(&model_name)
        .await
        .map_err(|e| format!("sherpa delete: {}", e))
}

#[command]
pub async fn open_sherpa_models_folder() -> Result<(), String> {
    let models_dir = get_models_directory()
        .ok_or_else(|| "Sherpa models directory not initialized".to_string())?
        .join("sherpa");
    if !models_dir.exists() {
        std::fs::create_dir_all(&models_dir)
            .map_err(|e| format!("Failed to create dir: {}", e))?;
    }
    let path = models_dir.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("explorer: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("open: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("xdg-open: {}", e))?;
    }
    Ok(())
}
