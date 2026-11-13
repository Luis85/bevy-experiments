//! Pins for the mix table (A6-02). The table is the one place a gain, a
//! cooldown, a priority or a calibration factor may live; `horde_game` only
//! executes it.

use horde_audio::{
    cue::Cue,
    mix::{
        self, Bus, CONTEXT_GAIN, GateGroup, MAX_EFFECT_VOICES, MIX_CALIBRATION_DB, VisualTwin,
        WARNING_TWINS, mix,
    },
    synth::legacy::PEAK,
};
use std::path::{Path, PathBuf};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn source(relative: &str) -> String {
    let path = workspace().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The Bevy adapter's audio sources. They execute the mix; they own no number
/// of it.
const GAME_AUDIO_SOURCES: [&str; 3] = [
    "crates/horde_game/src/audio.rs",
    "crates/horde_game/src/sfx.rs",
    "crates/horde_game/src/atmosphere_audio.rs",
];

#[test]
fn horde_game_audio_sources_define_no_gain_tables() {
    let mut found = Vec::new();
    for file in GAME_AUDIO_SOURCES {
        for (index, line) in source(file).lines().enumerate() {
            for needle in ["fn gain(", "fn cooldown_ms(", "fn priority(", "* 0.7"] {
                if line.contains(needle) {
                    found.push(format!("{file}:{}: `{needle}`", index + 1));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "mix numbers belong in horde_audio::mix:\n{}",
        found.join("\n")
    );
}

/// The v1 numbers as `horde_game` held them before the move: linear gain,
/// cooldown in ms, priority (0 is most important), fire lane.
const V1_ROWS: [(Cue, f32, u32, u8, bool); 28] = [
    (Cue::Pistol, 0.9, 80, 9, true),
    (Cue::Smg, 0.75, 55, 9, true),
    (Cue::Shotgun, 1.0, 120, 9, true),
    (Cue::Rifle, 0.9, 80, 9, true),
    (Cue::Marksman, 0.9, 120, 9, true),
    (Cue::MachineGun, 0.9, 55, 9, true),
    (Cue::NpcShot, 0.75, 100, 10, true),
    (Cue::TurretShot, 0.65, 150, 11, true),
    (Cue::Impact, 0.55, 85, 7, false),
    (Cue::Armor, 0.45, 140, 7, false),
    (Cue::Reload, 0.55, 180, 5, false),
    (Cue::Throw, 0.55, 180, 4, false),
    (Cue::Explosion, 1.0, 180, 2, false),
    (Cue::Molotov, 0.65, 180, 2, false),
    (Cue::Fire, 0.3, 850, 13, false),
    (Cue::Retreat, 0.65, 1500, 0, false),
    (Cue::Boss, 1.0, 3000, 0, false),
    (Cue::Wave, 0.65, 1800, 1, false),
    (Cue::Damage, 0.75, 240, 3, false),
    (Cue::Melee, 0.65, 180, 4, false),
    (Cue::Ready, 0.45, 400, 6, false),
    (Cue::Structure, 0.75, 550, 2, false),
    (Cue::Trap, 0.75, 220, 8, false),
    (Cue::Incoming, 0.3, 2400, 12, false),
    (Cue::Kill, 0.5, 160, 6, false),
    (Cue::Victory, 0.8, 3000, 0, false),
    (Cue::Defeat, 0.8, 3000, 0, false),
    (Cue::Confirm, 0.45, 250, 3, false),
];

#[test]
fn mix_rows_keep_the_v1_numbers() {
    assert_eq!(V1_ROWS.map(|row| row.0), Cue::ALL);
    for (cue, gain, cooldown_ms, priority, fire) in V1_ROWS {
        let row = mix(cue);
        assert_eq!(row.gain, gain, "{cue:?} gain");
        assert_eq!(row.cooldown_ms, cooldown_ms, "{cue:?} cooldown");
        assert_eq!(row.priority, priority, "{cue:?} priority");
        assert_eq!(row.gate == GateGroup::Fire, fire, "{cue:?} gate");
        // The old reduced column: x0.65 gain and a cooldown of 3/2, rounded
        // down.
        assert_eq!(row.level(false), gain, "{cue:?}");
        assert_eq!(row.level(true), gain * 0.65, "{cue:?}");
        assert_eq!(row.cooldown(false), u64::from(cooldown_ms), "{cue:?}");
        assert_eq!(
            row.cooldown(true),
            u64::from(cooldown_ms) * 3 / 2,
            "{cue:?}"
        );
    }
    // The hidden x0.7 on the effects bus is now the calibration knob.
    assert_eq!(MIX_CALIBRATION_DB, -3.1);
    assert!((mix::calibration_gain() - 0.7).abs() < 0.0002);
    assert_eq!(MAX_EFFECT_VOICES, 6);
    assert_eq!(
        [
            CONTEXT_GAIN.menu,
            CONTEXT_GAIN.camp,
            CONTEXT_GAIN.battle,
            CONTEXT_GAIN.paused
        ],
        [0.55, 0.75, 1.0, 0.35]
    );
    assert_eq!(mix::AMBIENCE_CALIBRATION_DB, 0.0);
    assert_eq!(mix::ambience_calibration_gain(), 1.0);
}

#[test]
fn every_cue_has_one_mix_row_bank_and_bus() {
    for cue in Cue::ALL {
        let row = mix(cue);
        // A cue is a voice: it rides Effects, UI or Music (`tests/buses.rs`
        // pins which), never Master or the loops' bus.
        assert!(
            matches!(row.bus, Bus::Effects | Bus::Ui | Bus::Music),
            "{cue:?}"
        );
        assert_eq!(row.retired, cue.retired(), "{cue:?}");
        assert!(row.gain > 0.0 && row.gain <= 1.0, "{cue:?}");
        // The bank is the cue's own or its alias family's.
        assert_eq!(cue.bank(), cue.legacy_family().bank(), "{cue:?}");
    }
}

#[test]
fn every_cue_reduced_gain_below_normal_and_cooldown_not_shorter() {
    for cue in Cue::ALL {
        let row = mix(cue);
        assert!(row.level(true) < row.level(false), "{cue:?}");
        assert!(row.cooldown(true) >= row.cooldown(false), "{cue:?}");
    }
}

/// Carried over from the horde_game v1 assert: six voices at the top row and
/// the calibration stay inside the old headroom.
#[test]
fn six_voices_keep_mix_headroom() {
    assert!(PEAK * MAX_EFFECT_VOICES as f32 * mix::calibration_gain() <= 0.76);
}

#[test]
fn every_warning_row_names_a_visual_twin() {
    assert!(!WARNING_TWINS.is_empty());
    for row in WARNING_TWINS {
        assert!(!row.event.is_empty(), "{row:?}");
        // The predicate reads the field the declaration names.
        let field = row
            .declaration
            .strip_prefix("pub ")
            .and_then(|rest| rest.split(':').next())
            .unwrap_or_else(|| panic!("{row:?}: declaration is not a pub field"));
        assert!(
            row.predicate.contains(&format!(".{field}.")),
            "{row:?}: the predicate does not read `{field}`"
        );
        // And the view model really has that field (the vfx rows W2-28 and
        // W2-37, and the windup ring that predates them).
        assert!(
            source(row.declared_in).contains(row.declaration),
            "{row:?}: `{}` is not in {}",
            row.declaration,
            row.declared_in
        );
    }
    // Every twin kind has a row, and no row is listed twice.
    for twin in VisualTwin::ALL {
        assert_eq!(
            WARNING_TWINS.iter().filter(|row| row.twin == twin).count(),
            1,
            "{twin:?}"
        );
    }
    // A cue classed as a warning names a twin that has a row.
    for cue in Cue::ALL {
        if let Some(twin) = mix(cue).warning {
            assert!(
                WARNING_TWINS.iter().any(|row| row.twin == twin),
                "{cue:?} warns without a twin row"
            );
        }
    }
}
