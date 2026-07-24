# PR-46a — LLM Provider Picker (single source of truth + auto-test on save)

## Problem

`ModelSettingsModal.tsx` already has a working LLM provider picker (provider
select, model dropdown, API key, custom-openai endpoint, ollama endpoint,
model catalogue fetcher, fallback lists). But three concrete pains remain:

1. **Two copies of the provider list.** Frontend hard-codes the union
   `'ollama' | 'groq' | 'claude' | 'openai' | 'openrouter' | 'builtin-ai' |
   'custom-openai'` and a label table; backend defines `LLMProvider` with
   the same seven variants in `summary/llm_client.rs`. New providers
   (e.g. when a user wants Kimi via CustomOpenAI in production) require
   edits in three places.

2. **No test on save.** `test_llm_connection` (PR-45a) exists for the
   diagnostics panel, but `ModelSettingsModal`'s "Save" button just stores
   config and toasts. Users who fat-finger an API key or paste a wrong
   base URL only find out at the next transcript postprocess.

3. **Provider labels are i18n strings baked into TSX.** The select shows
   `Ollama (local)`, `Claude (Anthropic)`, etc. hard-coded English;
   non-English locales either fall through or get a half-translated list.

## Goal

- Single source of truth for the provider list: one Rust `const`
  `PROVIDER_PRESETS` + one TS hook reading it
- Provider select dropdown uses that list (no TS union duplication)
- `Save` in `ModelSettingsModal` calls `test_llm_connection` after a
  successful save and surfaces pass/fail inline (no toast spam)
- Labels come from the preset `display_name` (English for now); i18n
  keys only for action / status copy

## Non-goals

- Replacing the existing model catalogue fetcher / fallback list — leave
  as is
- Adding new provider types (DeepSeek / Moonshot stay on CustomOpenAI)
- Multi-provider failover routing
- Migrating the existing on-disk settings.json — same path

## Backend

### New file: `frontend/src-tauri/src/llm_provider.rs`

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProviderPreset {
    pub id: &'static str,            // "openai", "ollama", ...
    pub display_name: &'static str,  // "OpenAI", "Ollama (local)"
    pub requires_api_key: bool,
    pub default_base_url: Option<&'static str>,
}

pub const PROVIDER_PRESETS: &[ProviderPreset] = &[
    ProviderPreset { id: "openai",        display_name: "OpenAI",                       requires_api_key: true,  default_base_url: None },
    ProviderPreset { id: "claude",        display_name: "Claude (Anthropic)",           requires_api_key: true,  default_base_url: None },
    ProviderPreset { id: "groq",          display_name: "Groq",                         requires_api_key: true,  default_base_url: None },
    ProviderPreset { id: "ollama",        display_name: "Ollama (local)",               requires_api_key: false, default_base_url: Some("http://localhost:11434") },
    ProviderPreset { id: "openrouter",    display_name: "OpenRouter",                   requires_api_key: true,  default_base_url: None },
    ProviderPreset { id: "builtin-ai",    display_name: "Built-in AI (local llama)",    requires_api_key: false, default_base_url: None },
    ProviderPreset { id: "custom-openai", display_name: "Custom OpenAI-compatible",     requires_api_key: true,  default_base_url: Some("") },
];

#[tauri::command]
pub fn list_llm_providers() -> Vec<ProviderPreset> {
    PROVIDER_PRESETS.to_vec()
}
```

One file, ~40 lines, no new deps. Mirrors the seven variants of
`summary::llm_client::LLMProvider` (we do **not** delete that enum —
`LLMProvider::from_str` is the runtime validation surface).

### `lib.rs` — register the new command (+1 line, next to the
existing `api_list_transcripts` etc.):

```rust
api::llm_provider::list_llm_providers,
```

## Frontend

### New file: `frontend/src/hooks/useLLMProviders.ts`

```ts
import { invoke } from '@tauri-apps/api/core';
import { useQuery } from '@tanstack/react-query';

export interface ProviderPreset {
  id: 'openai' | 'claude' | 'groq' | 'ollama' | 'openrouter' | 'builtin-ai' | 'custom-openai';
  display_name: string;
  requires_api_key: boolean;
  default_base_url: string | null;
}

export function useLLMProviders() {
  return useQuery({
    queryKey: ['llm-providers'],
    queryFn: () => invoke<ProviderPreset[]>('list_llm_providers'),
    staleTime: Infinity,
  });
}
```

Single responsibility: one fetch, cached forever. `useLLMDiagnostics`
already pulls `@tanstack/react-query` into the dep graph, so this
inherits the same provider — no new dep.

### Edit: `frontend/src/components/ModelSettingsModal.tsx`

Three surgical changes:

1. **Provider select source** — replace the current select items
   (lines ~890-920) with a map over `useLLMProviders().data ?? []`.
   Fallback to the existing hard-coded list when the hook hasn't
   resolved yet (zero regression).

2. **`requiresApiKey` derivation** — drive from the preset list
   (`preset.requires_api_key`) instead of the hard-coded four-way OR
   at lines 236-240. The result is identical for the existing 7
   providers.

3. **Save flow** — extend `handleSave` (lines ~1400 region) so that
   after `api_save_model_config` succeeds, it calls `invoke(
   'test_llm_connection', {})` and surfaces pass / fail inline
   beneath the Save button via two new local states
   `lastTestResult: 'ok' | 'fail' | null` + `lastTestMessage`. No
   toast (PR-45b's diagnostics panel already covers toasts).

The select's `<SelectValue placeholder>` stays the same. No layout
shift. No model catalogue fetcher change. No fallback list change.

### i18n — three new keys × 6 locales

```
settings.transcript.llm.test_after_save    "Connection tested on save"
settings.transcript.llm.test_ok            "Connection works"
settings.transcript.llm.test_fail          "Connection failed: {message}"
```

Reuse the existing `settings.transcript.llm.*` namespace established by
PR-45b; no new namespace.

## Testing

- Backend: 3 #[test] functions in `llm_provider.rs`:
  - preset IDs are unique (no accidental shadowing after a future copy-paste)
  - `list_llm_providers` returns 7 rows (matches `LLMProvider` variant count)
  - every preset has a non-empty `display_name`
- Frontend: no new test. The hook is a one-liner over `invoke`; the
  modal change is mechanical. Manual smoke:
  1. Open Settings → confirm provider select still shows the 7 entries
     in the same order
  2. Pick "Custom OpenAI" → confirm base URL field appears (unchanged)
  3. Pick "Ollama" → confirm API key field disappears (unchanged)
  4. Save with a deliberately bad OpenAI key → confirm inline
     "Connection failed: …" appears beneath the Save button
  5. Save with a working key → confirm inline "Connection works" appears
- CI: existing `i18n-check` (proves 6-locale parity)

## Risk

- The preset table is a typed const; adding a provider is a single Rust
  edit + nothing else. The TS `ProviderPreset['id']` union updates
  automatically when the backend adds a row.
- `requires_api_key` change in the modal is a refactor of existing
  logic, not a feature change — same boolean output for all 7
  current providers.
- `Save` already saves; the new test step is best-effort and does not
  roll back the save if the probe fails. The user sees both lines and
  can re-save after fixing credentials.

## Out of scope (explicit YAGNI)

- Per-provider model catalog refresh (already in PR scope elsewhere)
- Live progress for the test (instant already)
- Switching the provider label text to per-locale translations
  (English display_name is acceptable; PR-46d or later if requested)
- Streaming test progress / retry-with-backoff (PR-45a already covers
  this for the diagnostics path)
