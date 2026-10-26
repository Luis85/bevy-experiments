//! Path requests and path following (spec §7, ADR-0015).
//!
//! A unit that needs a path gets a [`PathRequest`] at the back of the Hashed [`PathQueue`]
//! ([`enqueue_path`]); its [`PathFollow`] waits on the request's id. Every tick
//! [`process_path_queue`] pops requests in order while the work spent stays below
//! `Path.budget_per_tick`, processing at least one per tick (a partial plan may cost more than
//! the whole budget, R98). A request whose unit is gone, or no longer waits on it, is stale:
//! discarded at no charge. Any other request is planned at once with
//! [`plan_path`](crate::nav::plan_path) from the unit's current position, charged its cold
//! cost ([`PathWork::charge`], ADR-0015: a cache never changes the charge), and its result
//! applied in the same tick, so results apply in request order.
//!
//! **Group requests (flow fields).** A non-queued `core.move` gives each set of at least
//! `Path.flow_min_group` same-profile units one [`RequestKind::Group`] request
//! ([`enqueue_group_in`]). Processing it charges the cold cost of its flow field
//! ([`FlowCache::cold_cost`](crate::nav::FlowCache::cold_cost): the abstract nodes its
//! integration settles, whether or not the cache held it) and puts every member still waiting
//! on it, in `SimId` order, on the field (`PathState::Flow`, the member's own formation slot as
//! its goal); a member whose start cell has no route gets a single request at the back of the
//! queue instead. A group none of whose members exists and still waits on it is stale; one
//! whose profile has no HPA* graph gets no field, and its result is journaled `"single"`
//! (charge 0), each member asking alone.
//!
//! **When a request is processed** depends only on where it was made, never on system order
//! inside a hook (Task 8 review Minor 9). [`process_path_queue`] is the first core system of
//! [`ThinkSet::Behaviour`] (steering, `compute_steering`, is ordered after it); the hooks run
//! in order and each hook's core slot runs before its feature slot. So a request made in
//! `Input` (the commands), in [`ThinkSet::Orders`] (`advance_orders`, wandering, feature order
//! systems) or by a core system ordered `.before(process_path_queue)` is processed in its own
//! tick, subject to the budget; one made by a feature system in `ThinkSet::Behaviour`, or in
//! any later hook, waits for the next tick. Another core system in `ThinkSet::Behaviour`
//! conflicts with this exclusive one, and the schedule's error-level ambiguity check fails the
//! build until the two are ordered.
//!
//! [`move_system`](crate::movement) drives a `Following` unit through the plan's corners. A
//! terrain change ([`invalidate_paths`], from
//! [`on_terrain_changed`](crate::nav::on_terrain_changed)) sets every unit whose remaining
//! route crosses a rebuilt HPA* cluster back to `Idle`, and
//! [`advance_orders`](crate::orders) asks for a new path.
//!
//! Journal (spec §15): `"core.path_request"` per request, `"core.path_result"` per processed
//! request (phase `Think`). Counters: `"path.requests"` (processed), `"path.queue_len"` (left
//! after processing), `"path.nodes_expanded"`, `"path.los_cells"` and `"path.budget_spent"`
//! (every charge, group requests' included), and `"flowfield.abstract_nodes"` and
//! `"flowfield.cells"` for the flow-field builds a group request makes (observe-only: they
//! depend on what the cache held, R86). Debug query `"core.paths"`.

use std::collections::VecDeque;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use fl_content::MoveClass;
use fl_math::{Fx, FxVec2};
use fl_protocol::{SimId, Tick};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::build::Content;
use crate::builder::SimBuilder;
use crate::classify::{Class, Codec};
use crate::clock::SimClock;
use crate::counters::SimCounters;
use crate::hash::{SimHash, StateHasher};
use crate::ids::SimIndex;
use crate::nav::{
    ClusterId, FlowCache, FlowWork, HpaGraphs, MoveProfile, NavGrid, PathPlan, PathRules, PathWork,
    PlanStatus, class_name, plan_path, profile_of,
};
use crate::observe::{Journal, JournalRecord};
use crate::orders::{Reason, journal_order_changed};
use crate::schedule::{Phase, ThinkSet};
use crate::terrain::CellRect;
use crate::units::{Footprint, Mobility, MoveOrder, Order, Pose};

