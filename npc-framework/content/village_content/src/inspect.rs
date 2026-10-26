//! The village inspector (spec §3.4): typed snapshots with the JSON shape
//! the CLI and the remote interface have always returned. Installed by
//! `VillagePlugin` as the kernel's `Inspector`, built from the kernel's
//! generic inspect helpers. Everything is ordered by `SimId` and only reads
//! the world (`&World`).

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use npc_framework::ai::BrainStats;
use npc_framework::kernel::inspect::{
    self, Inspector, LeafSnapshot, active_leaf, blackboard, recent_entries, tree_path,
};
use npc_framework::kernel::journal::JournalEntry;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::activity::Activity;
use crate::needs::VillagerNeeds;
use crate::villager::{BB_DEST, BB_TARGET};
use crate::world::Stockpile;

/// The whole sim at one tick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldSnapshot {
    /// Ticks completed (the next tick to run).
    pub tick: u64,
    /// 0-based day of `tick`.
    pub day: u64,
    /// Position within the day, 0..1000.
    pub time_permille: u32,
    pub night: bool,
    /// Every sim entity, ascending `SimId`. `recent_journal` is left empty
    /// here (use [`unit`] for one unit's history).
    pub entities: Vec<UnitSnapshot>,
    /// Every claim, ascending target.
    pub reservations: Vec<ReservationSnapshot>,
}

/// Claimants of one target, in claim order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservationSnapshot {
    pub target: SimId,
    pub claimants: Vec<SimId>,
}

/// One sim entity (a villager or a place). Fields a kind lacks are
/// `None`/empty.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitSnapshot {
    pub sim_id: SimId,
    /// `"villager"`, `"house"`, `"bed"`, `"stockpile"`, `"field"` or `"entity"`.
    pub kind: String,
    pub pos: Option<SimPos>,
    pub needs: Option<VillagerNeeds>,
    pub activity: Option<Activity>,
    pub stockpile: Option<Stockpile>,
    /// Running tree path as node names from the root (empty when idle).
    pub tree_path: Vec<String>,
    pub active_leaf: Option<LeafSnapshot>,
    /// Non-empty blackboard slots: `"target"` (slot 0), `"dest"` (slot 1),
    /// else `"slot<i>"`.
    pub blackboard: BTreeMap<String, BbValue>,
    pub order: Option<OrderKind>,
    pub brain_stats: Option<BrainStats>,
    /// Targets this unit holds, ascending.
    pub reservations_held: Vec<SimId>,
    /// This unit's latest (up to
    /// [`RECENT_JOURNAL`](npc_framework::kernel::inspect::RECENT_JOURNAL)) journal entries, oldest
    /// first (empty when the journal is off, and in [`WorldSnapshot`]).
    pub recent_journal: Vec<JournalEntry>,
}

/// Snapshot of every sim entity and resource that matters for debugging.
pub fn snapshot(world: &World) -> WorldSnapshot {
    let clock = inspect::clock(world);
    let entities = inspect::sim_entities(world)
        .into_iter()
        .map(|(id, e)| unit_of(world, id, e, false))
        .collect();
    let reservations = world
        .get_resource::<Reservations>()
        .map(|r| {
            r.claims
                .iter()
                .map(|(&target, claimants)| ReservationSnapshot {
                    target,
                    claimants: claimants.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    WorldSnapshot {
        tick: clock.tick,
        day: clock.day,
        time_permille: clock.time_permille,
        night: clock.night,
        entities,
        reservations,
    }
}

/// Snapshot of one unit, with its recent journal; `None` for an unknown id.
pub fn unit(world: &World, id: SimId) -> Option<UnitSnapshot> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    Some(unit_of(world, id, entity, true))
}

fn unit_of(world: &World, id: SimId, entity: Entity, with_journal: bool) -> UnitSnapshot {
    UnitSnapshot {
        sim_id: id,
        kind: inspect::kind_name(world, entity).to_owned(),
        pos: world.get::<SimPos>(entity).copied(),
        needs: world
            .get::<Needs>(entity)
            .and_then(|n| VillagerNeeds::of(world, n)),
        activity: world.get::<Activity>(entity).copied(),
        stockpile: world.get::<Stockpile>(entity).copied(),
        tree_path: tree_path(world, entity),
        active_leaf: active_leaf(world, entity),
        blackboard: blackboard(world, entity, slot_name),
        order: world.get::<CurrentOrder>(entity).and_then(|o| o.0),
        brain_stats: world.get::<BrainStats>(entity).copied(),
        reservations_held: world
            .get_resource::<Reservations>()
            .map(|r| r.held_by(id))
            .unwrap_or_default(),
        recent_journal: if with_journal {
            recent_entries(world, id)
        } else {
            Vec::new()
        },
    }
}

fn slot_name(i: usize) -> String {
    match i {
        _ if i == BB_TARGET.0 as usize => "target".to_owned(),
        _ if i == BB_DEST.0 as usize => "dest".to_owned(),
        _ => format!("slot{i}"),
    }
}

/// The kernel `Inspector` serving these snapshots as JSON.
pub fn inspector() -> Inspector {
    Inspector {
        unit: |world, id| unit(world, id).and_then(|u| serde_json::to_value(u).ok()),
        world: |world| serde_json::to_value(snapshot(world)).unwrap_or(Value::Null),
    }
}
