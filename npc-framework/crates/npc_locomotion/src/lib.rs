//! The locomotion seam (ADR 0008, actor-capabilities spec §4.1, plan Task C1).
//!
//! Actions never move `SimPos` themselves. They write a [`MoveIntent`]
//! (where to, how fast, how close is close enough) and read [`MoveStatus`]
//! (`Idle`, `Moving`, `Arrived`, `Blocked { ticks }`). A mover turns intents
//! into movement:
//! - [`StraightLineMover`] (this crate, in `NpcPlugins` by default): walks
//!   straight at `speed_mm_per_tick` using integer `SimPos::step_towards`, and
//!   only refuses to step into a blocking cell of the kernel's `SpatialGrid`
//!   (it reports `Blocked` instead). No blocking cells: no grid checks.
//! - `npc_nav::GridPathMover`: grid A* plus path following, per agent
//!   (`NavAgent`).
//! - Host adapters (physics, navmesh): put [`ExternalMover`] on the agents
//!   they own and run in [`NpcMoverSet`].
//!
//! ## Schedule placement
//!
//! `NpcSet::Actions → NpcSet::Locomotion → NpcSet::Effects`. Inside
//! `NpcSet::Locomotion` every mover runs in [`NpcMoverSet`]; systems that
//! react to `MoveStatus` in the same tick (an action noticing its arrival,
//! so it can succeed in the tick it arrived, exactly as when actions moved
//! units themselves) run `.after(NpcMoverSet)` in `NpcSet::Locomotion`. A
//! system that removes an intent with `Commands` in `NpcSet::Actions` takes
//! effect before the movers run (sync point between the chained sets).
//!
//! ## Hashing
//!
//! `MoveIntent` and `MoveStatus` are sim state but are **not hashed unless a
//! game asks** ([`register_locomotion_hash`]): registering a name changes
//! the content hash and so invalidates recorded replays, and the village's
//! own `move_to` component already carries the goal (its speed is a constant
//! of the unit and it never blocks). A game whose decisions depend on
//! `MoveStatus` (it has blockers, or its intents carry information no hashed
//! component has) should call it; `npc_nav` does.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::SystemSet;
use bevy_reflect::Reflect;
use npc_kernel::hash::{Fnv64, SimHash, SimHashRegistry};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::snapshot::SnapshotAppExt;
use npc_kernel::spatial::SpatialGrid;
use npc_kernel::{NpcSet, NpcSimTick, SimPos};
use serde::{Deserialize, Serialize};

/// Where an agent wants to go. Written by actions (or orders), read by
/// movers. `tolerance_mm` 0 means "exactly at `goal`".
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[require(MoveStatus)]
#[reflect(Component)]
pub struct MoveIntent {
    pub goal: SimPos,
    pub speed_mm_per_tick: i32,
    pub tolerance_mm: i32,
}

impl MoveIntent {
    /// An intent to stand exactly on `goal`.
    pub const fn to(goal: SimPos, speed_mm_per_tick: i32) -> Self {
        Self {
            goal,
            speed_mm_per_tick,
            tolerance_mm: 0,
        }
    }

    /// Whether `pos` is close enough to the goal (`dist ≤ tolerance`).
    pub fn reached(&self, pos: SimPos) -> bool {
        let tol = i64::from(self.tolerance_mm.max(0));
        pos.dist2(self.goal) <= tol * tol
    }
}

/// What the mover made of the intent, updated every tick a mover runs.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub enum MoveStatus {
    /// No intent (or not yet seen by a mover).
    #[default]
    Idle,
    /// On the way (or waiting for a path).
    Moving,
    /// Within `tolerance_mm` of the goal.
    Arrived,
    /// Could not move for `ticks` consecutive ticks (a blocking cell ahead,
    /// or no path).
    Blocked { ticks: u32 },
}

impl MoveStatus {
    /// The next `Blocked` status: one more tick than now.
    pub fn blocked_again(self) -> Self {
        match self {
            MoveStatus::Blocked { ticks } => MoveStatus::Blocked {
                ticks: ticks.saturating_add(1),
            },
            _ => MoveStatus::Blocked { ticks: 1 },
        }
    }
}

/// Marker: a mover other than [`StraightLineMover`] owns this agent's
/// intents (`npc_nav::NavAgent` requires it; host adapters insert it).
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
#[reflect(Component)]
pub struct ExternalMover;

/// Every mover runs in this set, inside `NpcSet::Locomotion`.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NpcMoverSet;

/// Starts (or retargets) a move: inserts `intent` and resets the status to
/// `Moving`.
pub fn set_move_intent(world: &mut World, agent: Entity, intent: MoveIntent) {
    if let Ok(mut e) = world.get_entity_mut(agent) {
        e.insert((intent, MoveStatus::Moving));
    }
}

