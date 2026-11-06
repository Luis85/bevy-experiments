//! Original survival SFX metadata and reproducible offline synthesis.
//! Only presentation time selects/coalesces variants. No combat RNG is read.

pub const MAX_EFFECT_VOICES: usize = 6;

#[derive(bevy::prelude::Message, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Cue {
    Pistol,
    Smg,
    Shotgun,
    Rifle,
    Marksman,
    MachineGun,
    NpcShot,
    TurretShot,
    Impact,
    Armor,
    Reload,
    Throw,
    Explosion,
    Molotov,
    Fire,
    Retreat,
    Boss,
    Wave,
    Damage,
    Melee,
    Ready,
    Structure,
    Trap,
    Incoming,
    // Presentation cues that reuse a synthesized family's bank (see `bank`).
    // They come last so the synthesized families keep their seeds.
    /// An enemy died.
    Kill,
    /// Attempt-finished stingers. A withdrawal plays `Retreat`.
    Victory,
    Defeat,
    /// A purchase, training or equip command succeeded.
    Confirm,
}
const CUE_COUNT: usize = 28;
impl Cue {
    /// The synthesized family whose WAV variants this cue plays. Alias cues
    /// differ from their bank only in priority, gain and repeat gating.
    pub fn bank(self) -> Self {
        match self {
            Self::Kill => Self::Impact,
            Self::Victory => Self::Wave,
            Self::Defeat => Self::Structure,
            Self::Confirm => Self::Ready,
            cue => cue,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Kill | Self::Victory | Self::Defeat | Self::Confirm => self.bank().name(),
            Self::Pistol => "pistol",
            Self::Smg => "smg",
            Self::Shotgun => "shotgun",
            Self::Rifle => "rifle",
            Self::Marksman => "marksman",
            Self::MachineGun => "machinegun",
            Self::NpcShot => "npc_shot",
            Self::TurretShot => "turret_shot",
            Self::Impact => "impact",
            Self::Armor => "armor",
            Self::Reload => "reload",
            Self::Throw => "throw",
            Self::Explosion => "explosion",
            Self::Molotov => "molotov",
            Self::Fire => "fire",
            Self::Retreat => "retreat",
            Self::Boss => "boss",
            Self::Wave => "wave",
            Self::Damage => "damage",
            Self::Melee => "melee",
            Self::Ready => "ready",
            Self::Structure => "structure",
            Self::Trap => "trap",
            Self::Incoming => "incoming",
        }
    }
    /// The existing master/effects bus multiplies this per-voice attenuation.
    pub fn gain(self, reduced: bool) -> f32 {
        let gain = match self {
            Self::Explosion | Self::Boss | Self::Shotgun => 1.0,
            Self::Pistol | Self::Rifle | Self::Marksman | Self::MachineGun => 0.9,
            Self::Victory | Self::Defeat => 0.8,
            Self::Smg | Self::NpcShot | Self::Damage | Self::Structure | Self::Trap => 0.75,
            Self::TurretShot | Self::Melee | Self::Molotov | Self::Retreat | Self::Wave => 0.65,
            Self::Impact | Self::Reload | Self::Throw => 0.55,
            // Quieter than a hit: a horde's kills must not mask the guns.
            Self::Kill => 0.5,
            Self::Armor | Self::Ready | Self::Confirm => 0.45,
            Self::Fire | Self::Incoming => 0.3,
        };
        gain * if reduced { 0.65 } else { 1.0 }
    }
    pub fn cooldown_ms(self, reduced: bool) -> u64 {
        let interval = match self {
            Self::Smg | Self::MachineGun => 55,
            Self::Pistol | Self::Rifle => 80,
            Self::Shotgun | Self::Marksman => 120,
            Self::NpcShot => 100,
            Self::TurretShot => 150,
            Self::Impact => 85,
            Self::Armor => 140,
            Self::Reload | Self::Throw | Self::Explosion | Self::Molotov | Self::Melee => 180,
            Self::Fire => 850,
            Self::Retreat => 1500,
            Self::Boss | Self::Victory | Self::Defeat => 3000,
            Self::Kill => 160,
            Self::Confirm => 250,
            Self::Wave => 1800,
            Self::Damage => 240,
            Self::Ready => 400,
            Self::Structure => 550,
            Self::Trap => 220,
            Self::Incoming => 2400,
        };
        if reduced { interval * 3 / 2 } else { interval }
    }
}
pub fn variant_path(cue: Cue, variant: u64) -> String {
    format!("audio/survival_{}_{}.wav", cue.name(), variant % 3)
}