/// How a unit is following a path. Hashed `"core.path_follow"` v1, on every unit with
/// [`Mobility`].
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathFollow {
    pub state: PathState,
    /// The point the current path was requested to.
    pub goal: FxVec2,
    /// The plan's corners, in driving order; the last is where the path ends (the goal, the
    /// passable cell it snapped to, or the nearest reachable point).
    pub corners: Vec<FxVec2>,
    /// The index of the corner the unit drives to.
    pub next: u16,
    /// The tick the current plan was applied.
    pub planned: Tick,
}

/// The state of a [`PathFollow`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathState {
    /// No path, and none asked for.
    #[default]
    Idle,
    /// Request `request` is queued.
    Waiting { request: u32 },
    /// Driving through the corners.
    Following,
    /// Following flow field `key` toward `goal` ([`FlowCache`]).
    Flow { key: FlowKey },
    /// At the end of the path.
    Done,
    /// No path: the goal (or the start) has no passable cell nearby.
    Failed,
}

/// The key of a cached flow field: the goal's cluster and the movement profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FlowKey {
    pub cluster: ClusterId,
    pub profile: MoveProfile,
}

/// The path requests not yet processed, oldest first. Hashed `"core.path_queue"` v1.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathQueue {
    pub pending: VecDeque<PathRequest>,
    /// The id the next request gets.
    pub next_id: u32,
}

/// One unit's request for a path to `goal`, made in tick `issued`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathRequest {
    pub id: u32,
    pub unit: SimId,
    pub goal: FxVec2,
    pub kind: RequestKind,
    pub issued: Tick,
}

/// What a [`PathRequest`] plans for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestKind {
    /// One unit's path.
    Single,
    /// A group move's flow field toward the request's goal, for `units` (in `SimId` order),
    /// each with its own goal (its formation slot).
    Group { units: Vec<(SimId, FxVec2)> },
}

/// A tag byte, then the data.
impl SimHash for PathState {
    fn sim_hash(&self, h: &mut StateHasher) {
        match self {
            PathState::Idle => h.write_u8(0),
            PathState::Waiting { request } => {
                h.write_u8(1);
                request.sim_hash(h);
            }
            PathState::Following => h.write_u8(2),
            PathState::Flow { key } => {
                h.write_u8(3);
                key.sim_hash(h);
            }
            PathState::Done => h.write_u8(4),
            PathState::Failed => h.write_u8(5),
        }
    }
}

impl SimHash for FlowKey {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.cluster.0.sim_hash(h);
        self.profile.class.sim_hash(h);
        self.profile.size.sim_hash(h);
    }
}

impl SimHash for PathFollow {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.state.sim_hash(h);
        self.goal.sim_hash(h);
        self.corners.sim_hash(h);
        self.next.sim_hash(h);
        self.planned.sim_hash(h);
    }
}

/// A tag byte, then the data.
impl SimHash for RequestKind {
    fn sim_hash(&self, h: &mut StateHasher) {
        match self {
            RequestKind::Single => h.write_u8(0),
            RequestKind::Group { units } => {
                h.write_u8(1);
                h.write_u64(units.len() as u64);
                for (id, goal) in units {
                    id.sim_hash(h);
                    goal.sim_hash(h);
                }
            }
        }
    }
}

impl SimHash for PathRequest {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.id.sim_hash(h);
        self.unit.sim_hash(h);
        self.goal.sim_hash(h);
        self.kind.sim_hash(h);
        self.issued.sim_hash(h);
    }
}

/// The pending requests' count as a `u64`, each request, then the next id.
impl SimHash for PathQueue {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_u64(self.pending.len() as u64);
        for request in &self.pending {
            request.sim_hash(h);
        }
        self.next_id.sim_hash(h);
    }
}

/// Journal `"core.path_request"`: request `request` asks for a path to `goal` for a unit of
/// `profile` (`null` without a navigation grid); a group request also lists its `units`.
#[derive(Debug, Serialize)]
struct PathRequested {
    request: u32,
    goal: FxVec2,
    profile: Option<MoveProfile>,
    /// `"single"` or `"group"`.
    kind: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    units: Vec<SimId>,
}