/// Ends a move: removes the intent and its status.
pub fn clear_move_intent(world: &mut World, agent: Entity) {
    if let Ok(mut e) = world.get_entity_mut(agent) {
        e.remove::<(MoveIntent, MoveStatus)>();
    }
}

impl SimHash for MoveIntent {
    const NAME: &'static str = "move_intent";
    fn sim_hash(&self, h: &mut Fnv64) {
        self.goal.sim_hash(h);
        h.i32(self.speed_mm_per_tick);
        h.i32(self.tolerance_mm);
    }
}

impl SimHash for MoveStatus {
    const NAME: &'static str = "move_status";
    fn sim_hash(&self, h: &mut Fnv64) {
        match *self {
            MoveStatus::Idle => h.u8(0),
            MoveStatus::Moving => h.u8(1),
            MoveStatus::Arrived => h.u8(2),
            MoveStatus::Blocked { ticks } => {
                h.u8(3);
                h.u32(ticks);
            }
        }
    }
}

/// Adds `move_intent` and `move_status` to the state hash (idempotent). This
/// changes the content hash; see the crate docs for when to call it.
pub fn register_locomotion_hash(app: &mut App) {
    let registry = app.world_mut().get_resource_or_init::<SimHashRegistry>();
    let has = |name: &str| registry.components.iter().any(|c| c.name == name);
    let (intent, status) = (has(MoveIntent::NAME), has(MoveStatus::NAME));
    if !intent {
        app.register_sim_component::<MoveIntent>();
    }
    if !status {
        app.register_sim_component::<MoveStatus>();
    }
}

/// The seam: [`NpcMoverSet`] inside `NpcSet::Locomotion` and the reflected
/// types. Hashes nothing. Added by every mover plugin (idempotent).
pub struct NpcLocomotionPlugin;

impl Plugin for NpcLocomotionPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(NpcSimTick, NpcMoverSet.in_set(NpcSet::Locomotion))
            .register_type::<MoveIntent>()
            .register_type::<MoveStatus>()
            .register_type::<ExternalMover>()
            // Saved in world snapshots whether or not they are hashed
            // (`register_locomotion_hash`); re-registration is a no-op.
            .register_snapshot_component_named::<MoveIntent>(MoveIntent::NAME)
            .register_snapshot_component_named::<MoveStatus>(MoveStatus::NAME)
            .register_snapshot_component_named::<ExternalMover>("external_mover");
    }

    fn is_unique(&self) -> bool {
        false
    }
}

/// The default mover: [`straight_line_mover_system`] for every agent with a
/// [`MoveIntent`] and no [`ExternalMover`].
pub struct StraightLineMover;

impl Plugin for StraightLineMover {
    fn build(&self, app: &mut App) {
        app.add_plugins(NpcLocomotionPlugin)
            .add_systems(NpcSimTick, straight_line_mover_system.in_set(NpcMoverSet));
    }
}

/// One straight-line step for `pos` under `intent` (pure; the mover system
/// and tests use it). Returns the new position and status.
///
/// - Already within tolerance: no move, `Arrived`.
/// - Otherwise `pos.step_towards(goal, speed)`; `Arrived` when the new
///   position is within tolerance, else `Moving`.
/// - With a grid that has blocking cells: a step that would enter a blocking
///   cell (other than the one the agent stands in) is not taken, and the
///   status becomes `Blocked { ticks + 1 }`.
pub fn straight_step(
    pos: SimPos,
    intent: &MoveIntent,
    status: MoveStatus,
    grid: Option<&SpatialGrid>,
) -> (SimPos, MoveStatus) {
    if intent.reached(pos) {
        return (pos, MoveStatus::Arrived);
    }
    let next = pos.step_towards(intent.goal, intent.speed_mm_per_tick);
    if let Some(grid) = grid.filter(|g| g.has_blocking())
        && next != pos
    {
        let here = grid.cell_of(pos);
        let clear = grid.trace_segment(pos, next, |c| c == here || !grid.is_blocking(c));
        if !clear {
            return (pos, status.blocked_again());
        }
    }
    let status = if intent.reached(next) {
        MoveStatus::Arrived
    } else {
        MoveStatus::Moving
    };
    (next, status)
}

/// `NpcMoverSet`: [`straight_step`] for every intent without an
/// [`ExternalMover`]. Entities are independent (the grid is read-only here),
/// so query order does not matter. Positions and statuses are only written
/// when they change.
pub fn straight_line_mover_system(
    grid: Option<Res<SpatialGrid>>,
    mut movers: Query<(&mut SimPos, &MoveIntent, &mut MoveStatus), Without<ExternalMover>>,
) {
    let grid = grid.as_deref();
    for (mut pos, intent, mut status) in &mut movers {
        let (next, new_status) = straight_step(*pos, intent, *status, grid);
        if next != *pos {
            *pos = next;
        }
        status.set_if_neq(new_status);
    }
}
