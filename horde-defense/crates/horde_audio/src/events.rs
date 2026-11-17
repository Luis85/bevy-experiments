//! Simulation events to cues: [`event_requests`], the one mapping the
//! runtime and the offline renderer share.
//!
//! The mapping is exhaustive. It names every `SimEvent` variant and has no
//! wildcard arm, so a new variant does not compile until its sound (or its
//! silence) is decided here.
//!
//! A request is its cue, queued once per batch, plus what [`Places`] holds
//! for the cue: the lane pan place of the event and the [`Voicing`] (an
//! enemy's voice by catalogue id, a boss's kind, the tone of a gun's
//! quality).

use crate::{
    bank::{BossStage, EnemyStage},
    cue::Cue,
    pan::{self, Places},
    voicing::{EnemyVoiceIndex, EnemyVoiceRef, Tone, Voicing, boss_voicing},
};
use horde_core::{
    content::StructureKind,
    ordnance::OrdnanceKind,
    sim::{Actor, BattleId, CorpseRecord, Outcome, SimEvent, Strike},
    weapons::WeaponClass,
};

/// Queues `cue` once per batch. A retired cue has no shipped file.
pub fn push_cue(cues: &mut Vec<Cue>, cue: Cue) {
    debug_assert!(!cue.retired(), "{cue:?} is retired");
    if !cues.contains(&cue) {
        cues.push(cue);
    }
}

/// The shot cue of a weapon class.
pub const fn weapon_cue(weapon: WeaponClass) -> Cue {
    match weapon {
        WeaponClass::Pistol => Cue::Pistol,
        WeaponClass::Smg => Cue::Smg,
        WeaponClass::Shotgun => Cue::Shotgun,
        WeaponClass::Rifle => Cue::Rifle,
        WeaponClass::Marksman => Cue::Marksman,
        WeaponClass::MachineGun => Cue::MachineGun,
    }
}

/// The cue of a battle's own defender firing: `Some` for a `Shot` by
/// `Actor::Defender`, in either mode. In the root battle that is the
/// player-weapon lane's shot.
pub fn defender_shot(event: &SimEvent) -> Option<Cue> {
    match event {
        SimEvent::Shot {
            shooter: Actor::Defender { weapon, .. },
            ..
        } => Some(weapon_cue(*weapon)),
        _ => None,
    }
}

/// The Block cue plays the Armor bank at this speed (its mix row is 3 dB
/// under Armor's).
pub const BLOCK_SPEED: f32 = 0.85;
/// The Ability cue plays the Ready bank at this speed.
pub const ABILITY_SPEED: f32 = 0.92;
/// The BossExposed cue plays the Ready bank at this speed (its mix row is
/// 2 dB over Ready's).
pub const BOSS_EXPOSED_SPEED: f32 = 0.9;

/// Where and how a batch of events is heard.
#[derive(Clone, Copy, Debug)]
pub struct EventContext<'a> {
    /// The battle the events come from. The root battle's defender shot
    /// takes the player-weapon lane.
    pub battle: BattleId,
    /// The width of the battle's field; `None` (no attempt, or an expedition
    /// heard from camp) leaves every cue centred.
    pub width_yards: Option<f32>,
    /// An expedition heard from camp: its shots are [`Tone::DISTANT`] and
    /// its alerts come over the radio ([`Voicing::radio`]).
    pub distant: bool,
    /// The battle's corpse records: the voice of an enemy that spawned and
    /// died inside one tick, which the index never saw.
    pub corpses: &'a [CorpseRecord],
}

