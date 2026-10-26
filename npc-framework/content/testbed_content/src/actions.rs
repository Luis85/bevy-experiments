//! The testbed's own actions and conditions (everything else the trees use
//! is a capability's: `ExecuteOrder`, `DriveVehicle`, `Interact`, ..).
//!
//! - `TbIdle` (param `Int` ticks, default 40): stand still, then `Success`.
//! - `TbWalk` (param `Int` tolerance mm, default 0): walk to the `Pos` in
//!   [`BB_DEST`] through the locomotion seam (actors are `NavAgent`s, so they
//!   path-find and open doors they may open); `Success` on arrival, `Failure`
//!   after [`WALK_GIVE_UP_TICKS`] blocked ticks or when the intent was dropped
//!   (the actor mounted a vehicle).
//! - `TbPickWander` (instant): a random free cell within the unit's wander
//!   radius of home into [`BB_DEST`] (`sim_range`, purpose [`WANDER_PURPOSE`]).
//! - `TbFindToolWork` (instant): the nearest free, not depleted `ToolTarget`
//!   within [`TOOL_WORK_RADIUS_MM`] whose tool the actor has **equipped**:
//!   its `SimId` into the interaction target slot, its position into
//!   [`BB_DEST`]. The condition `tb_has_tool_work` is the same search, so the
//!   guard implies the leaf can start.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::interaction::items::equipped_matching;
use npc_framework::jobs::free_for;
use npc_framework::kernel::rng::sim_range;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

use crate::world::TestbedUnit;

/// Blackboard slot of walk destinations (`BbValue::Pos`).
pub const BB_DEST: BbKey = BbKey(0);

pub const TB_IDLE: &str = "TbIdle";
pub const TB_WALK: &str = "TbWalk";
pub const TB_PICK_WANDER: &str = "TbPickWander";
pub const TB_FIND_TOOL_WORK: &str = "TbFindToolWork";
pub const COND_HAS_TOOL_WORK: &str = "tb_has_tool_work";

/// A walk fails after this many consecutive blocked ticks.
pub const WALK_GIVE_UP_TICKS: u32 = 60;
/// `sim_range` purpose of wander spots (`+ 2k`, `+ 2k + 1` per attempt).
pub const WANDER_PURPOSE: u32 = 0x7E57_0100;
/// Wander spot attempts per pick.
pub const WANDER_ATTEMPTS: u32 = 8;
/// How far an actor looks for tool work.
pub const TOOL_WORK_RADIUS_MM: i32 = 20_000;

/// The `TbIdle` action: ticks left.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct TbIdling {
    pub remaining: u32,
}

impl SimHash for TbIdling {
    const NAME: &'static str = "tb_idling";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.remaining);
    }
}

/// The `TbWalk` action: where to.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct TbWalking {
    pub goal: SimPos,
}

impl SimHash for TbWalking {
    const NAME: &'static str = "tb_walking";
    fn sim_hash(&self, h: &mut Fnv64) {
        self.goal.sim_hash(h);
    }
}

// ---------------------------------------------------------------- TbIdle

pub fn idle_start(world: &mut World, agent: Entity, param: &ParamValue) -> Status {
    let ticks = param.as_int().unwrap_or(40).max(1);
    world.entity_mut(agent).insert(TbIdling {
        remaining: u32::try_from(ticks).unwrap_or(1),
    });
    Status::Running
}

pub fn idle_stop(world: &mut World, agent: Entity, _reason: StopReason) {
    if let Ok(mut e) = world.get_entity_mut(agent) {
        e.remove::<TbIdling>();
    }
}

/// `NpcSet::Actions`: counts idle ticks down.
pub fn idle_system(mut q: Query<(&mut TbIdling, &mut ActiveLeaf)>) {
    for (mut idle, mut leaf) in &mut q {
        if leaf.status != Status::Running {
            continue;
        }
        idle.remaining = idle.remaining.saturating_sub(1);
        if idle.remaining == 0 {
            leaf.status = Status::Success;
        }
    }
}

// ---------------------------------------------------------------- TbWalk

fn speed_of(world: &World, agent: Entity) -> i32 {
    world
        .get::<TestbedUnit>(agent)
        .map_or(60, |u| u.speed_mm_per_tick.max(1))
}

pub fn walk_start(world: &mut World, agent: Entity, param: &ParamValue) -> Status {
    let tolerance = param.as_int().unwrap_or(0).max(0);
    let dest = world.get::<Blackboard>(agent).map(|bb| bb.get(BB_DEST));
    let (Some(BbValue::Pos(x, z)), Some(&pos)) = (dest, world.get::<SimPos>(agent)) else {
        return Status::Failure;
    };
    let goal = SimPos::new(x, z);
    if pos.dist2(goal) <= i64::from(tolerance) * i64::from(tolerance) {
        return Status::Success;
    }
    let speed = speed_of(world, agent);
    world.entity_mut(agent).insert(TbWalking { goal });
    set_move_intent(
        world,
        agent,
        MoveIntent {
            goal,
            speed_mm_per_tick: speed,
            tolerance_mm: tolerance,
        },
    );
    Status::Running
}

