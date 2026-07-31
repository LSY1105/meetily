# Onboarding Bugs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two onboarding UX bugs so users can complete model downloads and reach the main app.

**Architecture:** Two small surgical edits in the same file (`DownloadProgressStep.tsx`) and a small follow-up in the context. No design changes, no new flows. Each fix gets its own commit for traceability.

**Tech Stack:** React, Next.js, TypeScript, Tauri 2.x.

**Spec:** `docs/superpowers/specs/2026-07-31-onboarding-bugs-design.md`

**Working directory:** `C:\Users\qjl10\Documents\工作区\meetily` (Windows, bash shell)

---

## File Structure

| File | Responsibility | Touched by |
|---|---|---|
| `frontend/src/components/onboarding/steps/DownloadProgressStep.tsx` | Summary Engine event filter, Continue button | Tasks 1, 2 |
| `frontend/src/contexts/OnboardingContext.tsx` | `completeOnboarding` state update | Task 3 |

---

## Task 1: Loosen the Summary Engine download event filter

**Files:**
- Modify: `frontend/src/components/onboarding/steps/DownloadProgressStep.tsx:263-283`

- [ ] **Step 1: Read the current state**

```bash
awk 'NR>=260 && NR<=290' frontend/src/components/onboarding/steps/DownloadProgressStep.tsx
```

Expected: lines 263-283 contain the event listener that filters on `selectedSummaryModel && model === selectedSummaryModel`.

- [ ] **Step 2: Apply the edit**

Replace the filter and state-update block:

```rust
old_string:     }>('builtin-ai-download-progress', (event) => {
      const { model, progress, downloaded_mb, total_mb, speed_mbps, status, error } = event.payload;
      if (selectedSummaryModel && model === selectedSummaryModel) {
        setSummaryState((prev) => ({
          ...prev,
          status: status === 'completed'
            ? 'completed'
            : status === 'error'
            ? 'error'
            : 'downloading',
          progress,
          downloadedMb: downloaded_mb ?? prev.downloadedMb,
          totalMb: (total_mb ?? prev.totalMb) || getSummaryModelSizeMb(model),
          speedMbps: speed_mbps ?? prev.speedMbps,
          error: status === 'error' ? error : undefined,
        }));

        if (status === 'completed' || progress >= 100) {
          setSummaryModelDownloaded(true);
        }
      }
    });
new_string:     }>('builtin-ai-download-progress', (event) => {
      const { model, progress, downloaded_mb, total_mb, speed_mbps, status, error } = event.payload;
      // Accept the event if it matches the selected model, or if no
      // selected model is set yet (event arrives before the UI hint
      // is wired up). The event's `model` field is authoritative.
      const matchesSelected = selectedSummaryModel
        ? model === selectedSummaryModel
        : true;
      if (matchesSelected) {
        setSummaryState((prev) => ({
          ...prev,
          status: status === 'completed'
            ? 'completed'
            : status === 'error'
            ? 'error'
            : 'downloading',
          progress,
          downloadedMb: downloaded_mb ?? prev.downloadedMb,
          totalMb: (total_mb ?? prev.totalMb) || getSummaryModelSizeMb(model),
          speedMbps: speed_mbps ?? prev.speedMbps,
          error: status === 'error' ? error : undefined,
        }));

        if (status === 'completed' || progress >= 100) {
          setSummaryModelDownloaded(true);
        }
      }
    });
```

- [ ] **Step 3: Verify the change**

```bash
grep -n "matchesSelected\|selectedSummaryModel && model" frontend/src/components/onboarding/steps/DownloadProgressStep.tsx | head -5
```

Expected: `matchesSelected` appears, the old `selectedSummaryModel && model === selectedSummaryModel` pattern is gone.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/onboarding/steps/DownloadProgressStep.tsx
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(onboarding): accept summary download event even before selectedSummaryModel is set

The builtin-ai-download-progress event listener filtered events
by 'selectedSummaryModel && model === selectedSummaryModel',
which skipped the event entirely when selectedSummaryModel was
empty (the common case on first launch). The Summary Engine
card stayed in 'Waiting...' even after the model finished
downloading.

Loosen the filter: if selectedSummaryModel is set, match exactly;
otherwise accept any event. The event's model field is the
source of truth.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Remove page reload after completeOnboarding

**Files:**
- Modify: `frontend/src/components/onboarding/steps/DownloadProgressStep.tsx:367-385`

- [ ] **Step 1: Read the current state**

```bash
awk 'NR>=365 && NR<=395' frontend/src/components/onboarding/steps/DownloadProgressStep.tsx
```

Expected: lines 367-385 contain the `if (isMac) { goNext(); } else { setIsCompleting(true); try { await completeOnboarding(); ... window.location.reload(); } catch { ... } }` block.

- [ ] **Step 2: Apply the edit**

Replace the non-macOS branch to call `onComplete()` instead of reloading:

```rust
old_string:     if (isMac) {
      // macOS: Go to Permissions step (will complete after permissions granted)
      goNext();
    } else {
      // Non-macOS: Complete onboarding immediately (downloads continue in background)
      setIsCompleting(true);
      try {
        await completeOnboarding();

        // Small delay to ensure state is saved before reload
        await new Promise(resolve => setTimeout(resolve, 100));

        window.location.reload();
      } catch (error) {
new_string:     if (isMac) {
      // macOS: Go to Permissions step (will complete after permissions granted)
      goNext();
    } else {
      // Non-macOS: Complete onboarding immediately (downloads continue in background)
      setIsCompleting(true);
      try {
        await completeOnboarding();
        onComplete();
      } catch (error) {
```

The `catch (error) {` block is kept as-is.

- [ ] **Step 3: Verify the `onComplete` prop is in scope**

```bash
grep -n "onComplete\b" frontend/src/components/onboarding/steps/DownloadProgressStep.tsx | head -5
```

Expected: `onComplete` is destructured from props (alongside `completeOnboarding`, `parakeetDownloaded`, etc.). If not destructured, add it to the props destructure at the top of the function.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/onboarding/steps/DownloadProgressStep.tsx
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(onboarding): call onComplete instead of window.location.reload after completeOnboarding

window.location.reload() races with the Tauri store save: the
page reinitializes and reads the old (not-yet-flushed) state,
sending the user back to Setup Overview.

Replace the reload with onComplete(), which propagates the
in-memory completed=true state to the parent (which hides
OnboardingFlow). The store save still happens, but no longer
races with the page reload.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Verify completeOnboarding sets context.completed

**Files:**
- Modify: `frontend/src/contexts/OnboardingContext.tsx` (verify; only edit if broken)

- [ ] **Step 1: Read completeOnboarding**

```bash
awk 'NR>=460 && NR<=510' frontend/src/contexts/OnboardingContext.tsx
```

Expected: lines 464-498 contain `completeOnboarding` calling `await invoke('complete_onboarding', ...)`, then `setCompleted(true)`, then `onComplete()`. (The current code might be missing `onComplete()` — that's the gap.)

- [ ] **Step 2: Apply the fix if missing**

If `setCompleted(true)` is followed by the function ending without calling `onComplete`, add the call. The function should look like this (after the fix):

```typescript
const completeOnboarding = async () => {
  try {
    isCompletingRef.current = true;

    if (saveTimeoutRef.current) {
      clearTimeout(saveTimeoutRef.current);
      saveTimeoutRef.current = undefined;
    }

    let modelToSave = selectedSummaryModel;
    if (!modelToSave) {
      modelToSave = await invoke<string>('builtin_ai_get_recommended_model');
      setSelectedSummaryModel(modelToSave);
    }

    const selectedModelReady = await invoke<boolean>('builtin_ai_is_model_ready', {
      modelName: modelToSave,
      refresh: true,
    });
    setSummaryModelDownloaded(selectedModelReady);
    if (!selectedModelReady) {
      requestSummaryModelDownload(modelToSave);
    }

    await invoke('complete_onboarding', {
      model: modelToSave,
    });
    setCompleted(true);
    if (typeof onComplete === 'function') {
      onComplete();
    }
    console.log('[OnboardingContext] Onboarding completed with model:', modelToSave);
    isCompletingRef.current = false;
    // ... rest of function
  } catch (error) {
    // ...
  }
};
```

- [ ] **Step 3: Verify `onComplete` is in scope**

```bash
grep -n "onComplete" frontend/src/contexts/OnboardingContext.tsx | head -10
```

Expected: `onComplete` is in the `useOnboarding` return value (around line 629, where the context is provided). If not, add it.

- [ ] **Step 4: Commit if changes were made**

```bash
git add frontend/src/contexts/OnboardingContext.tsx
git -c user.email="claude@anthropic.com" -c user.name="Claude" commit -m "fix(onboarding): call onComplete in completeOnboarding

After setting completed=true, explicitly call the onComplete
callback to propagate the in-memory state to the parent.
Without this, the parent component doesn't know to hide
OnboardingFlow until a full page reload — and reloads race
with the Tauri store save, so the user lands back on step 2.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

If no changes were needed (onComplete was already called), skip the commit and document it in the report.

---

## Task 4: Manual verification

**Files:**
- None (verification only)

The Tauri dev server is already running from the previous session. The user can verify by:
1. Reload the app (Ctrl+R inside the Tauri window, or use the system's reload action).
2. The onboarding flow starts from step 1.
3. Walk through Welcome → Setup Overview → Download.
4. Watch the Summary Engine card: it should transition to "completed" within seconds of the download finishing.
5. Click "Continue": the app should transition to the main view, not back to Setup Overview.

If a problem persists, capture the new log output and report. The expected outcome is verified by the user, not by automated tooling.

## Self-Review Checklist

- [x] Spec coverage: bug 1 → Task 1; bug 2 → Tasks 2-3; manual verify → Task 4.
- [x] No placeholders: all file paths exact, all commands complete, all edits show concrete code.
- [x] Type/name consistency: `matchesSelected`, `onComplete`, `setCompleted`, `setSummaryModelDownloaded` consistent across tasks.
- [x] Each task has a commit step.
- [x] No TDD steps — these are UX bug fixes; verification is manual (Task 4).
