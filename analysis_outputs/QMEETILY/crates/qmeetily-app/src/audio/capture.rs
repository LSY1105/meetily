//! Audio capture via cpal.
//!
//! **Independent implementation for QMeetily** — uses cpal directly, no
//! dependency on meetily. Cross-platform microphone + system audio loopback.
//!
//! Design:
//! - Single producer per stream (cpal callback writes to channel)
//! - No locks in audio callback (lock-free crossbeam channel)
//! - 100 ms frames @ 16 kHz = 1600 samples
//! - Output: `Vec<f32>` mono PCM normalized to [-1.0, 1.0]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SampleRate, Stream, StreamConfig};
use crossbeam::channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("no input device available")]
    NoInputDevice,
    #[error("cpal: {0}")]
    Cpal(#[from] cpal::BuildStreamError),
    #[error("unsupported sample format")]
    UnsupportedFormat,
}

/// Description of an audio device (input or output). Returned by `list_devices`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub is_input: bool,
    pub is_default: bool,
    pub sample_rate: u32,
    pub channels: u16,
}

/// List all input + output audio devices on the system.
pub fn list_devices() -> Result<Vec<DeviceInfo>, CaptureError> {
    let host = cpal::default_host();
    let mut out = Vec::new();

    let default_input = host.default_input_device().and_then(|d| d.name().ok());
    let default_output = host.default_output_device().and_then(|d| d.name().ok());

    if let Ok(devices) = host.devices() {
        for device in devices {
            let name = match device.name() {
                Ok(n) => n,
                Err(_) => continue,
            };

            // Try input config
            if let Ok(cfg) = device.default_input_config() {
                out.push(DeviceInfo {
                    name: name.clone(),
                    is_input: true,
                    is_default: Some(&name) == default_input.as_ref(),
                    sample_rate: cfg.sample_rate().0,
                    channels: cfg.channels(),
                });
            }

            // Try output config (used for system audio loopback)
            if let Ok(cfg) = device.default_output_config() {
                out.push(DeviceInfo {
                    name: name.clone(),
                    is_input: false,
                    is_default: Some(&name) == default_output.as_ref(),
                    sample_rate: cfg.sample_rate().0,
                    channels: cfg.channels(),
                });
            }
        }
    }

    Ok(out)
}

#[derive(Debug, Clone, Copy)]
pub struct CaptureConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub frame_size: u32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            channels: 1,
            frame_size: 1_600, // 100ms @ 16kHz
        }
    }
}

pub struct AudioCapture {
    _stream: Stream,
    pub receiver: Receiver<Vec<f32>>,
    sample_rate: u32,
}

