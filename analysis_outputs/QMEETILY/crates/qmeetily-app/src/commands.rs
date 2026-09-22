//! Tauri commands — thin adapters.
//!
//! Real logic lives in services. Commands here only bridge Tauri IPC to
//! the underlying modules. The old meetily `summary_engine::commands` was
//! deleted (it was tightly coupled to meetily's `State<ModelManagerState>`)
//! and replaced with these slimmer wrappers that use `Arc<AppState>`.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::audio::{AudioCapture, CaptureConfig, WavWriter};
use crate::db::meetings::Meeting;
use crate::db::transcripts::{NewTranscript, Transcript};
use crate::state::{AppState, RecordingState};
use crate::error::Result;
use crate::summary_engine::models::ModelDef;

#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub is_recording: bool,
    pub sidecar_running: bool,
    pub asr_ready: bool,
    pub llm_ready: bool,
}

#[derive(Debug, Serialize)]
pub struct AsrSidecarStatus {
    pub url: String,
    pub running: bool,
}

#[tauri::command]
pub async fn ping() -> &'static str { "pong" }

#[tauri::command]
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

    // Audio capture: open cpal stream, spawn a worker that drains the
    // crossbeam channel and writes samples to <data_dir>/recordings/<id>.wav.
    // On stop, the cpal Stream is dropped (audio halts) and the worker
    // exits when its receiver closes; finalize() patches the WAV header.
    let recordings_dir = state.config().data_dir.join("recordings");
    std::fs::create_dir_all(&recordings_dir)?;
    let audio_path = recordings_dir.join(format!("{meeting_id}.wav"));
    let audio_path_for_worker = audio_path.clone();

    let cap = tauri::async_runtime::spawn_blocking(move || {
        AudioCapture::microphone(CaptureConfig::default())
    })
    .await
    .map_err(|e| crate::error::AppError::Other(format!("audio capture join: {e}")))?
    .map_err(|e| crate::error::AppError::Other(format!("audio capture open: {e}")))?;

    let sample_rate = cap.sample_rate();
    let rx = cap.receiver.clone();

    let worker = tauri::async_runtime::spawn_blocking(move || -> std::io::Result<()> {
        let mut w = WavWriter::create(&audio_path_for_worker, sample_rate, 1)?;
        while let Ok(chunk) = rx.recv() {
            w.write_samples(&chunk)?;
        }
        w.finalize()
    });

    state.set_audio_capture(Some(cap));
    state.set_audio_worker(Some(worker));

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

    // Halt audio capture by dropping the cpal Stream; the worker then
    // exits naturally because the crossbeam channel closes.
    let cap = state.take_audio_capture();
    drop(cap);
    if let Some(handle) = state.take_audio_worker() {
        let _ = handle.await;
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

#[tauri::command]
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
        .map_err(crate::error::AppError::Other)?;
    let mgr = std::sync::Arc::new(mgr);
    let client = LlmClient::new(mgr.clone(), state.config().data_dir.clone());

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
