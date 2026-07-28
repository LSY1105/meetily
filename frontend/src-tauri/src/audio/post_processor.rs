use std::sync::Arc;
use tokio::sync::mpsc;
use anyhow::Result;
use log::{info, warn, error};
use once_cell::sync::Lazy;
use regex::Regex;

/// Post-processing request for transcript text
#[derive(Debug, Clone)]
pub struct PostProcessRequest {
    pub sequence_id: u32,
    pub raw_text: String,
    pub is_partial: bool,
    pub timestamp: String,
}

/// Post-processing response with refined text
#[derive(Debug, Clone)]
pub struct PostProcessResponse {
    pub sequence_id: u32,
    pub processed_text: String,
    pub confidence: f32,
    pub is_partial: bool,
    pub timestamp: String,
    pub processing_time_ms: u64,
}

/// Background post-processing pipeline for transcript text
pub struct PostProcessor {
    request_sender: mpsc::UnboundedSender<PostProcessRequest>,
    response_receiver: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<PostProcessResponse>>>,
    _handle: tokio::task::JoinHandle<()>,
}

// Wave 18 PR-51: pre-compiled contraction regexes with word-boundary anchors.
// Substring `.replace` would mangle real words like `vacant` -> `vacan't`,
// `scant` -> `scan't`, `cantilever` -> `can'tilever`. Each rule is wrapped in
// `\b...\b` and the input literal is regex-escaped, so the rules never fire
// inside a larger word.
static CONTRACTION_RULES: Lazy<Vec<(Regex, &str)>> = Lazy::new(|| {
    let raw: &[(&str, &str)] = &[
        ("cant", "can't"),
        ("wont", "won't"),
        ("dont", "don't"),
        ("doesnt", "doesn't"),
        ("didnt", "didn't"),
        ("wouldnt", "wouldn't"),
        ("couldnt", "couldn't"),
        ("shouldnt", "shouldn't"),
        ("isnt", "isn't"),
        ("arent", "aren't"),
        ("wasnt", "wasn't"),
        ("werent", "weren't"),
        ("hasnt", "hasn't"),
        ("havent", "haven't"),
        ("hadnt", "hadn't"),
    ];
    raw.iter()
        .map(|(pat, repl)| {
            let re = Regex::new(&format!("\\b{}\\b", regex::escape(pat)))
                .expect("contraction regex must compile");
            (re, *repl)
        })
        .collect()
});

impl PostProcessor {
    /// Create a new post-processor with background processing
    pub fn new() -> Self {
        let (request_sender, mut request_receiver) = mpsc::unbounded_channel();
        let (response_sender, response_receiver) = mpsc::unbounded_channel();

        let handle = tokio::spawn(async move {
            info!("Background post-processor started");

            while let Some(request) = request_receiver.recv().await {
                let start_time = std::time::Instant::now();

                match Self::process_text(&request).await {
                    Ok(processed_text) => {
                        let processing_time = start_time.elapsed().as_millis() as u64;

                        let response = PostProcessResponse {
                            sequence_id: request.sequence_id,
                            processed_text,
                            confidence: if request.is_partial { 0.8 } else { 0.95 }, // Processed text has higher confidence
                            is_partial: request.is_partial,
                            timestamp: request.timestamp,
                            processing_time_ms: processing_time,
                        };

                        if let Err(e) = response_sender.send(response) {
                            error!("Failed to send post-processing response: {}", e);
                            break;
                        }

                        if processing_time > 100 {
                            warn!("Slow post-processing for sequence {}: {}ms", request.sequence_id, processing_time);
                        }
                    }
                    Err(e) => {
                        warn!("Post-processing failed for sequence {}: {}", request.sequence_id, e);
                        // Send original text as fallback
                        let response = PostProcessResponse {
                            sequence_id: request.sequence_id,
                            processed_text: request.raw_text.clone(),
                            confidence: 0.5, // Lower confidence for failed processing
                            is_partial: request.is_partial,
                            timestamp: request.timestamp,
                            processing_time_ms: start_time.elapsed().as_millis() as u64,
                        };

                        if let Err(e) = response_sender.send(response) {
                            error!("Failed to send fallback response: {}", e);
                            break;
                        }
                    }
                }
            }

            info!("Background post-processor stopped");
        });

        Self {
            request_sender,
            response_receiver: Arc::new(tokio::sync::Mutex::new(response_receiver)),
            _handle: handle,
        }
    }

