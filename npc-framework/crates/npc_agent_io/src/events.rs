//! The agent event buffer: journal entries that matter to controlled units,
//! numbered with a monotonic `seq` (spec §2.5). Tooling only (not hashed,
//! never read by the sim).
//!
//! [`harvest`] copies new journal entries in `(tick, seq)` order: those of
//! units that have an external controller, world events (no unit, e.g.
//! `PhaseChange`) and every agent kind. It runs whenever an agent method
//! reads events (observe, events, act) and after every frame of a session,
//! so nothing needs a sim system. The journal must be on (`AgentIoPlugin`
//! turns it on unless told not to); with it off the buffer stays empty.
//!
//! The buffer is a ring of [`DEFAULT_EVENT_CAPACITY`] events: `dropped`
//! counts evicted ones and [`AgentEvents::oldest_seq`] is the first one still
//! held, so a poller can tell when it missed events. Events are in
//! ascending `seq`, so queries binary-search their start (`partition_point`).

use std::collections::VecDeque;

use bevy_ecs::prelude::*;
use npc_kernel::SimId;
use npc_kernel::journal::{Journal, JournalKind};
use serde::{Deserialize, Serialize};

use crate::control::ExternalController;

/// Default buffer size.
pub const DEFAULT_EVENT_CAPACITY: usize = 20_000;

/// Kinds `agent/events` returns by default (`kinds: ["all"]` returns every
/// buffered kind, including the behaviour-tree chatter). Perception rolls
/// (`Roll` with purpose `perception.detect`) are left out of the default and
/// of observations' recent events; name `Roll` to get them.
pub const SIGNIFICANT_KINDS: &[&str] = &[
    "ExternalAction",
    "ActionRejected",
    "ControllerAttached",
    "ControllerDetached",
    "Interacted",
    "ItemMoved",
    "DoorChanged",
    "NeedLevel",
    "Effect",
    "Reserve",
    "Release",
    "PathFound",
    "PathFailed",
    "Despawn",
    "PhaseChange",
    "Perceived",
    "Lost",
    "Roll",
    // Wave F capabilities (journaled by their crates; offered to agents by
    // the facade glue).
    "OrderIssued",
    "OrderStarted",
    "OrderDone",
    "OrderFailed",
    "Mounted",
    "Dismounted",
    "VehicleStalled",
    "XpGained",
    "LevelUp",
    "SkillImproved",
    "CraftStarted",
    "CraftDone",
    "CraftFailed",
];

/// Agent kinds, buffered for any unit.
const AGENT_KINDS: &[&str] = &[
    "ExternalAction",
    "ActionRejected",
    "ControllerAttached",
    "ControllerDetached",
    "Interacted",
    "ItemMoved",
    "DoorChanged",
];

/// The purpose of perception detection rolls (`per_roll`).
pub const PERCEPTION_ROLL_PURPOSE: &str = "perception.detect";

/// One buffered event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEvent {
    /// Monotonic, from 1.
    pub seq: u64,
    pub tick: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<SimId>,
    /// The journal kind's name.
    pub kind: String,
    /// The journal entry's kind (`{"type":"NeedLevel",..}`).
    pub event: JournalKind,
}

impl AgentEvent {
    /// Whether this event says the unit's situation changed in a way an
    /// agent planning ahead did not foresee, regardless of what it acts on:
    /// its own actions, orders, interactions and crafts ending (done,
    /// failed, cancelled, rejected), need level changes, mounting, the
    /// controller detached, a despawn, a failed path. Percepts count only
    /// for the targets an act refers to ([`AgentEvent::interrupts_for`]).
    pub fn interrupts(&self) -> bool {
        match &self.event {
            JournalKind::ExternalAction { status, .. } => {
                matches!(
                    status.as_str(),
                    "done" | "failed" | "cancelled" | "rejected"
                )
            }
            JournalKind::Interacted { outcome, .. } => outcome != "started",
            JournalKind::NeedLevel { .. }
            | JournalKind::ControllerDetached { .. }
            | JournalKind::Despawn
            | JournalKind::PathFailed { .. }
            | JournalKind::OrderDone { .. }
            | JournalKind::OrderFailed { .. }
            | JournalKind::CraftDone { .. }
            | JournalKind::CraftFailed { .. }
            | JournalKind::Mounted { .. }
            | JournalKind::Dismounted { .. } => true,
            _ => false,
        }
    }

    /// [`AgentEvent::interrupts`], plus a new or lost percept of one of
    /// `targets` (what the pending act refers to).
    pub fn interrupts_for(&self, targets: &[SimId]) -> bool {
        match &self.event {
            JournalKind::Perceived { target, .. } | JournalKind::Lost { target, .. } => {
                targets.contains(target)
            }
            _ => self.interrupts(),
        }
    }

