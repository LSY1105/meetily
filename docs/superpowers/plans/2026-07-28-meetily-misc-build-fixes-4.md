# Misc Build Fixes #4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the final remaining compile error so cargo check passes cleanly.

**Architecture:** Single-file mechanical fix — cache `chunk` field values as locals before the move at line 156, then use the locals in the later `push_window` call.

**Tech Stack:** Rust (cargo), Tauri 2.x.

**Spec:** `docs/superpowers/specs/2026-07-28-meetily-misc-build-fixes-4-design.md`

**Working directory:** `C:\Users\qjl10\Documents\工作区\meetily` (Windows, bash shell)

**Prerequisite:** Spec #1, #2, and #3 must have already landed (they did).

---

## File Structure

| File | Responsibility | Touched by |
|---|---|---|
| `frontend/src-tauri/src/audio/transcription/worker.rs` | Use-of-moved-value fix | Task 1 |

---

## Task 1: Cache chunk fields before move in worker.rs

**Files:**
- Modify: `frontend/src-tauri/src/audio/transcription/worker.rs:151-156, 222-223`

- [ ] **Step 1: Verify current state**

```bash
awk 'NR>=149 && NR<=160' frontend/src-tauri/src/audio/transcription/worker.rs
echo "---"
awk 'NR>=218 && NR<=235' frontend/src-tauri/src/audio/transcription/worker.rs
```

Expected output for the first block:
```rust
                            let chunk_timestamp = chunk.timestamp;
                            let chunk_duration = chunk.data.len() as f64 / chunk.sample_rate as f64;

                            // Transcribe with provider-agnostic approach
                            match transcribe_chunk_with_provider(
                                &engine_clone,
                                chunk,
                                ...
```

Expected output for the second block (within `Ok(...) => { ... }` arm):
```rust
                                        let transient_speaker: Option<String> = {
                                            let buf = crate::audio::recording_commands::current_diarization_buffer();
                                            if crate::diarization::embedding::push_window(
                                                buf.as_ref(),
                                                &chunk.data,
                                                chunk.sample_rate,
                                                chunk_timestamp,
                                                chunk_timestamp + chunk_duration,
                                            ) {
                                                Some("Speaker ?".to_string())
```

- [ ] **Step 2: Add cache lines after line 151**

Use Edit tool. Insert two lines after `let chunk_duration = chunk.data.len() as f64 / chunk.sample_rate as f64;`:

```rust
old_string:
                            let chunk_timestamp = chunk.timestamp;
                            let chunk_duration = chunk.data.len() as f64 / chunk.sample_rate as f64;

                            // Transcribe with provider-agnostic approach
                            match transcribe_chunk_with_provider(
new_string:
                            let chunk_timestamp = chunk.timestamp;
                            let chunk_duration = chunk.data.len() as f64 / chunk.sample_rate as f64;
                            let chunk_sample_rate = chunk.sample_rate;
                            let chunk_data = chunk.data.clone();

                            // Transcribe with provider-agnostic approach
                            match transcribe_chunk_with_provider(
```

- [ ] **Step 3: Change lines 222-223 to use the cached locals**

```rust
old_string:
                                            if crate::diarization::embedding::push_window(
                                                buf.as_ref(),
                                                &chunk.data,
                                                chunk.sample_rate,
new_string:
                                            if crate::diarization::embedding::push_window(
                                                buf.as_ref(),
                                                &chunk_data,
                                                chunk_sample_rate,
```

- [ ] **Step 4: Verify worker.rs compiles clean**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "worker\.rs" | head -5
```

Expected: no errors referencing `worker.rs`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/audio/transcription/worker.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(worker): cache chunk fields before move in transcription worker

chunk was moved into transcribe_chunk_with_provider at line 156,
but the Ok(...) arm at line 222-223 still needed chunk.data and
chunk.sample_rate for the embedding push_window call. Cache
sample_rate (Copy) and clone data (Vec<f32>) as locals before
the move.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Final cargo check verification

**Files:**
- None (verification only)

- [ ] **Step 1: Run cargo check**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | tee quickstart.log | tail -10
```

- [ ] **Step 2: Confirm zero errors**

```bash
grep -c "error\[" quickstart.log
```

Expected: 0 errors.

If any errors remain, list them with `awk '/^error\[/{err=$0; getline; print err" ::: "$0}' quickstart.log`. Report any remaining errors.

## Self-Review Checklist

- [x] Spec coverage: worker.rs cache fix → Task 1; final verification → Task 2.
- [x] No placeholders: all file paths exact, all commands complete.
- [x] Type/name consistency: `chunk_sample_rate`, `chunk_data` consistent across both edits.
- [x] Each task has a commit step.
- [x] No TDD steps — mechanical fix; verification is end-to-end build success (Task 2).
