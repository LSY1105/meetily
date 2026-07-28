# Design: Fix 10 pre-existing meetily build errors outside the LLMError spec

**Date**: 2026-07-28
**Branch**: devtest
**Status**: Approved

## Problem

`cargo check` on the devtest branch reports 37 pre-existing compile errors
that block `pnpm tauri:dev` from starting. About 22 of those errors fall
under the existing spec
[`docs/superpowers/specs/2026-07-19-llm-error-typed-public.md`](2026-07-19-llm-error-typed-public.md)
(LLMError propagation to processor.rs / failover.rs public API — PR-43
Wave 26 work that was never finished).

The remaining **15 errors are outside the LLMError spec's scope**. This
spec covers those 15.

Many of the 15 errors look like "cannot find function/value" / "cannot
find type", but on closer inspection several are **parser cascade errors**
caused by broken syntax elsewhere — once the real underlying errors are
fixed, the cascading errors typically resolve on their own.

This spec also ignores the 11 session-start "M" files (pre-existing
modifications visible in `git status`). The user stash-tested those
files: without M files, build reports 37 errors; with M files, build
reports 28 errors. M files are partial attempts at the LLMError spec
and add more complexity than they remove. They are intentionally left
stashed (`git stash list` should show them) and are NOT in scope.

## Goal

After this spec's implementation:
- The 10 real fix items below are corrected.
- Cascade errors (`current_diarization_buffer`, `initial_prompt`)
  either resolve themselves OR are explicitly added to a follow-up
  task list if they persist.
