//! World snapshots (ADR 0013): save the sim state between ticks, load it into
//! an app freshly built from the same scenario and seed.
//!
//! [`SnapshotRegistry`] lists, sorted by stable name, a save and a load
//! function for every component and resource that decides the next tick,
//! plus rebuild functions for derived state. It is separate from the hash
//! registry and **not part of [`ContentHash`](crate::registry::ContentHash)**,
//! so registering a snapshot entry never changes the content hash or a replay.
//!
//! - `register_sim_component::<T>()` (and `_with`) registers `T` under
//!   `SimHash::NAME`; `register_sim_resource::<R>()` registers `R` under
//!   `SimHashResource::NAME` when `R` opts in through
//!   [`SimHashResource::snapshot_fns`](crate::hash::SimHashResource::snapshot_fns).
//! - Everything else registers through [`SnapshotAppExt`] or the registry's
//!   `add_*` methods, always under an explicit stable name (never a Rust type
//!   path: the name is the key in saved files).
//!
//! [`save`] reads the world; [`load`] replaces its sim state (the steps are on
//! [`load`]). Both run between ticks, never inside `NpcSimTick`.

use std::any::TypeId;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use bevy_app::App;
use bevy_ecs::change_detection::{MAX_CHANGE_AGE, Tick};
use bevy_ecs::component::ComponentId;
use bevy_ecs::prelude::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use npc_ai::{ActiveLeaf, AiRegistry, Blackboard, Brain, BrainState, BrainStats, ThinkRequest};

use crate::clock::SimClock;
use crate::commands::{CommandQueue, SimCommand};
use crate::driver::HostPosition;
use crate::ids::{SimId, SimIdAllocator, SimIndex};
use crate::journal::Journal;
use crate::math::{PrevSimPos, SimPos};
use crate::registry::content_hash;

/// Format version of [`WorldSnapshot`]. Bump it when the format changes.
pub const SNAPSHOT_VERSION: u32 = 1;

/// The saved sim state of a world at a tick boundary.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorldSnapshot {
    /// [`SNAPSHOT_VERSION`] of the build that saved it.
    pub version: u32,
    /// The content hash of the build that saved it.
    pub content_hash: u64,
    /// `SimClock.tick` when saved (the next tick to run).
    pub tick: u64,
    /// Host data (agent name, world name, dates, scenario, seed). The sim never reads it.
    pub header: Map<String, Value>,
    /// Every sim entity, sorted by `id`.
    pub entities: Vec<EntitySnapshot>,
    /// Every registered resource that was present, by name.
    pub resources: BTreeMap<String, Value>,
}

/// One sim entity: its `SimId` and its registered components by name.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EntitySnapshot {
    pub id: u64,
    pub components: BTreeMap<String, Value>,
}

/// Saves `entity`'s component, if it has one.
pub type SaveComponentFn = fn(&World, Entity) -> Option<Value>;
/// Inserts a component decoded from its saved value.
pub type LoadComponentFn = fn(&mut EntityWorldMut, Value) -> Result<(), String>;
/// Saves a resource, if present.
pub type SaveResourceFn = fn(&World) -> Option<Value>;
/// Inserts a resource decoded from its saved value.
pub type LoadResourceFn = fn(&mut World, Value) -> Result<(), String>;
/// Rebuilds derived state after a load.
pub type RebuildFn = fn(&mut World);

/// A resource's save and load functions (see [`serde_resource_fns`]).
#[derive(Clone, Copy, Debug)]
pub struct ResourceSnapshotFns {
    pub save: SaveResourceFn,
    pub load: LoadResourceFn,
}

#[derive(Clone, Copy)]
struct ComponentEntry {
    name: &'static str,
    /// The serde type, for idempotent re-registration (`None`: custom functions).
    type_id: Option<TypeId>,
    save: SaveComponentFn,
    load: LoadComponentFn,
}

#[derive(Clone, Copy)]
struct ResourceEntry {
    name: &'static str,
    type_id: Option<TypeId>,
    save: SaveResourceFn,
    load: LoadResourceFn,
}