impl JournalRecord for PathRequested {
    const KIND: &'static str = "core.path_request";
}

/// Journal `"core.path_result"`: request `request` was processed `waited` ticks after it was
/// made, and charged `charge` (0 when `"stale"`).
#[derive(Debug, Serialize)]
struct PathResult {
    request: u32,
    /// `"found"`, `"partial"`, `"unreachable"` or `"stale"`; for a group request `"flow"` (its
    /// members are on the field), `"single"` (no flow field for its profile: every member was
    /// given a single request, charge 0) or `"stale"`.
    status: &'static str,
    corners: usize,
    /// The plan's cost; for a direct line an octile lower bound, not a true cost (R99); 0 for
    /// a group request.
    cost: u32,
    charge: u32,
    waited: u32,
}

impl JournalRecord for PathResult {
    const KIND: &'static str = "core.path_result";
}

/// The movement profile of `entity` on the [`NavGrid`]; `None` without a grid or
/// [`Mobility`].
pub(crate) fn unit_profile(world: &World, entity: Entity) -> Option<MoveProfile> {
    let nav = world.get_resource::<NavGrid>()?;
    let mobility = world.get::<Mobility>(entity)?;
    Some(profile_of(
        mobility,
        world.get::<Footprint>(entity),
        nav.cell_m(),
        nav.max_clearance(),
    ))
}

/// Ask for a path to `goal` for `unit` (see [`enqueue_path_in`]), journaled in
/// [`Phase::Think`]. Returns the request's id. A request made before [`process_path_queue`]
/// runs (in [`ThinkSet::Orders`] or earlier) is processed this tick, a later one next tick
/// (see the module docs).
pub fn enqueue_path(world: &mut World, unit: SimId, goal: FxVec2) -> u32 {
    enqueue_path_in(world, unit, goal, Phase::Think)
}

/// Push a [`RequestKind::Single`] request for `unit` to `goal` onto the back of the
/// [`PathQueue`], set the unit's [`PathFollow`] (inserted if missing) to wait on it with no
/// corners, and journal `"core.path_request"` in `phase`. Any older request of the unit turns
/// stale. Returns the request's id.
pub(crate) fn enqueue_path_in(world: &mut World, unit: SimId, goal: FxVec2, phase: Phase) -> u32 {
    let tick = world.resource::<SimClock>().tick;
    if world.get_resource::<PathQueue>().is_none() {
        world.insert_resource(PathQueue::default());
    }
    let id = {
        let mut queue = world.resource_mut::<PathQueue>();
        let id = queue.next_id;
        queue.next_id = id.wrapping_add(1);
        queue.pending.push_back(PathRequest {
            id,
            unit,
            goal,
            kind: RequestKind::Single,
            issued: tick,
        });
        id
    };
    let entity = world.resource::<SimIndex>().get(unit);
    let profile = entity.and_then(|entity| unit_profile(world, entity));
    if let Some(entity) = entity {
        let state = PathState::Waiting { request: id };
        match world.get_mut::<PathFollow>(entity) {
            Some(mut follow) => {
                follow.state = state;
                follow.goal = goal;
                follow.corners.clear();
                follow.next = 0;
            }
            None => {
                world.entity_mut(entity).insert(PathFollow {
                    state,
                    goal,
                    ..PathFollow::default()
                });
            }
        }
    }
    tracing::trace!(request = id, unit = unit.0, "path requested");
    let mut journal = world.resource_mut::<Journal>();
    if journal.enabled() {
        journal.record(
            tick,
            phase,
            Some(unit),
            None,
            &PathRequested {
                request: id,
                goal,
                profile,
                kind: "single",
                units: Vec::new(),
            },
        );
    }
    id
}

