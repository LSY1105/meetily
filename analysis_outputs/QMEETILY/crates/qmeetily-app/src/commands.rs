//! Tauri commands — thin adapters.
//!
//! Real logic lives in services. Commands here only bridge Tauri IPC to
//! the underlying modules. The old meetily `summary_engine::commands` was
//! deleted (it was tightly coupled to meetily's `State<ModelManagerState>`)
//! and replaced with these slimmer wrappers that use `Arc<AppState>`.

use std::sync::Arc;

use anyhow::anyhow;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::audio::{AsrPipeline, AudioSession};
use crate::db::meetings::Meeting;
use crate::db::transcripts::Transcript;
use crate::state::{AppState, RecordingState};
use crate::error::{AppError, Result};
use crate::summary_engine::models::ModelDef;

#[derive(Debug, Serialize, specta::Type)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub is_recording: bool,
    pub sidecar_running: bool,
    pub asr_ready: bool,
    pub llm_ready: bool,
}

#[derive(Debug, Serialize, specta::Type)]
pub struct AsrSidecarStatus {
    pub url: String,
    pub running: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn ping() -> &'static str { "pong" }

#[tauri::command]
#[specta::specta]
pub async fn get_app_info(state: State<'_, AppState>) -> Result<AppInfo> {
    let sidecar_running = state.asr_sidecar().map(|s| s.is_running()).unwrap_or(false);
    // In v0.1 the ASR pipeline IS the Python sidecar, so readiness tracks
    // the sidecar process. A future revision that adds a separate Rust
    // frontend can split this without changing the field shape.
    let asr_ready = sidecar_running;
    let llm_ready = match crate::summary_engine::models::get_model_path(
        &state.config().data_dir,
        &state.config().preferred_llm_model,
    ) {
        Ok(p) => p.exists(),
        Err(_) => false,
    };
    Ok(AppInfo {
        name: "QMeetily",
        version: env!("CARGO_PKG_VERSION"),
        is_recording: state.is_recording(),
        sidecar_running,
        asr_ready,
        llm_ready,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_asr_sidecar_status(state: State<'_, AppState>) -> Result<AsrSidecarStatus> {
    let Some(sidecar) = state.asr_sidecar() else {
        return Ok(AsrSidecarStatus { url: String::new(), running: false });
    };
    Ok(AsrSidecarStatus {
        url: sidecar.url().to_string(),
        running: sidecar.is_running(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    title: String,
) -> Result<i64> {
    use crate::db::meetings::NewMeeting;
    use chrono::Utc;

    if state.is_recording() {
        return Err(AppError::AlreadyRecording);
    }

    let new_meeting = NewMeeting {
        title,
        started_at: Utc::now(),
        language_primary: None,
        audio_path: None,
        participants: vec![],
    };
    let meeting_id = state.db().create_meeting(&new_meeting).await?;
    state.set_recording(RecordingState::Recording)?;

    // If ASR client is configured, spawn the audio capture + ASR pipeline.
    // v0.1: only start the sidecar connection; full streaming pipeline lands in v0.2.
    if let Some(asr_url) = state.config().asr_url.clone() {
        match crate::asr::build_client(&asr_url, state.config().asr_model.clone()) {
            Ok(client) => {
                if let Err(e) = client.health().await {
                    tracing::warn!("ASR sidecar not healthy at {}: {e}", asr_url);
                } else {
                    state.set_asr(Arc::new(client));
                    tracing::info!("ASR client ready at {}", asr_url);
                }
            }
            Err(e) => tracing::warn!("Failed to build ASR client: {e}"),
        }
    }

    // AudioSession owns the cpal stream + wav writer inside a
    // spawn_blocking worker; only the JoinHandle and a one-shot stop
    // signal leak back into AppState, which is why the field is Send-safe.
    let recordings_dir = state.config().data_dir.join("recordings");
    std::fs::create_dir_all(&recordings_dir)?;
    let audio_path = recordings_dir.join(format!("{meeting_id}.wav"));

    // Optional ASR pipeline: per-chunk HTTP upload + DB insert on a
    // 5-second interval, entirely in async tasks. None when no ASR URL
    // is configured (recording still produces a WAV).
    let asr = state.config().asr_url.clone().map(|url| {
        let asr_model = state.config().asr_model.clone().unwrap_or_default();
        Arc::new(AsrPipeline::start(state.db().clone(), url, asr_model, meeting_id))
    });
    state.set_audio_asr(asr.clone());
    let asr_for_session = asr;

    let session = AudioSession::start(audio_path, asr_for_session)
        .map_err(|e| AppError::Other(anyhow!("audio session start: {e}")))?;
    state.set_audio_session(Some(session));

    let _ = app.emit("recording-started", meeting_id);
    Ok(meeting_id)
}

#[tauri::command]
#[specta::specta]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    meeting_id: i64,
) -> Result<()> {
    use chrono::Utc;
    state.set_recording(RecordingState::Stopping)?;
    state.db().end_meeting(meeting_id, Utc::now()).await?;

    // Halt audio capture first; the AsrPipeline's worker will then
    // drain its final buffer on its own stop signal below.
    if let Some(session) = state.take_audio_session() {
        session.stop().await;
    }
    if let Some(asr) = state.take_audio_asr() {
        asr.stop().await;
    }

    // Persist the recorded audio path on the meeting row so the Library
    // view and ASR re-transcription can find it later.
    let audio_path = state
        .config()
        .data_dir
        .join("recordings")
        .join(format!("{meeting_id}.wav"));
    if audio_path.exists() {
        let path_str = audio_path.to_string_lossy().into_owned();
        sqlx::query("UPDATE meetings SET audio_path = ? WHERE id = ?")
            .bind(&path_str)
            .bind(meeting_id)
            .execute(state.db().pool())
            .await?;
    }

    state.set_recording(RecordingState::Idle)?;
    state.clear_asr();
    let _ = app.emit("recording-stopped", meeting_id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn list_meetings(
    state: State<'_, AppState>,
    limit: u32,
    offset: u32,
) -> Result<Vec<Meeting>> {
    state.db().list_meetings(limit, offset).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_meeting(state: State<'_, AppState>, id: i64) -> Result<Option<Meeting>> {
    state.db().get_meeting(id).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_transcript(state: State<'_, AppState>, meeting_id: i64) -> Result<Vec<Transcript>> {
    state.db().get_meeting_transcripts(meeting_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn search_meetings(
    state: State<'_, AppState>,
    query: String,
    limit: u32,
) -> Result<Vec<crate::db::transcripts::TranscriptSearchHit>> {
    state.db().search(&query, limit).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_available_models() -> Result<Vec<ModelDef>> {
    Ok(crate::summary_engine::get_available_models())
}

#[tauri::command]
#[specta::specta]
pub async fn list_model_status(
    state: State<'_, AppState>,
) -> Result<Vec<crate::summary_engine::model_manager::ModelInfo>> {
    use crate::summary_engine::model_manager::ModelManager;
    let mgr = ModelManager::new_with_models_dir(Some(state.config().models_dir))
        .map_err(AppError::Other)?;
    mgr.scan_models().await.map_err(AppError::Other)?;
    Ok(mgr.list_models().await)
}

#[tauri::command]
#[specta::specta]
pub async fn download_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model_name: String,
) -> Result<()> {
    use crate::summary_engine::model_manager::{DownloadProgress, ModelManager};
    let mgr = ModelManager::new_with_models_dir(Some(state.config().models_dir))
        .map_err(AppError::Other)?;
    mgr.scan_models().await.map_err(AppError::Other)?;
    let app_for_cb = app.clone();
    let name_for_cb = model_name.clone();
    mgr.download_model_detailed(
        &model_name,
        Some(Box::new(move |p: DownloadProgress| {
            let payload = serde_json::json!({
                "model_name": name_for_cb,
                "progress": p,
            });
            let _ = app_for_cb.emit("model-download-progress", payload);
        })),
    )
    .await
    .map_err(AppError::Other)?;
    let _ = app.emit("model-download-complete", model_name.clone());
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn generate_summary(
    app: AppHandle,
    state: State<'_, AppState>,
    meeting_id: i64,
) -> Result<String> {
    use crate::summary_engine::client::LlmClient;
    use crate::summary_engine::sidecar::SidecarManager;

    let transcripts = state.db().get_meeting_transcripts(meeting_id).await?;
    let full_text: String = transcripts
        .iter()
        .filter(|t| !t.is_partial)
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let mgr = SidecarManager::new(state.config().data_dir.clone())
        .map_err(AppError::Other)?;
    let mgr = std::sync::Arc::new(mgr);
    let client = LlmClient::new(mgr.clone(), state.config().data_dir.clone());

    let _ = app.emit("summary-progress", "");
    let model_name = state.config().preferred_llm_model.clone();
    let summary = client
        .summarize_transcript(full_text, Some(model_name.clone()), None, None)
        .await
        .map_err(AppError::Other)?;

    sqlx::query(
        "INSERT INTO summaries (meeting_id, summary_markdown, language, model, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(meeting_id)
    .bind(&summary)
    .bind("zh")
    .bind(&model_name)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(state.db().pool())
    .await?;

    mgr.shutdown().await.ok();
    let _ = app.emit("summary-ready", summary.clone());
    Ok(summary)
}




#[cfg(test)]
mod tests {
    use super::*;
    use tauri_specta::collect_commands;

    /// Compile-time lock on PR #1.5 invariants. Just constructing this list
    /// exercises `collect_commands!` for every command; the macro expansion
    /// fails if any of these regress:
    ///   * a `#[tauri::command]` is missing `#[specta::specta]`
    ///   * an arg / return type is missing `specta::Type`
    ///   * `NewMeeting` (a command arg) is missing `Serialize`
    /// The snapshot list MUST match lib.rs's production `collect_commands![]`.
    #[test]
    fn commands_carry_specta_derive() {
        let _ = collect_commands![
            ping,
            get_app_info,
            get_asr_sidecar_status,
            start_recording,
            stop_recording,
            list_meetings,
            get_meeting,
            search_meetings,
            get_transcript,
            generate_summary,
            get_available_models,
            list_model_status,
            download_model,
        ];
    }
}