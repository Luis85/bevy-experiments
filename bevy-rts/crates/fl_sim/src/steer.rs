//! Separation steering (spec §7, R76): units whose footprints come closer than their pair
//! distance push apart. [`compute_steering`] works out each steered unit's push for this tick
//! from the positions at the start of the tick into the Transient [`SteerPushes`] resource;
//! [`move_system`](crate::movement) adds the push to the unit's step and never lets it enter a
//! cell the unit's profile cannot pass.
//!
//! **No per-tick Transient component (R76).** Housekeeping removes every Transient component
//! from every entity each tick (R72), an archetype move per unit; the pushes are one
//! `(SimId, push)` entry per steered unit in one resource, sorted by `SimId`, emptied (capacity
//! kept) at the start of every tick.
//!
//! Counters: `"steer.units"` (units steered) and `"steer.pairs"` (neighbour candidates the
//! spatial index returned, each unit itself left out: the work).

use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Query, Res, ResMut};
use fl_math::{Fx, FxVec2};
use fl_protocol::SimId;

use crate::build::Content;
use crate::builder::SimBuilder;
use crate::classify::Class;
use crate::clock::SimClock;
use crate::counters::SimCounters;
use crate::ids::SimEntity;
use crate::movement::SteeringRules;
use crate::path::{PathFollow, PathState, process_path_queue};
use crate::schedule::ThinkSet;
use crate::spatial::SpatialGrid;
use crate::units::{Footprint, Mobility, Pose};

/// This tick's separation push of every steered unit, in metres: one `(SimId, push)` entry per
/// steered unit, in `SimId` order. Written only by
/// [`compute_steering`] (in [`ThinkSet::Behaviour`]), read by movement in the same tick.
/// Transient `"core.steer"` (`Class::TransientReset(SteerPushes::clear)`): emptied at the start
/// of every tick by Housekeeping, its capacity kept, so steering allocates nothing per tick in
/// the steady state.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct SteerPushes {
    entries: Vec<(SimId, FxVec2)>,
}

impl SteerPushes {
    /// Remove every entry, keeping the capacity: the Transient reset.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// This tick's push of unit `id`, in metres; [`FxVec2::ZERO`] for a unit not steered this
    /// tick. A binary search.
    pub fn get(&self, id: SimId) -> FxVec2 {
        self.entries
            .binary_search_by_key(&id, |&(id, _)| id)
            .map_or(FxVec2::ZERO, |at| self.entries[at].1)
    }

    /// Every entry, ascending by `SimId`.
    pub fn entries(&self) -> &[(SimId, FxVec2)] {
        &self.entries
    }

    /// How many units were steered this tick.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no unit was steered this tick.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A unit [`compute_steering`] steers this tick: its id, position, footprint radius and the
/// largest push it may get.
#[derive(Clone, Copy, Debug)]
struct Steered {
    id: SimId,
    pos: FxVec2,
    radius: Fx,
    cap: Fx,
}

/// Scratch lists of [`compute_steering`], kept between ticks only for their capacity, so
/// steering allocates nothing per tick in the steady state. Transient `"core.steer_scratch"`,
/// emptied (capacity kept) at the start of every tick; [`compute_steering`] also empties it
/// after use. Never read across ticks.
#[derive(Resource, Default, Debug)]
pub(crate) struct SteerScratch {
    /// Every unit with a footprint, `(id, position, radius)`, sorted by `SimId`.
    bodies: Vec<(SimId, FxVec2, Fx)>,
    /// The units steered this tick, sorted by `SimId`.
    steered: Vec<Steered>,
    /// One steered unit's neighbour candidates.
    near: Vec<SimId>,
}

impl SteerScratch {
    /// Empty every list, keeping its capacity: the Transient reset.
    fn clear(&mut self) {
        self.bodies.clear();
        self.steered.clear();
        self.near.clear();
    }
}

/// Below this distance (1/1024 m) two units count as coincident, so the push direction never
/// divides by a vanishing distance.
const COINCIDENT_M: Fx = Fx::from_ratio(1, 1024);

/// The radius steering gives a footprint: `max(width, length) / 2`.
fn radius(footprint: &Footprint) -> Fx {
    footprint.width.max(footprint.length) / Fx::from_int(2)
}

/// The units [`compute_steering`] reads: every unit with a [`Pose`] and a [`Footprint`] is a
/// neighbour; one that also has [`Mobility`] may be steered.
type SteerQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static SimEntity,
        &'static Pose,
        &'static Footprint,
        Option<&'static Mobility>,
        Option<&'static PathFollow>,
    ),
>;

