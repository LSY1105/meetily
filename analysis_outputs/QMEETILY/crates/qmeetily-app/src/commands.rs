//! Tauri commands — thin adapters.
//!
//! Real logic lives in services. Commands here only bridge Tauri IPC to
//! the underlying modules. The old meetily `summary_engine::commands` was
//! deleted (it was tightly coupled to meetily's `State<ModelManagerState>`)
//! and replaced with these slimmer wrappers that use `Arc<AppState>`.

use std::path::Path;
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
    list_model_status_inner(&state.config().models_dir).await
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
    let client = LlmClient::new(mgr.clone(), state.config().data_dir.clone())
        .with_app(app.clone(), state.config().preferred_llm_model.clone());

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

#[tauri::command]
#[specta::specta]
pub async fn export_meeting(
    state: State<'_, AppState>,
    meeting_id: i64,
    format: String,
) -> Result<String> {
    export_meeting_inner(state.db(), meeting_id, &format).await
}


// =====================================================================
// Inner helpers (extracted from #[tauri::command] wrappers in #2 to make
// the public commands trivially mockable from tests). Each helper takes
// only its core dependencies -- no `State<'_, AppState>`.
//
// `pub(crate)` so the `#[cfg(test)]` modules below can reach them via
// `super::*`. They are NOT part of the Tauri IPC surface.
// =====================================================================

pub(crate) async fn list_model_status_inner(
    models_dir: &Path,
) -> Result<Vec<crate::summary_engine::model_manager::ModelInfo>> {
    use crate::summary_engine::model_manager::ModelManager;
    let mgr = ModelManager::new_with_models_dir(Some(models_dir.to_path_buf()))
        .map_err(AppError::Other)?;
    mgr.scan_models().await.map_err(AppError::Other)?;
    Ok(mgr.list_models().await)
}

pub(crate) async fn export_meeting_inner(
    db: &crate::db::Db,
    meeting_id: i64,
    format: &str,
) -> Result<String> {
    let meeting = db
        .get_meeting(meeting_id)
        .await?
        .ok_or_else(|| AppError::Other(anyhow!("meeting {meeting_id} not found")))?;
    let mut transcripts = db.get_meeting_transcripts(meeting_id).await?;
    transcripts.retain(|t| !t.is_partial);
    transcripts.sort_by_key(|t| t.sequence_id);

    let summary = crate::db::summaries::get_latest(db.pool(), meeting_id).await?;

    let content = match format {
        "txt" => render_txt(&meeting, &transcripts, summary.as_deref()),
        "srt" => render_srt(&transcripts),
        "json" => {
            let payload = serde_json::json!({
                "meeting": serde_json::to_value(&meeting).map_err(AppError::Serde)?,
                "transcripts": serde_json::to_value(&transcripts).map_err(AppError::Serde)?,
                "summary": summary,
            });
            serde_json::to_string_pretty(&payload).map_err(AppError::Serde)?
        }
        "md" => render_md(&meeting, &transcripts, summary.as_deref()),
        other => {
            return Err(AppError::Other(anyhow!(
                "unknown export format: {other}; expected one of txt, srt, json, md"
            )))
        }
    };
    Ok(content)
}

fn ms_to_srt_time(ms: i32) -> String {
    let total = ms.max(0) as u64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        total / 3_600_000,
        (total % 3_600_000) / 60_000,
        (total % 60_000) / 1_000,
        total % 1_000,
    )
}

fn speaker(t: &crate::db::transcripts::Transcript) -> &str {
    t.speaker_label.as_deref().unwrap_or("Speaker")
}

fn render_txt(
    meeting: &crate::db::meetings::Meeting,
    transcripts: &[crate::db::transcripts::Transcript],
    summary: Option<&str>,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "{}", meeting.title);
    let _ = writeln!(
        out,
        "Date: {}",
        meeting.started_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    if let Some(end) = meeting.ended_at {
        let dur_min = (end - meeting.started_at).num_minutes().max(0);
        let _ = writeln!(out, "Duration: {dur_min} min");
    }
    if let Some(lang) = &meeting.language_primary {
        let _ = writeln!(out, "Language: {lang}");
    }
    if !meeting.participants.is_empty() {
        let _ = writeln!(out, "Participants: {}", meeting.participants.join(", "));
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "=== Transcript ===");
    for t in transcripts {
        let mins = t.start_ms / 60_000;
        let secs = (t.start_ms.abs() % 60_000) / 1_000;
        let text = t.rewritten_text.as_deref().unwrap_or(&t.text);
        let _ = writeln!(out, "[{mins:02}:{secs:02}] {}: {}", speaker(t), text);
    }
    if let Some(s) = summary {
        let _ = writeln!(out);
        let _ = writeln!(out, "=== Summary ===");
        let _ = writeln!(out, "{s}");
    }
    out
}

