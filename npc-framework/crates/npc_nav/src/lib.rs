//! `npc_nav`: opt-in grid pathfinding (actor-capabilities spec §4.1, plan
//! Task C1).
//!
//! Add [`NavPlugin`] (also known as the [`GridPathMover`]) next to
//! `NpcPlugins` and put [`NavAgent`] on the units that should path-find;
//! every other unit keeps the `StraightLineMover`. Actions do not change:
//! they write a `MoveIntent` and wait on `MoveStatus` (`npc_locomotion`).
//!
//! ## What happens in a tick (`NpcSet::Locomotion`, inside `NpcMoverSet`)
//!
//! 1. [`plan::nav_plan_system`] (exclusive), over every `NavAgent` in `SimId`
//!    order:
//!    - a new or changed intent goal enqueues a [`PathRequest`];
//!    - a `GridChanged` cell on the rest of the current path, or
//!      `MoveStatus::Blocked { ticks }` reaching a multiple of
//!      [`REPLAN_BLOCKED_TICKS`], enqueues a replan (the old path is dropped);
//!    - after a failure, any `GridChanged` retries, and so does
//!      [`retry_failed_path`] for one agent (what the grid does not report,
//!      e.g. `npc_interaction` when a unit gets the key of a locked door).
//!
//!    Then it serves the queue in `(enqueue tick, SimId)` order (so
//!    simultaneous requests go in `SimId` order) under the per-tick node
//!    budget [`NavBudget`] (default 4000 expansions). The request at the head
//!    may pause mid-search and resumes next tick (carry-over); the ones
//!    behind it wait. A `GridChanged` restarts a paused search only when a
//!    changed cell is one the search looked at ([`astar::Search::touches`]);
//!    a change elsewhere cannot change its result. The work one request may
//!    take over all its restarts is capped at [`SEARCH_LIMIT_BUDGETS`] ×
//!    the budget (at least the grid's cell count, which no single search can
//!    exceed): past it the request fails with `PathFailReason::SearchLimit`,
//!    so constant grid churn cannot keep the queue's head busy forever.
//!    Results: a [`Path`] component and journal `PathFound { len, nodes }`,
//!    or `PathFailed { reason }`.
//! 2. [`mover::grid_path_mover_system`]: follows the waypoints at the
//!    intent's speed (several waypoints per tick if the step allows), never
//!    entering a blocking cell (it reports `Blocked` instead), and reports
//!    `Moving` / `Arrived` / `Blocked`. Waiting for a path is `Moving`; a
//!    failed request is `Blocked`, so actions can give up.
//!
//! ## Search
//!
//! [`astar`]: integer A*, costs 10/14 plus the cell's extra cost, no corner
//! cutting, ties broken by `(f, h, cell index)`. Then line-of-sight
//! string-pulling over cells that are open with no extra cost
//! ([`plan::smooth`]): in an open field the path is the straight line (one
//! waypoint, the goal).
//!
//! An agent with its own `GridBlocker` (e.g. a `Direct` vehicle) blocks the
//! cells it stands on; to its own search, smoothing and mover those cells
//! are what they would be without it ([`SpatialGrid::cell_without_one_blocker`]
//! over its footprint at its indexed position), and changes of its own
//! cells do not make it replan.
//!
//! ## Doors and other interactions (hook for wave E)
//!
//! Cells carry content tag bits (`CellFlags::tags`). [`NavAppExt::register_pass_cost`]
//! registers a [`PassCostFn`] per tag bit; it decides per agent whether the
//! cell is blocked, costs extra, or costs extra **and needs an interaction**
//! ([`PassCost::Marked`]). A marked cell becomes a waypoint with a
//! [`PathMarker`] (e.g. "open the door here"); the mover waits in front of it
//! while it is blocking (no replan while waiting at a marker).
//!
//! ## Why a walk is blocked
//!
//! [`explain_blocked`] (read-only tooling, its own arrays and no budget):
//! the reachable cell closest to a goal and what is in the way, a wall, a
//! door ([`NavAppExt::register_cell_blocker`] names it) or the world's
//! edge. `npc_command` puts it in `OrderFailed { reason: "blocked" }`.
//!
//! ## Determinism and hashing
//!
//! Integer-only; agents in `SimId` order; requests in a `BTreeMap`; the heap
//! has a total order. The plugin registers its state for the state hash
//! (`nav_agent`, `nav_state`, `path`, `nav_queue`: the requests and the
//! paused search's whole progress) plus the locomotion
//! (`move_intent`, `move_status`) and spatial (`spatial_grid`,
//! `grid_blocker`, `grid_occluder`) state it depends on. That changes the
//! content hash of any app that adds it, which is why it is opt-in.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