    /// A `Roll` with purpose [`PERCEPTION_ROLL_PURPOSE`] (noise for agents).
    pub fn is_perception_roll(&self) -> bool {
        matches!(&self.event, JournalKind::Roll { purpose, .. } if purpose == PERCEPTION_ROLL_PURPOSE)
    }

    /// The controller's own outcomes: its actions and their rejections,
    /// interactions, their skill rolls, items, orders, crafts.
    pub fn is_own_outcome(&self) -> bool {
        (self.kind == "Roll" && !self.is_perception_roll())
            || matches!(
                self.kind.as_str(),
                "ExternalAction"
                    | "ActionRejected"
                    | "Interacted"
                    | "ItemMoved"
                    | "OrderDone"
                    | "OrderFailed"
                    | "CraftDone"
                    | "CraftFailed"
                    | "Mounted"
                    | "Dismounted"
            )
    }
}

/// The buffer.
#[derive(Resource, Clone, Debug)]
pub struct AgentEvents {
    pub events: VecDeque<AgentEvent>,
    pub capacity: usize,
    /// The seq the next event gets.
    pub next_seq: u64,
    /// Events evicted from the front so far (the buffer is a ring).
    pub dropped: u64,
    /// Last journal `(tick, seq)` harvested.
    cursor: Option<(u64, u32)>,
}

impl Default for AgentEvents {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            capacity: DEFAULT_EVENT_CAPACITY,
            next_seq: 1,
            dropped: 0,
            cursor: None,
        }
    }
}

impl AgentEvents {
    /// The latest seq (0 before the first event).
    pub fn last_seq(&self) -> u64 {
        self.next_seq - 1
    }

    /// The seq of the oldest event still buffered (`next_seq` when empty).
    pub fn oldest_seq(&self) -> u64 {
        self.events.front().map_or(self.next_seq, |e| e.seq)
    }

    /// Index of the first event with `seq > since_seq` (binary search: the
    /// buffer is in ascending `seq`).
    pub fn start_after(&self, since_seq: u64) -> usize {
        self.events.partition_point(|e| e.seq <= since_seq)
    }

    /// Events with `seq > since_seq`, oldest first.
    pub fn after(&self, since_seq: u64) -> impl Iterator<Item = &AgentEvent> {
        self.events.range(self.start_after(since_seq)..)
    }
}

/// The latest event seq of `world` (0 without events).
pub fn last_seq(world: &World) -> u64 {
    world
        .get_resource::<AgentEvents>()
        .map_or(0, AgentEvents::last_seq)
}

/// Copies new journal entries into [`AgentEvents`] (see the module docs).
pub fn harvest(world: &mut World) {
    let cursor = world.get_resource_or_init::<AgentEvents>().cursor;
    let Some(journal) = world.get_resource::<Journal>() else {
        return;
    };
    let mut fresh: Vec<_> = journal
        .entries
        .iter()
        .rev()
        .take_while(|e| cursor.is_none_or(|c| (e.tick, e.seq) > c))
        .cloned()
        .collect();
    if fresh.is_empty() {
        return;
    }
    fresh.reverse();
    let index = world.get_resource::<npc_kernel::SimIndex>();
    let controlled = |id: SimId| {
        index
            .and_then(|i| i.get(id))
            .is_some_and(|e| world.get::<ExternalController>(e).is_some())
    };
    let keep: Vec<_> = fresh
        .iter()
        .filter(|e| match e.unit {
            None => true,
            Some(u) => controlled(u) || AGENT_KINDS.contains(&e.kind.name()),
        })
        .cloned()
        .collect();
    let last = fresh.last().map(|e| (e.tick, e.seq));
    let mut events = world.resource_mut::<AgentEvents>();
    events.cursor = last;
    for e in keep {
        let seq = events.next_seq;
        events.next_seq += 1;
        events.events.push_back(AgentEvent {
            seq,
            tick: e.tick,
            unit: e.unit,
            kind: e.kind.name().to_owned(),
            event: e.kind,
        });
        while events.events.len() > events.capacity.max(1) {
            events.events.pop_front();
            events.dropped += 1;
        }
    }
}

/// An events query.
#[derive(Clone, Debug, Default)]
pub struct EventQuery {
    /// Only this unit's events (plus world events, which have no unit).
    pub unit: Option<SimId>,
    /// Only events after this seq.
    pub since_seq: u64,
    /// Kind names (case-insensitive); empty: [`SIGNIFICANT_KINDS`] without
    /// perception rolls; `["all"]`: everything.
    pub kinds: Vec<String>,
    /// At most this many (the oldest first).
    pub limit: usize,
}

