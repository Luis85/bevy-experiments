//! Doors: grid flags, the nav pass-cost hook and opening doors on a path.
//!
//! A [`Door`] entity stands in one grid cell (its `SimPos`'s cell; one door
//! per cell). [`door_sync_system`] (in `NpcSet::Inputs`, after the spatial
//! update) and every door verb keep that cell's **static** layer in step:
//! the door tag bit ([`InteractionConfig::door_tag_bit`](crate::InteractionConfig))
//! is set, a closed door makes the cell blocking and occluding, an open one
//! clears both, and a locked door sets the cell's extra cost to 1 (so
//! locking or unlocking changes the cell data: `GridChanged` makes nav agents
//! re-plan). Removing a door clears its cell.
//!
//! [`door_pass_cost`] is the nav pass-cost function for the door tag bit:
//! an open door is open ground; a closed one is passable at
//! `door_pass_cost` extra **with a path marker** for an actor that may open
//! it ([`Interactor`], and the key when it is locked), otherwise it blocks.
//! [`auto_door_system`] then makes a `NavAgent` waiting in front of its
//! next marked door open it (or unlock it first): a normal `open`/`unlock`
//! interaction (source `Auto`), journaled and replayable like any other.
//! [`key_replan_system`] retries the failed path of an agent that just got
//! the key of a locked door (that is no `GridChanged`: the door blocks only
//! agents without the key).

use std::collections::BTreeMap;

use bevy_ecs::change_detection::{ComponentTicks, Tick};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::Schedules;
use bevy_reflect::Reflect;
use npc_kernel::spatial::{Blocker, CellCoord, CellInfo, SpatialGrid};
use npc_kernel::{NpcSimTick, SimId, SimIndex, SimPos};
use npc_nav::{NavAgent, PassCost, Path};
use serde::{Deserialize, Serialize};

use crate::InteractionConfig;
use crate::items::{Equipment, Interactor, Inventory, carried_matching};
use crate::run::{Interacting, Source, begin, can_interact};

/// A door. `locked_by` is the key that unlocks it while it is locked
/// (`None`: unlocked); `key` is the key that fits its lock (what `lock`
/// needs; `None`: it cannot be locked). A key is an item whose kind name or
/// one of whose tags equals the string (e.g. `"key:gate"`).
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Door {
    pub open: bool,
    pub locked_by: Option<String>,
    pub key: Option<String>,
}

impl Door {
    /// Closed, unlocked, without a lock.
    pub fn closed() -> Self {
        Self::default()
    }

    /// Open, without a lock.
    pub fn opened() -> Self {
        Self {
            open: true,
            ..Self::default()
        }
    }

    /// Closed and locked with `key` (which also locks it again).
    pub fn locked(key: impl Into<String>) -> Self {
        let key = key.into();
        Self {
            open: false,
            locked_by: Some(key.clone()),
            key: Some(key),
        }
    }

    /// Closed, unlocked, with a lock that `key` fits.
    pub fn lockable(key: impl Into<String>) -> Self {
        Self {
            open: false,
            locked_by: None,
            key: Some(key.into()),
        }
    }

    pub fn is_locked(&self) -> bool {
        self.locked_by.is_some()
    }
}

/// Which cell each door occupies (derived from `Door` + `SimPos`; not
/// hashed).
#[derive(Resource, Clone, Debug, Default)]
pub struct DoorCells {
    pub by_cell: BTreeMap<CellCoord, Entity>,
    by_entity: BTreeMap<Entity, CellCoord>,
    /// After a snapshot load: cells of doors that were removed after the
    /// saved tick's [`door_sync_system`] (cleared by its next run, as the
    /// removal would have been).
    orphans: Vec<CellCoord>,
    /// After a snapshot load: the next [`door_sync_system`] syncs every door
    /// (change detection does not carry over a load; syncing an unchanged
    /// door changes nothing).
    resync: bool,
}

impl DoorCells {
    /// The door entity in `cell`.
    pub fn door_at(&self, cell: CellCoord) -> Option<Entity> {
        self.by_cell.get(&cell).copied()
    }
}

