//! Driving units along their planned paths and flow fields ([`PathFollow`]) over the terrain,
//! each step with the unit's separation push ([`SteerPushes`], worked out earlier in the tick
//! by [`steer`](crate::steer)) and never into a cell its profile cannot pass; and the
//! [`Wander`] behaviour that gives idle wanderers move orders. Movement is entity-local, so it
//! iterates in any order (its flow-field reads only fill an outcome-neutral cache, and the
//! pushes come from start-of-tick positions); wandering requests paths and the flow-tile
//! warm-up fills the cache, so they walk units in `SimId` order. The `"Movement"` and
//! `"Steering"` rules sections live here.

use std::collections::BTreeMap;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryState;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{InMut, IntoSystem, Query, Res, ResMut};
use bevy_ecs::world::{Mut, World};
use fl_content::{LinkCtx, MoveClass, RulesSection};
use fl_math::{Angle, Fx, FxVec2};
use serde::{Deserialize, Serialize};

use crate::build::Content;
use crate::builder::SimBuilder;
use crate::clock::SimClock;
use crate::counters::SimCounters;
use crate::ids::SimEntity;
use crate::map::MapInfo;
use crate::nav::{
    COST_ROAD, CellXY, FlowCache, FlowDir, FlowWork, HpaGraphs, MoveProfile, NavGrid, cell_centre,
    line_clear, profile_of, step_clear,
};
use crate::orders::{OrderScratch, advance_orders};
use crate::path::{FlowKey, PathFollow, PathState, enqueue_path_in};
use crate::rng::{MatchSeed, Stream, rolls};
use crate::schedule::{ActSet, Phase, ThinkSet};
use crate::steer::SteerPushes;
use crate::units::{Footprint, Mobility, MoveOrder, Order, Pose, Wander};

/// `rules.ron` section `"Movement"`; every value is a guess until tuned (spec §7).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MovementRules {
    /// The least fraction of road speed a unit keeps on any cell (the speed formula's clamp).
    pub min_speed_frac: Fx,
    /// A unit whose heading is off by more than this turns in place, in degrees.
    pub turn_in_place_deg: Fx,
    /// How close to a waypoint counts as arrived, in metres.
    pub arrive_m: Fx,
    /// How far apart a formation's slots may spread, in metres.
    pub formation_max_spread_m: Fx,
    /// How far behind its leader a follower keeps, in metres.
    pub follow_distance_m: Fx,
    /// How often a follower re-plans toward its leader, in ticks.
    pub follow_repath_ticks: u32,
    /// The largest height step to a neighbour cell a class can climb, in metres.
    pub max_slope_m: BTreeMap<MoveClass, Fx>,
    /// The terrain factor path costs use, per class (a unit's own factor drives its speed).
    pub path_terrain_factor: BTreeMap<MoveClass, Fx>,
}

