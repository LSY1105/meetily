//! End-to-end ASR smoke test.
//!
//! Spawns the Qwen3-ASR Python sidecar, uploads a synthetic 5s WAV, and prints
//! the sidecar's response.
//!
//! Usage:
//!   cd analysis_outputs/QMEETILY
//!   cargo run --example asr_smoke_test

use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    println!("=== qmeetily ASR end-to-end smoke test ===");

    let python = std::env::var("QMEETILY_ASR_PYTHON")
        .unwrap_or_else(|_| "python".into());
    let sidecar_dir = std::env::var("QMEETILY_ASR_DIR")
        .unwrap_or_else(|_| "../sidecar".into());
    let use_existing = std::env::var("QMEETILY_ASR_URL").is_ok();

    let mut child: Option<Child> = None;
    let health_url = std::env::var("QMEETILY_ASR_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:11436/health".into());

    if !use_existing {
        println!("spawning sidecar: {python} -m qmeetily_sidecar_asr.server");
        println!("  cwd: {sidecar_dir}");

        let c = Command::new(&python)
            .arg("-m").arg("qmeetily_sidecar_asr.server")
            .current_dir(&sidecar_dir)
            .env("QMEETILY_ASR_PORT", "11436")
            .env("HF_ENDPOINT", "https://hf-mirror.com")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        child = Some(c);
    } else {
        println!("using existing sidecar at {health_url}");
    }

    let client = reqwest::Client::new();
    let mut ready = false;
    for _ in 0..180 {
        if let Ok(resp) = client.get(&health_url).send().await {
            if resp.status().is_success() {
                ready = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    if !ready {
        eprintln!("[FAIL] sidecar did not become ready within 180s");
        if let Some(mut c) = child { let _ = c.kill(); }
        std::process::exit(1);
    }
    println!("sidecar ready");

    println!("generating synthetic 5s 440Hz WAV @ 48kHz/2ch...");
    let sr = 48_000u32;
    let duration = 5.0_f32;
    let n = (sr as f32 * duration) as usize;
    let mut samples: Vec<f32> = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let v = 0.3 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
        samples.push(v);
        samples.push(v);
    }

    println!("uploading to sidecar /v1/audio/transcriptions...");
    let wav = pcm_f32_to_wav_stereo(&samples, sr);

    let form = reqwest::multipart::Form::new()
        .text("model", "Qwen/Qwen3-ASR-0.6B")
        .text("language", "English")
        .part(
            "file",
            reqwest::multipart::Part::bytes(wav)
                .file_name("test.wav")
                .mime_str("audio/wav")?,
        );
    let t0 = std::time::Instant::now();
    let resp = client
        .post("http://127.0.0.1:11436/v1/audio/transcriptions")
        .multipart(form)
        .send()
        .await?;
    println!("HTTP {} in {:.2}s", resp.status(), t0.elapsed().as_secs_f32());
    if resp.status() != reqwest::StatusCode::OK {
        let body = resp.text().await.unwrap_or_default();
        eprintln!("[FAIL] sidecar returned error: {body}");
        if let Some(mut c) = child { let _ = c.kill(); }
        std::process::exit(1);
    }
    let json: serde_json::Value = resp.json().await?;
    println!("--- sidecar response ---");
    println!("{}", serde_json::to_string_pretty(&json)?);

    if let Some(mut c) = child {
        let _ = c.kill();
        let _ = c.wait();
    }
    println!("\n[OK] ASR smoke test passed");
    Ok(())
}

/// Convert interleaved stereo f32 PCM to a minimal WAV byte stream.
fn pcm_f32_to_wav_stereo(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let channels = 2u16;
    let n = samples.len();
    let mut buf = Vec::with_capacity(44 + n * 2);
    buf.extend_from_slice(b"RIFF");
    let data_size = (n * 2) as u32;
    buf.extend_from_slice(&(36 + data_size).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * (channels as u32) * 2;
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&((channels * 2) as u16).to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}
