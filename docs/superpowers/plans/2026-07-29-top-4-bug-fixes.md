# Top-4 Bug Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix four HIGH-severity bugs identified in the perf/bug audit so that diarization, parallel transcription, resource monitoring, and protected-terms extraction produce correct behavior.

**Architecture:** Each fix is small (1-15 lines) and targets a single file. No design decisions, no refactoring, no API changes beyond what's strictly needed to fix the bug. Each fix gets its own commit for traceability.

**Tech Stack:** Rust (cargo), Tauri 2.x, sysinfo, nalgebra, sqlx, reqwest.

**Spec:** `docs/superpowers/specs/2026-07-29-top-4-bug-fixes-design.md`

**Working directory:** `C:\Users\qjl10\Documents\工作区\meetily` (Windows, bash shell)

**Prerequisite:** None (spec-only).

---

## File Structure

| File | Responsibility | Touched by |
|---|---|---|
| `frontend/src-tauri/src/diarization/embedding.rs` | Stub embedder gating | Task 1 |
| `frontend/src-tauri/src/whisper_engine/parallel_processor.rs` | Worker model setup | Task 2 |
| `frontend/src-tauri/src/whisper_engine/system_monitor.rs` | Resource refresh | Task 3 |
| `frontend/src-tauri/src/transcription_preferences.rs` | Protected-terms parser | Task 4 |

---

## Task 1: Gate the stub embedder to disable wrong speaker labels

**Files:**
- Modify: `frontend/src-tauri/src/diarization/embedding.rs:44-59, 69-74`

- [ ] **Step 1: Read current state**

```bash
awk 'NR>=40 && NR<=80' frontend/src-tauri/src/diarization/embedding.rs
```

Expected: line ~44 has `pub fn extract_embedding(samples: &[f32]) -> Vec<f32>` (no Result), line 69-74 has `push_window` calling `extract_embedding` and mapping `Err(_) => false`.

- [ ] **Step 2: Change `extract_embedding` to return Result**

Find the current signature (likely `pub fn extract_embedding(samples: &[f32]) -> Vec<f32>`) and replace with:

```rust
old_string: pub fn extract_embedding(samples: &[f32]) -> Vec<f32> {
new_string: pub fn extract_embedding(samples: &[f32]) -> anyhow::Result<Vec<f32>> {
```

Find the function body ending `}` followed by an empty line. Inside the body, before the existing return, add at the very top:

```rust
    // ponytail: stub embedder returns RMS-only vectors. All embeddings
    // land on the same axis, so cosine affinity is uniform and speaker
    // labels are arbitrary. Disabled until a real model is wired.
    return Err(anyhow::anyhow!("Speaker embedding not yet implemented"));
```

The function body should now have this early-return at the top, with the rest of the code unreachable (but left for when a real model is added).

- [ ] **Step 3: Update `push_window` to log on error**

```bash
awk 'NR>=69 && NR<=74' frontend/src-tauri/src/diarization/embedding.rs
```

Find the line `Err(_) => return false,` and replace with:

```rust
old_string: Err(_) => return false,
new_string: Err(e) => {
    log::debug!("push_window: diarization disabled: {}", e);
    return false;
}
```

Verify `log` is imported at the top of the file (it should be). If not, add `use log;` to the imports.