- The remaining 22 LLMError-related errors are still present (those
  belong to the existing spec's implementation plan, not this one).

## Approach

### Real fixes (10 items)

1. **`E0252` `Arc` defined multiple times** —
   `frontend/src-tauri/src/audio/recording_saver.rs:13` has
   `use std::sync::Arc;` redundantly. The first import at line 1 is
   `use std::sync::{Arc, Mutex};`. **Fix:** delete line 13.

2. **`E0432` `unresolved import once_cell::sync::OnceLock`** —
   `frontend/src-tauri/src/llm_postprocess.rs:16`. `once_cell::sync::OnceLock`
   may not exist in the resolved `once_cell` version; or `OnceLock` is
   available in `std::sync` (Rust 1.70+). **Fix:** try `use std::sync::OnceLock;`
   first; fall back to checking `once_cell` crate version if std doesn't
   have it.

3. **`E0599` `no method named 'contains' found for enum 'LLMError'`** —
   `frontend/src-tauri/src/summary/processor.rs:66`. The code uses
   `e.contains("cancelled")` against an `LLMError`. **Fix:** replace with
   `matches!(e, LLMError::Cancelled)` (typed enum matching).

4. **`E0599` `no method named 'try_state'`** —
   `frontend/src-tauri/src/audio/recording_saver.rs:471`. `try_state` is a
   `tauri::Manager` trait method. **Fix:** add `use tauri::Manager;` near
   the other `tauri::*` imports in the file.

5. **`E0599` `no method named 'emit'`** —
   `frontend/src-tauri/src/lib.rs:518`. `emit` is a `tauri::Emitter` trait
   method. **Fix:** add `use tauri::Emitter;` near the other `tauri::*`
   imports in `lib.rs`.

6. **`E0728` `await outside async function`** —
   `frontend/src-tauri/src/lib.rs:535`. The function at line 535 is not
   marked `async` but uses `.await`. **Fix:** read the surrounding
   function signature; if it should be async, add `async`; if it should
   be sync, replace `.await` with a sync alternative (`tokio::runtime::Handle::block_on`,
   `pollster::block_on`, or restructure the caller). Document the chosen
   approach in the commit message.

7. **`E0433` `cannot find type FallbackEngine / EngineKind` (5 sites)** —
   `frontend/src-tauri/src/whisper_engine/parallel_processor.rs` references
   `FallbackEngine` and `EngineKind` (defined in
   `whisper_engine/fallback.rs`) without importing them. **Fix:** add
   `use super::fallback::{FallbackEngine, EngineKind};` near the existing
   `use super::whisper_engine::WhisperEngine;` import.

8. **`E0425` `cannot find function protect_terms / restore_protected_terms`** —
   `frontend/src-tauri/src/whisper_engine/whisper_engine.rs:713, 715`
   calls `crate::audio::post_processor::protect_terms(...)` as a free
   function. The actual definitions are associated methods on
   `PostProcessor` in `post_processor.rs:324, 368`. **Fix (minimal):**
   change the two call sites in `whisper_engine.rs` to use the method
   syntax: `PostProcessor::protect_terms(&final_result)` and
   `PostProcessor::restore_protected_terms(&cleaned_guarded, &mapping)`.
   Add `use crate::audio::post_processor::PostProcessor;` if not already
   present.

9. **`E0432` `unresolved imports in tests_offline.rs`** —
   `frontend/src-tauri/src/diarization/tests_offline.rs:1, 2`. Imports
   `super::super::status` (no `status` module under `diarization`) and
   `super::commit_speaker_labels` (actual location is
   `diarization::offline::commit_speaker_labels`). **Fix:** inspect the
   test file end-to-end; rewrite imports to point at the correct paths
   (likely `super::offline::commit_speaker_labels` and whatever holds
   the status type — check `diarization/mod.rs` and the offline module
   to find it). The body of the test likely needs corresponding updates
   if it uses `super::super::update_status(...)`.

10. **`E0425` `cannot find function default_origin`** —
    `frontend/src-tauri/src/llm_diagnostics.rs:41`. The struct field
    `pub origin: &'static str` is annotated
    `#[serde(default = "default_origin")]` but no `default_origin`
    function is defined. **Fix:** add a `fn default_origin() -> &'static str
    { "manual" }` near the top of the file (next to `now_ts()` which
    follows the same pattern).

### Cascade errors (verify after real fixes)

After all 10 real fixes land, run `cargo check --target
x86_64-pc-windows-msvc` again. Some previously reported errors will
disappear because the parser cascade will have cleared:

- `E0425` `cannot find function current_diarization_buffer` in
  `audio/transcription/worker.rs:219` — the function exists as
  `pub fn current_diarization_buffer()` in
  `audio/recording_commands.rs:272`. This is almost certainly a parser
  cascade error that resolves once other errors are fixed.
- `E0425` `cannot find value initial_prompt` in
  `audio/retranscription.rs:385` — `initial_prompt: Option<String>` is
  declared as a function parameter at line 97 and is in scope at line
  385. This is a parser cascade error.

If after the 10 fixes these errors PERSIST, they need separate fixes
(add to the implementation plan as a follow-up task).

## Files Changed

| File | Change |
|---|---|
| `frontend/src-tauri/src/audio/recording_saver.rs` | Delete duplicate `use std::sync::Arc;` at line 13. Add `use tauri::Manager;` near other tauri imports. |
| `frontend/src-tauri/src/llm_postprocess.rs` | Change `use once_cell::sync::OnceLock;` (line 16) to `use std::sync::OnceLock;`. |
| `frontend/src-tauri/src/summary/processor.rs` | Replace `e.contains("cancelled")` (line 66) with `matches!(e, LLMError::Cancelled)`. |
| `frontend/src-tauri/src/lib.rs` | Add `use tauri::Emitter;`. Investigate function at line 535 and either add `async` or replace `.await` with a sync alternative. |
| `frontend/src-tauri/src/whisper_engine/parallel_processor.rs` | Add `use super::fallback::{FallbackEngine, EngineKind};`. |
| `frontend/src-tauri/src/whisper_engine/whisper_engine.rs` | Change call sites at lines 713, 715 to `PostProcessor::protect_terms(...)` and `PostProcessor::restore_protected_terms(...)`. Add import if needed. |
| `frontend/src-tauri/src/diarization/tests_offline.rs` | Rewrite imports at lines 1, 2 to point at correct paths. Likely `use super::offline::commit_speaker_labels;` plus the right path for the status type. Update call sites in the test body if they use `super::super::update_status(...)`. |
| `frontend/src-tauri/src/llm_diagnostics.rs` | Add `fn default_origin() -> &'static str { "manual" }` near `now_ts()`. |

## Validation

After all 10 fixes are committed, run:

```bash
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | tee quickstart.log
```

Expected:
- All 10 real fixes resolve.
- Cascade errors (`current_diarization_buffer`, `initial_prompt`) either
  resolve or are explicitly listed as remaining (and small enough to
  handle in a follow-up).
- 22 errors from the LLMError spec (`docs/superpowers/specs/2026-07-19-llm-error-typed-public.md`)
  remain — those belong to that spec's implementation plan, not this one.

## Out of Scope

- 22 LLMError propagation errors — covered by the existing spec.
- whisper-rs / sys / patches — already fixed.
- llama-helper `llama-cpp-2`/`sys-2` version mismatch — separate task.
- The 11 session-start "M" files (stashed via `git stash`) — left
  stashed. They are partial attempts at the LLMError spec and add
  more complexity than they remove.

## Non-Goals

- Not refactoring any of the affected files beyond the minimum fix.
- Not adding documentation beyond the commit messages.
- Not addressing test failures (only compile errors are in scope).
