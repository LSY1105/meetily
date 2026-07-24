# PR-46b — LLM Provider Picker Phase 2 (consume preset fields + endpoint pre-check)

## Problem

PR-46a exposed two preset fields (`requires_api_key`, `default_base_url`) on the
backend and rewired the provider select to read from `PROVIDER_PRESETS`. The
modal still does **not consume** either field on screen:

1. **No API-key hint.** Switch from `openai` (key required) to `ollama` (no key):
   the API key input stays visible and the user has no signal that it's now
   irrelevant. Same story in reverse for `builtin-ai`. The `requires_api_key`
   boolean only drives `requiresApiKey` (an internal gate); it never reaches the
   user as a hint.

2. **No default-base-url surfacing.** Ollama's `default_base_url` is
   `http://localhost:11434`. New users have to guess or google it; the input
   placeholder is empty. `custom-openai` ships `default_base_url: Some("")`
   which means "user fills their own" — fine, but we should auto-fill when the
   field is empty so the user isn't stuck on an empty form.

3. **No endpoint pre-save probe.** `custom-openai` endpoint can be any string;
   the user types, hits Save, and only finds out it was wrong when LLM
   postprocess fails minutes later. PR-45a's diagnostics panel catches the
   failure but does not prevent the bad save.

## Goal

- Show a one-line muted hint under the API key input that follows the
  provider's `requires_api_key` flag ("Requires API key" /
  "No API key needed").
- When switching provider, if the base-URL input for that provider is empty,
  prefill it with `default_base_url` (only useful for `ollama` today; no-op
  for everyone else).
- On Save, if provider is `custom-openai`, fetch the endpoint with a short
  HEAD/OPTIONS; only persist if the response is 2xx (or a known 405 for
  CORS-restricted servers). Otherwise show an inline error and keep the
  modal open.

## Non-goals

- No Rust changes. `llm_provider.rs` and the `list_llm_providers` command are
  frozen — the new behaviour is purely a frontend read of the existing
  payload.
- No new provider entries in `PROVIDER_PRESETS`.
- No i18n of the English `display_name` labels (still English; deferred).
- No change to the model catalogue fetcher or fallback list.
- No change to the existing `test_llm_connection` post-save probe (PR-46a
  inline result stays).

## Frontend

### Edit: `frontend/src/components/ModelSettingsModal.tsx`

Three surgical changes, all inside the existing modal body.

**Change 1 — API key hint (one new `<p>` under the existing key input)**

```tsx
<p className="text-xs text-muted-foreground mt-1">
  {activePreset?.requires_api_key
    ? tSettings('transcript.llm.api_key_required')
    : tSettings('transcript.llm.api_key_not_required')}
</p>
```

`<p>` lives directly under the existing `<Input id="provider-api-key" .../>`
element, same conditional visibility as the input itself.

**Change 2 — Auto-fill on provider change (extend existing `onValueChange`)**

Inside the `<Select onValueChange={...}>` handler, after the existing
`setModelConfig` call, add:

```tsx
const newPreset = providerPresets.find(p => p.id === provider);
if (newPreset?.default_base_url && !customOpenAIEndpoint) {
  setCustomOpenAIEndpoint(newPreset.default_base_url);
}
```

Scope: only auto-fills `custom-openai` endpoint today (the only base-URL
input currently bound to provider switching). Ollama uses a separate
`ollamaEndpoint` state — leave it alone for this PR; a one-line follow-up
will cover it when we wire ollama into the preset's default URL.

**Change 3 — Endpoint pre-save probe (`handleSave` head)**

Insert a probe block before the existing `onSave(updatedConfig)` call. Only
runs when `provider === 'custom-openai'`:

```tsx
if (modelConfig.provider === 'custom-openai' && customOpenAIEndpoint) {
  setIsCheckingEndpoint(true);
  setEndpointCheckError(null);
  try {
    const ok = await checkEndpointReachable(customOpenAIEndpoint);
    if (!ok) {
      setEndpointCheckError(tSettings('transcript.llm.endpoint_unreachable', {
        message: customOpenAIEndpoint,
      }));
      return;   // keep modal open; do not call onSave
    }
  } finally {
    setIsCheckingEndpoint(false);
  }
}
```

`checkEndpointReachable(url)` is a small helper in the same file:

```ts
async function checkEndpointReachable(url: string): Promise<boolean> {
  // Try HEAD first; fall back to OPTIONS (CORS-restricted servers often
  // 405 OPTIONS but allow GET). 5s timeout. Treat 2xx and 405 as reachable.
  for (const method of ['HEAD', 'OPTIONS'] as const) {
    try {
      const res = await fetch(url, { method, mode: 'cors' });
      if (res.ok) return true;
      if (res.status === 405) return true;   // method not allowed but server is up
    } catch { /* try next method */ }
  }
  return false;
}
```

**State additions** (near the existing PR-46a `lastTestResult` state block):

