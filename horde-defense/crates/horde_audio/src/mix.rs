//! The mix table: every gain, cooldown, priority, bus, cap and calibration
//! factor the runtime applies. `horde_game` executes these numbers and defines
//! none of its own (`horde_game_audio_sources_define_no_gain_tables`).
//!
//! The rows hold the v1 numbers unchanged (`mix_rows_keep_the_v1_numbers`);
//! value changes come in later steps.

use crate::cue::Cue;

/// The output buses. `Master` scales every other bus; a voice rides exactly
/// one of the others, named by its cue's mix row ([`CueMix::bus`]):
///
/// - `Effects`: every combat cue, warnings included, so no slider but Master
///   and Combat effects can mute a danger cue.
/// - `Ui`: interface sounds.
/// - `Music`: the attempt stingers (and the music tracks, once there are any).
/// - `Atmosphere`: the ambience loops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Bus {
    Master,
    Effects,
    Ui,
    Music,
    Atmosphere,
}
impl Bus {
    /// Every bus, in discriminant order; indexes [`Sliders::bus_gains`].
    pub const ALL: [Self; 5] = [
        Self::Master,
        Self::Effects,
        Self::Ui,
        Self::Music,
        Self::Atmosphere,
    ];
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
    /// The warning cue.
    pub cue: Cue,
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

/// Every audio warning's visual twin: a cue row classed as a warning names
/// one of these twins, and each row here names its warning cue
/// (`warning_cue_table_has_a_visual_twin`). A ranged windup has two twins,
/// the ring and the aim line.
pub const WARNING_TWINS: &[WarningTwin] = &[
    WarningTwin {
        cue: Cue::EnemyWindup,
        event: "Windup",
        twin: VisualTwin::WindupRing,
        predicate: "EnemyView.windup.is_some()",
        declared_in: BATTLE_VIEW_FIELDS,
        declaration: "pub windup: Option<f32>",
    },
    WarningTwin {
        cue: Cue::EnemyWindup,
        event: "Windup (ranged)",
        twin: VisualTwin::AimLine,
        predicate: "EnemyView.aim_target.is_some()",
        declared_in: BATTLE_VIEW_FIELDS,
        declaration: "pub aim_target: Option<[f32; 2]>",
    },
    WarningTwin {
        cue: Cue::BossPhase,
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

/// How far a start's playback speed and gain may stray from the clip's own,
/// so a repeated cue is not heard as one sample. `select::jitter` draws
/// inside the bounds from a presentation counter. Jitter stays on in reduced
/// effects: it adds no intensity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JitterBounds {
    /// The playback speed strays by up to this share either way.
    pub speed: f32,
    /// The gain strays by up to this many dB either way.
    pub gain_db: f32,
}
impl JitterBounds {
    /// Gunfire and weapon handling.
    pub const GUNS: Self = Self::new(0.02, 0.75);
    /// Enemy windup, contact and death voices.
    pub const ENEMY_VOCALS: Self = Self::new(0.04, 1.0);
    /// Impacts and deaths.
    pub const IMPACTS: Self = Self::new(0.03, 1.0);
    /// Boss cues: a slight detune, the level untouched.
    pub const BOSS: Self = Self::new(0.01, 0.0);
    /// Interface sounds: the pitch untouched.
    pub const UI: Self = Self::new(0.0, 0.3);
    /// Stingers and warnings play as mastered.
    pub const NONE: Self = Self::new(0.0, 0.0);

    const fn new(speed: f32, gain_db: f32) -> Self {
        Self { speed, gain_db }
    }
}

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
    /// The jitter group of the cue's starts.
    pub jitter: JitterBounds,
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
        jitter: JitterBounds::NONE,
    }
}
/// An attempt stinger: it rides the Music bus.
const fn stinger(gain: f32, cooldown_ms: u32, priority: u8) -> CueMix {
    CueMix {
        bus: Bus::Music,
        ..row(gain, cooldown_ms, priority)
    }
}
/// An interface sound: it rides the UI bus.
const fn interface(gain: f32, cooldown_ms: u32, priority: u8) -> CueMix {
    CueMix {
        bus: Bus::Ui,
        jitter: JitterBounds::UI,
        ..row(gain, cooldown_ms, priority)
    }
}
const fn fire(gain: f32, cooldown_ms: u32, priority: u8) -> CueMix {
    CueMix {
        gate: GateGroup::Fire,
        ..handling(gain, cooldown_ms, priority)
    }
}
/// Handling a weapon: it varies like firing one.
const fn handling(gain: f32, cooldown_ms: u32, priority: u8) -> CueMix {
    CueMix {
        jitter: JitterBounds::GUNS,
        ..row(gain, cooldown_ms, priority)
    }
}
/// An impact or a death.
const fn hit(gain: f32, cooldown_ms: u32, priority: u8) -> CueMix {
    CueMix {
        jitter: JitterBounds::IMPACTS,
        ..row(gain, cooldown_ms, priority)
    }
}

