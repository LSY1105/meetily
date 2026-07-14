# Transcript Highlight (Wave 14 PR-44a)

## Overview

Lightweight in-transcript-view keyword / pattern highlighting. Pure
frontend -- no LLM, no backend calls. PR-44a ships the rendering
infrastructure and Settings UI; full pipeline (custom keywords from
postprocess, per-category toggles) lands in PR-44a-ii.

## Categories

| Category | Pattern | Color (Tailwind) |
|---|---|---|
| `number` | 1,234 / 50% / ¥100 / 百分之二十 | amber-100 / amber-900 |
| `date` | 2026-07-14 / 明天 / 下周三 | emerald-100 / emerald-900 |
| `url` | http(s) URLs / emails | sky-100 / sky-900 |
| `proper` | Acme / ProjectX (English only) | violet-100 / violet-900 |
| `custom` | user-supplied keywords (comma-separated) | rose-100 / rose-900 |

## Usage

```tsx
import { HighlightSettings, DEFAULT_HIGHLIGHT_CONFIG } from "@/components/HighlightSettings";

// in settings page:
const [hl, setHl] = useState(DEFAULT_HIGHLIGHT_CONFIG);
<HighlightSettings config={hl} setConfig={setHl} />

// in transcript panel:
<VirtualizedTranscriptView segments={...} highlightConfig={hl} />
```

Disable globally: pass `{ enabled: false, customKeywords: "" }`.

## Implementation

- `frontend/src/lib/transcriptHighlight.ts` -- pure-function tokenizer
  with 5 category patterns + custom keyword injection. Linear scan
  (no catastrophic-backtracking risk).
- `frontend/src/components/HighlightSettings.tsx` -- settings UI.
- `frontend/src/components/MeetingDetails/TranscriptPanel.tsx` --
  forwards highlightConfig through to VirtualizedTranscriptView.
- `frontend/src/components/VirtualizedTranscriptView.tsx` -- tokens
  rendered as <mark class="hl-...">...</mark>.

## Coverage

| Locale | highlight.* keys |
|---|---|
| en-US / en-GB / zh-CN / zh-TW | title / description / enable_label / custom_label / _placeholder / _help / legend_number / legend_date / legend_proper / legend_custom |

ja-JP / ko-KR follow after Wave 9/10 merge into stability-wave8.

## Roadmap

| PR | Scope |
|---|---|
| 44a (this) | infra + UI settings |
| 44b | LLM keyword extraction + click-to-search |
| 44c | timestamp click-to-jump audio playback |