/// Push one [`RequestKind::Group`] request for `units` (in `SimId` order, each with its own
/// goal, all of one movement profile) toward `goal` onto the back of the [`PathQueue`], set
/// every unit's [`PathFollow`] to wait on it with its own goal and no corners, and journal one
/// `"core.path_request"` (kind `"group"`, for the first unit) in `phase`. Any older request of
/// the units turns stale. Returns the request's id.
///
/// # Panics
/// If `units` is empty.
pub(crate) fn enqueue_group_in(
    world: &mut World,
    units: Vec<(SimId, FxVec2)>,
    goal: FxVec2,
    phase: Phase,
) -> u32 {
    let first = units.first().expect("a group has units").0;
    let tick = world.resource::<SimClock>().tick;
    let id = {
        let mut queue = world.resource_mut::<PathQueue>();
        let id = queue.next_id;
        queue.next_id = id.wrapping_add(1);
        queue.pending.push_back(PathRequest {
            id,
            unit: first,
            goal,
            kind: RequestKind::Group {
                units: units.clone(),
            },
            issued: tick,
        });
        id
    };
    let mut profile = None;
    for &(unit, own_goal) in &units {
        let Some(entity) = world.resource::<SimIndex>().get(unit) else {
            continue;
        };
        profile = profile.or_else(|| unit_profile(world, entity));
        if let Some(mut follow) = world.get_mut::<PathFollow>(entity) {
            follow.state = PathState::Waiting { request: id };
            follow.goal = own_goal;
            follow.corners.clear();
            follow.next = 0;
        }
    }
    tracing::trace!(request = id, units = units.len(), "group path requested");
    let mut journal = world.resource_mut::<Journal>();
    if journal.enabled() {
        journal.record(
            tick,
            phase,
            Some(first),
            None,
            &PathRequested {
                request: id,
                goal,
                profile,
                kind: "group",
                units: units.iter().map(|&(unit, _)| unit).collect(),
            },
        );
    }
    id
}

/// The plan for `entity` from its position to `goal`: [`plan_path`] with its profile, adding
/// the work to `work`. Without a navigation grid the map is open ground: a straight line, at
/// no charge. A unit without a [`Pose`] or [`Mobility`] cannot move: `Unreachable`. Every
/// setup installs the HPA* graphs with the grid ([`install_nav`](crate::nav::install_nav)); a
/// grid without them is a bug, planned as `Unreachable` rather than a line through anything.
fn plan_for(
    world: &World,
    entity: Entity,
    goal: FxVec2,
    rules: &PathRules,
    work: &mut PathWork,
) -> PathPlan {
    let unreachable = || PathPlan {
        corners: Vec::new(),
        cost: 0,
        status: PlanStatus::Unreachable,
    };
    let (Some(pose), Some(mobility)) = (world.get::<Pose>(entity), world.get::<Mobility>(entity))
    else {
        return unreachable();
    };
    let (nav, hpa) = match (
        world.get_resource::<NavGrid>(),
        world.get_resource::<HpaGraphs>(),
    ) {
        (Some(nav), Some(hpa)) => (nav, hpa),
        (None, _) => {
            return PathPlan {
                corners: vec![goal],
                cost: 0,
                status: PlanStatus::Found,
            };
        }
        (Some(_), None) => {
            debug_assert!(false, "a navigation grid without HPA* graphs");
            return unreachable();
        }
    };
    let profile = profile_of(
        mobility,
        world.get::<Footprint>(entity),
        nav.cell_m(),
        nav.max_clearance(),
    );
    plan_path(nav, hpa, profile, pose.pos, goal, rules, work)
}

fn status_name(status: PlanStatus) -> &'static str {
    match status {
        PlanStatus::Found => "found",
        PlanStatus::Partial => "partial",
        PlanStatus::Unreachable => "unreachable",
    }
}

