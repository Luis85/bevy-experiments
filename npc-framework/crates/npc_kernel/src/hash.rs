//! Platform-independent state hash for determinism tests and desync detection,
//! driven by a registry of sim components and resources.
//!
//! Every component that is sim state is registered once
//! (`register_sim_component::<T>()`, `T: SimHash`) under a stable
//! [`SimHash::NAME`]; every hashed resource with `register_sim_resource::<R>()`.
//! [`state_hash`] then covers, in this order: `SimClock.tick`, the entity
//! count, and for every entity with a [`SimId`] in ascending `SimId` order the
//! id plus, for each registered component **in name order**, a presence tag
//! byte and (if present) the component's own encoding; then every registered
//! resource in name order. Registration order never changes the hash.
//!
//! Encoding rules for `SimHash` impls: fixed-width little-endian integers,
//! `u8` enum discriminants (declaration order), `u32` length prefixes for
//! `Vec`s, presence tags for `Option`s. Never `std::hash::DefaultHasher`.
//! The same registry feeds [`components_json`] (divergence diffs) and the
//! generic inspector.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use npc_ai::{ActiveLeaf, AiRegistry, BbValue, Blackboard, Brain, BrainState, Status};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::clock::SimClock;
use crate::ids::SimId;
use crate::math::SimPos;
use crate::snapshot::ResourceSnapshotFns;

/// A sim component (or other value) with a fixed, platform-independent hash
/// encoding and a stable name.
pub trait SimHash {
    /// Stable name: orders the state hash and keys inspect/diff JSON (e.g.
    /// `"pos"`, `"needs"`). Unique among registered components.
    const NAME: &'static str;
    /// Writes this value's encoding.
    fn sim_hash(&self, h: &mut Fnv64);
}

/// A hashed sim resource. Its per-entity part (if any) is added to that
/// entity's hash in [`entity_hashes`] and to its diff JSON, so a divergence
/// in the resource shows up on the entity it concerns.
pub trait SimHashResource: Resource {
    /// Stable name: orders resources in the state hash.
    const NAME: &'static str;
    fn sim_hash(&self, h: &mut Fnv64);
    /// The part of the resource that belongs to `id`.
    fn sim_hash_entity(&self, _id: SimId, _h: &mut Fnv64) {}
    /// `(key, json)` of the part that belongs to `id`, for diffs.
    fn entity_json(&self, _id: SimId) -> Option<(&'static str, Value)> {
        None
    }
    /// Snapshot save/load of this resource (ADR 0013). `None` (the default)
    /// leaves it out of snapshots; a serde resource returns
    /// `Some(snapshot::serde_resource_fns::<Self>())`, and
    /// `register_sim_resource` then registers it under [`Self::NAME`].
    /// Not part of the hash or the content hash.
    fn snapshot_fns() -> Option<ResourceSnapshotFns> {
        None
    }
}

/// Hashes `entity`'s component; false when it has none.
pub type HashComponentFn = fn(&World, Entity, &mut Fnv64) -> bool;
/// JSON of `entity`'s component, if it has one.
pub type JsonComponentFn = fn(&World, Entity) -> Option<Value>;

/// One registered sim component.
#[derive(Clone, Copy)]
pub struct SimComponentInfo {
    pub name: &'static str,
    pub hash: HashComponentFn,
    pub json: JsonComponentFn,
}

/// One registered sim resource.
#[derive(Clone, Copy)]
pub struct SimResourceInfo {
    pub name: &'static str,
    pub hash: fn(&World, &mut Fnv64) -> bool,
    pub hash_entity: fn(&World, SimId, &mut Fnv64),
    pub entity_json: fn(&World, SimId) -> Option<(&'static str, Value)>,
}

/// What [`state_hash`] covers, sorted by name.
#[derive(Resource, Clone, Default)]
pub struct SimHashRegistry {
    pub components: Vec<SimComponentInfo>,
    pub resources: Vec<SimResourceInfo>,
}

/// JSON of a component: its `Serialize` form, with `null` (unit structs,
/// i.e. markers) shown as `true`.
pub fn component_json<T: Component + Serialize>(world: &World, entity: Entity) -> Option<Value> {
    let value = serde_json::to_value(world.get::<T>(entity)?).ok()?;
    Some(if value.is_null() {
        Value::Bool(true)
    } else {
        value
    })
}

impl SimHashRegistry {
    /// Registers component `T` (name `T::NAME`), JSON via `Serialize`.
    pub fn add_component<T: Component + SimHash + Serialize>(&mut self) {
        self.add_component_with::<T>(component_json::<T>);
    }

    /// Registers component `T` with a custom JSON function (e.g. one that
    /// needs other resources to name its fields).
    pub fn add_component_with<T: Component + SimHash>(&mut self, json: JsonComponentFn) {
        let info = SimComponentInfo {
            name: T::NAME,
            hash: |world, entity, h| match world.get::<T>(entity) {
                Some(c) => {
                    c.sim_hash(h);
                    true
                }
                None => false,
            },
            json,
        };
        match self.components.binary_search_by_key(&T::NAME, |c| c.name) {
            Ok(_) => panic!("sim component name `{}` is registered twice", T::NAME),
            Err(i) => self.components.insert(i, info),
        }
    }