/// Every WAV the runtime can load, relative to `assets/`: each synthesized
/// family's three variants (alias cues play these banks) plus the ambience
/// beds. The asset audit holds `assets/audio` to exactly this set.
#[cfg(test)]
pub(crate) fn runtime_wav_paths() -> Vec<String> {
    let mut paths: Vec<String> = synthesis::ALL
        .iter()
        .flat_map(|&cue| (0..3).map(move |variant| variant_path(cue, variant)))
        .collect();
    paths.extend(
        crate::atmosphere_audio::AmbienceChannel::ALL
            .iter()
            .map(|channel| channel.path().to_owned()),
    );
    paths
}

/// Coalesce repeated ticks and accelerated observation into a restrained real-
/// time soundscape. A cue dropped by the gate never delays a simulation action.
#[derive(Debug, Default)]
pub struct CueGate {
    last_started: [Option<u64>; CUE_COUNT],
}
impl CueGate {
    pub fn accept(&mut self, cue: Cue, now_ms: u64, reduced: bool) -> bool {
        let last = &mut self.last_started[cue as usize];
        if last.is_some_and(|previous| {
            now_ms >= previous && now_ms - previous < cue.cooldown_ms(reduced)
        }) {
            return false;
        }
        *last = Some(now_ms);
        true
    }
}

