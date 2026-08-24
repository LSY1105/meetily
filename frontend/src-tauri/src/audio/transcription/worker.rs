// audio/transcription/worker.rs
//
// Parallel transcription worker pool and chunk processing logic.

use super::engine::TranscriptionEngine;
use super::provider::TranscriptionError;
use crate::audio::AudioChunk;
use crate::sherpa_engine::SherpaEngine;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Runtime};

// Sequence counter for transcript updates
static SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);

// Speech detection flag - reset per recording session
static SPEECH_DETECTED_EMITTED: AtomicBool = AtomicBool::new(false);

/// Reset the speech detected flag for a new recording session
pub fn reset_speech_detected_flag() {
    SPEECH_DETECTED_EMITTED.store(false, Ordering::SeqCst);
    info!("🔍 SPEECH_DETECTED_EMITTED reset to: {}", SPEECH_DETECTED_EMITTED.load(Ordering::SeqCst));
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum SentenceStatus {
    /// `bg=1` — a new sentence is starting. `text` is empty or a
    /// placeholder; the frontend renders this as the streaming/in-
    /// progress grey-italic row.
    Begin,
    /// `rst=mid` — provisional hypothesis for the current sentence.
    /// Frontend can ignore this by default; if shown, it is grey/italic
    /// and updated in place by `sentence_id`.
    Mid,
    /// `rst=full` + `ed=1` — sentence is complete. `text` is the final
    /// hypothesis for `sentence_id`; frontend renders this in normal
    /// dark text.
    Full,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscriptUpdate {
    pub text: String,
    pub timestamp: String, // Wall-clock time for reference (e.g., "14:30:05")
    pub source: String,
    pub sequence_id: u64,
    pub chunk_start_time: f64, // Legacy field, kept for compatibility
    pub is_partial: bool,
    pub confidence: f32,
    // NEW: Recording-relative timestamps for playback sync
    pub audio_start_time: f64, // Seconds from recording start (e.g., 125.3)
    pub audio_end_time: f64,   // Seconds from recording start (e.g., 128.6)
    pub duration: f64,          // Segment duration in seconds (e.g., 3.3)
    // PR-44a: realtime speaker hint derived from the VAD segment. Persisted
    // on the segment only after offline re-clustering (PR-44b); until then
    // the value is advisory and the frontend renders it with a badge.
    #[serde(skip_serializing_if = "Option::is_none", rename = "transientSpeaker")]
    pub transient_speaker: Option<String>,
    // ponytail: iFlytek-style sentence protocol. `sentence_id` is the
    // stable key the frontend uses to dedup/merge messages for one
    // sentence (Begin, Mid*, Full). `sentence_status` carries the role
    // of this message in the sentence lifecycle. See comment on
    // SentenceStatus above.
    #[serde(rename = "sentenceId")]
    pub sentence_id: u32,
    #[serde(rename = "sentenceStatus")]
    pub sentence_status: SentenceStatus,
}

// NOTE: get_transcript_history and get_recording_meeting_name functions
// have been moved to recording_commands.rs where they have access to RECORDING_MANAGER

/// Optimized parallel transcription task ensuring ZERO chunk loss
pub fn start_transcription_task<R: Runtime>(
    app: AppHandle<R>,
    transcription_receiver: tokio::sync::mpsc::UnboundedReceiver<AudioChunk>,
    initial_prompt: Option<String>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("🚀 Starting optimized parallel transcription task - guaranteeing zero chunk loss");

        // Initialize transcription engine (Whisper or Parakeet based on config)
        let transcription_engine = match super::engine::get_or_init_transcription_engine(&app).await {
            Ok(engine) => engine,
            Err(e) => {
                error!("Failed to initialize transcription engine: {}", e);
                let _ = app.emit("transcription-error", serde_json::json!({
                    "error": e,
                    "userMessage": "Recording failed: Unable to initialize speech recognition. Please check your model settings.",
                    "actionable": true
                }));
                return;
            }
        };

        // Create parallel workers for faster processing while preserving ALL chunks
        const NUM_WORKERS: usize = 1; // Serial processing ensures transcripts emit in chronological order
        let (work_sender, work_receiver) = tokio::sync::mpsc::unbounded_channel::<AudioChunk>();
        let work_receiver = Arc::new(tokio::sync::Mutex::new(work_receiver));

        // Track completion: AtomicU64 for chunks queued, AtomicU64 for chunks completed
        let chunks_queued = Arc::new(AtomicU64::new(0));
        let chunks_completed = Arc::new(AtomicU64::new(0));
        let input_finished = Arc::new(AtomicBool::new(false));

        info!("📊 Starting {} transcription worker{} (serial mode for ordered emission)", NUM_WORKERS, if NUM_WORKERS == 1 { "" } else { "s" });

        // Spawn worker tasks
        let mut worker_handles = Vec::new();
        for worker_id in 0..NUM_WORKERS {
            let engine_clone = match &transcription_engine {
                TranscriptionEngine::Whisper(e) => TranscriptionEngine::Whisper(e.clone()),
                TranscriptionEngine::Parakeet(e) => TranscriptionEngine::Parakeet(e.clone()),
                TranscriptionEngine::Sherpa(e) => TranscriptionEngine::Sherpa(e.clone()),
                TranscriptionEngine::Provider(p) => TranscriptionEngine::Provider(p.clone()),
            };
            let app_clone = app.clone();
            let work_receiver_clone = work_receiver.clone();
            let chunks_completed_clone = chunks_completed.clone();
            let input_finished_clone = input_finished.clone();
            let chunks_queued_clone = chunks_queued.clone();
            let initial_prompt_clone = initial_prompt.clone();

            let worker_handle = tokio::spawn(async move {
                info!("👷 Worker {} started", worker_id);

                // PRE-VALIDATE model state to avoid repeated async calls per chunk
                let initial_model_loaded = engine_clone.is_model_loaded().await;
                let current_model = engine_clone
                    .get_current_model()
                    .await
                    .unwrap_or_else(|| "unknown".to_string());

                let engine_name = engine_clone.provider_name();

                if initial_model_loaded {
                    info!(
                        "✅ Worker {} pre-validation: {} model '{}' is loaded and ready",
                        worker_id, engine_name, current_model
                    );
                } else {
                    warn!("⚠️ Worker {} pre-validation: {} model not loaded - chunks may be skipped", worker_id, engine_name);
                }

                loop {
                    // Try to get a chunk to process
                    let chunk = {
                        let mut receiver = work_receiver_clone.lock().await;
                        receiver.recv().await
                    };

                    match chunk {
                        Some(chunk) => {
                            // PERFORMANCE OPTIMIZATION: Reduce logging in hot path
                            // Only log every 10th chunk per worker to reduce I/O overhead
                            let should_log_this_chunk = chunk.chunk_id % 10 == 0;

                            if should_log_this_chunk {
                                info!(
                                    "👷 Worker {} processing chunk {} with {} samples",
                                    worker_id,
                                    chunk.chunk_id,
                                    chunk.data.len()
                                );
                            }

                            // Check if model is still loaded before processing
                            if !engine_clone.is_model_loaded().await {
                                warn!("⚠️ Worker {}: Model unloaded, but continuing to preserve chunk {}", worker_id, chunk.chunk_id);
                                // Still count as completed even if we can't process
                                chunks_completed_clone.fetch_add(1, Ordering::SeqCst);
                                continue;
                            }

                            let chunk_timestamp = chunk.timestamp;
                            let chunk_duration = chunk.data.len() as f64 / chunk.sample_rate as f64;
                            let chunk_sample_rate = chunk.sample_rate;
                            let chunk_data = chunk.data.clone();

                            // Transcribe with provider-agnostic approach
                            match transcribe_chunk_with_provider(
                                &engine_clone,
                                chunk,
                                &app_clone,
                                initial_prompt_clone.clone(),
                            )
                            .await
                            {
                                Ok((transcript, confidence_opt, is_partial)) => {
                                    // Provider-aware confidence threshold
                                    let confidence_threshold = match &engine_clone {
                                        TranscriptionEngine::Whisper(_) | TranscriptionEngine::Provider(_) => 0.3,
                                        TranscriptionEngine::Parakeet(_) => 0.0, // Parakeet has no confidence, accept all
                                        TranscriptionEngine::Sherpa(_) => 0.0, // Sherpa has no confidence, accept all
                                    };

                                    let confidence_str = match confidence_opt {
                                        Some(c) => format!("{:.2}", c),
                                        None => "N/A".to_string(),
                                    };

                                    info!("🔍 Worker {} transcription result: text='{}', confidence={}, partial={}, threshold={:.2}",
                                          worker_id, transcript, confidence_str, is_partial, confidence_threshold);

                                    // Check confidence threshold (or accept if no confidence provided)
                                    let meets_threshold = confidence_opt.map_or(true, |c| c >= confidence_threshold);

                                    if !transcript.trim().is_empty() && meets_threshold {
                                        // PERFORMANCE: Only log transcription results, not every processing step
                                        info!("✅ Worker {} transcribed: {} (confidence: {}, partial: {})",
                                              worker_id, transcript, confidence_str, is_partial);

                                        // Emit speech-detected event for frontend UX (only on first detection per session)
                                        // This is lightweight and provides better user feedback
                                        let current_flag = SPEECH_DETECTED_EMITTED.load(Ordering::SeqCst);
                                        info!("🔍 Checking speech-detected flag: current={}, will_emit={}", current_flag, !current_flag);

                                        if !current_flag {
                                            SPEECH_DETECTED_EMITTED.store(true, Ordering::SeqCst);
                                            match app_clone.emit("speech-detected", serde_json::json!({
                                                "message": "Speech activity detected"
                                            })) {
                                                Ok(_) => info!("🎤 ✅ First speech detected - successfully emitted speech-detected event"),
                                                Err(e) => error!("🎤 ❌ Failed to emit speech-detected event: {}", e),
                                            }
                                        } else {
                                            info!("🔍 Speech already detected in this session, not re-emitting");
                                        }

                                        // Generate sequence ID and calculate timestamps FIRST
                                        let sequence_id = SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
                                        let audio_start_time = chunk_timestamp; // Already in seconds from recording start
                                        let audio_end_time = chunk_timestamp + chunk_duration;

                                        // Save structured transcript segment to recording manager (only final results)
                                        // Save ALL segments (partial and final) to ensure complete JSON
                                        // Create structured segment with full timestamp data
                                        // NOTE: This is now handled via the transcript-update event emission below
                                        // The recording_commands module listens to these events and saves them
                                        // This decouples the transcription worker from direct RECORDING_MANAGER access

                                        // Emit transcript update with NEW recording-relative timestamps

                                        // PR-44a: realtime speaker hint. Failures are non-fatal
                                        // (transient_speaker simply stays None) so transcription never blocks.
                                        let transient_speaker: Option<String> = {
                                            let buf = crate::audio::recording_commands::current_diarization_buffer();
                                            if crate::diarization::embedding::push_window(
                                                buf.as_ref(),
                                                &chunk_data,
                                                chunk_sample_rate,
                                                chunk_timestamp,
                                                chunk_timestamp + chunk_duration,
                                            ) {
                                                Some("Speaker ?".to_string())
                                            } else {
                                                None
                                            }
                                        };

                                        let update = TranscriptUpdate {
                                            transient_speaker,
                                            text: transcript,
                                            timestamp: format_current_timestamp(), // Wall-clock for reference
                                            source: "Audio".to_string(),
                                            sequence_id,
                                            chunk_start_time: chunk_timestamp, // Legacy compatibility
                                            is_partial,
                                            confidence: confidence_opt.unwrap_or(0.85), // Default for providers without confidence
                                            // NEW: Recording-relative timestamps for sync
                                            audio_start_time,
                                            audio_end_time,
                                            duration: chunk_duration,
                                            // ponytail: non-streaming engines
                                            // (Whisper/Parakeet) emit full
                                            // sentences directly, not
                                            // Begin/Mid/Full lifecycle.
                                            // Use sequence_id as the
                                            // sentence_id so the
                                            // frontend dedups the same
                                            // way it does for streaming.
                                            sentence_id: sequence_id as u32,
                                            sentence_status: SentenceStatus::Full,
                                        };

                                        if let Err(e) = app_clone.emit("transcript-update", &update)
                                        {
                                            error!(
                                                "Worker {}: Failed to emit transcript update: {}",
                                                worker_id, e
                                            );
                                        }
                                        // PERFORMANCE: Removed verbose logging of every emission
                                    } else if !transcript.trim().is_empty() && should_log_this_chunk
                                    {
                                        // PERFORMANCE: Only log low-confidence results occasionally
                                        if let Some(c) = confidence_opt {
                                            info!("Worker {} low-confidence transcription (confidence: {:.2}), skipping", worker_id, c);
                                        }
                                    }
                                }
                                Err(e) => {
                                    // Improved error handling with specific cases
                                    match e {
                                        TranscriptionError::AudioTooShort { .. } => {
                                            // Skip silently, this is expected for very short chunks
                                            info!("Worker {}: {}", worker_id, e);
                                            chunks_completed_clone.fetch_add(1, Ordering::SeqCst);
                                            continue;
                                        }
                                        TranscriptionError::ModelNotLoaded => {
                                            warn!("Worker {}: Model unloaded during transcription", worker_id);
                                            chunks_completed_clone.fetch_add(1, Ordering::SeqCst);
                                            continue;
                                        }
                                        _ => {
                                            warn!("Worker {}: Transcription failed: {}", worker_id, e);
                                            let _ = app_clone.emit("transcription-warning", e.to_string());
                                        }
                                    }
                                }
                            }

                            // Mark chunk as completed
                            let completed =
                                chunks_completed_clone.fetch_add(1, Ordering::SeqCst) + 1;
                            let queued = chunks_queued_clone.load(Ordering::SeqCst);

                            // PERFORMANCE: Only log progress every 5th chunk to reduce I/O overhead
                            if completed % 5 == 0 || should_log_this_chunk {
                                info!(
                                    "Worker {}: Progress {}/{} chunks ({:.1}%)",
                                    worker_id,
                                    completed,
                                    queued,
                                    (completed as f64 / queued.max(1) as f64 * 100.0)
                                );
                            }

                            // Emit progress event for frontend
                            let progress_percentage = if queued > 0 {
                                (completed as f64 / queued as f64 * 100.0) as u32
                            } else {
                                100
                            };

                            let _ = app_clone.emit("transcription-progress", serde_json::json!({
                                "worker_id": worker_id,
                                "chunks_completed": completed,
                                "chunks_queued": queued,
                                "progress_percentage": progress_percentage,
                                "message": format!("Worker {} processing... ({}/{})", worker_id, completed, queued)
                            }));
                        }
                        None => {
                            // No more chunks available
                            if input_finished_clone.load(Ordering::SeqCst) {
                                // Double-check that all queued chunks are actually completed
                                let final_queued = chunks_queued_clone.load(Ordering::SeqCst);
                                let final_completed = chunks_completed_clone.load(Ordering::SeqCst);

                                if final_completed >= final_queued {
                                    info!(
                                        "👷 Worker {} finishing - all {}/{} chunks processed",
                                        worker_id, final_completed, final_queued
                                    );
                                    break;
                                } else {
                                    warn!("👷 Worker {} detected potential chunk loss: {}/{} completed, waiting...", worker_id, final_completed, final_queued);
                                    // AGGRESSIVE POLLING: Reduced from 50ms to 5ms for faster chunk detection during shutdown
                                    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                                }
                            } else {
                                // AGGRESSIVE POLLING: Reduced from 10ms to 1ms for faster response during shutdown
                                tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                            }
                        }
                    }
                }

                info!("👷 Worker {} completed", worker_id);
            });

            worker_handles.push(worker_handle);
        }

        // Main dispatcher: receive chunks and distribute to workers
        let mut receiver = transcription_receiver;
        while let Some(chunk) = receiver.recv().await {
            let queued = chunks_queued.fetch_add(1, Ordering::SeqCst) + 1;
            info!(
                "📥 Dispatching chunk {} to workers (total queued: {})",
                chunk.chunk_id, queued
            );

            if let Err(_) = work_sender.send(chunk) {
                error!("❌ Failed to send chunk to workers - this should not happen!");
                break;
            }
        }

        // Signal that input is finished
        input_finished.store(true, Ordering::SeqCst);
        drop(work_sender); // Close the channel to signal workers

        let total_chunks_queued = chunks_queued.load(Ordering::SeqCst);
        info!("📭 Input finished with {} total chunks queued. Waiting for all {} workers to complete...",
              total_chunks_queued, NUM_WORKERS);

        // Emit final chunk count to frontend
        let _ = app.emit("transcription-queue-complete", serde_json::json!({
            "total_chunks": total_chunks_queued,
            "message": format!("{} chunks queued for processing - waiting for completion", total_chunks_queued)
        }));

        // Wait for all workers to complete
        for (worker_id, handle) in worker_handles.into_iter().enumerate() {
            if let Err(e) = handle.await {
                error!("❌ Worker {} panicked: {:?}", worker_id, e);
            } else {
                info!("✅ Worker {} completed successfully", worker_id);
            }
        }

        // Final verification with retry logic to catch any stragglers
        let mut verification_attempts = 0;
        const MAX_VERIFICATION_ATTEMPTS: u32 = 10;

        loop {
            let final_queued = chunks_queued.load(Ordering::SeqCst);
            let final_completed = chunks_completed.load(Ordering::SeqCst);

            if final_queued == final_completed {
                info!(
                    "🎉 ALL {} chunks processed successfully - ZERO chunks lost!",
                    final_completed
                );
                break;
            } else if verification_attempts < MAX_VERIFICATION_ATTEMPTS {
                verification_attempts += 1;
                warn!("⚠️ Chunk count mismatch (attempt {}): {} queued, {} completed - waiting for stragglers...",
                     verification_attempts, final_queued, final_completed);

                // Wait a bit for any remaining chunks to be processed
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            } else {
                error!(
                    "❌ CRITICAL: After {} attempts, chunk loss detected: {} queued, {} completed",
                    MAX_VERIFICATION_ATTEMPTS, final_queued, final_completed
                );

                // Emit critical error event
                let _ = app.emit(
                    "transcript-chunk-loss-detected",
                    serde_json::json!({
                        "chunks_queued": final_queued,
                        "chunks_completed": final_completed,
                        "chunks_lost": final_queued - final_completed,
                        "message": "Some transcript chunks may have been lost during shutdown"
                    }),
                );
                break;
            }
        }

        info!("✅ Parallel transcription task completed - all workers finished, ready for model unload");
    })
}

