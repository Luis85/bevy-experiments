//! The spawn funnel (spec §5.6, ADR-0005): every in-tick spawn is a [`SpawnRequest`] Message
//! handled in [`LifecycleSet::Spawn`], every in-tick despawn a [`DespawnRequest`] handled in
//! [`LifecycleSet::Despawn`]. Setup and load spawn outside `SimTick` with [`setup_spawn`],
//! which is the code the funnel runs. Only this module allocates [`SimId`]s.
//!
//! The funnels journal (phase `Lifecycle`) `"core.unit_spawned"` and `"core.unit_despawned"`
//! for every in-tick spawn and despawn; setup and load spawns are not journaled.

use bevy_ecs::message::{Message, Messages};
use bevy_ecs::world::{EntityWorldMut, World};
use fl_content::{ContentDb, ResolvedObject, SpawnCtx};
use fl_math::{Angle, FxVec2};
use fl_protocol::{PlayerId, SimId, UnitTypeId};
use fl_view::SimEvent;
use serde::Serialize;

use crate::build::Content;
use crate::builder::SimBuilder;
use crate::clock::SimClock;
use crate::counters::SimCounters;
use crate::events::SimEvents;
use crate::ids::{SimIdAllocator, SimIndex, spawn_with_id};
use crate::map::MapInfo;
use crate::nav::{NavGrid, def_profile};
use crate::observe::{Journal, JournalRecord};
use crate::players::Players;
use crate::schedule::{LifecycleSet, Phase};
use crate::units::{Owner, Pose, UnitType};

/// Runs on a new entity after its def components are instantiated: how a feature spawns
/// a non-unit entity (`unit: None`) or adds to a unit.
pub type SpawnInit = fn(&mut EntityWorldMut<'_>, &SpawnRequest);

/// A request to spawn an entity at the end of the tick, in [`LifecycleSet::Spawn`].
///
/// - `unit: Some(id)`: [`SimEntity`](crate::SimEntity) + [`Owner`] + [`UnitType`] +
///   [`Pose`], then every def component of the unit, in component-name order;
/// - `unit: None`: [`SimEntity`](crate::SimEntity) + [`Owner`] + [`Pose`] only;
///
/// then `init`. Message `"core.msg.spawn"`.
#[derive(Message, Clone, Debug)]
pub struct SpawnRequest {
    pub unit: Option<UnitTypeId>,
    pub owner: PlayerId,
    pub pos: FxVec2,
    pub heading: Angle,
    pub init: Option<SpawnInit>,
}

/// A request to despawn the entity with this id at the end of the tick, in
/// [`LifecycleSet::Despawn`]. Message `"core.msg.despawn"`.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DespawnRequest(pub SimId);

/// Why a [`SpawnRequest`] could not be carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SpawnError {
    #[error("no concrete unit has this id")]
    UnknownUnit,
    #[error("no such player")]
    UnknownPlayer,
    #[error("the position is outside the map")]
    OutsideMap,
    #[error("no map is set up")]
    NoMap,
    /// The unit moves, and its profile cannot stand on the position's cell
    /// ([`NavGrid::passable`]).
    #[error("the position is blocked for this unit")]
    BlockedCell,
}

/// Journal `"core.unit_spawned"`: the funnel spawned an entity (the record's sim id), of
/// unit `unit` (its content id; `None` for a non-unit entity) for `owner` at `pos`.
#[derive(Debug, Serialize)]
struct UnitSpawned {
    unit: Option<String>,
    owner: PlayerId,
    pos: FxVec2,
}

impl JournalRecord for UnitSpawned {
    const KIND: &'static str = "core.unit_spawned";
}

/// Journal `"core.unit_despawned"`: the funnel despawned the record's sim id.
#[derive(Debug, Serialize)]
struct UnitDespawned {}

impl JournalRecord for UnitDespawned {
    const KIND: &'static str = "core.unit_despawned";
}

