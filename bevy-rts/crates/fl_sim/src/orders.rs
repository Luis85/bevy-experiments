//! The order queue (spec §7; R69): each unit's [`MoveOrder`] holds its [`Order`]s, the first
//! being the current one. [`advance_orders`] moves every unit on to its next order once the
//! current one is over, and asks for a path ([`enqueue_path_in`]) whenever a current move has
//! none or a follower needs a new one. Orders come from the core commands
//! ([`commands`](crate::commands): `core.move`, `core.rotate`, `core.follow`, each replacing
//! the orders or, Shift-queued, appended to them) and from wandering
//! ([`wander_system`](crate::movement)), which runs right after it. A group move spreads its
//! units over [`formation_targets`], kept apart by [`space_slots`].
//!
//! Every path request [`advance_orders`] makes is processed in the same tick:
//! [`process_path_queue`](crate::path) runs later in the tick, in the core slot of
//! [`ThinkSet::Behaviour`](crate::ThinkSet::Behaviour) (see [`path`](crate::path)).
//!
//! Journal `"core.order_changed"` (replacing M0's `"core.move_order"`): every change to a
//! unit's orders by a command, a completed or failed order, a stop, and a path a terrain
//! change invalidated.

use std::collections::BTreeMap;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryState;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use fl_math::{Fx, FxVec2};
use fl_protocol::{PlayerId, SimId, Tick};
use serde::Serialize;

use crate::build::Content;
use crate::builder::SimBuilder;
use crate::classify::Class;
use crate::clock::SimClock;
use crate::ids::{SimEntity, SimIndex};
use crate::map::MapInfo;
use crate::movement::MovementRules;
use crate::nav::CellXY;
use crate::observe::{Journal, JournalRecord};
use crate::path::{FlowKey, PathFollow, PathState, enqueue_path_in};
use crate::schedule::Phase;
use crate::units::{MoveOrder, Order, Pose, Wander};

/// Scratch buffers of [`advance_orders`], [`wander_system`](crate::movement) and
/// [`prepare_flow_tiles`](crate::movement), kept between ticks only for their capacity, so the
/// order queue and movement allocate nothing per tick in the steady state. Transient
/// `"core.order_scratch"`, emptied (capacity kept) at the start of every tick; each system also
/// empties its buffer after use. Never read across ticks.
#[derive(Resource, Default, Debug)]
pub(crate) struct OrderScratch {
    /// The units [`advance_orders`] looks at this tick, in `SimId` order.
    pub(crate) due: Vec<(SimId, Entity)>,
    /// The idle wanderers, in `SimId` order.
    pub(crate) wanderers: Vec<(SimId, Entity, Wander)>,
    /// The units on a flow field and the cell each stands in, in `SimId` order
    /// ([`prepare_flow_tiles`](crate::movement)).
    pub(crate) flow: Vec<(SimId, FlowKey, CellXY)>,
}

impl OrderScratch {
    /// Empty every buffer, keeping its capacity: the Transient reset.
    fn clear(&mut self) {
        self.due.clear();
        self.wanderers.clear();
        self.flow.clear();
    }
}

/// The units [`advance_orders`] walks; the state lives in the system (a cache of matching
/// archetypes, never sim state), so it is not rebuilt every tick.
pub(crate) type OrdersQuery = QueryState<(
    Entity,
    &'static SimEntity,
    &'static MoveOrder,
    &'static PathFollow,
)>;

/// Why a unit's orders changed, as `"core.order_changed"` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    /// A command set or extended the orders.
    Command,
    /// The current order is done: arrived (at the goal or the nearest reachable point),
    /// facing, or its leader is gone.
    Completed,
    /// The current move has no path at all.
    Failed,
    /// A stop command cleared the orders.
    Stopped,
    /// A terrain change invalidated the current path, or a hot reload changed the unit's
    /// movement profile; a new path is requested.
    Replanned,
}

impl Reason {
    fn name(self) -> &'static str {
        match self {
            Reason::Command => "command",
            Reason::Completed => "completed",
            Reason::Failed => "failed",
            Reason::Stopped => "stopped",
            Reason::Replanned => "replanned",
        }
    }
}

/// Journal `"core.order_changed"`: the unit now has `orders` orders, the current one of kind
/// `current` (`"move"`, `"rotate"`, `"follow"` or `"none"`), because of `reason`.
#[derive(Debug, Serialize)]
struct OrderChanged {
    orders: usize,
    current: &'static str,
    reason: &'static str,
}

