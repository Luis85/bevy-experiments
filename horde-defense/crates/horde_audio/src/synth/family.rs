//! The wave-6 families: gun handling, enemy voices, boss moments and two
//! interface sounds. All are non-melodic: knocks, noise bands and inharmonic
//! clinks, built from the v1 noise source and knock.
//!
//! Like the v1 synthesis this is offline authoring. Each function returns a
//! raw clip at [`RATE`], peak-normalised to [`PEAK`]; `synth::master` brings
//! it to its shipped level.

use super::legacy::{Noise, PEAK, RATE, SHIFTS, knock, pulse};
use crate::bank::{BankId, BossStage, EnemyStage, EnemyVoice, GunStage, UiFamily, seed};
use horde_core::content::BossKind;
use std::f32::consts::{PI, TAU};

/// A raised-sine window over `from..to`, zero outside it.
fn band(t: f32, from: f32, to: f32) -> f32 {
    if (from..to).contains(&t) {
        (PI * (t - from) / (to - from)).sin().powi(2)
    } else {
        0.0
    }
}

/// A sine whose pitch bends by `bend` (a share of `hz`) over `seconds`, so it
/// never settles on a note.
fn glide(t: f32, hz: f32, bend: f32, seconds: f32) -> f32 {
    (TAU * hz * (t + bend * t * t / (2.0 * seconds))).sin()
}

/// A throat-like flutter at `rate` Hz, 0.1..=1.
fn rough(t: f32, rate: f32) -> f32 {
    0.1 + 0.9 * (PI * rate * t).sin().abs()
}

/// Runs `voice(t, [white, low, mid])` for `seconds` and finishes the clip the
/// way the v1 synthesis does: onset and release ramps, DC removal, an edge
/// taper and peak normalisation.
fn render(
    bank: BankId,
    variant: u64,
    seconds: f32,
    mut voice: impl FnMut(f32, [f32; 3]) -> f32,
) -> Vec<f32> {
    let count = (seconds * RATE as f32).round() as usize;
    let mut source = Noise {
        state: seed(bank, variant),
        low: 0.0,
        mid: 0.0,
    };
    let mut result: Vec<f32> = (0..count)
        .map(|index| {
            let t = index as f32 / RATE as f32;
            let ramp = (t / 0.0007).min(1.0) * ((seconds - t) / 0.025).min(1.0);
            voice(t, source.next()) * ramp
        })
        .collect();
    let mean = result.iter().sum::<f32>() / result.len() as f32;
    for (index, sample) in result.iter_mut().enumerate() {
        let edge = (index as f32 / 96.0).min(1.0) * ((count - 1 - index) as f32 / 256.0).min(1.0);
        *sample = (*sample - mean) * edge;
    }
    let peak = result.iter().map(|s| s.abs()).fold(0.0, f32::max);
    for sample in &mut result {
        *sample *= PEAK / peak.max(0.0001);
    }
    result
}

/// One variant of a wave-6 bank. A v1 family is `legacy::synthesize`.
pub fn synthesize(bank: BankId, variant: u64) -> Vec<f32> {
    let shift = SHIFTS[variant as usize];
    let variation = variant as f32;
    match bank {
        BankId::Legacy(_) => panic!("{bank:?} is a v1 family: use legacy::synthesize"),
        BankId::Gun(stage) => gun(bank, stage, variant, shift, variation),
        BankId::Enemy(voice, stage) => enemy(bank, voice, stage, variant, shift, variation),
        BankId::Boss(kind, stage) => boss(bank, kind, stage, variant, shift, variation),
        BankId::Ui(family) => ui(bank, family, variant, shift, variation),
    }
}