/// Whether `req` can spawn against `content`, `map`, `nav` and `players`. Checks, in order: the
/// unit type, the map, the player, the position, and for a unit with `Mobility` that its
/// profile can stand on the position's cell (with a `nav` grid). Returns the unit's id and
/// resolved def when `req.unit` is set. [`setup_spawn`] (and with it the funnel) and scenario
/// setup's up-front check run this one function, so they cannot disagree.
///
/// # Errors
/// The first failed check.
pub(crate) fn check_spawn<'c>(
    content: &'c ContentDb,
    map: Option<&MapInfo>,
    nav: Option<&NavGrid>,
    players: &Players,
    req: &SpawnRequest,
) -> Result<Option<(UnitTypeId, &'c ResolvedObject)>, SpawnError> {
    let def = match req.unit {
        Some(unit) => Some((unit, content.unit(unit).ok_or(SpawnError::UnknownUnit)?)),
        None => None,
    };
    let map = map.ok_or(SpawnError::NoMap)?;
    if players.get(req.owner).is_none() {
        return Err(SpawnError::UnknownPlayer);
    }
    if !map.contains(req.pos) {
        return Err(SpawnError::OutsideMap);
    }
    if let (Some(nav), Some((_, def))) = (nav, def)
        && blocked_for(content, def, nav, req.pos)
    {
        return Err(SpawnError::BlockedCell);
    }
    Ok(def)
}

/// Whether a unit of `def` that moves cannot stand at `pos` on `nav`; a unit without
/// `Mobility` stands anywhere.
pub(crate) fn blocked_for(
    content: &ContentDb,
    def: &ResolvedObject,
    nav: &NavGrid,
    pos: FxVec2,
) -> bool {
    let Some(profile) = def_profile(content, def, nav.cell_m(), nav.max_clearance()) else {
        return false;
    };
    let (x, y) = nav.cell_of(pos);
    !nav.passable(profile, i64::from(x), i64::from(y))
}

/// Spawn `req` now, outside `SimTick`: the setup and load path. The spawn funnel runs this
/// same function for every request.
///
/// Checks with [`check_spawn`] first; nothing changes when a check fails.
///
/// # Errors
/// The first failed check.
///
/// # Panics
/// If `world` was not built by [`build_sim_app`](crate::build_sim_app).
pub fn setup_spawn(world: &mut World, req: &SpawnRequest) -> Result<SimId, SpawnError> {
    let content = world.resource::<Content>().0.clone();
    let def = check_spawn(
        &content,
        world.get_resource::<MapInfo>(),
        world.get_resource::<NavGrid>(),
        world.resource::<Players>(),
        req,
    )?;

    let id = world.resource_mut::<SimIdAllocator>().alloc();
    let mut entity = spawn_with_id(
        world,
        id,
        (
            Owner(req.owner),
            Pose {
                pos: req.pos,
                heading: req.heading,
            },
        ),
    );
    if let Some((unit, def)) = def {
        entity.insert(UnitType(unit));
        let ctx = SpawnCtx {
            content: &content,
            unit,
            player: req.owner,
            pos: req.pos,
            heading: req.heading,
        };
        for component in def.components.values() {
            component.instantiate(&ctx, &mut entity);
        }
    }
    if let Some(init) = req.init {
        init(&mut entity, req);
    }
    crate::spatial::file_spawn(world, id);
    tracing::debug!(%id, unit = ?req.unit, owner = req.owner.0, "spawned");
    Ok(id)
}

/// Despawn the entity with sim id `id` now, outside `SimTick`; `false` if there is none.
pub fn setup_despawn(world: &mut World, id: SimId) -> bool {
    let Some(entity) = world.resource::<SimIndex>().get(id) else {
        return false;
    };
    world.despawn(entity)
}

