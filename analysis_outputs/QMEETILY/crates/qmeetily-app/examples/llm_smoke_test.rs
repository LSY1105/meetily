//! End-to-end smoke test for the Qwen3.5 LLM pipeline.
//!
//! Runs without Tauri or a GUI. Spawns llama-helper as a subprocess, sends
//! a chat request over stdin/stdout, and prints the response.
//!
//! Usage:
//!   MEETILY_LLAMA_HELPER=/path/to/llama-helper.exe \
//!     QMEETILY_DATA_DIR=$LOCALAPPDATA/QMeetily \
//!     cargo run --release --example llm_smoke_test

use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let helper_path = std::env::var("MEETILY_LLAMA_HELPER")
        .unwrap_or_else(|_| "../target/release/llama-helper.exe".into());
    let model_path = std::env::var("QMEETILY_MODEL_PATH")
        .unwrap_or_else(|_| {
            let local_app_data = std::env::var("LOCALAPPDATA")
                .unwrap_or_else(|_| ".".into());
            format!(
                "{}/QMeetily/models/qwen3.5-2b/Qwen3.5-2B-Q4_K_M.gguf",
                local_app_data
            )
        });

    println!("=== qmeetily LLM smoke test ===");
    println!("llama-helper: {helper_path}");
    println!("model:        {model_path}");

    let helper = PathBuf::from(&helper_path);
    let model = PathBuf::from(&model_path);

    if !helper.exists() {
        anyhow::bail!("llama-helper not found at {helper_path}. Build it with: cargo build --release -p llama-helper");
    }
    if !model.exists() {
        anyhow::bail!("model not found at {model_path}. Download it with: hf download unsloth/Qwen3.5-2B-GGUF --include 'Qwen3.5-2B-Q4_K_M.gguf'");
    }

    let mut child: Child = Command::new(&helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout).lines();

    // Step 1: Send Generate request
    let req = serde_json::json!({
        "type": "generate",
        "prompt": "QMeetily 是一个隐私优先的本地会议助手。请用三句话介绍它的核心功能。",
        "max_tokens": 1024,
        "context_size": 4096,
        "model_path": model.to_string_lossy(),
        "temperature": 0.7,
        "stop_tokens": ["<|im_end|>"]
    });
    let req_line = format!("{}\n", serde_json::to_string(&req)?);
    println!("\n--- sending generate request ---");
    println!("{}", serde_json::to_string_pretty(&req)?);
    stdin.write_all(req_line.as_bytes()).await?;
    stdin.flush().await?;

    // Step 2: Read response
    println!("\n--- waiting for response ---");
    let mut got_response = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while std::time::Instant::now() < deadline {
        tokio::select! {
            line = reader.next_line() => {
                match line? {
                    Some(line) => {
                        println!("< {line}");
                        let parsed: serde_json::Value = serde_json::from_str(&line)?;
                        if parsed.get("type").and_then(|v| v.as_str()) == Some("response") {
                            let text = parsed.get("text").and_then(|v| v.as_str()).unwrap_or("");
                            println!("\n=== LLM response ===");
                            println!("{text}");
                            println!("=====================");
                            got_response = true;
                            break;
                        }
                    }
                    None => break, // EOF
                }
            }
            _ = tokio::time::sleep_until(deadline.into()) => {
                eprintln!("timeout");
                break;
            }
        }
    }

    // Step 3: Shutdown
    let _ = stdin.write_all(b"{\"type\":\"shutdown\"}\n").await;
    drop(stdin);
    let _ = child.wait().await;

    if got_response {
        println!("\n[OK] end-to-end LLM smoke test passed");
        Ok(())
    } else {
        anyhow::bail!("did not receive response from llama-helper")
    }
}
