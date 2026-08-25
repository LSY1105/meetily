//! Context assembly for Q&A: turns meetings / transcripts / web results
//! into a numbered source list plus a single text block for the LLM.

use serde::Serialize;
use sqlx::SqlitePool;

const MAX_CONTEXT_CHARS: usize = 120_000;
const MAX_WEB_RESULTS: usize = 5;
const ALL_MODE_MEETING_LIMIT: i64 = 30;
const SUMMARY_SNIPPET_CHARS: usize = 1_200;
const SEGMENT_SNIPPET_CHARS: usize = 600;

#[derive(Debug, Clone, Serialize)]
pub struct QaSource {
    /// 1-based citation index used in the answer text (`[index]`).
    pub index: u32,
    pub label: String,
    /// "transcript" | "summary" | "web"
    pub kind: String,
    pub meeting_id: Option<String>,
    pub meeting_title: Option<String>,
    pub timestamp: Option<String>,
    pub url: Option<String>,
    pub snippet: String,
}

pub struct ContextBundle {
    pub context_text: String,
    pub sources: Vec<QaSource>,
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{}…", cut)
    }
}

/// Extracts search terms from a question. Whitespace tokens plus CJK
/// bigrams (CJK questions have no spaces, so 2-char shingles are the
/// cheapest usable retrieval unit).
fn extract_terms(question: &str) -> Vec<String> {
    let lower = question.to_lowercase();
    let mut terms: Vec<String> = Vec::new();
    for token in lower.split_whitespace() {
        if token.chars().count() >= 2 {
            terms.push(token.to_string());
        }
    }
    let cjk: Vec<char> = lower
        .chars()
        .filter(|c| {
            matches!(c,
                '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}')
        })
        .collect();
    if !cjk.is_empty() {
        for w in cjk.windows(2) {
            terms.push(w.iter().collect::<String>());
        }
    }
    terms
}

/// Simple relevance score: how many question terms appear in the text.
fn score_text(text: &str, terms: &[String]) -> usize {
    if terms.is_empty() {
        return 0;
    }
    let lower = text.to_lowercase();
    terms.iter().map(|t| lower.matches(t.as_str()).count()).sum()
}

/// Single-meeting scope: every transcript segment (chronological) plus the
/// saved summary. Over budget -> keep the highest-scoring segments (still
/// in chronological order) and note how many were omitted.
pub async fn build_meeting_context(
    pool: &SqlitePool,
    meeting_id: &str,
    question: &str,
) -> Result<ContextBundle, String> {
    let title: Option<(String,)> =
        sqlx::query_as("SELECT title FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| r);

    let meeting_title = title.map(|(t,)| t).unwrap_or_else(|| "未命名会议".into());

    let rows: Vec<(String, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT transcript, timestamp, speaker FROM transcripts \
             WHERE meeting_id = ? ORDER BY id ASC",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let terms = extract_terms(question);

    let mut sources: Vec<QaSource> = Vec::new();
    let mut context = String::new();
    let mut index: u32 = 0;

    // Summary first (if any).
    let summary_md = fetch_summary_markdown(pool, meeting_id).await;
    if let Some(md) = &summary_md {
        index += 1;
        sources.push(QaSource {
            index,
            label: format!("{} · 会议总结", meeting_title),
            kind: "summary".into(),
            meeting_id: Some(meeting_id.to_string()),
            meeting_title: Some(meeting_title.clone()),
            timestamp: None,
            url: None,
            snippet: truncate_chars(md, SUMMARY_SNIPPET_CHARS),
        });
    }

    // Score segments for the over-budget case.
    let scored: Vec<(usize, &(String, Option<String>, Option<String>))> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (score_text(&r.0, &terms), r))
        .collect();

    let total_chars: usize = rows.iter().map(|r| r.0.len()).sum();
    let over_budget = total_chars > MAX_CONTEXT_CHARS;

    let mut kept: Vec<usize> = Vec::new();
    if over_budget {
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(scored[i].0));
        let mut budget = MAX_CONTEXT_CHARS;
        for &i in &order {
            let len = rows[i].0.len();
            if len + 200 > budget {
                continue;
            }
            budget -= len + 200;
            kept.push(i);
            if budget < 2_000 {
                break;
            }
        }
        kept.sort_unstable();
    } else {
        kept = (0..rows.len()).collect();
    }

    for i in &kept {
        let (text, timestamp, speaker) = &rows[*i];
        index += 1;
        let ts = timestamp.clone().unwrap_or_default();
        let sp = speaker.clone().unwrap_or_default();
        let mut label = meeting_title.clone();
        if !ts.is_empty() {
            label.push_str(&format!(" · {}", ts));
        }
        if !sp.is_empty() {
            label.push_str(&format!(" · {}", sp));
        }
        sources.push(QaSource {
            index,
            label,
            kind: "transcript".into(),
            meeting_id: Some(meeting_id.to_string()),
            meeting_title: Some(meeting_title.clone()),
            timestamp: timestamp.clone(),
            url: None,
            snippet: truncate_chars(text, SEGMENT_SNIPPET_CHARS),
        });
    }

    if let Some(md) = &summary_md {
        context.push_str(&format!(
            "【会议总结】[{}]\n{}\n\n",
            sources[0].index,
            sources[0].snippet
        ));
    }
    for s in sources.iter().skip(if summary_md.is_some() { 1 } else { 0 }) {
        context.push_str(&format!("[{}] {}\n{}\n\n", s.index, s.label, s.snippet));
    }
    if over_budget {
        let omitted = rows.len() - kept.len();
        context.push_str(&format!(
            "[注意：为控制长度，另有 {} 个转录分段未纳入上下文]\n",
            omitted
        ));
    }

    Ok(ContextBundle { context_text: context, sources })
}

