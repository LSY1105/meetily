# Misc Build Fixes #2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 7 remaining pre-existing compile errors in devtest so cargo check passes (after misc-build-fixes spec #1 also lands).

**Architecture:** Single-line / single-block fixes for type-mismatch and import-path errors. Each fix is mechanical — change a return type, swap a method call, or split an import. No design decisions; the spec lists exact replacements.

**Tech Stack:** Rust (cargo), Tauri 2.x, nalgebra 0.32.

**Spec:** `docs/superpowers/specs/2026-07-28-meetily-misc-build-fixes-2-design.md`

**Working directory:** `C:\Users\qjl10\Documents\工作区\meetily` (Windows, bash shell)

**Prerequisite:** Spec #1 (commit `bd734fb`) must be implemented first.

---

## File Structure

| File | Responsibility | Touched by |
|---|---|---|
| `frontend/src-tauri/src/diarization/offline.rs` | f32/f64 type fix | Task 1 |
| `frontend/src-tauri/src/diarization/clustering.rs` | nalgebra API fix | Task 2 |
| `frontend/src-tauri/src/summary/processor.rs` | String → LLMError conversion | Task 3 |
| `frontend/src-tauri/src/summary/failover.rs` | String → LLMError conversion | Task 4 |
| `frontend/src-tauri/src/llm_health.rs` | Import path split | Task 5 |

---

## Task 1: Fix f32/f64 mismatch in diarization/offline.rs

**Files:**
- Modify: `frontend/src-tauri/src/diarization/offline.rs:65`

- [ ] **Step 1: Verify current state**

```bash
grep -n "best_dist" frontend/src-tauri/src/diarization/offline.rs | head -10
```

Expected: line 65 shows `let mut best_dist = f32::MAX;` and lines 63 show `(seg_start + seg_end) / 2.0` is f64 (since `seg_start` and `seg_end` come from `TranscriptsRepository::fetch_segment_times` which returns i64 timestamps converted to f64).

- [ ] **Step 2: Apply the edit**

```bash
sed -i 's/let mut best_dist = f32::MAX;/let mut best_dist = f64::MAX;/' frontend/src-tauri/src/diarization/offline.rs
```

- [ ] **Step 3: Verify the edit**

```bash
grep -n "best_dist" frontend/src-tauri/src/diarization/offline.rs | head -10
```

Expected: `let mut best_dist = f64::MAX;`