pub fn walk_stop(world: &mut World, agent: Entity, _reason: StopReason) {
    let had = world
        .get_entity_mut(agent)
        .ok()
        .and_then(|mut e| e.take::<TbWalking>())
        .is_some();
    if had {
        clear_move_intent(world, agent);
    }
}

type Walkers = (With<TbWalking>, With<MoveIntent>);
type WalkRow<'a> = (Option<&'a MoveStatus>, Has<MoveIntent>, &'a mut ActiveLeaf);

/// `NpcSet::Actions`: a walk whose leaf no longer runs stops moving before
/// the movers run (its stop removes the rest).
pub fn walk_cancel_system(walkers: Query<(Entity, &ActiveLeaf), Walkers>, mut commands: Commands) {
    for (agent, leaf) in &walkers {
        if leaf.status != Status::Running {
            commands.entity(agent).remove::<(MoveIntent, MoveStatus)>();
        }
    }
}

/// `NpcSet::Locomotion`, after the movers: `Success` on arrival, `Failure`
/// when blocked too long or when the intent is gone (mounted).
pub fn walk_arrival_system(mut walkers: Query<WalkRow, With<TbWalking>>) {
    for (status, has_intent, mut leaf) in &mut walkers {
        if leaf.status != Status::Running {
            continue;
        }
        if !has_intent {
            leaf.status = Status::Failure;
            continue;
        }
        match status {
            Some(MoveStatus::Arrived) => leaf.status = Status::Success,
            Some(MoveStatus::Blocked { ticks }) if *ticks >= WALK_GIVE_UP_TICKS => {
                leaf.status = Status::Failure;
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- TbPickWander

/// A free spot within the unit's wander radius of home (see the module docs).
pub fn wander_spot(world: &World, agent: Entity) -> Option<SimPos> {
    let unit = world.get::<TestbedUnit>(agent)?;
    let &me = world.get::<SimId>(agent)?;
    let seed = world.resource::<WorldSeed>().0;
    let tick = world.resource::<SimClock>().tick;
    let grid = world.resource::<SpatialGrid>();
    let r = unit.wander_radius_mm.max(1);
    (0..WANDER_ATTEMPTS).find_map(|k| {
        let dx = sim_range(seed, me, tick, WANDER_PURPOSE + 2 * k, -r, r + 1);
        let dz = sim_range(seed, me, tick, WANDER_PURPOSE + 2 * k + 1, -r, r + 1);
        let cell = grid.cell_of(SimPos::new(unit.home.x + dx, unit.home.z + dz));
        (grid.bounds().contains(cell) && !grid.is_blocking(cell)).then(|| grid.center(cell))
    })
}

pub fn pick_wander_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let Some(spot) = wander_spot(world, agent) else {
        return Status::Failure;
    };
    if let Some(mut bb) = world.get_mut::<Blackboard>(agent) {
        bb.set(BB_DEST, BbValue::Pos(spot.x, spot.z));
    }
    Status::Success
}

pub fn instant_stop(_: &mut World, _: Entity, _: StopReason) {}

// ---------------------------------------------------------------- tool work

/// The nearest tool work for `agent` (see the module docs): `(target, pos)`.
pub fn find_tool_work(world: &World, agent: Entity) -> Option<(SimId, SimPos)> {
    let &me = world.get::<SimId>(agent)?;
    let &pos = world.get::<SimPos>(agent)?;
    let grid = world.get_resource::<SpatialGrid>()?;
    let index = world.get_resource::<SimIndex>()?;
    let reservations = world.get_resource::<Reservations>();
    grid.in_radius(pos, TOOL_WORK_RADIUS_MM)
        .into_iter()
        .filter_map(|id| {
            let e = index.get(id)?;
            let target = world.get::<ToolTarget>(e)?;
            if target.depleted() {
                return None;
            }
            if reservations.is_some_and(|r| !free_for(r, id, me)) {
                return None;
            }
            equipped_matching(world, agent, &target.required_tag)?;
            let at = *world.get::<SimPos>(e)?;
            Some((pos.dist2(at), id, at))
        })
        .min_by_key(|&(d2, id, _)| (d2, id))
        .map(|(_, id, at)| (id, at))
}

pub fn cond_has_tool_work(world: &World, agent: Entity) -> bool {
    find_tool_work(world, agent).is_some()
}

pub fn find_tool_work_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let Some((target, at)) = find_tool_work(world, agent) else {
        return Status::Failure;
    };
    let slot = world
        .get_resource::<InteractionConfig>()
        .copied()
        .unwrap_or_default()
        .target_slot;
    if let Some(mut bb) = world.get_mut::<Blackboard>(agent) {
        bb.set(BB_DEST, BbValue::Pos(at.x, at.z));
        bb.set(BbKey(slot), BbValue::U32(target.0));
    }
    Status::Success
}