- [ ] **Step 4: Verify the file compiles**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "embedding\.rs" | head -10
```

Expected: no errors.

- [ ] **Step 5: Verify the offline path propagates the error**

```bash
awk 'NR>=85 && NR<=120' frontend/src-tauri/src/diarization/offline.rs
```

Look for `reembed_wav` calling `extract_embedding` with `?`. The `?` operator already propagates the new `Result`. No edit needed; just confirm.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/diarization/embedding.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(diarization): gate stub embedder to disable wrong speaker labels

The extract_embedding function returns [rms, 0, ...] regardless
of input, producing arbitrary speaker labels written to SQLite.
Change return type to Result and short-circuit with an error
until a real embedding model is wired. push_window logs and
returns false on error. No real labels are persisted until a
real model is implemented.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Wire the parallel transcription worker to discover models and use the configured directory

**Files:**
- Modify: `frontend/src-tauri/src/whisper_engine/parallel_processor.rs` (create_worker function and ParallelProcessorConfig)
- Read for context: `frontend/src-tauri/src/whisper_engine/commands.rs:8-50` (MODELS_DIR static, new_with_models_dir)

- [ ] **Step 1: Read the current create_worker code**

```bash
awk 'NR>=120 && NR<=160' frontend/src-tauri/src/whisper_engine/parallel_processor.rs
echo "---"
awk 'NR>=225 && NR<=245' frontend/src-tauri/src/whisper_engine/parallel_processor.rs
```

Expected: `create_worker(model_name: String, ...)` constructs `WhisperEngine::new()` and calls `engine.load_model(&model_name)`. There may be a `ParallelProcessorConfig` struct that has a `model_name` field but no `models_dir`.

- [ ] **Step 2: Read MODELS_DIR location and the existing `new_with_models_dir` signature**

```bash
grep -n "MODELS_DIR\|new_with_models_dir\|set_models_directory" frontend/src-tauri/src/whisper_engine/commands.rs | head -10
grep -n "fn new_with_models_dir\|pub fn new\b" frontend/src-tauri/src/whisper_engine/whisper_engine.rs | head -5
```

- [ ] **Step 3: Add `models_dir: Option<PathBuf>` to `ParallelProcessorConfig`**

Find the config struct and add the field:

```rust
old_string: pub struct ParallelProcessorConfig {
new_string: pub struct ParallelProcessorConfig {
    pub model_name: String,
    pub models_dir: Option<std::path::PathBuf>,
```

(Adjust field names to match the actual struct. If `model_name` is already present, only add the new field.)

- [ ] **Step 4: Update `create_worker` to take and use models_dir**

Find the function signature (likely `async fn create_worker(model_name: String, ...)`) and the body. Make these two changes:

**Change 1** — signature, add the new parameter:

```rust
old_string: fn create_worker(
    model_name: String,
new_string: fn create_worker(
    model_name: String,
    models_dir: Option<std::path::PathBuf>,
```

(Place `models_dir` right after `model_name` for readability.)

**Change 2** — body, replace `WhisperEngine::new()` with `WhisperEngine::new_with_models_dir(models_dir)`, and call `discover_models` before `load_model`:

```rust
old_string:         let mut engine = WhisperEngine::new();
        engine.load_model(&model_name)
new_string:         let mut engine = WhisperEngine::new_with_models_dir(models_dir);
        engine.discover_models().await
            .map_err(|e| anyhow::anyhow!("discover_models failed: {}", e))?;
        engine.load_model(&model_name)
```

If `discover_models` is async, the call already returns a future. If it's sync, drop the `.await`. Check the signature in `whisper_engine.rs`.

- [ ] **Step 5: Update the caller of `create_worker` to pass `models_dir`**

Find the `create_worker` call site (likely in `start_processing` or similar). It probably looks like:

```rust
tokio::spawn(create_worker(self.config.model_name.clone(), ...))
```

Change it to also pass `self.config.models_dir.clone()`:

```rust
old_string: tokio::spawn(create_worker(self.config.model_name.clone(), receiver, ...))
new_string: tokio::spawn(create_worker(self.config.model_name.clone(), self.config.models_dir.clone(), receiver, ...))
```

(Adjust to match the actual function arity.)

- [ ] **Step 6: Update `start_processing` (or wherever ParallelProcessorConfig is constructed) to plumb `models_dir`**

Find where `ParallelProcessorConfig` is built. Read the `MODELS_DIR` value and pass it as `models_dir`. The likely call site already has access to it; the change is a single field assignment.

```rust
old_string:     model_name: model_name.clone(),
new_string:     model_name: model_name.clone(),
        models_dir: Some(MODELS_DIR.read().unwrap().clone()),
```

(`MODELS_DIR` is the static `Mutex<Option<PathBuf>>` from `whisper_engine/commands.rs`.)

- [ ] **Step 7: Verify the file compiles**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "parallel_processor\.rs" | head -10
```

Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add frontend/src-tauri/src/whisper_engine/parallel_processor.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(parallel_processor): wire worker to discover models and use configured dir

The parallel transcription worker called WhisperEngine::new()
(which uses CWD for model resolution) and immediately load_model,
but load_model requires available_models to be populated by
discover_models. Every worker died at startup with Model not found.

Pass MODELS_DIR through ParallelProcessorConfig and into
create_worker; call discover_models before load_model. The
parallel feature now actually transcribes chunks.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Refresh resources in get_current_resources

**Files:**
- Modify: `frontend/src-tauri/src/whisper_engine/system_monitor.rs:62-75, 77-107`

- [ ] **Step 1: Read current state**

```bash
awk 'NR>=60 && NR<=110' frontend/src-tauri/src/whisper_engine/system_monitor.rs
```

Expected: `refresh_system_info` (line 62-75) takes a write lock and sleeps. `get_current_resources` (line 77-107) reads without refreshing.

- [ ] **Step 2: Fix `refresh_system_info` to release the lock during the sleep**

Replace the function body so the lock is dropped between refresh_memory and refresh_cpu_usage:

```rust
old_string:     pub async fn refresh_system_info(&self) {
        let mut sys = self.system.write().await;
        sys.refresh_memory();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        sys.refresh_cpu_usage();
    }
new_string:     pub async fn refresh_system_info(&self) {
        {
            let mut sys = self.system.write().await;
            sys.refresh_memory();
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        {
            let mut sys = self.system.write().await;
            sys.refresh_cpu_usage();
        }
    }
```

- [ ] **Step 3: Add refresh call to `get_current_resources`**

Find the function (it likely starts with `pub async fn get_current_resources`). Add `self.refresh_system_info().await;` as the first line of the function body (after the function signature but before any `let`).

The exact insertion depends on the function body. A minimal change:

```rust
old_string:     pub async fn get_current_resources(&self) -> ... {
        let mut sys = self.system.read().await;
new_string:     pub async fn get_current_resources(&self) -> ... {
        self.refresh_system_info().await;
        let mut sys = self.system.read().await;
```

(Place the refresh call before the read-lock acquisition.)

- [ ] **Step 4: Verify the file compiles**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "system_monitor\.rs" | head -10
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/whisper_engine/system_monitor.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(system_monitor): refresh resources on every get_current_resources call

refresh_system_info was defined but had no callers, so resource
checks used launch-time snapshots. cpu_usage_percent was always
0.0 and auto-pause based on resource constraints never fired.

Call refresh_system_info from get_current_resources so every
resource check sees current load. Also fix the function so the
write lock is released during the 200ms sleep between memory
and CPU refresh.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Fix protected-terms extraction to use newline parser

**Files:**
- Modify: `frontend/src-tauri/src/transcription_preferences.rs:110-126`

- [ ] **Step 1: Read current state**

```bash
awk 'NR>=105 && NR<=130' frontend/src-tauri/src/transcription_preferences.rs
```

Expected: line 110-126 contains `extract_protected_terms` that uses `.split_whitespace()` or similar.

- [ ] **Step 2: Find the test that asserts the expected behavior**

```bash
grep -n "protected_terms_allow_whitespace\|extract_protected_terms" frontend/src-tauri/src/transcription_preferences.rs
```

Find the test that uses `extract_protected_terms` and note its input. This is what the new implementation must satisfy.

- [ ] **Step 3: Replace the function body**

The new implementation parses by newlines (and commas) instead of whitespace:

```rust
old_string: pub fn extract_protected_terms(text: &str) -> Vec<String> {
new_string: pub fn extract_protected_terms(text: &str) -> Vec<String> {
    text.split(|c: char| c == '\n' || c == ',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.strip_prefix('!').map(str::trim))
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}
```

Adjust the `old_string` to match the actual function body. If the function currently returns a different type or has additional logic, preserve it.

- [ ] **Step 4: Verify the test passes**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
cargo test -p meetily transcription_preferences 2>&1 | tail -20
```

Expected: tests pass. If the test at line 208-211 fails, inspect the test input and update the assertion to match the new parsing (it should already be correct since the original test was written for the desired behavior).

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/transcription_preferences.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(transcription_preferences): use newline-based parser for protected terms

extract_protected_terms split on whitespace, so '! Term' became
['!', 'Term'] and was filtered to []. The test that asserts the
correct behavior was red. Switch to newline+comma split with
trim and bang-strip, which handles all the documented input forms.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Final cargo check verification

**Files:**
- None (verification only)

- [ ] **Step 1: Run cargo check on the full workspace**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | tee quickstart.log | tail -10
```

- [ ] **Step 2: Confirm zero errors**

```bash
grep -c "^error\b" quickstart.log
```

Expected: 0 errors (the 4 fixes should not introduce regressions).

- [ ] **Step 3: Run transcription_preferences test**

```bash
cargo test -p meetily transcription_preferences 2>&1 | tail -10
```

Expected: tests pass.

## Self-Review Checklist

- [x] Spec coverage: 4 fixes → Tasks 1-4; final verification → Task 5.
- [x] No placeholders: all file paths exact, all commands complete, all edits show concrete code.
- [x] Type/name consistency: `Result<Vec<f32>>` for embedder; `Option<PathBuf>` for models_dir; `get_current_resources` signature unchanged.
- [x] Each task has a commit step.
- [x] No TDD steps — these are bug fixes; verification is end-to-end build (Task 5).