/// The saved [`DoorCells`] (world snapshots, ADR 0013): each synced cell
/// with its door's `SimId`, or `None` when that door was removed (or lost
/// its `Door`) after the saved tick's [`door_sync_system`]; ascending by cell.
#[derive(Resource, Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct SavedDoorCells(Vec<(CellCoord, Option<SimId>)>);

pub(crate) fn save_door_cells(world: &World) -> Option<serde_json::Value> {
    let cells = world.get_resource::<DoorCells>()?;
    let mut rows: Vec<(CellCoord, Option<SimId>)> = cells
        .by_entity
        .iter()
        .map(|(&e, &cell)| {
            let alive = world
                .get_entity(e)
                .ok()
                .filter(|e| e.contains::<Door>())
                .and_then(|e| e.get::<SimId>().copied());
            (cell, alive)
        })
        .collect();
    rows.sort_unstable();
    Some(serde_json::to_value(SavedDoorCells(rows)).expect("door cells serialize to JSON"))
}

pub(crate) fn load_door_cells(world: &mut World, value: serde_json::Value) -> Result<(), String> {
    let saved: SavedDoorCells = serde_json::from_value(value).map_err(|e| e.to_string())?;
    world.insert_resource(saved);
    Ok(())
}

/// Snapshot rebuild of [`DoorCells`]: each door at the cell it was synced to
/// (a door moved after the saved tick's sync is re-synced from there, as in
/// an uninterrupted run), the cells of doors removed since then as orphans,
/// and a full re-sync on the next [`door_sync_system`]. Without a saved
/// entry (a lenient load) every door maps to its current cell.
pub(crate) fn rebuild_door_cells(world: &mut World) {
    let saved = world.remove_resource::<SavedDoorCells>();
    let mut doors: Vec<(SimId, Entity, SimPos)> = world
        .query_filtered::<(Entity, &SimId, &SimPos), With<Door>>()
        .iter(world)
        .map(|(e, &id, &pos)| (id, e, pos))
        .collect();
    let Some(grid) = world.get_resource::<SpatialGrid>() else {
        return;
    };
    doors.sort_unstable_by_key(|r| r.0);
    let mut cells = DoorCells {
        resync: true,
        ..DoorCells::default()
    };
    match saved {
        Some(SavedDoorCells(rows)) => {
            for (cell, id) in rows {
                match id.and_then(|id| doors.iter().find(|d| d.0 == id)) {
                    Some(&(_, e, _)) => {
                        cells.by_entity.insert(e, cell);
                        cells.by_cell.insert(cell, e);
                    }
                    None => cells.orphans.push(cell),
                }
            }
        }
        None => {
            for (_, e, pos) in doors {
                let cell = grid.cell_of(pos);
                cells.by_entity.insert(e, cell);
                cells.by_cell.insert(cell, e);
            }
        }
    }
    world.insert_resource(cells);
}

/// The cell data a door makes of `prev`.
pub fn door_cell_info(prev: CellInfo, door: &Door, bit: u8) -> CellInfo {
    let mut info = prev;
    info.flags.tags |= 1 << bit;
    info.flags.blocking = !door.open;
    info.flags.occluding = !door.open;
    info.extra_cost = u16::from(door.is_locked());
    info
}

fn clear_cell(grid: &mut SpatialGrid, cell: CellCoord, bit: u8) {
    let mut info = grid.static_cell(cell);
    info.flags.tags &= !(1 << bit);
    info.flags.blocking = false;
    info.flags.occluding = false;
    info.extra_cost = 0;
    grid.set_static(cell, info);
}

fn sync_one(
    grid: &mut SpatialGrid,
    cells: &mut DoorCells,
    bit: u8,
    entity: Entity,
    door: &Door,
    pos: SimPos,
) {
    let cell = grid.cell_of(pos);
    if let Some(old) = cells.by_entity.get(&entity).copied()
        && old != cell
    {
        clear_cell(grid, old, bit);
        cells.by_cell.remove(&old);
    }
    let info = door_cell_info(grid.static_cell(cell), door, bit);
    grid.set_static(cell, info);
    cells.by_entity.insert(entity, cell);
    cells.by_cell.insert(cell, entity);
}