/// Process the [`PathQueue`] (see the module docs): pop requests while the work spent this
/// tick is below `Path.budget_per_tick`, at least one; discard stale ones; plan and apply the
/// others: `Following` with the plan's corners, or `Failed` when `Unreachable`, `planned` =
/// this tick; a group request goes through [`process_group`]. Journals `"core.path_result"`
/// per request and counts the `path.*` and `flowfield.*` counters.
/// Exclusive, the first core system of [`ThinkSet::Behaviour`] (steering runs after it): after
/// every request of `Input` and [`ThinkSet::Orders`], before every feature `Behaviour` system.
pub(crate) fn process_path_queue(world: &mut World) {
    if world
        .get_resource::<PathQueue>()
        .is_none_or(|queue| queue.pending.is_empty())
    {
        return;
    }
    let tick = world.resource::<SimClock>().tick;
    let rules = *world.resource::<Content>().0.rules::<PathRules>();
    let mut spent: u32 = 0;
    let mut processed: u64 = 0;
    let mut total = PathWork::default();
    let mut flow = FlowWork::default();
    loop {
        if processed > 0 && spent >= rules.budget_per_tick {
            break;
        }
        let Some(request) = world.resource_mut::<PathQueue>().pending.pop_front() else {
            break;
        };
        processed += 1;
        let waited = tick.0.saturating_sub(request.issued.0);
        if let RequestKind::Group { units } = &request.kind {
            let (status, charge) = match process_group(world, tick, &request, units, &mut flow) {
                GroupOutcome::Flow { charge } => ("flow", charge),
                GroupOutcome::Single => ("single", 0),
                GroupOutcome::Stale => ("stale", 0),
            };
            spent = spent.saturating_add(charge);
            let result = PathResult {
                request: request.id,
                status,
                corners: 0,
                cost: 0,
                charge,
                waited,
            };
            record_result(world, tick, request.unit, result);
            continue;
        }
        let entity = world
            .resource::<SimIndex>()
            .get(request.unit)
            .filter(|&entity| {
                world.get::<PathFollow>(entity).map(|f| f.state)
                    == Some(PathState::Waiting {
                        request: request.id,
                    })
            });
        let Some(entity) = entity else {
            tracing::trace!(request = request.id, "stale path request discarded");
            record_result(
                world,
                tick,
                request.unit,
                PathResult {
                    request: request.id,
                    status: "stale",
                    corners: 0,
                    cost: 0,
                    charge: 0,
                    waited,
                },
            );
            continue;
        };
        let mut work = PathWork::default();
        let plan = plan_for(world, entity, request.goal, &rules, &mut work);
        let charge = work.charge();
        spent = spent.saturating_add(charge);
        total.nodes_expanded = total.nodes_expanded.saturating_add(work.nodes_expanded);
        total.los_cells = total.los_cells.saturating_add(work.los_cells);
        let result = PathResult {
            request: request.id,
            status: status_name(plan.status),
            corners: plan.corners.len(),
            cost: plan.cost,
            charge,
            waited,
        };
        if let Some(mut follow) = world.get_mut::<PathFollow>(entity) {
            apply_plan(&mut follow, plan, tick);
        }
        record_result(world, tick, request.unit, result);
    }
    let left = world.resource::<PathQueue>().pending.len() as u64;
    let mut counters = world.resource_mut::<SimCounters>();
    counters.add("path.requests", processed);
    for (name, n) in [
        ("path.nodes_expanded", u64::from(total.nodes_expanded)),
        ("path.los_cells", u64::from(total.los_cells)),
        ("path.budget_spent", u64::from(spent)),
        ("path.queue_len", left),
        ("flowfield.abstract_nodes", u64::from(flow.abstract_nodes)),
        ("flowfield.cells", u64::from(flow.cells)),
    ] {
        if n > 0 {
            counters.add(name, n);
        }
    }
}

/// What processing a group request did.
enum GroupOutcome {
    /// Charged `charge`, the cold cost of its field; the members that could were put on it.
    Flow { charge: u32 },
    /// No flow field for its profile (no navigation grid or no HPA* graph): every waiting
    /// member was given a single request, and nothing was charged.
    Single,
    /// No member exists and still waits on it.
    Stale,
}