impl RulesSection for MovementRules {
    /// Ranges per ADR-0014: `min_speed_frac` in (0, 1] and at least `COST_ROAD / 65535` (so
    /// every path cost fits a `u16`); `max_slope_m` and `path_terrain_factor` each cover every
    /// class, slopes are at least 0, path factors lie in [−1, 1]. Path following (Task 8):
    /// `turn_in_place_deg` in (0, 180] (an angle beyond wraps), `arrive_m` above 0. Group
    /// orders (Task 9): `formation_max_spread_m` and `follow_distance_m` in [0, 4096] m (the
    /// largest map side, R89), `follow_repath_ticks` at least 1.
    fn link(&self, ctx: &mut LinkCtx<'_>) {
        let turn = self.turn_in_place_deg;
        if turn <= Fx::ZERO || turn > Fx::from_int(180) {
            ctx.error(
                "turn_in_place_deg",
                format!("{turn} must lie in (0, 180] degrees"),
            );
        }
        if self.arrive_m <= Fx::ZERO {
            ctx.error("arrive_m", format!("{} m must be above 0", self.arrive_m));
        }
        // Group orders (Task 9): a negative spread or distance would flip offsets, one beyond
        // the largest map side could overflow a product of distances (final review M2), and a
        // follower re-plans at most once per `follow_repath_ticks`.
        for (field, metres) in [
            ("formation_max_spread_m", self.formation_max_spread_m),
            ("follow_distance_m", self.follow_distance_m),
        ] {
            if metres < Fx::ZERO || metres > Fx::from_int(MAX_RULE_DISTANCE_M) {
                ctx.error(
                    field,
                    format!("{metres} m must lie in [0, {MAX_RULE_DISTANCE_M}] m"),
                );
            }
        }
        if self.follow_repath_ticks < 1 {
            ctx.error("follow_repath_ticks", "must be at least 1");
        }
        let frac = self.min_speed_frac;
        // frac ≥ COST_ROAD / 65535, on raw integers.
        let least = i128::from(COST_ROAD) << 32;
        if frac <= Fx::ZERO || frac > Fx::ONE {
            ctx.error("min_speed_frac", format!("{frac} must lie in (0, 1]"));
        } else if i128::from(frac.raw()) * i128::from(u16::MAX) < least {
            ctx.error(
                "min_speed_frac",
                format!(
                    "{frac} is below {COST_ROAD}/{}: path costs would not fit a u16",
                    u16::MAX
                ),
            );
        }
        for (field, map) in [
            ("max_slope_m", &self.max_slope_m),
            ("path_terrain_factor", &self.path_terrain_factor),
        ] {
            for class in MoveClass::ALL {
                if !map.contains_key(&class) {
                    ctx.error(field, format!("has no entry for {class:?}"));
                }
            }
        }
        for (class, slope) in &self.max_slope_m {
            if *slope < Fx::ZERO {
                ctx.error(
                    format!("max_slope_m.{class:?}"),
                    format!("{slope} must be at least 0"),
                );
            }
        }
        for (class, factor) in &self.path_terrain_factor {
            if *factor < -Fx::ONE || *factor > Fx::ONE {
                ctx.error(
                    format!("path_terrain_factor.{class:?}"),
                    format!("{factor} must lie in [-1, 1]"),
                );
            }
        }
    }
}

/// `rules.ron` section `"Steering"`; every value is a guess until tuned (spec §7).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SteeringRules {
    /// How strongly units push apart.
    pub separation_gain: Fx,
    /// The gap kept between footprints, in metres.
    pub separation_margin_m: Fx,
    /// An idle unit steers every this many ticks: 1..=64.
    pub idle_every: u32,
    /// The largest push per tick, as a fraction of the unit's speed.
    pub max_push_frac: Fx,
}

/// The largest `Steering.idle_every`: an idle unit's push cap is `max_push_frac × speed ×
/// idle_every` (Task 11 review Minor 8), so the stagger is bounded.
const MAX_IDLE_EVERY: u32 = 64;

/// The largest `Steering.separation_gain`.
const MAX_SEPARATION_GAIN: i32 = 4;

/// The largest `Steering.separation_margin_m`, in metres: steering squares the reach of a
/// footprint plus its margin, which stays far inside `Fx`.
const MAX_SEPARATION_MARGIN_M: i32 = 50;

/// The largest distance a `Movement` rule names, in metres: the largest map side `fl_content`
/// accepts (R89).
const MAX_RULE_DISTANCE_M: i32 = 4096;