/// Brings `door`'s cell up to date now (the door verbs call it, so the
/// grid, perception and nav see the change in the same tick).
pub fn sync_door(world: &mut World, door: Entity) {
    let Some((d, pos)) = world
        .get::<Door>(door)
        .cloned()
        .zip(world.get::<SimPos>(door).copied())
    else {
        return;
    };
    let bit = world
        .get_resource::<InteractionConfig>()
        .copied()
        .unwrap_or_default()
        .door_tag_bit;
    world.resource_scope(|world, mut grid: Mut<SpatialGrid>| {
        let mut cells = world.get_resource_or_init::<DoorCells>();
        sync_one(&mut grid, &mut cells, bit, door, &d, pos);
    });
}

type DoorRow<'a> = (Entity, &'a SimId, &'a Door, &'a SimPos);
type DoorChanged = Or<(Changed<Door>, Changed<SimPos>)>;

/// `NpcSet::Inputs` (after the spatial update): syncs the cells of added,
/// changed or moved doors (in `SimId` order) and clears the cells of removed
/// ones. After a snapshot load it clears the cells of doors removed before
/// the save but after the saved tick's sync, and syncs every door once.
pub fn door_sync_system(
    mut grid: ResMut<SpatialGrid>,
    mut cells: ResMut<DoorCells>,
    config: Res<InteractionConfig>,
    doors: Query<DoorRow, DoorChanged>,
    all_doors: Query<DoorRow>,
    mut removed: RemovedComponents<Door>,
) {
    let bit = config.door_tag_bit;
    let mut gone: Vec<Entity> = removed.read().collect();
    gone.sort_unstable();
    gone.dedup();
    for e in gone {
        if doors.contains(e) {
            continue;
        }
        if let Some(cell) = cells.by_entity.remove(&e) {
            if cells.by_cell.get(&cell) == Some(&e) {
                cells.by_cell.remove(&cell);
            }
            clear_cell(&mut grid, cell, bit);
        }
    }
    for cell in std::mem::take(&mut cells.orphans) {
        clear_cell(&mut grid, cell, bit);
    }
    let row = |(e, &id, d, &p): (Entity, &SimId, &Door, &SimPos)| (id, e, d.clone(), p);
    let mut rows: Vec<(SimId, Entity, Door, SimPos)> = if std::mem::take(&mut cells.resync) {
        all_doors.iter().map(row).collect()
    } else {
        doors.iter().map(row).collect()
    };
    rows.sort_unstable_by_key(|r| r.0);
    for (_, e, d, p) in rows {
        sync_one(&mut grid, &mut cells, bit, e, &d, p);
    }
}

/// Whether `actor` may get through `door` (open it, unlocking first when it
/// has the key).
pub fn may_pass(world: &World, actor: Entity, door_e: Entity) -> bool {
    let Some(door) = world.get::<Door>(door_e) else {
        return true;
    };
    if door.open {
        return true;
    }
    if world.get::<Interactor>(actor).is_none() {
        return false;
    }
    if let Some(i) = world.get::<crate::Interactable>(door_e)
        && !i.offers("open")
    {
        return false;
    }
    match &door.locked_by {
        None => true,
        Some(key) => carried_matching(world, actor, key).is_some(),
    }
}

/// The nav pass-cost function for the door tag bit (see the module docs).
pub fn door_pass_cost(world: &World, agent: Entity, cell: CellCoord, _info: CellInfo) -> PassCost {
    let Some(door_e) = world
        .get_resource::<DoorCells>()
        .and_then(|c| c.door_at(cell))
    else {
        return PassCost::Default;
    };
    let Some(door) = world.get::<Door>(door_e) else {
        return PassCost::Default;
    };
    if door.open {
        return PassCost::Default;
    }
    if !may_pass(world, agent, door_e) {
        return PassCost::Blocked;
    }
    let cost = world
        .get_resource::<InteractionConfig>()
        .copied()
        .unwrap_or_default()
        .door_pass_cost;
    if door.is_locked() {
        PassCost::Marked(cost.saturating_mul(2))
    } else {
        PassCost::Marked(cost)
    }
}

