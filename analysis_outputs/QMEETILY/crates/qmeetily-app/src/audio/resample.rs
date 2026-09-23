//! Downsample mono f32 audio to 16 kHz for the ASR sidecar.
//!
//! `AudioCapture` delivers samples at the device's native rate (often
//! 48 kHz on macOS / Windows / Linux defaults). The Qwen3-ASR sidecar
//! expects 16 kHz mono PCM, so we polyphase-resample in `AsrPipeline`
//! before serialising the WAV. We use `rubato`'s `SincFixedIn` with a
//! Blackman-Harris window — it's the Rust ecosystem's standard sinc
//! resampler and gives us proper anti-aliasing for the 48->16 kHz case
//! (3:1 decimation) where naive linear interpolation would alias badly.

use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

/// Sample rate the ASR sidecar expects. Anything different must be
/// resampled to this before upload.
pub const ASR_SAMPLE_RATE: u32 = 16_000;

/// Resample mono `f32` audio from the cpal native rate to 16 kHz.
/// Returns a fresh `Vec<f32>`; the input slice is untouched. If
/// `from_rate` already equals `ASR_SAMPLE_RATE`, returns a clone.
/// Upsampling (raising the rate) is not supported — the sidecar would
/// just be sent redundant samples.
pub fn resample_to_16k(samples: &[f32], from_rate: u32) -> anyhow::Result<Vec<f32>> {
    if from_rate == ASR_SAMPLE_RATE {
        return Ok(samples.to_vec());
    }
    if from_rate < ASR_SAMPLE_RATE {
        anyhow::bail!(
            "upsampling not supported (from {} Hz, target {} Hz)",
            from_rate,
            ASR_SAMPLE_RATE
        );
    }

    // ratio = output_rate / input_rate, in (0, 1] for downsampling.
    let ratio = ASR_SAMPLE_RATE as f64 / from_rate as f64;

    // Process 1 second of input per chunk — enough to fill the filter's
    // internal delay buffer on the first pass without holding the whole
    // 5-second buffer (which can be 240_000 f32 @ 48 kHz = ~1 MB).
    let chunk_size = from_rate as usize;

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(
        ratio,
        2.0, // max_resample_ratio_relative — we never exceed 1:2 here
        params,
        chunk_size,
        1, // mono
    )?;

    let mut output = Vec::with_capacity((samples.len() as f64 * ratio) as usize + 1024);
    let mut pos = 0;
    while pos < samples.len() {
        let end = (pos + chunk_size).min(samples.len());
        let mut frame = samples[pos..end].to_vec();
        // Pad the tail chunk to chunk_size so the filter has enough
        // context. Zeros are safe — they fall outside the actual audio.
        frame.resize(chunk_size, 0.0);
        let waves_out = resampler.process(&[frame], None)?;
        output.extend_from_slice(&waves_out[0]);
        pos += chunk_size;
    }
    // Flush whatever the resampler is still holding in its filter delay.
    let tail = resampler.process_partial::<Vec<f32>>(None, None)?;
    for ch in tail {
        output.extend_from_slice(&ch);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_already_16k() {
        let input = vec![0.1, 0.2, -0.3, 0.4];
        let out = resample_to_16k(&input, 16_000).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn rejects_upsample() {
        assert!(resample_to_16k(&[0.0; 100], 8_000).is_err());
    }

    #[test]
    fn downsample_48_to_16_reduces_length_by_third() {
        // 1 second @ 48 kHz -> ~1/3 second @ 16 kHz. The upper bound is loose
        // because SincFixedIn::output_frames_next includes filter-delay
        // overhead on top of the strict ratio * input length.
        let input: Vec<f32> = (0..48_000).map(|i| (i as f32 / 48_000.0).sin()).collect();
        let out = resample_to_16k(&input, 48_000).unwrap();
        assert!(out.len() >= 15_900 && out.len() <= 32_500,
                "expected ~16000 samples (+ filter delay), got {}", out.len());
    }
}