impl RulesSection for SteeringRules {
    /// `idle_every` in `1..=64`, `separation_gain` in [0, 4], `separation_margin_m` in [0, 50]
    /// m and `max_push_frac` in (0, 1] (final review M2): a negative gain attracts, a push cap
    /// of 0 or less reverses the push, and an extreme margin overflows its square.
    fn link(&self, ctx: &mut LinkCtx<'_>) {
        if !(1..=MAX_IDLE_EVERY).contains(&self.idle_every) {
            ctx.error(
                "idle_every",
                format!("{} must lie in 1..={MAX_IDLE_EVERY}", self.idle_every),
            );
        }
        let gain = self.separation_gain;
        if gain < Fx::ZERO || gain > Fx::from_int(MAX_SEPARATION_GAIN) {
            ctx.error(
                "separation_gain",
                format!("{gain} must lie in [0, {MAX_SEPARATION_GAIN}]"),
            );
        }
        let margin = self.separation_margin_m;
        if margin < Fx::ZERO || margin > Fx::from_int(MAX_SEPARATION_MARGIN_M) {
            ctx.error(
                "separation_margin_m",
                format!("{margin} m must lie in [0, {MAX_SEPARATION_MARGIN_M}] m"),
            );
        }
        let push = self.max_push_frac;
        if push <= Fx::ZERO || push > Fx::ONE {
            ctx.error("max_push_frac", format!("{push} must lie in (0, 1]"));
        }
    }
}

/// Give every wandering unit without orders a move to a uniform point in `home ± radius`
/// (x drawn first, then y, from the unit's draws of `stream`, `"debug.wander"`, this tick),
/// clamped to the map, and ask for its path, walking units in `SimId` order. Exclusive,
/// chained after [`advance_orders`] in [`ThinkSet::Orders`], so a unit whose last order just
/// ended wanders on in the same tick. Its query state is cached in the system and its scratch
/// list is [`OrderScratch`], so it allocates nothing per tick of its own.
pub(crate) fn wander_system(
    InMut(stream): InMut<Stream>,
    world: &mut World,
    query: &mut WanderQuery,
) {
    let Some(map) = world.get_resource::<MapInfo>().cloned() else {
        return;
    };
    let tick = world.resource::<SimClock>().tick;
    let seed = *world.resource::<MatchSeed>();
    let mut idle = std::mem::take(&mut world.resource_mut::<OrderScratch>().wanderers);
    idle.extend(
        query
            .iter(world)
            .filter(|(_, _, _, order)| order.orders.is_empty())
            .map(|(entity, sim, wander, _)| (sim.id(), entity, *wander)),
    );
    idle.sort_unstable_by_key(|&(id, _, _)| id);
    for &(id, entity, wander) in &idle {
        let mut draws = rolls(&seed, tick, id, *stream);
        let r = wander.radius.max(Fx::ZERO);
        let x = draws.range_fx(-r, r);
        let y = draws.range_fx(-r, r);
        let target = map.clamp(wander.home + FxVec2::new(x, y));
        if let Some(mut order) = world.get_mut::<MoveOrder>(entity) {
            order.orders.push(Order::Move { target });
        }
        enqueue_path_in(world, id, target, Phase::Think);
    }
    idle.clear();
    world.resource_mut::<OrderScratch>().wanderers = idle;
}

/// The units [`wander_system`] looks at; a cache of matching archetypes, never sim state.
type WanderQuery = QueryState<(
    Entity,
    &'static SimEntity,
    &'static Wander,
    &'static MoveOrder,
)>;

