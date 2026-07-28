# Design: Fix final 7 pre-existing meetily build errors

**Date**: 2026-07-28
**Branch**: devtest
**Status**: Approved

## Context

This is the third spec in a series fixing pre-existing compile errors on
devtest. The first spec
[`docs/superpowers/specs/2026-07-28-meetily-misc-build-fixes-design.md`](2026-07-28-meetily-misc-build-fixes-design.md)
(target: 10 errors) and second spec
[`docs/superpowers/specs/2026-07-28-meetily-misc-build-fixes-2-design.md`](2026-07-28-meetily-misc-build-fixes-2-design.md)
(target: 5 errors) landed cleanly. After both, cargo check still reports
**7 errors**. This spec covers those 7.

## Goal

After this spec's implementation, cargo check passes for the entire
workspace source tree (cargo errors, not TS/frontend).

## Real fixes (7 items)

### 1. `ffmpeg.rs:240` — missing lifetime specifier

```rust
pub fn path_str(path: &Path, what: &str) -> anyhow::Result<&str> {
```

The function returns `&str` (borrowed) but its signature has no lifetime
parameter. Rust 2021 requires explicit lifetimes when a function
returns a borrow whose source could be ambiguous.

**Fix**: add `<'a>` lifetime:

```rust
pub fn path_str<'a>(path: &'a Path, what: &str) -> anyhow::Result<&'a str> {
```

The lifetime is tied to `path` (not `what`, since `what` is `&str` only
used in the error message which is converted to `String` via
`format!`).

### 2. `recording_commands.rs:272` — extract `current_diarization_buffer` to module scope

The function `pub fn current_diarization_buffer() -> std::sync::Arc<...>`
is **nested inside another function body** (around line 261 the parent
function `start_recording_with_meeting_name` ends with `}`, then the
function definition follows). Nested `pub fn` items are not accessible
as `crate::module::function_name` — they can only be reached through
their enclosing scope.

**Fix**: extract the function definition to module-level (top of
`recording_commands.rs` or just below the imports). The function body
itself is correct; only its nesting needs to change.

This requires:
- Move the `pub fn current_diarization_buffer(...) { ... }` block out of
  the enclosing function
- Move any imports (`use std::sync::Arc;` inside the body) to module
  level if not already there
- Ensure `RECORDING_MANAGER` static is accessible at module level (it
  should be)

### 3. `recording_saver.rs:472, 476` — `app_state` lifetime in spawn

Lines 472-476 in `recording_saver.rs`:
```rust
if let Some(app_state) = pool_opt {
    let pool = app_state.db_manager.pool();    // line 472
    let wav_path = self.meeting_folder.as_ref().map(|f| f.join("audio.wav"));
    let windows = self.diarization_buffer.snapshot();
    let app_clone = app.clone();
    tokio::spawn(async move {                  // line 476
        ...
    });
}
```

`app_state` is owned by `pool_opt` (which is owned by the function).
When the `tokio::spawn` closure captures `pool` (borrowed from
`app_state`), the borrow must outlive the `'static` requirement of
the spawn. Since `app_state` is dropped at the end of the `if let`
block, this fails.

**Fix**: clone the pool before the spawn:

```rust
let pool_opt = app.try_state::<crate::state::AppState>();
if let Some(app_state) = pool_opt {
    let pool = app_state.db_manager.pool().clone();   // <-- add .clone()
    let wav_path = self.meeting_folder.as_ref().map(|f| f.join("audio.wav"));
    let windows = self.diarization_buffer.snapshot();
    let app_clone = app.clone();
    tokio::spawn(async move {
        let res = crate::diarization::offline::commit_speaker_labels(
            &pool,
            &meeting_id,
            wav_path.as_deref(),
            windows,
            2,
            6,
        ).await;
        // ...
    });
}
```

`SqlitePool` is internally `Arc`-based and `.clone()` is cheap. The
lifetime of the spawned task no longer depends on `app_state`.

### 4. `llm_health.rs:168` — LLMError has no `.code`/`.message` fields

