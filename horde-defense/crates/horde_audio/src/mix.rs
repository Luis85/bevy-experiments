//! The mix table: every gain, cooldown, priority, bus, cap and calibration
//! factor the runtime applies. `horde_game` executes these numbers and defines
//! none of its own (`horde_game_audio_sources_define_no_gain_tables`).
//!
//! The rows hold the v1 numbers unchanged (`mix_rows_keep_the_v1_numbers`);
//! value changes come in later steps.

use crate::cue::Cue;

/// The output buses. Every cue rides `Effects` until the bus split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    Master,
    Effects,
    Ui,
    Music,
    Atmosphere,
}

/// Which selection lane a cue competes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateGroup {
    /// Gunfire: sources take turns for the frame's free slots.
    Fire,
    General,
}

/// A view-model element that shows what a warning cue announces, so the
/// warning never depends on hearing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualTwin {
    /// The windup ellipse and bar under every enemy in windup.
    WindupRing,
    /// The dashed aim line of a ranged windup (W2-28).
    AimLine,
    /// The "PHASE n - NAME" label and ring of a boss phase hold (W2-37).
    BossPhaseRing,
}
impl VisualTwin {
    pub const ALL: [Self; 3] = [Self::WindupRing, Self::AimLine, Self::BossPhaseRing];
}

/// One warning and the view-model predicate that is its visual twin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarningTwin {
    /// The `SimEvent` the warning cue announces.
    pub event: &'static str,
    pub twin: VisualTwin,
    /// True in the battle view model on every frame the warning applies.
    pub predicate: &'static str,
    /// The file that declares the predicate's field, relative to the
    /// workspace, and the declaration itself.
    pub declared_in: &'static str,
    pub declaration: &'static str,
}

const BATTLE_VIEW_FIELDS: &str = "crates/horde_game/src/battle_view/field.rs";

/// Every audio warning's visual twin. The twins exist before the cues do: the
/// windup and boss-phase cues join the cue list with the event mapping, and a
/// cue row classed as a warning must name one of these.
pub const WARNING_TWINS: &[WarningTwin] = &[
    WarningTwin {
        event: "Windup",
        twin: VisualTwin::WindupRing,
        predicate: "EnemyView.windup.is_some()",
        declared_in: BATTLE_VIEW_FIELDS,
        declaration: "pub windup: Option<f32>",
    },
    WarningTwin {
        event: "Windup (ranged)",
        twin: VisualTwin::AimLine,
        predicate: "EnemyView.aim_target.is_some()",
        declared_in: BATTLE_VIEW_FIELDS,
        declaration: "pub aim_target: Option<[f32; 2]>",
    },
    WarningTwin {
        event: "BossPhase",
        twin: VisualTwin::BossPhaseRing,
        predicate: "BattleView.boss_phase.is_some()",
        declared_in: BATTLE_VIEW_FIELDS,
        declaration: "pub boss_phase: Option<BossPhaseView>",
    },
];

/// Reduced effects: every cue plays at this share of its gain.
pub const REDUCED_GAIN: f32 = 0.65;
/// Reduced effects: every cooldown is 3/2 as long, rounded down.
pub const REDUCED_COOLDOWN: (u64, u64) = (3, 2);

/// One cue's mix numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CueMix {
    pub bus: Bus,
    /// Linear per-voice gain; the bus gain multiplies it.
    pub gain: f32,
    /// Minimum real time between two starts of the cue.
    pub cooldown_ms: u32,
    /// 0 is most important; a lower number displaces a higher one.
    pub priority: u8,
    pub gate: GateGroup,
    pub reduced_gain: f32,
    /// The cue is never emitted; the row keeps the slot's numbers.
    pub retired: bool,
    /// Set for a warning cue: the twin that shows it.
    pub warning: Option<VisualTwin>,
}
impl CueMix {
    /// The per-voice gain in normal or reduced effects.
    pub fn level(self, reduced: bool) -> f32 {
        self.gain * if reduced { self.reduced_gain } else { 1.0 }
    }
    /// The cooldown in ms in normal or reduced effects.
    pub fn cooldown(self, reduced: bool) -> u64 {
        let interval = u64::from(self.cooldown_ms);
        if reduced {
            interval * REDUCED_COOLDOWN.0 / REDUCED_COOLDOWN.1
        } else {
            interval
        }
    }
}

