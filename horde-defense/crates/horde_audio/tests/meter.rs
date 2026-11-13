//! Pins for the loudness meter (A6-03): ITU-R BS.1770-4 gated loudness and
//! the 4x true peak, against the EBU Tech 3341 compliance signals and the
//! Python baseline the audio design measured the v1 library with.

use horde_audio::{
    cue::Cue,
    meter::{Biquad, MOMENTARY_SECONDS, k_weighting, measure, sample_peak, true_peak},
    synth::legacy::{self, synthesize},
};
use std::f64::consts::TAU;

/// A sine of `seconds` at `dbfs` peak and `hz`, starting at `phase` radians.
fn sine(rate: u32, hz: f64, dbfs: f64, seconds: f64, phase: f64) -> Vec<f32> {
    let amplitude = 10.0_f64.powf(dbfs / 20.0);
    (0..(f64::from(rate) * seconds).round() as usize)
        .map(|index| (amplitude * (TAU * hz * index as f64 / f64::from(rate) + phase).sin()) as f32)
        .collect()
}

fn stereo_integrated(rate: u32, signal: &[f32]) -> f64 {
    measure(rate, &[signal, signal]).integrated
}

fn assert_close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} is not {expected} +/-{tolerance}"
    );
}

/// EBU Tech 3341 case 1: a stereo 1 kHz sine at -23 dBFS for 20 s reads
/// -23.0 +/-0.1 LUFS. Checked at both rates the project meets (the exporter's
/// 44.1 kHz and the standard's 48 kHz) so the per-rate K-weighting is pinned.
#[test]
fn ebu3341_case1_stereo_1khz_minus23() {
    for rate in [44_100, 48_000] {
        let signal = sine(rate, 1_000.0, -23.0, 20.0, 0.0);
        assert_close(
            stereo_integrated(rate, &signal),
            -23.0,
            0.1,
            &format!("case 1 at {rate} Hz"),
        );
    }
}

/// EBU Tech 3341 case 2: the same sine at -33 dBFS reads -33.0 +/-0.1 LUFS.
#[test]
fn ebu3341_case2_minus33() {
    for rate in [44_100, 48_000] {
        let signal = sine(rate, 1_000.0, -33.0, 20.0, 0.0);
        assert_close(
            stereo_integrated(rate, &signal),
            -33.0,
            0.1,
            &format!("case 2 at {rate} Hz"),
        );
    }
}

/// EBU Tech 3341 case 3: 10 s at -36 dBFS, 60 s at -23 dBFS, 10 s at -36 dBFS.
/// The relative gate drops the quiet ends, so it reads -23.0 +/-0.1 LUFS; an
/// ungated mean would read about -23.9.
#[test]
fn ebu3341_gating_minus36_minus23_minus36_reads_minus23() {
    let rate = 48_000;
    let mut signal = sine(rate, 1_000.0, -36.0, 10.0, 0.0);
    signal.extend(sine(rate, 1_000.0, -23.0, 60.0, 0.0));
    signal.extend(sine(rate, 1_000.0, -36.0, 10.0, 0.0));
    assert_close(stereo_integrated(rate, &signal), -23.0, 0.1, "case 3");
}

/// The absolute gate: a programme at -80 dBFS never opens the -70 LUFS gate,
/// so its integrated loudness is negative infinity while its momentary
/// loudness is still a finite reading.
#[test]
fn absolute_gate_closes_below_minus70() {
    let rate = 48_000;
    let signal = sine(rate, 1_000.0, -80.0, 5.0, 0.0);
    let loudness = measure(rate, &[&signal, &signal]);
    assert_eq!(loudness.integrated, f64::NEG_INFINITY);
    assert_close(loudness.momentary_max, -80.0, 0.1, "momentary at -80 dBFS");
}

#[test]
fn silence_reads_neg_inf() {
    let silence = vec![0.0_f32; 48_000 * 2];
    let loudness = measure(48_000, &[&silence, &silence]);
    assert_eq!(loudness.integrated, f64::NEG_INFINITY);
    assert_eq!(loudness.momentary_max, f64::NEG_INFINITY);
    assert_eq!(loudness.short_term_max, f64::NEG_INFINITY);
    assert_eq!(loudness.sample_peak, 0.0);
    assert_eq!(loudness.true_peak, 0.0);

    let empty = measure(48_000, &[&[], &[]]);
    assert_eq!(empty.integrated, f64::NEG_INFINITY);
    assert_eq!(empty.true_peak, 0.0);
    assert_eq!(measure(48_000, &[]).integrated, f64::NEG_INFINITY);
}

