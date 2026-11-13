//! Ambience loops made in code for the layers no recording was found for
//! (sea waves at the shore, the leaves' rustle, night crickets, a village
//! murmur) and as a stand-in should a recorded loop be missing (wind, rain,
//! fire): filtered noise, crackle and chirps. Each is generated once at startup into a buffer
//! that loops seamlessly (its tail cross-faded into its head) and is played
//! by Bevy's audio as a [`Synth`] source. Deterministic: a view-side hash,
//! never the simulation's numbers.
use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;

use bevy::audio::{ChannelCount, Decodable, SampleRate, Source};
use bevy::prelude::*;

/// Samples a second of the generated loops.
pub(crate) const RATE: u32 = 22_050;
/// Seconds one generated loop lasts.
pub(crate) const LOOP_SECS: f32 = 8.0;

/// A generated loop: mono samples at [`RATE`].
#[derive(Asset, TypePath, Clone, Debug)]
pub(crate) struct Synth {
    pub(crate) samples: Arc<[f32]>,
}

/// Plays a [`Synth`] buffer from its start; Bevy's `PlaybackMode::Loop`
/// restarts it.
pub(crate) struct SynthDecoder {
    samples: Arc<[f32]>,
    at: usize,
}

impl Iterator for SynthDecoder {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.samples.get(self.at).copied();
        self.at += 1;
        sample
    }
}

impl Source for SynthDecoder {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.samples.len().saturating_sub(self.at))
    }
    fn channels(&self) -> ChannelCount {
        NonZero::new(1).expect("one channel")
    }
    fn sample_rate(&self) -> SampleRate {
        NonZero::new(RATE).expect("a rate")
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f32(
            self.samples.len() as f32 / RATE as f32,
        ))
    }
}

impl Decodable for Synth {
    type Decoder = SynthDecoder;
    fn decoder(&self) -> SynthDecoder {
        SynthDecoder {
            samples: self.samples.clone(),
            at: 0,
        }
    }
}

/// The sounds that can be made in code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Made {
    Waves,
    Wind,
    Rustle,
    Rain,
    Fire,
    Crickets,
    Murmur,
}

impl Made {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Made::Waves => "made:waves",
            Made::Wind => "made:wind",
            Made::Rustle => "made:rustle",
            Made::Rain => "made:rain",
            Made::Fire => "made:fire",
            Made::Crickets => "made:crickets",
            Made::Murmur => "made:murmur",
        }
    }
}

/// A small deterministic noise source (xorshift), view side only.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// One loop of `made`, [`LOOP_SECS`] long at [`RATE`], peaking below 0.9.
pub(crate) fn generate(made: Made) -> Vec<f32> {
    let length = (LOOP_SECS * RATE as f32) as usize;
    // A second more than the loop: the extra is faded into the head.
    let fade = RATE as usize / 2;
    let total = length + fade;
    let mut noise = Noise(0x9E37_79B9 ^ (made as u32).wrapping_mul(0x85EB_CA6B));
    let tau = std::f32::consts::TAU;
    let mut out = Vec::with_capacity(total);
    let (mut low, mut lower, mut crackle) = (0.0_f32, 0.0_f32, 0.0_f32);
    let (mut band, mut voice, mut syllable) = (0.0_f32, 0.0_f32, 0.0_f32);
    for i in 0..total {
        let t = i as f32 / RATE as f32;
        let white = noise.next();
        low += (white - low) * 0.03;
        lower += (white - lower) * 0.004;
        // Slow swells and gusts with a period that divides the loop.
        let slow = 0.5 + 0.5 * (tau * t / LOOP_SECS).sin();
        let swell = 0.5 + 0.5 * (tau * 2.0 * t / LOOP_SECS).sin();
        let sample = match made {
            // Surf: low rumbling noise rising and falling with each wave.
            Made::Waves => low * 3.2 * (0.25 + 0.75 * swell * swell),
            // Wind: a softer hiss that gusts.
            Made::Wind => low * 2.2 * (0.35 + 0.65 * slow),
            // Leaves: bright noise fluttering fast, in slow gusts.
            Made::Rustle => {
                let flutter = 0.5 + 0.5 * (tau * 7.0 * t + 3.0 * slow).sin();
                (white - low) * 0.28 * (0.3 + 0.7 * slow) * (0.6 + 0.4 * flutter)
            }
            // Rain: the hiss of bright noise, steady.
            Made::Rain => (white - low) * 0.32,
            // A murmur: voice-band noise in uneven syllables, several
            // talkers blurred together.
            Made::Murmur => {
                band += (white - band) * 0.12;
                voice += (band - voice) * 0.05;
                if i % (RATE as usize / 5) == 0 {
                    syllable = 0.4 + 0.6 * (noise.next() + 1.0) * 0.5;
                }
                let rate = 0.5 + 0.5 * (tau * 4.0 * t).sin().abs();
                (band - voice) * 1.6 * syllable * (0.5 + 0.5 * rate)
            }
            // A fire: a soft roar and sparse cracks.
            Made::Fire => {
                if (noise.next() + 1.0) * 0.5 > 0.9994 {
                    crackle = 0.8;
                }
                crackle *= 0.992;
                lower * 4.0 + white * crackle * 0.6
            }
            // Crickets: three short chirps every 0.7 s, two voices apart.
            Made::Crickets => {
                let chirp = |phase: f32| {
                    let at = (t + phase) % 0.7;
                    let on = at < 0.18 && (at % 0.06) < 0.03;
                    if on {
                        (tau * 4_400.0 * t).sin() * 0.18
                    } else {
                        0.0
                    }
                };
                chirp(0.0) + chirp(0.33) * 0.6
            }
        };
        out.push(sample.clamp(-0.89, 0.89));
    }
    // Fade the tail over the head so the loop has no click.
    for i in 0..fade {
        let w = i as f32 / fade as f32;
        out[i] = out[i] * w + out[length + i] * (1.0 - w);
    }
    out.truncate(length);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn made_loops_are_audible_bounded_seamless_and_repeatable() {
        for made in [
            Made::Waves,
            Made::Wind,
            Made::Rustle,
            Made::Rain,
            Made::Fire,
            Made::Crickets,
            Made::Murmur,
        ] {
            let samples = generate(made);
            assert_eq!(samples.len(), (LOOP_SECS * RATE as f32) as usize);
            assert!(samples.iter().all(|s| s.abs() < 0.9), "{made:?} clips");
            let loudness =
                (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
            assert!(loudness > 0.005, "{made:?} is silent: {loudness}");
            // The seam: the last sample runs into the first without a jump
            // larger than the sound's own sample-to-sample movement.
            let steps: f32 = samples
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0, f32::max);
            let seam = (samples[0] - samples[samples.len() - 1]).abs();
            assert!(
                seam <= steps + 1e-3,
                "{made:?} clicks at the seam: {seam} > {steps}"
            );
            assert_eq!(samples, generate(made), "{made:?} is not repeatable");
        }
        // Different sounds differ.
        assert_ne!(generate(Made::Wind), generate(Made::Waves));
    }

    #[test]
    fn a_made_loop_decodes_as_a_mono_source_of_its_samples() {
        let synth = Synth {
            samples: vec![0.1, -0.2, 0.3].into(),
        };
        let decoder = synth.decoder();
        assert_eq!(decoder.channels().get(), 1);
        assert_eq!(decoder.sample_rate().get(), RATE);
        assert_eq!(decoder.collect::<Vec<_>>(), vec![0.1, -0.2, 0.3]);
    }
}