/// The units [`prepare_flow_tiles`] looks at; a cache of matching archetypes, never sim state.
type FlowQuery = QueryState<(&'static SimEntity, &'static Pose, &'static PathFollow)>;

/// Warm the flow-field tiles movement is about to read: in [`SimIndex`](crate::SimIndex)
/// order, [`FlowCache::ensure_tile`] for the cluster each unit on a flow field stands in.
/// A warm-up only: [`move_system`] reads through [`FlowCache::direction`], which rebuilds
/// anything evicted since (R84), so it changes no outcome; its builds count as
/// `"flowfield.*"`. Exclusive, in the core slot of [`ActSet::Move`], right before
/// [`move_system`]. Its query state is cached in the system and its scratch list is
/// [`OrderScratch`], so it allocates nothing per tick of its own.
pub(crate) fn prepare_flow_tiles(world: &mut World, query: &mut FlowQuery) {
    let mut due = std::mem::take(&mut world.resource_mut::<OrderScratch>().flow);
    if let Some(nav) = world.get_resource::<NavGrid>() {
        due.extend(
            query
                .iter(world)
                .filter_map(|(sim, pose, follow)| match follow.state {
                    PathState::Flow { key } => Some((sim.id(), key, nav.cell_of(pose.pos))),
                    _ => None,
                }),
        );
    }
    if !due.is_empty() {
        due.sort_unstable_by_key(|&(id, _, _)| id);
        let tick = world.resource::<SimClock>().tick;
        let mut work = FlowWork::default();
        world.resource_scope(|world, mut cache: Mut<FlowCache>| {
            let (Some(nav), Some(hpa)) = (
                world.get_resource::<NavGrid>(),
                world.get_resource::<HpaGraphs>(),
            ) else {
                return;
            };
            cache.assert_tick(tick);
            for &(_, key, cell) in &due {
                if let Some(graph) = hpa.graph(key.profile) {
                    cache.ensure_tile(key, graph.cluster_of(cell), nav, hpa, &mut work);
                }
            }
        });
        count_flow_work(&mut world.resource_mut::<SimCounters>(), work);
    }
    due.clear();
    world.resource_mut::<OrderScratch>().flow = due;
}

/// Add `work` to the `"flowfield.*"` counters.
fn count_flow_work(counters: &mut SimCounters, work: FlowWork) {
    if work.abstract_nodes > 0 {
        counters.add("flowfield.abstract_nodes", u64::from(work.abstract_nodes));
    }
    if work.cells > 0 {
        counters.add("flowfield.cells", u64::from(work.cells));
    }
}

/// A unit stuck at a blocked step asks for a new path at most once per this many ticks (Task 8
/// review Minor 8): while its plan is younger, a fully blocked step leaves the path as it is
/// and the unit tries again next tick (a neighbour or a push may have moved meanwhile); once the
/// plan is this old the path goes back to `Idle`, and the order asks for a new one. So a blocked
/// unit never re-requests every tick, and cannot livelock with steering.
const BLOCKED_REPLAN_TICKS: u32 = 10;

/// How many cells of its flow field's direction chain a unit looks along for the farthest one
/// it sees (R111), its own next cell included.
const FLOW_LOOKAHEAD_CELLS: u32 = 4;

/// Road speed times the speed factor of the cell under `pos` (spec §7); open ground without a
/// grid.
fn speed_at(nav: Option<&NavGrid>, mobility: &Mobility, pos: FxVec2) -> Fx {
    match nav {
        Some(nav) => {
            let (x, y) = nav.cell_of(pos);
            mobility.speed * nav.speed_factor(mobility.terrain_factor, x, y)
        }
        None => mobility.speed,
    }
}

/// Turn `pose` one tick toward `target` by at most the turn rate. Unless the heading is still
/// off by more than `turn_in_place` (raw [`Angle`] units), the drive's step: the point `speed`
/// along the straight line (or `target` itself within `speed`, then `reached`), clamped to
/// `map`. `None` while the unit turns in place. Passability is [`take_step`]'s.
fn aim(
    pose: &mut Pose,
    mobility: &Mobility,
    target: FxVec2,
    speed: Fx,
    turn_in_place: i32,
    map: &MapInfo,
) -> Option<(FxVec2, bool)> {
    let to_target = target - pose.pos;
    let reached = to_target.length() <= speed;
    if to_target != FxVec2::ZERO {
        let desired = to_target.angle();
        pose.heading = pose.heading.rotate_towards(desired, mobility.turn_rate);
        if pose.heading.signed_diff(desired).abs() > turn_in_place {
            return None;
        }
    }
    let to = map.clamp(if reached {
        target
    } else {
        pose.pos + to_target.normalize_or_zero() * speed
    });
    Some((to, reached))
}

/// Which move [`take_step`] made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Taken {
    /// The drive's step plus the push.
    Whole,
    /// The drive's step without the push.
    Unpushed,
    /// One axis of the whole move.
    Slid,
    /// None fits: the unit stays.
    Stayed,
}

