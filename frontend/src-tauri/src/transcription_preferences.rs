use anyhow::{anyhow, Result};
use log::info;
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "transcription-preferences.json";
const STORE_KEY: &str = "hotwords";
pub const MAX_HOTWORD_CHARS: usize = 500;

fn normalize_hotwords(value: String) -> Result<Option<String>> {
    let trimmed = value.trim();
    if trimmed.chars().count() > MAX_HOTWORD_CHARS {
        return Err(anyhow!(
            "Hotwords must not exceed {} characters",
            MAX_HOTWORD_CHARS
        ));
    }
    Ok((!trimmed.is_empty()).then(|| trimmed.to_string()))
}

pub async fn load_transcription_hotwords<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Option<String>> {
    let store = app
        .store(STORE_FILE)
        .map_err(|error| anyhow!("Failed to access transcription preferences: {}", error))?;

    match store.get(STORE_KEY) {
        Some(value) => {
            let hotwords = value
                .as_str()
                .ok_or_else(|| anyhow!("Stored transcription hotwords are invalid"))?;
            normalize_hotwords(hotwords.to_string())
        }
        None => Ok(None),
    }
}

pub async fn save_transcription_hotwords<R: Runtime>(
    app: &AppHandle<R>,
    hotwords: String,
) -> Result<Option<String>> {
    let normalized = normalize_hotwords(hotwords)?;
    let store = app
        .store(STORE_FILE)
        .map_err(|error| anyhow!("Failed to access transcription preferences: {}", error))?;

    match &normalized {
        Some(value) => store.set(STORE_KEY, serde_json::json!(value)),
        None => {
            store.delete(STORE_KEY);
        }
    }
    store
        .save()
        .map_err(|error| anyhow!("Failed to persist transcription preferences: {}", error))?;

    info!(
        "Saved transcription hotwords: enabled={}, chars={}",
        normalized.is_some(),
        normalized
            .as_ref()
            .map(|value| value.chars().count())
            .unwrap_or(0)
    );
    // Wave 18 PR-55: refresh the global protected-terms cache so the next
    // postprocess call sees the latest `!`-prefixed entries. No-op when the
    // chain never runs or no terms are protected.
    let raw_for_protected = normalized.clone().unwrap_or_default();
    crate::audio::post_processor::set_protected_terms(extract_protected_terms(&raw_for_protected));
    crate::audio::post_processor::set_hotwords_for_llm(extract_all_hotwords(&raw_for_protected));
    // PR-A: keep the cache the hit-rate counter reads in sync.
    crate::hotword_stats::cache_raw(&raw_for_protected);
    Ok(normalized)
}

#[tauri::command]
pub async fn get_transcription_hotwords<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<String>, String> {
    let loaded = load_transcription_hotwords(&app)
        .await
        .map_err(|error| format!("Failed to load transcription hotwords: {}", error))?;
    // Wave 18 PR-55: refresh the protected-terms global cache so the first
    // postprocess call after app start picks up `!`-prefixed entries even
    // when the user never edits the hotword list.
    let raw_for_protected = loaded.clone().unwrap_or_default();
    crate::audio::post_processor::set_protected_terms(extract_protected_terms(&raw_for_protected));
    crate::audio::post_processor::set_hotwords_for_llm(extract_all_hotwords(&raw_for_protected));
    // PR-A: same sync as the save path.
    crate::hotword_stats::cache_raw(&raw_for_protected);
    Ok(loaded)
}

#[tauri::command]
pub async fn set_transcription_hotwords<R: Runtime>(
    app: AppHandle<R>,
    hotwords: String,
) -> Result<Option<String>, String> {
    save_transcription_hotwords(&app, hotwords)
        .await
        .map_err(|error| format!("Failed to save transcription hotwords: {}", error))
}


