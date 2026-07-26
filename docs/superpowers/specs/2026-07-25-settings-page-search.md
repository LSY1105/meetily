# PR-52: Settings page IA + search

## Problem

The Settings page currently has 7 top-level tabs (`general / recording / transcript / summary / models / about / beta`) and several modal dialogs that open on demand (`ModelSettingsModal` is 1387 lines and is the same dialog reached from two different tabs). A user opening Settings has to scan 7 labels and remember which one hides the setting they want. The page has no search.

Modal overlap is a separate problem and is deferred — this PR is the IA fix that lands first.

## Goal

Cut the time it takes to reach a setting by adding a single search box that filters the existing tab list and reorders the tabs by predicted usefulness.

## Scope

In:

1. **Search input** above the tab list, matching on the localized label and (when known) on the i18n key. Empty input shows all tabs.
2. **Tab reorder** by likely-use frequency, top to bottom:
   `recording → transcript → models → summary → general → beta → about`
3. **First-run tab pinning**: the previously selected tab is restored on mount (in-memory only — no persistence; this is already how the component behaves, just making it explicit in the spec).

Out (deferred to a future PR):

- Replacing `ModelSettingsModal` with inline sections — separate spec required.
- Search inside a tab's body (not the tab list).
- Keyboard shortcuts (`Cmd+,`).
- Settings card restyling.

## Design

### Search

- `<Input>` (already in `components/ui/input.tsx`) at the top of the sidebar.
- Localized placeholder: `settings.shell.search_placeholder` = `"Search settings..."` (new key, 6 locales).
- A `<Search>` icon from `lucide-react` on the left side of the input.
- `useState('')` in the page component; `useMemo` derives the visible tab list.
- Match rule: case-insensitive substring on the displayed label. Empty query → all tabs in the new order. No results → render a single muted `<p>` with `settings.shell.no_results`.
- Search does **not** persist across navigation; opening Settings is a fresh state.

### Tab order

The tab definitions currently live inline in `app/settings/page.tsx` (line 19). Change the array order — no structural change. `tab.value` values stay identical so the i18n key path (`tabs.${tab.value}`) keeps working.

### Layout

The sidebar already wraps tabs in a vertical flex column with a header. The new search input slots in immediately after the header, before the tab list. No width change; the existing `<Button>` style for tabs is preserved.

## Files

- `frontend/src/app/settings/page.tsx` — add search input, reorder `tabs` array, derive visible tabs.
- `frontend/src/components/ui/input.tsx` — no change (already exists).
- `frontend/locales/{en-US,en-GB,zh-CN,zh-TW,ja-JP,ko-KR}/settings.json` — add `settings.shell.search_placeholder` and `settings.shell.no_results`.

Net diff: ~30 lines added to `page.tsx`, 2 new i18n keys × 6 locales. No other files touched.

## Behavior verification

Three scenarios, each <2 minutes of manual smoke:

1. Cold open → all 7 tabs visible, search box empty, default tab (current behavior) selected.
2. Type "rec" → only Recording tab visible. Clear → all 7 back. Type "xy" → empty-state message visible. Clear → all back.
3. Tab order with no query: Recording at top, About at bottom.

## Risk

Low. The change is additive in one component. Search input is uncontrolled-feeling (controlled state, no debouncing, no async). No backend, no persistence.

## Out of scope reminder

`ModelSettingsModal` (1387 lines, reached from two tabs) is the next-largest IA win and gets its own spec.

## Test plan (matches repo convention)

- [x] Manual smoke: 3 scenarios above
- [x] Local i18n JSON validation passed for all 6 locales