    /// Submit text for background post-processing
    pub fn process_async(&self, request: PostProcessRequest) -> Result<()> {
        self.request_sender
            .send(request)
            .map_err(|e| anyhow::anyhow!("Failed to submit post-processing request: {}", e))
    }

    /// Try to receive processed results (non-blocking)
    pub async fn try_recv(&self) -> Option<PostProcessResponse> {
        let mut receiver = self.response_receiver.lock().await;
        receiver.try_recv().ok()
    }

    /// Wait for the next processed result
    pub async fn recv(&self) -> Option<PostProcessResponse> {
        let mut receiver = self.response_receiver.lock().await;
        receiver.recv().await
    }

    /// Process text synchronously (for testing or direct use)
    async fn process_text(request: &PostProcessRequest) -> Result<String> {
        let text = &request.raw_text;

        // Wave 18 PR-55: pre-pass protected terms to sentinel so the postprocess
        // chain (which can rewrite names, casing, digits, etc.) never touches them.
        let (guarded, mapping) = Self::protect_terms(text);

        // Skip processing for empty or very short text
        if guarded.trim().len() < 3 {
            return Ok(Self::restore_protected_terms(&guarded, &mapping));
        }

        // Step 1: Clean repetitive text (most expensive operation)
        let deduplicated = Self::clean_repetitive_text(&guarded);

        // Step 2: Remove common transcription artifacts
        let cleaned = Self::remove_artifacts(&deduplicated);

        // Step 3: Normalize whitespace and punctuation
        let normalized = Self::normalize_text(&cleaned);

        // Step 4: Apply contextual improvements (if not partial)
        let final_text = if !request.is_partial {
            Self::apply_contextual_improvements(&normalized)
        } else {
            normalized
        };

        // Wave 18 PR-55: post-pass restore sentinels back to original terms.
        Ok(Self::restore_protected_terms(&final_text, &mapping))
    }

    /// Clean repetitive text patterns (same as whisper_engine but moved to background)
    fn clean_repetitive_text(text: &str) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() < 4 {
            return text.to_string();
        }

        let mut result = Vec::new();
        let mut i = 0;

        while i < words.len() {
            let current_word = words[i];

            // Check for immediate repetitions (same word repeated)
            if i + 1 < words.len() && words[i + 1] == current_word {
                result.push(current_word);
                // Skip repeated instances
                while i + 1 < words.len() && words[i + 1] == current_word {
                    i += 1;
                }
            }
            // Check for phrase repetitions
            else if i + 3 < words.len() {
                let phrase = &words[i..i+2];
                let next_phrase = &words[i+2..i+4];

                if phrase == next_phrase {
                    result.extend_from_slice(phrase);
                    i += 4; // Skip both phrases

                    // Skip additional repetitions of the same phrase
                    while i + 1 < words.len() && i + 1 < words.len() - 1 {
                        let check_phrase = &words[i..std::cmp::min(i+2, words.len())];
                        if check_phrase == phrase && check_phrase.len() == 2 {
                            i += 2;
                        } else {
                            break;
                        }
                    }
                    continue;
                }
                result.push(current_word);
            } else {
                result.push(current_word);
            }
            i += 1;
        }