// Synthesis is an offline authoring tool, never a packaged runtime write. The
// ignored export test regenerates assets plus the documented audition reel.
#[cfg(test)]
mod synthesis {
    use super::Cue;
    use std::{
        f32::consts::{PI, TAU},
        io::Write,
        path::Path,
    };
    pub const RATE: u32 = 44_100;
    pub const PEAK: f32 = 0.18;
    pub const ALL: [Cue; 24] = [
        Cue::Pistol,
        Cue::Smg,
        Cue::Shotgun,
        Cue::Rifle,
        Cue::Marksman,
        Cue::MachineGun,
        Cue::NpcShot,
        Cue::TurretShot,
        Cue::Impact,
        Cue::Armor,
        Cue::Reload,
        Cue::Throw,
        Cue::Explosion,
        Cue::Molotov,
        Cue::Fire,
        Cue::Retreat,
        Cue::Boss,
        Cue::Wave,
        Cue::Damage,
        Cue::Melee,
        Cue::Ready,
        Cue::Structure,
        Cue::Trap,
        Cue::Incoming,
    ];
    struct Noise {
        state: u64,
        low: f32,
        mid: f32,
    }
    impl Noise {
        fn next(&mut self) -> [f32; 3] {
            self.state ^= self.state << 13;
            self.state ^= self.state >> 7;
            self.state ^= self.state << 17;
            let white = ((self.state >> 40) as f32 / 8_388_607.5) - 1.0;
            self.low += 0.014 * (white - self.low);
            self.mid += 0.19 * (white - self.mid);
            [white, self.low, self.mid]
        }
    }
    fn pulse(t: f32, onset: f32, decay: f32) -> f32 {
        if t >= onset {
            (-(t - onset) / decay).exp()
        } else {
            0.0
        }
    }
    fn knock(t: f32, onset: f32, noise: f32, frequency: f32, decay: f32) -> f32 {
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
            Cue::Kill | Cue::Victory | Cue::Defeat | Cue::Confirm => duration(cue.bank()),
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
        let body =
            ((TAU * body_hz * shift * t).sin() * 0.36 + low * 2.1) * pulse(t, 0.0, body_decay);
        let room = mid * (pulse(t, 0.035, tail) * 0.32 + pulse(t, 0.081, tail * 0.7) * 0.13);
        let bolt = if cue == Cue::TurretShot {
            knock(t, 0.037, white, 940.0, 0.012) * 0.15
        } else {
            0.0
        };
        crack + body + room + bolt
    }
    pub fn synthesize(cue: Cue, variant: u64) -> Vec<f32> {
        let seconds = duration(cue);
        let count = (seconds * RATE as f32).round() as usize;
        let mut source = Noise {
            state: 0x85eb_ca6b_u64.wrapping_mul(1 + cue as u64 * 17 + variant % 3),
            low: 0.0,
            mid: 0.0,
        };
        let variation = (variant % 3) as f32;
        let shift = 0.96 + variation * 0.04;
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
                Cue::Impact => {
                    knock(t, 0.004, mid, 97.0 * shift, 0.025) + low * pulse(t, 0.008, 0.07)
                }
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
                    let mechanical = (TAU * 67.0 * shift * t).sin() * 0.13
                        + (TAU * 103.0 * shift * t).sin() * 0.055;
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
                Cue::Kill | Cue::Victory | Cue::Defeat | Cue::Confirm => {
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
            let edge =
                (index as f32 / 96.0).min(1.0) * ((count - 1 - index) as f32 / 256.0).min(1.0);
            *sample = (*sample - mean) * edge;
        }
        let peak = result.iter().map(|s| s.abs()).fold(0.0, f32::max);
        for sample in &mut result {
            *sample *= PEAK / peak.max(0.0001);
        }
        result
    }
    pub fn wav(path: &Path, samples: &[f32]) -> std::io::Result<()> {
        let bytes = samples.len() as u32 * 2;
        let mut output = std::io::BufWriter::new(std::fs::File::create(path)?);
        output.write_all(b"RIFF")?;
        output.write_all(&(36 + bytes).to_le_bytes())?;
        output.write_all(b"WAVEfmt ")?;
        output.write_all(&16_u32.to_le_bytes())?;
        output.write_all(&1_u16.to_le_bytes())?;
        output.write_all(&1_u16.to_le_bytes())?;
        output.write_all(&RATE.to_le_bytes())?;
        output.write_all(&(RATE * 2).to_le_bytes())?;
        output.write_all(&2_u16.to_le_bytes())?;
        output.write_all(&16_u16.to_le_bytes())?;
        output.write_all(b"data")?;
        output.write_all(&bytes.to_le_bytes())?;
        for sample in samples {
            output
                .write_all(&((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes())?;
        }
        output.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::{synthesis::*, *};
    #[test]
    fn every_variant_is_repeatable_click_free_and_has_mix_headroom() {
        let mut measured_peak = 0.0_f32;
        for cue in ALL {
            for variant in 0..3 {
                let samples = synthesize(cue, variant);
                assert_eq!(samples, synthesize(cue, variant));
                assert!(
                    samples
                        .iter()
                        .all(|v| v.is_finite() && v.abs() <= PEAK + 0.000001)
                );
                measured_peak = samples
                    .iter()
                    .map(|s| s.abs())
                    .fold(measured_peak, f32::max);
                assert_eq!(samples[0], 0.0);
                assert_eq!(*samples.last().unwrap(), 0.0);
                let rms =
                    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
                assert!(
                    (0.003..0.10).contains(&rms),
                    "{cue:?} variant {variant} rms {rms}"
                );
                let mean = samples.iter().sum::<f32>() / samples.len() as f32;
                assert!(mean.abs() < 0.001, "{cue:?} mean {mean}");
            }
            assert_ne!(synthesize(cue, 0), synthesize(cue, 1));
            assert!(cue.gain(true) < cue.gain(false));
        }
        assert!(measured_peak * MAX_EFFECT_VOICES as f32 * 0.7 <= 0.76);
    }
    #[test]
    fn weapon_reports_have_distinct_duration_and_energy_profiles() {
        let pistol = synthesize(Cue::Pistol, 0);
        let smg = synthesize(Cue::Smg, 0);
        let shotgun = synthesize(Cue::Shotgun, 0);
        let marksman = synthesize(Cue::Marksman, 0);
        assert!(
            smg.len() < pistol.len()
                && pistol.len() < shotgun.len()
                && shotgun.len() < marksman.len()
        );
        let late_energy = |samples: &[f32]| {
            samples
                .iter()
                .skip((RATE as f32 * 0.12) as usize)
                .map(|s| s * s)
                .sum::<f32>()
        };
        assert!(late_energy(&shotgun) > late_energy(&smg) * 2.0);
    }
    #[test]
    fn real_time_gate_bounds_accelerated_feedback_and_keeps_critical_cues_independent() {
        let mut gate = CueGate::default();
        assert!(gate.accept(Cue::Fire, 0, false));
        assert!(!gate.accept(Cue::Fire, 1, false));
        assert!(gate.accept(Cue::Boss, 1, false));
        assert!(!gate.accept(Cue::Boss, 1000, false));
        assert!(gate.accept(Cue::Fire, 850, false));
        assert!(!gate.accept(Cue::Fire, 1700, true));
        assert!(gate.accept(Cue::Fire, 2125, true));
        assert_eq!(
            variant_path(Cue::Explosion, 7),
            "audio/survival_explosion_1.wav"
        );
    }
    #[test]
    fn alias_cues_play_existing_synthesized_banks() {
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        for (alias, bank) in [
            (Cue::Kill, Cue::Impact),
            (Cue::Victory, Cue::Wave),
            (Cue::Defeat, Cue::Structure),
            (Cue::Confirm, Cue::Ready),
        ] {
            assert_eq!(alias.bank(), bank);
            assert!(
                !ALL.contains(&alias),
                "{alias:?} must not be re-synthesized"
            );
            assert!(ALL.contains(&bank));
            for variant in 0..3 {
                let path = variant_path(alias, variant);
                assert_eq!(path, variant_path(bank, variant));
                assert!(assets.join(&path).is_file(), "missing {path}");
            }
            assert!(alias.gain(true) < alias.gain(false));
        }
        assert!(Cue::Kill.gain(false) < Cue::Impact.gain(false));
        // A horde's kills are rate limited like other repeated feedback.
        let mut gate = CueGate::default();
        assert!(gate.accept(Cue::Kill, 0, false));
        assert!(!gate.accept(Cue::Kill, 100, false));
        assert!(gate.accept(Cue::Impact, 100, false));
        assert!(gate.accept(Cue::Kill, 160, false));
        assert!(gate.accept(Cue::Confirm, 160, false));
    }
    #[test]
    #[ignore = "Offline asset authoring: explicitly set LASTLIGHT_AUDIO_EXPORT to workspace root"]
    fn export_survival_audio_assets_and_audition() {
        let root = std::path::PathBuf::from(
            std::env::var_os("LASTLIGHT_AUDIO_EXPORT").expect("Set LASTLIGHT_AUDIO_EXPORT"),
        );
        let assets = root.join("assets/audio");
        let audition = root.join("docs/audio");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::create_dir_all(&audition).unwrap();
        let mut reel = Vec::new();
        let mut index = String::from("cue,start_seconds,duration_seconds\n");
        for cue in ALL {
            for variant in 0..3 {
                let samples = synthesize(cue, variant);
                let path = root.join("assets").join(variant_path(cue, variant));
                wav(&path, &samples).unwrap();
                if variant == 0 {
                    index.push_str(&format!(
                        "{:?},{:.3},{:.3}\n",
                        cue,
                        reel.len() as f32 / RATE as f32,
                        samples.len() as f32 / RATE as f32
                    ));
                    reel.extend(samples);
                    reel.resize(reel.len() + RATE as usize / 3, 0.0);
                }
            }
        }
        wav(&audition.join("survival-audition.wav"), &reel).unwrap();
        std::fs::write(audition.join("survival-audition.csv"), index).unwrap();
    }
}