fn render_srt(transcripts: &[crate::db::transcripts::Transcript]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for (i, t) in transcripts.iter().enumerate() {
        let _ = writeln!(out, "{}", i + 1);
        let _ = writeln!(
            out,
            "{} --> {}",
            ms_to_srt_time(t.start_ms),
            ms_to_srt_time(t.end_ms)
        );
        let text = t.rewritten_text.as_deref().unwrap_or(&t.text);
        let _ = writeln!(out, "{}: {}", speaker(t), text);
        let _ = writeln!(out);
    }
    out
}

fn render_md(
    meeting: &crate::db::meetings::Meeting,
    transcripts: &[crate::db::transcripts::Transcript],
    summary: Option<&str>,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "# {}", meeting.title);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "- **Date**: {}",
        meeting.started_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    if let Some(end) = meeting.ended_at {
        let dur_min = (end - meeting.started_at).num_minutes().max(0);
        let _ = writeln!(out, "- **Duration**: {dur_min} min");
    }
    if let Some(lang) = &meeting.language_primary {
        let _ = writeln!(out, "- **Language**: {lang}");
    }
    if !meeting.participants.is_empty() {
        let _ = writeln!(out, "- **Participants**: {}", meeting.participants.join(", "));
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Transcript");
    let _ = writeln!(out);
    for t in transcripts {
        let mins = t.start_ms / 60_000;
        let secs = (t.start_ms.abs() % 60_000) / 1_000;
        let text = t.rewritten_text.as_deref().unwrap_or(&t.text);
        let _ = writeln!(out, "**`[{mins:02}:{secs:02}] {sp}`**: {text}", sp = speaker(t));
        let _ = writeln!(out);
    }
    if let Some(s) = summary {
        let _ = writeln!(out, "## Summary");
        let _ = writeln!(out);
        let _ = writeln!(out, "{s}");
    }
    out
}


// =====================================================================
// Compile-time commands list (compile-time lock on the public Tauri IPC
// surface). Keep this block in sync with `lib.rs::collect_commands![]`.
// =====================================================================
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
            export_meeting,
        ];
    }
}


