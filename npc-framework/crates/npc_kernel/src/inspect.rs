//! JSON inspection of the sim (spec §3.4). Everything is ordered by `SimId`
//! and only reads the world (`&World`), so inspecting never changes sim
//! results.
//!
//! [`unit_json`] / [`world_json`] are what the CLI and the remote interface
//! return. They call the [`Inspector`] resource: the generic default shows
//! every registered sim component (the same registry as the state hash) plus
//! the AI state by name; content may install its own inspector (e.g. a typed
//! snapshot with a stable JSON shape) with `KernelAppExt::set_inspector`.
//! The building blocks below ([`tree_path`], [`active_leaf`], ...) are public
//! so a content inspector reuses them.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use npc_ai::{ActiveLeaf, AiRegistry, BbValue, Blackboard, Brain, BrainState, BrainStats, Status};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::clock::{SimClock, SimConfig};
use crate::hash::components_json;
use crate::ids::{SimId, SimIndex};
use crate::journal::{Journal, JournalEntry};

/// How many of a unit's latest journal entries a unit snapshot includes.
pub const RECENT_JOURNAL: usize = 20;

/// Names a unit kind (`Some("villager")`) or passes (`None`).
pub type KindNameFn = fn(&World, Entity) -> Option<&'static str>;

/// Registered kind namers, sorted by registration name; the first that
/// answers wins, else `"entity"`.
#[derive(Resource, Clone, Default)]
pub struct KindNames {
    pub namers: Vec<(&'static str, KindNameFn)>,
}

impl KindNames {
    pub fn add(&mut self, name: &'static str, f: KindNameFn) {
        match self.namers.binary_search_by_key(&name, |(n, _)| n) {
            Ok(i) => self.namers[i].1 = f,
            Err(i) => self.namers.insert(i, (name, f)),
        }
    }
}

/// Short kind name of a sim entity, used by the journal (`Spawn`) and by
/// inspection: what the registered [`KindNames`] say, or `"entity"`.
pub fn kind_name(world: &World, entity: Entity) -> &'static str {
    world
        .get_resource::<KindNames>()
        .and_then(|k| k.namers.iter().find_map(|(_, f)| f(world, entity)))
        .unwrap_or("entity")
}

/// The inspector behind [`unit_json`] and [`world_json`].
#[derive(Resource, Clone, Copy)]
pub struct Inspector {
    /// One unit (with its recent journal); `None` for an unknown id.
    pub unit: fn(&World, SimId) -> Option<Value>,
    /// The whole world.
    pub world: fn(&World) -> Value,
}

impl Default for Inspector {
    fn default() -> Self {
        Self {
            unit: generic_unit,
            world: generic_world,
        }
    }
}

/// One unit as JSON (the installed [`Inspector`]); `None` for an unknown id.
pub fn unit_json(world: &World, id: SimId) -> Option<Value> {
    (world
        .get_resource::<Inspector>()
        .copied()
        .unwrap_or_default()
        .unit)(world, id)
}

/// The whole world as JSON (the installed [`Inspector`]).
pub fn world_json(world: &World) -> Value {
    (world
        .get_resource::<Inspector>()
        .copied()
        .unwrap_or_default()
        .world)(world)
}

/// The running leaf, by name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeafSnapshot {
    pub node: String,
    pub action: String,
    pub status: Status,
}

/// Clock fields of a world snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockSnapshot {
    /// Ticks completed (the next tick to run).
    pub tick: u64,
    /// 0-based day of `tick`.
    pub day: u64,
    /// Position within the day, 0..1000.
    pub time_permille: u32,
    pub night: bool,
}

/// The clock part of a world snapshot.
pub fn clock(world: &World) -> ClockSnapshot {
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    let config = world
        .get_resource::<SimConfig>()
        .copied()
        .unwrap_or_default();
    ClockSnapshot {
        tick,
        day: config.day(tick),
        time_permille: config.time_of_day_permille(tick),
        night: config.is_night(tick),
    }
}