/// An enemy's windup, contact or death voice.
const fn voice(gain: f32, cooldown_ms: u32, priority: u8) -> CueMix {
    CueMix {
        jitter: JitterBounds::ENEMY_VOCALS,
        ..row(gain, cooldown_ms, priority)
    }
}
/// A boss moment.
const fn boss(gain: f32, cooldown_ms: u32, priority: u8) -> CueMix {
    CueMix {
        jitter: JitterBounds::BOSS,
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
        Cue::Impact => hit(0.55, 85, 7),
        Cue::Armor => hit(0.45, 140, 7),
        Cue::Reload => handling(0.55, 180, 5),
        Cue::Throw => handling(0.55, 180, 4),
        Cue::Explosion => hit(1.0, 180, 2),
        Cue::Molotov => hit(0.65, 180, 2),
        Cue::Fire => row(0.3, 850, 13),
        Cue::Retreat => stinger(0.65, 1500, 0),
        Cue::Boss => CueMix {
            jitter: JitterBounds::BOSS,
            ..row(1.0, 3000, 0)
        },
        Cue::Wave => row(0.65, 1800, 1),
        Cue::Damage => hit(0.75, 240, 3),
        Cue::Melee => hit(0.65, 180, 4),
        Cue::Ready => row(0.45, 400, 6),
        Cue::Structure => hit(0.75, 550, 2),
        Cue::Trap => hit(0.75, 220, 8),
        Cue::Incoming => row(0.3, 2400, 12),
        // Quieter than a hit: a horde's kills must not mask the guns.
        Cue::Kill => hit(0.5, 160, 6),
        Cue::Victory => stinger(0.8, 3000, 0),
        Cue::Defeat => stinger(0.8, 3000, 0),
        Cue::Confirm => interface(0.45, 250, 3),
        // The enemy and boss voices (A6-12). The gains keep the design's
        // distance to their neighbours: a windup at an impact's level, a
        // contact and a death at the kill's.
        Cue::EnemyWindup => CueMix {
            warning: Some(VisualTwin::WindupRing),
            ..voice(0.55, 300, 3)
        },
        Cue::EnemyContact => voice(0.5, 260, 5),
        Cue::EnemyDeath => voice(0.5, 160, 6),
        Cue::BossArrival => boss(1.0, 3000, 0),
        Cue::BossPhase => CueMix {
            warning: Some(VisualTwin::BossPhaseRing),
            ..boss(0.9, 1500, 1)
        },
        Cue::BossDeath => boss(0.9, 1500, 2),
        // The Ready bank, 2 dB up (the voicing lowers its pitch).
        Cue::BossExposed => row(0.55, 400, 4),
        // The Armor bank, 3 dB down.
        Cue::Block => hit(0.32, 140, 7),
        Cue::Ability => row(0.45, 400, 6),
        // Handling the gun (A6-13). The stages sit 2 dB under the reload's
        // finish and the dry trigger 6.5 dB under it, as in the design's
        // table (-7.5 and -12 against Reload's -5.5).
        Cue::DryFire => handling(0.26, 350, 6),
        // Three variants each, so no restart inside 250 ms
        // (`frequent_cues_have_at_least_five_variants`).
        Cue::ReloadOut => handling(0.44, 300, 5),
        Cue::ReloadIn => handling(0.44, 300, 5),
        // Five variants, and short enough for six shells of a quick reload.
        Cue::ShellInsert => handling(0.44, 80, 5),
        Cue::BoltCycle => handling(0.44, 300, 6),
    }
}

