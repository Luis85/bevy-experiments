//! Per-clip mastering of the Rust-synthesized families (ruling R3: offline
//! asset mastering, not a runtime stage).
//!
//! A clip is brought to its bank's target momentary-max loudness and held
//! under a true-peak [`CEILING`] by a lookahead limiter that may take at most
//! [`MAX_GAIN_REDUCTION_DB`].
//!
//! The target is [`TARGET_MMAX_LUFS`] wherever the limiter can reach it
//! within that bound. A few v1 families are short clicks and cracks whose
//! peak sits far above their loudness; their synthesis is frozen, so they
//! cannot be reshaped, and reaching -21 LUFS would take 6 to 17 dB of
//! limiting. Such a bank is mastered to the loudest half-LU step all its
//! variants reach inside the bound ([`bank_target`]), and the provenance
//! records that target per file.
//!
//! The result is deterministic and already on the PCM16 grid, so the numbers
//! recorded in the provenance are the numbers of the written file.

use crate::meter;

/// The momentary-max loudness a mastered clip has (mono file) unless its bank
/// is peak-bound.
pub const TARGET_MMAX_LUFS: f64 = -21.0;
/// How far a mastered clip may sit from its bank's target.
pub const TOLERANCE_LU: f64 = 0.5;
/// The true-peak ceiling of a mastered clip, linear full scale.
pub const CEILING: f32 = 0.80;
/// How far ahead of a peak the limiter starts to turn down.
pub const LOOKAHEAD_SECONDS: f32 = 0.0015;
/// The limiter's release time constant.
pub const RELEASE_SECONDS: f32 = 0.040;
/// The most gain the limiter may take from any clip.
pub const MAX_GAIN_REDUCTION_DB: f32 = 5.0;

/// The limiter aims slightly under [`CEILING`]: rounding to PCM16 and the
/// moving gain shift the reconstructed peaks by a fraction of a percent.
const CEILING_MARGIN: f32 = 0.985;
/// A peak-bound bank's target leaves this much of the limiter bound unused,
/// so the last correction step cannot cross it.
const BOUND_MARGIN_DB: f32 = 0.25;
const PCM16_SCALE: f32 = 32_767.0;

/// One mastered clip.
#[derive(Clone, Debug, PartialEq)]
pub struct Mastered {
    /// Mono samples on the PCM16 grid.
    pub samples: Vec<f32>,
    /// The largest gain reduction the limiter applied, in dB (0 = untouched).
    pub limiter_gr_db: f32,
}

fn quantise(samples: &mut [f32]) {
    for sample in samples {
        *sample = (sample.clamp(-1.0, 1.0) * PCM16_SCALE).round() / PCM16_SCALE;
    }
}

fn decibels(db: f64) -> f32 {
    10.0_f64.powf(db / 20.0) as f32
}

/// `raw * gain` through the limiter: the limited samples and the largest gain
/// reduction in dB. `peaks` is `meter::oversampled_peaks(raw)`.
fn limit(rate: u32, raw: &[f32], peaks: &[f32], gain: f32) -> (Vec<f32>, f32) {
    let ceiling = CEILING * CEILING_MARGIN;
    let count = raw.len();
    let window = ((LOOKAHEAD_SECONDS * rate as f32).round() as usize).max(1);
    // The gain each sample needs on its own.
    let needed: Vec<f32> = peaks
        .iter()
        .map(|peak| (ceiling / (peak * gain).max(1e-9)).min(1.0))
        .collect();
    // Lookahead: the lowest need among this sample and the next `window`.
    let mut ahead = vec![1.0_f32; count];
    for index in 0..count {
        let end = (index + window + 1).min(count);
        ahead[index] = needed[index..end].iter().copied().fold(1.0, f32::min);
    }
    // Release: after a peak the gain recovers exponentially.
    let recovery = (-1.0 / (RELEASE_SECONDS * rate as f32)).exp();
    let mut held = 1.0_f32;
    for value in &mut ahead {
        held = value.min(1.0 - (1.0 - held) * recovery);
        *value = held;
    }
    // Attack: average the last `window + 1` values. Each of them already
    // covers this sample's own need, so the average never exceeds it.
    let mut output = Vec::with_capacity(count);
    let mut lowest = 1.0_f32;
    let mut sum = 0.0_f64;
    for index in 0..count {
        sum += f64::from(ahead[index]);
        if index > window {
            sum -= f64::from(ahead[index - window - 1]);
        }
        let span = (index + 1).min(window + 1);
        // Before the first full window the missing history counts as unity.
        let smoothed = ((sum + (window + 1 - span) as f64) / (window + 1) as f64) as f32;
        let applied = smoothed.min(needed[index]);
        lowest = lowest.min(applied);
        output.push(raw[index] * gain * applied);
    }
    (output, -20.0 * lowest.log10())
}

/// The loudest momentary-max `raw` reaches while the limiter stays
/// [`BOUND_MARGIN_DB`] inside its bound.
fn reach(rate: u32, raw: &[f32]) -> f64 {
    let peaks = meter::oversampled_peaks(raw);
    let top = peaks.iter().copied().fold(1e-9_f32, f32::max);
    // The limiter's largest reduction is set by the largest peak alone, so
    // the bound fixes the input gain.
    let bound = MAX_GAIN_REDUCTION_DB - BOUND_MARGIN_DB;
    let gain = CEILING * CEILING_MARGIN / top * decibels(f64::from(bound));
    let (limited, _) = limit(rate, raw, &peaks, gain);
    meter::momentary_max(rate, &[&limited])
}

/// The target of a bank whose variants are `raws`: [`TARGET_MMAX_LUFS`], or
/// for a peak-bound bank the loudest half-LU step every variant reaches
/// inside the limiter bound.
pub fn bank_target(rate: u32, raws: &[Vec<f32>]) -> f64 {
    let reachable = raws
        .iter()
        .map(|raw| reach(rate, raw))
        .fold(f64::INFINITY, f64::min);
    if reachable >= TARGET_MMAX_LUFS {
        TARGET_MMAX_LUFS
    } else {
        (reachable * 2.0).floor() / 2.0
    }
}

/// Masters one raw clip at `rate` to `target` LUFS momentary-max.
pub fn master(rate: u32, raw: &[f32], target: f64) -> Mastered {
    let peaks = meter::oversampled_peaks(raw);
    let mut gain = decibels(target - meter::momentary_max(rate, &[raw]));
    let mut samples = Vec::new();
    let mut limiter_gr_db = 0.0;
    // Limiting takes a little loudness back, so the input gain is corrected
    // until the limited clip reads the target.
    for _ in 0..12 {
        (samples, limiter_gr_db) = limit(rate, raw, &peaks, gain);
        quantise(&mut samples);
        let error = target - meter::momentary_max(rate, &[&samples]);
        if error.abs() < 0.02 {
            break;
        }
        gain *= decibels(error);
    }
    let true_peak = meter::true_peak(&[&samples]);
    if true_peak > CEILING {
        let trim = CEILING / true_peak * 0.999;
        for sample in &mut samples {
            *sample *= trim;
        }
        quantise(&mut samples);
    }
    Mastered {
        samples,
        limiter_gr_db,
    }
}
