//! Tauri commands — thin adapters.
//!
//! All real logic lives in services. Commands here only bridge Tauri IPC to
//! the underlying modules. The old meetily `summary_engine::commands` was
//! deleted (it was tightly coupled to meetily's `State<ModelManagerState>`)
//! and replaced with these slimmer wrappers that use `Arc<AppState>`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::db::meetings::Meeting;
use crate::db::transcripts::Transcript;
use crate::state::{AppState, RecordingState};
use crate::error::Result;
use crate::summary_engine::{ModelDef, SidecarManager, LlmClient};

#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub is_recording: bool,
}

#[tauri::command]
pub async fn ping() -> &'static str { "pong" }

#[tauri::command]
pub async fn get_app_info(state: State<'_, AppState>) -> Result<AppInfo> {
    Ok(AppInfo {
        name: "QMeetily",
        version: env!("CARGO_PKG_VERSION"),
        is_recording: state.is_recording(),
    })
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    title: String,
) -> Result<i64> {
    use crate::db::meetings::NewMeeting;
    use chrono::Utc;

    if state.is_recording() {
        return Err(crate::error::AppError::AlreadyRecording);
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
    let _ = app.emit("recording-started", meeting_id);
    Ok(meeting_id)
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    meeting_id: i64,
) -> Result<()> {
    use chrono::Utc;
    state.set_recording(RecordingState::Stopping)?;
    state.db().end_meeting(meeting_id, Utc::now()).await?;
    state.set_recording(RecordingState::Idle)?;
    let _ = app.emit("recording-stopped", meeting_id);
    Ok(())
}

#[tauri::command]
pub async fn list_meetings(
    state: State<'_, AppState>,
    limit: u32,
    offset: u32,
) -> Result<Vec<Meeting>> {
    state.db().list_meetings(limit, offset).await
}

#[tauri::command]
pub async fn get_meeting(state: State<'_, AppState>, id: i64) -> Result<Option<Meeting>> {
    state.db().get_meeting(id).await
}

#[tauri::command]
pub async fn get_transcript(state: State<'_, AppState>, meeting_id: i64) -> Result<Vec<Transcript>> {
    state.db().get_meeting_transcripts(meeting_id).await
}

#[tauri::command]
pub async fn search_meetings(
    state: State<'_, AppState>,
    query: String,
    limit: u32,
) -> Result<Vec<crate::db::transcripts::TranscriptSearchHit>> {
    state.db().search(&query, limit).await
}

#[tauri::command]
pub async fn get_available_models() -> Result<Vec<ModelDef>> {
    Ok(crate::summary_engine::get_available_models())
}

/// Generate a summary using the local llama-helper sidecar.
///
/// Spawns the sidecar if not running, streams generation, persists to DB.
/// This is the main LLM call from the UI.
#[tauri::command]
pub async fn generate_summary(
    app: AppHandle,
    state: State<'_, AppState>,
    meeting_id: i64,
) -> Result<String> {
    let transcripts = state.db().get_meeting_transcripts(meeting_id).await?;
    let full_text: String = transcripts
        .iter()
        .filter(|t| !t.is_partial)
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let data_dir = state.config().data_dir.clone();
    let mgr = SidecarManager::new(data_dir.clone()).map_err(crate::error::AppError::Other)?;
    let mgr = std::sync::Arc::new(mgr);
    let client = LlmClient::new(mgr.clone(), data_dir);

    let _ = app.emit("summary-progress", "");
    let model_name = state.config().preferred_llm_model.clone();
    let summary = client
        .summarize_transcript(full_text, Some(model_name.clone()), None, None)
        .await
        .map_err(crate::error::AppError::Other)?;

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