/// Where a unit at `from` goes when its drive's step is `step` (zero when it does not drive)
/// and its separation push is `push`. The candidates, in order, each clamped to `map` and
/// skipped when it does not move the unit or repeats an earlier one: the whole move `step +
/// push`; the step without the push (a push never stalls a step that fits on its own); the
/// whole move's x part alone; its y part alone. The first that `fits` is taken; if none does,
/// the unit stays.
fn take_step(
    from: FxVec2,
    step: FxVec2,
    push: FxVec2,
    map: &MapInfo,
    mut fits: impl FnMut(FxVec2) -> bool,
) -> (Taken, FxVec2) {
    let whole = step + push;
    let candidates = [
        (Taken::Whole, whole),
        (Taken::Unpushed, step),
        (Taken::Slid, FxVec2::new(whole.x, Fx::ZERO)),
        (Taken::Slid, FxVec2::new(Fx::ZERO, whole.y)),
    ];
    let mut tried = [from; 4];
    for (i, (taken, delta)) in candidates.into_iter().enumerate() {
        let to = map.clamp(from + delta);
        tried[i] = to;
        if to == from || tried[..i].contains(&to) {
            continue;
        }
        if fits(to) {
            return (taken, to);
        }
    }
    (Taken::Stayed, from)
}

/// The cell a unit at `pos` on flow field `key` steers to (R111): from `first`, the next cell
/// of its own cell's direction, along the field's direction chain, the farthest of the next
/// [`FLOW_LOOKAHEAD_CELLS`] cells whose centre `pos` sees ([`line_clear`]); the walk stops at
/// the first that is not seen or has no next cell. So a unit cuts across the field's 45° bends
/// instead of driving cell centre to cell centre. [`FlowCache::direction`] is a pure function of
/// the navigation data (R84), so this depends on no cache state. Adds the line-of-sight cells
/// to `los` and any field builds to `work`.
#[expect(
    clippy::too_many_arguments,
    reason = "the field, the unit and the work counters"
)]
fn look_ahead(
    flow: &mut FlowCache,
    key: FlowKey,
    first: CellXY,
    pos: FxVec2,
    nav: &NavGrid,
    hpa: &HpaGraphs,
    profile: MoveProfile,
    work: &mut FlowWork,
    los: &mut u32,
) -> CellXY {
    let mut aim = first;
    for _ in 1..FLOW_LOOKAHEAD_CELLS {
        let Some(FlowDir::Toward(next)) = flow.direction(key, aim, nav, hpa, work) else {
            break;
        };
        if !line_clear(nav, profile, pos, cell_centre(nav, next), los) {
            break;
        }
        aim = next;
    }
    aim
}

/// Set `follow` back to `Idle` with no corners, so its order asks for a new path.
fn to_idle(follow: &mut PathFollow) {
    follow.state = PathState::Idle;
    follow.corners.clear();
    follow.next = 0;
}

/// The units [`move_system`] drives.
type MoveQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static SimEntity,
        &'static mut Pose,
        &'static Mobility,
        Option<&'static Footprint>,
        &'static MoveOrder,
        &'static mut PathFollow,
    ),
>;

/// What a unit drives to this tick.
#[derive(Clone, Copy)]
enum DriveTo {
    /// Corner `corners[next]` of its path.
    Corner(FxVec2),
    /// A cell centre on flow field `key`.
    Field(FxVec2, FlowKey),
}