        result.join(" ")
    }

    /// Remove common transcription artifacts using simple string matching
    fn remove_artifacts(text: &str) -> String {
        let mut words: Vec<String> = text.split_whitespace()
            .map(|w| w.to_string())
            .collect();

        // Remove common filler words and sounds
        let fillers = [
            "uh", "um", "er", "ah", "oh", "hm", "hmm",
            "uhh", "umm", "err", "ahh", "ohh",
        ];

        words.retain(|word| {
            let clean_word_temp = word.to_lowercase();
            let clean_word = clean_word_temp.trim_matches(|c: char| !c.is_alphabetic());
            !fillers.contains(&clean_word) || clean_word.len() > 3
        });

        words.join(" ")
    }

    /// Normalize text formatting
    fn normalize_text(text: &str) -> String {
        let mut normalized = text.trim().to_string();

        // Fix spacing around punctuation
        normalized = normalized.replace(" .", ".");
        normalized = normalized.replace(" ,", ",");
        normalized = normalized.replace(" ?", "?");
        normalized = normalized.replace(" !", "!");

        // Ensure single space after sentence endings
        normalized = normalized.replace(".  ", ". ");
        normalized = normalized.replace("?  ", "? ");
        normalized = normalized.replace("!  ", "! ");

        // Capitalize first letter of sentences
        if let Some(first_char) = normalized.chars().next() {
            if first_char.is_lowercase() {
                normalized = first_char.to_uppercase().collect::<String>() + &normalized[1..];
            }
        }

        normalized
    }

    /// Apply contextual improvements for final transcripts
    fn apply_contextual_improvements(text: &str) -> String {
        // Wave 18 PR-51: pre-compiled word-boundary regexes (see CONTRACTION_RULES).
        // The previous substring-based `.replace` mangled real words like
        // `vacant` -> `vacan't`. `\b...\b` + `regex::escape` guarantees we
        // only rewrite a token when it stands alone, and `match_case`
        // preserves the original capitalisation (`Cant` -> `Can't`).
        let mut improved = text.to_string();
        for (re, replacement) in CONTRACTION_RULES.iter() {
            let result = re.replace_all(&improved, |caps: &regex::Captures<'_>| {
                let matched = caps.get(0).map(|m| m.as_str()).unwrap_or("");
                Self::match_case(replacement, matched)
            });
            improved = result.into_owned();
        }
        improved
    }

    // Wave 18 PR-51: mirror the case style of `original` onto `template`.
    //   match_case("can't", "cant") == "can't"
    //   match_case("can't", "Cant") == "Can't"
    //   match_case("can't", "CANT") == "CAN'T"
    fn match_case(template: &str, original: &str) -> String {
        if !original.is_empty() && original.chars().all(|c| c.is_uppercase()) {
            return template.to_uppercase();
        }
        let mut chars = template.chars();
        if let Some(first) = chars.next() {
            if original.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                let mut out = first.to_uppercase().to_string();
                out.push_str(chars.as_str());
                return out;
            }
        }
        template.to_string()
    }

    // ---- Wave 18 PR-55: protected-terms restoration ----

    /// Replace protected terms in `text` with unique sentinels. Returns
    /// the rewritten text and a mapping table for the post-pass restoration.
    /// Empty mapping means no protected terms are configured (the chain runs
    /// unmodified). Greedy longest-match from left to right so overlapping
    /// terms never double-replace the same span.
    pub fn protect_terms(text: &str) -> (String, Vec<(String, String)>) {
        let terms = read_protected_terms();
        if terms.is_empty() {
            return (text.to_string(), Vec::new());
        }
        let mut mapping: Vec<(String, String)> = Vec::new();
        let mut out = String::with_capacity(text.len());
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            // Caller (`get_protected_terms`) is responsible for length-desc sort;
            // here we just iterate the list, which gives longest-first behaviour
            // for any input that is already pre-sorted.
            let mut matched = false;
            for term in &terms {
                let term_bytes = term.as_bytes();
                if term_bytes.is_empty() || i + term_bytes.len() > bytes.len() {
                    continue;
                }
                if &bytes[i..i + term_bytes.len()] == term_bytes {
                    let sentinel = make_sentinel(mapping.len());
                    mapping.push((sentinel.clone(), term.clone()));
                    out.push_str(&sentinel);
                    i += term_bytes.len();
                    matched = true;
                    break;
                }
            }
            if !matched {
                // Advance one UTF-8 char. `is_char_boundary` returns true at the
                // start of every char and at `bytes.len()`; `i+1..=bytes.len()`
                // guarantees progress even when `i` is the last byte.
                let ch_end = (i + 1..=bytes.len())
                    .find(|&j| text.is_char_boundary(j))
                    .unwrap_or(bytes.len());
                out.push_str(&text[i..ch_end]);
                i = ch_end;
            }
        }
        (out, mapping)
    }

    /// Replace sentinels from `mapping` back to their original protected terms.
    /// No-op when mapping is empty.
    pub fn restore_protected_terms(text: &str, mapping: &[(String, String)]) -> String {
        if mapping.is_empty() {
            return text.to_string();
        }
        let mut out = text.to_string();
        for (sentinel, original) in mapping {
            out = out.replace(sentinel, original);
        }
        out
    }

}

// ---- Wave 18 PR-55: module-level state for protected terms ----

