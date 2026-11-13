//! Simulation events to cues. This is the v1 mapping, moved here unchanged so
//! the runtime and the offline renderer hear the same battle.

use crate::{
    cue::Cue,
    pan::{self, Places},
};
use horde_core::{
    content::StructureKind,
    ordnance::OrdnanceKind,
    sim::{Actor, Outcome, SimEvent},
    weapons::WeaponClass,
};

/// Queues `cue` once per batch. A retired cue has no shipped file.
pub fn push_cue(cues: &mut Vec<Cue>, cue: Cue) {
    debug_assert!(!cue.retired(), "{cue:?} is retired");
    if !cues.contains(&cue) {
        cues.push(cue);
    }
}

/// Queues the cues of one simulation event.
pub fn event_sound(cues: &mut Vec<Cue>, event: &SimEvent) {
    if let SimEvent::Hit(strike)
    | SimEvent::Armor(strike)
    | SimEvent::Miss(strike)
    | SimEvent::Defended(strike) = event
        && matches!(
            strike.attacker,
            Actor::Structure {
                kind: StructureKind::Trap,
                ..
            }
        )
    {
        push_cue(cues, Cue::Trap);
    }
    let cue = match event {
        SimEvent::Boss { .. } => Some(Cue::Boss),
        SimEvent::BossWarning { .. } | SimEvent::Reinforcements { .. } => Some(Cue::Wave),
        SimEvent::BossExposure { .. } | SimEvent::CooldownReady => Some(Cue::Ready),
        SimEvent::StructureLost { .. } => Some(Cue::Structure),
        SimEvent::Damage(_) => Some(Cue::Damage),
        SimEvent::Melee(_) => Some(Cue::Melee),
        SimEvent::Reload => Some(Cue::Reload),
        SimEvent::Shot { shooter, .. } => match *shooter {
            Actor::Structure {
                kind: StructureKind::Trap,
                ..
            } => Some(Cue::Trap),
            Actor::Structure {
                kind: StructureKind::Turret,
                ..
            } => Some(Cue::TurretShot),
            // Either mode sounds like its weapon; NpcShot is retired.
            Actor::Defender { weapon, .. } => Some(match weapon {
                WeaponClass::Pistol => Cue::Pistol,
                WeaponClass::Smg => Cue::Smg,
                WeaponClass::Shotgun => Cue::Shotgun,
                WeaponClass::Rifle => Cue::Rifle,
                WeaponClass::Marksman => Cue::Marksman,
                WeaponClass::MachineGun => Cue::MachineGun,
            }),
            // A source that no longer exists has no reliable timbre.
            _ => None,
        },
        SimEvent::Throw { .. } => Some(Cue::Throw),
        SimEvent::Explosion {
            ordnance: OrdnanceKind::Molotov,
            ..
        } => Some(Cue::Molotov),
        SimEvent::Explosion { .. } => Some(Cue::Explosion),
        SimEvent::Burn { .. } => Some(Cue::Fire),
        SimEvent::Hit(_) | SimEvent::WeakPoint(_) => Some(Cue::Impact),
        SimEvent::Armor(_) => Some(Cue::Armor),
        SimEvent::Spawn { .. } => Some(Cue::Incoming),
        SimEvent::Kill { .. } => Some(Cue::Kill),
        SimEvent::AttemptFinished { outcome, .. } => Some(match outcome {
            Outcome::Victory => Cue::Victory,
            Outcome::Defeat => Cue::Defeat,
            Outcome::Withdrawal => Cue::Retreat,
        }),
        // Enemy-side events have no cue until audio's exhaustive SimEvent
        // mapping in wave 6 (A6-12). Their visual twins already exist: the
        // W2-28 aim line for a ranged windup and the W2-37 phase label and
        // ring for a boss phase.
        SimEvent::Spit(_)
        | SimEvent::Detonation { .. }
        | SimEvent::Split { .. }
        | SimEvent::BossPhase { .. } => None,
        _ => None,
    };
    if let Some(cue) = cue {
        push_cue(cues, cue);
    }
}

/// The x in yards a cue of `event` sounds from, when the event has a place:
/// a trap or turret at its own position, an impact where it lands, a kill, a
/// spawn, a blast, a burn and a lost structure where they happen.
pub fn event_x(cue: Cue, event: &SimEvent) -> Option<f32> {
    let at = match (cue, event) {
        (Cue::Trap | Cue::TurretShot, SimEvent::Shot { from, .. }) => from,
        (
            Cue::Trap,
            SimEvent::Hit(strike)
            | SimEvent::Armor(strike)
            | SimEvent::Miss(strike)
            | SimEvent::Defended(strike),
        ) => &strike.from,
        (
            Cue::Impact | Cue::Armor,
            SimEvent::Hit(strike) | SimEvent::WeakPoint(strike) | SimEvent::Armor(strike),
        ) => &strike.to,
        (Cue::Kill, SimEvent::Kill { at, .. })
        | (Cue::Incoming, SimEvent::Spawn { at, .. })
        | (Cue::Explosion | Cue::Molotov, SimEvent::Explosion { at, .. })
        | (Cue::Fire, SimEvent::Burn { at, .. })
        | (Cue::Structure, SimEvent::StructureLost { at, .. }) => at,
        _ => return None,
    };
    Some(at[0])
}

/// [`event_sound`], and the pan place of every cue the event newly queued.
/// `width_yards` is the width of the field the event happened on; `None`
/// (no attempt) leaves the cues centred.
pub fn event_sound_at(
    cues: &mut Vec<Cue>,
    places: &mut Places,
    event: &SimEvent,
    width_yards: Option<f32>,
) {
    let queued = cues.len();
    event_sound(cues, event);
    let Some(width) = width_yards else { return };
    for cue in &cues[queued..] {
        if let Some(x) = event_x(*cue, event) {
            places.place(*cue, pan::lane_position(x, width));
        }
    }
}