    /// Registers resource `R` (name `R::NAME`).
    pub fn add_resource<R: SimHashResource>(&mut self) {
        let info = SimResourceInfo {
            name: R::NAME,
            hash: |world, h| match world.get_resource::<R>() {
                Some(r) => {
                    r.sim_hash(h);
                    true
                }
                None => false,
            },
            hash_entity: |world, id, h| {
                if let Some(r) = world.get_resource::<R>() {
                    r.sim_hash_entity(id, h);
                }
            },
            entity_json: |world, id| world.get_resource::<R>()?.entity_json(id),
        };
        match self.resources.binary_search_by_key(&R::NAME, |r| r.name) {
            Ok(_) => panic!("sim resource name `{}` is registered twice", R::NAME),
            Err(i) => self.resources.insert(i, info),
        }
    }

    /// Registers a hashed resource by name with its own hash function, for
    /// state that lives in a resource type the registering crate does not
    /// own (e.g. `npc_perception` hashes the pending `Messages<Noise>` as
    /// `pending_noises`). `hash` returns false when the state is absent.
    pub fn add_resource_fn(&mut self, name: &'static str, hash: fn(&World, &mut Fnv64) -> bool) {
        let info = SimResourceInfo {
            name,
            hash,
            hash_entity: |_, _, _| {},
            entity_json: |_, _| None,
        };
        match self.resources.binary_search_by_key(&name, |r| r.name) {
            Ok(_) => panic!("sim resource name `{name}` is registered twice"),
            Err(i) => self.resources.insert(i, info),
        }
    }