/// All-meetings scope: recent meetings, each contributing its summary
/// (truncated) plus its top-2 question-matched transcript segments.
pub async fn build_all_context(
    pool: &SqlitePool,
    question: &str,
) -> Result<ContextBundle, String> {
    let meetings: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, title FROM meetings ORDER BY created_at DESC LIMIT ?",
    )
    .bind(ALL_MODE_MEETING_LIMIT)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let terms = extract_terms(question);
    let mut sources: Vec<QaSource> = Vec::new();
    let mut context = String::from("以下是全部会议的资料（每条来源开头为 [编号]）：\n\n");
    let mut index: u32 = 0;
    let mut budget = MAX_CONTEXT_CHARS;

    for (mid, mtitle) in &meetings {
        if budget < 2_000 {
            break;
        }

        // Summary snippet
        let summary_md = fetch_summary_markdown(pool, mid).await;
        if let Some(md) = summary_md {
            index += 1;
            let snippet = truncate_chars(&md, SUMMARY_SNIPPET_CHARS);
            budget = budget.saturating_sub(snippet.len());
            context.push_str(&format!("[{}] 《{}》会议总结\n{}\n\n", index, mtitle, snippet));
            sources.push(QaSource {
                index,
                label: format!("《{}》· 会议总结", mtitle),
                kind: "summary".into(),
                meeting_id: Some(mid.clone()),
                meeting_title: Some(mtitle.clone()),
                timestamp: None,
                url: None,
                snippet,
            });
        }

        // Top-2 matched segments
        let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT transcript, timestamp, speaker FROM transcripts \
             WHERE meeting_id = ? ORDER BY id ASC",
        )
        .bind(mid)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        let mut scored: Vec<(usize, usize)> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| (score_text(&r.0, &terms), i))
            .collect();
        scored.sort_by_key(|&(s, _)| std::cmp::Reverse(s));

        for &(score, i) in scored.iter().take(2) {
            if score == 0 && !terms.is_empty() {
                continue;
            }
            let (text, timestamp, speaker) = &rows[i];
            let snippet = truncate_chars(text, SEGMENT_SNIPPET_CHARS);
            budget = budget.saturating_sub(snippet.len());
            index += 1;
            let ts = timestamp.clone().unwrap_or_default();
            let mut label = mtitle.clone();
            if !ts.is_empty() {
                label.push_str(&format!(" · {}", ts));
            }
            if !speaker.clone().unwrap_or_default().is_empty() {
                label.push_str(&format!(" · {}", speaker.clone().unwrap()));
            }
            context.push_str(&format!("[{}] {}\n{}\n\n", index, label, snippet));
            sources.push(QaSource {
                index,
                label,
                kind: "transcript".into(),
                meeting_id: Some(mid.clone()),
                meeting_title: Some(mtitle.clone()),
                timestamp: timestamp.clone(),
                url: None,
                snippet,
            });
        }
    }

    if sources.is_empty() {
        return Err("没有任何会议资料可以检索，请先录制或导入会议".into());
    }

    Ok(ContextBundle { context_text: context, sources })
}

