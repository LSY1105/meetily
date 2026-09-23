//! Audio layer — borrowed from meetily's battle-tested capture code.
//!
//! Cross-platform audio:
//!   - macOS: ScreenCaptureKit for system audio; cpal for mic
//!   - Windows: WASAPI loopback for system audio; cpal for mic
//!   - Linux: PulseAudio monitor source for system audio; cpal for mic
//!
//! We borrow meetily's cpal-based mic capture (proven across platforms)
//! and replace the complex 3-layer sender stack with a simple crossbeam
//! channel.

pub mod asr_pipeline;
pub mod capture;
pub mod mixer;
pub mod session;
pub mod vad;
pub mod wav;

pub use asr_pipeline::AsrPipeline;
pub use capture::{AudioCapture, CaptureConfig, CaptureError};
pub use mixer::{AudioMixer, MixedFrame};
pub use session::AudioSession;
pub use vad::{VadEvent, VadProcessor};
pub use wav::WavWriter;
pub mod resample;
pub use resample::resample_to_16k;
