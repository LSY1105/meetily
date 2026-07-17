// Wave 23 / PR-42-iii: per-segment LLM auto postprocess.
//
// Every ASR segment that is long enough to be worth rewriting gets an
// async pass through the user-configured LLM provider via
// summary::llm_client::generate_summary. The result is emitted to the
// frontend as `transcript-postprocessed`; failures go out as
// `transcript-postprocess-failed`. Segments below the length threshold
// skip the LLM call entirely so the stream is not flooded with
// trivial rewrites of "uh huh" / "yes" / "ok".

use crate::audio::post_processor;
use crate::database::repositories::setting::SettingsRepository;
use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::CustomOpenAIConfig;
use once_cell::sync::OnceLock;
use serde::Serialize;
use sqlx::SqlitePool;
use std::str::FromStr;
use tauri::{AppHandle, Emitter, Manager};

/// Minimum CJK characters before a segment is worth LLM rewriting.
/// Shorter fragments tend to lose information when rewritten.
pub const MIN_CJK_CHARS: usize = 8;

/// Minimum ASCII alphanumeric characters before rewriting kicks in.
/// Roughly one short English sentence.
pub const MIN_ASCII_CHARS: usize = 20;

const SYSTEM_PROMPT: &str = "You are a transcript corrector. Read the ASR chunk and: \
- Fix obvious ASR errors (homophones, punctuation, spacing). \
- Preserve all proper nouns / project names / jargon (see <glossary>). \
- Do not translate. Do not summarize. Keep the language of the source. \
Output ONLY the corrected text, no commentary, no quotes.";

#[derive(Serialize, Clone)]
struct PostprocessedSegment {
    segment_id: String,
    text: String,
    latency_ms: u64,
}

#[derive(Serialize, Clone)]
struct PostprocessFailedSegment {
    segment_id: String,
    error: String,
}

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static APP: OnceLock<AppHandle> = OnceLock::new();

fn http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to build reqwest client for llm_postprocess")
    })
}

/// Wire the Tauri AppHandle once at startup. RecordingSaver has no
/// direct AppHandle parameter on add_transcript_segment, so the postprocess
/// path relies on this global.
pub fn init_app(app: AppHandle) {
    let _ = APP.set(app);
}

fn is_cjk(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp) || (0x3400..=0x4DBF).contains(&cp)
}

pub fn should_skip_for_length(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    let cjk_count = trimmed.chars().filter(|c| is_cjk(*c)).count();
    if cjk_count > 0 {
        cjk_count < MIN_CJK_CHARS
    } else {
        let ascii_count = trimmed
            .chars()
            .filter(|c| c.is_ascii_alphabetic() || c.is_ascii_digit())
            .count();
        ascii_count < MIN_ASCII_CHARS
    }
}

fn build_glossary_block() -> Option<String> {
    let terms = post_processor::read_hotwords_for_llm();
    if terms.is_empty() {
        return None;
    }
    Some(format!("<glossary>\n{}\n</glossary>", terms.join("\n")))
}

fn build_user_prompt(text: &str) -> String {
    let mut prompt = String::from(SYSTEM_PROMPT);
    prompt.push_str("\n\n");
    if let Some(block) = build_glossary_block() {
        prompt.push_str(&block);
        prompt.push_str("\n\n");
    }
    prompt.push_str(&format!("<source>\n{}\n</source>", text));
    prompt
}

async fn load_provider_inputs(
    pool: &SqlitePool,
) -> Result<
    (
        LLMProvider,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<u32>,
        Option<f32>,
        Option<f32>,
    ),
    String,
> {
    let setting = SettingsRepository::get_model_config(pool)
        .await
        .map_err(|e| format!("Failed to load provider config: {}", e))?
        .ok_or_else(|| "Provider not configured".to_string())?;
    let provider_str = setting.provider.clone();
    let provider = LLMProvider::from_str(&provider_str)
        .map_err(|_| format!("Unsupported provider: {}", provider_str))?;

    if provider_str == "custom-openai" {
        let cfg: CustomOpenAIConfig = setting
            .get_custom_openai_config()
            .ok_or_else(|| "Custom OpenAI config missing".to_string())?;
        Ok((
            provider,
            cfg.model,
            cfg.api_key.unwrap_or_default(),
            None,
            Some(cfg.endpoint),
            cfg.max_tokens.map(|v| v as u32),
            cfg.temperature,
            cfg.top_p,
        ))
    } else {
        let key = SettingsRepository::get_api_key(pool, &provider_str)
            .await
            .map_err(|e| format!("Failed to load api key: {}", e))?
            .unwrap_or_default();
        Ok((
            provider,
            setting.model,
            key,
            setting.ollama_endpoint,
            None,
            None,
            None,
            None,
        ))
    }
}