/// Every sim entity, ascending `SimId`.
pub fn sim_entities(world: &World) -> Vec<(SimId, Entity)> {
    world
        .get_resource::<SimIndex>()
        .map(|index| index.map.iter().map(|(&id, &e)| (id, e)).collect())
        .unwrap_or_default()
}

fn node_name(world: &World, entity: Entity) -> impl Fn(u16) -> String + '_ {
    let tree = world
        .get_resource::<AiRegistry>()
        .zip(world.get::<Brain>(entity))
        .map(|(r, b)| r.tree(b.tree));
    move |n: u16| {
        tree.and_then(|t| t.nodes.get(n as usize))
            .map_or_else(|| format!("#{n}"), |node| node.name.clone())
    }
}

/// Running tree path as node names from the root (empty when idle).
pub fn tree_path(world: &World, entity: Entity) -> Vec<String> {
    let name = node_name(world, entity);
    world
        .get::<BrainState>(entity)
        .map(|s| s.running_path.iter().map(|&n| name(n)).collect())
        .unwrap_or_default()
}

/// The running leaf by node and action name.
pub fn active_leaf(world: &World, entity: Entity) -> Option<LeafSnapshot> {
    let name = node_name(world, entity);
    let registry = world.get_resource::<AiRegistry>();
    world.get::<ActiveLeaf>(entity).map(|leaf| LeafSnapshot {
        node: name(leaf.node),
        action: registry
            .and_then(|r| r.actions.get(leaf.action.0 as usize))
            .map_or_else(|| format!("#{}", leaf.action.0), |a| a.name.to_owned()),
        status: leaf.status,
    })
}

/// Non-empty blackboard slots keyed by `slot_name(i)`.
pub fn blackboard(
    world: &World,
    entity: Entity,
    slot_name: impl Fn(usize) -> String,
) -> BTreeMap<String, BbValue> {
    world
        .get::<Blackboard>(entity)
        .map(|bb| {
            bb.slots
                .iter()
                .enumerate()
                .filter(|(_, v)| **v != BbValue::None)
                .map(|(i, v)| (slot_name(i), *v))
                .collect()
        })
        .unwrap_or_default()
}

/// This unit's latest (up to [`RECENT_JOURNAL`]) journal entries, oldest
/// first (empty when the journal is off).
pub fn recent_entries(world: &World, id: SimId) -> Vec<JournalEntry> {
    let Some(journal) = world.get_resource::<Journal>() else {
        return Vec::new();
    };
    let mut recent: Vec<JournalEntry> = journal
        .entries
        .iter()
        .rev()
        .filter(|e| e.unit == Some(id))
        .take(RECENT_JOURNAL)
        .cloned()
        .collect();
    recent.reverse();
    recent
}

fn generic_unit_of(world: &World, id: SimId, entity: Entity, with_journal: bool) -> Value {
    json!({
        "sim_id": id,
        "kind": kind_name(world, entity),
        "components": components_json(world, entity),
        "tree_path": tree_path(world, entity),
        "active_leaf": active_leaf(world, entity),
        "blackboard": blackboard(world, entity, |i| format!("slot{i}")),
        "brain_stats": world.get::<BrainStats>(entity).copied(),
        "recent_journal": if with_journal { recent_entries(world, id) } else { Vec::new() },
    })
}

/// The default unit inspector: every registered sim component by name.
pub fn generic_unit(world: &World, id: SimId) -> Option<Value> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    Some(generic_unit_of(world, id, entity, true))
}

/// The default world inspector.
pub fn generic_world(world: &World) -> Value {
    let c = clock(world);
    let entities: Vec<Value> = sim_entities(world)
        .into_iter()
        .map(|(id, e)| generic_unit_of(world, id, e, false))
        .collect();
    json!({
        "tick": c.tick,
        "day": c.day,
        "time_permille": c.time_permille,
        "night": c.night,
        "entities": entities,
    })
}
