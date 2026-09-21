//! Mic + system audio mixer.
//!
//! Both streams arrive at slightly different sample rates; in v0.1 we assume
//! both are already at 16 kHz mono (downstream resampling). When input rates
//! differ, system_audio.rs handles resampling before the mixer.

use crossbeam::channel::Receiver;

/// One mixed frame at 16 kHz mono, ready for VAD / ASR.
#[derive(Debug, Clone)]
pub struct MixedFrame {
    pub samples: Vec<f32>,
    pub timestamp_ms: u64,
}

pub struct AudioMixer {
    mic_rx: Receiver<Vec<f32>>,
    system_rx: Receiver<Vec<f32>>,
    sample_rate: u32,
    frame_size: usize,
    /// v0.1: simple average. v0.2: RMS-based ducking (borrowed from meetily's mixer).
    mode: MixMode,
}

#[derive(Debug, Clone, Copy)]
pub enum MixMode {
    /// Simple equal average.
    Average,
    /// RMS-based ducking: when system audio is loud, reduce mic gain.
    Ducking,
}

impl AudioMixer {
    pub fn new(
        mic_rx: Receiver<Vec<f32>>,
        system_rx: Receiver<Vec<f32>>,
        sample_rate: u32,
        frame_size: usize,
    ) -> Self {
        Self {
            mic_rx,
            system_rx,
            sample_rate,
            frame_size,
            mode: MixMode::Average,
        }
    }

    pub fn with_mode(mut self, mode: MixMode) -> Self {
        self.mode = mode;
        self
    }

    /// Pull one aligned frame. Blocks until both streams produce a frame.
    pub fn next_frame(&self, frame_count: u64) -> Option<MixedFrame> {
        let mic = self.mic_rx.recv().ok()?;
        let sys = self.system_rx.recv().ok()?;

        let mixed = match self.mode {
            MixMode::Average => mix_average(&mic, &sys),
            MixMode::Ducking => mix_ducking(&mic, &sys),
        };

        Some(MixedFrame {
            timestamp_ms: (frame_count * self.frame_size as u64 * 1000) / self.sample_rate as u64,
            samples: mixed,
        })
    }
}

fn mix_average(mic: &[f32], sys: &[f32]) -> Vec<f32> {
    mic.iter().zip(sys.iter()).map(|(m, s)| 0.5 * (m + s)).collect()
}

/// RMS-based ducking: when system audio is louder than mic, duck mic;
/// otherwise equal mix. Prevents system audio from drowning out the speaker.
fn mix_ducking(mic: &[f32], sys: &[f32]) -> Vec<f32> {
    let mic_rms = rms(mic);
    let sys_rms = rms(sys);

    // If system is much louder (>= 6 dB), duck mic.
    let mic_gain = if sys_rms > 0.0 && mic_rms / (sys_rms + 1e-6) < 0.5 {
        0.3
    } else {
        1.0
    };
    let sys_gain = 1.0;

    mic.iter()
        .zip(sys.iter())
        .map(|(m, s)| mic_gain * m + sys_gain * s)
        .collect()
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}