pub mod astar;
pub mod explain;
pub mod mover;
pub mod plan;

use std::collections::BTreeMap;

use bevy_app::prelude::*;
use bevy_ecs::message::{MessageCursor, Messages};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_kernel::hash::{Fnv64, SimHash, SimHashResource};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::snapshot::ResourceSnapshotFns;
use npc_kernel::spatial::{
    Blocker, CellCoord, CellInfo, GridChanged, PathFailReason, SpatialGrid, register_spatial_hash,
};
use npc_kernel::{NpcSimTick, SimId, SimPos};
use npc_locomotion::{
    ExternalMover, NpcLocomotionPlugin, NpcMoverSet, register_locomotion_hash,
    straight_line_mover_system,
};
use serde::{Deserialize, Serialize};

pub use explain::{BlockedReport, EXPLAIN_CELL_BUDGET, explain_blocked, explain_blocked_within};
pub use plan::{NavStats, nav_plan_system, retry_failed_path};

/// Default [`NavBudget`]: A* expansions per tick, over all requests.
pub const DEFAULT_NAV_BUDGET: u32 = 4000;
/// A path is replanned when `MoveStatus::Blocked { ticks }` reaches a
/// multiple of this.
pub const REPLAN_BLOCKED_TICKS: u32 = 10;
/// One request may expand at most this many [`NavBudget`]s of cells over
/// all its restarts (but at least the grid's cell count) before it fails
/// with `PathFailReason::SearchLimit`.
pub const SEARCH_LIMIT_BUDGETS: u32 = 8;

/// Opt-in per unit: this agent's `MoveIntent`s are path-found on the grid
/// (and not walked by the `StraightLineMover`).
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[require(ExternalMover, NavState)]
#[reflect(Component)]
pub struct NavAgent;

/// Where the agent's navigation stands. Managed by the plugin.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub enum NavState {
    /// No intent.
    #[default]
    Idle,
    /// A request for `goal` is queued since `tick`.
    Pending { goal: SimPos, tick: u64 },
    /// Following a [`Path`] to `goal`.
    Following { goal: SimPos },
    /// The last request for `goal` failed; retried on `GridChanged`.
    Failed {
        goal: SimPos,
        reason: PathFailReason,
    },
}

impl NavState {
    /// The goal the current request/path/failure is for.
    pub fn goal(&self) -> Option<SimPos> {
        match *self {
            NavState::Idle => None,
            NavState::Pending { goal, .. }
            | NavState::Following { goal }
            | NavState::Failed { goal, .. } => Some(goal),
        }
    }
}

/// A waypoint that needs an interaction (e.g. a door to open).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct PathMarker {
    /// Index into [`Path::waypoints`] (the marked cell's centre).
    pub waypoint: u32,
    pub cell: CellCoord,
    /// The cell tag bit whose pass-cost function marked it.
    pub tag: u8,
}

/// A smoothed path. `waypoints` excludes the start and ends at the goal;
/// `next` is the waypoint being walked to.
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Path {
    pub waypoints: Vec<SimPos>,
    pub markers: Vec<PathMarker>,
    pub next: u32,
}

impl Path {
    /// The remaining waypoints.
    pub fn remaining(&self) -> &[SimPos] {
        self.waypoints.get(self.next as usize..).unwrap_or(&[])
    }

    /// The marker on waypoint `index`, if any.
    pub fn marker_at(&self, index: u32) -> Option<PathMarker> {
        self.markers.iter().copied().find(|m| m.waypoint == index)
    }
}

/// Per-tick A* expansion budget over all requests (default 4000).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavBudget(pub u32);

impl Default for NavBudget {
    fn default() -> Self {
        Self(DEFAULT_NAV_BUDGET)
    }
}

/// One queued request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PathRequest {
    pub agent: SimId,
    pub goal: SimPos,
    /// Tick it was enqueued (the first part of the serving order).
    pub tick: u64,
}

/// The request queue, served in `(tick, SimId)` order. Hashed as
/// `nav_queue` (requests plus the paused search's whole progress).
#[derive(Resource, Default)]
pub struct NavQueue {
    pub requests: BTreeMap<(u64, SimId), PathRequest>,
    /// The paused search of the head request.
    pub(crate) active: Option<ActiveSearch>,
    pub(crate) scratch: astar::Scratch,
    pub(crate) cursor: MessageCursor<GridChanged>,
}

/// The head request's search and the work it took so far.
pub(crate) struct ActiveSearch {
    pub key: (u64, SimId),
    pub search: astar::Search,
    /// Expansions over every restart of this request (capped, see
    /// [`SEARCH_LIMIT_BUDGETS`]).
    pub spent: u32,
}