/// One batch's cues and their places and voices.
struct Batch<'a> {
    cues: &'a mut Vec<Cue>,
    places: &'a mut Places,
    width_yards: Option<f32>,
    /// The batch's alerts come over the radio.
    radio: bool,
}
impl Batch<'_> {
    /// The pan place of `cue` for `event`, on a field with a width.
    fn place(&self, cue: Cue, event: &SimEvent) -> Option<f32> {
        let width = self.width_yards?;
        event_x(cue, event).map(|x| pan::lane_position(x, width))
    }
    /// Queues `cue` at `event`'s place, with `voicing` if it names one.
    fn push(&mut self, cue: Cue, event: &SimEvent, voicing: Option<Voicing>) {
        push_cue(self.cues, cue);
        if let Some(p) = self.place(cue, event) {
            self.places.place(cue, p);
        }
        if let Some(voicing) = voicing {
            self.places.voice(cue, voicing);
        }
    }
    /// Queues an alert (a boss, a boss warning, reinforcements, a lost
    /// structure) in `voicing`, over the radio when heard from camp. A
    /// mastered voicing in the root battle stays implicit.
    fn alert(&mut self, cue: Cue, event: &SimEvent, voicing: Voicing) {
        let voicing = if self.radio {
            Some(voicing.radio())
        } else {
            (voicing != Voicing::of(cue)).then_some(voicing)
        };
        self.push(cue, event, voicing);
    }
    /// Queues a warning: of a batch's requests of `cue`, the one nearest
    /// the defender stands.
    fn warn(&mut self, cue: Cue, event: &SimEvent, voicing: Voicing) {
        push_cue(self.cues, cue);
        let place = self.place(cue, event);
        self.places.warn(cue, place, voicing);
    }
    /// A trap's strike is the trap's snap, whatever came of it.
    fn trap(&mut self, event: &SimEvent, strike: &Strike) {
        if matches!(
            strike.attacker,
            Actor::Structure {
                kind: StructureKind::Trap,
                ..
            }
        ) {
            self.push(Cue::Trap, event, None);
        }
    }
}

