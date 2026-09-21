//! End-to-end smoke test for audio capture.
//!
//! Runs without Tauri/GUI. Opens the default input device, captures 2 seconds
//! of audio, prints the RMS level + sample count.
//!
//! Usage:
//!   cargo run --example audio_smoke_test

use std::time::Duration;
use qmeetily_lib::audio::capture::{AudioCapture, CaptureConfig};
use qmeetily_lib::audio::vad::{VadProcessor, VadEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    println!("=== qmeetily audio capture + VAD smoke test ===");

    let cfg = CaptureConfig::default();
    println!("config: requested {}Hz, {} ch", cfg.sample_rate, cfg.channels);

    let mut cap = AudioCapture::microphone(cfg)?;
    println!("opened default input device");

    let mut vad = VadProcessor::new(0.001, 300); // threshold 0.001 (sensitive), 300ms silence = end
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut total_samples = 0usize;
    let mut total_chunks = 0usize;
    let mut max_rms = 0f32;
    let mut speech_chunks = 0usize;
    let mut silence_chunks = 0usize;
    let mut speech_start_emitted = false;

    while std::time::Instant::now() < deadline {
        match cap.receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => {
                total_chunks += 1;
                total_samples += chunk.len();
                let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
                if rms > max_rms {
                    max_rms = rms;
                }

                // Run VAD on the captured chunk
                let frame = qmeetily_lib::audio::mixer::MixedFrame {
                    samples: chunk,
                    timestamp_ms: 0,
                };
                match vad.process(&frame) {
                    VadEvent::SpeechStart => {
                        println!("VAD: SpeechStart detected");
                        speech_start_emitted = true;
                    }
                    VadEvent::SpeechEnd => {
                        println!("VAD: SpeechEnd detected");
                    }
                    VadEvent::Speech { .. } => speech_chunks += 1,
                    VadEvent::Silence => silence_chunks += 1,
                }
            }
            Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
                // no data yet
            }
            Err(e) => return Err(Box::new(e)),
        }
    }

    let seconds = 3.0;
    let samples_per_sec = total_samples as f32 / seconds;
    println!();
    println!("--- capture stats ---");
    println!("captured {} samples in {} chunks over {} seconds", total_samples, total_chunks, seconds);
    println!("throughput: {} samples/sec", samples_per_sec);
    println!("max RMS: {} (0.0=silence, 1.0=clipping)", max_rms);
    println!();
    println!("--- VAD stats ---");
    println!("SpeechStart emitted: {}", speech_start_emitted);
    println!("Speech chunks: {} / Silence chunks: {}", speech_chunks, silence_chunks);

    if total_samples == 0 {
        eprintln!("[FAIL] no samples captured");
        std::process::exit(1);
    }
    if samples_per_sec < 8000.0 {
        eprintln!("[WARN] low throughput");
    }

    println!("\n[OK] audio capture + VAD end-to-end smoke test passed");
    Ok(())
}