/// A full-scale sine at fs/4 sampled 45 degrees off its crests has samples of
/// +/-0.7071 only. The sample peak misses 3 dB; the 4x true peak finds 1.0.
#[test]
fn true_peak_of_fs_over_4_sine_at_45deg_is_unity() {
    let rate = 48_000;
    let signal = sine(rate, f64::from(rate) / 4.0, 0.0, 0.5, TAU / 8.0);
    assert_close(
        f64::from(sample_peak(&[&signal])),
        std::f64::consts::FRAC_1_SQRT_2,
        1.0e-4,
        "sample peak",
    );
    assert_close(f64::from(true_peak(&[&signal])), 1.0, 0.05, "true peak");
    let loudness = measure(rate, &[&signal, &signal]);
    assert_close(
        f64::from(loudness.true_peak),
        1.0,
        0.05,
        "measured true peak",
    );
}

/// The oversampler's phase 0 is the input itself, so the true peak is never
/// below the sample peak, and a single full-scale sample reads exactly 1.0.
#[test]
fn true_peak_is_never_below_the_sample_peak() {
    let mut click = vec![0.0_f32; 64];
    click[32] = -1.0;
    assert_eq!(true_peak(&[&click]), 1.0);
    for variant in 0..legacy::VARIANTS {
        let clip = synthesize(Cue::Explosion, variant);
        assert!(true_peak(&[&clip]) >= sample_peak(&[&clip]));
    }
    // The loudest channel wins.
    let quiet = vec![0.25_f32; 64];
    assert_eq!(sample_peak(&[&quiet, &click]), 1.0);
}

/// The design measured the shipped v1 library in Python before any Rust meter
/// existed: `survival_pistol_0` read -35.9 LUFS momentary-max (design-audio
/// section 1). The clip is synthesised here, byte-identical to the shipped
/// file (`legacy_library.rs`), so this pin outlives the wave-6 re-master.
#[test]
fn momentary_max_matches_python_baseline_survival_pistol_0() {
    let clip = synthesize(Cue::Pistol, 0);
    // Shorter than one 400 ms block: the meter pads it to one block.
    assert!((clip.len() as f64) < MOMENTARY_SECONDS * f64::from(legacy::RATE));
    let loudness = measure(legacy::RATE, &[&clip]);
    assert_close(loudness.momentary_max, -35.9, 0.3, "pistol_0 momentary max");
}

/// A signal shorter than a short-term window still reports a short-term
/// reading (one padded window), and a steady tone reads the same on all three
/// time scales.
#[test]
fn steady_tone_reads_the_same_on_every_time_scale() {
    let rate = 44_100;
    let signal = sine(rate, 1_000.0, -23.0, 8.0, 0.0);
    let loudness = measure(rate, &[&signal, &signal]);
    assert_close(loudness.momentary_max, -23.0, 0.1, "momentary");
    assert_close(loudness.short_term_max, -23.0, 0.1, "short-term");
    assert_close(loudness.integrated, -23.0, 0.1, "integrated");
    // Mono carries half the power of the same tone on two channels.
    let mono = measure(rate, &[&signal]);
    assert_close(mono.integrated, -26.01, 0.1, "mono");
}

/// At 48 kHz the derived K-weighting is the coefficient table BS.1770-4
/// prints (tables 1 and 2).
#[test]
fn k_weighting_at_48k_matches_the_bs1770_tables() {
    let [shelf, high_pass] = k_weighting(48_000);
    let expect = |actual: Biquad, b: [f64; 3], a: [f64; 2]| {
        let pairs = [
            (actual.b0, b[0]),
            (actual.b1, b[1]),
            (actual.b2, b[2]),
            (actual.a1, a[0]),
            (actual.a2, a[1]),
        ];
        for (index, (actual, expected)) in pairs.into_iter().enumerate() {
            assert_close(actual, expected, 1.0e-9, &format!("coefficient {index}"));
        }
    };
    expect(
        shelf,
        [
            1.535_124_859_586_97,
            -2.691_696_189_406_38,
            1.198_392_810_852_85,
        ],
        [-1.690_659_293_182_41, 0.732_480_774_215_85],
    );
    expect(
        high_pass,
        [1.0, -2.0, 1.0],
        [-1.990_047_454_833_98, 0.990_072_250_366_21],
    );
    // Another rate gets its own coefficients, not the 48 kHz table.
    assert!((k_weighting(44_100)[0].b0 - shelf.b0).abs() > 1.0e-3);
}

/// The meter is a pure function of its input.
#[test]
fn measure_is_deterministic() {
    let clip = synthesize(Cue::Explosion, 1);
    assert_eq!(
        measure(legacy::RATE, &[&clip]),
        measure(legacy::RATE, &[&clip])
    );
}
