# Misc Build Fixes #3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the final 7 pre-existing compile errors in devtest so cargo check passes cleanly.

**Architecture:** Single-file mechanical fixes for lifetime issues, nested-fn extraction, LLMError enum access, and one unknown root-cause fix that requires investigation at implementation time.

**Tech Stack:** Rust (cargo), Tauri 2.x.

**Spec:** `docs/superpowers/specs/2026-07-28-meetily-misc-build-fixes-3-design.md`

**Working directory:** `C:\Users\qjl10\Documents\工作区\meetily` (Windows, bash shell)

**Prerequisite:** Spec #1 and spec #2 must have already landed (they did, commits `bd734fb` and `57026ab`).

---

## File Structure

| File | Responsibility | Touched by |
|---|---|---|
| `frontend/src-tauri/src/audio/ffmpeg.rs` | path_str lifetime | Task 1 |
| `frontend/src-tauri/src/audio/recording_commands.rs` | extract nested fn | Task 2 |
| `frontend/src-tauri/src/audio/recording_saver.rs` | spawn lifetime | Task 3 |
| `frontend/src-tauri/src/audio/retranscription.rs` | initial_prompt scope (investigate) | Task 4 |
| `frontend/src-tauri/src/llm_health.rs` | LLMError enum access | Task 5 |

---

## Task 1: Add lifetime to ffmpeg.rs path_str

**Files:**
- Modify: `frontend/src-tauri/src/audio/ffmpeg.rs:240`

- [ ] **Step 1: Verify current state**

```bash
awk 'NR>=238 && NR<=245' frontend/src-tauri/src/audio/ffmpeg.rs
```

Expected:
```rust
/// Three callsites in the codebase hit the same OsStr->&str dance; this keeps
/// the error messages uniform.
pub fn path_str(path: &Path, what: &str) -> anyhow::Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("{} path is not valid UTF-8: {}", what, path.display()))
```

- [ ] **Step 2: Apply the edit**

```rust
old_string: pub fn path_str(path: &Path, what: &str) -> anyhow::Result<&str> {
new_string: pub fn path_str<'a>(path: &'a Path, what: &str) -> anyhow::Result<&'a str> {
```