fn gun(bank: BankId, stage: GunStage, variant: u64, shift: f32, variation: f32) -> Vec<f32> {
    // A sliding part: band-passed hiss over `from..to`.
    let slide =
        |t: f32, [white, _, mid]: [f32; 3], from: f32, to: f32| (white - mid) * band(t, from, to);
    match stage {
        // A hammer falling on nothing: a click, a duller answer, a spring.
        GunStage::DryFire => render(bank, variant, 0.26, |t, noise| {
            let [white, low, mid] = noise;
            knock(t, 0.004, white, 2100.0 * shift, 0.008) * 0.55
                + knock(t, 0.024 + variation * 0.001, mid, 760.0 * shift, 0.022) * 0.8
                + mid * pulse(t, 0.03, 0.05) * 0.5
                + low * pulse(t, 0.02, 0.06) * 1.2
        }),
        // The magazine released and drawn out.
        GunStage::ReloadOut => render(bank, variant, 0.4, |t, noise| {
            let [_, low, mid] = noise;
            knock(t, 0.01, mid, 880.0 * shift, 0.016) * 0.6
                + slide(t, noise, 0.04, 0.22) * 0.22
                + knock(t, 0.23 + variation * 0.002, mid, 540.0 * shift, 0.03) * 0.7
                + low * pulse(t, 0.23, 0.06) * 1.2
        }),
        // A fresh magazine pushed home until it latches.
        GunStage::ReloadIn => render(bank, variant, 0.42, |t, noise| {
            let [white, low, mid] = noise;
            slide(t, noise, 0.01, 0.16) * 0.2
                + knock(t, 0.16, mid, 690.0 * shift, 0.028) * 0.9
                + knock(t, 0.2 + variation * 0.002, white, 1350.0 * shift, 0.012) * 0.3
                + low * pulse(t, 0.16, 0.07) * 1.4
        }),
        // One shell thumbed into a tube magazine.
        GunStage::ShellInsert => render(bank, variant, 0.3, |t, noise| {
            let [white, low, mid] = noise;
            knock(t, 0.008, mid, 420.0 * shift, 0.03) * 0.8
                + knock(t, 0.07 + variation * 0.002, white, 1500.0 * shift, 0.012) * 0.3
                + slide(t, noise, 0.01, 0.09) * 0.15
                + low * pulse(t, 0.008, 0.07) * 1.5
        }),
        // Bolt up and back, then forward and down.
        GunStage::BoltCycle => render(bank, variant, 0.58, |t, noise| {
            let [white, low, mid] = noise;
            knock(t, 0.01, white, 1250.0 * shift, 0.012) * 0.5
                + slide(t, noise, 0.03, 0.2) * 0.2
                + knock(t, 0.21, mid, 640.0 * shift, 0.026) * 0.7
                + slide(t, noise, 0.25, 0.39) * 0.2
                + knock(t, 0.4 + variation * 0.003, mid, 820.0 * shift, 0.024) * 0.9
                + low * (pulse(t, 0.21, 0.06) + pulse(t, 0.4, 0.07)) * 1.2
        }),
    }
}

/// What makes one enemy voice: body pitch, flutter rate, length, and how much
/// low rumble, mid rasp and hiss it carries.
struct Throat {
    body: f32,
    flutter: f32,
    scale: f32,
    low: f32,
    mid: f32,
    hiss: f32,
}

const fn throat(voice: EnemyVoice) -> Throat {
    let (body, flutter, scale, low, mid, hiss) = match voice {
        EnemyVoice::Shambler => (92.0, 23.0, 1.0, 1.6, 0.7, 0.05),
        EnemyVoice::Runner => (148.0, 41.0, 0.75, 0.6, 0.9, 0.2),
        EnemyVoice::Brute => (61.0, 15.0, 1.25, 2.6, 0.5, 0.03),
        EnemyVoice::Armored => (104.0, 19.0, 1.0, 1.2, 0.6, 0.1),
        EnemyVoice::Demolisher => (54.0, 11.0, 1.4, 3.0, 0.45, 0.03),
        EnemyVoice::Swarm => (310.0, 67.0, 0.55, 0.2, 0.8, 0.5),
        EnemyVoice::Spitter => (190.0, 33.0, 0.8, 0.4, 0.6, 0.7),
        EnemyVoice::Exploder => (76.0, 27.0, 0.9, 1.4, 0.7, 0.3),
    };
    Throat {
        body,
        flutter,
        scale,
        low,
        mid,
        hiss,
    }
}