/// The one calibration knob of the voice buses (Effects, UI and Music), in
/// dB. It replaces the hidden x0.7 the effects bus used to apply.
pub const MIX_CALIBRATION_DB: f32 = -3.1;
/// The calibration knob of the ambience loops, in dB.
pub const AMBIENCE_CALIBRATION_DB: f32 = 0.0;

/// The default slider values. `Settings::default()` in `horde_game` reads
/// these, and the offline render gates the mix at them. They hold the v1
/// values until the calibration step retunes them.
pub const DEFAULT_MASTER: f32 = 0.6;
pub const DEFAULT_EFFECTS: f32 = 0.7;
pub const DEFAULT_AMBIENCE: f32 = 0.15;
/// "Interface sounds" and "Music & stingers" start at the level their cues
/// had on the effects bus, so the bus split is silent at the defaults.
pub const DEFAULT_UI: f32 = 0.7;
pub const DEFAULT_MUSIC: f32 = 0.7;

/// The volume sliders: one per bus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sliders {
    pub master: f32,
    /// "Combat effects".
    pub effects: f32,
    /// "Interface sounds".
    pub ui: f32,
    /// "Music & stingers".
    pub music: f32,
    /// "Atmosphere".
    pub ambience: f32,
}
impl Sliders {
    pub const DEFAULT: Self = Self {
        master: DEFAULT_MASTER,
        effects: DEFAULT_EFFECTS,
        ui: DEFAULT_UI,
        music: DEFAULT_MUSIC,
        ambience: DEFAULT_AMBIENCE,
    };
    pub const MAX: Self = Self {
        master: 1.0,
        effects: 1.0,
        ui: 1.0,
        music: 1.0,
        ambience: 1.0,
    };

    /// The linear gain of everything on `bus`: Master, times the bus's own
    /// slider, times the bus's calibration. A slider is clamped to 0..=1 and
    /// a non-finite one is silence. A voice plays at its bus gain times its
    /// row gain ([`CueMix::level`]).
    pub fn bus_gain(self, bus: Bus) -> f32 {
        fn level(slider: f32) -> f32 {
            if slider.is_finite() {
                slider.clamp(0.0, 1.0)
            } else {
                0.0
            }
        }
        let master = level(self.master);
        match bus {
            Bus::Master => master,
            Bus::Effects => master * level(self.effects) * calibration_gain(),
            Bus::Ui => master * level(self.ui) * calibration_gain(),
            Bus::Music => master * level(self.music) * calibration_gain(),
            Bus::Atmosphere => master * level(self.ambience) * ambience_calibration_gain(),
        }
    }
    /// [`Sliders::bus_gain`] of every bus, indexed by `Bus as usize`.
    pub fn bus_gains(self) -> [f32; 5] {
        Bus::ALL.map(|bus| self.bus_gain(bus))
    }
}

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

/// How long a duck stays at its full depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuckHold {
    /// This many ms after the attack.
    Ms(u32),
    /// For as long as a voice of the triggering cue sounds (a stinger's clip
    /// length), [`DUCK_HOLD_MAX_MS`] at most.
    WhileSounding,
}

/// What a starting cue does to the ambience and the music tracks: it pulls
/// them down over `attack_ms`, holds them there and lets them back over
/// `release_ms`. A row has no column for Effects or UI: those never duck, so
/// a late-battle warning is never pulled down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Duck {
    /// The depth on the Atmosphere bus, in dB (0 or negative).
    pub atmosphere_db: f32,
    /// The depth on the music tracks, in dB. The stingers, which ride the
    /// Music bus too, are not ducked: they are what the tracks make room for.
    pub music_db: f32,
    pub attack_ms: u32,
    pub hold: DuckHold,
    pub release_ms: u32,
}

/// The attack of a duck whose row names none.
pub const DUCK_ATTACK_MS: u32 = 30;
/// The ducks of a bus add up in dB, down to this floor.
pub const DUCK_FLOOR_DB: f32 = -9.0;
/// The longest a [`DuckHold::WhileSounding`] duck holds, in ms.
pub const DUCK_HOLD_MAX_MS: u32 = 8000;