impl AudioCapture {
    /// Capture from the system default input device at 16 kHz mono.
    pub fn microphone(config: CaptureConfig) -> Result<Self, CaptureError> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(CaptureError::NoInputDevice)?;
        Self::from_device(&device, config)
    }

    /// Capture from a specific device (e.g. selected by user in Settings).
    pub fn from_device(device: &Device, config: CaptureConfig) -> Result<Self, CaptureError> {
        // We must open the device at its native sample rate + channel count,
        // because cpal's WASAPI / CoreAudio backends reject configs that don't
        // match the supported range exactly. We then downmix to mono in the
        // callback if the caller asked for fewer channels.
        let native_cfg = device.default_input_config().map_err(|e| match e {
            cpal::DefaultStreamConfigError::DeviceNotAvailable => CaptureError::NoInputDevice,
            _ => CaptureError::UnsupportedFormat,
        })?;

        let native_sr = native_cfg.sample_rate().0;
        let native_ch = native_cfg.channels();
        tracing::info!(
            "device native: {}Hz {}ch, requested: {}Hz {}ch (will downmix in callback)",
            native_sr, native_ch, config.sample_rate, config.channels
        );

        // We always open the device at its native config and let the callback
        // downmix / resample as needed. frame_size=0 → device default buffer.
        let adapted = CaptureConfig {
            sample_rate: native_sr,
            channels: native_ch,
            frame_size: 0,
        };
        Self::from_device_with_config(device, &native_cfg, adapted)
    }

    /// Capture from a device whose input config is already known (used by
    /// the system-audio loopback path on Windows/macOS).
    pub fn from_device_with_config(
        device: &Device,
        input_cfg: &cpal::SupportedStreamConfig,
        config: CaptureConfig,
    ) -> Result<Self, CaptureError> {
        let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = crossbeam::channel::unbounded();

        let stream_config = StreamConfig {
            channels: config.channels,
            sample_rate: SampleRate(config.sample_rate),
            buffer_size: if config.frame_size == 0 {
                cpal::BufferSize::Default
            } else {
                cpal::BufferSize::Fixed(config.frame_size)
            },
        };

        let err_fn = |err| tracing::error!("audio stream error: {err}");

        let stream = match input_cfg.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mono: Vec<f32> = if data.len() > config.channels as usize {
                        // Average all channels to mono
                        let n = config.channels as usize;
                        data.chunks(n).map(|c| c.iter().sum::<f32>() / n as f32).collect()
                    } else {
                        data.to_vec()
                    };
                    let _ = tx.send(mono);
                },
                err_fn,
                None,
            )?,
            SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let mono: Vec<f32> = if data.len() > config.channels as usize {
                        let n = config.channels as usize;
                        data.chunks(n)
                            .map(|c| c.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / n as f32)
                            .collect()
                    } else {
                        data.iter().map(|&s| s as f32 / 32768.0).collect()
                    };
                    let _ = tx.send(mono);
                },
                err_fn,
                None,
            )?,
            SampleFormat::U16 => device.build_input_stream(
                &stream_config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let mono: Vec<f32> = if data.len() > config.channels as usize {
                        let n = config.channels as usize;
                        data.chunks(n)
                            .map(|c| c.iter().map(|&s| (s as f32 - 32768.0) / 32768.0).sum::<f32>() / n as f32)
                            .collect()
                    } else {
                        data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0).collect()
                    };
                    let _ = tx.send(mono);
                },
                err_fn,
                None,
            )?,
            _ => return Err(CaptureError::UnsupportedFormat),
        };

        stream.play().map_err(|e| {
            CaptureError::Cpal(match e {
                cpal::PlayStreamError::DeviceNotAvailable => {
                    cpal::BuildStreamError::DeviceNotAvailable
                }
                _ => cpal::BuildStreamError::DeviceNotAvailable,
            })
        })?;

        Ok(Self {
            _stream: stream,
            receiver: rx,
            sample_rate: config.sample_rate,
        })
    }

    /// Sample rate the underlying stream was opened at (always the device's
    /// native rate after `from_device`'s adaptation). Recorded samples come
    /// out at this rate; downstream code is resampling needed for 16 kHz ASR.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_targets_asr() {
        let cfg = CaptureConfig::default();
        assert_eq!(cfg.sample_rate, 16_000);
        assert_eq!(cfg.channels, 1);
        assert_eq!(cfg.frame_size, 1_600);
    }

    #[test]
    fn list_devices_does_not_panic() {
        // Just confirm enumeration doesn't crash; device count varies by host.
        let _ = list_devices();
    }
}

/// Pick a sample rate the device supports: prefer `desired` if it's in the
/// supported range, otherwise return the device's `fallback`.
#[allow(dead_code)] // kept for future when caller picks a target sample rate
fn pick_sample_rate(device: &Device, desired: u32, fallback: u32) -> Option<u32> {
    let ranges: Vec<_> = device.supported_input_configs().ok()?.collect();
    ranges
        .into_iter()
        .find(|r| r.min_sample_rate().0 <= desired && desired <= r.max_sample_rate().0)
        .map(|_| desired)
        .or(Some(fallback))
}

/// Pick a supported config matching (sample_rate, channels), else None.
#[allow(dead_code)] // kept for future when caller picks a target sample rate
fn find_matching_config(device: &Device, sr: u32, channels: u16) -> Option<cpal::SupportedStreamConfig> {
    let ranges: Vec<_> = match device.supported_input_configs() {
        Ok(r) => r.collect(),
        Err(e) => {
            tracing::warn!("supported_input_configs failed: {e}");
            return None;
        }
    };
    for r in ranges {
        if r.min_sample_rate().0 <= sr && sr <= r.max_sample_rate().0 && r.channels() == channels {
            return Some(r.with_sample_rate(cpal::SampleRate(sr)));
        }
    }
    None
}