    /// Every registered name (`"component"`/`"resource"`), for the content hash.
    pub fn manifest(&self) -> Vec<(&'static str, String)> {
        let mut out: Vec<(&'static str, String)> = self
            .components
            .iter()
            .map(|c| ("sim_component", c.name.to_owned()))
            .collect();
        out.extend(
            self.resources
                .iter()
                .map(|r| ("sim_resource", r.name.to_owned())),
        );
        out
    }
}

/// Sim entities in ascending `SimId` order.
fn sim_entities(world: &mut World) -> Vec<(SimId, Entity)> {
    let mut query = world.query::<(&SimId, Entity)>();
    let mut rows: Vec<(SimId, Entity)> = query.iter(world).map(|(&id, e)| (id, e)).collect();
    rows.sort_unstable_by_key(|&(id, _)| id);
    rows
}

fn hash_entity(h: &mut Fnv64, world: &World, registry: &SimHashRegistry, id: SimId, e: Entity) {
    h.u32(id.0);
    for c in &registry.components {
        let mark = h.mark();
        h.u8(1);
        if !(c.hash)(world, e, h) {
            // Absent: the tag is 0 and nothing else is written.
            h.reset_to(mark);
            h.u8(0);
        }
    }
}

fn len_u32(len: usize) -> u32 {
    u32::try_from(len).expect("more than u32::MAX hashed items")
}

/// FNV-1a 64 of the whole sim state (see the module docs for the encoding).
pub fn state_hash(world: &mut World) -> u64 {
    let rows = sim_entities(world);
    let registry = world
        .get_resource::<SimHashRegistry>()
        .cloned()
        .unwrap_or_default();
    let mut h = Fnv64::default();
    h.opt(world.get_resource::<SimClock>(), |h, clock| {
        h.u64(clock.tick)
    });
    h.u32(len_u32(rows.len()));
    for &(id, e) in &rows {
        hash_entity(&mut h, world, &registry, id, e);
    }
    for r in &registry.resources {
        let mark = h.mark();
        h.u8(1);
        if !(r.hash)(world, &mut h) {
            h.reset_to(mark);
            h.u8(0);
        }
    }
    h.0
}

/// One hash per sim entity, over the same per-entity encoding as
/// [`state_hash`] plus each registered resource's per-entity part, so a
/// reservation difference shows up on its claimant. Used to find which
/// entities differ when two runs diverge.
pub fn entity_hashes(world: &mut World) -> BTreeMap<SimId, u64> {
    let rows = sim_entities(world);
    let registry = world
        .get_resource::<SimHashRegistry>()
        .cloned()
        .unwrap_or_default();
    rows.iter()
        .map(|&(id, e)| {
            let mut h = Fnv64::default();
            hash_entity(&mut h, world, &registry, id, e);
            for r in &registry.resources {
                (r.hash_entity)(world, id, &mut h);
            }
            (id, h.0)
        })
        .collect()
}

/// Every registered sim component of `entity` as JSON keyed by its
/// [`SimHash::NAME`] (markers as `true`), plus each registered resource's
/// per-entity part (e.g. `reservations_held`). Absent components are left
/// out. This is what divergence diffs compare.
pub fn components_json(world: &World, entity: Entity) -> Map<String, Value> {
    let mut map = Map::new();
    let Some(registry) = world.get_resource::<SimHashRegistry>() else {
        return map;
    };
    for c in &registry.components {
        if let Some(v) = (c.json)(world, entity) {
            map.insert(c.name.to_owned(), v);
        }
    }
    if let Some(&id) = world.get::<SimId>(entity) {
        for r in &registry.resources {
            if let Some((key, v)) = (r.entity_json)(world, id) {
                map.insert(key.to_owned(), v);
            }
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Kernel-owned encodings
// ---------------------------------------------------------------------------

impl SimHash for SimPos {
    const NAME: &'static str = "pos";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.x);
        h.i32(self.z);
    }
}

/// `npc_ai::Brain` (the frozen id of the tree it runs; ids are sorted by
/// tree name, and the names are in the content hash). **Opt-in**
/// ([`register_brain_hash`]): see there.
impl SimHash for Brain {
    const NAME: &'static str = "brain";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u16(self.tree.0);
    }
}

/// Adds `npc_ai::Brain` (which tree a unit runs) to the state hash
/// (idempotent; call before the freeze). Off by default: the kernel always
/// hashed `BrainState`, `ActiveLeaf` and `Blackboard` but never `Brain`, and
/// registering it adds the name `brain` to the content hash and a component
/// to every entity's encoding, which would change every recorded village
/// hash and replay. Content whose units switch trees at run time (so the
/// tree is state the rest does not imply) should opt in.
pub fn register_brain_hash(app: &mut bevy_app::App) {
    let mut registry = app.world_mut().get_resource_or_init::<SimHashRegistry>();
    if registry.components.iter().any(|c| c.name == Brain::NAME) {
        return;
    }
    registry.add_component_with::<Brain>(|world, entity| {
        let brain = world.get::<Brain>(entity)?;
        let name = world
            .get_resource::<AiRegistry>()
            .filter(|r| (brain.tree.0 as usize) < r.trees.len())
            .map(|r| r.tree(brain.tree).name.to_owned());
        Some(serde_json::json!({ "tree": name.map_or_else(|| Value::from(brain.tree.0), Value::from) }))
    });
}

/// `npc_ai::BrainState` (the running path and per-node memory).
impl SimHash for BrainState {
    const NAME: &'static str = "brain_state";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(len_u32(self.running_path.len()));
        for &n in &self.running_path {
            h.u16(n);
        }
        h.u32(len_u32(self.mem.len()));
        for &m in &self.mem {
            h.u32(m);
        }
    }
}

/// `npc_ai::ActiveLeaf` (node, frozen action id, status).
impl SimHash for ActiveLeaf {
    const NAME: &'static str = "active_leaf";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u16(self.node);
        h.u16(self.action.0);
        h.u8(match self.status {
            Status::Running => 0,
            Status::Success => 1,
            Status::Failure => 2,
        });
    }
}

/// `npc_ai::Blackboard` (every slot).
impl SimHash for Blackboard {
    const NAME: &'static str = "blackboard";
    fn sim_hash(&self, h: &mut Fnv64) {
        for slot in &self.slots {
            hash_bb_value(h, *slot);
        }
    }
}

fn hash_bb_value(h: &mut Fnv64, v: BbValue) {
    match v {
        BbValue::None => h.u8(0),
        BbValue::U32(x) => {
            h.u8(1);
            h.u32(x);
        }
        BbValue::I32(x) => {
            h.u8(2);
            h.i32(x);
        }
        BbValue::Pos(x, z) => {
            h.u8(3);
            h.i32(x);
            h.i32(z);
        }
    }
}

/// Small FNV-1a 64 writer used by `state_hash`.
#[derive(Clone, Copy, Debug)]
pub struct Fnv64(pub u64);

impl Default for Fnv64 {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv64 {
    pub fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    pub fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }
    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }
    /// Presence tag (0 = absent, 1 = present) followed by `write(value)` when present.
    pub fn opt<T: ?Sized>(&mut self, value: Option<&T>, write: impl FnOnce(&mut Self, &T)) {
        match value {
            Some(v) => {
                self.u8(1);
                write(self, v);
            }
            None => self.u8(0),
        }
    }
    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }
    /// Length-prefixed (`u32`) UTF-8 string.
    pub fn str(&mut self, s: &str) {
        self.u32(len_u32(s.len()));
        self.bytes(s.as_bytes());
    }
    /// A position to rewind to (see [`Fnv64::reset_to`]).
    pub fn mark(&self) -> u64 {
        self.0
    }
    /// Rewinds to a [`Fnv64::mark`]: FNV state is the whole hash, so this
    /// undoes everything written since.
    pub fn reset_to(&mut self, mark: u64) {
        self.0 = mark;
    }
}
