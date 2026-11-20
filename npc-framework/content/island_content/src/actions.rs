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
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

pub const ISLAND_IDLE: &str = "IslandIdle";

/// The `TbIdle` action: ticks left.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct IslandIdling {
    pub remaining: u32,
}

impl SimHash for IslandIdling {
    const NAME: &'static str = "island_idling";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.remaining);
    }
}

// ---------------------------------------------------------------- TbIdle

pub fn idle_start(world: &mut World, agent: Entity, param: &ParamValue) -> Status {
    let ticks = param.as_int().unwrap_or(40).max(1);
    world.entity_mut(agent).insert(IslandIdling {
        remaining: u32::try_from(ticks).unwrap_or(1),
    });
    Status::Running
}

pub fn idle_stop(world: &mut World, agent: Entity, _reason: StopReason) {
    if let Ok(mut e) = world.get_entity_mut(agent) {
        e.remove::<IslandIdling>();
    }
}

/// `NpcSet::Actions`: counts idle ticks down.
pub fn idle_system(mut q: Query<(&mut IslandIdling, &mut ActiveLeaf)>) {
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

// ---------------------------------------------------------------- TbPickWander

pub fn instant_stop(_: &mut World, _: Entity, _: StopReason) {}

// ---------------------------------------------------------------- tool work
