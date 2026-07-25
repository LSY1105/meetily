# PR-50: Surface `recording-error` events in the UI

## Problem

The Rust backend fires a `recording-error` Tauri event whenever the audio
pipeline reports an `AudioError` (device disconnect that cannot recover,
channel closed, buffer overflow, permission denied post-start, etc.). The
event payload is `error.user_message()` — a short user-friendly string from
`recording_state::AudioError::user_message()`.

A repo-wide grep confirms **zero frontend listeners** for this event:

```
rg 'recording-error' src/    # 0 hits
```

Net effect: when a meeting is recording and the audio system fails
mid-session, the UI keeps showing "recording" while capture is silently
dead. The user has no signal. This is the highest-impact stability gap in
the recording flow — orphan-checkpoints and recovery banners cover *previous*
sessions, but a crash happening *right now* is invisible.

## Fix (Ponytail: smallest possible)

One new listener component, one mount line in `ClientRootLayout`, two i18n
keys across six locales.

### New: `frontend/src/components/RecordingErrorListener.tsx`

```ts
'use client';

import { useEffect } from 'react';
import { listen } from '@/lib/transport';
import { toast } from 'sonner';
import { useTranslations } from 'next-intl';
import { useRecordingState, RecordingStatus } from '@/contexts/RecordingStateContext';

export function RecordingErrorListener() {
  const t = useTranslations('recording.errors');
  const { setStatus } = useRecordingState();

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      const fn = await listen<string>('recording-error', (event) => {
        const message = typeof event.payload === 'string' && event.payload
          ? event.payload
          : t('unknown_message');
        setStatus(RecordingStatus.ERROR, message);
        toast.error(t('unexpected_stop'), { description: message });
      });
      unlisten = fn;
    })().catch((e) => {
      console.warn('recording-error listener failed', e);
    });
    return () => { unlisten?.(); };
  }, [setStatus, t]);

  return null;
}
```

### Edit: `frontend/src/app/ClientRootLayout.tsx`

Mount `<RecordingErrorListener />` immediately after
`<RecordingStateProvider>` so it picks up `useRecordingState()` from context.
One new import + one JSX line.

### i18n (6 locales)

Two new flat keys appended to the existing `recording.errors.*` namespace
in `frontend/locales/*/recording.json`:

- `recording.errors.unexpected_stop`: "Recording stopped unexpectedly"
- `recording.errors.unknown_message`: "Recording stopped unexpectedly"
  (used as fallback when payload is empty)

(Locale strings: en-US, en-GB, zh-CN, zh-TW, ja-JP, ko-KR. Mirror the
existing tone — short, sentence case.)

## Why this is enough

`RecordingStateProvider` already exposes `setStatus(status, message)` and
already drives the recording status bar / overlay components — they consume
`status === RecordingStatus.ERROR` and react. So:

- Existing status UI shows the error state automatically (no new banner)
- A toast gives an immediate, dismissable notification
- `setStatus` re-syncs with backend on next mount via the existing 500 ms
  polling in `RecordingStateContext`, so a stuck "ERROR" cannot persist
  past restart

No new dependencies, no backend changes, no spec for the Rust side (it
already emits).

## Out of scope

- Replacing `user_message()` with localised strings on the Rust side
  (would require plumbed i18n into the Rust crate; huge surface, not
  needed for this PR)
- Auto-retry / reconnect UX — `AudioError::is_recoverable()` exists in
  Rust but recovery is already wired for device-disconnect; transient
  failures bubble up here for user awareness
- Crash analytics — error events should feed `track_meeting_ended`
  already (via `state.has_fatal_error()`); not in scope

## Verification

1. CI: `i18n-check/Check i18n completeness + build` passes (no schema
   changes, no new exports, no new deps)
2. Manual smoke (per existing PR-46b / PR-46c style): simulate a runtime
   error by killing the mic permission mid-session — toast should appear
   with the localised message and the status pill should flip to ERROR
3. After PR-49's dev-mode fix, the listener registration no longer races

## Risk

Minimal. Pure additive UI listener. If the Rust side never emits
`recording-error` (e.g. when not recording), `listen()` no-ops. The
listener is mounted once, cleans up on unmount.