```ts
const [isCheckingEndpoint, setIsCheckingEndpoint] = useState<boolean>(false);
const [endpointCheckError, setEndpointCheckError] = useState<string | null>(null);
```

**Save button disable**: extend the existing `isDoneDisabled` derivation to
include `isCheckingEndpoint || !!endpointCheckError`. While the probe is
running, label the button "Checking..." instead of "Save".

**Inline error render**: under the `custom-openai` endpoint input, render
`endpointCheckError` in red (only when set).

## i18n — 4 new keys × 6 locales

```
settings.transcript.llm.api_key_required        "Requires API key"
settings.transcript.llm.api_key_not_required    "No API key needed"
settings.transcript.llm.endpoint_checking       "Checking endpoint..."
settings.transcript.llm.endpoint_unreachable    "Endpoint unreachable: {message}"
```

| locale | api_key_required | api_key_not_required | endpoint_checking | endpoint_unreachable |
|---|---|---|---|---|
| en-US | Requires API key | No API key needed | Checking endpoint... | Endpoint unreachable: {message} |
| en-GB | Requires API key | No API key needed | Checking endpoint... | Endpoint unreachable: {message} |
| zh-CN | 需要 API 密钥 | 不需要 API 密钥 | 正在检查端点... | 端点不可达：{message} |
| zh-TW | 需要 API 金鑰 | 不需要 API 金鑰 | 正在檢查端點... | 端點無法連線：{message} |
| ja-JP | API キーが必要です | API キーは不要です | エンドポイントを確認中... | エンドポイントに到達できません：{message} |
| ko-KR | API 키 필요 | API 키 불필요 | 엔드포인트 확인 중... | 엔드포인트에 연결할 수 없음: {message} |

All keys go under the existing `transcript.llm.diagnostics` block (or a new
`transcript.llm.*` sibling — pick whichever stays JSON-valid without moving
the existing trailing key). If sibling placement is needed, add `endpoint_*`
right after the PR-46a `test_*` keys.

## Testing

- Backend: no changes, no new tests.
- Frontend:
  - `useLLMProviders` already exposes `requires_api_key` and `default_base_url`
    in its return shape (since PR-46a). No hook change.
  - `checkEndpointReachable` helper gets one `__manual__` smoke call behind
    a `// ponytail:` note — happy-path only (a reachable mock URL returns
    true; an unreachable URL returns false within 5s). Defer full unit-test
    coverage to a later PR; this helper is a 10-line function with one
    branch worth a manual check.
- Manual smoke (the only verification path for the modal):
  1. Open Settings. Switch provider Ollama → OpenAI → Ollama. Confirm the
     hint line under the API key input reads "No API key needed" /
     "Requires API key" in lockstep with the input visibility.
  2. Switch to Ollama. Confirm endpoint input is empty (we did not wire
     ollama auto-fill this PR; placeholder-only is acceptable).
  3. Switch to Custom OpenAI. Type a known-good endpoint (e.g. a local
     llama.cpp server). Save. Confirm save succeeds.
  4. Switch to Custom OpenAI. Type `http://127.0.0.1:1/should-fail` (closed
     port). Save. Confirm inline "Endpoint unreachable" appears and the
     modal stays open.
  5. Re-run with the deliberate bad endpoint after fixing it. Save. Confirm
     success.
- CI: existing `i18n-check` proves 6-locale parity for the four new keys.

## Risk

- **Auto-fill surprise.** If a user has previously cleared the custom-openai
  endpoint on purpose, switching away and back will silently refill the
  default. Mitigated by only auto-filling when the field is currently empty;
  once they type something, switching providers never overwrites it.
- **Probe timeout.** 5s timeout per method, two methods worst-case = 10s on a
  fully dead endpoint. Acceptable for a modal that's already gated by the
  user hitting Save.
- **CORS.** `mode: 'cors'` will fail for endpoints without CORS headers.
  The 405 fallback covers "method not allowed but server is up" — most LLM
  endpoints respond to OPTIONS even when HEAD isn't allowed. If a user
  hits a CORS-locked endpoint, the probe fails and the inline error names
  the URL; they can still save by accepting the warning (we do **not**
  hard-block the save on CORS-only failures — we block on
  *unreachable*, defined as both methods throwing).

  Updated decision after risk review: **block only when both HEAD and
  OPTIONS throw** (network-level unreachable). 4xx/5xx responses are
  treated as reachable because they prove the server is up and the user
  likely has an auth/path issue worth surfacing through the existing
  post-save probe, not through Save-time validation.

## Out of scope (explicit YAGNI)

- Auto-fill for Ollama endpoint (separate state variable; one-line follow-up)
- Retry-with-backoff on the probe (one shot is enough; user can click Save again)
- Surfacing `default_base_url` for non-custom-openai providers (none have one
  besides ollama, deferred)
- Disabling the API key `<Input>` when `requires_api_key` is false — keep the
  input visible but mute it, so the user can still paste a key if they want
  to (some proxies expect it). Defer hard-disable to a later wave.
