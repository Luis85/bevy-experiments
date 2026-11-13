//! The v1 survival synthesis, moved verbatim from `horde_game::audio`.
//!
//! Synthesis is an offline authoring tool, never a packaged runtime write:
//! `examples/export_audio.rs` masters these raw clips into the shipped files
//! (`crate::export`). `raw_synthesis_matches_v1_library`
//! holds every sample of variants 0-2 to the sha256 recorded when the v1
//! library was exported, so nothing in this file may change a produced sample.
use crate::{bank::LEGACY_FAMILIES, cue::Cue};
use std::f32::consts::{PI, TAU};

pub const RATE: u32 = 44_100;
pub const PEAK: f32 = 0.18;
/// Variants per v1 family: the three the v1 library shipped.
pub const VARIANTS: u64 = 3;
/// The pitch shift of each variant. Variants 0-2 are the v1 shifts (0.96 +
/// 0.04 per variant); 3 and 4 were added by the wave-6 exporter.
pub const SHIFTS: [f32; 5] = [0.96, 1.00, 1.04, 0.92, 1.08];

/// The synthesized families that ship: [`LEGACY_FAMILIES`] minus retired
/// cues. `LEGACY_FAMILIES` keeps every family so seeds and the archived files
/// stay reproducible.
pub fn shipped() -> impl Iterator<Item = Cue> {
    LEGACY_FAMILIES.into_iter().filter(|cue| !cue.retired())
}

