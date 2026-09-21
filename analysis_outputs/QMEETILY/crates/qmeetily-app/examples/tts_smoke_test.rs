//! TTS end-to-end smoke test (Rust side).
//!
//! Loads Qwen3-TTS-0.6B-Base, generates speech from text, writes WAV to disk.
//! Verifies the model loads on ARM64 Windows + produces non-empty audio.

use std::path::PathBuf;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let model_path = std::env::var("QMEETILY_TTS_MODEL")
        .unwrap_or_else(|_| {
            let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
            format!(
                "{}/QMeetily/models/qwen3-tts-0.6b-base",
                local_app_data
            )
        });

    println!("=== qmeetily TTS smoke test ===");
    println!("model: {model_path}");

    let model_dir = PathBuf::from(&model_path);
    if !model_dir.exists() {
        anyhow::bail!("model not found at {model_path}. Download via: hf download Qwen/Qwen3-TTS-12Hz-0.6B-Base --local-dir {model_path}");
    }

    // We can't easily invoke the Python Qwen3-TTS package from Rust, so
    // this smoke test shells out to a Python helper that does the work.
    let python = std::env::var("QMEETILY_SIDECAR_PYTHON")
        .unwrap_or_else(|_| {
            // Default: look for the sidecar venv python next to the helper script.
            // This ensures torch + qwen_tts are available.
            std::env::current_dir().ok()
                .map(|d| d.join("sidecar/.venv/Scripts/python.exe"))
                .unwrap_or_else(|| std::path::PathBuf::from("python"))
        });
    // Try a few candidate paths because the example binary can be run from
    // different cwds (workspace root, target/, etc.).
    let candidates = [
        std::env::current_dir().ok().map(|d| d.join("sidecar/scripts/tts_smoke.py")),
        std::env::current_exe().ok()
            .and_then(|p| p.parent().map(|d| d.join("../../../sidecar/scripts/tts_smoke.py"))),
        Some(PathBuf::from("sidecar/scripts/tts_smoke.py")),
        Some(PathBuf::from("../sidecar/scripts/tts_smoke.py")),
    ];
    let helper = candidates.into_iter()
        .flatten()
        .find(|p| p.exists())
        .ok_or_else(|| anyhow::anyhow!(
            "tts_smoke.py not found in any of: sidecar/scripts/tts_smoke.py, ../sidecar/scripts/tts_smoke.py"
        ))?;

    println!("helper: {}", helper.display());
    if !helper.exists() {
        anyhow::bail!("helper script not found: {}. Run `pnpm install` then re-check.", helper.display());
    }

    let text = std::env::var("QMEETILY_TTS_TEXT")
        .unwrap_or_else(|_| "Hello, this is a test of the QMeetily text to speech system.".to_string());
    let output_wav = std::env::var("QMEETILY_TTS_OUTPUT")
        .unwrap_or_else(|_| "./qmeetily-tts-test.wav".to_string());

    println!("text: {text}");
    println!("output: {output_wav}");

    let t0 = Instant::now();
    let status = std::process::Command::new(&python)
        .arg(&helper)
        .arg("--model-dir").arg(&model_path)
        .arg("--text").arg(&text)
        .arg("--output").arg(&output_wav)
        .status()?;
    let elapsed = t0.elapsed().as_secs_f32();
    println!("Python helper exited {} in {:.2}s", status, elapsed);

    if !status.success() {
        anyhow::bail!("Python helper failed");
    }

    // Verify the WAV file exists and has non-trivial size
    let wav_path = PathBuf::from(&output_wav);
    let meta = std::fs::metadata(&wav_path)?;
    if meta.len() < 1024 {
        anyhow::bail!("WAV file too small ({} bytes); TTS likely produced silence", meta.len());
    }

    println!();
    println!("output WAV: {} ({} bytes)", wav_path.display(), meta.len());
    println!();
    println!("[OK] TTS smoke test passed");
    Ok(())
}
