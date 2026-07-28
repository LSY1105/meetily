# Design: Fix 18 remaining pre-existing meetily build errors

**Date**: 2026-07-28
**Branch**: devtest
**Status**: Approved

## Context

This spec is the second of two specs covering pre-existing compile errors
on devtest. The first spec
[`docs/superpowers/specs/2026-07-28-meetily-misc-build-fixes-design.md`](2026-07-28-meetily-misc-build-fixes-design.md)
(target: 10 errors, commit `bd734fb`) covers structural fixes (duplicate
imports, missing imports, missing trait imports).

After that spec lands, **18 errors remain**. These are **type-mismatch
errors and one nalgebra API drift error** introduced by various
post-PR-43 PRs (PR-44 diarization, PR-45 LLM diagnostics, PR-46
provider picker, PR-47 health watch, PR-48 interval persistence, etc.).

## Goal

After this spec's implementation, cargo check passes for all source
files in the workspace (except for the 11 session-start "M" files
which are stashed and out of scope).

## Real fixes (8 distinct items)

### 1. `diarization/offline.rs:68-69` — f32 vs f64 mismatch

The function uses `best_dist: f32 = f32::MAX` but compares against
`d` (computed from `seg_start + seg_end`, which is `f64`).

**Fix**: change `best_dist` to `f64`:

```rust
let mut best_dist = f64::MAX;
```

### 2. `diarization/clustering.rs:49` — nalgebra `DVector::collect()`

The code `let deg: DVector<f32> = w.row_iter().map(|r| r.sum()).collect();`
fails because nalgebra 0.32's `DVector` does not implement
`FromIterator<f32>`.

**Fix**: use `DVector::from_iterator` with explicit length:

```rust
let deg: DVector<f32> = DVector::from_iterator(w.nrows(), w.row_iter().map(|r| r.sum()));
```

### 3. `summary/processor.rs:538` — return type mismatch

After PR-43, `generate_meeting_summary` returns `Result<_, LLMError>`.
Line 538 still returns `Err("Summary generation was cancelled".to_string())`
which produces `String`.

**Fix**: replace with `LLMError::Cancelled`:

```rust
return Err(LLMError::Cancelled);
```

### 4. `summary/processor.rs:585` — same pattern as #3

Different code path in the same function (or related function) returning
`String` instead of `LLMError`. **Investigate exact line at implementation
time** — the fix is the same pattern (`LLMError::Cancelled` or
`LLMError::Other(...)`).

### 5. `summary/failover.rs:83` — return type mismatch

Line 83: `return Err("Provider chain is empty".to_string());` — but the
function returns `Result<String, LLMError>`.

**Fix**:

```rust
return Err(LLMError::Other("Provider chain is empty".to_string()));
```

### 6. `summary/failover.rs:90` — return type mismatch

Line 90: `return Err("Summary generation was cancelled".to_string());`
inside the for-loop.

**Fix**:

```rust
return Err(LLMError::Cancelled);
```

### 7. `llm_health.rs:16` — wrong import path

Imports `crate::llm_postprocess::{generate_summary, http_client,
load_provider_inputs}` but `generate_summary` is not re-exported by
`llm_postprocess`. `llm_postprocess.rs:13` does
`use crate::summary::llm_client::{generate_summary, ...}` — but a `use`
is not a re-export unless marked `pub use`.

**Fix**: import `generate_summary` directly from its definition site:

```rust
use crate::summary::llm_client::generate_summary;
use crate::llm_postprocess::{http_client, load_provider_inputs};
```

## Cascade errors (verify after fixes 1-7)

After all 7 real fixes land, run cargo check. Errors that **should
resolve on their own** (cascade from real errors):

- `E0425` `cannot find function current_diarization_buffer` in
  `audio/transcription/worker.rs:219`
- `E0425` `cannot find value initial_prompt` in
  `audio/retranscription.rs:385`
- `E0425` `cannot find function protect_terms` in
  `whisper_engine/whisper_engine.rs:713`
- `E0425` `cannot find function restore_protected_terms` in
  `whisper_engine/whisper_engine.rs:715`

The `protect_terms` / `restore_protected_terms` errors are covered by
the first spec (item 8 — change call site to method syntax). The
other three are parser-cascade errors that should resolve once real
fixes land.

If any cascade error PERSISTS after real fixes, add a new item to
the implementation plan to address it.

## Files Changed

| File | Change |
|---|---|
| `frontend/src-tauri/src/diarization/offline.rs` | `f32::MAX` → `f64::MAX` |
| `frontend/src-tauri/src/diarization/clustering.rs` | Use `DVector::from_iterator(n, iter)` |
| `frontend/src-tauri/src/summary/processor.rs` | 2× `Err(String)` → `Err(LLMError::*)` |
| `frontend/src-tauri/src/summary/failover.rs` | 2× `Err(String)` → `Err(LLMError::*)` |
| `frontend/src-tauri/src/llm_health.rs` | Split import: `generate_summary` from `summary::llm_client`, others from `llm_postprocess` |

## Validation

```bash
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | tee quickstart.log
```

Expected after the misc-build-fixes spec AND this spec are both applied:
- 0 errors related to LLMError propagation or type mismatch.
- The 4 cascade errors either resolve OR are explicitly listed as a
  follow-up task.
- Remaining errors (if any) are unrelated to either spec and would
  require additional investigation.

## Out of Scope

- 22 errors in the original `2026-07-19-llm-error-typed-public.md` spec —
  already implemented in PR-43 (commit `87c6af0`).
- 10 errors in misc-build-fixes spec (commit `bd734fb`) — separate
  spec, must land first.
- whisper-rs / sys / patches — already fixed.
- llama-helper — separate task.
- The 11 session-start "M" files — left stashed, out of scope.

## Non-Goals

- Not refactoring any file beyond the minimum fix.
- Not addressing test failures (only compile errors are in scope).
- Not migrating to a newer nalgebra version.