pub(crate) struct Noise {
    pub(crate) state: u64,
    pub(crate) low: f32,
    pub(crate) mid: f32,
}
impl Noise {
    pub(crate) fn next(&mut self) -> [f32; 3] {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        let white = ((self.state >> 40) as f32 / 8_388_607.5) - 1.0;
        self.low += 0.014 * (white - self.low);
        self.mid += 0.19 * (white - self.mid);
        [white, self.low, self.mid]
    }
}
pub(crate) fn pulse(t: f32, onset: f32, decay: f32) -> f32 {
    if t >= onset {
        (-(t - onset) / decay).exp()
    } else {
        0.0
    }
}
pub(crate) fn knock(t: f32, onset: f32, noise: f32, frequency: f32, decay: f32) -> f32 {
    let age = (t - onset).max(0.0);
    pulse(t, onset, decay) * (0.65 * noise + 0.35 * (TAU * frequency * age).sin())
}
fn duration(cue: Cue) -> f32 {
    match cue {
        Cue::Pistol => 0.36,
        Cue::Smg => 0.22,
        Cue::Shotgun => 0.62,
        Cue::Rifle => 0.46,
        Cue::Marksman => 0.78,
        Cue::MachineGun => 0.37,
        Cue::NpcShot => 0.42,
        Cue::TurretShot => 0.4,
        Cue::Impact => 0.24,
        Cue::Armor => 0.38,
        Cue::Reload => 0.86,
        Cue::Throw => 0.56,
        Cue::Explosion | Cue::Trap => 1.5,
        Cue::Molotov => 0.76,
        Cue::Fire => 1.15,
        Cue::Retreat => 0.9,
        Cue::Boss => 1.8,
        Cue::Wave => 0.72,
        Cue::Damage => 0.43,
        Cue::Melee => 0.5,
        Cue::Ready => 0.34,
        Cue::Structure => 1.2,
        Cue::Incoming => 0.3,
        // Not v1 families: they play banks and are never synthesized here.
        Cue::Kill
        | Cue::Victory
        | Cue::Defeat
        | Cue::Confirm
        | Cue::EnemyWindup
        | Cue::EnemyContact
        | Cue::EnemyDeath
        | Cue::BossArrival
        | Cue::BossPhase
        | Cue::BossDeath
        | Cue::BossExposed
        | Cue::Block
        | Cue::Ability
        | Cue::DryFire
        | Cue::ReloadOut
        | Cue::ReloadIn
        | Cue::ShellInsert
        | Cue::BoltCycle => duration(cue.legacy_family()),
    }
}
fn gun(cue: Cue, t: f32, noise: [f32; 3], shift: f32) -> f32 {
    let (body_hz, body_decay, crack_decay, tail) = match cue {
        Cue::Pistol => (115.0, 0.028, 0.006, 0.09),
        Cue::Smg => (185.0, 0.018, 0.004, 0.045),
        Cue::Shotgun => (64.0, 0.072, 0.017, 0.19),
        Cue::Rifle => (142.0, 0.036, 0.004, 0.13),
        Cue::Marksman => (90.0, 0.053, 0.005, 0.23),
        Cue::MachineGun | Cue::TurretShot => (89.0, 0.041, 0.009, 0.1),
        _ => (127.0, 0.038, 0.008, 0.13),
    };
    let [white, low, mid] = noise;
    let crack = (white - mid) * pulse(t, 0.001, crack_decay) * 0.9;
    let body = ((TAU * body_hz * shift * t).sin() * 0.36 + low * 2.1) * pulse(t, 0.0, body_decay);
    let room = mid * (pulse(t, 0.035, tail) * 0.32 + pulse(t, 0.081, tail * 0.7) * 0.13);
    let bolt = if cue == Cue::TurretShot {
        knock(t, 0.037, white, 940.0, 0.012) * 0.15
    } else {
        0.0
    };
    crack + body + room + bolt
}
/// One family's variant as mono samples at [`RATE`], peak-normalised to
/// [`PEAK`]. `cue` is a synthesized family ([`LEGACY_FAMILIES`]); alias cues
/// play their bank's files and are never synthesized. `variant` indexes
/// [`SHIFTS`]; variants 0-2 are the v1 library, sample for sample.
pub fn synthesize(cue: Cue, variant: u64) -> Vec<f32> {
    let seconds = duration(cue);
    let count = (seconds * RATE as f32).round() as usize;
    let mut source = Noise {
        state: crate::bank::seed(cue.bank(), variant),
        low: 0.0,
        mid: 0.0,
    };
    let variation = variant as f32;
    // The v1 expression for variants 0-2, kept so their samples cannot move.
    let shift = if variant < VARIANTS {
        0.96 + variation * 0.04
    } else {
        SHIFTS[variant as usize]
    };
    let mut result = Vec::with_capacity(count);
    for index in 0..count {
        let t = index as f32 / RATE as f32;
        let noise = source.next();
        let [white, low, mid] = noise;
        let value = match cue {
            Cue::Pistol
            | Cue::Smg
            | Cue::Shotgun
            | Cue::Rifle
            | Cue::Marksman
            | Cue::MachineGun
            | Cue::NpcShot
            | Cue::TurretShot => gun(cue, t, noise, shift),
            Cue::Impact => knock(t, 0.004, mid, 97.0 * shift, 0.025) + low * pulse(t, 0.008, 0.07),
            Cue::Armor => {
                knock(t, 0.002, white, 1720.0 * shift, 0.014) * 0.55
                    + ((TAU * 2300.0 * shift * t).sin() + (TAU * 3173.0 * t).sin() * 0.3)
                        * pulse(t, 0.0, 0.045)
                        * 0.1
            }
            Cue::Reload => {
                knock(t, 0.015, mid, 780.0 * shift, 0.013) * 0.7
                    + knock(t, 0.24, white, 1230.0 * shift, 0.009) * 0.25
                    + knock(t, 0.55, mid, 620.0 * shift, 0.018)
                    + if (0.29..0.49).contains(&t) {
                        (white - mid) * (PI * (t - 0.29) / 0.2).sin() * 0.045
                    } else {
                        0.0
                    }
            }
            Cue::Throw => {
                knock(t, 0.018, mid, 1100.0 * shift, 0.008) * 0.32
                    + if (0.07..0.42).contains(&t) {
                        mid * (PI * (t - 0.07) / 0.35).sin().powi(2) * 0.5
                    } else {
                        0.0
                    }
            }
            Cue::Explosion | Cue::Trap => {
                (white - mid) * pulse(t, 0.001, 0.014) * 0.6
                    + low * pulse(t, 0.0, 0.27) * 3.5
                    + (TAU * 53.0 * shift * t).sin() * pulse(t, 0.0, 0.085) * 0.3
                    + mid * (pulse(t, 0.055, 0.18) * 0.35 + pulse(t, 0.17, 0.17) * 0.16)
                    + knock(t, 0.22, mid, 440.0, 0.027) * 0.06
            }
            Cue::Molotov => {
                [0.001, 0.017, 0.041, 0.073, 0.12, 0.19]
                    .into_iter()
                    .enumerate()
                    .map(|(n, onset)| {
                        knock(
                            t,
                            onset + variation * 0.001,
                            white,
                            (2400.0 + n as f32 * 511.0) * shift,
                            0.005 + n as f32 * 0.0015,
                        ) / (1.0 + n as f32 * 0.7)
                    })
                    .sum::<f32>()
                    + mid * pulse(t, 0.025, 0.11) * 0.2
            }
            Cue::Fire => {
                let bed = mid * 0.13 * (0.7 + 0.3 * (TAU * 2.1 * t).sin());
                bed + [0.08, 0.31, 0.54, 0.81]
                    .into_iter()
                    .enumerate()
                    .map(|(n, onset)| {
                        knock(
                            t,
                            onset + variation * 0.013,
                            white,
                            1300.0 + n as f32 * 370.0,
                            0.003,
                        ) * 0.12
                    })
                    .sum::<f32>()
            }
            Cue::Retreat | Cue::Wave | Cue::Incoming => {
                // Short squelch and relay clicks: radio/equipment, no melody.
                let spacing = if cue == Cue::Retreat { 0.26 } else { 0.14 };
                knock(t, 0.015, mid, 910.0 * shift, 0.012) * 0.6
                    + knock(t, spacing, mid, 620.0 * shift, 0.018) * 0.42
                    + if (0.05..spacing).contains(&t) {
                        mid * 0.045
                    } else {
                        0.0
                    }
            }
            Cue::Boss => {
                let fade = (t / 0.13).min(1.0) * ((seconds - t) / 0.4).min(1.0);
                let mechanical =
                    (TAU * 67.0 * shift * t).sin() * 0.13 + (TAU * 103.0 * shift * t).sin() * 0.055;
                (mechanical * (0.65 + 0.35 * (TAU * 2.0 * t).sin()) + low * 0.23) * fade
            }
            Cue::Damage => {
                knock(t, 0.001, low * 3.0, 68.0 * shift, 0.046) * 0.8
                    + mid * pulse(t, 0.0, 0.035) * 0.22
            }
            Cue::Melee => {
                let swish = if t < 0.2 {
                    (white - mid) * (PI * t / 0.2).sin().powi(2) * 0.09
                } else {
                    0.0
                };
                swish + knock(t, 0.18 + variation * 0.003, mid, 85.0 * shift, 0.036) * 0.8
            }
            Cue::Ready => {
                knock(t, 0.015, mid, 710.0 * shift, 0.01) * 0.5
                    + knock(t, 0.075, mid, 1020.0 * shift, 0.008) * 0.35
            }
            Cue::Kill
            | Cue::Victory
            | Cue::Defeat
            | Cue::Confirm
            | Cue::EnemyWindup
            | Cue::EnemyContact
            | Cue::EnemyDeath
            | Cue::BossArrival
            | Cue::BossPhase
            | Cue::BossDeath
            | Cue::BossExposed
            | Cue::Block
            | Cue::Ability
            | Cue::DryFire
            | Cue::ReloadOut
            | Cue::ReloadIn
            | Cue::ShellInsert
            | Cue::BoltCycle => {
                unreachable!("alias cues play their bank's files and are never synthesized")
            }
            Cue::Structure => [0.001, 0.14, 0.29, 0.51]
                .into_iter()
                .enumerate()
                .map(|(n, onset)| {
                    knock(
                        t,
                        onset,
                        low * 2.0 + mid * 0.25,
                        80.0 - n as f32 * 11.0,
                        0.07,
                    ) / (1.0 + n as f32 * 0.65)
                })
                .sum(),
        };
        let ramp = (t / 0.0007).min(1.0) * ((seconds - t) / 0.025).min(1.0);
        result.push(value * ramp);
    }
    // Remove DC before a final edge taper; normalized clips share explicit
    // headroom rather than each file maximizing device output.
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