impl NavQueue {
    /// Expansions done so far by the paused head search since it last
    /// (re)started (0 if none).
    pub fn paused_nodes(&self) -> u32 {
        self.active.as_ref().map_or(0, |a| a.search.nodes)
    }

    /// Expansions the paused head request took over all its restarts.
    pub fn paused_spent(&self) -> u32 {
        self.active.as_ref().map_or(0, |a| a.spent)
    }
}

/// What a [`PassCostFn`] decides for one agent and cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassCost {
    /// Not this function's business: the cell's flags decide.
    Default,
    Blocked,
    /// Passable at this extra cost.
    Cost(u16),
    /// Passable at this extra cost through an interaction: the cell becomes a
    /// marked waypoint ([`PathMarker`]).
    Marked(u16),
}

/// Pass-cost function for a cell tag bit: `(world, agent, cell, cell data)`.
pub type PassCostFn = fn(&World, Entity, CellCoord, CellInfo) -> PassCost;

/// Registered pass-cost functions, by tag bit (0..8). The lowest set bit
/// with a function decides.
#[derive(Resource, Clone, Default)]
pub struct PassCosts {
    pub by_tag: BTreeMap<u8, PassCostFn>,
}

/// Names what blocks a cell with a content tag bit, for the blocked-walk
/// report ([`explain_blocked`]): `(world, cell)` → e.g. [`Blocker::Door`]
/// for a closed door there, `None` to leave it a wall.
pub type CellBlockerFn = fn(&World, CellCoord) -> Option<Blocker>;

/// Registered [`CellBlockerFn`]s, by tag bit (0..8); the lowest set bit
/// with an answer decides. Tooling only (not hashed, never read by the sim).
#[derive(Resource, Clone, Default)]
pub struct CellBlockers {
    pub by_tag: BTreeMap<u8, CellBlockerFn>,
}

/// Registration for the nav hooks.
pub trait NavAppExt {
    /// Registers `f` for cells with tag bit `tag` (0..8) set.
    fn register_pass_cost(&mut self, tag: u8, f: PassCostFn) -> &mut Self;

    /// Registers `f` to name the blocker of cells with tag bit `tag` (0..8)
    /// in blocked-walk reports ([`explain_blocked`]).
    fn register_cell_blocker(&mut self, tag: u8, f: CellBlockerFn) -> &mut Self;
}

impl NavAppExt for App {
    fn register_pass_cost(&mut self, tag: u8, f: PassCostFn) -> &mut Self {
        assert!(tag < 8, "cell tag bits are 0..8");
        self.world_mut()
            .get_resource_or_init::<PassCosts>()
            .by_tag
            .insert(tag, f);
        self
    }

    fn register_cell_blocker(&mut self, tag: u8, f: CellBlockerFn) -> &mut Self {
        assert!(tag < 8, "cell tag bits are 0..8");
        self.world_mut()
            .get_resource_or_init::<CellBlockers>()
            .by_tag
            .insert(tag, f);
        self
    }
}

/// The grid path mover. Requires the kernel's `SpatialPlugin` (in
/// `NpcPlugins`); adds the locomotion seam, [`NavBudget`], [`NavQueue`],
/// [`PassCosts`], the hash registrations (see the crate docs) and the two
/// systems in `NpcMoverSet` (after the `StraightLineMover`, which skips
/// `NavAgent`s).
pub struct NavPlugin;

/// [`NavPlugin`] under the plan's name.
pub type GridPathMover = NavPlugin;

impl Plugin for NavPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(NpcLocomotionPlugin)
            .init_resource::<SpatialGrid>()
            .init_resource::<Messages<GridChanged>>()
            .init_resource::<NavBudget>()
            .init_resource::<NavQueue>()
            .init_resource::<NavStats>()
            .init_resource::<PassCosts>();
        register_spatial_hash(app);
        register_locomotion_hash(app);
        app.register_sim_component::<NavAgent>()
            .register_sim_component::<NavState>()
            .register_sim_component::<Path>()
            .register_sim_resource::<NavQueue>()
            .add_systems(
                NpcSimTick,
                (nav_plan_system, mover::grid_path_mover_system)
                    .chain()
                    .in_set(NpcMoverSet)
                    .after(straight_line_mover_system),
            );
    }
}

// ---------------------------------------------------------------------------
// Hash encodings
// ---------------------------------------------------------------------------

