//! Mono signed PCM16 little-endian WAV encoding.

use std::path::Path;

/// `samples` as a complete mono PCM16 WAV file. Samples clamp to -1..=1.
pub fn pcm16_bytes(rate: u32, samples: &[f32]) -> Vec<u8> {
    let bytes = samples.len() as u32 * 2;
    let mut output = Vec::with_capacity(44 + bytes as usize);
    output.extend_from_slice(b"RIFF");
    output.extend_from_slice(&(36 + bytes).to_le_bytes());
    output.extend_from_slice(b"WAVEfmt ");
    output.extend_from_slice(&16_u32.to_le_bytes());
    output.extend_from_slice(&1_u16.to_le_bytes());
    output.extend_from_slice(&1_u16.to_le_bytes());
    output.extend_from_slice(&rate.to_le_bytes());
    output.extend_from_slice(&(rate * 2).to_le_bytes());
    output.extend_from_slice(&2_u16.to_le_bytes());
    output.extend_from_slice(&16_u16.to_le_bytes());
    output.extend_from_slice(b"data");
    output.extend_from_slice(&bytes.to_le_bytes());
    for sample in samples {
        output
            .extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    output
}

/// Writes [`pcm16_bytes`] to `path`. Offline authoring only.
pub fn write_pcm16(path: &Path, rate: u32, samples: &[f32]) -> std::io::Result<()> {
    std::fs::write(path, pcm16_bytes(rate, samples))
}
