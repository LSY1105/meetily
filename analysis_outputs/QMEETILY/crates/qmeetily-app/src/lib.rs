//! QMeetily library crate — all real code lives here.
//!
//! Most modules are copied from meetily and simplified. New additions are
//! marked with `// qmeetily:` comments.

pub mod error;
pub mod state;
pub mod audio;
pub mod db;
pub mod commands;
pub mod mcp;

// Borrowed from meetily — verified working LLM pipeline
pub mod summary_engine;

pub use error::{AppError, Result};
pub use state::AppState;

use tauri::Manager;
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

    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let state = AppState::new(handle).await
                    .map_err(|e| format!("init failed: {e}"))?;
                app.manage(state);
                Ok::<(), String>(())
            })?;
            info!("QMeetily ready");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::get_app_info,
            commands::start_recording,
            commands::stop_recording,
            commands::list_meetings,
            commands::get_meeting,
            commands::search_meetings,
            commands::get_transcript,
            commands::generate_summary,
            commands::get_available_models,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