// =====================================================================
// Pure-function tests for the render helpers (T1-T6).
// =====================================================================
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
mod export_tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn ts(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, mi, s).unwrap()
    }

    fn meeting_full() -> Meeting {
        Meeting {
            id: 1,
            title: "Q4 Planning Sync".to_string(),
            started_at: ts(2026, 9, 24, 10, 0, 0),
            ended_at: Some(ts(2026, 9, 24, 10, 45, 0)),
            language_primary: Some("zh".to_string()),
            audio_path: None,
            participants: vec!["Alice".to_string(), "Bob".to_string()],
        }
    }

    fn meeting_minimal() -> Meeting {
        Meeting {
            id: 2,
            title: "Quick Check".to_string(),
            started_at: ts(2026, 9, 24, 9, 0, 0),
            ended_at: None,
            language_primary: None,
            audio_path: None,
            participants: vec![],
        }
    }

    fn transcript(
        seq: i64,
        start_ms: i32,
        end_ms: i32,
        text: &str,
        rewritten: Option<&str>,
        speaker: Option<&str>,
    ) -> Transcript {
        Transcript {
            id: seq,
            meeting_id: 1,
            sequence_id: seq,
            start_ms,
            end_ms,
            text: text.to_string(),
            rewritten_text: rewritten.map(String::from),
            language: Some("zh".to_string()),
            speaker_label: speaker.map(String::from),
            confidence: Some(0.95),
            is_partial: false,
            created_at: ts(2026, 9, 24, 10, 0, 0),
        }
    }

    // -------- T1 --------
    #[test]
    fn render_txt_full_meeting_produces_expected_layout() {
        let m = meeting_full();
        let transcripts = vec![transcript(
            1, 0, 5_000, "raw hello", Some("polished hello"), Some("Alice"),
        )];
        let summary = Some("- bullet 1\n- bullet 2");
        let out = render_txt(&m, &transcripts, summary);
        assert!(out.starts_with("Q4 Planning Sync\n"), "starts with title, got: {out:?}");
        assert!(out.contains("Date: 2026-09-24 10:00:00 UTC"));
        assert!(out.contains("Duration: 45 min"));
        assert!(out.contains("Language: zh"));
        assert!(out.contains("Participants: Alice, Bob"));
        assert!(out.contains("=== Transcript ==="));
        assert!(
            out.contains("[00:00] Alice: polished hello"),
            "rewritten_text should win over text, got: {out:?}"
        );
        assert!(out.contains("=== Summary ==="));
        assert!(out.contains("- bullet 1"));
    }

    // -------- T2 --------
    #[test]
    fn render_txt_minimal_meeting_omits_optional_sections() {
        let m = meeting_minimal();
        let transcripts = vec![transcript(1, 0, 1_000, "hello", None, None)];
        let out = render_txt(&m, &transcripts, None);
        assert!(!out.contains("Duration:"));
        assert!(!out.contains("Language:"));
        assert!(!out.contains("Participants:"));
        assert!(!out.contains("=== Summary ==="));
        // speaker_label=None falls back to literal "Speaker".
        assert!(out.contains("Speaker: hello"));
    }

    // -------- T3 --------
    #[test]
    fn render_txt_prefers_rewritten_text_when_present() {
        let m = meeting_minimal();
        let t = transcript(1, 0, 1_000, "raw text", Some("polished text"), None);
        let out = render_txt(&m, std::slice::from_ref(&t), None);
        assert!(out.contains("polished text"));
        assert!(!out.contains("raw text"));
    }

    // -------- T4 --------
    #[test]
    fn render_srt_emits_indexed_blocks_in_time_order() {
        let transcripts = vec![
            transcript(1, 1_000, 3_500, "first", None, Some("Alice")),
            transcript(2, 4_000, 6_000, "second", None, Some("Bob")),
        ];
        let out = render_srt(&transcripts);
        assert!(out.contains("1\n00:00:01,000 --> 00:00:03,500\nAlice: first\n\n"));
        assert!(out.contains("2\n00:00:04,000 --> 00:00:06,000\nBob: second\n\n"));
        assert!(out.starts_with("1\n"));
        assert!(out.ends_with("\n\n"), "each SRT block must end with a blank line, got: {out:?}");
    }

    // -------- T5 --------
    #[test]
    fn render_md_starts_with_h1_and_meta_list() {
        let m = meeting_full();
        let transcripts = vec![transcript(1, 65_000, 70_000, "x", None, Some("Bob"))];
        let out = render_md(&m, &transcripts, Some("summary body"));
        assert!(out.starts_with("# Q4 Planning Sync\n"));
        assert!(out.contains("- **Date**: 2026-09-24 10:00:00 UTC"));
        assert!(out.contains("- **Duration**: 45 min"));
        assert!(out.contains("## Transcript"));
        assert!(out.contains("**[01:05] Bob**"));
        assert!(out.contains("## Summary"));
    }

    // -------- T6 --------
    #[test]
    fn ms_to_srt_time_handles_negative_zero_and_large() {
        assert_eq!(ms_to_srt_time(0), "00:00:00,000");
        assert_eq!(ms_to_srt_time(999), "00:00:00,999");
        // Negative values clamp to zero (defensive against clock skew).
        assert_eq!(ms_to_srt_time(-500), "00:00:00,000");
        assert_eq!(ms_to_srt_time(i32::MIN), "00:00:00,000");
        // 1h23m45s678ms.
        let v = 1 * 3_600_000 + 23 * 60_000 + 45 * 1_000 + 678;
        assert_eq!(ms_to_srt_time(v), "01:23:45,678");
    }
}


// =====================================================================
// DB-fixture tests (T7-T10). Requires the AGENTS.md DB exception granted
// for test fixtures only; each test creates a fresh sqlite file under a
// tempdir (no production DB touched, no tokens spent on real data).
// =====================================================================
#[cfg(test)]
mod export_db_tests {
    use super::*;
    use crate::db::meetings::NewMeeting;
    use crate::db::transcripts::NewTranscript;
    use tempfile::TempDir;

    // Bring with_ymd_and_hms into scope for chrono::Utc below.
    use chrono::TimeZone;

    /// Open an ephemeral SQLite DB inside a TempDir. Db::open runs the
    /// schema migration, so all tables exist on return. The file is
    /// cleaned up automatically when the TempDir drops.
    async fn open_temp_db() -> (TempDir, crate::db::Db) {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("qmeetily_test.sqlite");
        let db = crate::db::Db::open(&path).await.expect("open db");
        (dir, db)
    }