- [ ] **Step 4: Run cargo check (just this file's errors should disappear)**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "diarization/offline\.rs|E0308" | head -10
```

Expected: no errors referencing `diarization/offline.rs`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/diarization/offline.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(diarization): use f64 for best_dist to match segment timestamps

TranscriptsRepository::fetch_segment_times returns i64 timestamps
that get converted to f64 for midpoint computation. best_dist was
declared f32, causing a type mismatch when comparing against the
f64 midpoint difference.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Fix nalgebra DVector collection in diarization/clustering.rs

**Files:**
- Modify: `frontend/src-tauri/src/diarization/clustering.rs:49`

- [ ] **Step 1: Verify current state**

```bash
awk 'NR==49' frontend/src-tauri/src/diarization/clustering.rs
```

Expected:
```rust
let deg: DVector<f32> = w.row_iter().map(|r| r.sum()).collect();
```

- [ ] **Step 2: Apply the edit**

Read the file's full line 49 context, then use Edit to replace the line:

```rust
old_string: let deg: DVector<f32> = w.row_iter().map(|r| r.sum()).collect();
new_string: let deg: DVector<f32> = DVector::from_iterator(w.nrows(), w.row_iter().map(|r| r.sum()));
```

- [ ] **Step 3: Verify the edit**

```bash
awk 'NR==49' frontend/src-tauri/src/diarization/clustering.rs
```

Expected:
```rust
let deg: DVector<f32> = DVector::from_iterator(w.nrows(), w.row_iter().map(|r| r.sum()));
```

- [ ] **Step 4: Verify no errors from clustering.rs**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "diarization/clustering\.rs|E0277.*nalgebra" | head -5
```

Expected: empty.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/diarization/clustering.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(diarization): use DVector::from_iterator for row sums

nalgebra 0.32's DVector does not implement FromIterator<f32>.
Use DVector::from_iterator with explicit length instead.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Convert String errors to LLMError in summary/processor.rs

**Files:**
- Modify: `frontend/src-tauri/src/summary/processor.rs:538` and `:585`

- [ ] **Step 1: Verify both error sites**

```bash
grep -nE 'return Err\(".*\.to_string\(\)' frontend/src-tauri/src/summary/processor.rs | head -10
```

Expected: at least 2 lines, one at line 538 (`Summary generation was cancelled`) and one at line 585 (also cancellation-string pattern).

- [ ] **Step 2: Read the surrounding context for both sites**

```bash
awk 'NR>=534 && NR<=545' frontend/src-tauri/src/summary/processor.rs
echo "---"
awk 'NR>=580 && NR<=590' frontend/src-tauri/src/summary/processor.rs
```

Read both call sites. Determine for each: is the error string `"Summary generation was cancelled"` (use `LLMError::Cancelled`) or some other text (use `LLMError::Other(text)`)?

- [ ] **Step 3: Apply edits**

Use Edit tool for each. Two patterns:

**For `"Summary generation was cancelled":**

```rust
old_string: return Err("Summary generation was cancelled".to_string());
new_string: return Err(LLMError::Cancelled);
```

**For other cancellation/error strings:**

```rust
old_string: return Err("<original text>".to_string());
new_string: return Err(LLMError::Other("<original text>".to_string()));
```

Apply each edit. Verify `LLMError` is already imported (it should be from PR-43 commit `87c6af0`):

```bash
grep -n "use.*LLMError" frontend/src-tauri/src/summary/processor.rs | head -3
```

- [ ] **Step 4: Verify no errors from processor.rs**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "summary/processor\.rs:5[38][58]" | head -5
```

Expected: empty.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/summary/processor.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(summary): convert String errors to typed LLMError in processor.rs

After PR-43, generate_meeting_summary returns Result<_, LLMError>.
Two remaining sites still returned Err(\"...\".to_string()) which
produces String. Convert to LLMError::Cancelled (cancellation
strings) or LLMError::Other(...) for other error strings.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Convert String errors to LLMError in summary/failover.rs

**Files:**
- Modify: `frontend/src-tauri/src/summary/failover.rs:83` and `:90`

- [ ] **Step 1: Verify both error sites**

```bash
grep -nE 'return Err\(".*\.to_string\(\)' frontend/src-tauri/src/summary/failover.rs | head -10
```

Expected: at least 2 lines, one at line 83 (`Provider chain is empty`) and one at line 90 (`Summary generation was cancelled`).

- [ ] **Step 2: Read context for both sites**

```bash
awk 'NR>=80 && NR<=95' frontend/src-tauri/src/summary/failover.rs
```

Verify `LLMError` is imported:

```bash
grep -n "use.*LLMError" frontend/src-tauri/src/summary/failover.rs | head -3
```

If not imported, add to the `use` declarations at the top of the file.

- [ ] **Step 3: Apply edits**

Two patterns:

**Line 83 (`Provider chain is empty`):**

```rust
old_string: return Err("Provider chain is empty".to_string());
new_string: return Err(LLMError::Other("Provider chain is empty".to_string()));
```

**Line 90 (`Summary generation was cancelled`):**

```rust
old_string: return Err("Summary generation was cancelled".to_string());
new_string: return Err(LLMError::Cancelled);
```

- [ ] **Step 4: Verify no errors from failover.rs**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "summary/failover\.rs:(83|90)" | head -5
```

Expected: empty.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/summary/failover.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(summary): convert String errors to typed LLMError in failover.rs

generate_with_failover returns Result<String, LLMError>. Two sites
still returned Err(\"...\".to_string()). Use LLMError::Other for
the empty-chain case and LLMError::Cancelled for cancellation.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Split import in llm_health.rs

**Files:**
- Modify: `frontend/src-tauri/src/llm_health.rs:16`

- [ ] **Step 1: Verify current state**

```bash
awk 'NR==16' frontend/src-tauri/src/llm_health.rs
```

Expected:
```rust
use crate::llm_postprocess::{generate_summary, http_client, load_provider_inputs};
```

- [ ] **Step 2: Apply the edit**

`generate_summary` is defined in `summary::llm_client`. `llm_postprocess`
imports it but doesn't re-export (no `pub use`). Split the import so
`generate_summary` comes directly from the definition site:

```rust
old_string: use crate::llm_postprocess::{generate_summary, http_client, load_provider_inputs};
new_string: use crate::summary::llm_client::generate_summary;
use crate::llm_postprocess::{http_client, load_provider_inputs};
```

(Apply as one Edit replacing the single line with two lines.)

- [ ] **Step 3: Verify no errors from llm_health.rs**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | grep -E "llm_health\.rs" | head -5
```

Expected: empty.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/llm_health.rs
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(llm_health): import generate_summary from llm_client, not llm_postprocess

llm_postprocess.rs has 'use crate::summary::llm_client::{generate_summary, ...}'
but a use is not a re-export unless marked 'pub use'. Import
generate_summary directly from llm_client.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Cascade verification

**Files:**
- None (verification only)

- [ ] **Step 1: Run full cargo check**

```bash
cd /c/Users/qjl10/Documents/工作区/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc 2>&1 | tee quickstart.log | tail -30
```

- [ ] **Step 2: Check for the 4 expected-to-resolve cascade errors**

```bash
grep -E "current_diarization_buffer|initial_prompt|protect_terms|restore_protected_terms" quickstart.log | head -10
```

Expected: these errors should **NOT appear**. They are parser-cascade
errors that resolve once the real underlying errors are fixed.

If any of these errors **still appear**, report BLOCKED. They need a
separate fix.

- [ ] **Step 3: Check total error count**

```bash
grep -c "error\[E" quickstart.log
```

Compare against the pre-spec baseline. After misc-build-fixes spec #1
(this depends on it landing first) AND this spec, the error count
should drop significantly.

If errors remain, categorize them by error code:
```bash
grep -oE "error\[E[0-9]+\]" quickstart.log | sort | uniq -c | sort -rn
```

Report remaining errors. If they fall into a clear pattern (e.g.,
whisper-rs system or llama-helper) and the user wants to fix them,
write a follow-up spec.

---

## Self-Review Checklist

- [x] Spec coverage: offline.rs f64 → Task 1; clustering.rs nalgebra → Task 2; processor.rs String → LLMError → Task 3; failover.rs String → LLMError → Task 4; llm_health.rs import split → Task 5; cascade verification → Task 6.
- [x] No placeholders: all file paths exact, all commands complete.
- [x] Type/name consistency: `LLMError::Cancelled`, `LLMError::Other`, `DVector::from_iterator`, `f64::MAX` consistent across tasks.
- [x] Each task has a commit step (frequent commits).
- [x] No TDD steps because these are mechanical fixes — verification is end-to-end build success (Task 6).
