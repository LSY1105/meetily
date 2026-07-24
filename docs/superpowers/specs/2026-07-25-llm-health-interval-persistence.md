# PR-48 — LLM Health Check Interval Persistence

## Problem

PR-47 introduced the scheduled background health probe for the configured LLM
provider. The interval lives in an `AtomicU64` (`CURRENT_INTERVAL`) inside
`llm_health.rs` — **in-memory only**. Spec explicitly punted persistence:

> "persistence deferred to a follow-up once `SettingsRepository` grows a
> generic KV API"

This is now a real, observable bug: a user who picks `Disabled` (or `5 min`)
in the settings panel sees their setting honored until the next app restart,
at which point it reverts to the hard-coded default `600` (10 min). The
diagnostics panel still shows the last scheduled probe timestamp, which makes
the "loss on restart" feel especially confusing — the UI strongly implies the
schedule persists.

`SettingsRepository` (in `database/repositories/setting.rs`) today has only
domain-specific methods (`save_model_config`, `save_api_key`, etc.) backed
by the single-row `settings` table. Adding a generic KV API now opens the
door for any future "small piece of app-wide user preference" to land without
yet another bespoke repository method.

## Goal

- Persist the LLM health check interval across app restarts.
- Add a generic `app_settings` table and three `SettingsRepository` methods
  (`get_kv`, `set_kv`, `delete_kv`) so future PRs can reuse the same
  mechanism instead of inventing more bespoke persistence.
- Delete the in-memory `CURRENT_INTERVAL: AtomicU64` and replace it with
  reads from / writes to the new table.

## Non-goals

