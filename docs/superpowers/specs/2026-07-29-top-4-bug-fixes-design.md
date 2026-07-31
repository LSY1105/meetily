# Design: Fix top-4 HIGH-severity bugs from the perf/bug audit

**Date**: 2026-07-29
**Branch**: devtest
**Status**: Approved

## Context

A full audit of the meetily codebase produced 100+ findings. The 20
HIGH-severity issues were prioritized. This spec covers the **top 4 by
ROI** (high impact, low fix cost):

1. `embedding.rs:44-59` — stub embedder writes wrong speaker labels
2. `parallel_processor.rs:237-239` — parallel transcription is dead
3. `system_monitor.rs:77-107` — resource monitor is a startup-time snapshot
4. `transcription_preferences.rs:110-126` — protected-terms feature returns `[]` for `! Term`

(Sidecar concurrency issues from the audit are real but high-cost; deferred
to a follow-up spec.)

## Goal

After this spec's implementation, four broken subsystems produce correct
behavior — no regressions in already-working code.

## Fix 1: Gate the stub embedder

**File:** `frontend/src-tauri/src/diarization/embedding.rs:44-59`

**Problem:** `extract_embedding` returns `[rms, 0, 0, ...]` regardless of
input. All embeddings lie on the same axis, so cosine affinity is all-ones
and the resulting speaker labels are arbitrary. `ensure_loaded()` at line 34
also reports `status=1 (Loaded)` purely because a model file exists, not
because anything is actually loaded.

**Fix:**

1. Add a `Result<Vec<f32>>` return type to `extract_embedding` (currently
   returns `Vec<f32>`). Until a real model is wired in, return
   `Err(anyhow::anyhow!("Speaker embedding not yet implemented; diarization disabled"))`.
2. In `push_window` (line 69-74), convert the current `Err(_) => false` to
   log a debug message and return `false` (silently disable realtime
   diarization rather than writing wrong state).
3. In `reembed_wav` (line 91-119 of `diarization/offline.rs`), the `?`
   operator already propagates the error. Confirm that the propagation
   gracefully aborts the offline pass — `commit_speaker_labels` should
   catch the error and log it (line 24-28 of `offline.rs`).

After this fix, the diarization subsystem produces no labels until a real
embedding model is wired. The frontend UI will show no speaker labels,
which is the honest correct behavior.

## Fix 2: Wire the parallel transcription worker

**File:** `frontend/src-tauri/src/whisper_engine/parallel_processor.rs:237-239`

**Problem:** `create_worker` calls `WhisperEngine::new().load_model(&name)`
directly. `load_model` requires `self.available_models` to be populated by
`discover_models()`, but no caller in the parallel path calls that.
Combined with `WhisperEngine::new()` resolving models from CWD instead of
the configured `MODELS_DIR`, every parallel worker dies at startup.

**Fix:**

1. Before `load_model`, call `engine.discover_models().await?` so the
   model map is populated. The discovery is sync-IO (reads file system)
   but only happens once at worker startup, so `spawn_blocking` is not
   needed.
2. Pass the configured models directory into the worker. Read
   `whisper_engine/commands.rs:8-33` for the `MODELS_DIR` static and
   either clone the `Arc<PathBuf>` into the worker or recreate the
   `WhisperEngine` using `WhisperEngine::new_with_models_dir(Some(MODELS_DIR.read()))`.

**Recommended approach:** add a `models_dir: Option<PathBuf>` field to
`ParallelProcessorConfig` and pass it through `ParallelProcessor::start`.
Worker constructor signature becomes
`create_worker(model_name: String, models_dir: Option<PathBuf>)`, which
calls `WhisperEngine::new_with_models_dir(models_dir)`.

## Fix 3: Refresh resources in get_current_resources

**File:** `frontend/src-tauri/src/whisper_engine/system_monitor.rs:77-107`

