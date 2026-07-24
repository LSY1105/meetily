// PR-46a: Single source of truth for the LLM provider list exposed to the
// frontend. Mirrors the seven variants of summary::llm_client::LLMProvider
// but stays a flat const so it survives any future enum refactor.
//
// Keep this table small and serialisable; the frontend uses it for the
// provider select dropdown and the `requires_api_key` derivation.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProviderPreset {
    pub id: &'static str,
    pub display_name: &'static str,
    pub requires_api_key: bool,
    pub default_base_url: Option<&'static str>,
}

pub const PROVIDER_PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "openai",
        display_name: "OpenAI",
        requires_api_key: true,
        default_base_url: None,
    },
    ProviderPreset {
        id: "claude",
        display_name: "Claude (Anthropic)",
        requires_api_key: true,
        default_base_url: None,
    },
    ProviderPreset {
        id: "groq",
        display_name: "Groq",
        requires_api_key: true,
        default_base_url: None,
    },
    ProviderPreset {
        id: "ollama",
        display_name: "Ollama (local)",
        requires_api_key: false,
        default_base_url: Some("http://localhost:11434"),
    },
    ProviderPreset {
        id: "openrouter",
        display_name: "OpenRouter",
        requires_api_key: true,
        default_base_url: None,
    },
    ProviderPreset {
        id: "builtin-ai",
        display_name: "Built-in AI (local llama)",
        requires_api_key: false,
        default_base_url: None,
    },
    ProviderPreset {
        id: "custom-openai",
        display_name: "Custom OpenAI-compatible",
        requires_api_key: true,
        default_base_url: Some(""),
    },
];

#[tauri::command]
pub fn list_llm_providers() -> Vec<ProviderPreset> {
    PROVIDER_PRESETS.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for p in PROVIDER_PRESETS {
            assert!(seen.insert(p.id), "duplicate preset id: {}", p.id);
        }
    }

    #[test]
    fn preset_count_matches_llm_provider_variants() {
        // Keep this in sync with summary::llm_client::LLMProvider. The current
        // count is 7. If a future PR adds an 8th, update both sides.
        assert_eq!(PROVIDER_PRESETS.len(), 7);
    }

    #[test]
    fn every_preset_has_a_display_name() {
        for p in PROVIDER_PRESETS {
            assert!(!p.display_name.trim().is_empty(), "empty display_name for {}", p.id);
        }
    }
}