impl EventQuery {
    fn kind_ok(&self, e: &AgentEvent) -> bool {
        if self.kinds.is_empty() {
            return SIGNIFICANT_KINDS.contains(&e.kind.as_str()) && !e.is_perception_roll();
        }
        self.kinds
            .iter()
            .any(|k| k.eq_ignore_ascii_case("all") || k.eq_ignore_ascii_case(&e.kind))
    }

    /// Whether `e` matches (ignoring `since_seq` and `limit`).
    pub fn matches(&self, e: &AgentEvent) -> bool {
        (self.unit.is_none() || e.unit.is_none() || e.unit == self.unit) && self.kind_ok(e)
    }

    /// Matching events, oldest first.
    pub fn run(&self, events: &AgentEvents) -> Vec<AgentEvent> {
        events
            .after(self.since_seq)
            .filter(|e| self.matches(e))
            .take(self.limit.max(1))
            .cloned()
            .collect()
    }

    /// How many events match (no limit).
    pub fn count(&self, events: &AgentEvents) -> usize {
        events
            .after(self.since_seq)
            .filter(|e| self.matches(e))
            .count()
    }
}

/// The events of `unit` after `since_seq` that change its situation for an
/// act referring to `targets` ([`AgentEvent::interrupts_for`]), oldest first.
pub fn interruptions_since(
    world: &World,
    unit: SimId,
    since_seq: u64,
    targets: &[SimId],
) -> Vec<AgentEvent> {
    world
        .get_resource::<AgentEvents>()
        .map_or_else(Vec::new, |ev| {
            ev.after(since_seq)
                .filter(|e| e.unit == Some(unit) && e.interrupts_for(targets))
                .cloned()
                .collect()
        })
}

/// The unit's last `n` significant events (without perception rolls),
/// oldest first. The controller's own outcomes ([`AgentEvent::is_own_outcome`])
/// are kept in preference: other events give way to them when there are
/// more than `n`.
pub fn recent(world: &World, unit: SimId, n: usize) -> Vec<AgentEvent> {
    let Some(ev) = world.get_resource::<AgentEvents>() else {
        return Vec::new();
    };
    if n == 0 {
        return Vec::new();
    }
    let mut own = Vec::new();
    let mut other = Vec::new();
    for e in ev.events.iter().rev() {
        if own.len() >= n && other.len() >= n {
            break;
        }
        if e.unit != Some(unit)
            || !SIGNIFICANT_KINDS.contains(&e.kind.as_str())
            || e.is_perception_roll()
        {
            continue;
        }
        if e.is_own_outcome() {
            if own.len() < n {
                own.push(e);
            }
        } else if other.len() < n {
            other.push(e);
        }
    }
    // Newest first; drop the oldest *other* events while too many.
    while own.len() + other.len() > n && !other.is_empty() {
        other.pop();
    }
    let mut out: Vec<AgentEvent> = own.into_iter().chain(other).cloned().collect();
    out.sort_unstable_by_key(|e| e.seq);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(seq: u64, kind: JournalKind) -> AgentEvent {
        AgentEvent {
            seq,
            tick: seq,
            unit: Some(SimId(1)),
            kind: kind.name().to_owned(),
            event: kind,
        }
    }

    #[test]
    fn queries_binary_search_their_start_and_count_drops() {
        let mut b = AgentEvents {
            capacity: 3,
            ..AgentEvents::default()
        };
        for s in 1..=5 {
            b.events.push_back(ev(s, JournalKind::Despawn));
            b.next_seq = s + 1;
            while b.events.len() > b.capacity {
                b.events.pop_front();
                b.dropped += 1;
            }
        }
        assert_eq!((b.oldest_seq(), b.last_seq(), b.dropped), (3, 5, 2));
        assert_eq!(b.start_after(0), 0);
        assert_eq!(b.start_after(3), 1);
        assert_eq!(b.start_after(9), 3);
        let q = EventQuery {
            since_seq: 3,
            kinds: vec!["all".into()],
            limit: 10,
            ..EventQuery::default()
        };
        let seqs: Vec<u64> = q.run(&b).iter().map(|e| e.seq).collect();
        assert_eq!(seqs, [4, 5]);
    }

    #[test]
    fn perceptions_interrupt_only_for_their_targets() {
        let p = ev(
            1,
            JournalKind::Perceived {
                target: SimId(7),
                via: "vision".into(),
            },
        );
        assert!(!p.interrupts());
        assert!(p.interrupts_for(&[SimId(7)]));
        assert!(!p.interrupts_for(&[SimId(8)]));
        let done = ev(
            2,
            JournalKind::OrderDone {
                order: "Move".into(),
            },
        );
        assert!(done.interrupts_for(&[]));
    }
}