/// Transcribe audio chunk using the appropriate provider (Whisper, Parakeet, or trait-based)
/// Returns: (text, confidence Option, is_partial)
async fn transcribe_chunk_with_provider<R: Runtime>(
    engine: &TranscriptionEngine,
    chunk: AudioChunk,
    app: &AppHandle<R>,
    initial_prompt: Option<String>,
) -> std::result::Result<(String, Option<f32>, bool), TranscriptionError> {
    // Convert to 16kHz mono for transcription
    let transcription_data = if chunk.sample_rate != 16000 {
        crate::audio::audio_processing::resample_audio(&chunk.data, chunk.sample_rate, 16000)
    } else {
        chunk.data
    };

    // Skip VAD processing here since the pipeline already extracted speech using VAD
    let speech_samples = transcription_data;

    // Check for empty samples - improved error handling
    if speech_samples.is_empty() {
        warn!(
            "Audio chunk {} is empty, skipping transcription",
            chunk.chunk_id
        );
        return Err(TranscriptionError::AudioTooShort {
            samples: 0,
            minimum: 1600, // 100ms at 16kHz
        });
    }

    // Calculate energy for logging/monitoring only
    let energy: f32 =
        speech_samples.iter().map(|&x| x * x).sum::<f32>() / speech_samples.len() as f32;
    info!(
        "Processing speech audio chunk {} with {} samples (energy: {:.6})",
        chunk.chunk_id,
        speech_samples.len(),
        energy
    );

    // Transcribe using the appropriate engine (with improved error handling)
    match engine {
        TranscriptionEngine::Whisper(whisper_engine) => {
            // Get language preference from global state
            let language = crate::get_language_preference_internal();

            match whisper_engine
                .transcribe_audio_with_confidence(speech_samples, language, initial_prompt)
                .await
            {
                Ok((text, confidence, is_partial)) => {
                    let cleaned_text = text.trim().to_string();
                    if cleaned_text.is_empty() {
                        return Ok((String::new(), Some(confidence), is_partial));
                    }

                    info!(
                        "Whisper transcription complete for chunk {}: '{}' (confidence: {:.2}, partial: {})",
                        chunk.chunk_id, cleaned_text, confidence, is_partial
                    );

                    Ok((cleaned_text, Some(confidence), is_partial))
                }
                Err(e) => {
                    error!(
                        "Whisper transcription failed for chunk {}: {}",
                        chunk.chunk_id, e
                    );

                    let transcription_error = TranscriptionError::EngineFailed(e.to_string());
                    let _ = app.emit(
                        "transcription-error",
                        &serde_json::json!({
                            "error": transcription_error.to_string(),
                            "userMessage": format!("Transcription failed: {}", transcription_error),
                            "actionable": false
                        }),
                    );

                    Err(transcription_error)
                }
            }
        }
        TranscriptionEngine::Parakeet(parakeet_engine) => {
            match parakeet_engine.transcribe_audio(speech_samples).await {
                Ok(text) => {
                    let cleaned_text = text.trim().to_string();
                    if cleaned_text.is_empty() {
                        return Ok((String::new(), None, false));
                    }

                    info!(
                        "Parakeet transcription complete for chunk {}: '{}'",
                        chunk.chunk_id, cleaned_text
                    );

                    // Parakeet doesn't provide confidence or partial results
                    Ok((cleaned_text, None, false))
                }
                Err(e) => {
                    error!(
                        "Parakeet transcription failed for chunk {}: {}",
                        chunk.chunk_id, e
                    );

                    let transcription_error = TranscriptionError::EngineFailed(e.to_string());
                    let _ = app.emit(
                        "transcription-error",
                        &serde_json::json!({
                            "error": transcription_error.to_string(),
                            "userMessage": format!("Transcription failed: {}", transcription_error),
                            "actionable": false
                        }),
                    );

                    Err(transcription_error)
                }
            }
        }
        TranscriptionEngine::Sherpa(_sherpa_engine) => {
            // ponytail: the streaming task (recording_commands.rs →
            // start_streaming_task) is the *only* source of
            // transcript-update events for the live UI. It uses the
            // independent `current_streaming` slot and feeds raw
            // 16 kHz audio every 100 ms, with its own 1.5 s
            // silence-based endpoint policy. The VAD-batched path
            // running here used to drive the UI directly, but every
            // VAD chunk triggered a `reset()` on the `current` slot,
            // which fragmented the hypothesis into 1-3-character
            // outputs (the model needs ≥3 s of context to decode
            // anything meaningful). When the streaming task is active
            // we just drop the VAD chunk on the floor — returning an
            // empty transcript makes the worker skip emitting
            // transcript-update, so the streaming output is the only
            // thing the UI renders.
            Ok((String::new(), None, false))
        }
        TranscriptionEngine::Provider(provider) => {
            // NEW: Trait-based provider (clean, unified interface)
            let language = crate::get_language_preference_internal();

            match provider.transcribe(speech_samples, language).await {
                Ok(result) => {
                    let cleaned_text = result.text.trim().to_string();
                    if cleaned_text.is_empty() {
                        return Ok((String::new(), result.confidence, result.is_partial));
                    }

                    let confidence_str = match result.confidence {
                        Some(c) => format!("confidence: {:.2}", c),
                        None => "no confidence".to_string(),
                    };

                    info!(
                        "{} transcription complete for chunk {}: '{}' ({}, partial: {})",
                        provider.provider_name(),
                        chunk.chunk_id,
                        cleaned_text,
                        confidence_str,
                        result.is_partial
                    );

                    Ok((cleaned_text, result.confidence, result.is_partial))
                }
                Err(e) => {
                    error!(
                        "{} transcription failed for chunk {}: {}",
                        provider.provider_name(),
                        chunk.chunk_id,
                        e
                    );

                    let _ = app.emit(
                        "transcription-error",
                        &serde_json::json!({
                            "error": e.to_string(),
                            "userMessage": format!("Transcription failed: {}", e),
                            "actionable": false
                        }),
                    );

                    Err(e)
                }
            }
        }
    }
}

