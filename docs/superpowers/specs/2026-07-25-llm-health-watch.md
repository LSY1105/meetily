# PR-47 — LLM Provider Health Watch (scheduled background probe)

## Problem

`test_llm_connection` (PR-45a) only runs when the user clicks a button. If the
LLM provider's API key gets revoked, the upstream rate-limits the user, or the
provider goes down between meetings, the user has no warning — they only find
out at the next transcript postprocess, several minutes into a meeting.

We have every building block except the trigger:
- `LLMDiagnosticsState` (in-memory ring buffer + `last_test` slot)
- `test_llm_connection` Tauri command (calls `generate_summary` with a `"ping"`
  input against the configured provider)
- `tokio` 1.32 + `tokio-util` (for `time::interval` + `CancellationToken`)
- `tauri-plugin-notification` (already in `Cargo.toml`, unused for this PR)
- `LLMDiagnosticsPanel` UI (PR-45b) already shows `last_test`

This PR wires a periodic background probe on top of those, with a settings
toggle and a UI marker so the user can see "last scheduled check" without
spamming notifications.

## Goal

- A background tokio task starts at app launch and runs `test_llm_connection`
  every N seconds (default 600 = 10 minutes), where N is configurable.
- The probe result is written to the same `last_test` slot as the manual
  probe, tagged with `origin: 'scheduled'` so the UI can distinguish.
- The probe never pushes to the `buckets` ring buffer (manual probes don't
  either — they share this property; see `llm_postprocess.rs:301-302`).
- Settings page gains a `Health check interval` dropdown with options:
  `Disabled`, `5 min`, `10 min` (default), `30 min`, `1 hour`.
- `LLMDiagnosticsPanel` shows the last scheduled run timestamp separately
  from the manual probe timestamp (single new line; no layout shift).

## Non-goals

- No new dependencies. `tokio::time::interval` is enough.
- No retry-with-backoff on probe failure. One shot; the next interval tick
  retries. Failure semantics are identical to a manual probe failure.
- No push notifications on probe failure. The user opts in by opening the
  diagnostics panel; we do not interrupt.
- No per-meeting probe. App-wide only.
- No background probe during active recording (the existing `tokio` runtime
  is shared with the recording pipeline; the probe's `"ping"` call adds a
  few hundred ms of latency to the postprocess queue at most, and the
  existing `CancellationToken` machinery already handles this).

## Backend

### New file: `frontend/src-tauri/src/llm_health.rs`

Single small module, ~70 lines. Two public items:

```rust
pub fn start(app: tauri::AppHandle, interval_secs: u64)
```

`start` is called once from `lib.rs::init_app`. It spawns a
`tokio::spawn` task that loops:

```rust
loop {
    tokio::time::sleep(Duration::from_secs(interval_secs)).await;
    run_health_check(&app, /* scheduled */ true).await;
}
```

The task holds an `Arc<()>` to detect `AppHandle` drop and bail.

```rust
pub async fn run_health_check(app: &AppHandle, scheduled: bool) -> LastTestResult
```

`run_health_check` is the extracted shared body. It does exactly what
`test_llm_connection` does today (load provider inputs → call
`generate_summary` with `"ping"` → set `last_test` → emit
`llm-diagnostics-updated`), but it tags the result with
`origin = if scheduled { "scheduled" } else { "manual" }`. It does NOT push
a `record_failure` entry (consistent with the current manual probe).

### Edit: `frontend/src-tauri/src/llm_diagnostics.rs`

Extend `LastTestResult` with an `origin: &'static str` field, serialized
as `"manual"` or `"scheduled"`. Add a `LastTestResult::scheduled(...)`
constructor alongside the existing `ok` / `failed`.

No other field changes. `DiagnosticsSnapshot` already carries `last_test`,
so the frontend gets the new field transparently.

### Edit: `frontend/src-tauri/src/llm_postprocess.rs`

Replace `test_llm_connection`'s inline body with a call to
`llm_health::run_health_check(&app, false)`. Returns the same
`LastTestResult`. Net diff: ~40 lines removed, 2 lines added. No behavior
change for existing callers (`test_llm_connection` keeps its name + signature
+ return type).

### Edit: `frontend/src-tauri/src/llm_postprocess.rs` (settings command)

Add a new `#[tauri::command]` `get_llm_health_check_interval_secs() -> u64`
and `set_llm_health_check_interval_secs(secs: u64)`:

- Persist into the existing `app_settings` table (single new row, key
  `llm_health_check_interval_secs`).
- `0` = disabled. `5` / `600` / `1800` / `3600` are the only valid values;
  anything else round-trips to `600`.

### Edit: `frontend/src-tauri/src/lib.rs`

In `init_app`, after the diagnostics state is registered, read the stored
interval (defaulting to `600` if absent) and call
`llm_health::start(app_handle, interval_secs)`.

Also add the new commands to `invoke_handler!`:

```rust
llm_postprocess::set_llm_health_check_interval_secs,
llm_postprocess::get_llm_health_check_interval_secs,
```

### i18n — 4 new keys × 6 locales

```
settings.transcript.llm.health_check_interval_label   "Health check interval"
settings.transcript.llm.health_check_interval_opt_disabled   "Disabled"
settings.transcript.llm.health_check_interval_opt_5min       "Every 5 minutes"
settings.transcript.llm.health_check_interval_opt_10min      "Every 10 minutes"
settings.transcript.llm.health_check_interval_opt_30min      "Every 30 minutes"
settings.transcript.llm.health_check_interval_opt_1h         "Every hour"
settings.transcript.llm.last_scheduled_run_label   "Last scheduled check: {when}"
```

