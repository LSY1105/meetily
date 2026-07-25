# PR-52: Shared `ModelDownloadStatus` UI across the 3 model managers

## Problem (revised scope)

The 3 model managers — `WhisperModelManager.tsx` (25 KB),
`ParakeetModelManager.tsx` (21 KB), `BuiltInModelManager.tsx` (18 KB),
totalling ~65 KB — each render their own download progress UI:
percentage text, animated bar, status badge, cancel button.

A repo-wide grep shows each file re-implements the same JSX block
("X% downloaded", `<Progress value={n} />`, etc.) with subtle
inconsistencies (different shade of green, different placeholder text,
different rounding on the percentage display). The full managers have
non-trivial state machines and Rust-side differences (different
`api_*` commands, different model lists), so a deep refactor is out of
scope.

**Fix (Ponytail: smallest possible)**

Extract only the **display** of "currently downloading this model at X%"
into one shared `<ModelDownloadStatus model={...} onCancel={...} />`
component (~60 lines). All 3 managers keep their state + command
plumbing; only the JSX rendering is unified.

### New: `frontend/src/components/ModelDownloadStatus.tsx`

```tsx
'use client';

import { useTranslations } from 'next-intl';
import { Loader2, X } from 'lucide-react';
import { Button } from './ui/button';
import { Progress } from './ui/progress';

interface ModelDownloadStatusProps {
  modelName: string;
  progress: number;        // 0..100
  onCancel?: () => void;
}

export function ModelDownloadStatus({ modelName, progress, onCancel }: ModelDownloadStatusProps) {
  const t = useTranslations('settings.transcript.models');
  const pct = Math.max(0, Math.min(100, Math.round(progress)));
  return (
    <div className="flex items-center gap-3 py-2" data-testid={`download-status-${modelName}`}>
      <Loader2 className="h-4 w-4 animate-spin text-blue-500 flex-shrink-0" aria-hidden />
      <div className="flex-1 min-w-0">
        <Progress value={pct} className="h-2" aria-label={t('downloading', { name: modelName })} />
        <p className="text-xs text-gray-500 mt-1">
          {t('downloading_pct', { name: modelName, pct })}
        </p>
      </div>
      {onCancel && (
        <Button
          variant="ghost"
          size="sm"
          onClick={onCancel}
          aria-label={t('cancel_download', { name: modelName })}
          title={t('cancel_download', { name: modelName })}
        >
          <X className="h-4 w-4" />
        </Button>
      )}
    </div>
  );
}
```

### Edits

In each of the 3 managers:

1. Import `ModelDownloadStatus` from `'@/components/ModelDownloadStatus'`
2. Replace the existing in-file progress JSX (typically `<Progress
   value={X} />` + `<span>{X}%</span>` + cancel button) with one line:
   `<ModelDownloadStatus modelName={m.name} progress={X} onCancel={...} />`
3. Net change: -20 to -40 lines per file (most files have duplicate JSX
   paths for both list and detail views)

### i18n (6 locales)

3 new flat keys under `settings.transcript.models.*`:

- `downloading`: "Downloading {name}"
- `downloading_pct`: "{name} — {pct}%"
- `cancel_download`: "Cancel download of {name}"

(Translations for en-US / en-GB / zh-CN / zh-TW / ja-JP / ko-KR.)

## Why this is enough

- The 3 managers still own their own data, state, and Rust commands
  (different `api_*` invocations, different model catalogues)
- The unified UI guarantees consistent look across Whisper / Parakeet /
  BuiltIn download flows
- One component = one place to tweak styling, accessibility, or copy
  later

## Out of scope (deferred)

- Refactoring the 3 managers into a shared `useModelManager` hook
- Extracting the model list rendering (each manager has different
  column sets and per-model metadata)
- Replacing per-manager delete / select buttons with shared ones (each
  has unique state side-effects)

## Verification

1. CI: `i18n-check/Check i18n completeness + build` passes
2. Manual smoke: start a Whisper model download, confirm the new
   `<ModelDownloadStatus>` shows progress; same for Parakeet and
   BuiltIn; visual parity across all 3

## Risk

Low. Pure UI extraction. Each manager's behavior is unchanged; only
the JSX rendering differs. If a manager's progress state shape is
unexpected, the component will just render `0%` (graceful fallback).