/// Format current timestamp (wall-clock time)
fn format_current_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();

    let hours = (now.as_secs() / 3600) % 24;
    let minutes = (now.as_secs() / 60) % 60;
    let seconds = now.as_secs() % 60;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

/// Format recording-relative time as [MM:SS]
#[allow(dead_code)]
fn format_recording_time(seconds: f64) -> String {
    let total_seconds = seconds.floor() as u64;
    let minutes = total_seconds / 60;
    let secs = total_seconds % 60;

    format!("[{:02}:{:02}]", minutes, secs)
}

/// ponytail: feed raw 16kHz mono audio into the sherpa-onnx
/// `current_streaming` slot every 100ms. The slot is fully
/// independent from the VAD-batched path's `current` slot, so the
/// VAD `reset()` at endpoint does not wipe the streaming hypothesis.
/// The worker emits a `transcript-update` with `is_partial=true` for
/// every decoded chunk. The frontend's existing partial-dedup logic
/// (the only-keep-latest-partial rule in `processBufferedTranscripts`)
/// collapses the stream of partials into a single in-place row whose
/// text grows with every chunk.
///
/// The streaming task is **completely independent** of the VAD
/// segment task — both run in parallel. The VAD task still commits
/// `is_partial=false` segments on endpoint for the saved history;
/// the streaming task never commits (no endpoint-driven reset of the
/// streaming slot), it just keeps emitting the latest partial. When
/// the session ends, `streaming_reset` is called once to clear state.
pub fn start_streaming_task<R: Runtime>(
    app: AppHandle<R>,
    engine: Arc<SherpaEngine>,
    mut raw_receiver: tokio::sync::mpsc::UnboundedReceiver<AudioChunk>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("🛰️ Sherpa streaming task started — feeding raw 16k audio every 100ms");

        // ponytail: iFlytek append-only. One sentence in flight at a
        // time (sherpa's OnlineStream is single-channel). Begin is
        // implicit — the first non-empty hypothesis after
        // silence/full becomes the new sentence and the frontend
        // opens a fresh row keyed on the new `sentence_id`. Full
        // emits the cumulative text. Mid emits the suffix delta
        // against the last emission for that `sentence_id` so the
        // front text never reflows. If sherpa rewrites earlier
        // tokens (`strip_prefix` fails), we treat it as a
        // correction: emit Full with the corrected text on the
        // current `sn`, then open a new sentence on the next `sn`.
        let mut last_emitted_for_sn: HashMap<u32, String> = HashMap::new();
        // ponytail: tracks the last raw sherpa hypothesis per sn.
        // The Mid path needs two diff bases:
        //   - raw_prev  : for dedupe ("did sherpa change anything?")
        //   - emitted_prev : for strip_prefix on the punctuated
        //     string (the iFlytek invariant).
        // Splitting them lets us keep the 250ms throttle and
        // dedupe against the raw stream while still diffing
        // punctuated output against the previously-emitted
        // punctuated string.
        let mut last_raw_for_sn: HashMap<u32, String> = HashMap::new();
        let mut last_mid_ts: Option<Instant> = None;
        // ponytail: punctuation helper. If the punctuator model is
        // loaded, returns the same characters with `，。？！` inserted
        // between them (never inside a word span). The output
        // preserves input order so the iFlytek strip_prefix
        // invariant keeps holding — we just store and diff against
        // the punctuated text instead of the raw text. When the
        // punctuator is not loaded, the helper falls back to
        // returning the raw text unchanged (frontend rule-based
        // inserter takes over downstream).
        // ponytail: bound local fn so we don't pay the async
        // closure type ambiguity (Rust 1.77 has no async closures
        // in stable).
        async fn try_punctuate(
            engine: &Arc<SherpaEngine>,
            raw: String,
        ) -> String {
            engine.punctuate(&raw).await.unwrap_or(raw)
        }
        // ponytail: per-utterance id so the frontend React row stays
        // mounted for the whole sentence. The streaming task
        // allocates a fresh id when the model reports endpoint, then
        // resets the streaming slot so the *next* utterance starts
        // from zero. (The slot is independent of the VAD slot, so
        // the VAD path's reset does not interfere.)
        let mut utterance_seq_base: u64 = 0;
        let mut utterance_seq_allocated: bool = false;

        // ponytail: own endpoint policy. Sherpa's internal
        // rule1/2/3 trailing-silence threshold never fires on raw
        // 100ms chunks because most chunks contain some non-zero
        // samples (room tone, mic noise). Track how many consecutive
        // chunks have been effectively silent (RMS < SILENCE_RMS)
        // and commit when that crosses `SILENT_CHUNKS_FOR_ENDPOINT`
        // (~1.5s of silence). The streaming task is the only thing
        // that drives the UI now — the VAD path is also running in
        // parallel for saved-history, but it can lag without affecting
        // the live transcript.
        // ponytail: empirical noise floor of a quiet mic (laptop fan + mic
        // self-noise after AEC) sits around 0.008-0.02 RMS; the previous
        // 0.005 threshold never tripped even in silence, so the
        // silence-based endpoint never fired. 0.015 still sits well
        // below normal speech (0.05-0.2) and below music (~0.1+).
        // ponytail: iFlytek-style sentence endpoint. 12 chunks × 100 ms =
        // 1.2 s of continuous audio below `SILENCE_RMS` triggers Full
        // (sentence commit). The previous 2.0 s / 3.5 s thresholds
        // were too conservative — sherpa Zip waits for trailing
        // silence that doesn't come in continuous Chinese monologue,
        // so the user reports "nothing on screen". 1.2 s tolerates
        // the natural 0.5-0.8 s Chinese clause pause (just barely)
        // but commits on a real speaker pause. Matches
        // sherpa rule1=1.0s within a small tolerance. The frontend
        // coalesce (TranscriptContext.tsx, COALESCE_GAP_SEC=8) still
        // stitches nearby short rows back together so the persisted
        // meeting transcript stays readable.
        const SILENCE_RMS: f32 = 0.015;
        const SILENT_CHUNKS_FOR_ENDPOINT: u32 = 12;
        // ponytail: Mid emit cadence. The previous 250 ms throttle
        // matched an old "iFlytek console" cadence, but the user
        // explicitly asked for character-level streaming, not a
        // 250-ms heartbeat. sherpa-onnx Zip emits a new hypothesis
        // roughly every 100 ms when speech is active, so we throttle
        // Mid emits to 50 ms — same as the audio chunk rate — and
        // let the frontend typewriter animate the in-place reveal
        // from there. This gives the user a real-time character
        // stream rather than a periodic burst-and-pause cadence.
        const MIN_MID_INTERVAL_MS: u64 = 50;
        let mut silent_chunks: u32 = 0;
        // ponytail: max-utterance safety net. Even when the user is
        // talking continuously (silent_chunks stays at 0), we commit
        // the current hypothesis after this many 100-ms chunks so the
        // UI doesn't hold a stale single-line partial for the entire
        // long sentence. Matches sherpa-onnx rule3_min_utterance_length
        // (engine.rs). If sherpa's internal endpoint already fired,
        // we reset earlier — this is just the backstop.
        // ponytail: reverted to 5 s = 50 chunks. With a 30 s
        // backstop, sherpa Zip never reaches its internal endpoint
        // on typical Chinese speech (it waits for trailing silence
        // that doesn't come in continuous monologue), so the worker
        // holds the entire utterance as one row and the user
        // reports "nothing on screen" until recording stops. 5 s
        // forces a periodic commit that re-seeds the streaming slot
        // and gives the frontend something to display.
        const MAX_CHUNKS_BETWEEN_COMMITS: u32 = 50;

        let mut log_counter: u64 = 0;
        let mut chunks_since_last_commit: u32 = 0;
        while let Some(chunk) = raw_receiver.recv().await {
            let samples_16k: Vec<f32> = if chunk.sample_rate == 16000 {
                chunk.data
            } else {
                crate::audio::audio_processing::resample_audio(
                    &chunk.data,
                    chunk.sample_rate,
                    16000,
                )
            };

            // ponytail: compute RMS *before* handing the samples to
            // sherpa — we don't want to call resample_audio twice.
            let rms = if samples_16k.is_empty() {
                0.0
            } else {
                let sum: f32 = samples_16k.iter().map(|s| s * s).sum();
                (sum / samples_16k.len() as f32).sqrt()
            };
            if rms < SILENCE_RMS {
                silent_chunks = silent_chunks.saturating_add(1);
            } else {
                silent_chunks = 0;
            }
            // ponytail: temporary diagnostic — log RMS once a second
            // so we can see whether raw audio is reaching the
            // streaming task and whether it's mostly silence.
            log_counter += 1;
            if log_counter % 10 == 0 {
                info!(
                    "🛰️ streaming chunk {}: rms={:.4}, samples={}, silent_run={}",
                    log_counter, rms, samples_16k.len(), silent_chunks
                );
            }

            // ponytail: we trigger our own endpoint when 1.5s of
            // consecutive silence has passed AND we have a
            // hypothesis to commit. We commit the *cached* text
            // (the last non-empty hypothesis emitted for the
            // current sentence) rather than the empty text the
            // decoder returns on silence — sherpa tends to emit
            // `""` once silence goes on long enough, and we'd lose
            // the last few characters.
            let cached_for_current = last_emitted_for_sn
                .get(&(utterance_seq_base as u32))
                .cloned()
                .unwrap_or_default();
            let our_endpoint = silent_chunks >= SILENT_CHUNKS_FOR_ENDPOINT
                && !cached_for_current.is_empty()
                && utterance_seq_allocated;

            // ponytail: tick the commit-backstop counter and trigger an
            // endpoint after MAX_CHUNKS_BETWEEN_COMMITS chunks even if
            // sherpa never fires its own rule3. Matches the
            // rule3_min_utterance_length=6.0 backstop in engine.rs.
            chunks_since_last_commit = chunks_since_last_commit.saturating_add(1);
            let backstop_commit = chunks_since_last_commit >= MAX_CHUNKS_BETWEEN_COMMITS
                && utterance_seq_allocated
                && !cached_for_current.is_empty();

            match engine.accept_samples_for_streaming(samples_16k, 16000).await {
                Ok(result) => {
                    let text = result.text.trim();
                    let endpoint = result.is_endpoint || our_endpoint || backstop_commit;
                    if text.is_empty() && !endpoint {
                        continue;
                    }
                    if endpoint {
                        // ponytail: iFlytek Full on silence (or
                        // sherpa's internal endpoint). Emit the
                        // cumulative text for the current sentence
                        // id, reset sherpa so the next utterance
                        // starts clean, and arm the next sentence
                        // id (lazy — the Begin row is opened by the
                        // first non-empty hypothesis on the next
                        // tick, not pre-emitted here).
                        if !utterance_seq_allocated {
                            utterance_seq_base =
                                SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
                            utterance_seq_allocated = true;
                        }
                        let sequence_id = utterance_seq_base;
                        let sn = sequence_id as u32;
                        // ponytail: when sherpa returned an empty
                        // hypothesis at endpoint we fall back to
                        // the previously emitted cumulative text
                        // (cached_for_current). Otherwise we trust
                        // sherpa's latest hypothesis. We keep
                        // `cached_for_current` as a separate
                        // `String` (not consumed) so the
                        // strip_prefix check below can borrow it.
                        let emit_text_raw = if text.is_empty() {
                            cached_for_current.as_str().to_string()
                        } else {
                            text.to_string()
                        };
                        // ponytail: punctuation on the cumulative
                        // sentence before the Full commit. The
                        // punctuator runs once per sentence (cheap)
                        // and replaces the raw run-on string the
                        // user has been watching live with the
                        // punctuated final form. The Mid ticks
                        // already added tail characters to the
                        // frontend row, so this Final event just
                        // patches the tail (whatever new characters
                        // the latest partial added) — front text
                        // still doesn't move.
                        let emit_text =
                            try_punctuate(&engine, emit_text_raw.clone())
                                .await;
                        // ponytail: Full must extend the previously
                        // emitted punctuated string, never replace
                        // it whole — that would cause the front
                        // text to reflow on every endpoint. If the
                        // punctuator produced an output that does
                        // NOT start with the already-emitted tail,
                        // emit only the new suffix instead.
                        let tail = if !emit_text.is_empty()
                            && emit_text.starts_with(cached_for_current.as_str())
                        {
                            emit_text[cached_for_current.len()..]
                                .to_string()
                        } else {
                            // ponytail: invariant guard. Either the
                            // punctuator rewrote earlier characters
                            // (shouldn't happen for CT-Transformer)
                            // or cached_for_current is empty. Emit
                            // the full punctuated string; the
                            // frontend's row replacement on Full
                            // (per TranscriptContext.tsx merge
                            // logic) is acceptable here since the
                            // row was about to be locked into
                            // committed style anyway.
                            emit_text.clone()
                        };
                        let update = TranscriptUpdate {
                            text: tail.clone(),
                            timestamp: format_current_timestamp(),
                            source: "Audio".to_string(),
                            sequence_id,
                            chunk_start_time: chunk.timestamp,
                            is_partial: false,
                            confidence: 0.85,
                            audio_start_time: chunk.timestamp,
                            audio_end_time: chunk.timestamp + 0.1,
                            duration: 0.1,
                            transient_speaker: None,
                            sentence_id: sn,
                            sentence_status: SentenceStatus::Full,
                        };
                        if let Err(e) = app.emit("transcript-update", &update) {
                            warn!("streaming task: failed to emit full: {}", e);
                        }
                        // ponytail: store the *punctuated*
                        // cumulative so the next tick's strip_prefix
                        // check is against the same key we emit.
                        last_emitted_for_sn.insert(sn, emit_text);
                        last_raw_for_sn.remove(&sn);
                        if let Err(e) = engine.streaming_reset().await {
                            warn!("streaming task: streaming_reset after full failed: {}", e);
                        }
                        // ponytail: iFlytek-style sentence commit. Both
                        // sherpa's internal endpoint and our silence
                        // detector commit the current hypothesis and
                        // reset sherpa so the next utterance starts
                        // clean. We do NOT pre-allocate the next
                        // sentence id here — the next non-empty
                        // hypothesis below fetches a fresh id and
                        // emits Begin for it. This matches the
                        // iFlytek console: rows only appear when
                        // there is content for them.
                        //
                        // ponytail: the backstop path (rule3
                        // timeout) reuses the same sentence_id, so
                        // the next hypothesis tick continues the
                        // same row rather than allocating a new one.
                        // Only the silence-driven commit
                        // (`our_endpoint`) allocates a fresh id —
                        // that's where a real new sentence boundary
                        // is detected. We keep `last_emitted_for_sn`
                        // populated across the backstop reset so the
                        // Begin guard (next block) sees the slot is
                        // not empty and reuses `utterance_seq_base`.
                        let next = if backstop_commit && !our_endpoint {
                            utterance_seq_base
                        } else {
                            SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst)
                        };
                        utterance_seq_base = next;
                        utterance_seq_allocated = false;
                        if !backstop_commit || our_endpoint {
                            last_emitted_for_sn.remove(&(sn));
                            last_raw_for_sn.remove(&sn);
                        }
                        last_mid_ts = None;
                        silent_chunks = 0;
                        chunks_since_last_commit = 0;
                        continue;
                    }
                    // ponytail: iFlytek append-only Mid path. The
                    // current sherpa hypothesis may have grown
                    // (good — emit a delta) or may have rewritten
                    // earlier tokens (correction — close the current
                    // sentence and start a new one). Empty
                    // hypotheses are deduped against the last seen
                    // text; same-text ticks emit nothing.
                    if !utterance_seq_allocated {
                        // ponytail: reuse the current sentence_id
                        // when the streaming slot was just reset
                        // (backstop commit above) so the next
                        // hypothesis continues the same row in the
                        // UI rather than starting a brand-new row
                        // with its own timestamp. Only a fresh
                        // recording session or a real
                        // silence-driven reset above allocates a
                        // new id.
                        if utterance_seq_base == 0 && last_emitted_for_sn.is_empty() {
                            utterance_seq_base =
                                SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
                        }
                        utterance_seq_allocated = true;
                        let sn = utterance_seq_base as u32;
                        let begin = TranscriptUpdate {
                            text: String::new(),
                            timestamp: format_current_timestamp(),
                            source: "Audio".to_string(),
                            sequence_id: utterance_seq_base,
                            chunk_start_time: chunk.timestamp,
                            is_partial: true,
                            confidence: 0.0,
                            audio_start_time: chunk.timestamp,
                            audio_end_time: chunk.timestamp + 0.1,
                            duration: 0.1,
                            transient_speaker: None,
                            sentence_id: sn,
                            sentence_status: SentenceStatus::Begin,
                        };
                        if let Err(e) = app.emit("transcript-update", &begin) {
                            warn!("streaming task: failed to emit begin: {}", e);
                        }
                        // ponytail: temporary diagnostic for
                        // sentence_id allocation.
                        log::info!("🛰️ Begin emit sn={}", sn);
                        last_emitted_for_sn.insert(sn, String::new());
                        // ponytail: no raw hypothesis seen yet for
                        // this sn. Empty baseline so the next dedupe
                        // comparison accepts the first non-empty
                        // hypothesis without false-positive "same as
                        // previous" skip.
                        last_raw_for_sn
                            .insert(sn, String::new());
                        last_mid_ts = Some(Instant::now());
                    }
                    let sn = utterance_seq_base as u32;
                    let prev_raw = last_raw_for_sn
                        .get(&sn)
                        .cloned()
                        .unwrap_or_default();
                    let prev_emitted = last_emitted_for_sn
                        .get(&sn)
                        .cloned()
                        .unwrap_or_default();
                    // ponytail: dedupe identical ticks (sherpa
                    // sometimes emits the same hypothesis twice in
                    // a row before adding new tokens). We compare
                    // against the raw stream, not the punctuated
                    // output, so punctuation is the only effect of
                    // a re-emitted tick (and the punctuator is
                    // deterministic, so re-emit yields the same
                    // punctuated string).
                    if text == prev_raw {
                        continue;
                    }
                    // ponytail: throttle Mid cadence to ~4/s. If we
                    // are inside the throttle window, cache the new
                    // text so the next tick after the window emits
                    // the accumulated delta (no chars lost).
                    let now = Instant::now();
                    let elapsed_ok = match last_mid_ts {
                        Some(t) => now.duration_since(t).as_millis()
                            >= MIN_MID_INTERVAL_MS as u128,
                        None => true,
                    };
                    if !elapsed_ok {
                        last_raw_for_sn.insert(sn, text.to_string());
                        continue;
                    }
                    // ponytail: detect sherpa rewrite of earlier
                    // tokens. If the new hypothesis does NOT start
                    // with the previously RAW (non-punctuated)
                    // hypothesis, treat it as a correction: close
                    // the current sentence with the corrected text,
                    // then open a new sentence with the corrected
                    // text as its baseline.
                    if !text.starts_with(prev_raw.as_str()) {
                        // ponytail: correction closes the previous
                        // sentence with its punctuated cumulative
                        // text (so the user sees the final
                        // committed sentence with `，。？！`
                        // applied, not a raw run-on string). If
                        // nothing was emitted for this sn yet, skip
                        // the closure emit.
                        if !prev_emitted.is_empty() {
                            let corrected_update = TranscriptUpdate {
                                text: prev_emitted.clone(),
                                timestamp: format_current_timestamp(),
                                source: "Audio".to_string(),
                                sequence_id: utterance_seq_base,
                                chunk_start_time: chunk.timestamp,
                                is_partial: false,
                                confidence: 0.85,
                                audio_start_time: chunk.timestamp,
                                audio_end_time: chunk.timestamp + 0.1,
                                duration: 0.1,
                                transient_speaker: None,
                                sentence_id: sn,
                                sentence_status: SentenceStatus::Full,
                            };
                            if let Err(e) = app.emit(
                                "transcript-update",
                                &corrected_update,
                            ) {
                                warn!(
                                    "streaming task: failed to emit correction full: {}",
                                    e
                                );
                            }
                        }
                        let next =
                            SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
                        utterance_seq_base = next;
                        let new_sn = next as u32;
                        // ponytail: the next sn starts from the
                        // current raw hypothesis. Its `last_emitted`
                        // baseline is empty (Begin); the first Mid
                        // will pick the punctuated prefix from
                        // scratch and emit a delta from there.
                        let begin = TranscriptUpdate {
                            text: String::new(),
                            timestamp: format_current_timestamp(),
                            source: "Audio".to_string(),
                            sequence_id: next,
                            chunk_start_time: chunk.timestamp,
                            is_partial: true,
                            confidence: 0.0,
                            audio_start_time: chunk.timestamp,
                            audio_end_time: chunk.timestamp + 0.1,
                            duration: 0.1,
                            transient_speaker: None,
                            sentence_id: new_sn,
                            sentence_status: SentenceStatus::Begin,
                        };
                        if let Err(e) = app.emit("transcript-update", &begin) {
                            warn!(
                                "streaming task: failed to emit begin after correction: {}",
                                e
                            );
                        }
                        // ponytail: new sn starts at empty
                        // (Begin just fired). The next Mid will run
                        // the punctuator on the raw hypothesis and
                        // emit a delta from this empty baseline.
                        last_emitted_for_sn
                            .insert(new_sn, String::new());
                        last_raw_for_sn.insert(new_sn, text.to_string());
                        last_mid_ts = Some(now);
                        continue;
                    }
                    // ponytail: normal append path with punctuation. We diff against
                    // the previously *emitted* (and thus punctuated)
                    // string. The sherpa punctuator inserts
                    // `，。？！` between existing characters without
                    // ever deleting them, so
                    // `punctuated_now.starts_with(prev_emitted)`
                    // holds in practice. We slice on byte offsets
                    // directly — the prefix is verbatim character
                    // order, so byte boundaries line up.
                    let punctuated_now =
                        try_punctuate(&engine, text.to_string()).await;
                    if !punctuated_now.starts_with(prev_emitted.as_str()) {
                        // ponytail: invariant guard. If the
                        // punctuator produced an output that
                        // doesn't extend the previously emitted
                        // string (rare; should never happen for
                        // the CT-Transformer), skip this Mid and
                        // let the next tick retry once the
                        // hypothesis stabilises.
                        continue;
                    }
                    let tail: String =
                        punctuated_now[prev_emitted.len()..].to_string();
                    if tail.is_empty() {
                        last_emitted_for_sn.insert(sn, punctuated_now);
                        continue;
                    }
                    let update = TranscriptUpdate {
                        text: tail.clone(),
                        timestamp: format_current_timestamp(),
                        source: "Audio".to_string(),
                        sequence_id: utterance_seq_base,
                        chunk_start_time: chunk.timestamp,
                        is_partial: true,
                        confidence: 0.85,
                        audio_start_time: chunk.timestamp,
                        audio_end_time: chunk.timestamp + 0.1,
                        duration: 0.1,
                        transient_speaker: None,
                        sentence_id: sn,
                        sentence_status: SentenceStatus::Mid,
                    };
                    if let Err(e) = app.emit("transcript-update", &update) {
                        warn!("streaming task: failed to emit mid: {}", e);
                    }
                    // ponytail: temporary diagnostic to confirm
                    // sentence_id is reused on Mid emits.
                    log::info!(
                        "🛰️ Mid emit sn={}, text_len={}",
                        sn,
                        punctuated_now.len()
                    );
                    last_emitted_for_sn.insert(sn, punctuated_now);
                    last_raw_for_sn.insert(sn, text.to_string());
                    last_mid_ts = Some(now);
                    chunks_since_last_commit = 0;
                }
                Err(e) => {
                    warn!("streaming task: accept_samples failed: {}", e);
                }
            }
        }

        // Session end: clear the streaming slot so the next
        // recording starts fresh. (VAD-batched path handles its own
        // state in the existing stop path.)
        if let Err(e) = engine.streaming_reset().await {
            warn!("streaming task: final streaming_reset failed: {}", e);
        }

        info!("🛰️ Sherpa streaming task ended (raw receiver closed)");
    })
}
