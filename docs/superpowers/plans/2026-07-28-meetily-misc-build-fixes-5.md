# Misc Build Fixes #5 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the final 6 pre-existing compile errors so cargo check passes cleanly.

**Architecture:** Single-file mechanical fixes — missing `&` on return type, doc comments on function parameters changed to `//`, and one field type change with cascade updates.

**Tech Stack:** Rust (cargo), Tauri 2.x, serde.

**Spec:** `docs/superpowers/specs/2026-07-28-meetily-misc-build-fixes-5-design.md`

**Working directory:** `C:\Users\qjl10\Documents\工作区\meetily` (Windows, bash shell)

**Prerequisite:** Specs #1-#4 must have already landed (they did).

---

## File Structure

| File | Responsibility | Touched by |
|---|---|---|
| `frontend/src-tauri/src/whisper_engine/fallback.rs` | Add `&` to return type | Task 1 |
| `frontend/src-tauri/src/whisper_engine/whisper_engine.rs` | Change `///` to `//` on parameter docs | Task 2 |
| `frontend/src-tauri/src/llm_diagnostics.rs` | `&'static str` → `String` cascade | Task 3 |

---

## Task 1: Add `&` to EngineKind::name return type

**Files:**
- Modify: `frontend/src-tauri/src/whisper_engine/fallback.rs:22`

- [ ] **Step 1: Verify current state**

```bash
awk 'NR>=21 && NR<=30' frontend/src-tauri/src/whisper_engine/fallback.rs
```

Expected: line 22 shows `pub fn name(self) -> 'static str {`.

- [ ] **Step 2: Apply the edit**

```rust
old_string: pub fn name(self) -> 'static str {
new_string: pub fn name(self) -> &'static str {
```

- [ ] **Step 3: Verify the edit**

```bash
awk 'NR==22' frontend/src-tauri/src/whisper_engine/fallback.rs
```

Expected: `pub fn name(self) -> &'static str {`

- [ ] **Step 4: Verify fallback.rs compiles clean**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "fallback\.rs" | head -5
```

Expected: no errors referencing `fallback.rs`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/whisper_engine/fallback.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(whisper_engine): add & to EngineKind::name return type

The function returns a string literal but the type was missing &,
so rustc interpreted 'static str as a type name. Add the borrow.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Change `///` to `//` on parameter doc comments

**Files:**
- Modify: `frontend/src-tauri/src/whisper_engine/whisper_engine.rs:592-595`

- [ ] **Step 1: Verify current state**

```bash
awk 'NR>=588 && NR<=598' frontend/src-tauri/src/whisper_engine/whisper_engine.rs
```

Expected: lines 592-595 start with `///` and lines 596 is `initial_prompt: Option<String>`.

- [ ] **Step 2: Apply the edit**

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

- [ ] **Step 3: Verify the edit**

```bash
awk 'NR>=590 && NR<=598' frontend/src-tauri/src/whisper_engine/whisper_engine.rs
```

Expected: all 4 comment lines start with `//` (not `///`).

- [ ] **Step 4: Verify whisper_engine.rs compiles clean**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "whisper_engine\.rs:59[2-5]" | head -5
```

Expected: no errors referencing those lines.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/whisper_engine/whisper_engine.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(whisper_engine): change /// to // on parameter doc comments

rustc disallows doc comments on function parameters (they would
attach to nothing meaningful). Use plain // comments instead.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Change `LastTestResult.origin` from `&'static str` to `String`

**Files:**
- Modify: `frontend/src-tauri/src/llm_diagnostics.rs` (field type + cascade updates)

- [ ] **Step 1: Read the full struct and its use sites**

```bash
grep -n "origin" frontend/src-tauri/src/llm_diagnostics.rs
```

Expected: shows the field declaration (~line 47), `with_origin` method (~line 59), `ok`/`failed` constructors (~lines 51, 55).

- [ ] **Step 2: Read the full LastTestResult block**

```bash
awk 'NR>=33 && NR<=65' frontend/src-tauri/src/llm_diagnostics.rs
```

- [ ] **Step 3: Apply the edits**

Three edits:

**3a. Change the field type:**

```rust
old_string:     pub origin: &'static str,
new_string:     pub origin: String,
```

**3b. Update `with_origin` method:**

```rust
old_string:     pub fn with_origin(mut self, origin: &'static str) -> Self {
        self.origin = origin;
        self
new_string:     pub fn with_origin(mut self, origin: &str) -> Self {
        self.origin = origin.to_string();
        self
```

**3c. Update `ok` constructor:**

```rust
old_string:         Self { ok: true, latency_ms, code: None, message: None, ts: now_ts(), origin: "manual" }
new_string:         Self { ok: true, latency_ms, code: None, message: None, ts: now_ts(), origin: "manual".to_string() }
```

**3d. Update `failed` constructor:**

```rust
old_string:             origin: "manual",
new_string:             origin: "manual".to_string(),
```

(There are two `origin: "manual",` lines in `failed()` — both need the same change. If your Edit tool can't do replace_all with multiple matches, apply the Edit twice with unique context.)

- [ ] **Step 4: Verify llm_diagnostics.rs compiles clean**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "llm_diagnostics\.rs" | head -5
```

Expected: no errors referencing `llm_diagnostics.rs`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/llm_diagnostics.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(llm_diagnostics): change LastTestResult.origin from &'static str to String

Deserialize borrows with lifetime 'de; &'static str requires
'static which doesn't hold for deserialized data. Switch to String
(owned) so the field works with both Serialize and Deserialize.

Cascade: with_origin takes &str and stores .to_string(); ok() and
failed() constructors use 'manual'.to_string().

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Final cargo check verification

**Files:**
- None (verification only)

- [ ] **Step 1: Run cargo check**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | tee quickstart.log | tail -10
```

- [ ] **Step 2: Confirm zero errors**

```bash
grep -c "^error\b" quickstart.log
```

Expected: 0 errors.

(Note: count lines starting with bare `error:` — not `error[Exxxx]` since this stage of compile errors is at the line-summary level.)

If any errors remain:
```bash
awk '/^error/{err=$0; getline; print err" ::: "$0}' quickstart.log | head -20
```

Report any remaining errors.

---

## Self-Review Checklist

- [x] Spec coverage: fallback.rs `&` → Task 1; whisper_engine.rs `///` → Task 2; llm_diagnostics.rs String cascade → Task 3; final verification → Task 4.
- [x] No placeholders: all file paths exact, all commands complete, all edits shown.
- [x] Type/name consistency: `&'static str`, `String`, `origin`, `with_origin` consistent across tasks.
- [x] Each task has a commit step.
- [x] No TDD steps — mechanical fixes; verification is end-to-end build success (Task 4).