```rust
LastTestResult::failed(latency_ms, e.code, &e.message).with_origin(origin)
```

`LLMError` is now a Rust **enum** with variants
(`Cancelled`, `Auth`, `ClientError { status, body }`, `ServerError { ...}`,
`Network(String)`, `JsonParse(String)`, `Other(String)`). It has no
`.code` or `.message` fields directly.

**Fix**: use `to_string()` for both. The caller wanted two strings —
a "code" identifier and a human message. Use a short identifier
derived from the variant discriminant and `to_string()` for the
message:

```rust
LastTestResult::failed(
    latency_ms,
    match &e {
        LLMError::Cancelled => "cancelled",
        LLMError::Auth => "auth",
        LLMError::ClientError { .. } => "client_error",
        LLMError::ServerError { .. } => "server_error",
        LLMError::Network(_) => "network",
        LLMError::JsonParse(_) => "json_parse",
        LLMError::Other(_) => "other",
    },
    &e.to_string(),
).with_origin(origin)
```

Or simpler if the implementer prefers:

```rust
LastTestResult::failed(latency_ms, "llm_error", &e.to_string()).with_origin(origin)
```

Either is acceptable. Pick one and document the choice in the commit
message.

### 5. `retranscription.rs:385` — `initial_prompt` not in scope (ROOT CAUSE UNKNOWN)

The error:
```
error[E0425]: cannot find value `initial_prompt` in this scope
   --> frontend/src-tauri/src/audio/retranscription.rs:385:21
    |
385 |                     initial_prompt.clone(),
    |                     ^^^^^^^^^^^^^^ not found in this scope
```

The function `start_retranscription` at line 90 has parameter
`initial_prompt: Option<String>` at line 97. The call site at line 385
is inside the same function (line 381-388). The parameter should be
in scope.

This is NOT a parser cascade error — earlier specs fixed the parser
errors in this file (spec #2 task 3) and the cascade did not clear.
The root cause must be something else.

**Possibilities**:
- A `let initial_prompt = ...` binding inside an inner closure that
  shadows the parameter — would only fail to compile if the inner
  binding is consumed before line 385, but if it exists and shadows,
  the inner scope wouldn't see the parameter.
- The function got refactored (perhaps the parameter was removed in a
  commit) but the body wasn't updated.
- The parameter was renamed but only the type signature was updated.

**Investigation required at implementation time**: read lines 90-100
and 380-390 carefully. The likely fix is one of:
- Add the parameter back to the function signature if it's missing
- Rename the parameter for consistency
- Remove a shadowing inner `let initial_prompt` binding

If after investigation the parameter IS at line 97 in scope and the
error persists, the issue is likely a different scope/closure problem
that requires deeper refactoring. Report BLOCKED with diagnostic info.

## Files Changed

| File | Change |
|---|---|
| `frontend/src-tauri/src/audio/ffmpeg.rs` | Add `<'a>` lifetime to `path_str` |
| `frontend/src-tauri/src/audio/recording_commands.rs` | Extract `current_diarization_buffer` to module scope |
| `frontend/src-tauri/src/audio/recording_saver.rs` | Add `.clone()` to `app_state.db_manager.pool()` before spawn |
| `frontend/src-tauri/src/audio/retranscription.rs` | Investigate and fix `initial_prompt` scope issue |
| `frontend/src-tauri/src/llm_health.rs` | Replace `e.code`/`&e.message` with `LLMError` enum match + `to_string()` |

## Validation

```bash
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | tee quickstart.log | tail -30
```

Expected: 0 errors. Warnings may remain (they're benign).

## Out of Scope

- whisper-rs / sys / patches — already fixed.
- llama-helper `llama-cpp-2`/`sys-2` version mismatch — separate task.
- The 11 session-start "M" files — left stashed, out of scope.
- All errors covered by spec #1 and spec #2 — already fixed.

## Non-Goals

- Not refactoring any of the affected files beyond the minimum fix.
- Not addressing test failures (only compile errors are in scope).
- Not migrating to a newer nalgebra version.
