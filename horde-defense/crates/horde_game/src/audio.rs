//! The Bevy side of the survival SFX: the cue message.
//! The cue list, banks, the mix table and the offline synthesis live in
//! `horde_audio`. Only presentation time selects/coalesces variants.
//! No combat RNG is read.

pub use horde_audio::{bank::variant_path, cue::Cue, mix::MAX_EFFECT_VOICES, select::CueGate};

/// One requested cue, written by the runner and the UI and played by
/// `SfxPlugin`.
#[derive(bevy::prelude::Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CueRequest(pub Cue);

/// Every WAV the runtime can load, relative to `assets/`: every variant of
/// every shipped bank (alias cues play these banks; retired cues are
/// archived) plus the ambience beds. The asset audit holds `assets/audio` to
/// exactly this set. The wave-6 gun, enemy, boss and UI banks ship from A6-05
/// on; the cues that play them arrive with A6-12 to A6-14.
#[cfg(test)]
pub(crate) fn runtime_wav_paths() -> Vec<String> {
    let mut paths: Vec<String> = horde_audio::bank::BankId::shipped()
        .into_iter()
        .flat_map(|bank| (0..bank.variants()).map(move |variant| bank.variant_path(variant)))
        .collect();
    paths.extend(
        crate::atmosphere_audio::AmbienceChannel::ALL
            .iter()
            .map(|channel| channel.path().to_owned()),
    );
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use horde_audio::{bank::LEGACY_FAMILIES, cue::CUE_COUNT, mix::mix};
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
            "audio/survival_explosion_2.wav"
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
            for variant in 0..bank.bank().variants() {
                let path = variant_path(alias, variant);
                assert_eq!(path, variant_path(bank, variant));
                assert!(assets.join(&path).is_file(), "missing {path}");
            }
            assert!(mix(alias).level(true) < mix(alias).level(false));
        }
        assert!(mix(Cue::Kill).level(false) < mix(Cue::Impact).level(false));
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