- No change to the probe logic itself (`start`, `run_health_check`).
- No change to the frontend (`LLMDiagnosticsPanel` keeps calling
  `get_llm_health_check_interval_secs` / `set_llm_health_check_interval_secs`
  with the same shape; behavior is identical from the UI's perspective).
- No migration of the existing in-memory default. New installs land at
  `600`; existing users also land at `600` (same default; no migration
  needed — the first `set` from the panel will persist going forward).
- No change to the `LastTestResult.origin` field, no new i18n keys.

## Backend

### New migration: `frontend/src-tauri/migrations/20260725000000_add_app_settings_table.sql`

```sql
CREATE TABLE IF NOT EXISTS app_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
```

`updated_at` is set on every write via `DEFAULT (strftime('%s', 'now'))` and
overwritten by an explicit `?` bind in the `set_kv` query. Both `INTEGER`
and `strftime('%s', 'now')` match the convention used elsewhere in the
schema (see `transcripts.ts` and friends).

### Edit: `frontend/src-tauri/src/database/repositories/setting.rs`

Add three methods to `SettingsRepository`. All async, all take `&SqlitePool`,
matching the existing method signature style.

```rust
pub async fn get_kv(
    pool: &SqlitePool,
    key: &str,
) -> std::result::Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_settings WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(v,)| v))
}

pub async fn set_kv(
    pool: &SqlitePool,
    key: &str,
    value: &str,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO app_settings (key, value, updated_at)
        VALUES (?, ?, strftime('%s', 'now'))
        ON CONFLICT(key) DO UPDATE SET
            value      = excluded.value,
            updated_at = strftime('%s', 'now')
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn delete_kv(
    pool: &SqlitePool,
    key: &str,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM app_settings WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}
```

`delete_kv` is unused by this PR but ships now so future PRs have the full
trio. `#[allow(dead_code)]` keeps the linter quiet without forcing a
sprawling future-proofing comment.

### Edit: `frontend/src-tauri/src/llm_health.rs`

Three changes, all surgical:

1. **Remove `CURRENT_INTERVAL: AtomicU64` and the `AtomicU64` / `Ordering`
   imports.**
2. **Replace the two sync `#[tauri::command]` functions with async ones**
   that read/write the DB via `SettingsRepository`. Single constant for the
   key (`const HEALTH_INTERVAL_KEY: &str = "llm_health_check_interval_secs";`)
   so a future rename touches one line.

   ```rust
   pub async fn load_current_interval(
       pool: &SqlitePool,
   ) -> u64 {
       match SettingsRepository::get_kv(pool, HEALTH_INTERVAL_KEY).await {
           Ok(Some(v)) => v.parse::<u64>().map(snap_interval).unwrap_or(DEFAULT_INTERVAL),
           _ => DEFAULT_INTERVAL,
       }
   }

   #[tauri::command]
   pub async fn get_llm_health_check_interval_secs(
       app: tauri::AppHandle,
   ) -> Result<u64, String> {
       let pool = match app.try_state::<crate::state::AppState>() {
           Some(s) => s.db_manager.pool().clone(),
           None => return Err("AppState not initialized".into()),
       };
       Ok(load_current_interval(&pool).await)
   }

   #[tauri::command]
   pub async fn set_llm_health_check_interval_secs(
       app: tauri::AppHandle,
       secs: u64,
   ) -> Result<u64, String> {
       let pool = match app.try_state::<crate::state::AppState>() {
           Some(s) => s.db_manager.pool().clone(),
           None => return Err("AppState not initialized".into()),
       };
       let snapped = snap_interval(secs);
       SettingsRepository::set_kv(&pool, HEALTH_INTERVAL_KEY, &snapped.to_string())
           .await
           .map_err(|e| format!("failed to persist interval: {}", e))?;
       Ok(snapped)
   }
   ```

3. **`start` signature unchanged** — still takes a `u64` interval. The
   caller (`lib.rs::init_app`) is now responsible for loading from DB.

### Edit: `frontend/src-tauri/src/lib.rs`

In `init_app`, replace the current

```rust
let interval = llm_health::get_llm_health_check_interval_secs();
if interval > 0 {
    llm_health::start(_app.handle().clone(), interval);
}
```

with the awaited DB read:

```rust
let interval = if let Some(state) = _app.try_state::<state::AppState>() {
    llm_health::load_current_interval(state.db_manager.pool()).await
} else {
    llm_health::DEFAULT_INTERVAL
};
if interval > 0 {
    llm_health::start(_app.handle().clone(), interval);
}
```

(The `if let Some(state)` check mirrors the existing block above that
already gates `hotword_stats::init` and `llm_postprocess::init_app` on
`AppState` being present. If `AppState` is missing — which should never
happen — fall back to the default and skip the probe, matching PR-47's
behavior.)

## Testing

- Backend:
  - Add `#[cfg(test)] mod tests` to `setting.rs` (currently absent):
    - `kv_round_trip`: `set_kv("foo", "42")` then `get_kv("foo") == Some("42")`
    - `kv_overwrite`: `set_kv` twice, second value wins
    - `kv_missing`: `get_kv("nope") == None`
    - `kv_delete`: `set_kv` then `delete_kv` then `get_kv == None`
  - Existing `llm_health::snap_interval` test stays as is.
- Manual smoke:
  1. Open Settings, pick `Disabled`. Close app. Reopen. Confirm the
     dropdown still shows `Disabled` (previously reverted to `10 min`).
  2. Pick `5 minutes`. Close. Reopen. Confirm `5 minutes` is preserved.
  3. Pick `Disabled`. Inspect `app_settings` table directly via sqlite3
     — confirm one row with `key = 'llm_health_check_interval_secs'` and
     `value = '0'`.
- CI: existing `i18n-check` (no key changes; identical).

## Risk

- **Startup latency.** `init_app` now awaits a DB read before spawning the
  probe task. The read is a single indexed `SELECT` against a 1-row table
  that will not exist on a fresh install (the migration runs first, so it
  always exists by the time we query). Worst case: one extra round-trip on
  the order of milliseconds.
- **Concurrent writers.** `set_llm_health_check_interval_secs` and the
  startup `load_current_interval` are both `&SqlitePool` (not
  `&mut SqliteConnection`). SQLite's WAL mode (assumed; verify with
  existing settings table if unsure) serializes writes — no torn reads.
- **Migration ordering.** The new migration file timestamp
  (`20260725000000_…`) sorts after every existing migration, so it runs
  last on existing databases and alongside the others on new installs. No
  data backfill required.
- **Future migrations.** Adding more keys to `app_settings` is a pure
  additive change — no schema rework needed. The KV API stays stable.

## Out of scope (explicit YAGNI)

- Migration of the default interval away from `600`. The default matches
  PR-47 and the first user-set value wins; this is fine.
- TTL / expiry on `app_settings` rows. `updated_at` is captured but not
  read by anything in this PR.
- Enumerating all keys (`list_kv` / `keys_kv`). None of the callers need
  it.
- Bumping the `app_settings` schema to namespace by user / device.
  Single-user app for now.
- A "reset to default" command. The frontend can call `delete_kv` later
  if it wants.
