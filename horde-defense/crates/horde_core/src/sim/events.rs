//! Typed simulation event stream. Every emitted event receives a saved,
//! monotonic sequence number and is queued in a transient, drainable outbox,
//! including events from the tick on which an attempt finishes.
use super::*;
use crate::weapons::WeaponClass;

/// Undrained records retained for hosts that never consume the stream
/// (headless tests, benchmarks). The native runner drains every tick.
const MAX_UNDRAINED_EVENTS: usize = 8192;

/// Which battle emitted a record: the root deployment or a parallel expedition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BattleId {
    Root,
    Expedition(u64),
}

/// The resolved source of an attack, shot or strike at emission time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actor {
    Defender {
        mode: Mode,
        weapon: WeaponClass,
    },
    Structure {
        id: u64,
        kind: StructureKind,
    },
    Enemy {
        id: u64,
    },
    Ordnance {
        id: u64,
        kind: OrdnanceKind,
    },
    /// An ID that no longer names a live object when the event was emitted.
    Unknown {
        id: u64,
    },
}

/// One resolved strike. `target_id` is `None` when the defender was struck.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub attacker: Actor,
    pub target_id: Option<u64>,
    pub lane: Option<u8>,
    pub from: [f32; 2],
    pub to: [f32; 2],
    /// Injury or damage dealt; zero for misses, defenses and absorbed hits.
    pub value: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SimEvent {
    Shot {
        shooter: Actor,
        target_id: Option<u64>,
        lane: Option<u8>,
        from: [f32; 2],
        to: [f32; 2],
    },
    Hit(Strike),
    WeakPoint(Strike),
    /// Protection absorbed the whole strike.
    Armor(Strike),
    Miss(Strike),
    Defended(Strike),
    /// Hostile injury to the defender (no target) or to a structure.
    Damage(Strike),
    Melee(Strike),
    Kill {
        enemy_id: u64,
        lane: Option<u8>,
        at: [f32; 2],
        coins: i32,
    },
    /// The defender's magazine was refilled.
    Reload,
    Spawn {
        enemy_id: u64,
        lane: Option<u8>,
        at: [f32; 2],
    },
    Boss {
        enemy_id: u64,
        lane: Option<u8>,
        at: [f32; 2],
    },
    StructureLost {
        at: [f32; 2],
    },
    /// A heal (positive `healed`) or a brace (zero).
    Ability {
        healed: i32,
    },
    Contact {
        enemy_id: u64,
        structure_id: Option<u64>,
        lane: Option<u8>,
        at: [f32; 2],
    },
    Windup {
        enemy_id: u64,
        structure_id: Option<u64>,
        lane: Option<u8>,
        duration_ticks: u32,
        from: [f32; 2],
        to: [f32; 2],
    },
    BossWarning {
        seconds_left: i32,
        lane: Option<u8>,
        duration_ticks: u32,
    },
    BossExposure {
        boss_id: u64,
        exposed: bool,
        lane: Option<u8>,
        duration_ticks: u32,
        at: [f32; 2],
    },
    Reinforcements {
        lane: Option<u8>,
        duration_ticks: u32,
    },
    Throw {
        projectile_id: u64,
        ordnance: OrdnanceKind,
        lane: Option<u8>,
        from: [f32; 2],
        to: [f32; 2],
        flight_ticks: u32,
    },
    Explosion {
        projectile_id: u64,
        ordnance: OrdnanceKind,
        lane: Option<u8>,
        at: [f32; 2],
        /// Blast radius in yards.
        radius: f32,
    },
    Burn {
        projectile_id: u64,
        lane: Option<u8>,
        at: [f32; 2],
        radius: f32,
        duration_ticks: u32,
    },
    /// The defender's melee cooldown elapsed on this tick.
    CooldownReady,
    /// A rules resolution (3d6 attack, defense and injury) in log prose.
    RuleResolved {
        attacker: Actor,
        target_id: Option<u64>,
        summary: String,
    },
    AttemptFinished {
        attempt_id: u64,
        outcome: Outcome,
        mode: Mode,
        cause: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimEventRecord {
    /// Monotonic per emitting `Game`; `(battle, seq)` is unique.
    pub seq: u64,
    pub tick: u64,
    pub battle: BattleId,
    pub event: SimEvent,
}

impl Attempt {
    pub(super) fn actor(&self, id: Option<u64>, ordnance: Option<OrdnanceKind>) -> Actor {
        let Some(id) = id else {
            return Actor::Defender {
                mode: self.mode,
                weapon: self.profile.weapon.class,
            };
        };
        if let Some(kind) = ordnance {
            Actor::Ordnance { id, kind }
        } else if let Some(s) = self.structures.iter().find(|s| s.id == id) {
            Actor::Structure { id, kind: s.kind }
        } else if self.enemies.iter().any(|e| e.id == id) {
            Actor::Enemy { id }
        } else {
            Actor::Unknown { id }
        }
    }
    fn next_seq(&mut self) -> u64 {
        let seq = self.event_seq;
        self.event_seq += 1;
        seq
    }
    /// Queue a stream-only event (not part of the presentation window).
    pub(super) fn emit(&mut self, event: SimEvent) {
        let seq = self.next_seq();
        self.queue_record(seq, event);
    }
    fn queue_record(&mut self, seq: u64, event: SimEvent) {
        self.emitted.push(SimEventRecord {
            seq,
            tick: self.tick,
            battle: BattleId::Root,
            event,
        });
    }
    /// Append to the bounded presentation window and the typed stream with one
    /// shared sequence number.
    pub(super) fn event_with(
        &mut self,
        kind: EventKind,
        from: [f32; 2],
        to: [f32; 2],
        value: i32,
        meta: EventMetadata,
    ) {
        let seq = self.next_seq();
        if self.events.len() == MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(Event {
            kind,
            from,
            to,
            value,
            tick: self.tick,
            actor_id: meta.actor_id,
            target_id: meta.target_id,
            lane: meta.lane,
            duration_ticks: meta.duration_ticks,
            seq,
        });
        let event = self.typed(kind, from, to, value, meta);
        self.queue_record(seq, event);
    }
    fn typed(
        &self,
        kind: EventKind,
        from: [f32; 2],
        to: [f32; 2],
        value: i32,
        meta: EventMetadata,
    ) -> SimEvent {
        let id = meta.actor_id.unwrap_or_default();
        let lane = meta.lane;
        let strike = || Strike {
            attacker: self.actor(meta.actor_id, meta.ordnance),
            target_id: meta.target_id,
            lane,
            from,
            to,
            value,
        };
        let ordnance = || {
            debug_assert!(meta.ordnance.is_some(), "ordnance events name their kind");
            meta.ordnance.unwrap_or(OrdnanceKind::Grenade)
        };
        match kind {
            EventKind::Shot => SimEvent::Shot {
                shooter: self.actor(meta.actor_id, meta.ordnance),
                target_id: meta.target_id,
                lane,
                from,
                to,
            },
            EventKind::Hit => SimEvent::Hit(strike()),
            EventKind::WeakPoint => SimEvent::WeakPoint(strike()),
            EventKind::Armor => SimEvent::Armor(strike()),
            EventKind::Miss => SimEvent::Miss(strike()),
            EventKind::Defended => SimEvent::Defended(strike()),
            EventKind::Damage => SimEvent::Damage(strike()),
            EventKind::Melee => SimEvent::Melee(strike()),
            EventKind::Kill => SimEvent::Kill {
                enemy_id: id,
                lane,
                at: to,
                coins: value,
            },
            EventKind::Reload => SimEvent::Reload,
            EventKind::Spawn => SimEvent::Spawn {
                enemy_id: id,
                lane,
                at: to,
            },
            EventKind::Boss => SimEvent::Boss {
                enemy_id: id,
                lane,
                at: to,
            },
            EventKind::StructureLost => SimEvent::StructureLost { at: to },
            EventKind::Ability => SimEvent::Ability { healed: value },
            EventKind::Contact => SimEvent::Contact {
                enemy_id: id,
                structure_id: meta.target_id,
                lane,
                at: from,
            },
            EventKind::Windup => SimEvent::Windup {
                enemy_id: id,
                structure_id: meta.target_id,
                lane,
                duration_ticks: meta.duration_ticks,
                from,
                to,
            },
            EventKind::BossWarning => SimEvent::BossWarning {
                seconds_left: value,
                lane,
                duration_ticks: meta.duration_ticks,
            },
            EventKind::BossExposure => SimEvent::BossExposure {
                boss_id: id,
                exposed: value != 0,
                lane,
                duration_ticks: meta.duration_ticks,
                at: to,
            },
            EventKind::Reinforcements => SimEvent::Reinforcements {
                lane,
                duration_ticks: meta.duration_ticks,
            },
            EventKind::Throw => SimEvent::Throw {
                projectile_id: id,
                ordnance: ordnance(),
                lane,
                from,
                to,
                flight_ticks: meta.duration_ticks,
            },
            EventKind::Explosion => SimEvent::Explosion {
                projectile_id: id,
                ordnance: ordnance(),
                lane,
                at: to,
                radius: value as f32 / 100.0,
            },
            EventKind::Burn => SimEvent::Burn {
                projectile_id: id,
                lane,
                at: to,
                radius: value as f32 / 100.0,
                duration_ticks: meta.duration_ticks,
            },
        }
    }
}

impl Game {
    /// Typed events emitted since the last call, in publication order. Within
    /// one tick, records forwarded from expeditions (whose worker games tick
    /// first) come before the root attempt's own records.
    pub fn drain_events(&mut self) -> std::vec::Drain<'_, SimEventRecord> {
        self.collect_attempt_events();
        self.event_outbox.drain(..)
    }
    /// Move the live attempt's queued records into the outbox.
    pub(super) fn collect_attempt_events(&mut self) {
        if let Some(a) = &mut self.attempt
            && !a.emitted.is_empty()
        {
            let records = std::mem::take(&mut a.emitted);
            self.publish(records);
        }
    }
    /// A finishing attempt hands over its records and its sequence cursor,
    /// then the game records the terminal event itself.
    pub(super) fn finish_events(&mut self, a: &mut Attempt, outcome: Outcome, cause: &str) {
        let records = std::mem::take(&mut a.emitted);
        self.publish(records);
        self.next_event_seq = self.next_event_seq.max(a.event_seq);
        let seq = self.next_event_seq;
        self.next_event_seq += 1;
        self.publish([SimEventRecord {
            seq,
            tick: a.tick,
            battle: BattleId::Root,
            event: SimEvent::AttemptFinished {
                attempt_id: a.id,
                outcome,
                mode: a.mode,
                cause: cause.to_owned(),
            },
        }]);
    }
    /// Re-label every expedition's drained records with its battle ID.
    pub(super) fn forward_expedition_events(&mut self) {
        let mut forwarded = Vec::new();
        for battle in &mut self.battles {
            let id = battle.id;
            forwarded.extend(battle.game.drain_events().map(|mut record| {
                record.battle = BattleId::Expedition(id);
                record
            }));
        }
        if !forwarded.is_empty() {
            self.publish(forwarded);
        }
    }
    fn publish(&mut self, records: impl IntoIterator<Item = SimEventRecord>) {
        self.event_outbox.extend(records);
        if self.event_outbox.len() > MAX_UNDRAINED_EVENTS {
            let excess = self.event_outbox.len() - MAX_UNDRAINED_EVENTS / 2;
            self.event_outbox.drain(..excess);
        }
    }
}
