//! Q&A Tauri commands.

use super::context::{build_all_context, build_meeting_context, build_web_context, QaSource};
use crate::database::repositories::setting::SettingsRepository;
use crate::summary::llm_client::{generate_summary, LLMProvider};
use serde::Serialize;
use tauri::Runtime;

#[derive(Debug, Serialize)]
pub struct QaAnswer {
    pub answer: String,
    pub sources: Vec<QaSource>,
}

/// Search-provider key (Tavily) stored in app_settings via the existing KV.
const SEARCH_KEY_KV: &str = "tavily_api_key";

#[tauri::command]
pub async fn qa_set_search_key(
    state: tauri::State<'_, crate::state::AppState>,
    key: String,
) -> Result<(), String> {
    let pool = state.db_manager.pool();
    let trimmed = key.trim().to_string();
    if trimmed.is_empty() {
        SettingsRepository::delete_kv(pool, SEARCH_KEY_KV)
            .await
            .map_err(|e| e.to_string())
    } else {
        SettingsRepository::set_kv(pool, SEARCH_KEY_KV, &trimmed)
            .await
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn qa_has_search_key(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<bool, String> {
    let pool = state.db_manager.pool();
    let v = SettingsRepository::get_kv(pool, SEARCH_KEY_KV)
        .await
        .map_err(|e| e.to_string())?;
    Ok(v.map(|k| !k.trim().is_empty()).unwrap_or(false))
}

/// Ask a question against a single meeting or the whole corpus, with
/// optional web enrichment. Uses the currently configured summarization
/// model for the actual LLM call.
#[tauri::command]
pub async fn api_ask_meeting<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, crate::state::AppState>,
    question: String,
    scope: String,
    meeting_id: Option<String>,
    use_web: bool,
) -> Result<QaAnswer, String> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err("问题不能为空".into());
    }

    let pool = state.db_manager.pool();

    // ---- Resolve LLM configuration (mirrors summary service) ----
    let model_config = SettingsRepository::get_model_config(&pool)
        .await
        .map_err(|e| format!("读取模型配置失败: {}", e))?
        .ok_or_else(|| "尚未配置总结模型，请先在设置中选择模型".to_string())?;
    let model_provider = model_config.provider.clone();
    let model_name = if model_config.model.is_empty() {
        return Err("尚未配置总结模型，请先在设置中选择模型".into());
    } else {
        model_config.model.clone()
    };

    let provider = LLMProvider::from_str(&model_provider)
        .map_err(|e| format!("不支持的模型提供商: {}", e))?;

    let api_key = if provider == LLMProvider::Ollama
        || provider == LLMProvider::BuiltInAI
        || provider == LLMProvider::CustomOpenAI
    {
        String::new()
    } else {
        match SettingsRepository::get_api_key(&pool, &model_provider).await {
            Ok(Some(key)) if !key.is_empty() => key,
            Ok(None) | Ok(Some(_)) => {
                return Err(format!("未找到 {} 的 API key，请先在设置中配置", &model_provider))
            }
            Err(e) => return Err(format!("读取 API key 失败: {}", e)),
        }
    };

    let ollama_endpoint = if provider == LLMProvider::Ollama {
        model_config.ollama_endpoint.clone()
    } else {
        None
    };

    let custom_openai_endpoint = if provider == LLMProvider::CustomOpenAI {
        Some(
            SettingsRepository::get_custom_openai_config(&pool)
                .await
                .map_err(|e| format!("读取自定义端点配置失败: {}", e))?
                .ok_or_else(|| "选择了自定义 OpenAI 提供商但未配置端点".to_string())?
                .endpoint,
        )
    } else {
        None
    };

    // ---- Build context ----
    let mut sources: Vec<QaSource> = Vec::new();
    let mut context_text = String::new();

    match scope.as_str() {
        "all" => {
            let bundle = build_all_context(&pool, &question).await?;
            context_text.push_str(&bundle.context_text);
            sources.extend(bundle.sources);
        }
        _ => {
            let mid = meeting_id.ok_or_else(|| "单会议模式需要 meeting_id".to_string())?;
            let bundle = build_meeting_context(&pool, &mid, &question).await?;
            context_text.push_str(&bundle.context_text);
            sources.extend(bundle.sources);
        }
    }

    if use_web {
        let key = SettingsRepository::get_kv(&pool, SEARCH_KEY_KV)
            .await
            .map_err(|e| format!("读取搜索配置失败: {}", e))?
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| "未配置网络搜索 Key，请先在问答面板中填写 Tavily API Key".to_string())?;
        let bundle = build_web_context(&question, &key).await?;
        context_text.push_str(&bundle.context_text);
        sources.extend(bundle.sources);
    }

    // ---- Prompt ----
    let system_prompt = "\
你是一名会议助手。仅根据提供的资料回答用户问题；引用资料时在句子后标注来源编号，\
格式为 [编号]（如 [2] 或 [2][5]）。如果资料不足以回答，请明确说明，不要编造。\
使用与用户提问相同的语言回答。回答要简洁、直接、结构清晰。";

    let user_prompt = format!(
        "以下是参考资料：\n\n{}\n\n用户问题：{}",
        context_text, question
    );

    // ---- LLM call ----
    let client = reqwest::Client::new();
    let answer = generate_summary(
        &client,
        &provider,
        &model_name,
        &api_key,
        system_prompt,
        &user_prompt,
        ollama_endpoint.as_deref(),
        custom_openai_endpoint.as_deref(),
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .map_err(|e| format!("AI 回答失败: {}", e))?;

    Ok(QaAnswer { answer, sources })
}