/// Sentinel version + per-process random hash baked into every placeholder.
/// Generated once on first access, then reused for the lifetime of the
/// process so the same protected term in two sentences gets a different
/// placeholder (per-sentence uniqueness isn't needed because the
/// postprocess chain never duplicates terms, but a per-process prefix
/// guarantees we never collide with ASR output).
static SENTINEL_HASH: Lazy<String> = Lazy::new(|| {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u32)
        .unwrap_or(0);
    // Knuth multiplicative hash so we get a uniform-looking 8-hex prefix
    // without pulling in the `rand` crate. Collision probability against
    // natural ASR text is negligible.
    format!("{:08x}", nanos.wrapping_mul(0x9E3779B1))
});

fn make_sentinel(index: usize) -> String {
    format!("__MP_PROTECTED_v1_{}_{}__", SENTINEL_HASH.as_str(), index)
}

/// Global protected-term list, owned by the postprocessor so the chain
/// never has to plumb terms through every call site. Updated by
/// `set_protected_terms` before each recording session.
static PROTECTED_TERMS: Lazy<std::sync::Mutex<Vec<String>>> =
    Lazy::new(|| std::sync::Mutex::new(Vec::new()));

/// Public setter called by transcription / worker before invoking
/// `process_text`. Pass the result of `transcription_preferences::
/// get_protected_terms` (or an empty vec when no preferences are stored).
pub fn set_protected_terms(terms: Vec<String>) {
    if let Ok(mut guard) = PROTECTED_TERMS.lock() {
        *guard = terms;
    }
}

fn read_protected_terms() -> Vec<String> {
    PROTECTED_TERMS
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default()
}

// Wave 21 PR-F: parallel cache that holds the full hotword list (protected + bare)
// so the LLM summary prompts can inject it as a glossary block. Distinct from
// PROTECTED_TERMS which only stores the !-prefixed entries used for the
// postprocessor restoration step.
static HOTWORD_LIST_FOR_LLM: Lazy<std::sync::Mutex<Vec<String>>> =
    Lazy::new(|| std::sync::Mutex::new(Vec::new()));

pub fn set_hotwords_for_llm(terms: Vec<String>) {
    if let Ok(mut guard) = HOTWORD_LIST_FOR_LLM.lock() {
        *guard = terms;
    }
}

