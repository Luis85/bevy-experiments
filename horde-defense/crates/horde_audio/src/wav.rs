//! Signed PCM16 little-endian WAV: mono encoding, and decoding for the
//! offline renderer.

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

/// A decoded PCM16 file: interleaved samples in -1..1.
#[derive(Clone, Debug, PartialEq)]
pub struct Pcm16 {
    pub rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}
impl Pcm16 {
    /// The channels averaged into one.
    pub fn mono(&self) -> Vec<f32> {
        let channels = usize::from(self.channels.max(1));
        self.samples
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    }
}

/// Decodes a PCM16 WAV file. Only integer PCM at 16 bits is accepted: it is
/// the one format the project ships.
pub fn read_pcm16(bytes: &[u8]) -> Result<Pcm16, String> {
    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([bytes[at], bytes[at + 1]])
    }
    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF WAVE file".into());
    }
    let mut format = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(bytes, at + 4) as usize;
        let body = at + 8;
        let end = body
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or("a chunk runs past the end of the file")?;
        if id == b"fmt " {
            if size < 16 {
                return Err("the fmt chunk is too short".into());
            }
            let (tag, channels, bits) = (
                u16_at(bytes, body),
                u16_at(bytes, body + 2),
                u16_at(bytes, body + 14),
            );
            if tag != 1 || bits != 16 || channels == 0 {
                return Err(format!(
                    "not 16-bit integer PCM (format {tag}, {bits} bits, {channels} channels)"
                ));
            }
            format = Some((u32_at(bytes, body + 4), channels));
        } else if id == b"data" {
            let (rate, channels) = format.ok_or("the data chunk comes before fmt")?;
            let (pairs, _odd_byte) = bytes[body..end].as_chunks::<2>();
            let samples = pairs
                .iter()
                .map(|pair| f32::from(i16::from_le_bytes(*pair)) / 32_767.0)
                .collect();
            return Ok(Pcm16 {
                rate,
                channels,
                samples,
            });
        }
        // Chunks are word aligned.
        at = end + (size & 1);
    }
    Err("no data chunk".into())
}