**Problem:** `get_current_resources` reads from a `System` snapshot that is
populated only at `SystemMonitor::new()`. `refresh_system_info` exists but
has no callers, so every resource check uses launch-time data. `cpu_usage_percent`
is `0.0` because `new_all()` doesn't prime CPU samples. Auto-pause based
on resource constraints can never trigger correctly.

**Fix:**

1. Make `refresh_system_info` actually update both memory and CPU. The
   current body (line 62-75) calls `sys.refresh_memory()` and `sys.refresh_cpu_usage()`,
   which is correct — the bug is that nothing calls it.
2. In `get_current_resources` (line 77-107), call `self.refresh_system_info().await`
   at the top, under the write lock, before reading the fields.
3. Fix the line 67-72 write-lock-held-during-sleep: the body should be
   ```
   {
     let mut sys = self.system.write().await;
     sys.refresh_memory();
   }
   tokio::time::sleep(...).await;
   {
     let mut sys = self.system.write().await;
     sys.refresh_cpu_usage();
   }
   ```
   so the lock is released during the sleep.

After this fix, the resource monitor reads current load every time it's
consulted. The 10-second monitoring loop will catch real pressure.

## Fix 4: Fix protected-terms extraction

**File:** `frontend/src-tauri/src/transcription_preferences.rs:110-126`

**Problem:** `extract_protected_terms` splits the input on whitespace.
`"! 张三"` becomes `["!", "张三"]`. The `!` token strips to `""` and is
filtered; the `张三` token has no `!` prefix and is filtered. Result:
`[]`. The doc comment claims "with optional whitespace" is supported;
the test at line 208-211 asserts the correct behavior — so the test is
red today.

**Fix:**

Replace the split-and-strip logic with a newline-based parser:

```rust
pub fn extract_protected_terms(text: &str) -> Vec<String> {
    text.split(|c| c == '\n' || c == ',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.strip_prefix('!').map(str::trim))
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}
```

This handles:
- `"张三\n!字节跳动"` → `["张三", "字节跳动"]`
- `"! 张三"` (one line, with space) → `["张三"]`
- `"张三,! 字节跳动"` → `["张三", "字节跳动"]`

Update the test at line 208-211 if its input is `"! 张三\n!\t字节跳动"`,
the new behavior returns `["张三", "字节跳动"]` matching the existing
assertion.

## Files Changed

| File | Change |
|---|---|
| `frontend/src-tauri/src/diarization/embedding.rs` | `extract_embedding` returns `Result`; gate on error |
| `frontend/src-tauri/src/whisper_engine/parallel_processor.rs` | Worker calls `discover_models` and takes `models_dir` |
| `frontend/src-tauri/src/whisper_engine/system_monitor.rs` | `get_current_resources` calls `refresh_system_info`; sleep pattern fix |
| `frontend/src-tauri/src/transcription_preferences.rs` | `extract_protected_terms` uses newline-based parser |

## Validation

```bash
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | tee quickstart.log | tail -10
```

Expected: 0 errors (no regressions in already-passing files).

Plus behavioral checks:
- For Fix 4: `cargo test -p meetily transcription_preferences::tests` should
  pass after the change.
- For Fix 1: trigger a transcription, then check that no speaker label
  events are emitted (diarization silently disabled).
- For Fix 3: log `cpu_usage_percent` over a few seconds while under load;
  should reflect current load, not `0.0`.
- For Fix 2: run `start_processing` and confirm a chunk is actually
  transcribed (visible in `get_processing_status`).

## Out of Scope

- Sidecar concurrency (3 issues, deferred)
- Whisper model load lock ordering (high-cost, deferred)
- lib.rs DB-init panic (separate concern, deferred)
- 100+ MEDIUM/LOW findings from the audit (separate spec batch)

## Non-Goals

- Adding a real sherpa-onnx embedding model
- Refactoring beyond the minimum fix for each item
- Performance optimization (the audit found O(n²) issues; out of scope here)