/// What a snapshot holds, sorted by name. Not part of the content hash.
#[derive(Resource, Default, Clone)]
pub struct SnapshotRegistry {
    components: Vec<ComponentEntry>,
    resources: Vec<ResourceEntry>,
    rebuilds: Vec<(&'static str, RebuildFn)>,
}

fn save_component<T: Component + Serialize>(world: &World, entity: Entity) -> Option<Value> {
    let value = world.get::<T>(entity)?;
    Some(serde_json::to_value(value).unwrap_or_else(|e| {
        panic!(
            "snapshot component `{}` does not serialize to JSON: {e}",
            std::any::type_name::<T>()
        )
    }))
}

fn load_component<T: Component + DeserializeOwned>(
    entity: &mut EntityWorldMut,
    value: Value,
) -> Result<(), String> {
    let c: T = serde_json::from_value(value).map_err(|e| e.to_string())?;
    entity.insert(c);
    Ok(())
}

fn save_resource<R: Resource + Serialize>(world: &World) -> Option<Value> {
    let value = world.get_resource::<R>()?;
    Some(serde_json::to_value(value).unwrap_or_else(|e| {
        panic!(
            "snapshot resource `{}` does not serialize to JSON: {e}",
            std::any::type_name::<R>()
        )
    }))
}

fn load_resource<R: Resource + DeserializeOwned>(
    world: &mut World,
    value: Value,
) -> Result<(), String> {
    let r: R = serde_json::from_value(value).map_err(|e| e.to_string())?;
    world.insert_resource(r);
    Ok(())
}

/// Save/load of a serde resource, for
/// [`SimHashResource::snapshot_fns`](crate::hash::SimHashResource::snapshot_fns).
pub fn serde_resource_fns<R: Resource + Serialize + DeserializeOwned>() -> ResourceSnapshotFns {
    ResourceSnapshotFns {
        save: save_resource::<R>,
        load: load_resource::<R>,
    }
}

impl SnapshotRegistry {
    /// Registers component `T` under `name` (serde JSON). Registering the
    /// same `T` under the same name again is a no-op (a plugin added twice,
    /// or a component that is saved before a capability starts hashing it).
    ///
    /// # Panics
    /// When `name` is already registered for another type or with custom
    /// functions.
    pub fn add_component<T: Component + Serialize + DeserializeOwned>(
        &mut self,
        name: &'static str,
    ) {
        self.insert_component(ComponentEntry {
            name,
            type_id: Some(TypeId::of::<T>()),
            save: save_component::<T>,
            load: load_component::<T>,
        });
    }

    /// Registers a component entry with its own functions.
    ///
    /// # Panics
    /// When `name` is already registered as a component.
    pub fn add_component_fns(
        &mut self,
        name: &'static str,
        save: SaveComponentFn,
        load: LoadComponentFn,
    ) {
        self.insert_component(ComponentEntry {
            name,
            type_id: None,
            save,
            load,
        });
    }

    fn insert_component(&mut self, entry: ComponentEntry) {
        let name = entry.name;
        match self.components.binary_search_by_key(&name, |c| c.name) {
            Ok(i) if entry.type_id.is_some() && self.components[i].type_id == entry.type_id => {}
            Ok(_) => panic!("snapshot component name `{name}` is registered twice"),
            Err(i) => self.components.insert(i, entry),
        }
    }

    /// Registers resource `R` under `name` (serde JSON); the same `R` under
    /// the same name again is a no-op.
    ///
    /// # Panics
    /// When `name` is already registered for another type or with custom
    /// functions.
    pub fn add_resource<R: Resource + Serialize + DeserializeOwned>(&mut self, name: &'static str) {
        self.insert_resource(ResourceEntry {
            name,
            type_id: Some(TypeId::of::<R>()),
            save: save_resource::<R>,
            load: load_resource::<R>,
        });
    }