/// Drive every unit along its path or flow field, add its separation push ([`SteerPushes`]),
/// and turn every unit whose current order is a rotate.
///
/// A `Following` unit drives to `corners[next]`. Within `Movement.arrive_m` of the last
/// corner, but more than one step away, it is done where it stands (`Done`). Otherwise its
/// heading turns toward the corner by at most its turn rate; while the heading is off by more
/// than `Movement.turn_in_place_deg` it turns in place, else its step is its speed along the
/// straight line to the corner: road speed times the [`NavGrid::speed_factor`] of the cell
/// under it (spec §7). Within one step of the corner it snaps onto it and, when that step is
/// taken (with or without its push), goes on to the next one; past the last one the path is
/// `Done`.
///
/// A unit on a flow field (`Flow { key }`) reads [`FlowCache::direction`] for the cell it
/// stands in: [`FlowDir::Toward`] a cell, it drives the same way to the centre of the cell
/// [`look_ahead`] picks along the field (R111); [`FlowDir::Arrived`] (in the destination
/// cluster), it follows a straight path to its goal if [`line_clear`] from where it stands, and
/// in this tick, else it goes `Idle`, so its order asks for a single path; no direction (no
/// route from its cell, say after a terrain change), it goes `Idle` too. Reads build whatever
/// the cache lacks (R84); that work counts as `"flowfield.*"`.
///
/// **The step** (Task 11). Every unit, driving or not, adds this tick's push from
/// [`SteerPushes`] to its step, and [`take_step`] picks the move: the whole move, the step
/// without the push, the x part alone, the y part alone. A move fits when every cell its
/// segment sweeps is passable for the unit's profile ([`step_clear`]: no diagonal corner cut,
/// no hop over a thin wall; a unit stranded in a blocked cell may still leave it) and, for a
/// unit on a flow field, when the cell it ends in (if another) has a direction, so a push never
/// drops it off the field. The position is always clamped to the map; without a [`NavGrid`] the
/// map is open ground. A unit that moves nowhere though it had a step or a push counts
/// `"move.blocked"`; one whose drive was blocked keeps its path while the plan is younger than
/// [`BLOCKED_REPLAN_TICKS`], then goes back to `Idle` (with no corners), so its order asks
/// for a new one (Task 8 review Minor 8). Line-of-sight cells tested (sweeps, look-ahead,
/// arrival) count as `"move.los_cells"`: movement work, never path budget.
///
/// A unit neither following a path nor on a field whose current order is [`Order::Rotate`]
/// turns toward its facing by at most its turn rate.
#[expect(clippy::too_many_arguments, reason = "the resources movement reads")]
pub(crate) fn move_system(
    content: Res<Content>,
    clock: Res<SimClock>,
    map: Option<Res<MapInfo>>,
    nav: Option<Res<NavGrid>>,
    hpa: Option<Res<HpaGraphs>>,
    pushes: Res<SteerPushes>,
    mut flow: ResMut<FlowCache>,
    mut counters: ResMut<SimCounters>,
    mut units: MoveQuery,
) {
    let Some(map) = map else {
        return;
    };
    let nav = nav.as_deref();
    let hpa = hpa.as_deref();
    let rules = content.0.rules::<MovementRules>();
    let turn_in_place = i32::from(Angle::from_degrees(rules.turn_in_place_deg).raw());
    let arrive = rules.arrive_m;
    let tick = clock.tick;
    flow.assert_tick(tick);
    let mut work = FlowWork::default();
    let mut los: u32 = 0;
    let mut blocked = 0;
    for (sim, mut pose, mobility, footprint, order, mut follow) in &mut units {
        let profile =
            nav.map(|nav| profile_of(mobility, footprint, nav.cell_m(), nav.max_clearance()));
        let mut drive_to = None;
        if let PathState::Flow { key } = follow.state {
            match (nav, hpa, profile) {
                (Some(nav), Some(hpa), Some(profile)) => {
                    match flow.direction(key, nav.cell_of(pose.pos), nav, hpa, &mut work) {
                        Some(FlowDir::Toward(first)) => {
                            let cell = look_ahead(
                                &mut flow, key, first, pose.pos, nav, hpa, profile, &mut work,
                                &mut los,
                            );
                            drive_to = Some(DriveTo::Field(cell_centre(nav, cell), key));
                        }
                        Some(FlowDir::Arrived)
                            if line_clear(nav, profile, pose.pos, follow.goal, &mut los) =>
                        {
                            let goal = follow.goal;
                            follow.state = PathState::Following;
                            follow.corners.clear();
                            follow.corners.push(goal);
                            follow.next = 0;
                        }
                        Some(FlowDir::Arrived) | None => to_idle(&mut follow),
                    }
                }
                _ => to_idle(&mut follow),
            }
        }
        if follow.state == PathState::Following {
            let next = usize::from(follow.next);
            match follow.corners.get(next) {
                None => follow.state = PathState::Done,
                Some(&target) => {
                    let last = next + 1 >= follow.corners.len();
                    let dist = (target - pose.pos).length();
                    if last && dist > speed_at(nav, mobility, pose.pos) && dist <= arrive {
                        follow.state = PathState::Done;
                    } else {
                        drive_to = Some(DriveTo::Corner(target));
                    }
                }
            }
        } else if drive_to.is_none()
            && let Some(&Order::Rotate { facing }) = order.orders.first()
            && pose.heading != facing
        {
            pose.heading = pose.heading.rotate_towards(facing, mobility.turn_rate);
        }

        let push = pushes.get(sim.id());
        let step = drive_to.and_then(|to| {
            let target = match to {
                DriveTo::Corner(target) | DriveTo::Field(target, _) => target,
            };
            let speed = speed_at(nav, mobility, pose.pos);
            aim(&mut pose, mobility, target, speed, turn_in_place, &map)
        });
        let from = pose.pos;
        let step_delta = step.map_or(FxVec2::ZERO, |(to, _)| to - from);
        let on_field = match drive_to {
            Some(DriveTo::Field(_, key)) => Some(key),
            _ => None,
        };
        let (taken, to) = take_step(from, step_delta, push, &map, |to| {
            let (Some(nav), Some(profile)) = (nav, profile) else {
                return true;
            };
            if !step_clear(nav, profile, from, to, &mut los) {
                return false;
            }
            match (on_field, hpa) {
                (Some(key), Some(hpa)) => {
                    let end = nav.cell_of(to);
                    end == nav.cell_of(from)
                        || flow.direction(key, end, nav, hpa, &mut work).is_some()
                }
                _ => true,
            }
        });
        if to != pose.pos {
            pose.pos = to;
        }
        if taken == Taken::Stayed && (step_delta != FxVec2::ZERO || push != FxVec2::ZERO) {
            blocked += 1;
            if step_delta != FxVec2::ZERO
                && tick.0.saturating_sub(follow.planned.0) >= BLOCKED_REPLAN_TICKS
            {
                to_idle(&mut follow);
                continue;
            }
        }
        // The drive reached its point when its own step was taken, or when it already stood
        // there (no step to take, whatever became of its push).
        let reached = step.is_some_and(|(_, reached)| reached)
            && (matches!(taken, Taken::Whole | Taken::Unpushed) || step_delta == FxVec2::ZERO);
        if reached && matches!(drive_to, Some(DriveTo::Corner(_))) {
            follow.next = follow.next.saturating_add(1);
            if usize::from(follow.next) >= follow.corners.len() {
                follow.state = PathState::Done;
            }
        }
    }
    for (name, n) in [
        ("move.blocked", blocked),
        ("move.los_cells", u64::from(los)),
    ] {
        if n > 0 {
            counters.add(name, n);
        }
    }
    count_flow_work(&mut counters, work);
}

/// The `"Movement"` and `"Steering"` rules, the `"debug.wander"` stream, the order queue and
/// wandering (chained) in [`ThinkSet::Orders`], separation steering
/// ([`steer::build`](crate::steer)) and the flow-tile warm-up then movement (chained) in
/// [`ActSet::Move`].
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.register_rules_section::<MovementRules>("Movement")
        .register_rules_section::<SteeringRules>("Steering");
    crate::steer::build(sim);
    let wander = sim.register_stream("debug.wander");
    sim.add_systems(
        ThinkSet::Orders,
        (advance_orders, wander_system.with_input(wander)).chain(),
    )
    .add_systems(ActSet::Move, (prepare_flow_tiles, move_system).chain());
}