const fn ducks(atmosphere_db: f32, hold_ms: u32, release_ms: u32) -> Option<Duck> {
    Some(Duck {
        atmosphere_db,
        music_db: 0.0,
        attack_ms: DUCK_ATTACK_MS,
        hold: DuckHold::Ms(hold_ms),
        release_ms,
    })
}

/// The duck a start of `cue` triggers.
pub const fn duck(cue: Cue) -> Option<Duck> {
    match cue {
        // The attempt stingers, for their clip's length.
        Cue::Victory | Cue::Defeat | Cue::Retreat => Some(Duck {
            atmosphere_db: -6.0,
            music_db: -9.0,
            attack_ms: 40,
            hold: DuckHold::WhileSounding,
            release_ms: 600,
        }),
        // The boss arrival.
        Cue::Boss | Cue::BossArrival => Some(Duck {
            atmosphere_db: -6.0,
            music_db: -9.0,
            attack_ms: 30,
            hold: DuckHold::Ms(1200),
            release_ms: 800,
        }),
        Cue::BossPhase => ducks(-4.0, 600, 500),
        Cue::Wave => ducks(-4.0, 600, 400),
        Cue::Explosion => ducks(-3.0, 300, 400),
        Cue::Pistol
        | Cue::Smg
        | Cue::Shotgun
        | Cue::Rifle
        | Cue::Marksman
        | Cue::MachineGun
        | Cue::NpcShot
        | Cue::TurretShot
        | Cue::Impact
        | Cue::Armor
        | Cue::Reload
        | Cue::Throw
        | Cue::Molotov
        | Cue::Fire
        | Cue::Damage
        | Cue::Melee
        | Cue::Ready
        | Cue::Structure
        | Cue::Trap
        | Cue::Incoming
        | Cue::Kill
        | Cue::Confirm
        | Cue::EnemyWindup
        | Cue::EnemyContact
        | Cue::EnemyDeath
        | Cue::BossDeath
        | Cue::BossExposed
        | Cue::Block
        | Cue::Ability
        | Cue::DryFire
        | Cue::ReloadOut
        | Cue::ReloadIn
        | Cue::ShellInsert
        | Cue::BoltCycle => None,
    }
}

/// Density compensation: up to this many effect voices sound at their own
/// level. Beyond it every compensated voice plays at
/// `sqrt(DENSITY_FULL_VOICES / N)`, so the sum of N uncorrelated voices stops
/// growing.
pub const DENSITY_FULL_VOICES: usize = 4;
/// The time constants of the compensation, in ms: it comes in fast and lets
/// go slowly.
pub const DENSITY_ATTACK_MS: f32 = 10.0;
pub const DENSITY_RELEASE_MS: f32 = 250.0;
/// The ambience makes room for a crowd of effect voices: this many dB at
/// `DENSITY_ATMOSPHERE_FROM + DENSITY_ATMOSPHERE_SPAN` voices and more,
/// nothing up to `DENSITY_ATMOSPHERE_FROM`, linear in between.
pub const DENSITY_ATMOSPHERE_DB: f32 = -4.0;
pub const DENSITY_ATMOSPHERE_FROM: usize = 2;
pub const DENSITY_ATMOSPHERE_SPAN: usize = 4;
pub const DENSITY_ATMOSPHERE_RELEASE_MS: f32 = 300.0;

/// The gain of a compensated effect voice among `voices` sounding effect
/// voices.
pub fn density_gain(voices: usize) -> f32 {
    if voices <= DENSITY_FULL_VOICES {
        1.0
    } else {
        (DENSITY_FULL_VOICES as f32 / voices as f32).sqrt()
    }
}
/// The ambience's density term among `voices` sounding effect voices, in dB.
pub fn density_atmosphere_db(voices: usize) -> f32 {
    let over = voices.saturating_sub(DENSITY_ATMOSPHERE_FROM) as f32;
    DENSITY_ATMOSPHERE_DB * (over / DENSITY_ATMOSPHERE_SPAN as f32).clamp(0.0, 1.0)
}
