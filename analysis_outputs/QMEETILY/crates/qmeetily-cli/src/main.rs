//! qmeetily-cli — headless command-line driver.
//!
//! Read-only ops + model listing. Summary generation itself goes through the
//! Tauri command `generate_summary`, which spawns the sidecar on demand.
//!
//! Usage:
//!   qmeetily-cli status
//!   qmeetily-cli list [--limit N]
//!   qmeetily-cli models

use clap::{Parser, Subcommand};
use qmeetily_lib::state::AppState;

#[derive(Parser)]
#[command(name = "qmeetily-cli", about = "QMeetily headless driver")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show app + DB status
    Status,
    /// List recent meetings
    List {
        #[arg(short, long, default_value_t = 10)]
        limit: u32,
    },
    /// List available built-in LLM models
    Models,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let state = AppState::new_standalone().await?;

    match cli.cmd {
        Cmd::Status => {
            println!("App data dir:  {:?}", state.config().data_dir);
            println!("Models dir:    {:?}", state.config().models_dir);
            println!("Default LLM:   {}", state.config().preferred_llm_model);
            Ok(())
        }
        Cmd::List { limit } => {
            let meetings = state.db().list_meetings(limit, 0).await?;
            if meetings.is_empty() {
                println!("(no meetings yet)");
            } else {
                for m in meetings {
                    println!("[{}] {} ({})", m.id, m.title, m.started_at);
                }
            }
            Ok(())
        }
        Cmd::Models => {
            use qmeetily_lib::summary_engine::models::get_available_models;
            println!("Available models (auto-download on first use):");
            for m in get_available_models() {
                println!("  {:<14} ({:>5.0} MB)  {}", m.name, m.size_mb, m.display_name);
            }
            Ok(())
        }
    }
}