/// Process group request `request` for `members` in tick `tick` (see the module docs), adding
/// the flow-field builds' work to `flow`.
///
/// The field's key is the cluster holding `request.goal` and the profile of the first member
/// still waiting. In `SimId` order, each waiting member whose cell has a direction
/// ([`FlowCache::direction`], which builds the start's tile) is put on the field: `Flow`, its
/// own goal, no corners, `planned` = `tick`. The others (and every member, without a
/// navigation grid or an HPA* graph: [`GroupOutcome::Single`]) get single requests at the
/// back of the queue, in `SimId` order, journaled in `Think`.
fn process_group(
    world: &mut World,
    tick: Tick,
    request: &PathRequest,
    members: &[(SimId, FxVec2)],
    flow: &mut FlowWork,
) -> GroupOutcome {
    let waiting: Vec<(SimId, Entity, FxVec2)> = {
        let index = world.resource::<SimIndex>();
        members
            .iter()
            .filter_map(|&(id, goal)| {
                let entity = index.get(id)?;
                let state = world.get::<PathFollow>(entity)?.state;
                (state
                    == PathState::Waiting {
                        request: request.id,
                    })
                .then_some((id, entity, goal))
            })
            .collect()
    };
    let Some(&(_, first, _)) = waiting.first() else {
        return GroupOutcome::Stale;
    };
    let profile = unit_profile(world, first);
    // Per waiting member, whether it starts on the field; and the key and the charge.
    let routed = world.resource_scope(|world, mut cache: Mut<FlowCache>| {
        let (Some(nav), Some(hpa), Some(profile)) = (
            world.get_resource::<NavGrid>(),
            world.get_resource::<HpaGraphs>(),
            profile,
        ) else {
            return None;
        };
        let graph = hpa.graph(profile)?;
        let key = FlowKey {
            cluster: graph.cluster_of(nav.cell_of(request.goal)),
            profile,
        };
        cache.assert_tick(tick);
        let charge = cache.cold_cost(key, nav, hpa, flow);
        let starts: Vec<bool> = waiting
            .iter()
            .map(|&(_, entity, _)| {
                world.get::<Pose>(entity).is_some_and(|pose| {
                    cache
                        .direction(key, nav.cell_of(pose.pos), nav, hpa, flow)
                        .is_some()
                })
            })
            .collect();
        Some((key, charge, starts))
    });
    let (outcome, singles) = match routed {
        Some((key, charge, starts)) => {
            let mut singles = Vec::new();
            for (&(id, entity, goal), on_field) in waiting.iter().zip(starts) {
                if !on_field {
                    singles.push((id, goal));
                    continue;
                }
                if let Some(mut follow) = world.get_mut::<PathFollow>(entity) {
                    follow.state = PathState::Flow { key };
                    follow.goal = goal;
                    follow.corners.clear();
                    follow.next = 0;
                    follow.planned = tick;
                }
            }
            (GroupOutcome::Flow { charge }, singles)
        }
        None => (
            GroupOutcome::Single,
            waiting.iter().map(|&(id, _, goal)| (id, goal)).collect(),
        ),
    };
    for (id, goal) in singles {
        enqueue_path_in(world, id, goal, Phase::Think);
    }
    tracing::trace!(request = request.id, "group request processed");
    outcome
}

/// Put `plan` into `follow`: `Following` its corners from the first (at most `u16::MAX` of
/// them, which `next` can index), or `Failed` with none when `Unreachable`.
fn apply_plan(follow: &mut PathFollow, plan: PathPlan, tick: Tick) {
    follow.planned = tick;
    follow.next = 0;
    follow.corners = plan.corners;
    follow.corners.truncate(usize::from(u16::MAX));
    follow.state = match plan.status {
        PlanStatus::Unreachable => {
            follow.corners.clear();
            PathState::Failed
        }
        _ if follow.corners.is_empty() => PathState::Done,
        _ => PathState::Following,
    };
}

fn record_result(world: &mut World, tick: Tick, unit: SimId, result: PathResult) {
    tracing::trace!(
        request = result.request,
        status = result.status,
        charge = result.charge,
        "path result"
    );
    let mut journal = world.resource_mut::<Journal>();
    if journal.enabled() {
        journal.record(tick, Phase::Think, Some(unit), None, &result);
    }
}