const fn row(gain: f32, cooldown_ms: u32, priority: u8) -> CueMix {
    CueMix {
        bus: Bus::Effects,
        gain,
        cooldown_ms,
        priority,
        gate: GateGroup::General,
        reduced_gain: REDUCED_GAIN,
        retired: false,
        warning: None,
    }
}
const fn fire(gain: f32, cooldown_ms: u32, priority: u8) -> CueMix {
    CueMix {
        gate: GateGroup::Fire,
        ..row(gain, cooldown_ms, priority)
    }
}

/// The mix row of a cue.
pub const fn mix(cue: Cue) -> CueMix {
    match cue {
        Cue::Pistol => fire(0.9, 80, 9),
        Cue::Smg => fire(0.75, 55, 9),
        Cue::Shotgun => fire(1.0, 120, 9),
        Cue::Rifle => fire(0.9, 80, 9),
        Cue::Marksman => fire(0.9, 120, 9),
        Cue::MachineGun => fire(0.9, 55, 9),
        Cue::NpcShot => CueMix {
            retired: true,
            ..fire(0.75, 100, 10)
        },
        Cue::TurretShot => fire(0.65, 150, 11),
        Cue::Impact => row(0.55, 85, 7),
        Cue::Armor => row(0.45, 140, 7),
        Cue::Reload => row(0.55, 180, 5),
        Cue::Throw => row(0.55, 180, 4),
        Cue::Explosion => row(1.0, 180, 2),
        Cue::Molotov => row(0.65, 180, 2),
        Cue::Fire => row(0.3, 850, 13),
        Cue::Retreat => row(0.65, 1500, 0),
        Cue::Boss => row(1.0, 3000, 0),
        Cue::Wave => row(0.65, 1800, 1),
        Cue::Damage => row(0.75, 240, 3),
        Cue::Melee => row(0.65, 180, 4),
        Cue::Ready => row(0.45, 400, 6),
        Cue::Structure => row(0.75, 550, 2),
        Cue::Trap => row(0.75, 220, 8),
        Cue::Incoming => row(0.3, 2400, 12),
        // Quieter than a hit: a horde's kills must not mask the guns.
        Cue::Kill => row(0.5, 160, 6),
        Cue::Victory => row(0.8, 3000, 0),
        Cue::Defeat => row(0.8, 3000, 0),
        Cue::Confirm => row(0.45, 250, 3),
    }
}

/// Effect voices that may sound at once.
pub const MAX_EFFECT_VOICES: usize = 6;

/// The one calibration knob of the effects bus, in dB. It replaces the hidden
/// x0.7 the bus used to apply.
pub const MIX_CALIBRATION_DB: f32 = -3.1;
/// The calibration knob of the ambience loops, in dB.
pub const AMBIENCE_CALIBRATION_DB: f32 = 0.0;

/// The default slider values. `Settings::default()` in `horde_game` reads
/// these, and the offline render gates the mix at them. They hold the v1
/// values until the calibration step retunes them.
pub const DEFAULT_MASTER: f32 = 0.6;
pub const DEFAULT_EFFECTS: f32 = 0.7;
pub const DEFAULT_AMBIENCE: f32 = 0.15;

/// dB to a linear factor.
pub fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}
/// [`MIX_CALIBRATION_DB`] as the linear factor the bus multiplies in.
pub fn calibration_gain() -> f32 {
    db_to_linear(MIX_CALIBRATION_DB)
}
/// [`AMBIENCE_CALIBRATION_DB`] as a linear factor.
pub fn ambience_calibration_gain() -> f32 {
    db_to_linear(AMBIENCE_CALIBRATION_DB)
}

/// How loud the ambience loops are in each screen context.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextGain {
    pub menu: f32,
    pub camp: f32,
    pub battle: f32,
    pub paused: f32,
}
pub const CONTEXT_GAIN: ContextGain = ContextGain {
    menu: 0.55,
    camp: 0.75,
    battle: 1.0,
    paused: 0.35,
};