fn enemy(
    bank: BankId,
    voice: EnemyVoice,
    stage: EnemyStage,
    variant: u64,
    shift: f32,
    variation: f32,
) -> Vec<f32> {
    let v = throat(voice);
    let seconds = v.scale
        * match stage {
            EnemyStage::Windup => 0.5,
            EnemyStage::Contact => 0.34,
            EnemyStage::Death => 0.66,
        };
    // When the voice's own mark sounds within the clip.
    let mark = match stage {
        EnemyStage::Windup => seconds * 0.55,
        EnemyStage::Contact => 0.004,
        EnemyStage::Death => 0.02,
    };
    render(bank, variant, seconds, |t, noise| {
        let [white, low, mid] = noise;
        let growl =
            (low * v.low + mid * v.mid + (white - mid) * v.hiss) * rough(t, v.flutter * shift);
        let stage_sound = match stage {
            // A swelling growl that rises: the attack is coming.
            EnemyStage::Windup => {
                let swell = (t / seconds).powf(1.5);
                growl * swell + glide(t, v.body * shift, 0.35, seconds) * swell * 0.1
            }
            // The blow lands: a thud and a short bark.
            EnemyStage::Contact => {
                knock(
                    t,
                    0.004,
                    low * 2.0 + mid * 0.3,
                    v.body * 0.8 * shift,
                    0.04 * v.scale,
                ) * 0.9
                    + growl * pulse(t, 0.01, 0.1) * 0.8
            }
            // A falling growl and the body dropping.
            EnemyStage::Death => {
                let fall = pulse(t, 0.0, 0.2 * v.scale) * (t / 0.02).min(1.0);
                growl * fall
                    + glide(t, v.body * shift, -0.45, seconds) * fall * 0.12
                    + knock(
                        t,
                        0.3 * v.scale + variation * 0.004,
                        low * 2.0,
                        v.body * 0.6,
                        0.05,
                    ) * 0.5
            }
        };
        let own = match voice {
            // Plates knocking together.
            EnemyVoice::Armored => knock(t, mark, white, 1720.0 * shift, 0.014) * 0.3,
            // Many small legs.
            EnemyVoice::Swarm => {
                (0..5)
                    .map(|n| {
                        let n = n as f32;
                        knock(
                            t,
                            mark + n * 0.021 + variation * 0.001,
                            white,
                            2900.0 + n * 430.0,
                            0.004,
                        )
                    })
                    .sum::<f32>()
                    * 0.25
            }
            // A wet hiss.
            EnemyVoice::Spitter => (white - mid) * band(t, mark, mark + 0.14) * 0.35,
            // A fizzing crackle through the whole clip.
            EnemyVoice::Exploder => {
                if white.abs() > 0.97 {
                    white * 0.3 * band(t, 0.0, seconds)
                } else {
                    0.0
                }
            }
            // The ground carrying its weight.
            EnemyVoice::Demolisher => low * 1.5 * band(t, 0.0, seconds),
            EnemyVoice::Shambler | EnemyVoice::Runner | EnemyVoice::Brute => 0.0,
        };
        stage_sound + own
    })
}