    /// Registers a resource entry with its own functions (state in a type the
    /// registering crate does not own, or only part of a resource).
    ///
    /// # Panics
    /// When `name` is already registered as a resource.
    pub fn add_resource_fns(
        &mut self,
        name: &'static str,
        save: SaveResourceFn,
        load: LoadResourceFn,
    ) {
        self.insert_resource(ResourceEntry {
            name,
            type_id: None,
            save,
            load,
        });
    }

    fn insert_resource(&mut self, entry: ResourceEntry) {
        let name = entry.name;
        match self.resources.binary_search_by_key(&name, |r| r.name) {
            Ok(i) if entry.type_id.is_some() && self.resources[i].type_id == entry.type_id => {}
            Ok(_) => panic!("snapshot resource name `{name}` is registered twice"),
            Err(i) => self.resources.insert(i, entry),
        }
    }

    /// Adds (or replaces) rebuild function `name`, run in name order at the
    /// end of a load, after change detection is settled (a component it marks
    /// changed looks changed to the next tick).
    pub fn add_rebuild(&mut self, name: &'static str, f: RebuildFn) {
        match self.rebuilds.binary_search_by_key(&name, |(n, _)| n) {
            Ok(i) => self.rebuilds[i].1 = f,
            Err(i) => self.rebuilds.insert(i, (name, f)),
        }
    }

    /// Registered component names, sorted.
    pub fn component_names(&self) -> Vec<&'static str> {
        self.components.iter().map(|c| c.name).collect()
    }

    /// Registered resource names, sorted.
    pub fn resource_names(&self) -> Vec<&'static str> {
        self.resources.iter().map(|r| r.name).collect()
    }

    fn component(&self, name: &str) -> Option<&ComponentEntry> {
        self.components
            .binary_search_by_key(&name, |c| c.name)
            .ok()
            .map(|i| &self.components[i])
    }
}

/// Registration of snapshot entries on `App` (ADR 0013). Every entry has an
/// explicit, stable snake_case name (like the hash registry's): the name is
/// the key in saved files, so it must never follow a Rust type path.
pub trait SnapshotAppExt {
    /// Saves component `T` (serde JSON) under `name`.
    fn register_snapshot_component_named<T: Component + Serialize + DeserializeOwned>(
        &mut self,
        name: &'static str,
    ) -> &mut Self;
    /// Saves component state `name` with its own functions.
    fn register_snapshot_component_with(
        &mut self,
        name: &'static str,
        save: SaveComponentFn,
        load: LoadComponentFn,
    ) -> &mut Self;
    /// Saves resource `R` (serde JSON) under `name`.
    fn register_snapshot_resource_named<R: Resource + Serialize + DeserializeOwned>(
        &mut self,
        name: &'static str,
    ) -> &mut Self;
    /// Saves resource state `name` with its own functions.
    fn register_snapshot_resource_with(
        &mut self,
        name: &'static str,
        save: SaveResourceFn,
        load: LoadResourceFn,
    ) -> &mut Self;
    /// Runs `f` after every load, in name order (rebuilds derived state).
    fn add_snapshot_rebuild(&mut self, name: &'static str, f: RebuildFn) -> &mut Self;
}

impl SnapshotAppExt for App {
    fn register_snapshot_component_named<T: Component + Serialize + DeserializeOwned>(
        &mut self,
        name: &'static str,
    ) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<SnapshotRegistry>()
            .add_component::<T>(name);
        self
    }

    fn register_snapshot_component_with(
        &mut self,
        name: &'static str,
        save: SaveComponentFn,
        load: LoadComponentFn,
    ) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<SnapshotRegistry>()
            .add_component_fns(name, save, load);
        self
    }

    fn register_snapshot_resource_named<R: Resource + Serialize + DeserializeOwned>(
        &mut self,
        name: &'static str,
    ) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<SnapshotRegistry>()
            .add_resource::<R>(name);
        self
    }

    fn register_snapshot_resource_with(
        &mut self,
        name: &'static str,
        save: SaveResourceFn,
        load: LoadResourceFn,
    ) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<SnapshotRegistry>()
            .add_resource_fns(name, save, load);
        self
    }

    fn add_snapshot_rebuild(&mut self, name: &'static str, f: RebuildFn) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<SnapshotRegistry>()
            .add_rebuild(name, f);
        self
    }
}