fn hash_reason(h: &mut Fnv64, r: PathFailReason) {
    h.u8(match r {
        PathFailReason::StartBlocked => 0,
        PathFailReason::GoalBlocked => 1,
        PathFailReason::OutOfBounds => 2,
        PathFailReason::Unreachable => 3,
        PathFailReason::SearchLimit => 4,
    });
}

impl SimHash for NavAgent {
    const NAME: &'static str = "nav_agent";
    fn sim_hash(&self, _h: &mut Fnv64) {}
}

impl SimHash for NavState {
    const NAME: &'static str = "nav_state";
    fn sim_hash(&self, h: &mut Fnv64) {
        match *self {
            NavState::Idle => h.u8(0),
            NavState::Pending { goal, tick } => {
                h.u8(1);
                goal.sim_hash(h);
                h.u64(tick);
            }
            NavState::Following { goal } => {
                h.u8(2);
                goal.sim_hash(h);
            }
            NavState::Failed { goal, reason } => {
                h.u8(3);
                goal.sim_hash(h);
                hash_reason(h, reason);
            }
        }
    }
}

impl SimHash for Path {
    const NAME: &'static str = "path";
    fn sim_hash(&self, h: &mut Fnv64) {
        let len = |n: usize| u32::try_from(n).expect("path too long");
        h.u32(len(self.waypoints.len()));
        for w in &self.waypoints {
            w.sim_hash(h);
        }
        h.u32(len(self.markers.len()));
        for m in &self.markers {
            h.u32(m.waypoint);
            h.i32(m.cell.x);
            h.i32(m.cell.z);
            h.u8(m.tag);
        }
        h.u32(self.next);
    }
}

/// The saved form of [`NavQueue`] (world snapshots, ADR 0013): the requests,
/// the paused head search with its scratch stamps, and the scratch's shape.
/// The `GridChanged` cursor is not saved: after a load it starts at the
/// current end of the (empty) message buffer.
#[derive(Serialize, Deserialize)]
struct SavedNavQueue {
    requests: Vec<PathRequest>,
    active: Option<SavedActive>,
    scratch_len: usize,
    scratch_generation: u32,
}

#[derive(Serialize, Deserialize)]
struct SavedActive {
    tick: u64,
    agent: SimId,
    spent: u32,
    search: astar::SavedSearch,
}

fn save_nav_queue(world: &World) -> Option<serde_json::Value> {
    let queue = world.get_resource::<NavQueue>()?;
    let (scratch_len, scratch_generation) = queue.scratch.shape();
    let saved = SavedNavQueue {
        requests: queue.requests.values().copied().collect(),
        active: queue.active.as_ref().map(|a| SavedActive {
            tick: a.key.0,
            agent: a.key.1,
            spent: a.spent,
            search: a.search.save(&queue.scratch),
        }),
        scratch_len,
        scratch_generation,
    };
    Some(serde_json::to_value(saved).expect("the nav queue serializes to JSON"))
}

fn load_nav_queue(world: &mut World, value: serde_json::Value) -> Result<(), String> {
    let saved: SavedNavQueue = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let mut scratch = astar::Scratch::restore(saved.scratch_len, saved.scratch_generation);
    let active = match saved.active {
        Some(a) => Some(ActiveSearch {
            key: (a.tick, a.agent),
            search: astar::Search::load(a.search, &mut scratch)?,
            spent: a.spent,
        }),
        None => None,
    };
    let cursor = world
        .get_resource::<Messages<GridChanged>>()
        .map(Messages::get_cursor_current)
        .unwrap_or_default();
    world.insert_resource(NavQueue {
        requests: saved
            .requests
            .into_iter()
            .map(|r| ((r.tick, r.agent), r))
            .collect(),
        active,
        scratch,
        cursor,
    });
    Ok(())
}

impl SimHashResource for NavQueue {
    const NAME: &'static str = "nav_queue";
    fn snapshot_fns() -> Option<ResourceSnapshotFns> {
        Some(ResourceSnapshotFns {
            save: save_nav_queue,
            load: load_nav_queue,
        })
    }
    /// The requests, then the paused search's expansion count; with a
    /// paused search also its key, its work over all restarts and its whole
    /// progress ([`astar::Search::sim_hash`]). Without one the encoding is
    /// the requests plus a 0 count, as before the progress was hashed.
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(u32::try_from(self.requests.len()).expect("too many requests"));
        for r in self.requests.values() {
            h.u64(r.tick);
            h.u32(r.agent.0);
            r.goal.sim_hash(h);
        }
        h.u32(self.paused_nodes());
        if let Some(a) = &self.active {
            h.u64(a.key.0);
            h.u32(a.key.1.0);
            h.u32(a.spent);
            a.search.sim_hash(&self.scratch, h);
        }
    }
}