fn boss(
    bank: BankId,
    kind: BossKind,
    stage: BossStage,
    variant: u64,
    shift: f32,
    variation: f32,
) -> Vec<f32> {
    let (body, flutter) = match kind {
        BossKind::Breacher => (58.0, 13.0),
        BossKind::Caller => (131.0, 9.0),
        BossKind::SiegeBrute => (43.0, 7.0),
    };
    let seconds = match stage {
        BossStage::Arrival => 1.6,
        BossStage::Phase => 1.1,
        BossStage::Death => 1.8,
    };
    render(bank, variant, seconds, |t, noise| {
        let [white, low, mid] = noise;
        let thud =
            |onset: f32, hz: f32, decay: f32| knock(t, onset, low * 2.0 + mid * 0.3, hz, decay);
        let growl = (low * 2.3 + mid * 0.5) * rough(t, flutter * shift);
        let stage_sound = match stage {
            // A rumble that fades in under two heavy steps.
            BossStage::Arrival => {
                let fade = (t / 0.25).min(1.0) * ((seconds - t) / 0.5).min(1.0);
                growl * fade * 0.8
                    + glide(t, body * shift, 0.2, seconds) * fade * 0.08
                    + thud(0.35, body * 0.7, 0.09) * 0.8
                    + thud(0.82 + variation * 0.01, body * 0.65, 0.09) * 0.6
            }
            // One hard blow, then a rasp that climbs.
            BossStage::Phase => {
                thud(0.01, body * shift, 0.08)
                    + (white - mid) * pulse(t, 0.005, 0.02) * 0.3
                    + mid
                        * rough(t, flutter * 2.0 * shift)
                        * band(t, 0.12, seconds)
                        * (t / seconds)
                        * 0.9
            }
            // A long falling growl and the collapse.
            BossStage::Death => {
                let fall = pulse(t, 0.0, 0.5) * (t / 0.03).min(1.0);
                growl * fall
                    + glide(t, body * shift, -0.5, seconds) * fall * 0.1
                    + [0.5, 0.82, 1.2]
                        .into_iter()
                        .enumerate()
                        .map(|(n, onset)| {
                            let n = n as f32;
                            thud(onset + variation * 0.006, body * 0.6 - n * 6.0, 0.07)
                                / (1.0 + n * 0.6)
                        })
                        .sum::<f32>()
                        * 0.7
            }
        };
        let own = match kind {
            // Tools on metal.
            BossKind::Breacher => {
                knock(t, 0.36, white, 620.0 * shift, 0.02) * 0.25
                    + knock(t, 0.84, white, 790.0 * shift, 0.016) * 0.18
            }
            // A broad call: a noise band that swells and flutters.
            BossKind::Caller => mid * rough(t, 9.0) * band(t, 0.15, seconds * 0.85) * 0.7,
            // A third, heavier step.
            BossKind::SiegeBrute => thud(seconds * 0.7, body * 0.6, 0.11) * 0.7,
        };
        stage_sound + own
    })
}

fn ui(bank: BankId, family: UiFamily, variant: u64, shift: f32, variation: f32) -> Vec<f32> {
    match family {
        // A soft breath of noise, then two clinks that share no interval.
        UiFamily::LootReveal => render(bank, variant, 0.52, |t, noise| {
            let [white, low, mid] = noise;
            mid * band(t, 0.01, 0.3) * 0.5
                + low * band(t, 0.01, 0.3) * 0.8
                + knock(t, 0.27, white, 2630.0 * shift, 0.016) * 0.45
                + knock(t, 0.325 + variation * 0.003, white, 3970.0 * shift, 0.012) * 0.3
                + mid * pulse(t, 0.27, 0.07) * 0.3
        }),
        // A handful of coins: irregular inharmonic clinks over a soft rustle.
        UiFamily::CoinBatch => render(bank, variant, 0.46, |t, noise| {
            let [white, _, mid] = noise;
            [
                (0.006, 3100.0),
                (0.049, 4270.0),
                (0.081, 2390.0),
                (0.137, 5130.0),
                (0.171, 3650.0),
                (0.232, 2810.0),
            ]
            .into_iter()
            .enumerate()
            .map(|(n, (onset, hz))| {
                let n = n as f32;
                knock(t, onset + variation * 0.002 * n, white, hz * shift, 0.011) / (1.0 + n * 0.25)
            })
            .sum::<f32>()
                + mid * band(t, 0.0, 0.4) * 0.35
        }),
    }
}
