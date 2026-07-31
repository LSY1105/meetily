# Design: Fix 2 onboarding bugs (summary engine wait + onboarding loop)

**Date**: 2026-07-31
**Branch**: devtest
**Status**: Approved

## Problem

Two UX-blocking bugs in the onboarding flow prevent users from reaching
the main app after the model downloads complete.

### Bug 1: Summary Engine shows "Waiting..." forever after download completes

`DownloadProgressStep.tsx:265` filters the `builtin-ai-download-progress`
event:

```typescript
if (selectedSummaryModel && model === selectedSummaryModel) {
    // setSummaryState({...})
    if (status === 'completed' || progress >= 100) {
      setSummaryModelDownloaded(true);
    }
}
```

If `selectedSummaryModel` is empty when the download event arrives,
the `if` guard skips both state update AND `setSummaryModelDownloaded(true)`.
The Summary Engine card stays in "Waiting..." state forever, even
though the model file is fully downloaded on disk (confirmed: 2,613 MiB
Qwen3.5-4B-Q4_K_M.gguf at the expected path, in 10% size band).

By contrast, the Parakeet handler at line 207 uses a hardcoded constant
(`PARAKEET_MODEL`) so it always matches.

### Bug 2: Onboarding resets to step 2 after clicking Continue

The flow on Windows (non-macOS):
1. User clicks "Get Started" on step 1 → `setCurrentStep(2)`
2. User clicks "Let's Go" on step 2 → `setCurrentStep(3)`
3. Models download; user clicks "Continue" → `handleContinue()` →
   `completeOnboarding()` → `window.location.reload()`
4. After reload, `OnboardingContext` re-initializes from
   `get_onboarding_status` which returns the saved state

If the saved state shows `completed: true`, the user should land on
the main app. If it shows `currentStep: 2, completed: false`, the user
lands back on the Setup Overview screen (the observed bug).

Root cause candidates (need investigation at implementation time):
- The Tauri `complete_onboarding` command fails silently
- The store save doesn't flush before the page reload
- The `get_onboarding_status` is being called before the save is
  visible

A previous fix attempt (commit `7aaf94a`) was reverted: it added
`useState(false)` for `isCompletingRef.current` in the
`useOnboarding()` provider, but the flag is set to `true` only inside
`completeOnboarding()` itself, and the page reload races with the
backend save.

## Goal

After this spec's implementation:
- Summary Engine card shows "completed" within seconds of the download
  finishing
- Clicking Continue after both downloads finishes successfully
  transitions to the main app (not back to Setup Overview)

## Approach

### Fix 1: Summary Engine state update should not require selectedSummaryModel

Change the event filter at `DownloadProgressStep.tsx:265` to use the
actual `event.model` value to update state, with `selectedSummaryModel`
as a fallback for marking downloaded.

```typescript
// Before:
if (selectedSummaryModel && model === selectedSummaryModel) {
    setSummaryState({...});
    if (status === 'completed' || progress >= 100) {
      setSummaryModelDownloaded(true);
    }
}

// After:
const isSummaryEvent = (eventModel: string) => {
  // The event's `model` field is authoritative; selectedSummaryModel
  // is a UI hint that may not be set yet. Match on:
  //   1. exact equality with selectedSummaryModel (when set)
  //   2. both being non-empty and the event's model name is one of
  //      the configured summary model identifiers
  if (selectedSummaryModel && model === selectedSummaryModel) return true;
  // Best-effort match for events arriving before selectedSummaryModel
  // is wired up: any model whose name starts with "qwen" or matches
  // a downloaded file
  return model.startsWith('qwen') || model.includes('4b') || model.includes('2b');
};

if (isSummaryEvent(model)) {
    setSummaryState({...});
    if (status === 'completed' || progress >= 100) {
      setSummaryModelDownloaded(true);
    }
}
```

Alternative (simpler) approach: just match on `model` being a non-empty
string and trust the event:

```typescript
if (model && (model === selectedSummaryModel || !selectedSummaryModel)) {
    // update state
}
```

Choose the simpler one. The point is: if `selectedSummaryModel` is
empty but the event is for a model we're downloading, still update
state.

### Fix 2: Onboarding completion is race-free

Two parts:

**(a) Don't reload the page after `completeOnboarding`.** The reload
races with the store save. Instead, update the in-memory state to
`completed: true` and call `onComplete()` directly:

```typescript
// Before (line 371-379):
if (isMac) {
  goNext();
} else {
  setIsCompleting(true);
  try {
    await completeOnboarding();
    await new Promise(resolve => setTimeout(resolve, 100));
    window.location.reload();   // ← remove this
  } catch (error) { ... }
}

// After:
if (isMac) {
  goNext();
} else {
  setIsCompleting(true);
  try {
    await completeOnboarding();   // this sets context's `completed` to true
    onComplete();                  // this hides the OnboardingFlow
  } catch (error) { ... }
}
```

The `onComplete` prop is the existing escape hatch (defined in
`OnboardingFlow.tsx:11`).

**(b) Make `completeOnboarding` reliable on the backend.** The current
implementation saves then sets `completed = true` in memory. The save
can fail silently if the store write fails. Add explicit error
propagation:

```rust
// In onboarding.rs complete_onboarding, after save_onboarding_status:
save_onboarding_status(&app, &status).await
    .map_err(|e| format!("Failed to save completed onboarding status: {}", e))?;
```

This is already there (line 216-218). No change needed; just verify the
error path is being honored in the frontend.

If `completeOnboarding` in OnboardingContext.tsx:464 throws (which it
should now), the catch in handleContinue:379 will log it. The user
should see the error rather than being silently sent back to step 2.

## Files Changed

| File | Change |
|---|---|
| `frontend/src/components/onboarding/steps/DownloadProgressStep.tsx` | Loosen event filter at line 265; remove `window.location.reload()` at line 379; call `onComplete()` instead |
| `frontend/src/contexts/OnboardingContext.tsx` | Verify `completeOnboarding` (line 464) sets `completed = true` and calls `onComplete` |

## Validation

1. Run the dev server with `pnpm tauri:dev`
2. Open the app → onboarding starts
3. Download both Parakeet and Summary models
4. Verify Summary Engine card shows "completed" within 5 seconds of
   download finishing (not "Waiting..." anymore)
5. Click "Continue" → app transitions to main view, **not** back to
   Setup Overview
6. Re-launch the app — should skip the onboarding entirely (completed
   state persisted)

## Out of Scope

- Refactoring OnboardingContext (larger restructuring)
- Changing the model download flow itself
- Frontend (TypeScript) build issues unrelated to onboarding

## Non-Goals

- Not changing the model selection logic
- Not changing the order of onboarding steps
- Not adding new onboarding steps
