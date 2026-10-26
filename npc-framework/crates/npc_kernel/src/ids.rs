//! Stable simulation identities. `Entity` ids differ between machines; `SimId`s
//! are allocated by the sim in a deterministic order.

use std::collections::BTreeMap;

use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::journal::{self, JournalKind, record_deferred};
use crate::math::{PrevSimPos, SimPos};
use crate::snapshot::SnapshotLoading;

/// Stable id of a sim entity. Kept current in [`SimIndex`] by component hooks:
/// `on_insert` maps the id to the entity (fires on add and on replace),
/// `on_discard` unmaps the old value and runs every registered
/// [`SimIdHooks::on_leave`] hook (e.g. `npc_jobs` releases the reservations
/// made by it or on it; fires on replace, remove and despawn).
/// Using insert/discard rather than add/remove also keeps the index right when
/// a `SimId` is overwritten in place. `on_despawn` journals `Despawn`.
/// While [`SnapshotLoading`] exists (a snapshot load replacing the world), the
/// leave hooks and the `Despawn` entry are skipped; the index is still kept.
///
/// Serializes as a plain number (`5`, not `{"0":5}`).
#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Reflect,
)]
#[serde(transparent)]
#[reflect(Component)]
#[component(
    on_insert = sim_id_inserted,
    on_discard = sim_id_discarded,
    on_despawn = sim_id_despawned
)]
// BRP compatibility (ADR 0009): the reflected type path from before the
// P3 crate split, so `world.*` queries keep working.
#[type_path = "npc_sim::ids"]
pub struct SimId(pub u32);

fn sim_id_inserted(mut world: DeferredWorld, ctx: HookContext) {
    let Some(&id) = world.get::<SimId>(ctx.entity) else {
        return;
    };
    if let Some(mut index) = world.get_resource_mut::<SimIndex>() {
        index.map.insert(id, ctx.entity);
    }
}

fn sim_id_discarded(mut world: DeferredWorld, ctx: HookContext) {
    let Some(&id) = world.get::<SimId>(ctx.entity) else {
        return;
    };
    let Some(mut index) = world.get_resource_mut::<SimIndex>() else {
        return;
    };
    // Only act if the id still points at this entity (a duplicate SimId
    // inserted elsewhere later must not be unmapped by this one leaving).
    if index.map.get(&id) != Some(&ctx.entity) {
        return;
    }
    index.map.remove(&id);
    // A snapshot load replaces the world: nothing leaves the sim.
    if world.contains_resource::<SnapshotLoading>() {
        return;
    }
    // The entity leaves the sim: generic crates drop what refers to it (its
    // running leaf's stop is never called when it is despawned mid-branch).
    let hooks = world
        .get_resource::<SimIdHooks>()
        .map(|h| h.on_leave.clone())
        .unwrap_or_default();
    for (_, hook) in hooks {
        hook(&mut world, id, ctx.entity);
    }
}

/// Called when a `SimId` leaves the sim (despawn, removal or replacement),
/// after it was unmapped from [`SimIndex`], with the entity it leaves:
/// its components are still readable (a despawn removes them after the
/// hooks), so a hook can see what the leaver held or where it stood.
/// Structural changes go through `world.commands()`.
pub type OnLeaveFn = fn(&mut DeferredWorld, SimId, Entity);

/// Hooks run by `SimId`'s `on_discard`, sorted by name (registration order
/// never matters). Registered with
/// [`KernelAppExt::on_sim_id_leave`](crate::registry::KernelAppExt::on_sim_id_leave).
#[derive(Resource, Clone, Debug, Default)]
pub struct SimIdHooks {
    pub on_leave: Vec<(&'static str, OnLeaveFn)>,
}

impl SimIdHooks {
    /// Adds (or replaces) hook `name`, keeping the list sorted by name.
    pub fn add(&mut self, name: &'static str, hook: OnLeaveFn) {
        match self.on_leave.binary_search_by_key(&name, |(n, _)| n) {
            Ok(i) => self.on_leave[i].1 = hook,
            Err(i) => self.on_leave.insert(i, (name, hook)),
        }
    }
}

/// Journals `Despawn` (before `on_discard` releases the entity's claims).
fn sim_id_despawned(mut world: DeferredWorld, ctx: HookContext) {
    let Some(&id) = world.get::<SimId>(ctx.entity) else {
        return;
    };
    if world.contains_resource::<SnapshotLoading>() {
        return;
    }
    let indexed = world
        .get_resource::<SimIndex>()
        .and_then(|index| index.get(id));
    if indexed == Some(ctx.entity) {
        record_deferred(&mut world, Some(id), || JournalKind::Despawn);
    }
}

/// Hands out `SimId`s in increasing order starting at 1.
#[derive(Resource, Debug, Default, Serialize, Deserialize)]
pub struct SimIdAllocator {
    /// Next id to hand out. 0 (the default) is treated as 1: `SimId(0)` is never allocated.
    pub next: u32,
}

impl SimIdAllocator {
    pub fn alloc(&mut self) -> SimId {
        let id = self.next.max(1);
        self.next = id.checked_add(1).expect("SimId space exhausted");
        SimId(id)
    }
}

/// `SimId → Entity` lookup, kept current by `SimId`'s hooks.
#[derive(Resource, Debug, Default)]
pub struct SimIndex {
    pub map: BTreeMap<SimId, Entity>,
}

impl SimIndex {
    pub fn get(&self, id: SimId) -> Option<Entity> {
        self.map.get(&id).copied()
    }
}

/// Allocates a `SimId` and spawns `bundle` with it. The way every sim entity is created.
///
/// Initialises `SimIdAllocator`/`SimIndex` if missing. If the spawned entity has a
/// `SimPos` but no `PrevSimPos`, a `PrevSimPos` equal to the spawn position is
/// added so render interpolation does not start from the origin.
/// Journals `Spawn { kind }` when the journal is on (`kind` from the
/// registered kind names, see [`inspect::kind_name`](crate::inspect::kind_name)).
pub fn spawn_sim(world: &mut World, bundle: impl Bundle) -> (SimId, Entity) {
    world.init_resource::<SimIndex>();
    let id = world.get_resource_or_init::<SimIdAllocator>().alloc();
    let mut entity = world.spawn((bundle, id));
    if !entity.contains::<PrevSimPos>()
        && let Some(&pos) = entity.get::<SimPos>()
    {
        entity.insert(PrevSimPos(pos));
    }
    let entity = entity.id();
    if journal::is_recording(world) && !world.contains_resource::<SnapshotLoading>() {
        let kind = crate::inspect::kind_name(world, entity).to_owned();
        journal::record(world, Some(id), || JournalKind::Spawn { kind });
    }
    (id, entity)
}
