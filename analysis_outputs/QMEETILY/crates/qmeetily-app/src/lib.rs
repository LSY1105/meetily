//! QMeetily library crate — all real code lives here.
//!
//! Most modules are copied from meetily and simplified. New additions are
//! marked with `// qmeetily:` comments.

pub mod error;
pub mod state;
pub mod audio;
pub mod asr;
pub mod db;
pub mod commands;
pub mod mcp;

// Borrowed from meetily — verified working LLM pipeline
pub mod summary_engine;

pub use error::{AppError, Result};
pub use state::AppState;

use tauri::Manager;
use tauri_specta::{collect_commands, Builder};
use tracing::info;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .init();

    info!("QMeetily starting");

    // Collect every `#[tauri::command]` + `#[specta::specta]` so the same
    // list drives both Tauri's invoke handler (runtime dispatch) and the
    // generated TypeScript bindings (`frontend/src/lib/bindings.ts`).
    // This replaces the previous hand-maintained list under
    // `tauri::generate_handler!` so the two can never get out of sync.
    let specta = Builder::<tauri::Wry>::new().dangerously_cast_bigints_to_number().commands(collect_commands![
        commands::ping,
        commands::get_app_info,
        commands::get_asr_sidecar_status,
        commands::start_recording,
        commands::stop_recording,
        commands::list_meetings,
        commands::get_meeting,
        commands::search_meetings,
        commands::get_transcript,
        commands::generate_summary,
        commands::get_available_models,
    ]);

    // Regenerate the frontend bindings on every dev build. Release builds
    // skip this so a stray stale file can't break a tagged release — the
    // last debug build's output is always checked in alongside the source.
    #[cfg(debug_assertions)]
    specta
        .export(
            specta_typescript::Typescript::default(),
            format!(
                "{}/../../frontend/src/lib/bindings.ts",
                env!("CARGO_MANIFEST_DIR")
            ),
        )
        .expect("failed to export TypeScript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            // setup runs inside Tauri's tokio runtime, so any `block_on` here
            // would deadlock the worker. Spawn the async init and synchronously
            // wait via a std mpsc channel (OS-level blocking, no tokio dep).
            let (tx, rx) = std::sync::mpsc::channel::<std::result::Result<AppState, AppError>>();
            tauri::async_runtime::spawn(async move {
                let _ = tx.send(AppState::new(handle).await);
            });
            let state = rx.recv()
                .map_err(|e| format!("init channel dropped: {e}"))?
                .map_err(|e| format!("init failed: {e}"))?;
            app.manage(state);
            info!("QMeetily ready");
            Ok(())
        })
        .invoke_handler(specta.invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}