pub fn read_hotwords_for_llm() -> Vec<String> {
    HOTWORD_LIST_FOR_LLM
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- clean_repetitive_text ----

    #[test]
    fn cjk_short_interjection_preserved() {
        // 4x 嗯 = 12 UTF-8 bytes. Previously dropped as `unique_chars == 1 && text.len() > 10`.
        assert_eq!(PostProcessor::clean_repetitive_text("嗯嗯嗯嗯"), "嗯嗯嗯嗯");
    }

    #[test]
    fn cjk_long_interjection_preserved() {
        // 10x 啊 = 30 UTF-8 bytes. Below the new CJK threshold of 30 chars.
        assert_eq!(
            PostProcessor::clean_repetitive_text("啊啊啊啊啊啊啊啊啊啊啊啊"),
            "啊啊啊啊啊啊啊啊啊啊啊啊"
        );
    }

    #[test]
    fn english_uh_uh_uh_still_filtered() {
        assert_eq!(PostProcessor::clean_repetitive_text("uh uh uh"), "");
    }

    #[test]
    fn english_thanks_for_watching_still_filtered() {
        assert_eq!(
            PostProcessor::clean_repetitive_text("thank you for watching"),
            ""
        );
    }

    #[test]
    fn chinese_person_name_preserved() {
        assert_eq!(PostProcessor::clean_repetitive_text("张三丰"), "张三丰");
    }

    #[test]
    fn chinese_two_char_name_preserved() {
        assert_eq!(PostProcessor::clean_repetitive_text("李四"), "李四");
    }

    // ---- apply_contextual_improvements ----

    #[test]
    fn contractions_skip_inside_vacant() {
        assert_eq!(PostProcessor::apply_contextual_improvements("vacant"), "vacant");
    }

    #[test]
    fn contractions_skip_inside_scant() {
        assert_eq!(PostProcessor::apply_contextual_improvements("scant"), "scant");
    }

    #[test]
    fn contractions_skip_inside_cantilever() {
        assert_eq!(
            PostProcessor::apply_contextual_improvements("cantilever"),
            "cantilever"
        );
    }

    #[test]
    fn contractions_apply_standalone() {
        assert_eq!(
            PostProcessor::apply_contextual_improvements("cant believe it"),
            "can't believe it"
        );
    }

    #[test]
    fn contractions_case_preserved_capitalised() {
        assert_eq!(
            PostProcessor::apply_contextual_improvements("Cant believe"),
            "Can't believe"
        );
    }

    #[test]
    fn contractions_coexist_with_cjk() {
        assert_eq!(
            PostProcessor::apply_contextual_improvements("张三 said cant"),
            "张三 said can't"
        );
    }

    // ---- Wave 18 PR-55: protected-terms restoration ----

    // Wave 18 PR-55: tests share the global PROTECTED_TERMS cache, so a
    // per-test mutex serialises them and prevents parallel runs from
    // racing each other's terms. (Avoids pulling in the `serial_test` crate.)
    static TERMS_LOCK: once_cell::sync::Lazy<std::sync::Mutex<()>> =
        once_cell::sync::Lazy::new(|| std::sync::Mutex::new(()));

    fn set_terms_for_test(terms: Vec<&'static str>) {
        let _guard = TERMS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_protected_terms(terms.into_iter().map(String::from).collect());
    }

    #[test]
    fn protect_restore_roundtrip_single_term() {
        set_terms_for_test(vec!["张三"]);
        let (guarded, mapping) = PostProcessor::protect_terms("我和张三开会");
        assert_eq!(mapping.len(), 1);
        assert!(guarded.contains("__MP_PROTECTED_v1_"));
        assert!(!guarded.contains("张三"));
        let restored = PostProcessor::restore_protected_terms(&guarded, &mapping);
        assert_eq!(restored, "我和张三开会");
        set_terms_for_test(vec![]);
    }

    #[test]
    fn protect_handles_zero_terms() {
        set_terms_for_test(vec![]);
        let (guarded, mapping) = PostProcessor::protect_terms("hello world");
        assert_eq!(guarded, "hello world");
        assert!(mapping.is_empty());
        set_terms_for_test(vec![]);
    }

    #[test]
    fn protect_longest_match_wins_on_overlap() {
        // Caller sorts length-desc; here we pass "张三丰" first so the
        // protect loop tries the longer match before the shorter one.
        set_terms_for_test(vec!["张三丰", "张三"]);
        let (guarded, mapping) = PostProcessor::protect_terms("张三丰来了");
        assert_eq!(mapping.len(), 1);
        let restored = PostProcessor::restore_protected_terms(&guarded, &mapping);
        assert_eq!(restored, "张三丰来了");
        assert_eq!(mapping[0].1, "张三丰");
        set_terms_for_test(vec![]);
    }

    #[test]
    fn protect_multiple_occurrences_get_distinct_sentinels() {
        set_terms_for_test(vec!["张三"]);
        let (guarded, mapping) = PostProcessor::protect_terms("张三说张三");
        assert_eq!(mapping.len(), 2);
        // Sentinels should differ by index
        assert_ne!(mapping[0].0, mapping[1].0);
        let restored = PostProcessor::restore_protected_terms(&guarded, &mapping);
        assert_eq!(restored, "张三说张三");
        set_terms_for_test(vec![]);
    }

    #[test]
    fn protect_sentinel_format_is_stable() {
        set_terms_for_test(vec!["张三"]);
        let (_, mapping) = PostProcessor::protect_terms("张三");
        assert_eq!(mapping.len(), 1);
        let sentinel = &mapping[0].0;
        assert!(sentinel.starts_with("__MP_PROTECTED_v1_"));
        assert!(sentinel.ends_with("__"));
        // 8 hex chars between "v1_" and "_<index>"
        let middle = sentinel
            .trim_start_matches("__MP_PROTECTED_v1_")
            .trim_end_matches("__");
        let parts: Vec<&'static str> = middle.splitn(2, '_').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].len(), 8);
        assert!(parts[0].chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(parts[1], "0");
        set_terms_for_test(vec![]);
    }

    #[test]
    fn hotwords_for_llm_set_read_roundtrip_empty() {
        set_hotwords_for_llm(vec![]);
        assert!(read_hotwords_for_llm().is_empty());
    }

    #[test]
    fn hotwords_for_llm_set_read_roundtrip_multi() {
        set_hotwords_for_llm(vec!["AGI".to_string(), "OpenAI".to_string(), "foo".to_string()]);
        assert_eq!(read_hotwords_for_llm(), vec!["AGI", "OpenAI", "foo"]);
    }
}

impl Default for PostProcessor {
    fn default() -> Self {
        Self::new()
    }
}
