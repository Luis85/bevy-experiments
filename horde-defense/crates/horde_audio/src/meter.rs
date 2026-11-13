//! Loudness and true-peak meter after ITU-R BS.1770-4 and EBU R 128.
//!
//! Offline and pure: [`measure`] takes whole planar channels and returns one
//! [`Loudness`]. The offline renderer's gate, the exporter's per-clip
//! mastering and the provenance records all read this one meter.
//!
//! - K-weighting is derived per sample rate from the filter parameters behind
//!   the standard's 48 kHz tables ([`k_weighting`]), so 44.1 kHz material is
//!   metered without resampling.
//! - Momentary loudness uses 400 ms blocks, short-term 3 s windows, both
//!   stepped by 100 ms (75% block overlap).
//! - Integrated loudness gates the 400 ms blocks at -70 LUFS absolute, then at
//!   -10 LU below the mean of the surviving blocks.
//! - True peak is the largest magnitude after 4x oversampling with a 48-tap
//!   polyphase FIR, as in the standard's annex 2.
//!
//! Every channel has weight 1.0: the game mixes mono and stereo only, so the
//! surround weights of the standard never apply.

use std::f64::consts::PI;

/// Momentary block length.
pub const MOMENTARY_SECONDS: f64 = 0.4;
/// Short-term window length.
pub const SHORT_TERM_SECONDS: f64 = 3.0;
/// Step between successive blocks and windows.
pub const STEP_SECONDS: f64 = 0.1;
/// Blocks below this never count towards integrated loudness.
pub const ABSOLUTE_GATE_LUFS: f64 = -70.0;
/// The relative gate sits this far below the absolute-gated mean.
pub const RELATIVE_GATE_LU: f64 = -10.0;
/// The true-peak oversampling factor.
pub const TRUE_PEAK_OVERSAMPLE: usize = 4;

/// The loudness offset of BS.1770: a 997 Hz full-scale sine on one channel
/// reads -3.01 LUFS.
const OFFSET_LUFS: f64 = -0.691;

/// Input samples each oversampling phase reaches to either side.
const TRUE_PEAK_HALF_TAPS: isize = 6;
/// Taps per oversampling phase (48 across the four phases).
const TRUE_PEAK_PHASE_TAPS: usize = (TRUE_PEAK_HALF_TAPS * 2) as usize;

/// One measurement of a whole signal. Loudness is in LUFS, peaks are linear
/// full scale. A reading with no energy behind it is negative infinity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loudness {
    /// Gated programme loudness.
    pub integrated: f64,
    /// The loudest 400 ms block.
    pub momentary_max: f64,
    /// The loudest 3 s window.
    pub short_term_max: f64,
    /// The largest sample magnitude.
    pub sample_peak: f32,
    /// The largest magnitude after 4x oversampling; never below `sample_peak`.
    pub true_peak: f32,
}

/// A direct-form biquad with `a0` normalised to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

/// The two K-weighting stages at `rate`: the high shelf that models the head,
/// then the RLB high-pass. At 48 kHz these are the standard's tables 1 and 2.
pub fn k_weighting(rate: u32) -> [Biquad; 2] {
    let rate = f64::from(rate);

    // Stage 1: +4 dB shelf above about 1.7 kHz.
    let k = (PI * 1_681.974_450_955_533 / rate).tan();
    let q = 0.707_175_236_955_419_6;
    let high = 10.0_f64.powf(3.999_843_853_973_347 / 20.0);
    let band = high.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b0: (high + band * k / q + k * k) / a0,
        b1: 2.0 * (k * k - high) / a0,
        b2: (high - band * k / q + k * k) / a0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
    };

    // Stage 2: second-order high-pass at about 38 Hz. The numerator stays
    // 1, -2, 1 at every rate, as the standard's table prints it.
    let k = (PI * 38.135_470_876_024_44 / rate).tan();
    let q = 0.500_327_037_323_877_3;
    let a0 = 1.0 + k / q + k * k;
    let high_pass = Biquad {
        b0: 1.0,
        b1: -2.0,
        b2: 1.0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
    };

    [shelf, high_pass]
}

/// The largest sample magnitude across `channels`.
pub fn sample_peak(channels: &[&[f32]]) -> f32 {
    channels
        .iter()
        .flat_map(|channel| channel.iter())
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
}

/// The largest magnitude across `channels` after 4x oversampling. Samples
/// beyond either end count as silence.
pub fn true_peak(channels: &[&[f32]]) -> f32 {
    let phases = oversampling_phases();
    let mut peak = sample_peak(channels);
    for channel in channels {
        let length = channel.len() as isize;
        for index in 0..length {
            for taps in &phases {
                let mut sum = 0.0_f64;
                for (tap, weight) in taps.iter().enumerate() {
                    // Tap `tap` weighs the input `offset` samples after `index`.
                    let source = index + tap as isize - (TRUE_PEAK_HALF_TAPS - 1);
                    if (0..length).contains(&source) {
                        sum += f64::from(channel[source as usize]) * weight;
                    }
                }
                peak = peak.max(sum.abs() as f32);
            }
        }
    }
    peak
}

