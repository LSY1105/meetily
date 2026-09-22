//! Minimal RIFF/WAVE writer for mono 16-bit PCM.
//!
//! AudioCapture delivers mono f32 chunks at the device's native sample
//! rate (often 48 kHz); we record them verbatim to disk so a later stage
//! can resample to 16 kHz for ASR or play back the raw capture. The
//! WAV header is written up front; finalize() patches the two length
//! fields and flushes.

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

pub struct WavWriter {
    file: File,
    data_bytes: u32,
}

impl WavWriter {
    pub fn create(path: &Path, sample_rate: u32, channels: u16) -> std::io::Result<Self> {
        let mut file = File::create(path)?;
        // RIFF header
        file.write_all(b"RIFF")?;
        file.write_all(&0u32.to_le_bytes())?;
        file.write_all(b"WAVE")?;
        // fmt subchunk
        file.write_all(b"fmt ")?;
        file.write_all(&16u32.to_le_bytes())?;
        file.write_all(&1u16.to_le_bytes())?;
        file.write_all(&channels.to_le_bytes())?;
        file.write_all(&sample_rate.to_le_bytes())?;
        let byte_rate = sample_rate * channels as u32 * 2;
        file.write_all(&byte_rate.to_le_bytes())?;
        file.write_all(&(channels * 2).to_le_bytes())?;
        file.write_all(&16u16.to_le_bytes())?;
        // data subchunk header
        file.write_all(b"data")?;
        file.write_all(&0u32.to_le_bytes())?;
        Ok(Self { file, data_bytes: 0 })
    }

    pub fn write_samples(&mut self, samples: &[f32]) -> std::io::Result<()> {
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            self.file.write_all(&v.to_le_bytes())?;
        }
        self.data_bytes += (samples.len() as u32) * 2;
        Ok(())
    }

    pub fn finalize(mut self) -> std::io::Result<()> {
        self.file.seek(SeekFrom::Start(4))?;
        self.file.write_all(&(36 + self.data_bytes).to_le_bytes())?;
        self.file.seek(SeekFrom::Start(40))?;
        self.file.write_all(&self.data_bytes.to_le_bytes())?;
        self.file.sync_all()?;
        Ok(())
    }
}