/// Wave 18 PR-55: extract protected terms from the raw hotwords string.
/// A term is "protected" when prefixed with `!` (with optional whitespace).
/// Returns deduplicated, byte-length-descending list (longest match wins
/// for the postprocess restoration step).
pub(crate) fn extract_protected_terms(raw: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    // One term per line (comma also accepted). Splitting on spaces broke
    // "! 张三" into ["!", "张三"], dropping both, and any multi-word term.
    let mut terms: Vec<String> = raw
        .split(|c: char| matches!(c, '\n' | '\r' | ','))
        .filter_map(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return None;
            }
            trimmed.strip_prefix('!').map(|rest| rest.trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert(s.clone()))
        .collect();
    // Length-descending so the longest match wins. Name as the tiebreak:
    // "张三" and "OpenAI" are both 6 bytes, so without it the order came
    // from input position and depended on how the splitter chunked the line.
    terms.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    terms
}

/// Wave 21 PR-F: extract all hotwords (both !-prefixed and bare) from the
/// raw hotwords string. Mirrors `extract_protected_terms` but keeps the
/// `!` prefix so the LLM glossary block can see both lists. Returns
/// deduplicated, byte-length-descending list (longest match wins).
pub(crate) fn extract_all_hotwords(raw: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut terms: Vec<String> = raw
        .split(|c: char| matches!(c, '\n' | '\r' | ','))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert(s.clone()))
        .collect();
    terms.sort_by(|a, b| b.len().cmp(&a.len()));
    terms
}

/// Wave 18 PR-55: Tauri command returning the protected-term list.
/// Frontend calls this once on app start (and after hotword edits) and
/// forwards the result to the postprocessor for restoration.
#[tauri::command]
pub async fn get_protected_terms<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<String>, String> {
    let raw = load_transcription_hotwords(&app)
        .await
        .map_err(|e| format!("Failed to load hotwords: {}", e))?
        .unwrap_or_default();
    Ok(extract_protected_terms(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_disables_hotwords() {
        assert_eq!(normalize_hotwords(" \n ".to_string()).unwrap(), None);
    }

    #[test]
    fn trims_hotwords_without_changing_lines() {
        assert_eq!(
            normalize_hotwords("  Meetily\n星河项目  ".to_string()).unwrap(),
            Some("Meetily\n星河项目".to_string())
        );
    }

    #[test]
    fn accepts_500_unicode_characters() {
        let value = "术".repeat(MAX_HOTWORD_CHARS);
        assert_eq!(normalize_hotwords(value.clone()).unwrap(), Some(value));
    }

    #[test]
    fn rejects_more_than_500_unicode_characters() {
        let error = normalize_hotwords("术".repeat(MAX_HOTWORD_CHARS + 1)).unwrap_err();
        assert!(error.to_string().contains("500"));
    }

    // ---- extract_protected_terms ----

    #[test]
    fn protected_terms_empty_when_no_bang_prefix() {
        assert!(extract_protected_terms("").is_empty());
        assert!(extract_protected_terms("Meetily\n张三").is_empty());
    }

    #[test]
    fn protected_terms_extracts_bang_prefixed_words() {
        let result = extract_protected_terms("Meetily\n!张三\n!字节跳动\n!OpenAI");
        assert_eq!(result, vec!["字节跳动", "OpenAI", "张三"]);
    }

    #[test]
    fn protected_terms_dedup() {
        let result = extract_protected_terms("!张三\n!张三\n!OpenAI\n!OpenAI");
        assert_eq!(result, vec!["OpenAI", "张三"]);
    }

    #[test]
    fn protected_terms_allow_whitespace_after_bang() {
        let result = extract_protected_terms("! 张三\n!\t字节跳动");
        assert_eq!(result, vec!["字节跳动", "张三"]);
    }



    #[test]
    fn extract_all_includes_bare_and_protected() {
        let terms = extract_all_hotwords("OpenAI\n!AGI\nfoo");
        assert_eq!(terms, vec!["OpenAI".to_string(), "AGI".to_string(), "foo".to_string()]);
    }

    #[test]
    fn extract_all_dedupes() {
        let terms = extract_all_hotwords("foo\nfoo\nbar");
        assert_eq!(terms, vec!["foo".to_string(), "bar".to_string()]);
    }

    #[test]
    fn extract_all_empty_input() {
        assert!(extract_all_hotwords("").is_empty());
        assert!(extract_all_hotwords("   \n  \t  ").is_empty());
    }
}