/// Web enrichment via Tavily. `api_key` comes from app_settings.
pub async fn build_web_context(
    question: &str,
    api_key: &str,
) -> Result<ContextBundle, String> {
    #[derive(serde::Deserialize)]
    struct TavilyResult {
        title: Option<String>,
        url: Option<String>,
        content: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct TavilyResponse {
        results: Option<Vec<TavilyResult>>,
    }

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.tavily.com/search")
        .bearer_auth(api_key.trim())
        .json(&serde_json::json!({
            "query": question,
            "max_results": MAX_WEB_RESULTS,
            "search_depth": "basic",
        }))
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("网络搜索请求失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("网络搜索失败: HTTP {}", resp.status()));
    }

    let parsed: TavilyResponse = resp
        .json()
        .await
        .map_err(|e| format!("解析搜索结果失败: {}", e))?;

    let mut sources: Vec<QaSource> = Vec::new();
    let mut context = String::from("以下是与问题相关的网络资料：\n\n");
    let mut index = sources.len() as u32;

    for r in parsed.results.unwrap_or_default().into_iter().take(MAX_WEB_RESULTS) {
        index += 1;
        let title = r.title.unwrap_or_else(|| "Untitled".into());
        let url = r.url.unwrap_or_default();
        let content = truncate_chars(&r.content.unwrap_or_default(), 800);
        context.push_str(&format!("[W{}] {}（{}）\n{}\n\n", index, title, url, content));
        sources.push(QaSource {
            index,
            label: format!("Web · {}", title),
            kind: "web".into(),
            meeting_id: None,
            meeting_title: None,
            timestamp: None,
            url: Some(url.clone()),
            snippet: content,
        });
    }

    if sources.is_empty() {
        return Err("网络搜索没有返回结果".into());
    }

    Ok(ContextBundle { context_text: context, sources })
}

/// Extracts human-readable markdown from the summary_processes result JSON.
/// Falls back to the raw JSON string when the shape is unknown.
async fn fetch_summary_markdown(pool: &SqlitePool, meeting_id: &str) -> Option<String> {
    let raw: Option<(Option<String>,)> =
        sqlx::query_as("SELECT result FROM summary_processes WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .ok()?;

    let result_str = raw?.0?;
    let parsed: serde_json::Value = serde_json::from_str(&result_str).ok()?;

    if let Some(md) = parsed.get("markdown").and_then(|v| v.as_str()) {
        return Some(md.to_string());
    }
    // Common alternative: {"data": {"markdown": ...}}
    if let Some(md) = parsed
        .get("data")
        .and_then(|d| d.get("markdown"))
        .and_then(|v| v.as_str())
    {
        return Some(md.to_string());
    }
    Some(truncate_chars(&result_str, SUMMARY_SNIPPET_CHARS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_whitespace_terms() {
        let t = extract_terms("What about budget planning?");
        assert!(t.contains(&"what".to_string()));
        assert!(t.contains(&"budget".to_string()));
    }

    #[test]
    fn extracts_cjk_bigrams() {
        let t = extract_terms("讨论预算");
        assert!(t.contains(&"讨论".to_string()));
        assert!(t.contains(&"论预".to_string()));
        assert!(t.contains(&"预算".to_string()));
    }

    #[test]
    fn scores_term_overlap() {
        let terms = extract_terms("预算");
        assert!(score_text("我们讨论了预算问题", &terms) > 0);
        assert_eq!(score_text("no match here", &terms), 0);
    }

    #[test]
    fn truncates_by_chars_not_bytes() {
        let s = "中".repeat(50);
        let t = truncate_chars(&s, 10);
        assert_eq!(t.chars().count(), 11); // 10 + ellipsis
    }
}
