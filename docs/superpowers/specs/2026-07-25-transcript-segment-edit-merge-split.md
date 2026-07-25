# PR-51: Transcript segment edit / merge / split (zero backend)

## Problem

The transcript view in `VirtualizedTranscriptView` is read-only. A
mis-transcribed word, a run-on speaker boundary, or an accidental combined
utterance cannot be corrected without re-running the full transcription.
Users regularly paste or export and edit externally, then lose alignment
with the audio timestamps.

## Fix (Ponytail: smallest possible, zero backend)

Add inline edit / merge / split to each segment using the **same edit-state
machinery** that already powers speaker rename in `TranscriptSegment`:

- Click the segment text → swap to a `<textarea>` (matches existing rename
  input pattern)
- `Esc` cancels, `Ctrl/Cmd+Enter` saves
- Typing a literal `|` marker in the textarea enables split at that
  boundary on save
- A small `...` menu next to the timestamp exposes "Merge with next" (only
  enabled when the next segment has the same speaker)

All three operations call **the existing `storageService.saveMeeting()`**
which writes the whole transcript to SQLite via `api_save_transcript`. No
new Rust commands, no new schema, no migration.

### New: `frontend/src/hooks/useTranscriptMutations.ts`

```ts
export function useTranscriptMutations(opts: {
  meetingId: string;
  folderPath: string | null;
  meetingTitle: string;
  segments: TranscriptSegmentData[];
  onSaved: (next: TranscriptSegmentData[]) => void;
}) {
  // editSegmentText(id, text)   -> updates + saves
  // mergeWithNext(id)           -> combines same-speaker pair + saves
  // splitSegment(id, position)  -> splits at char position + saves
  // Each calls storageService.saveMeeting() and toasts on result
}
```

### Edits

- `frontend/src/components/VirtualizedTranscriptView.tsx`
  - `TranscriptSegment` gains `isEditing` + `draftText` state (mirrors
    `isRenaming` already present)
  - Text node becomes a button that toggles edit mode on click
  - In edit mode, a `textarea` shows the draft; `Esc` cancels, `Cmd/Ctrl+
    Enter` saves
  - A 3-dot menu (`Popover` from existing UI) exposes "Merge with next"
    (disabled when speaker differs) and "Split at `|`" (visible only when
    edit draft contains `|`)
  - All three operations call back to the parent through the existing
    `onSegmentsChange` prop chain (passed through `TranscriptPanel` to the
    hook above)

- `frontend/src/components/MeetingDetails/TranscriptPanel.tsx`
  - Wires `useTranscriptMutations` and forwards its `onSaved` to
    `onRefetchTranscripts` (already a prop) — local state is replaced
    with the saved result, no new state needed in the panel

### i18n (6 locales)

7 new flat keys appended to `settings.transcript.*` namespace (already has
50+ keys, mirror the tone):

- `transcript.segment.edit`: "Edit segment"
- `transcript.segment.save`: "Save"
- `transcript.segment.cancel`: "Cancel"
- `transcript.segment.merge_with_next`: "Merge with next"
- `transcript.segment.split_at_marker`: "Split at |"
- `transcript.segment.menu`: "Segment actions"
- `transcript.segment.save_hint`: "Ctrl+Enter to save, Esc to cancel"

(Localised for en-US / en-GB / zh-CN / zh-TW / ja-JP / ko-KR.)

## Why this is enough

- `Transcript` type is `[key: string]: any` — the backend stores an opaque
  JSON blob; any field shape we produce round-trips fine
- `storageService.saveMeeting()` already exists and is the canonical save
  path for the meeting details page (and for incremental recording saves)
- Existing rename UX pattern (button → input → save/cancel) is the proven
  template; copy it

## Out of scope

- Drag-to-reorder segments (next-iteration UX)
- Multi-segment bulk edit (paste-replace across N)
- Speaker reassignment (already covered by existing `onSpeakerRename`)
- Undo/redo (defer; one-step undo per save via Sonner toast action is a
  reasonable follow-up)
- Backend `update_segment` command (no need; full-meeting save is fast
  enough for typical meeting sizes and already happens on stop)

## Verification

1. CI: `i18n-check/Check i18n completeness + build` passes
2. Manual smoke per existing PR-46b/46c style (3 scenarios):
   - Click segment text → edit → Cmd+Enter → toast success → refresh page,
     edit persists
   - Same-speaker adjacent pair → menu → Merge with next → second segment
     disappears, first segment shows combined text
   - Edit mode → type `foo | bar` → Save → two segments appear

## Risk

Minimal. Pure additive UI. Existing rename code path is the template; the
edit path follows the same hook structure. Persistence goes through a
command that the rest of the app already exercises thousands of times per
session.