impl JournalRecord for OrderChanged {
    const KIND: &'static str = "core.order_changed";
}

/// Journal `"core.order_changed"` for unit `id` (its [`MoveOrder`] as it is now), in `phase`
/// of `tick`, on behalf of `player` (`None` for the sim's own changes).
pub(crate) fn journal_order_changed(
    world: &mut World,
    tick: Tick,
    phase: Phase,
    id: SimId,
    player: Option<PlayerId>,
    reason: Reason,
) {
    if !world.resource::<Journal>().enabled() {
        return;
    }
    let (orders, current) = world
        .resource::<SimIndex>()
        .get(id)
        .and_then(|entity| world.get::<MoveOrder>(entity))
        .map_or((0, "none"), |order| {
            (
                order.orders.len(),
                order.orders.first().map_or("none", Order::kind_name),
            )
        });
    world.resource_mut::<Journal>().record(
        tick,
        phase,
        Some(id),
        player,
        &OrderChanged {
            orders,
            current,
            reason: reason.name(),
        },
    );
}

/// Whether `order`'s unit needs [`advance_orders`] this tick: its current move has no path
/// or has ended, it has a rotate or follow order, or it has no order but an ended path.
fn needs_attention(order: &MoveOrder, follow: &PathFollow) -> bool {
    match order.orders.first() {
        None => matches!(follow.state, PathState::Done | PathState::Failed),
        Some(Order::Move { .. }) => matches!(
            follow.state,
            PathState::Idle | PathState::Done | PathState::Failed
        ),
        Some(Order::Rotate { .. } | Order::Follow { .. }) => true,
    }
}

/// Set `entity`'s path back to `Idle` with no corners; a request it waited on turns stale.
pub(crate) fn idle(world: &mut World, entity: Entity) {
    if let Some(mut follow) = world.get_mut::<PathFollow>(entity)
        && (follow.state != PathState::Idle || !follow.corners.is_empty())
    {
        follow.state = PathState::Idle;
        follow.corners.clear();
        follow.next = 0;
    }
}

/// Drop `entity`'s current order, reset its path to `Idle` and journal `reason`.
fn pop(world: &mut World, tick: Tick, id: SimId, entity: Entity, reason: Reason) {
    if let Some(mut order) = world.get_mut::<MoveOrder>(entity)
        && !order.orders.is_empty()
    {
        order.orders.remove(0);
    }
    idle(world, entity);
    journal_order_changed(world, tick, Phase::Think, id, None, reason);
}

/// Advance every unit's orders, walking [`SimIndex`] in order. Per unit, until its current
/// order needs nothing more this tick:
/// - a current move whose path is `Done` pops (`"completed"`), one whose path `Failed` pops
///   (`"failed"`); a current move with an `Idle` path (a new or queued move becoming current,
///   a path blocked or invalidated, a loaded unit) gets a path request;
/// - a current rotate pops (`"completed"`) once the unit faces its direction; until then the
///   path stays `Idle` and the movement system turns the unit;
/// - a current follow pops (`"completed"`) when its leader is gone, and otherwise asks for a
///   path to its goal, the leader's position plus the offset (clamped to the map), as
///   [`follow_needs_path`] says;
/// - with no order left, an ended path goes back to `Idle`.
///
/// Exclusive, first in [`ThinkSet::Orders`](crate::ThinkSet::Orders). Its query state is cached
/// in the system and its scratch list is [`OrderScratch`], so it allocates nothing per tick.
pub(crate) fn advance_orders(world: &mut World, query: &mut OrdersQuery) {
    let tick = world.resource::<SimClock>().tick;
    let mut due = std::mem::take(&mut world.resource_mut::<OrderScratch>().due);
    due.extend(
        query
            .iter(world)
            .filter(|(_, _, order, follow)| needs_attention(order, follow))
            .map(|(entity, sim, _, _)| (sim.id(), entity)),
    );
    due.sort_unstable_by_key(|&(id, _)| id);
    for &(id, entity) in &due {
        advance_unit(world, tick, id, entity);
    }
    due.clear();
    world.resource_mut::<OrderScratch>().due = due;
}

