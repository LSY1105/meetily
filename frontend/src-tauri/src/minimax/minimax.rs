// MiniMax open-platform client (OpenAI-compatible API).
//
// Docs: https://platform.minimaxi.com/docs/api-reference/text-openai-api
// Base URL (domestic / Token 计划): https://api.minimaxi.com/v1
// Auth: Bearer API key. `GET /v1/models` lists available models in the
// standard OpenAI `{ object: "list", data: [{ id, ... }] }` shape.

use serde::{Deserialize, Serialize};
use tauri::command;
use reqwest::blocking::Client;

#[derive(Debug, Serialize, Deserialize)]
pub struct MiniMaxModel {
    pub id: String,
    pub owned_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MiniMaxModelsResponse {
    data: Option<Vec<MiniMaxApiModel>>,
}

#[derive(Debug, Deserialize)]
struct MiniMaxApiModel {
    id: String,
    #[serde(default)]
    owned_by: Option<String>,
}

#[command]
pub fn get_minimax_models(api_key: Option<String>) -> Result<Vec<MiniMaxModel>, String> {
    let key = api_key
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| "MiniMax API key is required to list models".to_string())?;

    let client = Client::new();
    let response = client
        .get("https://api.minimaxi.com/v1/models")
        .bearer_auth(key.trim())
        .send()
        .map_err(|e| format!("Failed to make HTTP request: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "HTTP request failed with status: {}",
            response.status()
        ));
    }

    let api_response: MiniMaxModelsResponse = response
        .json()
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    Ok(api_response
        .data
        .unwrap_or_default()
        .into_iter()
        .map(|m| MiniMaxModel {
            id: m.id,
            owned_by: m.owned_by,
        })
        .collect())
}
