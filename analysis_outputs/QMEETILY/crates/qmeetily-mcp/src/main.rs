//! qmeetily-mcp — standalone MCP server binary.
//!
//! Configure in Claude Desktop / Cursor / Codex:
//!   { "command": "qmeetily-mcp", "args": [] }
//!
//! Wire: JSON-RPC 2.0 over stdio. Compatible with any MCP client.

use qmeetily_lib::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let state = AppState::new_standalone().await?;
    qmeetily_lib::mcp::serve_stdio(state).await
}