/// Registers the kernel's own snapshot entries: resources `sim_clock`,
/// `sim_id_allocator`, `command_queue`; the `npc_ai` components `brain` (by
/// tree name), `brain_state`, `active_leaf`, `blackboard`, `brain_stats`,
/// `think_request`; the Relaxed-mode `host_position`.
pub(crate) fn register_kernel_entries(app: &mut App) {
    let mut registry = app.world_mut().get_resource_or_init::<SnapshotRegistry>();
    registry.add_resource::<SimClock>("sim_clock");
    registry.add_resource::<SimIdAllocator>("sim_id_allocator");
    registry.add_resource_fns("command_queue", save_command_queue, load_command_queue);
    registry.add_component_fns("brain", save_brain, load_brain);
    registry.add_component::<BrainState>("brain_state");
    registry.add_component::<ActiveLeaf>("active_leaf");
    registry.add_component::<Blackboard>("blackboard");
    registry.add_component::<BrainStats>("brain_stats");
    registry.add_component::<ThinkRequest>("think_request");
    registry.add_component::<HostPosition>("host_position");
}

/// The saved form of [`CommandQueue`] (JSON maps need string keys).
#[derive(Serialize, Deserialize)]
struct SavedCommandQueue {
    /// `(tick, seq, command)`, ascending.
    pending: Vec<(u64, u32, SimCommand)>,
    applied_through: Option<u64>,
}

fn save_command_queue(world: &World) -> Option<Value> {
    let queue = world.get_resource::<CommandQueue>()?;
    let saved = SavedCommandQueue {
        pending: queue
            .pending
            .iter()
            .map(|(&(tick, seq), cmd)| (tick, seq, cmd.clone()))
            .collect(),
        applied_through: queue.applied_through,
    };
    Some(serde_json::to_value(saved).expect("the command queue serializes to JSON"))
}

/// Replaces the queue (the build's own pending commands are dropped).
fn load_command_queue(world: &mut World, value: Value) -> Result<(), String> {
    let saved: SavedCommandQueue = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let mut queue = world.get_resource_or_init::<CommandQueue>();
    queue.pending = saved
        .pending
        .into_iter()
        .map(|(tick, seq, cmd)| ((tick, seq), cmd))
        .collect();
    queue.applied_through = saved.applied_through;
    Ok(())
}

/// `Brain` by tree name (tree ids follow the registered names).
fn save_brain(world: &World, entity: Entity) -> Option<Value> {
    let brain = world.get::<Brain>(entity)?;
    let registry = world.get_resource::<AiRegistry>()?;
    Some(Value::from(registry.tree(brain.tree).name.clone()))
}

fn load_brain(entity: &mut EntityWorldMut, value: Value) -> Result<(), String> {
    let name = value.as_str().ok_or("brain: expected a tree name")?;
    let tree = entity
        .world()
        .get_resource::<AiRegistry>()
        .and_then(|r| r.tree_id(name))
        .ok_or_else(|| format!("brain: unknown tree `{name}`"))?;
    // `Brain` requires `BrainState`/`Blackboard`/`BrainStats`: keep the
    // values loaded before it (names sort `blackboard` < `brain` <
    // `brain_state` < `brain_stats`, so only the blackboard is already there).
    entity.insert(Brain { tree });
    Ok(())
}

/// Present while [`load`] replaces the world: the `SimId` leave hooks and the
/// `Spawn`/`Despawn` journal entries are skipped (nothing leaves the sim).
#[derive(Resource, Debug, Default)]
pub struct SnapshotLoading;

/// How [`load`] treats a snapshot from another build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadMode {
    /// Refuse another format version or content hash.
    Strict,
    /// Load what still fits and report the rest.
    Lenient,
}

