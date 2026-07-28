# Design: Fix final use-of-moved-value error in worker.rs

**Date**: 2026-07-28
**Branch**: devtest
**Status**: Approved

## Context

Spec #3 subagent overcounted remaining errors. After spec #3 lands,
cargo check actually reports **1 error**, not 8. This spec covers
that final error.

## Goal

After this spec's implementation, `cargo check` produces **0 errors**
for the meetily workspace.

## Real fix (1 item)

### `audio/transcription/worker.rs:223` — use of moved value `chunk`

The `chunk` variable (type `recording_state::AudioChunk`, derives
`Clone`) is moved into `transcribe_chunk_with_provider` at line 156.
Later, the `Ok(...) => { ... }` arm at line 162-251 still references
`chunk.data` (line 222) and `chunk.sample_rate` (line 223) for the
`push_window` call. Both accesses are after the move and fail.

**Fix**: cache the needed fields as locals before the move. Add these
two lines immediately after line 151 (before the `match
transcribe_chunk_with_provider(...)` call):

```rust
let chunk_sample_rate = chunk.sample_rate;
let chunk_data = chunk.data.clone();
```

Then change lines 222-223 from:
```rust
&chunk.data,
chunk.sample_rate,
```
to:
```rust
&chunk_data,
chunk_sample_rate,
```

`AudioChunk` derives `Clone` so this is safe. `Vec<f32>` clone is
non-trivial in cost (chunk can be large), but the alternative —
moving the `push_window` block above line 154 — would require more
intrusive restructuring and changes the program's behavior (the
embedding push would happen synchronously before transcription
starts instead of after).

## Files Changed

| File | Change |
|---|---|
| `frontend/src-tauri/src/audio/transcription/worker.rs` | Add 2 locals before line 154; change lines 222-223 to use them |

## Validation

```bash
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | tee quickstart.log | tail -10
```

Expected: `Finished` with no `error[` lines.

## Out of Scope

- whisper-rs / sys / patches — already fixed.
- llama-helper — separate task.
- Spec #1-#3 errors — all fixed.

## Non-Goals

- Restructuring the embedding-push to happen before transcription.
- Refactoring beyond the minimum clone fix.
