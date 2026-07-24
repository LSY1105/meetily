# PR-46c — LLM Provider Picker Phase 3 (Ollama endpoint auto-fill, last open item)

## Problem

PR-46a exposed `default_base_url` per provider preset. PR-46b wired the field
into the UI for `custom-openai` only (`if (newPreset?.default_base_url &&
!customOpenAIEndpoint) setCustomOpenAIEndpoint(...)`). **Ollama's
`default_base_url = http://localhost:11434` is still unused** — when the user
switches the select from anything else to Ollama, the endpoint `<Input>`
remains empty and the user has to type the URL or guess it. The placeholder
shows the right value but the input is actually blank.

This was deliberately deferred from PR-46b (see PR-46b's "Out of scope" →
"Auto-fill for Ollama endpoint (separate state variable; one-line follow-up)").
This PR closes that one-line gap.

## Goal

When the user switches the provider select to `ollama` and the current
`ollamaEndpoint` state is empty, set it to the preset's
`default_base_url` (`http://localhost:11434`).

## Non-goals

- No backend changes.
- No new i18n keys.
- No change to Ollama endpoint validation, fetch logic, model list, or the
  `ollamaEndpointChanged` derivation.
- No change to the custom-openai auto-fill added in PR-46b.
- No change to the input `placeholder` text (it already reads
  `http://localhost:11434`).

## Frontend

### Edit: `frontend/src/components/ModelSettingsModal.tsx`

Extend the provider-select `onValueChange` handler. Insert one block directly
after the existing PR-46b auto-fill block (current lines ~916-920):

```tsx
// PR-46c: auto-fill Ollama endpoint from preset default. Mirrors the
// PR-46b custom-openai auto-fill above. Only fires when the user has
// not already typed a value.
if (provider === 'ollama' && !ollamaEndpoint) {
  const ollamaPreset = providerPresets.find(p => p.id === 'ollama');
  if (ollamaPreset?.default_base_url) {
    setOllamaEndpoint(ollamaPreset.default_base_url);
  }
}
```

The lookup uses `providerPresets.find(...)` rather than the literal string so
that a future edit to `llm_provider.rs`'s `PROVIDER_PRESETS` table
automatically follows through. The condition `provider === 'ollama'` guards
against accidental fills when switching between other providers.

Net diff: 5 inserted lines, 0 removed. No imports added (both `ollamaEndpoint`
state and `providerPresets` are already in scope).

## Testing

- Backend: no changes, no new tests.
- Frontend: no new tests. The block is a 5-line `if`; the behaviour is a
  mechanical read of `providerPresets`.
- Manual smoke (one scenario):
  1. Open Settings. Switch provider: Ollama → OpenAI → Ollama. Confirm the
     Ollama endpoint input now reads `http://localhost:11434` after the
     second Ollama selection (was empty before this PR).
  2. Switch to Ollama. Manually clear the endpoint back to empty. Switch to
     OpenAI and back to Ollama. Confirm the auto-fill **does not** overwrite
     what the user just had (it only fills when the current value is empty —
     same shape as PR-46b's custom-openai auto-fill).
  3. Switch to Ollama with an existing non-empty endpoint (e.g.
     `http://10.0.0.5:11434`). Switch away and back. Confirm the existing
     value is preserved.
- CI: existing `i18n-check` (no key changes; should pass identically).

## Risk

- **Auto-fill surprise.** Same trade-off PR-46b accepted: a user who has
  intentionally cleared the endpoint will see it refill when they switch
  away and back. Acceptable because (a) the placeholder already shows the
  same value, so the user has visual continuity, and (b) the user can
  re-clear in one keystroke.
- **Ollama validation side-effects.** The `setOllamaEndpoint` call may
  retrigger the existing `useEffect` that clears `models` + `error` when
  the endpoint differs from `lastFetchedEndpoint`. First switch into Ollama
  with no prior endpoint will fire this effect once (clears the model list).
  Already happens today on manual typing; not a regression.

## Out of scope (explicit YAGNI)

- Auto-fill for `builtin-ai` (no `default_base_url`; not needed)
- Storing the auto-filled URL to backend on save (already covered by the
  existing `api_get_model_config` / save flow)
- Cross-provider default-URL templating (no other preset has one)