/// The blocked-walk report's name for a door cell (`npc_nav`
/// [`CellBlockerFn`](npc_nav::CellBlockerFn)): the closed door there, with
/// whether it is locked; `None` for an open door or no door.
pub fn door_blocker(world: &World, cell: CellCoord) -> Option<Blocker> {
    let door_e = world.get_resource::<DoorCells>()?.door_at(cell)?;
    let door = world.get::<Door>(door_e)?;
    if door.open {
        return None;
    }
    Some(Blocker::Door {
        id: *world.get::<SimId>(door_e)?,
        locked: door.is_locked(),
    })
}

/// Idle path-following interactors ([`auto_door_system`]'s cached query:
/// these per-tick systems would otherwise build their `QueryState`s every
/// tick).
pub type DoorWalkerQuery = QueryState<
    (&'static SimId, Entity, &'static SimPos, &'static Path),
    (With<NavAgent>, With<Interactor>, Without<Interacting>),
>;

/// Every door ([`key_replan_system`]'s cached query).
pub type DoorQuery = QueryState<&'static Door>;

/// Interactors whose inventory or equipment changed since the system last
/// ran ([`key_replan_system`]'s cached query).
pub type KeyChangeQuery = QueryState<
    (&'static SimId, Entity, &'static npc_nav::NavState),
    (
        With<NavAgent>,
        With<Interactor>,
        Or<(Changed<Inventory>, Changed<Equipment>)>,
    ),
>;

/// `NpcInteractionSet::Begin`: every idle `NavAgent` + [`Interactor`] (in
/// `SimId` order) whose next waypoint is a door marker, standing next to
/// that closed door and within reach, starts `unlock` (locked, with the
/// key) or `open` on it. Off with `InteractionConfig::auto_open_doors`.
pub fn auto_door_system(world: &mut World, walkers: &mut DoorWalkerQuery) {
    let config = world
        .get_resource::<InteractionConfig>()
        .copied()
        .unwrap_or_default();
    if !config.auto_open_doors {
        return;
    }
    let mut agents: Vec<(SimId, Entity, SimPos, Path)> = walkers
        .iter(world)
        .filter(|(_, _, _, p)| !p.markers.is_empty())
        .map(|(&id, e, &pos, p)| (id, e, pos, p.clone()))
        .collect();
    agents.sort_unstable_by_key(|a| a.0);
    for (_, agent, pos, path) in agents {
        let Some(marker) = path.marker_at(path.next) else {
            continue;
        };
        if marker.tag != config.door_tag_bit {
            continue;
        }
        let Some(grid) = world.get_resource::<SpatialGrid>() else {
            return;
        };
        let here = grid.cell_of(pos);
        if (here.x - marker.cell.x).abs() > 1 || (here.z - marker.cell.z).abs() > 1 {
            continue;
        }
        let Some(door_e) = world
            .get_resource::<DoorCells>()
            .and_then(|c| c.door_at(marker.cell))
        else {
            continue;
        };
        let Some(door) = world.get::<Door>(door_e).cloned() else {
            continue;
        };
        if door.open {
            continue;
        }
        let Some(&door_id) = world.get::<SimId>(door_e) else {
            continue;
        };
        let verb = if door.is_locked() { "unlock" } else { "open" };
        if can_interact(world, agent, verb, door_id, None).is_ok() {
            let _ = begin(world, agent, verb, door_id, None, Source::Auto);
        }
    }
}

/// The change-detection state [`key_replan_system`] would see on the next
/// tick (world snapshots, ADR 0013): the sim entities whose `Inventory` /
/// `Equipment` changed after it last ran (every one, when it never ran).
/// A load marks them changed again after the settle step.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct KeyReplanPending {
    inventory: Vec<SimId>,
    equipment: Vec<SimId>,
}

