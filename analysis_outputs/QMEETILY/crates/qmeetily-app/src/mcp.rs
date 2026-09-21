//! MCP (Model Context Protocol) — minimal stdlib-only implementation.
//!
//! Why not use the `rmcp` crate? It uses procedural macros (`#[tool_router]`,
//! `#[tool]`) that couple tightly to the tool's struct layout, and its
//! generated code is hard to debug. For our 4 simple read-only tools,
//! a 100-line stdlib implementation is clearer and lighter.
//!
//! Protocol: JSON-RPC 2.0 over stdin/stdout (newline-delimited).
//! This matches the MCP stdio transport contract. Compatible with Claude
//! Desktop, Cursor, and Codex out of the box.
//!
//! Wire format:
//!   request:  `{"jsonrpc":"2.0","id":N,"method":"tools/call","params":{...}}\n`
//!   response: `{"jsonrpc":"2.0","id":N,"result":{...}}\n` or `error`
//!   notification: `{"jsonrpc":"2.0","method":"notifications/initialized"}\n`
//!
//! To upgrade to `rmcp` later: replace `serve` body with `QmeetilyMcp::serve(...)`.
//! The tool semantics stay identical.

use crate::state::AppState;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

#[derive(Debug, Deserialize)]
struct RpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

const METHOD_NOT_FOUND: i32 = -32601;
const INTERNAL_ERROR: i32 = -32603;
const INVALID_PARAMS: i32 = -32602;

/// Serve MCP over stdio. Blocks until stdin closes.
pub async fn serve_stdio(state: AppState) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // Announce capabilities
    write_response(
        &mut out,
        RpcResponse {
            jsonrpc: "2.0",
            id: None,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "qmeetily",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            })),
            error: None,
        },
    )?;

    let mut input = String::new();
    loop {
        input.clear();
        let n = stdin.lock().read_line(&mut input)?;
        if n == 0 {
            break; // EOF
        }
        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: RpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                write_response(
                    &mut out,
                    RpcResponse {
                        jsonrpc: "2.0",
                        id: None,
                        result: None,
                        error: Some(RpcError { code: INVALID_PARAMS, message: format!("parse error: {e}") }),
                    },
                )?;
                continue;
            }
        };

        let resp = handle(&state, req).await;
        write_response(&mut out, resp)?;
    }
    Ok(())
}

async fn handle(state: &AppState, req: RpcRequest) -> RpcResponse {
    let id = req.id.clone();
    match req.method.as_str() {
        "initialize" => RpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "qmeetily",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            })),
            error: None,
        },

        "tools/list" => RpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "tools": tools_list()
            })),
            error: None,
        },

        "tools/call" => {
            let tool_name = req.params.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let args = req.params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(state, &tool_name, args).await {
                Ok(content) => RpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(json!({ "content": [{ "type": "text", "text": content }] })),
                    error: None,
                },
                Err(e) => RpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(RpcError { code: INTERNAL_ERROR, message: format!("{e}") }),
                },
            }
        }

        "ping" => RpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({})),
            error: None,
        },

        _ => RpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError { code: METHOD_NOT_FOUND, message: format!("unknown method: {}", req.method) }),
        },
    }
}

fn tools_list() -> Value {
    json!([
        {
            "name": "search_meetings",
            "description": "Full-text search across meeting transcripts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "integer", "default": 10 }
                },
                "required": ["query"]
            }
        },
        {
            "name": "get_meeting",
            "description": "Get meeting metadata by ID.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "integer" } },
                "required": ["id"]
            }
        },
        {
            "name": "get_transcript",
            "description": "Get full transcript of a meeting.",
            "inputSchema": {
                "type": "object",
                "properties": { "meeting_id": { "type": "integer" } },
                "required": ["meeting_id"]
            }
        },
        {
            "name": "list_meetings",
            "description": "List recent meetings.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "default": 20 }
                }
            }
        }
    ])
}

async fn call_tool(state: &AppState, name: &str, args: Value) -> Result<String> {
    match name {
        "search_meetings" => {
            let query = args.get("query").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'query'"))?;
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as u32;
            let hits = state.db().search(query, limit).await?;
            Ok(serde_json::to_string_pretty(&hits)?)
        }
        "get_meeting" => {
            let id = args.get("id").and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;
            let m = state.db().get_meeting(id).await?;
            Ok(serde_json::to_string_pretty(&m)?)
        }
        "get_transcript" => {
            let meeting_id = args.get("meeting_id").and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("missing 'meeting_id'"))?;
            let t = state.db().get_meeting_transcripts(meeting_id).await?;
            Ok(serde_json::to_string_pretty(&t)?)
        }
        "list_meetings" => {
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as u32;
            let m = state.db().list_meetings(limit, 0).await?;
            Ok(serde_json::to_string_pretty(&m)?)
        }
        _ => anyhow::bail!("unknown tool: {name}"),
    }
}

fn write_response<W: Write>(out: &mut W, resp: RpcResponse) -> Result<()> {
    let s = serde_json::to_string(&resp)? + "\n";
    out.write_all(s.as_bytes())?;
    out.flush()?;
    Ok(())
}