/// Carry out this tick's [`SpawnRequest`]s in write order through [`setup_spawn`], so
/// [`SimId`]s are allocated in request order. A success counts `"spawn.entities"`, raises
/// [`SimEvent::Spawned`] and is journaled `"core.unit_spawned"`; a failure counts
/// `"spawn.rejected"` and is logged. A request written after the funnel ran would be dropped
/// by the end-of-tick clear (R72); debug builds panic on one instead. Exclusive, in
/// [`LifecycleSet::Spawn`].
pub(crate) fn spawn_funnel(world: &mut World) {
    let requests: Vec<SpawnRequest> = world
        .resource_mut::<Messages<SpawnRequest>>()
        .drain()
        .collect();
    let tick = world.resource::<SimClock>().tick;
    for req in requests {
        match setup_spawn(world, &req) {
            Ok(sim_id) => {
                if world.resource::<Journal>().enabled() {
                    let unit = req.unit.and_then(|unit| {
                        let content = &world.resource::<Content>().0;
                        content.unit(unit).map(|def| def.id.clone())
                    });
                    let record = UnitSpawned {
                        unit,
                        owner: req.owner,
                        pos: req.pos,
                    };
                    world.resource_mut::<Journal>().record(
                        tick,
                        Phase::Lifecycle,
                        Some(sim_id),
                        Some(req.owner),
                        &record,
                    );
                }
                world.resource_mut::<SimCounters>().add("spawn.entities", 1);
                world
                    .resource_mut::<SimEvents<SimEvent>>()
                    .push(SimEvent::Spawned {
                        sim_id,
                        unit: req.unit,
                        owner: req.owner,
                    });
            }
            Err(err) => {
                world.resource_mut::<SimCounters>().add("spawn.rejected", 1);
                tracing::debug!(
                    unit = ?req.unit,
                    owner = req.owner.0,
                    pos = ?req.pos,
                    %err,
                    "spawn request failed"
                );
            }
        }
    }
}

/// Carry out this tick's [`DespawnRequest`]s: deduplicated, in [`SimId`] order (the sort
/// counts `"sort.items"`, one per request). Each despawn counts `"despawn.entities"`, raises
/// [`SimEvent::Despawned`] and is journaled `"core.unit_despawned"`; a request for an id that
/// does not exist is logged. Exclusive, in [`LifecycleSet::Despawn`].
pub(crate) fn despawn_funnel(world: &mut World) {
    let mut ids: Vec<SimId> = world
        .resource_mut::<Messages<DespawnRequest>>()
        .drain()
        .map(|req| req.0)
        .collect();
    if !ids.is_empty() {
        world
            .resource_mut::<SimCounters>()
            .add("sort.items", ids.len() as u64);
    }
    ids.sort_unstable();
    ids.dedup();
    let tick = world.resource::<SimClock>().tick;
    for sim_id in ids {
        if setup_despawn(world, sim_id) {
            world.resource_mut::<Journal>().record(
                tick,
                Phase::Lifecycle,
                Some(sim_id),
                None,
                &UnitDespawned {},
            );
            world
                .resource_mut::<SimCounters>()
                .add("despawn.entities", 1);
            world
                .resource_mut::<SimEvents<SimEvent>>()
                .push(SimEvent::Despawned { sim_id });
        } else {
            tracing::debug!(%sim_id, "despawn request for an entity that does not exist");
        }
    }
}

/// In debug builds, at the end of a tick, before `step_tick` clears the message buffers:
/// no spawn or despawn request may be left. The funnels drain both buffers, so one left over
/// was written after its funnel ran, and the end-of-tick clear would drop it silently.
///
/// # Panics
/// If a spawn or despawn request is waiting.
pub(crate) fn assert_requests_drained(world: &World) {
    let spawns = world
        .get_resource::<Messages<SpawnRequest>>()
        .map_or(0, Messages::len);
    let despawns = world
        .get_resource::<Messages<DespawnRequest>>()
        .map_or(0, Messages::len);
    assert!(
        spawns == 0 && despawns == 0,
        "tick {}: {spawns} spawn and {despawns} despawn request(s) were written after the \
         Lifecycle funnels ran; the end-of-tick clear would drop them (write requests before \
         LifecycleSet::Spawn / LifecycleSet::Despawn)",
        world.resource::<SimClock>().tick.0
    );
}

/// The request messages, the two funnels and their journal records.
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.add_message::<SpawnRequest>("core.msg.spawn")
        .add_message::<DespawnRequest>("core.msg.despawn")
        .register_journal_record::<UnitSpawned>()
        .register_journal_record::<UnitDespawned>()
        .add_systems(LifecycleSet::Spawn, spawn_funnel)
        .add_systems(LifecycleSet::Despawn, despawn_funnel);
}