/// [`key_replan_system`]'s last run in `NpcSimTick` (`None`: it never ran,
/// or the schedule is not built yet).
fn key_replan_last_run(world: &World) -> Option<Tick> {
    let ty = IntoSystem::system_type_id(&key_replan_system);
    let schedule = world.get_resource::<Schedules>()?.get(NpcSimTick)?;
    let mut systems = schedule.systems().ok()?;
    systems
        .find(|(_, s)| s.system_type() == ty)
        .map(|(_, s)| s.get_last_run())
}

pub(crate) fn save_key_replan_pending(world: &World) -> Option<serde_json::Value> {
    let last_run = key_replan_last_run(world);
    let this_run = world.read_change_tick();
    let changed = |ticks: Option<ComponentTicks>| {
        ticks.is_some_and(|t| last_run.is_none_or(|last| t.is_changed(last, this_run)))
    };
    let mut pending = KeyReplanPending::default();
    let mut ids: Vec<(SimId, Entity)> = world
        .try_query::<(&SimId, Entity)>()?
        .iter(world)
        .map(|(&id, e)| (id, e))
        .collect();
    ids.sort_unstable_by_key(|r| r.0);
    for (id, e) in ids {
        let e = world.entity(e);
        if changed(e.get_change_ticks::<Inventory>()) {
            pending.inventory.push(id);
        }
        if changed(e.get_change_ticks::<Equipment>()) {
            pending.equipment.push(id);
        }
    }
    Some(serde_json::to_value(pending).expect("key replan state serializes to JSON"))
}

pub(crate) fn load_key_replan_pending(
    world: &mut World,
    value: serde_json::Value,
) -> Result<(), String> {
    let pending: KeyReplanPending = serde_json::from_value(value).map_err(|e| e.to_string())?;
    world.insert_resource(pending);
    Ok(())
}

/// Snapshot rebuild (after the settle step): marks the saved `Inventory` /
/// `Equipment` changes changed again, so [`key_replan_system`] sees them on
/// the first tick after the load, as it would have without the save.
pub(crate) fn rebuild_key_replan_pending(world: &mut World) {
    let Some(pending) = world.remove_resource::<KeyReplanPending>() else {
        return;
    };
    let entity = |world: &World, id: SimId| world.get_resource::<SimIndex>()?.get(id);
    for id in pending.inventory {
        if let Some(mut inv) = entity(world, id).and_then(|e| world.get_mut::<Inventory>(e)) {
            inv.set_changed();
        }
    }
    for id in pending.equipment {
        if let Some(mut eq) = entity(world, id).and_then(|e| world.get_mut::<Equipment>(e)) {
            eq.set_changed();
        }
    }
}

/// `NpcInteractionSet::Complete` (after the completions): a `NavAgent` +
/// [`Interactor`] whose path request failed and whose inventory or
/// equipment changed this tick (or since this system last ran) retries the
/// path ([`npc_nav::retry_failed_path`]) when it now carries the key of a
/// locked door. Locked doors without the key are blocked for that agent
/// only (a pass-cost decision, not cell data), so picking the key up is not a
/// `GridChanged`; without this the agent would wait for an unrelated grid
/// change. Agents in `SimId` order.
pub fn key_replan_system(world: &mut World, doors: &mut DoorQuery, changed: &mut KeyChangeQuery) {
    let mut keys: Vec<String> = doors
        .iter(world)
        .filter_map(|d| d.locked_by.clone())
        .collect();
    if keys.is_empty() {
        return;
    }
    keys.sort_unstable();
    keys.dedup();
    let mut agents: Vec<(SimId, Entity)> = changed
        .iter(world)
        .filter(|(_, _, s)| matches!(s, npc_nav::NavState::Failed { .. }))
        .map(|(&id, e, _)| (id, e))
        .collect();
    agents.sort_unstable_by_key(|a| a.0);
    for (_, agent) in agents {
        if keys
            .iter()
            .any(|k| carried_matching(world, agent, k).is_some())
        {
            npc_nav::retry_failed_path(world, agent);
        }
    }
}