/// What a load could not apply as saved.
#[derive(Debug, Default, Serialize)]
pub struct LoadReport {
    /// Names in the snapshot this build does not know (and, in lenient mode,
    /// the version or content hash mismatch).
    pub skipped: Vec<String>,
    /// Registered resources missing from the snapshot (they keep their built value).
    pub defaulted: Vec<String>,
}

/// Why [`load`] refused a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// Another format version (strict mode).
    Version { found: u32, expected: u32 },
    /// Another content hash: the game changed (strict mode).
    ContentHash { found: u64, expected: u64 },
    /// A malformed snapshot or a value that does not decode.
    Bad(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Version { found, expected } => write!(
                f,
                "snapshot format version {found} differs from this build's {expected}"
            ),
            LoadError::ContentHash { found, expected } => write!(
                f,
                "snapshot content hash {found:016x} differs from this build's {expected:016x}"
            ),
            LoadError::Bad(msg) => write!(f, "bad snapshot: {msg}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// The version and content-hash mismatches of `snap` against this build
/// (version first), shared by [`load`] and [`check`].
fn header_problems(world: &mut World, snap: &WorldSnapshot) -> Vec<LoadError> {
    let mut problems = Vec::new();
    if snap.version != SNAPSHOT_VERSION {
        problems.push(LoadError::Version {
            found: snap.version,
            expected: SNAPSHOT_VERSION,
        });
    }
    let expected = content_hash(world);
    if snap.content_hash != expected {
        problems.push(LoadError::ContentHash {
            found: snap.content_hash,
            expected,
        });
    }
    problems
}

/// Whether `snap` fits this build (format version, then content hash)
/// without loading it: the same strict checks [`load`] makes first.
///
/// # Errors
/// [`LoadError::Version`] or [`LoadError::ContentHash`].
pub fn check(world: &mut World, snap: &WorldSnapshot) -> Result<(), LoadError> {
    header_problems(world, snap)
        .into_iter()
        .next()
        .map_or(Ok(()), Err)
}

/// Writes `snap` as JSON to `path` atomically: to `path.with_extension("json.tmp")`
/// first, then renamed over `path` (which replaces an existing file, also on
/// Windows), so a crash never leaves a half-written save.
///
/// # Errors
/// Any I/O error; the temporary file is removed on a failed write.
pub fn write_atomic(path: &Path, snap: &WorldSnapshot) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(snap).map_err(std::io::Error::from)?;
    // `sync_all` before the rename: otherwise a crash can leave the new name
    // pointing at data that never reached the disk.
    let result = write_synced(&tmp, &bytes).and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Writes `bytes` to `path` and flushes them to the disk.
fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Reads a save written by [`write_atomic`].
///
/// # Errors
/// A message naming `path` and starting `unreadable save` when the file is
/// missing, truncated or not a snapshot.
pub fn read(path: &Path) -> Result<WorldSnapshot, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("unreadable save {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("unreadable save {}: {e}", path.display()))
}

/// Sim entities in ascending `SimId` order.
fn sim_entities(world: &mut World) -> Vec<(SimId, Entity)> {
    let mut query = world.query::<(&SimId, Entity)>();
    let mut rows: Vec<(SimId, Entity)> = query.iter(world).map(|(&id, e)| (id, e)).collect();
    rows.sort_unstable_by_key(|&(id, _)| id);
    rows
}

/// Saves the sim state: every `SimId` entity (ascending) with its registered
/// components, and every registered resource that is present. Reads only
/// (`&mut` freezes the content hash and runs the query); call it between ticks.
/// The `header` is empty: the host fills it.
///
/// # Errors
/// When two entities share a `SimId`.
///
/// # Panics
/// When a registered value does not serialize to JSON (e.g. a map with
/// non-string keys): a registration bug.
pub fn save(world: &mut World) -> Result<WorldSnapshot, String> {
    let content_hash = content_hash(world);
    let rows = sim_entities(world);
    if let Some(w) = rows.windows(2).find(|w| w[0].0 == w[1].0) {
        return Err(format!("SimId {} is on two entities", w[0].0.0));
    }
    let registry = world
        .get_resource::<SnapshotRegistry>()
        .cloned()
        .unwrap_or_default();
    let entities = rows
        .iter()
        .map(|&(id, e)| EntitySnapshot {
            id: u64::from(id.0),
            components: registry
                .components
                .iter()
                .filter_map(|c| Some((c.name.to_owned(), (c.save)(world, e)?)))
                .collect(),
        })
        .collect();
    let resources = registry
        .resources
        .iter()
        .filter_map(|r| Some((r.name.to_owned(), (r.save)(world)?)))
        .collect();
    Ok(WorldSnapshot {
        version: SNAPSHOT_VERSION,
        content_hash,
        tick: world.get_resource::<SimClock>().map_or(0, |c| c.tick),
        header: Map::new(),
        entities,
        resources,
    })
}

fn push_unique(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
    }
}

/// Replaces `world`'s sim state with `snap`. `world` is an app built from the
/// snapshot's scenario and seed (content, registries and statics come from
/// the build). Steps, in order:
///
/// 1. check the version and the content hash (`Strict`: error; `Lenient`:
///    continue and list the mismatch in `skipped`), and that the entity ids
///    are strictly ascending valid `SimId`s (`1..=u32::MAX`);
/// 2. insert [`SnapshotLoading`];
/// 3. despawn every `SimId` entity in id order;
/// 4. restore the registered resources (unknown names → `skipped`, registered
///    names absent from the snapshot → `defaulted`, keeping the built value),
///    then set `SimClock.tick = snap.tick`;
/// 5. spawn each entity as `(SimId(id),)` and load its components (unknown → `skipped`);
/// 6. set `PrevSimPos = SimPos` on entities with `SimPos`, and move
///    `SimIdAllocator.next` past the highest loaded id;
/// 7. settle change detection: loaded components look unchanged to every
///    system, so the next tick sees only its own changes (ADR 0013);
/// 8. run the rebuild functions in name order (after the settle step, so a
///    component a rebuild marks changed stays changed for the next tick);
/// 9. clear the journal (and pending AI trace events), remove [`SnapshotLoading`].
///
/// # Errors
/// [`LoadError::Version`] / [`LoadError::ContentHash`] in strict mode, and
/// [`LoadError::Bad`] for unsorted, duplicate, zero or out-of-range ids (the
/// world is untouched in all these cases); [`LoadError::Bad`] for a value
/// that does not decode (lenient mode skips an undecodable value instead).
/// After a decode `Bad` the world is half replaced: build a fresh app before
/// trying again.
pub fn load(
    world: &mut World,
    snap: &WorldSnapshot,
    mode: LoadMode,
) -> Result<LoadReport, LoadError> {
    let mut report = LoadReport::default();
    for problem in header_problems(world, snap) {
        match mode {
            LoadMode::Strict => return Err(problem),
            LoadMode::Lenient => report.skipped.push(problem.to_string()),
        }
    }
    if let Some(w) = snap.entities.windows(2).find(|w| w[0].id >= w[1].id) {
        return Err(LoadError::Bad(format!(
            "entity ids are not strictly ascending ({} then {})",
            w[0].id, w[1].id
        )));
    }
    if let Some(e) = snap
        .entities
        .iter()
        .find(|e| e.id == 0 || u32::try_from(e.id).is_err())
    {
        return Err(LoadError::Bad(format!(
            "entity id {} is not a valid SimId",
            e.id
        )));
    }
    let registry = world
        .get_resource::<SnapshotRegistry>()
        .cloned()
        .unwrap_or_default();

    world.insert_resource(SnapshotLoading);
    let result = replace(world, snap, mode, &registry, &mut report);
    world.remove_resource::<SnapshotLoading>();
    result.map(|()| report)
}

/// Makes the loaded world look unchanged to every system (ADR 0013, "Change
/// detection after a load"): every component of every sim entity gets added
/// and changed ticks [`MAX_CHANGE_AGE`] in the past, which no system's last
/// run is older than (a system that never ran counts as that old too, and
/// equal is not newer), and the removal buffers of the replaced build are
/// dropped. The first tick after a load sees only its own changes.
///
/// Bevy 0.20 gives no mutable access to the systems of an initialized
/// schedule (`Schedule::systems` is read-only), so the systems' last-run
/// ticks cannot be moved forward; ageing the components is the equivalent.
fn settle_change_detection(world: &mut World) {
    // `clear_trackers` swaps the removal buffers once: twice empties both.
    world.clear_trackers();
    world.clear_trackers();
    let old = Tick::new(world.change_tick().get().wrapping_sub(MAX_CHANGE_AGE));
    let entities: Vec<Entity> = sim_entities(world).into_iter().map(|(_, e)| e).collect();
    for entity in entities {
        let ids: Vec<ComponentId> = world.entity(entity).archetype().components().to_vec();
        let mut entity = world.entity_mut(entity);
        for id in ids {
            // Immutable components cannot be borrowed mutably; none is
            // change-detected by the sim.
            if let Ok(mut value) = entity.get_mut_by_id(id) {
                value.set_last_added(old);
            }
        }
    }
}

fn replace(
    world: &mut World,
    snap: &WorldSnapshot,
    mode: LoadMode,
    registry: &SnapshotRegistry,
    report: &mut LoadReport,
) -> Result<(), LoadError> {
    // A value that does not decode: an error in strict mode, skipped in lenient.
    let undecodable = |report: &mut LoadReport, what: String, err: String| match mode {
        LoadMode::Strict => Err(LoadError::Bad(format!("{what}: {err}"))),
        LoadMode::Lenient => {
            push_unique(&mut report.skipped, what);
            Ok(())
        }
    };

    for (_, entity) in sim_entities(world) {
        // An earlier despawn may have taken it along (a child of another).
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }

    for name in snap.resources.keys() {
        if registry
            .resources
            .binary_search_by_key(&name.as_str(), |r| r.name)
            .is_err()
        {
            push_unique(&mut report.skipped, name.clone());
        }
    }
    for r in &registry.resources {
        match snap.resources.get(r.name) {
            Some(value) => {
                if let Err(e) = (r.load)(world, value.clone()) {
                    undecodable(report, r.name.to_owned(), e)?;
                }
            }
            None => report.defaulted.push(r.name.to_owned()),
        }
    }
    world.get_resource_or_init::<SimClock>().tick = snap.tick;

    world.init_resource::<SimIndex>();
    let mut max_id = 0;
    for e in &snap.entities {
        // Checked by `load` before anything changed.
        let id = u32::try_from(e.id).expect("entity ids were range-checked");
        max_id = max_id.max(id);
        let mut entity = world.spawn((SimId(id),));
        for (name, value) in &e.components {
            let Some(c) = registry.component(name) else {
                push_unique(&mut report.skipped, name.clone());
                continue;
            };
            if let Err(err) = (c.load)(&mut entity, value.clone()) {
                undecodable(report, name.clone(), err)?;
            }
        }
        if let Some(&pos) = entity.get::<SimPos>() {
            entity.insert(PrevSimPos(pos));
        }
    }

    // Never hand out a loaded id again, even when the snapshot (lenient
    // mode) had no allocator or an older one.
    let mut allocator = world.get_resource_or_init::<SimIdAllocator>();
    allocator.next = allocator.next.max(max_id.saturating_add(1));

    // Settle first: a rebuild function may then mark components changed on
    // purpose (change-detection state it saved itself, e.g.
    // `npc_interaction`'s `key_replan_pending`).
    settle_change_detection(world);
    for (_, rebuild) in &registry.rebuilds {
        rebuild(world);
    }

    if let Some(mut journal) = world.get_resource_mut::<Journal>() {
        journal.clear();
    }
    if let Some(mut trace) = world.get_resource_mut::<npc_ai::AiTrace>() {
        trace.events.clear();
    }
    Ok(())
}