/// Work out this tick's separation pushes into [`SteerPushes`] (spec §7, `"Steering"` rules):
///
/// - **Who is steered:** in `SimId` order, every unit with [`Mobility`] and a [`Footprint`]
///   that is moving (its path `Following` or on a flow field), and every other such unit
///   whose `sim_id % idle_every == tick % idle_every` (idle steering is staggered).
/// - **Neighbours:** [`SpatialGrid::query_radius`] around the unit, wide enough for the
///   largest footprint present; every other id with a [`Pose`] and a [`Footprint`], in `SimId`
///   order. With `radius(u) = max(width, length) / 2`, a pair's distance is `r = radius(u) +
///   radius(v) + separation_margin_m`.
/// - **Push:** the sum over neighbours `v` closer than `r` (`d < r`) of `(u − v) / d × (r − d)
///   × separation_gain`. A coincident pair (`d` below 1/1024 m) pushes `r × separation_gain`
///   along +x for the higher `SimId` and −x for the lower. Each unit computes its own terms
///   from its own side, and `Fx` multiplication floors, so a pair's two terms are opposite
///   only to within one raw unit (`(−x)·s` and `−(x·s)` may differ by 1 ulp): deterministic,
///   not exactly symmetric (Task 11 review Minor 3). The push is capped at
///   `max_push_frac × speed` (road speed, metres per tick) per tick it stands for: a moving
///   unit's covers one tick, an idle unit's the `idle_every` ticks until it is steered again.
///
/// Positions are those at the start of the tick (nothing moves before
/// [`ActSet::Move`](crate::ActSet::Move)), so every push is order-free. The bodies (id,
/// position, radius) are gathered in query order and sorted by `SimId`, and neighbours are
/// looked up in them by binary search, so nothing depends on `Entity` order. Without a spatial
/// grid (no map) nothing is steered. The spatial grid is refiled in `Perceive`, so a unit
/// moved outside movement earlier in the tick (a `debug.teleport` in `Input`) is found by its
/// new neighbours' queries only from the next tick: deterministic, and debug-only. Its scratch
/// lists live in the Transient [`SteerScratch`], so it allocates nothing per tick in the
/// steady state. Core slot of
/// [`ThinkSet::Behaviour`], after [`process_path_queue`]; the only writer of [`SteerPushes`].
pub(crate) fn compute_steering(
    content: Res<Content>,
    clock: Res<SimClock>,
    grid: Option<Res<SpatialGrid>>,
    mut pushes: ResMut<SteerPushes>,
    mut scratch: ResMut<SteerScratch>,
    mut counters: ResMut<SimCounters>,
    units: SteerQuery,
) {
    let Some(grid) = grid else {
        return;
    };
    let rules = *content.0.rules::<SteeringRules>();
    let every = rules.idle_every.max(1);
    let phase = clock.tick.0 % every;
    let idle_ticks = i32::try_from(every).unwrap_or(i32::MAX);
    let scratch = &mut *scratch;
    let (bodies, steered, near) = (&mut scratch.bodies, &mut scratch.steered, &mut scratch.near);
    let mut widest = Fx::ZERO;
    for (sim, pose, footprint, mobility, follow) in &units {
        let (id, radius) = (sim.id(), radius(footprint));
        widest = widest.max(radius);
        bodies.push((id, pose.pos, radius));
        let Some(mobility) = mobility else {
            continue;
        };
        let moving = follow
            .is_some_and(|f| matches!(f.state, PathState::Following | PathState::Flow { .. }));
        if !moving && id.0 % every != phase {
            continue;
        }
        let ticks = if moving { 1 } else { idle_ticks };
        steered.push(Steered {
            id,
            pos: pose.pos,
            radius,
            cap: (rules.max_push_frac * mobility.speed).mul_int(ticks),
        });
    }
    bodies.sort_unstable_by_key(|&(id, _, _)| id);
    steered.sort_unstable_by_key(|u| u.id);
    let mut pairs: u64 = 0;
    for u in steered.iter() {
        let reach = u.radius + widest + rules.separation_margin_m;
        grid.query_radius(u.pos, reach, near);
        let mut push = FxVec2::ZERO;
        for &v in near.iter() {
            if v == u.id {
                continue;
            }
            pairs += 1;
            let Ok(at) = bodies.binary_search_by_key(&v, |&(id, _, _)| id) else {
                continue;
            };
            let (_, pv, rv) = bodies[at];
            let r = u.radius + rv + rules.separation_margin_m;
            let diff = u.pos - pv;
            if diff.length_sq() >= r * r {
                continue;
            }
            let d = diff.length();
            let part = if d < COINCIDENT_M {
                let along = r * rules.separation_gain;
                FxVec2::new(if u.id > v { along } else { -along }, Fx::ZERO)
            } else {
                diff * ((r - d) * rules.separation_gain / d)
            };
            push = push + part;
        }
        if push.length() > u.cap {
            push = push.normalize_or_zero() * u.cap;
        }
        pushes.entries.push((u.id, push));
    }
    counters.add("steer.units", steered.len() as u64);
    counters.add("steer.pairs", pairs);
    bodies.clear();
    steered.clear();
    near.clear();
}

/// The Transient [`SteerPushes`] and [`SteerScratch`], and [`compute_steering`] in the core
/// slot of [`ThinkSet::Behaviour`] after [`process_path_queue`].
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.app
        .init_resource::<SteerPushes>()
        .init_resource::<SteerScratch>();
    sim.register_resource::<SteerPushes>("core.steer", Class::TransientReset(SteerPushes::clear))
        .register_resource::<SteerScratch>(
            "core.steer_scratch",
            Class::TransientReset(SteerScratch::clear),
        )
        .add_systems(
            ThinkSet::Behaviour,
            compute_steering.after(process_path_queue),
        );
}