/// Whether the segment `a → b` touches the closed box `min..=max`: their bounding boxes
/// overlap and the box's corners are not all strictly on one side of the segment's line.
fn segment_touches_box(a: FxVec2, b: FxVec2, min: FxVec2, max: FxVec2) -> bool {
    if a.x.max(b.x) < min.x || a.x.min(b.x) > max.x || a.y.max(b.y) < min.y || a.y.min(b.y) > max.y
    {
        return false;
    }
    let d = b - a;
    let side = |p: FxVec2| d.perp_dot(p - a);
    let sides = [
        side(min),
        side(FxVec2::new(max.x, min.y)),
        side(max),
        side(FxVec2::new(min.x, max.y)),
    ];
    !(sides.iter().all(|&s| s > Fx::ZERO) || sides.iter().all(|&s| s < Fx::ZERO))
}

/// After a terrain change rebuilt the HPA* clusters covering `rects` (cells): in
/// [`SimIndex`] order, every `Following` unit whose remaining route (its position, then
/// `corners[next..]`) touches one of them is set to `Idle` with no corners, journaled
/// `"core.order_changed"` `"replanned"` in phase `Input` (terrain changes come from
/// commands); [`advance_orders`](crate::orders) asks for a new path in the same tick.
pub(crate) fn invalidate_paths(world: &mut World, rects: &[CellRect]) {
    let Some(cell) = world.get_resource::<NavGrid>().map(NavGrid::cell_m) else {
        return;
    };
    if rects.is_empty() {
        return;
    }
    let metres = |i: u32| cell.mul_int(i32::try_from(i).unwrap_or(i32::MAX));
    let boxes: Vec<(FxVec2, FxVec2)> = rects
        .iter()
        .map(|r| {
            (
                FxVec2::new(metres(r.min.0), metres(r.min.1)),
                FxVec2::new(metres(r.max.0 + 1), metres(r.max.1 + 1)),
            )
        })
        .collect();
    let tick = world.resource::<SimClock>().tick;
    let units: Vec<(SimId, Entity)> = world.resource::<SimIndex>().iter().collect();
    for (id, entity) in units {
        let (Some(pose), Some(follow)) =
            (world.get::<Pose>(entity), world.get::<PathFollow>(entity))
        else {
            continue;
        };
        if follow.state != PathState::Following {
            continue;
        }
        let rest = follow
            .corners
            .get(usize::from(follow.next)..)
            .unwrap_or(&[]);
        let mut from = pose.pos;
        let crosses = rest.iter().any(|&to| {
            let hit = boxes
                .iter()
                .any(|&(min, max)| segment_touches_box(from, to, min, max));
            from = to;
            hit
        });
        if !crosses {
            continue;
        }
        if let Some(mut follow) = world.get_mut::<PathFollow>(entity) {
            follow.state = PathState::Idle;
            follow.corners.clear();
            follow.next = 0;
        }
        journal_order_changed(world, tick, Phase::Input, id, None, Reason::Replanned);
    }
}

/// The name `"core.paths"` gives a state.
fn state_name(state: PathState) -> &'static str {
    match state {
        PathState::Idle => "idle",
        PathState::Waiting { .. } => "waiting",
        PathState::Following => "following",
        PathState::Flow { .. } => "flow",
        PathState::Done => "done",
        PathState::Failed => "failed",
    }
}

fn raw(v: FxVec2) -> Value {
    json!([v.x.raw(), v.y.raw()])
}

fn order_json(order: &Order) -> Value {
    match order {
        Order::Move { target } => json!({ "move": raw(*target) }),
        Order::Rotate { facing } => json!({ "rotate": facing.raw() }),
        Order::Follow { leader, .. } => json!({ "follow": leader.0 }),
    }
}