(7 keys total: 1 label + 5 dropdown options + 1 timestamp label.)

| locale | last_scheduled_run_label |
|---|---|
| en-US / en-GB | Last scheduled check: {when} |
| zh-CN | 上次定时检查：{when} |
| zh-TW | 上次排程檢查：{when} |
| ja-JP | 最後の定期チェック：{when} |
| ko-KR | 마지막 정기 확인: {when} |

The dropdown option strings are auto-translated; translations table omitted
for brevity (English copy is the source of truth, just like PR-46a/b).

All 7 keys go under `settings.transcript.llm.diagnostics` per the
PR-46a/b pattern. Splice in after `endpoint_unreachable`.

## Frontend

### Edit: `frontend/src/components/TranscriptSettings.tsx`

Add a `<Label>Health check interval</Label>` + `<Select>` block, placed
right after the existing `requiresApiKey` toggle. Reads / writes via
`get_llm_health_check_interval_secs` / `set_llm_health_check_interval_secs`.
Select value is one of `'disabled' | '5min' | '10min' | '30min' | '1h'` —
mapped client-side to the integer the backend stores.

### Edit: `frontend/src/components/LLMDiagnosticsPanel.tsx`

The panel already shows `last_test_ok` / `last_test_failed` with a
relative timestamp. Add one new line directly under, conditional on
`snapshot.last_test?.origin === 'scheduled'`:

```tsx
{snapshot.last_test?.origin === 'scheduled' && (
  <p className="text-xs text-muted-foreground">
    {tSettings('transcript.llm.last_scheduled_run_label', {
      when: formatRelative(snapshot.last_test.ts),
    })}
  </p>
)}
```

No layout shift; existing layout accommodates one extra muted line.

### Edit: `frontend/src/types/index.ts`

Mirror the backend `LastTestResult.origin` field in the TS interface:

```ts
export interface LastTestResult {
  ok: boolean;
  latency_ms: number;
  code?: string;
  message?: string;
  ts: string;
  origin?: 'manual' | 'scheduled';   // PR-47
}
```

The `?` keeps backward compatibility with old snapshots serialized before
this PR.

## Testing

- Backend:
  - New `#[test]` in `llm_health.rs`:
    - `interval_round_trip`: serializes / deserializes the four valid values
      correctly; rejects `7` and `12345` by rounding to the nearest valid
      value or to `600`.
    - `run_health_check_with_no_app_state`: returns
      `LastTestResult::failed(0, "internal", "AppState not initialized")`
      without panicking. Uses a mock `AppHandle`-less call site (the helper
      takes `&AppHandle`, so the test goes through a thin wrapper that
      accepts `Option<&AppHandle>`).
  - Existing `llm_diagnostics` tests still pass (the new `origin` field is
    a serialization add; no behavior change in the ring buffer).
- Frontend:
  - `useLLMDiagnostics` already exposes the snapshot unchanged. No new
    hook.
  - `LLMDiagnosticsPanel` change is a 5-line conditional render. No new
    test; verified by manual smoke + the existing `use-locale.test.tsx`.
- Manual smoke (3 scenarios):
  1. Open Settings. Set Health check interval to `5 minutes`. Restart the
     app. Wait 5 minutes. Open `LLMDiagnosticsPanel`. Confirm
     `Last scheduled check: just now` is visible and `last_test` reflects
     a fresh timestamp.
  2. Set interval to `Disabled`. Restart. Wait 10 minutes. Confirm the
     "Last scheduled check" line never appears (or only the prior manual
     probe shows).
  3. While a meeting is recording, watch CPU / network. Confirm the probe
     runs on schedule (network tab shows a request to the configured LLM
     every interval) without breaking recording.
- CI: existing `i18n-check` proves 6-locale parity for the 7 new keys.

## Risk

- **Probe latency during recording.** `generate_summary("ping", ...)` takes
  the same path as a real transcript postprocess — 200-2000 ms typically.
  It runs on the shared tokio runtime, so it queues behind any in-flight
  recording task. Worst case: one extra ~1 s hitch every N minutes during
  recording. Acceptable; users already tolerate worse during normal
  postprocess.
- **API key revocation mid-probe.** If the provider's auth flips between
  start and probe, the probe returns `LastTestResult::failed(AUTH_FAILED)`
  and we surface it the same way a manual probe would. No data loss; the
  user can re-save the config.
- **Cron drift across DST / sleep.** Not applicable — we use a fixed
  interval, not a wall-clock schedule. The probe fires every N seconds of
  runtime, not at HH:MM.
- **Settings round-trip mismatch.** Frontend `<Select>` value `'5min'` maps
  to backend `5`; backend validates and may snap to `600` if the user
  passes an unknown value via DevTools. The frontend only ever sends the
  four documented strings, so this is a defensive guard, not a real path.

## Out of scope (explicit YAGNI)

- Probe result history (last N scheduled runs) — current `last_test` is
  enough; ring buffer is for transcript failures.
- Per-provider override interval (one global setting is fine).
- Probe cancellation on system sleep / suspend.
- Adaptive intervals (slower when idle, faster after a failure). One knob.
- Slack / webhook / email alerting on probe failure.
- Surface probe status in the OS tray.