/// Queues the requests of one simulation event: its cues in `cues`, their
/// places and voices in `places`. `voices` resolves an enemy id to its
/// voice; a kill takes its enemy out of it.
pub fn event_requests(
    cues: &mut Vec<Cue>,
    places: &mut Places,
    voices: &mut EnemyVoiceIndex,
    event: &SimEvent,
    ctx: &EventContext,
) {
    let battle = ctx.battle;
    let mut batch = Batch {
        cues,
        places,
        width_yards: ctx.width_yards,
        radio: ctx.distant,
    };
    match event {
        SimEvent::Shot { shooter, .. } => match *shooter {
            // Either mode sounds like its weapon class, in the tone of the
            // gun's quality; NpcShot is retired.
            Actor::Defender { weapon, .. } => {
                let cue = weapon_cue(weapon);
                let tone = if ctx.distant {
                    Some(Tone::DISTANT)
                } else {
                    voices.shot_tone(battle)
                };
                batch.push(cue, event, tone.map(|tone| Voicing::toned(cue, tone)));
                if battle == BattleId::Root {
                    batch.places.mark_player_shot(cue);
                }
            }
            Actor::Structure {
                kind: StructureKind::Trap,
                ..
            } => batch.push(Cue::Trap, event, None),
            Actor::Structure {
                kind: StructureKind::Turret,
                ..
            } => batch.push(Cue::TurretShot, event, None),
            // A wall fires nothing, enemies and ordnance have their own
            // cues, and a source that no longer exists has no timbre.
            Actor::Structure {
                kind: StructureKind::Wall,
                ..
            }
            | Actor::Enemy { .. }
            | Actor::Ordnance { .. }
            | Actor::Unknown { .. } => {}
        },
        SimEvent::Hit(strike) | SimEvent::WeakPoint(strike) => {
            batch.trap(event, strike);
            batch.push(Cue::Impact, event, None);
        }
        SimEvent::Armor(strike) => {
            batch.trap(event, strike);
            batch.push(Cue::Armor, event, None);
        }
        // Silent: the tracer is a miss's twin.
        SimEvent::Miss(strike) => batch.trap(event, strike),
        SimEvent::Defended(strike) => {
            batch.trap(event, strike);
            // The defender defended (no target id is the defender).
            if strike.target_id.is_none() {
                let voicing = Voicing::tuned(Cue::Block.bank(), BLOCK_SPEED, 0.0);
                batch.push(Cue::Block, event, Some(voicing));
            }
        }
        SimEvent::Damage(_) => batch.push(Cue::Damage, event, None),
        SimEvent::Melee(_) => batch.push(Cue::Melee, event, None),
        SimEvent::Kill { enemy_id, .. } => {
            let voice = voices.remove(battle, *enemy_id).or_else(|| {
                ctx.corpses
                    .iter()
                    .find(|corpse| corpse.id == *enemy_id)
                    .map(EnemyVoiceRef::of_corpse)
            });
            match voice {
                Some(EnemyVoiceRef {
                    boss: Some(kind), ..
                }) => {
                    let voicing = boss_voicing(kind, BossStage::Death, voices.capstone(battle));
                    batch.push(Cue::BossDeath, event, Some(voicing));
                }
                Some(voice) => {
                    let voicing = voice.voicing(EnemyStage::Death);
                    batch.push(Cue::EnemyDeath, event, Some(voicing));
                }
                // Nothing resolves the enemy: the legacy kill.
                None => batch.push(Cue::Kill, event, None),
            }
        }
        SimEvent::Reload => batch.push(Cue::Reload, event, None),
        SimEvent::Spawn { .. } => batch.push(Cue::Incoming, event, None),
        SimEvent::Boss { enemy_id, .. } => {
            match voices.get(battle, *enemy_id).and_then(|voice| voice.boss) {
                Some(kind) => {
                    let voicing = boss_voicing(kind, BossStage::Arrival, voices.capstone(battle));
                    batch.alert(Cue::BossArrival, event, voicing);
                }
                // A boss without a kind: the legacy arrival.
                None => batch.alert(Cue::Boss, event, Voicing::of(Cue::Boss)),
            }
        }
        SimEvent::StructureLost { .. } => {
            batch.alert(Cue::Structure, event, Voicing::of(Cue::Structure));
        }
        SimEvent::Ability { .. } => {
            let voicing = Voicing::tuned(Cue::Ability.bank(), ABILITY_SPEED, 0.0);
            batch.push(Cue::Ability, event, Some(voicing));
        }
        // On the defender and on a structure alike.
        SimEvent::Contact { enemy_id, .. } => {
            let voicing = voices
                .get(battle, *enemy_id)
                .map(|voice| voice.voicing(EnemyStage::Contact));
            batch.push(Cue::EnemyContact, event, voicing);
        }
        SimEvent::Windup {
            enemy_id,
            structure_id,
            ..
        } => {
            // A windup against a structure stays visual: its ring.
            if structure_id.is_none() {
                let voicing = voices
                    .get(battle, *enemy_id)
                    .map_or(Voicing::of(Cue::EnemyWindup), |voice| {
                        voice.voicing(EnemyStage::Windup)
                    });
                batch.warn(Cue::EnemyWindup, event, voicing);
            }
        }
        SimEvent::BossWarning { .. } | SimEvent::Reinforcements { .. } => {
            batch.alert(Cue::Wave, event, Voicing::of(Cue::Wave));
        }
        SimEvent::BossExposure { exposed, .. } => {
            // The weak point opening is the news; its closing is silent.
            if *exposed {
                let voicing = Voicing::tuned(Cue::BossExposed.bank(), BOSS_EXPOSED_SPEED, 0.0);
                batch.push(Cue::BossExposed, event, Some(voicing));
            }
        }
        SimEvent::Throw { .. } => batch.push(Cue::Throw, event, None),
        SimEvent::Explosion { ordnance, .. } => match ordnance {
            OrdnanceKind::Molotov => batch.push(Cue::Molotov, event, None),
            OrdnanceKind::Grenade => batch.push(Cue::Explosion, event, None),
        },
        // Silent per event: the fire bed sounds while a zone burns.
        SimEvent::Burn { .. } => {}
        SimEvent::CooldownReady => batch.push(Cue::Ready, event, None),
        // Log prose.
        SimEvent::RuleResolved { .. } => {}
        SimEvent::AttemptFinished { outcome, .. } => {
            let cue = match outcome {
                Outcome::Victory => Cue::Victory,
                Outcome::Defeat => Cue::Defeat,
                Outcome::Withdrawal => Cue::Retreat,
            };
            batch.push(cue, event, None);
        }
        // A ranged enemy's strike is its contact, in its voice.
        SimEvent::Spit(strike) => {
            let voicing = match strike.attacker {
                Actor::Enemy { id } => voices
                    .get(battle, id)
                    .map(|voice| voice.voicing(EnemyStage::Contact)),
                Actor::Defender { .. }
                | Actor::Structure { .. }
                | Actor::Ordnance { .. }
                | Actor::Unknown { .. } => None,
            };
            batch.push(Cue::EnemyContact, event, voicing);
        }
        // An exploder's blast is a blast.
        SimEvent::Detonation { .. } => batch.push(Cue::Explosion, event, None),
        // Silent: the parent's death and the children's arrival sound.
        SimEvent::Split { .. } => {}
        SimEvent::BossPhase { kind, .. } => {
            let voicing = boss_voicing(*kind, BossStage::Phase, voices.capstone(battle));
            batch.warn(Cue::BossPhase, event, voicing);
        }
    }
}