    async fn seed_meeting(db: &crate::db::Db) -> i64 {
        let started_at = chrono::Utc
            .with_ymd_and_hms(2026, 9, 24, 10, 0, 0)
            .unwrap();
        let id = db
            .create_meeting(&NewMeeting {
                title: "Test Meeting".to_string(),
                started_at,
                language_primary: Some("zh".to_string()),
                audio_path: None,
                participants: vec!["Alice".to_string(), "Bob".to_string()],
            })
            .await
            .expect("create meeting");

        let segments = [
            NewTranscript {
                meeting_id: id,
                sequence_id: 1,
                start_ms: 0,
                end_ms: 3_500,
                text: "hello raw".to_string(),
                rewritten_text: Some("polished hello".to_string()),
                language: Some("zh".to_string()),
                speaker_label: Some("Alice".to_string()),
                confidence: Some(0.9),
                is_partial: false,
            },
            NewTranscript {
                meeting_id: id,
                sequence_id: 2,
                start_ms: 4_000,
                end_ms: 7_000,
                text: "raw only".to_string(),
                rewritten_text: None,
                language: Some("en".to_string()),
                speaker_label: Some("Bob".to_string()),
                confidence: Some(0.8),
                is_partial: false,
            },
        ];
        for t in &segments {
            db.insert_transcript(t).await.expect("insert transcript");
        }

        sqlx::query(
            "INSERT INTO summaries (meeting_id, summary_markdown, language, model, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind("- bullet 1\n- bullet 2")
        .bind("zh")
        .bind("test-model")
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(db.pool())
        .await
        .expect("insert summary");

        id
    }

    // -------- T7 --------
    #[tokio::test]
    async fn export_meeting_inner_writes_all_four_formats() {
        let (_dir, db) = open_temp_db().await;
        let meeting_id = seed_meeting(&db).await;

        for fmt in ["txt", "srt", "json", "md"] {
            let content = export_meeting_inner(&db, meeting_id, fmt)
                .await
                .unwrap_or_else(|e| panic!("format {fmt} failed: {e}"));
            assert!(!content.is_empty(), "{fmt} returned empty content");
        }

        let txt = export_meeting_inner(&db, meeting_id, "txt")
            .await
            .unwrap();
        assert!(txt.contains("Test Meeting"));
        assert!(txt.contains("polished hello"), "txt prefers rewritten_text");

        let srt = export_meeting_inner(&db, meeting_id, "srt")
            .await
            .unwrap();
        assert!(srt.contains("00:00:00,000 --> 00:00:03,500"));
        assert!(srt.contains("00:00:04,000 --> 00:00:07,000"));

        let json = export_meeting_inner(&db, meeting_id, "json")
            .await
            .unwrap();
        assert!(json.contains("\"title\": \"Test Meeting\""));
        assert!(
            json.contains("- bullet 1"),
            "summary should land inside the JSON payload, got: {json}"
        );

        let md = export_meeting_inner(&db, meeting_id, "md")
            .await
            .unwrap();
        assert!(md.starts_with("# Test Meeting\n"));
        assert!(md.contains("## Summary"));
    }

    // -------- T8 --------
    #[tokio::test]
    async fn export_meeting_inner_errors_on_unknown_format() {
        let (_dir, db) = open_temp_db().await;
        let meeting_id = seed_meeting(&db).await;
        let err = export_meeting_inner(&db, meeting_id, "xml")
            .await
            .expect_err("xml must be rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("unknown export format: xml"),
            "error should name the bad format, got: {msg}"
        );
    }

    // -------- T9 --------
    #[tokio::test]
    async fn export_meeting_inner_errors_when_meeting_missing() {
        let (_dir, db) = open_temp_db().await;
        let err = export_meeting_inner(&db, 99_999, "txt")
            .await
            .expect_err("missing meeting must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("meeting 99999 not found"),
            "error should name the missing meeting id, got: {msg}"
        );
    }

    // -------- T10 --------
    #[tokio::test]
    async fn list_model_status_inner_reads_models_from_tempdir() {
        let dir = TempDir::new().expect("tempdir");
        let result = list_model_status_inner(dir.path())
            .await
            .expect("scan on empty tempdir should succeed");
        // Static catalog must be returned; on empty disk every model must
        // report NotDownloaded. No fabricated .gguf files required.
        assert!(!result.is_empty(), "static catalog should be returned");
        for m in &result {
            assert!(
                matches!(
                    m.status,
                    crate::summary_engine::model_manager::ModelStatus::NotDownloaded
                ),
                "model {} without file should be NotDownloaded, got {:?}",
                m.name,
                m.status
            );
        }
    }
}
