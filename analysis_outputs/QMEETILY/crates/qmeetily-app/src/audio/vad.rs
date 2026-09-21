//! Voice Activity Detection — borrowed from meetily's VAD processor.
//!
//! v0.1: simple RMS-based detector (no model needed, ~0 CPU).
//! v0.2: Silero V5 ONNX integration for better accuracy on noisy audio.

use crate::audio::mixer::MixedFrame;

#[derive(Debug, Clone, PartialEq)]
pub enum VadEvent {
    SpeechStart,
    SpeechEnd,
    Speech { samples: Vec<f32> },
    Silence,
}

pub struct VadProcessor {
    threshold: f32,
    min_silence_ms: u32,
    in_speech: bool,
    silence_counter_ms: u32,
}

impl VadProcessor {
    pub fn new(threshold: f32, min_silence_ms: u32) -> Self {
        Self {
            threshold,
            min_silence_ms,
            in_speech: false,
            silence_counter_ms: 0,
        }
    }

    pub fn process(&mut self, frame: &MixedFrame) -> VadEvent {
        let rms = (frame.samples.iter().map(|s| s * s).sum::<f32>()
            / frame.samples.len().max(1) as f32)
            .sqrt();

        let is_speech = rms > self.threshold;

        if is_speech {
            self.silence_counter_ms = 0;
            if !self.in_speech {
                self.in_speech = true;
                return VadEvent::SpeechStart;
            }
            VadEvent::Speech {
                samples: frame.samples.clone(),
            }
        } else {
            self.silence_counter_ms += frame_duration_ms(frame);
            if self.in_speech && self.silence_counter_ms >= self.min_silence_ms {
                self.in_speech = false;
                self.silence_counter_ms = 0;
                return VadEvent::SpeechEnd;
            }
            if !self.in_speech {
                return VadEvent::Silence;
            }
            // still in speech but brief silence; emit accumulated
            VadEvent::Speech {
                samples: frame.samples.clone(),
            }
        }
    }
}

fn frame_duration_ms(frame: &MixedFrame) -> u32 {
    // Estimate from sample count; we don't carry sample_rate in MixedFrame yet.
    // 48 kHz is a reasonable default; for other rates the tolerance is fine for VAD.
    (frame.samples.len() as u32 * 1000) / 48_000
}
