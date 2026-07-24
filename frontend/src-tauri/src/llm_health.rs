//! PR-47: scheduled background health check for the configured LLM provider.
//!
//! Runs `run_health_check` periodically against the same code path as the
//! manual `test_llm_connection` Tauri command, tagging the result so the UI
//! can tell the two apart. No retry / backoff: the next interval tick is the
//! retry. Failures are surfaced through the same `last_test` slot a manual
//! probe would use; the ring buffer of transcript failures is never touched.
//!
//! PR-48: persists the chosen interval in the `app_settings` table so the
//! setting survives app restarts. Reads go through `load_current_interval`
//! (called by `lib.rs::init_app`); the two `#[tauri::command]` shims just
//! round-trip the value into and out of the DB.

use crate::database::repositories::setting::SettingsRepository;
use crate::llm_diagnostics::{DiagnosticsSnapshot, LastTestResult, LLMDiagnosticsState};
use crate::llm_postprocess::{generate_summary, http_client, load_provider_inputs};
use sqlx::SqlitePool;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Sentinel interval values accepted by `set_llm_health_check_interval_secs`.
pub const VALID_INTERVALS: &[u64] = &[0, 300, 600, 1800, 3600];
pub const DEFAULT_INTERVAL: u64 = 600;

/// Storage key for the persisted interval in the `app_settings` KV table.
const HEALTH_INTERVAL_KEY: &str = "llm_health_check_interval_secs";

/// Snap an arbitrary input to the nearest valid interval. `0` = disabled.
pub fn snap_interval(secs: u64) -> u64 {
    if secs == 0 {
        return 0;
    }
    // Pick the closest entry in VALID_INTERVALS (skipping 0).
    let candidates = &VALID_INTERVALS[1..];
    let mut best = candidates[0];
    let mut best_diff = secs.abs_diff(best);
    for &c in &candidates[1..] {
        let d = secs.abs_diff(c);
        if d < best_diff {
            best = c;
            best_diff = d;
        }
    }
    best
}

/// Load the persisted interval from `app_settings`, falling back to
/// `DEFAULT_INTERVAL` on missing row, parse error, or DB error.
///
/// Never returns `Err` because callers (notably `init_app`) can't usefully
/// surface a startup failure here; defaulting is the right safety net.
pub async fn load_current_interval(pool: &SqlitePool) -> u64 {
    match SettingsRepository::get_kv(pool, HEALTH_INTERVAL_KEY).await {
        Ok(Some(v)) => v
            .parse::<u64>()
            .map(snap_interval)
            .unwrap_or(DEFAULT_INTERVAL),
        _ => DEFAULT_INTERVAL,
    }
}

fn pool_from_app(app: &AppHandle) -> Result<SqlitePool, String> {
    match app.try_state::<crate::state::AppState>() {
        Some(s) => Ok(s.db_manager.pool().clone()),
        None => Err("AppState not initialized".into()),
    }
}

/// Spawn the periodic health-check task. Idempotent on `AppHandle` drop.
///
/// `interval_secs` follows the same convention as `VALID_INTERVALS`:
/// `0` = disabled (the task sleeps forever). Any other value is the
/// interval between probes.
pub fn start(app: AppHandle, interval_secs: u64) {
    if interval_secs == 0 {
        return;
    }
    tokio::spawn(async move {
        let interval = Duration::from_secs(interval_secs);
        loop {
            tokio::time::sleep(interval).await;
            // Bail if the app is shutting down (state gone).
            if app.try_state::<LLMDiagnosticsState>().is_none() {
                break;
            }
            let result = run_health_check(&app, true).await;
            // Touch result so the compiler keeps it alive across await.
            let _ = result;
        }
    });
}

/// Shared body for both the manual `test_llm_connection` Tauri command and
/// the scheduled background probe. Tags the result with `origin` so the UI
/// can tell the two apart.
///
/// Does NOT push a `record_failure` entry — this matches the existing
/// manual probe behavior. The `buckets` ring buffer is reserved for
/// transcript postprocess failures, not probe results.
pub async fn run_health_check(app: &AppHandle, scheduled: bool) -> LastTestResult {
    let origin = if scheduled { "scheduled" } else { "manual" };

    let diag = match app.try_state::<LLMDiagnosticsState>() {
        Some(d) => d,
        None => {
            return LastTestResult::failed(0, "internal", "AppState not initialized")
                .with_origin(origin);
        }
    };

    let state = match app.try_state::<crate::state::AppState>() {
        Some(s) => s,
        None => {
            let last = LastTestResult::failed(0, "internal", "AppState not initialized")
                .with_origin(origin);
            diag.set_last_test(last.clone());
            let _ = app.emit(
                "llm-diagnostics-updated",
                &DiagnosticsSnapshot {
                    buckets: diag.buckets(),
                    last_test: diag.last_test(),
                },
            );
            return last;
        }
    };

    let pool = state.db_manager.pool().clone();
    let inputs = match load_provider_inputs(&pool).await {
        Ok(v) => v,
        Err(e) => {
            let last = LastTestResult::failed(0, e.code, &e.message).with_origin(origin);
            diag.set_last_test(last.clone());
            let _ = app.emit(
                "llm-diagnostics-updated",
                &DiagnosticsSnapshot {
                    buckets: diag.buckets(),
                    last_test: diag.last_test(),
                },
            );
            return last;
        }
    };

    let (provider, model_name, api_key, ollama_endpoint, custom_openai_endpoint,
        max_tokens, temperature, top_p) = inputs;
    let start = std::time::Instant::now();
    let result = generate_summary(
        http_client(),
        &provider,
        &model_name,
        &api_key,
        "ping",
        "ping",
        ollama_endpoint.as_deref(),
        custom_openai_endpoint.as_deref(),
        Some(1).or(max_tokens),
        temperature,
        top_p,
        None,
        None,
    )
    .await;
    let latency_ms = start.elapsed().as_millis();
    let last = match result {
        Ok(_) => LastTestResult::ok(latency_ms).with_origin(origin),
        Err(e) => LastTestResult::failed(latency_ms, e.code, &e.message).with_origin(origin)
    };
    diag.set_last_test(last.clone());
    let _ = app.emit(
        "llm-diagnostics-updated",
        &DiagnosticsSnapshot {
            buckets: diag.buckets(),
            last_test: diag.last_test(),
        },
    );
    last
}

#[tauri::command]
pub async fn get_llm_health_check_interval_secs(app: AppHandle) -> Result<u64, String> {
    let pool = pool_from_app(&app)?;
    Ok(load_current_interval(&pool).await)
}

#[tauri::command]
pub async fn set_llm_health_check_interval_secs(
    app: AppHandle,
    secs: u64,
) -> Result<u64, String> {
    let pool = pool_from_app(&app)?;
    let snapped = snap_interval(secs);
    SettingsRepository::set_kv(&pool, HEALTH_INTERVAL_KEY, &snapped.to_string())
        .await
        .map_err(|e| format!("failed to persist interval: {}", e))?;
    Ok(snapped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_round_trip() {
        // Exact valid values pass through unchanged.
        for &v in VALID_INTERVALS {
            assert_eq!(snap_interval(v), v);
        }
        // Near-valid snaps to nearest entry.
        assert_eq!(snap_interval(299), 300);
        assert_eq!(snap_interval(301), 300);
        assert_eq!(snap_interval(900), 600); // closer to 600 than 1800
        assert_eq!(snap_interval(1500), 1800); // closer to 1800 than 600
        assert_eq!(snap_interval(7200), 3600);
        // Zero stays disabled.
        assert_eq!(snap_interval(0), 0);
    }
}