/// The x in yards a cue of `event` sounds from, when the event has a place:
/// a trap or turret at its own position, an impact where it lands, a death,
/// a spawn, a blast and a lost structure where they happen, an enemy's
/// windup and contact where the enemy stands, a block at the defender.
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
        (Cue::Block, SimEvent::Defended(strike)) => &strike.to,
        (Cue::EnemyContact, SimEvent::Spit(strike)) => &strike.from,
        (Cue::EnemyWindup, SimEvent::Windup { from, .. }) => from,
        (Cue::Kill | Cue::EnemyDeath, SimEvent::Kill { at, .. })
        | (Cue::EnemyContact, SimEvent::Contact { at, .. })
        | (Cue::Incoming, SimEvent::Spawn { at, .. })
        | (Cue::Explosion | Cue::Molotov, SimEvent::Explosion { at, .. })
        | (Cue::Explosion, SimEvent::Detonation { at, .. })
        | (Cue::Structure, SimEvent::StructureLost { at, .. }) => at,
        _ => return None,
    };
    Some(at[0])
}

/// A battle that is never the root and never indexed: the context of the
/// mappings below, which resolve no enemy.
const UNINDEXED: BattleId = BattleId::Expedition(u64::MAX);

/// [`event_requests`] without a battle: nothing resolves an enemy or a
/// gun's quality, so deaths and boss arrivals are the legacy cues and shots
/// play as mastered. For callers that have only the event.
pub fn event_sound(cues: &mut Vec<Cue>, event: &SimEvent) {
    event_sound_at(cues, &mut Places::default(), event, None);
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
    let ctx = EventContext {
        battle: UNINDEXED,
        width_yards,
        distant: false,
        corpses: &[],
    };
    event_requests(cues, places, &mut EnemyVoiceIndex::default(), event, &ctx);
}

/// [`event_sound_at`] for an event of the root battle: the root defender's
/// own shot is also marked for the player-weapon lane.
pub fn root_event_sound(
    cues: &mut Vec<Cue>,
    places: &mut Places,
    event: &SimEvent,
    width_yards: Option<f32>,
) {
    let ctx = EventContext {
        battle: BattleId::Root,
        width_yards,
        distant: false,
        corpses: &[],
    };
    event_requests(cues, places, &mut EnemyVoiceIndex::default(), event, &ctx);
}
