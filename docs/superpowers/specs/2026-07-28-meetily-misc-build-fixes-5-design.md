# Design: Fix final 6 pre-existing meetily build errors

**Date**: 2026-07-28
**Branch**: devtest
**Status**: Approved

## Context

This is the fifth spec in a series fixing pre-existing compile errors on
devtest. Specs #1-#4 landed cleanly. After spec #4, cargo check
reports **6 errors** across 3 files. This spec covers those 6.

## Goal

After this spec's implementation, `cargo check` produces **0 errors**
for the meetily workspace.

## Real fixes (3 files, 6 errors)

### Fix 1: `whisper_engine/fallback.rs:22` — missing `&` on return type

```rust
impl EngineKind {
    pub fn name(self) -> 'static str {   // WRONG
        match self {
            EngineKind::Cuda => "CUDA",
            ...
```

The function returns a string literal `&str` but the type is missing
the `&`. rustc suggests `&'static str`.

**Fix**:

```rust
pub fn name(self) -> &'static str {
```

### Fix 2-5: `whisper_engine/whisper_engine.rs:592-595` — doc comments on function parameters

Lines 592-595 have `///` doc comments placed BEFORE a function
parameter (`initial_prompt: Option<String>`). rustc disallows
doc comments in that position (they'd attach to nothing meaningful).

**Fix**: change `///` to `//` for all 4 lines:

```rust
old_string:
        /// Optional initial_prompt used to bias the decoder toward known
        /// vocabulary (project names, technical terms, etc.). Forwarded to
        /// whisper.cpp `params.set_initial_prompt`. Pass None for the legacy
        /// behavior (no prompt bias).
        initial_prompt: Option<String>,
new_string:
        // Optional initial_prompt used to bias the decoder toward known
        // vocabulary (project names, technical terms, etc.). Forwarded to
        // whisper.cpp `params.set_initial_prompt`. Pass None for the legacy
        // behavior (no prompt bias).
        initial_prompt: Option<String>,
```

### Fix 6: `llm_diagnostics.rs:71` — `LastTestResult.origin` lifetime

The struct `LastTestResult` (line 35 ish) has:
```rust
pub origin: &'static str,
```

This is wrapped in `DiagnosticsSnapshot` (line 69) which derives
`Deserialize`. Deserialization borrows with lifetime `'de`, but
`&'static str` requires `'static` — borrow checker fails.

**Fix**: change `origin: &'static str` to `origin: String`. This
makes the field own its data and works with both `Serialize` and
`Deserialize`.

Update cascade:
- `LastTestResult` field type: `&'static str` → `String`
- `with_origin` method (signature: `pub fn with_origin(mut self, origin: &'static str) -> Self`) — change parameter to `&str` and store `.to_string()`:
  ```rust
  pub fn with_origin(mut self, origin: &str) -> Self {
      self.origin = origin.to_string();
      self
  }
  ```
- `ok()` constructor: change `origin: "manual"` to `origin: "manual".to_string()`
- `failed()` constructor: same change
- Any other call sites of `with_origin` that pass `&'static str` — change to `&str` (the parameter widening from `&'static str` to `&str` is backward compatible)

## Files Changed

| File | Change |
|---|---|
| `frontend/src-tauri/src/whisper_engine/fallback.rs` | Add `&` to `EngineKind::name` return type |
| `frontend/src-tauri/src/whisper_engine/whisper_engine.rs` | 4× `///` → `//` on parameter doc comments |
| `frontend/src-tauri/src/llm_diagnostics.rs` | `origin: &'static str` → `String` + cascade in `with_origin`, `ok`, `failed` |

## Validation

```bash
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | tee quickstart.log | tail -10
```

Expected: `Finished` with no `error:` lines.

## Out of Scope

- whisper-rs / sys / patches — already fixed.
- llama-helper — separate task.
- Spec #1-#4 errors — all fixed.

## Non-Goals

- Refactoring beyond the minimum fix.
- Changing serde formats (changing `&'static str` to `String` does
  change JSON output from a string to a string — visually identical
  but length-flexible).
