//! LLM client — high-level API for built-in AI generation.
//!
//! **Borrowed from meetily, refactored**: removed the global `SIDECAR_MANAGER`
//! static and the `Lazy<RwLock<HashMap>>` cache. Now uses an instance-based
//! design that fits qmeetily's single `Arc<AppState>` source of truth.
//!
//! Usage:
//! ```ignore
//! let mgr = SidecarManager::new(data_dir)?;
//! let client = LlmClient::new(mgr, data_dir);
//! let text = client.summarize_transcript(transcript, Some("qwen3.5:2b"), None, None).await?;
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::models::{get_model_by_name, ModelDef};
use super::sidecar::SidecarManager;
use tauri::{AppHandle, Emitter};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Ping,
    Shutdown,
    Generate {
        prompt: String,
        max_tokens: Option<i32>,
        context_size: Option<u32>,
        model_path: Option<String>,
        temperature: Option<f32>,
        top_k: Option<i32>,
        top_p: Option<f32>,
        presence_penalty: Option<f32>,
        frequency_penalty: Option<f32>,
        repeat_penalty: Option<f32>,
        penalty_last_n: Option<i32>,
        stop_tokens: Option<Vec<String>>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Response {
        text: String,
        error: Option<String>,
    },
    Pong,
    Goodbye,
    Error {
        message: String,
    },
}

// ============================================================================
// LlmClient
// ============================================================================

/// High-level LLM client wrapping the llama-helper sidecar.
///
/// **Not** a singleton — create one per use (or hold one in your AppState).
/// The underlying `SidecarManager` handles process lifecycle.
///
/// `data_dir` is the qmeetily app data directory, used to resolve GGUF
/// model paths via `models::get_model_path(data_dir, model_name)`.
pub struct LlmClient {
    sidecar: Arc<SidecarManager>,
    data_dir: PathBuf,
    app: Option<AppHandle>,
    model_label: Option<String>,
}

impl LlmClient {
    pub fn new(sidecar: Arc<SidecarManager>, data_dir: PathBuf) -> Self {
        Self {
            sidecar,
            data_dir,
            app: None,
            model_label: None,
        }
    }

    pub fn with_app(mut self, app: AppHandle, model_label: String) -> Self {
        self.app = Some(app);
        self.model_label = Some(model_label);
        self
    }

    /// Ping the sidecar to verify it's healthy.
    pub async fn ping(&self) -> Result<()> {
        self.sidecar.send_request(serde_json::to_string(&Request::Ping)?, Duration::from_secs(5)).await?;
        Ok(())
    }

    /// Generate text completion.
    ///
    /// `model_name`: e.g. "qwen3.5:2b". If None, uses default.
    /// `cancellation`: optional — cancels mid-generation.
    pub async fn generate(
        &self,
        prompt: String,
        model_name: Option<String>,
        max_tokens: Option<i32>,
        cancellation: Option<CancellationToken>,
    ) -> Result<String> {
        let model = resolve_model(model_name.as_deref())?;
        let model_path = self.resolve_model_path(&model)?;
        let sampling = &model.sampling;

        // Start sidecar if not already running, with this model loaded.
        let label = self
            .model_label
            .clone()
            .unwrap_or_else(|| model.name.clone());
        if let Some(app) = &self.app {
            let _ = app.emit("model-loading", &label);
        }
        let load_result = self.sidecar.ensure_running(model_path).await;
        if let Some(app) = &self.app {
            match &load_result {
                Ok(()) => {
                    let _ = app.emit("model-loaded", &label);
                }
                Err(e) => {
                    let _ = app.emit(
                        "model-load-failed",
                        serde_json::json!({
                            "model_name": label,
                            "error": e.to_string(),
                        }),
                    );
                }
            }
        }
        load_result?;

        let req = Request::Generate {
            prompt,
            max_tokens: Some(max_tokens.unwrap_or(1024)),
            context_size: Some(model.context_size),
            model_path: None, // sidecar already loaded it
            temperature: Some(sampling.temperature),
            top_k: Some(sampling.top_k),
            top_p: Some(sampling.top_p),
            presence_penalty: Some(sampling.presence_penalty),
            frequency_penalty: Some(sampling.frequency_penalty),
            repeat_penalty: Some(sampling.repeat_penalty),
            penalty_last_n: Some(sampling.penalty_last_n),
            stop_tokens: Some(sampling.stop_tokens.clone()),
        };

        let req_json = serde_json::to_string(&req)?;
        let resp_json: String = if let Some(token) = cancellation {
            tokio::select! {
                result = self.sidecar.send_request(
                    req_json,
                    Duration::from_secs(600),
                ) => result?,
                _ = token.cancelled() => {
                    return Err(anyhow!("generation cancelled"));
                }
            }
        } else {
            self.sidecar.send_request(req_json, Duration::from_secs(600)).await?
        };

        let resp: Response = serde_json::from_str(&resp_json)
            .with_context(|| format!("invalid sidecar response: {resp_json}"))?;
        let text: String = match resp {
            Response::Response { text, error } => {
                if let Some(err) = error {
                    return Err(anyhow!("sidecar error: {err}"));
                }
                text
            }
            Response::Error { message } => return Err(anyhow!("sidecar error: {message}")),
            Response::Pong | Response::Goodbye => {
                return Err(anyhow!("unexpected sidecar response: {resp_json}"));
            }
        };
        Ok(text)
    }

    /// Summarize a meeting transcript.
    ///
    /// `model_name`: defaults to "qwen3.5:2b".
    /// `template_name`: chat template (e.g. "qwen3.5_nonthinking"); None = auto from model.
    /// `language`: currently unused; reserved for future.
    pub async fn summarize_transcript(
        &self,
        transcript: String,
        model_name: Option<String>,
        template_name: Option<String>,
        _language: Option<String>,
    ) -> Result<String> {
        let model = resolve_model(model_name.as_deref())?;
        let template = template_name.unwrap_or_else(|| model.template.clone());

        let system_prompt = "你是一个会议纪要助手。基于会议转写,生成结构化中文摘要。\
                             包含:议题、决策、行动项(负责人+截止日期)、风险、未决问题。\
                             使用 Markdown 格式。";
        let prompt = super::models::format_prompt(&template, system_prompt, &transcript)?;
        self.generate(prompt, Some(model.name.clone()), Some(1500), None).await
    }

    fn resolve_model_path(&self, model: &ModelDef) -> Result<PathBuf> {
        super::models::get_model_path(&self.data_dir, &model.name)
    }
}

fn resolve_model(name: Option<&str>) -> Result<ModelDef> {
    let default_name = super::models::get_default_model().name;
    let name = name.unwrap_or(default_name.as_str());
    get_model_by_name(name)
        .ok_or_else(|| anyhow!("unknown model: {name}; check available models via `qmeetily-cli models`"))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serialization() {
        let req = Request::Ping;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"ping\""));
    }

    #[test]
    fn response_deserialization() {
        let json = r#"{"type":"response","text":"hello","error":null}"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        match resp {
            Response::Response { text, error } => {
                assert_eq!(text, "hello");
                assert!(error.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn error_response_deserialization() {
        let json = r#"{"type":"error","message":"oops"}"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        match resp {
            Response::Error { message } => assert_eq!(message, "oops"),
            _ => panic!("wrong variant"),
        }
    }
}