/// [`advance_orders`] for one unit.
fn advance_unit(world: &mut World, tick: Tick, id: SimId, entity: Entity) {
    loop {
        let Some(order) = world.get::<MoveOrder>(entity) else {
            return;
        };
        let current = order.orders.first().cloned();
        let Some(state) = world.get::<PathFollow>(entity).map(|f| f.state) else {
            return;
        };
        match current {
            None => {
                if matches!(state, PathState::Done | PathState::Failed) {
                    idle(world, entity);
                }
                return;
            }
            Some(Order::Move { target }) => match state {
                PathState::Done => pop(world, tick, id, entity, Reason::Completed),
                PathState::Failed => pop(world, tick, id, entity, Reason::Failed),
                PathState::Idle => {
                    enqueue_path_in(world, id, target, Phase::Think);
                    return;
                }
                _ => return,
            },
            Some(Order::Rotate { facing }) => {
                if world
                    .get::<Pose>(entity)
                    .is_none_or(|pose| pose.heading == facing)
                {
                    pop(world, tick, id, entity, Reason::Completed);
                } else {
                    idle(world, entity);
                    return;
                }
            }
            Some(Order::Follow { leader, offset }) => {
                let at = world
                    .resource::<SimIndex>()
                    .get(leader)
                    .and_then(|leader| world.get::<Pose>(leader))
                    .map(|pose| pose.pos);
                let Some(at) = at else {
                    pop(world, tick, id, entity, Reason::Completed);
                    continue;
                };
                let goal = follow_goal(world, at, offset);
                if follow_needs_path(world, tick, entity, state, goal) {
                    enqueue_path_in(world, id, goal, Phase::Think);
                }
                return;
            }
        }
    }
}

/// Whether a follower whose path is in `state` asks for a new path to `goal` this tick
/// (Task 8 review Minor 7: an ended path re-plans, on a cadence, never every tick):
/// - `Idle` (a new or newly current order, a blocked step, a path a terrain change
///   invalidated): at once;
/// - `Waiting` for a request: never;
/// - otherwise only once `Movement.follow_repath_ticks` have passed since the current plan
///   was applied, and then: `Following` (or on a flow field) when the goal has drifted more
///   than one map cell from the planned one; `Done` when it has drifted more than
///   `max(Movement.arrive_m, cell / 2)`, so a follower that stopped closes a gap it can see,
///   but separation nudging an idle leader by less than half a cell re-plans nothing (Task 9
///   review Minor 5: the hysteresis between arriving, within `arrive_m`, and setting off
///   again); `Failed` always, since the terrain may have opened a way.
///
/// So a follower asks for at most one path per `follow_repath_ticks`, plus one per `Idle`.
fn follow_needs_path(
    world: &World,
    tick: Tick,
    entity: Entity,
    state: PathState,
    goal: FxVec2,
) -> bool {
    match state {
        PathState::Idle => return true,
        PathState::Waiting { .. } => return false,
        PathState::Following | PathState::Flow { .. } | PathState::Done | PathState::Failed => {}
    }
    let Some(follow) = world.get::<PathFollow>(entity) else {
        return false;
    };
    let rules = world.resource::<Content>().0.rules::<MovementRules>();
    if tick.0.saturating_sub(follow.planned.0) < rules.follow_repath_ticks {
        return false;
    }
    let drift = (goal - follow.goal).length();
    let cell = world
        .get_resource::<MapInfo>()
        .map_or(Fx::ONE, |map| map.cell);
    match state {
        PathState::Done => drift > rules.arrive_m.max(cell / Fx::from_int(2)),
        PathState::Failed => true,
        _ => drift > cell,
    }
}

/// The slots of a group move to `target` for units at `positions`, in the same order: each
/// unit keeps its offset from the group's centroid, scaled down so the farthest is at most
/// `max_spread` metres from `target`, and every slot is clamped to `map`.
///
/// The centroid is `c = Σ pos / n`, each offset `pos_i − c`, and `R` the largest offset's
/// length; if `R > max_spread` every offset is multiplied by `max_spread / R`. Slot `i` is
/// `map.clamp(target + offset_i)`. One unit gets `target` (clamped); none gets none.
pub fn formation_targets(
    positions: &[FxVec2],
    target: FxVec2,
    max_spread: Fx,
    map: &MapInfo,
) -> Vec<FxVec2> {
    let Ok(count) = i32::try_from(positions.len()) else {
        return vec![map.clamp(target); positions.len()];
    };
    if count == 0 {
        return Vec::new();
    }
    let sum = positions.iter().fold(FxVec2::ZERO, |sum, &p| sum + p);
    let centre = sum / Fx::from_int(count);
    let radius = positions
        .iter()
        .map(|&p| (p - centre).length())
        .max()
        .unwrap_or(Fx::ZERO);
    positions
        .iter()
        .map(|&p| {
            let offset = p - centre;
            let offset = if radius > max_spread {
                offset * max_spread / radius
            } else {
                offset
            };
            map.clamp(target + offset)
        })
        .collect()
}

