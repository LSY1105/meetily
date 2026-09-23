//! Minimal RIFF/WAVE writer for mono 16-bit PCM.
//!
//! AudioCapture delivers mono f32 chunks at the device's native sample
//! rate (often 48 kHz); we record them verbatim to disk so a later stage
//! can resample to 16 kHz for ASR or play back the raw capture. The
//! WAV header is written up front; finalize() patches the two length
//! fields and flushes.
//!
//! `WavWriter` is generic over the underlying `Write` so the same code
//! can serialise directly into an in-memory `Vec<u8>` for short ASR pings
//! (see `audio::asr_pipeline`) without round-tripping through a temp file.
//! `finalize()` still needs `Seek` so it lives in a separate impl block.

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

pub struct WavWriter<W: Write> {
    file: W,
    data_bytes: u32,
}

impl<W: Write> WavWriter<W> {
    pub fn new(writer: W, sample_rate: u32, channels: u16) -> std::io::Result<Self> {
        let mut file = writer;
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
        Ok(Self {
            file,
            data_bytes: 0,
        })
    }

    pub fn write_samples(&mut self, samples: &[f32]) -> std::io::Result<()> {
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            self.file.write_all(&v.to_le_bytes())?;
        }
        self.data_bytes += (samples.len() as u32) * 2;
        Ok(())
    }
}

// finalize() patches the two length fields back at offsets 4 and 40, so
// it needs Seek. Kept in a separate impl block so callers that only want
// Write (e.g. the in-memory ASR pings) don't have to provide Seek.
impl<W: Write + Seek> WavWriter<W> {
    pub fn finalize(mut self) -> std::io::Result<()> {
        self.file.seek(SeekFrom::Start(4))?;
        self.file.write_all(&(36 + self.data_bytes).to_le_bytes())?;
        self.file.seek(SeekFrom::Start(40))?;
        self.file.write_all(&self.data_bytes.to_le_bytes())?;
        Ok(())
    }
}

/// Convenience: open a file at `path` and wrap it in a `WavWriter`.
pub fn create(path: &Path, sample_rate: u32, channels: u16) -> std::io::Result<WavWriter<File>> {
    WavWriter::new(File::create(path)?, sample_rate, channels)
}