The lifetime `'a` ties the returned `&str` to `path` (since `path.to_str()`
returns a borrow of the path's contents).

- [ ] **Step 3: Verify the edit**

```bash
awk 'NR==240' frontend/src-tauri/src/audio/ffmpeg.rs
```

Expected: `pub fn path_str<'a>(path: &'a Path, what: &str) -> anyhow::Result<&'a str> {`

- [ ] **Step 4: Verify ffmpeg.rs compiles clean**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "ffmpeg\.rs" | head -5
```

Expected: no errors referencing `ffmpeg.rs`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/audio/ffmpeg.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(ffmpeg): add explicit lifetime to path_str

Return type Result<&str> borrows from path argument; without
lifetime annotation Rust cannot infer which input the borrow is
tied to.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Extract current_diarization_buffer to module scope

**Files:**
- Modify: `frontend/src-tauri/src/audio/recording_commands.rs:272` (extract from inside another function)

- [ ] **Step 1: Read the full context to confirm nesting**

```bash
awk 'NR>=255 && NR<=290' frontend/src-tauri/src/audio/recording_commands.rs
```

Expected: you'll see the parent function ending around line 261-267, then `///` doc comment, then `pub fn current_diarization_buffer() -> std::sync::Arc<crate::diarization::EmbeddingBuffer>` at line 272.

- [ ] **Step 2: Find a good location to put the extracted function**

Look at the top of `recording_commands.rs` (after imports, before any other `pub fn`). Insert the function there.

The function body (lines 273-285 ish) uses:
- `use std::sync::Arc;` (inside the function body — this `use` is local)
- `RECORDING_MANAGER.lock()` — module-level static
- `manager.diarization_buffer()` — method call

Move `use std::sync::Arc;` to module level (near other `use` statements at the top of the file). If `std::sync::Arc` is already imported at module level, you can drop the inner `use`.

- [ ] **Step 3: Cut the function from inside its parent and paste at module scope**

Edit the file:
1. Delete the entire `pub fn current_diarization_buffer() -> ... { ... }` block from where it currently sits (inside another function body).
2. Paste it at the chosen module-level location, with the doc comment `///` preceding it.

Verify the function body remains identical; only the indentation and nesting change.

- [ ] **Step 4: Verify recording_commands.rs and worker.rs compile clean**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "recording_commands\.rs|transcription\\worker\.rs:219" | head -10
```

Expected: no errors. The error at `worker.rs:219` (`cannot find function current_diarization_buffer`) should disappear.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/audio/recording_commands.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(recording_commands): extract current_diarization_buffer to module scope

The pub fn was nested inside another function body, making it
inaccessible as crate::audio::recording_commands::function_name.
Extract to module-level so the worker can call it.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Fix app_state lifetime in recording_saver.rs spawn

**Files:**
- Modify: `frontend/src-tauri/src/audio/recording_saver.rs:472`

- [ ] **Step 1: Verify current state**

```bash
awk 'NR>=469 && NR<=490' frontend/src-tauri/src/audio/recording_saver.rs
```

Expected: `let pool = app_state.db_manager.pool();` at line 472, followed by `tokio::spawn` at line 476 that captures `pool` and other references.

- [ ] **Step 2: Apply the edit**

```rust
old_string: let pool = app_state.db_manager.pool();
new_string: let pool = app_state.db_manager.pool().clone();
```

`SqlitePool` is internally `Arc`-based and `.clone()` is cheap (just an `Arc` clone).

- [ ] **Step 3: Verify recording_saver.rs compiles clean**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "recording_saver\.rs" | head -5
```

Expected: no errors. The E0597 (lifetime) and E0521 (escapes method) errors at lines 472, 476 should both disappear.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/audio/recording_saver.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(recording_saver): clone SqlitePool before tokio::spawn

app_state was dropped at the end of the if-let block but pool was
borrowed from it, then captured by tokio::spawn which requires
'static. Clone the pool (cheap, Arc-based) so the spawned task
owns its own reference.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Fix llm_health.rs LLMError enum access

**Files:**
- Modify: `frontend/src-tauri/src/llm_health.rs:168`

- [ ] **Step 1: Verify current state**

```bash
awk 'NR>=165 && NR<=172' frontend/src-tauri/src/llm_health.rs
```

Expected:
```rust
Err(e) => LastTestResult::failed(latency_ms, e.code, &e.message).with_origin(origin)
```

- [ ] **Step 2: Verify LLMError enum shape**

```bash
grep -A20 "^pub enum LLMError" frontend/src-tauri/src/summary/llm_client.rs | head -30
```

Expected: variants `Cancelled`, `Auth`, `ClientError { status, body }`, `ServerError { status, body }`, `Network(String)`, `JsonParse(String)`, `Other(String)`. **No `.code` or `.message` field** — those were old struct fields.

- [ ] **Step 3: Apply the edit**

Replace the call with a match that produces a short identifier for each variant:

```rust
old_string: Err(e) => LastTestResult::failed(latency_ms, e.code, &e.message).with_origin(origin)
new_string: Err(e) => LastTestResult::failed(
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

Verify `LLMError` is imported (likely `use crate::summary::llm_client::LLMError;`). If not, add it.

- [ ] **Step 4: Verify llm_health.rs compiles clean**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "llm_health\.rs" | head -5
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/llm_health.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(llm_health): adapt to LLMError enum (was struct)

LLMError was reworked from a struct (with .code/.message fields) to
an enum. Replace the field-access call with a match that produces
a short identifier per variant and uses to_string() for the message.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Investigate and fix initial_prompt scope in retranscription.rs

**Files:**
- Modify: `frontend/src-tauri/src/audio/retranscription.rs` (root cause unknown; investigate first)

- [ ] **Step 1: Read the full function signature and the call site**

```bash
awk 'NR>=90 && NR<=100' frontend/src-tauri/src/audio/retranscription.rs
echo "---"
awk 'NR>=378 && NR<=390' frontend/src-tauri/src/audio/retranscription.rs
```

Expected: function signature `pub async fn start_retranscription<R: Runtime>(...) -> Result<RetranscriptionResult>` includes `initial_prompt: Option<String>` at line 97. Call site at line 385 uses `initial_prompt.clone()`.

- [ ] **Step 2: Look for a shadowing inner `let initial_prompt`**

```bash
awk '/let initial_prompt/ {print NR": "$0}' frontend/src-tauri/src/audio/retranscription.rs
```

If a `let initial_prompt` exists **inside** a closure or block between line 97 and line 385, that's the shadowing culprit.

- [ ] **Step 3: Diagnose based on findings**

Common fixes:
- **If inner `let initial_prompt = ...` shadows the parameter**: rename the inner binding (e.g., to `loaded_prompt`) and update its uses.
- **If parameter was removed but body wasn't updated**: add the parameter back to the signature.
- **If the parameter is renamed but the body uses old name**: rename the body use to match.

Apply the appropriate fix based on what you found.

- [ ] **Step 4: Verify retranscription.rs compiles clean**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "retranscription\.rs:385" | head -5
```

Expected: no errors.

If the error persists after applying the fix, report BLOCKED with the full error context and what you tried.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/audio/retranscription.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(retranscription): resolve initial_prompt scope issue at line 385

<describe what you found and fixed>

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Final cargo check verification

**Files:**
- None (verification only)

- [ ] **Step 1: Clean target and run cargo check**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
rm -rf frontend/src-tauri/target/  # keep workspace target/ if shared
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | tee quickstart.log | tail -30
```

- [ ] **Step 2: Check for any remaining errors**

```bash
grep -c "error\[" quickstart.log
```

Expected: 0 errors.

If any errors remain:
```bash
awk '/^error\[/{err=$0; getline; print err" ::: "$0}' quickstart.log | head -20
```

Report any remaining errors. If they are pre-existing pattern issues (cascade from M-file state), document them.

---

## Self-Review Checklist

- [x] Spec coverage: ffmpeg lifetime → Task 1; recording_commands extraction → Task 2; recording_saver pool clone → Task 3; llm_health LLMError enum → Task 4; retranscription investigation → Task 5; final verification → Task 6.
- [x] No placeholders: all file paths exact, all commands complete.
- [x] Type/name consistency: `pub fn path_str<'a>`, `current_diarization_buffer`, `pool.clone()`, `LLMError::*`, `e.to_string()` consistent across tasks.
- [x] Each task has a commit step (frequent commits).
- [x] Task 5 explicitly handles the unknown root cause with investigation steps before fix.
- [x] No TDD steps because these are mechanical fixes — verification is end-to-end build success (Task 6).