/// The interpolating phases 1..4 of the oversampler: a Hann-windowed sinc
/// reaching six input samples to either side, each phase normalised to unity
/// gain at DC. Phase 0 is the input itself and needs no filter.
///
/// Phase `p` reconstructs the signal at `index + p / 4`; its tap `t` weighs
/// the input at `index + t - 5`.
fn oversampling_phases() -> [[f64; TRUE_PEAK_PHASE_TAPS]; TRUE_PEAK_OVERSAMPLE - 1] {
    let mut phases = [[0.0; TRUE_PEAK_PHASE_TAPS]; TRUE_PEAK_OVERSAMPLE - 1];
    let reach = TRUE_PEAK_HALF_TAPS as f64;
    for (phase, taps) in phases.iter_mut().enumerate() {
        let fraction = (phase + 1) as f64 / TRUE_PEAK_OVERSAMPLE as f64;
        for (tap, weight) in taps.iter_mut().enumerate() {
            // Distance from the tap's input sample to the reconstructed point.
            let distance = fraction - (tap as f64 - (reach - 1.0));
            let sinc = (PI * distance).sin() / (PI * distance);
            let window = 0.5 * (1.0 + (PI * distance / reach).cos());
            *weight = sinc * window;
        }
        let gain: f64 = taps.iter().sum();
        for weight in taps.iter_mut() {
            *weight /= gain;
        }
    }
    phases
}

/// Measures `channels` (planar, equal length, `rate` Hz).
///
/// A signal shorter than one block or window is measured as that one block
/// padded with silence, so a short clip still has a momentary and a
/// short-term reading.
pub fn measure(rate: u32, channels: &[&[f32]]) -> Loudness {
    let frames = channels
        .iter()
        .map(|channel| channel.len())
        .min()
        .unwrap_or(0);
    let energy = weighted_energy_prefix(rate, channels, frames);

    let block = window_frames(rate, MOMENTARY_SECONDS);
    let step = window_frames(rate, STEP_SECONDS);
    let momentary = window_powers(&energy, block, step);
    let short_term = window_powers(&energy, window_frames(rate, SHORT_TERM_SECONDS), step);

    Loudness {
        integrated: integrated(&momentary),
        momentary_max: lufs(max_power(&momentary)),
        short_term_max: lufs(max_power(&short_term)),
        sample_peak: sample_peak(channels),
        true_peak: true_peak(channels),
    }
}

fn window_frames(rate: u32, seconds: f64) -> usize {
    ((f64::from(rate) * seconds).round() as usize).max(1)
}

/// `prefix[n]` is the K-weighted squared signal summed over all channels and
/// the first `n` frames.
fn weighted_energy_prefix(rate: u32, channels: &[&[f32]], frames: usize) -> Vec<f64> {
    let mut squared = vec![0.0_f64; frames];
    for channel in channels {
        // Per stage: the two previous inputs and outputs.
        let stages = k_weighting(rate);
        let mut state = [[0.0_f64; 4]; 2];
        for (sample, total) in channel[..frames].iter().zip(&mut squared) {
            let mut value = f64::from(*sample);
            for (stage, memory) in stages.iter().zip(&mut state) {
                let [x1, x2, y1, y2] = *memory;
                let output = stage.b0 * value + stage.b1 * x1 + stage.b2 * x2
                    - stage.a1 * y1
                    - stage.a2 * y2;
                *memory = [value, x1, output, y1];
                value = output;
            }
            *total += value * value;
        }
    }
    let mut prefix = Vec::with_capacity(frames + 1);
    let mut running = 0.0_f64;
    prefix.push(running);
    for value in squared {
        running += value;
        prefix.push(running);
    }
    prefix
}

/// The mean power of every `window`-frame span, `step` frames apart. A signal
/// shorter than one window yields that one window, padded with silence.
fn window_powers(prefix: &[f64], window: usize, step: usize) -> Vec<f64> {
    let frames = prefix.len() - 1;
    if frames == 0 {
        return Vec::new();
    }
    if frames < window {
        return vec![prefix[frames] / window as f64];
    }
    (0..=(frames - window) / step)
        .map(|index| {
            let start = index * step;
            (prefix[start + window] - prefix[start]) / window as f64
        })
        .collect()
}

fn max_power(powers: &[f64]) -> f64 {
    powers.iter().copied().fold(0.0, f64::max)
}

fn lufs(power: f64) -> f64 {
    if power > 0.0 {
        OFFSET_LUFS + 10.0 * power.log10()
    } else {
        f64::NEG_INFINITY
    }
}

/// Gated loudness of the 400 ms block powers.
fn integrated(blocks: &[f64]) -> f64 {
    let mean_above = |gate: f64| {
        let (sum, count) = blocks
            .iter()
            .filter(|power| lufs(**power) > gate)
            .fold((0.0_f64, 0_u32), |(sum, count), power| {
                (sum + power, count + 1)
            });
        if count == 0 {
            0.0
        } else {
            sum / f64::from(count)
        }
    };
    let relative_gate = lufs(mean_above(ABSOLUTE_GATE_LUFS)) + RELATIVE_GATE_LU;
    lufs(mean_above(relative_gate.max(ABSOLUTE_GATE_LUFS)))
}
