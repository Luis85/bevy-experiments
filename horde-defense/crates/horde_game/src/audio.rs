//! The Bevy side of the survival SFX: the cue message, the legacy per-cue mix
//! numbers and the repeat gate. The cue list, banks and the offline synthesis
//! live in `horde_audio`. Only presentation time selects/coalesces variants.
//! No combat RNG is read.

pub use horde_audio::{bank::variant_path, cue::Cue};

pub const MAX_EFFECT_VOICES: usize = 6;

/// One requested cue, written by the runner and the UI and played by
/// `SfxPlugin`.
#[derive(bevy::prelude::Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CueRequest(pub Cue);

/// The per-cue mix numbers, still owned here until the mix table moves to
/// `horde_audio` (A6-02).
pub trait CueMix {
    fn gain(self, reduced: bool) -> f32;
    fn cooldown_ms(self, reduced: bool) -> u64;
}
impl CueMix for Cue {
    /// The existing master/effects bus multiplies this per-voice attenuation.
    fn gain(self, reduced: bool) -> f32 {
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
    fn cooldown_ms(self, reduced: bool) -> u64 {
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

/// Every WAV the runtime can load, relative to `assets/`: each live
/// synthesized family's three variants (alias cues play these banks; retired
/// cues are archived) plus the ambience beds. The asset audit holds
/// `assets/audio` to exactly this set.
#[cfg(test)]
pub(crate) fn runtime_wav_paths() -> Vec<String> {
    let mut paths: Vec<String> = horde_audio::synth::legacy::shipped()
        .flat_map(|cue| (0..3).map(move |variant| variant_path(cue, variant)))
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
    last_started: [Option<u64>; horde_audio::cue::CUE_COUNT],
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

#[cfg(test)]
mod tests {
    use super::*;
    use horde_audio::{
        bank::LEGACY_FAMILIES,
        cue::CUE_COUNT,
        synth::legacy::{PEAK, shipped},
    };
    #[test]
    fn every_cue_is_quieter_reduced_and_six_voices_keep_mix_headroom() {
        for cue in Cue::ALL {
            assert!(cue.gain(true) < cue.gain(false), "{cue:?}");
            assert!(cue.cooldown_ms(true) > cue.cooldown_ms(false), "{cue:?}");
        }
        // Every shipped clip is normalised to PEAK
        // (horde_audio: every_variant_is_repeatable_click_free_and_within_peak).
        assert!(shipped().count() > 0);
        assert!(PEAK * MAX_EFFECT_VOICES as f32 * 0.7 <= 0.76);
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
            assert_eq!(alias.bank(), bank.bank());
            assert!(
                !LEGACY_FAMILIES.contains(&alias),
                "{alias:?} must not be re-synthesized"
            );
            assert!(LEGACY_FAMILIES.contains(&bank));
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
    fn retired_cues_have_no_runtime_path_and_no_shipped_file() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let every: Vec<Cue> = LEGACY_FAMILIES
            .into_iter()
            .chain([Cue::Kill, Cue::Victory, Cue::Defeat, Cue::Confirm])
            .collect();
        assert_eq!(every.len(), CUE_COUNT);
        let retired: Vec<Cue> = every.iter().copied().filter(|cue| cue.retired()).collect();
        assert_eq!(retired, vec![Cue::NpcShot]);
        let runtime = runtime_wav_paths();
        for cue in &every {
            // No live cue plays a retired bank.
            assert!(cue.retired() || !cue.legacy_family().retired(), "{cue:?}");
        }
        for cue in retired {
            for variant in 0..3 {
                let path = variant_path(cue, variant);
                assert!(!runtime.contains(&path), "{path} is a runtime path");
                assert!(
                    !root.join("assets").join(&path).exists(),
                    "assets/{path} still ships"
                );
                assert!(
                    root.join("archive").join(&path).is_file(),
                    "archive/{path} is missing"
                );
            }
        }
    }
}
