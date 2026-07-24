'use client';

// PR-46a: hook over the backend `list_llm_providers` command.
// Returns the preset list once on mount. Mirrors the
// useLLMDiagnostics pattern (useState + useEffect + invoke) since
// the project does not depend on @tanstack/react-query.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export type ProviderId =
  | 'openai'
  | 'claude'
  | 'groq'
  | 'ollama'
  | 'openrouter'
  | 'builtin-ai'
  | 'custom-openai';

export interface ProviderPreset {
  id: ProviderId;
  display_name: string;
  requires_api_key: boolean;
  default_base_url: string | null;
}

const FALLBACK_PRESETS: ProviderPreset[] = [
  { id: 'openai',        display_name: 'OpenAI',                   requires_api_key: true,  default_base_url: null },
  { id: 'claude',        display_name: 'Claude (Anthropic)',       requires_api_key: true,  default_base_url: null },
  { id: 'groq',          display_name: 'Groq',                     requires_api_key: true,  default_base_url: null },
  { id: 'ollama',        display_name: 'Ollama (local)',           requires_api_key: false, default_base_url: 'http://localhost:11434' },
  { id: 'openrouter',    display_name: 'OpenRouter',               requires_api_key: true,  default_base_url: null },
  { id: 'builtin-ai',    display_name: 'Built-in AI (local llama)', requires_api_key: false, default_base_url: null },
  { id: 'custom-openai', display_name: 'Custom OpenAI-compatible', requires_api_key: true,  default_base_url: '' },
];

export interface UseLLMProvidersResult {
  data: ProviderPreset[];
  loading: boolean;
}

export function useLLMProviders(): UseLLMProvidersResult {
  const [data, setData] = useState<ProviderPreset[]>(FALLBACK_PRESETS);
  const [loading, setLoading] = useState<boolean>(true);

  useEffect(() => {
    let active = true;
    invoke<ProviderPreset[]>('list_llm_providers')
      .then((v) => {
        if (!active) return;
        setData(v ?? FALLBACK_PRESETS);
      })
      .catch((err) => {
        // Backend unreachable / Tauri not running — keep the fallback
        // list so the dropdown still renders. Matches the legacy
        // hard-coded behavior; zero regression for current users.
        console.warn('list_llm_providers failed, using fallback list:', err);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  return { data, loading };
}