/// `core.paths`: args `{"units": [id, ..]}` → `{"<id>": {"state", "goal": [xr, yr],
/// "corners": [[xr, yr], ..], "next", "orders": [{"move": [xr, yr]} | {"rotate": raw} |
/// {"follow": id}, ..], "profile": {"class", "size"}}}`, positions as raw `Fx`. An id that is
/// not a mobile unit is an error.
fn paths(world: &World, args: &Value) -> Result<Value, String> {
    let units = args
        .get("units")
        .and_then(Value::as_array)
        .ok_or_else(|| "give {\"units\": [id, ..]}".to_owned())?;
    let nav = world
        .get_resource::<NavGrid>()
        .ok_or_else(|| "no map is set up: there is no navigation grid".to_owned())?;
    let index = world.resource::<SimIndex>();
    let mut out = Map::new();
    for unit in units {
        let id = unit
            .as_u64()
            .and_then(|id| u32::try_from(id).ok())
            .ok_or_else(|| format!("unit ids are sim ids, not {unit}"))?;
        let entity = index
            .get(SimId(id))
            .ok_or_else(|| format!("no unit #{id}"))?;
        let (Some(follow), Some(mobility)) = (
            world.get::<PathFollow>(entity),
            world.get::<Mobility>(entity),
        ) else {
            return Err(format!("unit #{id} cannot move"));
        };
        let profile = profile_of(
            mobility,
            world.get::<Footprint>(entity),
            nav.cell_m(),
            nav.max_clearance(),
        );
        let orders: Vec<Value> = world
            .get::<MoveOrder>(entity)
            .map(|o| o.orders.iter().map(order_json).collect())
            .unwrap_or_default();
        out.insert(
            id.to_string(),
            json!({
                "state": state_name(follow.state),
                "goal": raw(follow.goal),
                "corners": follow.corners.iter().copied().map(raw).collect::<Vec<_>>(),
                "next": follow.next,
                "orders": orders,
                "profile": { "class": class_name(profile.class), "size": profile.size },
            }),
        );
    }
    Ok(Value::Object(out))
}

/// The R78 fill of `"core.path_queue"` for an M0 save: an empty queue.
fn fill_path_queue(world: &mut World) -> Result<(), String> {
    world.insert_resource(PathQueue::default());
    Ok(())
}

/// The R78 fill of `"core.path_follow"` for an M0 save: an idle [`PathFollow`] on every unit
/// with [`Mobility`], in [`SimIndex`] order, as a live spawn has.
fn fill_path_follow(world: &mut World) -> Result<(), String> {
    let units: Vec<Entity> = world
        .resource::<SimIndex>()
        .iter()
        .map(|(_, e)| e)
        .collect();
    for entity in units {
        if world.get::<Mobility>(entity).is_some() && world.get::<PathFollow>(entity).is_none() {
            world.entity_mut(entity).insert(PathFollow::default());
        }
    }
    Ok(())
}

/// The Hashed [`PathFollow`] and [`PathQueue`] (each with its R78 fill), the path journal
/// records, the `"core.paths"` debug query and [`process_path_queue`] in
/// [`ThinkSet::Behaviour`].
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.app.init_resource::<PathQueue>();
    sim.register_component::<PathFollow>(
        "core.path_follow",
        Class::Hashed(
            Codec::new(1)
                .trace_enum::<MoveClass>()
                .trace_enum::<PathState>()
                .fill_missing(fill_path_follow),
        ),
    )
    .register_resource::<PathQueue>(
        "core.path_queue",
        Class::Hashed(
            Codec::new(1)
                .trace_enum::<RequestKind>()
                .fill_missing(fill_path_queue),
        ),
    )
    .register_journal_record::<PathRequested>()
    .register_journal_record::<PathResult>()
    .register_debug_query("core.paths", paths)
    .add_systems(ThinkSet::Behaviour, process_path_queue);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: i32, y: i32) -> FxVec2 {
        FxVec2::new(Fx::from_int(x), Fx::from_int(y))
    }

    #[test]
    fn segment_box_contact() {
        let (min, max) = (v(10, 10), v(20, 20));
        assert!(
            segment_touches_box(v(0, 15), v(30, 15), min, max),
            "through"
        );
        assert!(
            segment_touches_box(v(12, 12), v(14, 14), min, max),
            "inside"
        );
        assert!(
            segment_touches_box(v(0, 0), v(10, 10), min, max),
            "the corner"
        );
        assert!(!segment_touches_box(v(0, 0), v(30, 0), min, max), "below");
        // The bounding boxes overlap, but the diagonal passes the box's corner.
        assert!(!segment_touches_box(
            v(0, 25),
            v(25, 0),
            v(15, 15),
            v(20, 20)
        ));
        assert!(
            segment_touches_box(v(15, 15), v(15, 15), min, max),
            "a point inside"
        );
    }
}