/// The largest pitch [`space_slots`] honours, in metres: the largest map side `fl_content`
/// accepts. Every slot lies on the map, so squared distances stay far inside `Fx`.
const MAX_PITCH_M: i32 = 4096;

/// `slots` (in unit order) moved apart so that no two are closer than `pitch` metres (Task 9
/// review Minor 2: a large or widely spread group scaled into `formation_max_spread_m` would
/// otherwise crowd many units onto nearly the same slot). Deterministic, in slot order:
///
/// 1. A slot at least `pitch` from every slot already placed keeps its place.
/// 2. Otherwise the candidates are `slot + (dx, dy) × pitch`, clamped to `map`, ring by
///    Chebyshev ring `r = 1, 2, …` around it, and within a ring by `(dx² + dy², dy, dx)`; the
///    first one at least `pitch` from every placed slot is taken.
/// 3. If none is found within `⌊√n⌋ + 1` rings (`n` slots; only possible when the clamp to a
///    small map folds candidates together) the slot keeps its place.
///
/// Distances are compared squared, on exact `Fx`. A `pitch` of 0 or less changes nothing; a
/// pitch above 4096 m counts as 4096 m. So a group's slots spread further than
/// `formation_max_spread_m` when it is too dense to fit (about `√n × pitch` across).
pub fn space_slots(slots: &[FxVec2], pitch: Fx, map: &MapInfo) -> Vec<FxVec2> {
    if pitch <= Fx::ZERO || slots.len() < 2 {
        return slots.to_vec();
    }
    let pitch = pitch.min(Fx::from_int(MAX_PITCH_M));
    let min_sq = pitch * pitch;
    let step = pitch.raw();
    let bucket = |p: FxVec2| (p.x.raw().div_euclid(step), p.y.raw().div_euclid(step));
    // Placed slots by `pitch`-square bucket: a slot closer than `pitch` to `c` lies in one of
    // the 3 × 3 buckets around `c`'s.
    let mut placed: BTreeMap<(i64, i64), Vec<FxVec2>> = BTreeMap::new();
    let free = |placed: &BTreeMap<(i64, i64), Vec<FxVec2>>, c: FxVec2| {
        let (bx, by) = bucket(c);
        (-1..=1).all(|dy| {
            (-1..=1).all(|dx| {
                placed
                    .get(&(bx + dx, by + dy))
                    .is_none_or(|list| list.iter().all(|&p| (c - p).length_sq() >= min_sq))
            })
        })
    };
    let rings = i32::try_from(slots.len().isqrt() + 1).unwrap_or(i32::MAX);
    let ring_offsets: Vec<Vec<(i32, i32)>> = (1..=rings)
        .map(|r| {
            let mut ring: Vec<(i32, i32)> = (-r..=r)
                .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
                .filter(|&(dx, dy)| dx.abs() == r || dy.abs() == r)
                .collect();
            ring.sort_unstable_by_key(|&(dx, dy)| (dx * dx + dy * dy, dy, dx));
            ring
        })
        .collect();
    let mut out = Vec::with_capacity(slots.len());
    for &slot in slots {
        let mut chosen = slot;
        if !free(&placed, slot) {
            chosen = ring_offsets
                .iter()
                .flatten()
                .map(|&(dx, dy)| {
                    map.clamp(slot + FxVec2::new(pitch.mul_int(dx), pitch.mul_int(dy)))
                })
                .find(|&c| free(&placed, c))
                .unwrap_or(slot);
        }
        placed.entry(bucket(chosen)).or_default().push(chosen);
        out.push(chosen);
    }
    out
}

/// `leader + offset`, clamped to the map.
fn follow_goal(world: &World, leader: FxVec2, offset: FxVec2) -> FxVec2 {
    let goal = leader + offset;
    world
        .get_resource::<MapInfo>()
        .map_or(goal, |map| map.clamp(goal))
}

/// The `"core.order_changed"` record and the Transient [`OrderScratch`].
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.app.init_resource::<OrderScratch>();
    sim.register_journal_record::<OrderChanged>()
        .register_resource::<OrderScratch>(
            "core.order_scratch",
            Class::TransientReset(OrderScratch::clear),
        );
}