pub async fn correct_segment(pool: &SqlitePool, text: &str) -> Result<String, String> {
    let (provider, model_name, api_key, ollama_endpoint, custom_openai_endpoint, max_tokens, temperature, top_p) =
        load_provider_inputs(pool).await?;
    let user_prompt = build_user_prompt(text);
    generate_summary(
        http_client(),
        &provider,
        &model_name,
        &api_key,
        "You are a transcript corrector. Fix obvious ASR errors. Preserve proper nouns. Keep source language. Output only the corrected text.",
        &user_prompt,
        ollama_endpoint.as_deref(),
        custom_openai_endpoint.as_deref(),
        max_tokens,
        temperature,
        top_p,
        None,
        None,
    )
    .await
}

pub fn spawn_segment_postprocess(segment_id: String, text: String) {
    if should_skip_for_length(&text) {
        return;
    }
    let app = match APP.get() {
        Some(a) => a.clone(),
        None => {
            log::warn!(
                "AppHandle not initialised; skipping postprocess for segment {}",
                segment_id
            );
            return;
        }
    };
    tauri::async_runtime::spawn(async move {
        let pool = match app.try_state::<crate::state::AppState>() {
            Some(state) => state.db_manager.pool().clone(),
            None => {
                log::warn!(
                    "AppState missing; skipping postprocess for segment {}",
                    segment_id
                );
                return;
            }
        };
        let start = std::time::Instant::now();
        match correct_segment(&pool, &text).await {
            Ok(corrected) => {
                let payload = PostprocessedSegment {
                    segment_id: segment_id.clone(),
                    text: corrected,
                    latency_ms: start.elapsed().as_millis() as u64,
                };
                if let Err(e) = app.emit("transcript-postprocessed", &payload) {
                    log::warn!("Failed to emit transcript-postprocessed: {}", e);
                }
            }
            Err(e) => {
                let payload = PostprocessFailedSegment {
                    segment_id: segment_id.clone(),
                    error: e,
                };
                if let Err(emit_err) = app.emit("transcript-postprocess-failed", &payload) {
                    log::warn!("Failed to emit transcript-postprocess-failed: {}", emit_err);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_cjk_skipped() {
        assert!(should_skip_for_length("你好"));
        assert!(should_skip_for_length("今天"));
        assert!(!should_skip_for_length("你好世界这是一个完整的测试句子"));
    }

    #[test]
    fn short_ascii_skipped() {
        assert!(should_skip_for_length("Hello"));
        assert!(should_skip_for_length("OK"));
        assert!(!should_skip_for_length(
            "This is a long enough English sentence to trigger postprocess."
        ));
    }

    #[test]
    fn empty_skipped() {
        assert!(should_skip_for_length(""));
        assert!(should_skip_for_length("   "));
    }

    #[test]
    fn cjk_dominates_threshold_choice() {
        assert!(should_skip_for_length("OpenAI 发布 GPT"));
        assert!(!should_skip_for_length(
            "OpenAI 刚刚发布了全新的 GPT-5 模型,性能显著提升"
        ));
    }

    #[test]
    fn glossary_block_empty_when_no_hotwords() {
        post_processor::set_hotwords_for_llm(vec![]);
        assert!(build_glossary_block().is_none());
    }

    #[test]
    fn glossary_block_renders_terms() {
        post_processor::set_hotwords_for_llm(vec!["AGI".to_string(), "Meetily".to_string()]);
        let block = build_glossary_block().unwrap();
        assert!(block.contains("AGI"));
        assert!(block.contains("Meetily"));
        post_processor::set_hotwords_for_llm(vec![]);
    }
}
