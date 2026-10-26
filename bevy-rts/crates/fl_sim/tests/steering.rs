//! Separation steering and the movement step (spec §7, Task 11): overlapping units push
//! apart, by at most `Steering.max_push_frac` of their speed per tick; idle units steer only on
//! their staggered tick; the pushes live in the Transient [`SteerPushes`] resource (R76: no
//! per-tick Transient component, so no archetype churn); a step never enters a cell the unit's
//! profile cannot pass, whether by a push, a diagonal corner cut or a hop over a thin wall (the
//! swept cells are checked, Task 5 review); a blocked step re-plans with a deterministic
//! backoff (Task 8 review Minor 8); a stopped follower ignores small nudges of its leader
//! (Task 9 review Minor 5); and flow-field units look ahead along the field and are never
//! pushed off it (R111).

use std::sync::Arc;

use bevy_ecs::world::Mut;
use fl_content::ContentDb;
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    DebugSetTerrainCmd, DebugTeleportCmd, FollowCmd, MoveCmd, PlayerId, RotateCmd, SimId, StopCmd,
};
use fl_sim::nav::{CellXY, FlowCache, FlowWork, HpaGraphs, NavGrid, line_clear, profile_of};
use fl_sim::{
    CellRect, Footprint, Mobility, MovementRules, PathFollow, PathState, SimCounters, SimFeature,
    SimIndex, SpawnRequest, SteerPushes, SteeringRules, TerrainGrid, rebuild_spatial, setup_map,
    setup_spawn,
};
use fl_test_utils::{
    Scenario, ScenarioBuilder, ascii_terrain_content, assert_branch, assert_deterministic,
    assert_shuffle_invariant,
};
use fl_view::JournalEntry;
use proptest::prelude::*;
use proptest::test_runner::TestRunner;

/// Cell side of every map here, in metres.
const CELL_M: i32 = 4;

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

/// The ASCII rows of a `w × h` map whose cell `(x, y)` is `f(x, y)` (row 0 is the north
/// edge, `y = h − 1`).
fn draw(w: u32, h: u32, f: impl Fn(u32, u32) -> char) -> Vec<String> {
    (0..h)
        .rev()
        .map(|y| (0..w).map(|x| f(x, y)).collect())
        .collect()
}

/// The centre of cell `c`, in metres.
fn at(c: CellXY) -> FxVec2 {
    let m = |i: u32| Fx::from_int(i32::try_from(i).expect("a small map") * CELL_M + CELL_M / 2);
    FxVec2::new(m(c.0), m(c.1))
}

/// `(x, y)` metres of the centre of cell `c`, for [`ScenarioBuilder::unit`].
fn metres(c: CellXY) -> (i32, i32) {
    let m = |i: u32| i32::try_from(i).expect("a small map") * CELL_M + CELL_M / 2;
    (m(c.0), m(c.1))
}

/// Player 0 (`t.red`) on the ASCII map `rows` with `(unit, x, y)` units (metres), in order
/// (#1 first).
fn on_map(rows: &[String], units: &[(&str, i32, i32)]) -> ScenarioBuilder {
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut b = ScenarioBuilder::new()
        .ascii_map(&rows, CELL_M)
        .player("t.red", 0);
    for &(unit, x, y) in units {
        b = b.unit(unit, 0, x, y);
    }
    b
}

fn entity(s: &Scenario, id: u32) -> bevy_ecs::entity::Entity {
    s.world()
        .resource::<SimIndex>()
        .get(SimId(id))
        .unwrap_or_else(|| panic!("#{id} is alive"))
}

fn pos(s: &Scenario, id: u32) -> FxVec2 {
    s.pose(id).expect("a unit with a pose").pos
}

fn follow(s: &Scenario, id: u32) -> PathFollow {
    s.world()
        .get::<PathFollow>(entity(s, id))
        .cloned()
        .expect("a mobile unit has a PathFollow")
}

fn counter(s: &Scenario, name: &str) -> u64 {
    s.world().resource::<SimCounters>().get(name)
}

fn steering(s: &Scenario) -> SteeringRules {
    *s.world()
        .resource::<fl_sim::Content>()
        .0
        .rules::<SteeringRules>()
}

fn movement(s: &Scenario) -> MovementRules {
    s.world()
        .resource::<fl_sim::Content>()
        .0
        .rules::<MovementRules>()
        .clone()
}

fn entries<'a>(s: &'a Scenario, kind: &str) -> Vec<&'a JournalEntry> {
    s.journal().iter().filter(|e| e.kind == kind).collect()
}

fn requests_of(s: &Scenario, id: u32) -> usize {
    entries(s, "core.path_request")
        .into_iter()
        .filter(|e| e.sim_id == Some(SimId(id)))
        .count()
}

/// The pair distance of two units: both footprint radii (`max(width, length) / 2`) plus
/// `Steering.separation_margin_m`.
fn pair_distance(s: &Scenario, a: u32, b: u32) -> Fx {
    let radius = |id: u32| {
        let f = s
            .world()
            .get::<Footprint>(entity(s, id))
            .expect("a footprint");
        f.width.max(f.length) / Fx::from_int(2)
    };
    radius(a) + radius(b) + steering(s).separation_margin_m
}

/// Every unit with `Mobility` whose cell its profile cannot pass, as `"#id at (x, y)"`.
fn on_blocked_cells(s: &Scenario) -> Vec<String> {
    let world = s.world();
    let nav = world.resource::<NavGrid>();
    world
        .resource::<SimIndex>()
        .iter()
        .filter_map(|(id, e)| {
            let mobility = world.get::<Mobility>(e)?;
            let profile = profile_of(
                mobility,
                world.get::<Footprint>(e),
                nav.cell_m(),
                nav.max_clearance(),
            );
            let p = world.get::<fl_sim::Pose>(e)?.pos;
            let (x, y) = nav.cell_of(p);
            (!nav.passable(profile, i64::from(x), i64::from(y)))
                .then(|| format!("#{} at ({x}, {y})", id.0))
        })
        .collect()
}

/// Every unit with `Mobility` and its position, in `SimId` order.
fn mobile_positions(s: &Scenario) -> Vec<(SimId, FxVec2)> {
    let world = s.world();
    world
        .resource::<SimIndex>()
        .iter()
        .filter(|&(_, e)| world.get::<Mobility>(e).is_some())
        .filter_map(|(id, e)| Some((id, world.get::<fl_sim::Pose>(e)?.pos)))
        .collect()
}

/// Every unit with `Mobility` that started the tick (at `before`) on a cell its profile can
/// pass and whose move this tick (`before` → now) touched a cell it cannot pass
/// ([`line_clear`]: a corner cut or a hop, by a drive step or a push), as `"#id from → to"`.
fn swept_blocked_cells(s: &Scenario, before: &[(SimId, FxVec2)]) -> Vec<String> {
    let world = s.world();
    let nav = world.resource::<NavGrid>();
    let index = world.resource::<SimIndex>();
    before
        .iter()
        .filter_map(|&(id, from)| {
            let e = index.get(id)?;
            let mobility = world.get::<Mobility>(e)?;
            let profile = profile_of(
                mobility,
                world.get::<Footprint>(e),
                nav.cell_m(),
                nav.max_clearance(),
            );
            let to = world.get::<fl_sim::Pose>(e)?.pos;
            let (x, y) = nav.cell_of(from);
            let started_clear = nav.passable(profile, i64::from(x), i64::from(y));
            (started_clear && !line_clear(nav, profile, from, to, &mut 0))
                .then(|| format!("#{} {from:?} → {to:?}", id.0))
        })
        .collect()
}

/// Run `s` for `ticks` ticks, panicking at the first tick a unit stands on a cell its
/// profile cannot pass or its move swept one.
fn run_on_passable_cells(s: &mut Scenario, ticks: u32) {
    for _ in 0..ticks {
        let before = mobile_positions(s);
        s.run_ticks(1);
        let tick = s.tick().0 - 1;
        let bad = on_blocked_cells(s);
        assert!(bad.is_empty(), "tick {tick}: {bad:?}");
        let swept = swept_blocked_cells(s, &before);
        assert!(swept.is_empty(), "tick {tick}: swept {swept:?}");
    }
}

// --- separation -----------------------------------------------------------------------------

#[test]
fn overlapping_idle_units_separate() {
    let rows = draw(16, 16, |_, _| '.');
    let mut s = on_map(&rows, &[("t.tank", 30, 30), ("t.tank", 30, 30)]).build();
    let r = pair_distance(&s, 1, 2);
    s.run_ticks(40);
    let d = (pos(&s, 1) - pos(&s, 2)).length();
    assert!(
        d >= r - Fx::from_ratio(1, 10),
        "after 40 ticks the tanks are {d} m apart, want at least {r} − 0.1"
    );
    // A coincident pair splits along x: the lower id to −x, the higher to +x.
    assert!(pos(&s, 1).x < pos(&s, 2).x);
    assert_eq!(pos(&s, 1).y, pos(&s, 2).y);
}

#[test]
fn separation_never_pushes_into_water() {
    // A one-cell strip of grass (y = 4) between water rows.
    let rows = draw(24, 9, |_, y| if y == 4 { '.' } else { '~' });
    let tanks: Vec<(&str, i32, i32)> = (0..10).map(|i| ("t.tank", 44 + i, 17 + i % 3)).collect();
    let mut s = on_map(&rows, &tanks).build();
    run_on_passable_cells(&mut s, 200);
    // They spread along the strip.
    let xs: Vec<Fx> = (1..=10).map(|id| pos(&s, id).x).collect();
    let spread = *xs.iter().max().expect("ten") - *xs.iter().min().expect("ten");
    assert!(
        spread > Fx::from_int(20),
        "the tanks spread {spread} m along the strip"
    );
}

#[test]
fn idle_steering_is_staggered() {
    let rows = draw(48, 48, |_, _| '.');
    // 100 idle tanks, 6 m apart on a 10 × 10 grid: closer than the pair distance (6.5 m).
    let tanks: Vec<(&str, i32, i32)> = (0..100)
        .map(|i| ("t.tank", 20 + 6 * (i % 10), 20 + 6 * (i / 10)))
        .collect();
    let mut s = on_map(&rows, &tanks).build();
    let every = u64::from(steering(&s).idle_every);
    let mut steered = 0;
    for _ in 0..5 * every {
        s.run_ticks(1);
        let n = counter(&s, "steer.units");
        assert!(
            n <= 100_u64.div_ceil(every),
            "{n} idle units steered in one tick"
        );
        steered += n;
    }
    assert_eq!(
        steered,
        5 * 100,
        "every idle unit steers once per idle_every ticks"
    );
}

#[test]
fn steer_pushes_are_per_tick_and_sorted() {
    let rows = draw(24, 24, |_, _| '.');
    let mut s = on_map(
        &rows,
        &[("t.tank", 40, 40), ("t.tank", 42, 40), ("t.tank", 60, 60)],
    )
    .build();
    s.issue(0, &[3], MoveCmd { target: v(80, 60) }, false);
    let archetypes = |s: &Scenario| -> Vec<_> {
        (1..=3)
            .map(|id| s.world().entity(entity(s, id)).archetype().id())
            .collect()
    };
    s.run_ticks(1);
    let before = archetypes(&s);
    let mut steered_some = false;
    for _ in 0..10 {
        s.run_ticks(1);
        let pushes = s.world().resource::<SteerPushes>();
        let ids: Vec<SimId> = pushes.entries().iter().map(|&(id, _)| id).collect();
        assert!(
            ids.windows(2).all(|w| w[0] < w[1]),
            "entries ascend: {ids:?}"
        );
        assert_eq!(pushes.len() as u64, counter(&s, "steer.units"));
        for &(id, push) in pushes.entries() {
            assert_eq!(pushes.get(id), push);
        }
        steered_some |= !pushes.is_empty();
        assert_eq!(
            archetypes(&s),
            before,
            "steering moves no unit between archetypes"
        );
    }
    assert!(steered_some);

    // Stop the mover: once no unit is moving, a tick whose stagger matches no idle unit
    // steers none, and the pushes of the tick before are gone (Housekeeping reset them).
    s.issue(0, &[3], StopCmd, false);
    s.run_ticks(2);
    let every = steering(&s).idle_every;
    let mut seen_empty = false;
    for _ in 0..every {
        s.run_ticks(1);
        let tick = s.tick().0 - 1;
        let pushes = s.world().resource::<SteerPushes>();
        if (1..=3).all(|id| id % every != tick % every) {
            assert!(pushes.is_empty(), "tick {tick} steers nothing");
            assert_eq!(pushes.get(SimId(1)), FxVec2::ZERO);
            seen_empty = true;
        }
    }
    assert!(seen_empty);
}

/// A crowd of tanks ordered through each other on an open map with a lake.
fn crowd() -> ScenarioBuilder {
    let rows = draw(32, 32, |x, y| {
        if (12..=18).contains(&x) && (10..=20).contains(&y) {
            '~'
        } else {
            '.'
        }
    });
    let mut tanks = Vec::new();
    for i in 0..12 {
        tanks.push(("t.tank", 20 + 3 * (i % 4), 20 + 3 * (i / 4)));
    }
    for i in 0..6 {
        tanks.push(("t.truck", 100 + 3 * (i % 3), 100 + 3 * (i / 3)));
    }
    let west: Vec<u32> = (1..=12).collect();
    let east: Vec<u32> = (13..=18).collect();
    on_map(&rows, &tanks)
        .command(
            0,
            0,
            &west,
            MoveCmd {
                target: v(104, 104),
            },
        )
        .command(0, 0, &east, MoveCmd { target: v(24, 24) })
        .command(150, 0, &[1, 2, 3], RotateCmd { toward: v(0, 0) })
}

#[test]
fn smoke_steering_twin_and_shuffle() {
    assert_deterministic(crowd, 300);
    assert_shuffle_invariant(crowd, 300);
}

#[test]
fn crowd_never_enters_blocked_cells() {
    let mut s = crowd().build();
    run_on_passable_cells(&mut s, 400);
}

/// Eight tanks stacked on two points of an open map, idle: separation alone moves them, on
/// their staggered ticks.
fn stacked_idle() -> ScenarioBuilder {
    let rows = draw(24, 24, |_, _| '.');
    let mut tanks = vec![("t.tank", 40, 40); 5];
    tanks.extend([("t.tank", 44, 42); 3]);
    on_map(&rows, &tanks)
}

/// A branch mid-steering equals the unbroken run (Task 11 review Minor 1): the pushes and the
/// stagger are a pure function of hashed state (positions, paths, the tick), so a save taken
/// at a tick that is not a multiple of `idle_every`, while overlapping idle units are still
/// pushing apart, loads to the same hashes.
#[test]
fn branch_mid_steering() {
    assert_branch(stacked_idle, 3, 120);
    assert_branch(crowd, 7, 300);
}

/// A mover pushes an idle unit in its way aside (R115: a push is capped below the mover's
/// speed, so an idle unit yields rather than blocking; yielding by choice is C7's).
#[test]
fn mover_pushes_an_idle_unit_aside() {
    let rows = draw(40, 20, |_, _| '.');
    // #2 idles 1.5 m north of #1's straight line to (140, 40).
    let mut s = on_map(&rows, &[("t.tank", 20, 40), ("t.tank", 80, 41)])
        .command(0, 0, &[1], MoveCmd { target: v(140, 40) })
        .build();
    let idle = pos(&s, 2);
    run_on_passable_cells(&mut s, 400);
    let moved = pos(&s, 2) - idle;
    assert!(
        moved.y > Fx::ONE,
        "the idle tank was pushed aside (north): moved {moved:?}"
    );
    assert!(follow(&s, 2).state == PathState::Idle, "it stayed idle");
    assert!(
        (pos(&s, 1) - v(140, 40)).length() <= movement(&s).arrive_m,
        "the mover got through to its goal: at {:?}",
        pos(&s, 1)
    );
}

// --- the swept step (Task 5 review) ----------------------------------------------------------

/// Put #`id` on a forced straight path to `goal`, as no planner would make it.
fn force_path(s: &mut Scenario, id: u32, goal: FxVec2) {
    let e = entity(s, id);
    let mut f = s.world_mut().get_mut::<PathFollow>(e).expect("mobile");
    f.state = PathState::Following;
    f.goal = goal;
    f.corners = vec![goal];
    f.next = 0;
}

#[test]
fn step_never_cuts_a_blocked_corner() {
    // (1, 0) and (0, 1) are water: the diagonal from (0, 0) to (1, 1) passes exactly through
    // their shared corner.
    let rows = draw(4, 4, |x, y| match (x, y) {
        (1, 0) | (0, 1) => '~',
        _ => '.',
    });
    let (x, y) = metres((0, 0));
    let mut s = on_map(&rows, &[("t.tank", x, y)]).build();
    // Face the corner first, so the tank drives as soon as the path is set.
    {
        let e = entity(&s, 1);
        let mut pose = s.world_mut().get_mut::<fl_sim::Pose>(e).expect("a pose");
        pose.heading = Angle::from_degrees(Fx::from_int(45));
    }
    force_path(&mut s, 1, at((1, 1)));
    for _ in 0..40 {
        s.run_ticks(1);
        let c = s.world().resource::<NavGrid>().cell_of(pos(&s, 1));
        assert_eq!(
            c,
            (0, 0),
            "tick {}: the tank cut the corner",
            s.tick().0 - 1
        );
    }
}

#[test]
fn fast_unit_never_hops_a_one_cell_wall() {
    // 1 m cells and a one-cell wall at x = 3; the rifleman (size 1) is made 2.5 m per tick,
    // so its first step from x = 2 would end at x = 4.5, past the wall.
    let rows = draw(10, 3, |x, _| if x == 3 { '~' } else { '.' });
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut s = ScenarioBuilder::new()
        .ascii_map(&rows, 1)
        .player("t.red", 0)
        .unit("t.rifleman", 0, 2, 1)
        .build();
    let e = entity(&s, 1);
    s.world_mut().get_mut::<Mobility>(e).expect("mobile").speed = Fx::from_ratio(5, 2);
    force_path(&mut s, 1, v(8, 1));
    for _ in 0..20 {
        s.run_ticks(1);
        assert!(
            pos(&s, 1).x < Fx::from_int(3),
            "tick {}: the rifleman crossed the wall to {:?}",
            s.tick().0 - 1,
            pos(&s, 1)
        );
    }
    // Blocked for `planned` + 10 ticks, the forced path is dropped: Idle, no corners.
    assert_eq!(follow(&s, 1).state, PathState::Idle);
}

// --- blocked steps re-plan with a backoff (Task 8 review Minor 8) ---------------------------

#[test]
fn blocked_step_replans_with_backoff() {
    // The tank stands at (4, 4); at tick 0 the 3 × 3 cells around it turn to water, so it is
    // stranded: every plan starts at a snapped cell two cells away, and every step toward it
    // ends in water and is blocked.
    let rows = draw(16, 9, |_, _| '.');
    let (x, y) = metres((4, 4));
    let mut s = on_map(&rows, &[("t.tank", x, y)])
        .command(
            0,
            0,
            &[],
            DebugSetTerrainCmd {
                min_cell: (3, 3),
                max_cell: (5, 5),
                terrain: "t.water".to_owned(),
            },
        )
        .command(
            1,
            0,
            &[1],
            MoveCmd {
                target: at((14, 4)),
            },
        )
        .build();
    let ticks = 110;
    let mut blocked = 0;
    for _ in 0..ticks {
        s.run_ticks(1);
        blocked += counter(&s, "move.blocked");
    }
    assert!(blocked > 50, "the tank was blocked {blocked} times");
    let requests = requests_of(&s, 1);
    assert!(
        requests <= ticks as usize / 10 + 1,
        "{requests} path requests in {ticks} ticks"
    );
    assert_eq!(s.world().resource::<NavGrid>().cell_of(pos(&s, 1)), (4, 4));
}

// --- follower hysteresis (Task 9 review Minor 5) --------------------------------------------

#[test]
fn stopped_follower_ignores_small_leader_nudges() {
    // #1 stands still; #2, 30 m south of it, follows it to 8 m and stops there; #3 is far
    // off.
    let rows = draw(32, 32, |_, _| '.');
    let mut s = on_map(
        &rows,
        &[("t.tank", 60, 60), ("t.tank", 60, 30), ("t.tank", 110, 110)],
    )
    .build();
    s.issue(0, &[2], FollowCmd { leader: SimId(1) }, false);
    s.run_ticks(200);
    assert_eq!(follow(&s, 2).state, PathState::Done);
    let settled = requests_of(&s, 2);

    // #3 is put 4 m east of the leader: separation nudges the leader about 1.25 m west, more
    // than `arrive_m` but less than half a cell.
    let leader = pos(&s, 1);
    s.issue(0, &[3], DebugTeleportCmd { pos: v(64, 60) }, false);
    s.run_ticks(100);
    let nudge = (pos(&s, 1) - leader).length();
    assert!(
        nudge > movement(&s).arrive_m && nudge < Fx::from_int(CELL_M / 2),
        "the leader moved {nudge} m"
    );
    assert_eq!(
        requests_of(&s, 2),
        settled,
        "a nudge under half a cell re-plans nothing"
    );

    // A leader that moves on by more than half a cell is followed again.
    s.issue(0, &[1], DebugTeleportCmd { pos: v(50, 60) }, false);
    s.run_ticks(40);
    assert!(requests_of(&s, 2) > settled, "the follower re-planned");
}

// --- flow fields (R111) ----------------------------------------------------------------------

/// Eight tanks (#1–#8) in a block near the south-west corner of an open 64 × 64 map with a
/// lake, ordered at tick 0 to the north-east corner: a flow-field group whose field bends
/// around the lake.
fn flow_group() -> ScenarioBuilder {
    let rows = draw(64, 64, |x, y| {
        if (20..=40).contains(&x) && (20..=40).contains(&y) {
            '~'
        } else {
            '.'
        }
    });
    let tanks: Vec<(&str, i32, i32)> = (0..8)
        .map(|i| ("t.tank", 30 + 8 * (i % 4), 30 + 8 * (i / 4)))
        .collect();
    let ids: Vec<u32> = (1..=8).collect();
    on_map(&rows, &tanks).command(
        0,
        0,
        &ids,
        MoveCmd {
            target: v(220, 220),
        },
    )
}

/// Every unit on a flow field stands in a cell the field has a direction for.
fn off_field(s: &mut Scenario) -> Vec<u32> {
    let units: Vec<(u32, PathState, FxVec2)> = (1..=8)
        .map(|id| (id, follow(s, id).state, pos(s, id)))
        .collect();
    s.world_mut()
        .resource_scope(|world, mut cache: Mut<FlowCache>| {
            let nav = world.resource::<NavGrid>();
            let hpa = world.resource::<HpaGraphs>();
            units
                .iter()
                .filter_map(|&(id, state, p)| match state {
                    PathState::Flow { key } => cache
                        .direction(key, nav.cell_of(p), nav, hpa, &mut FlowWork::default())
                        .is_none()
                        .then_some(id),
                    _ => None,
                })
                .collect()
        })
}

#[test]
fn flow_units_stay_on_the_field() {
    let mut s = flow_group().build();
    let mut flow_ticks = 0;
    for _ in 0..600 {
        s.run_ticks(1);
        let off = off_field(&mut s);
        assert!(
            off.is_empty(),
            "tick {}: {off:?} off the field",
            s.tick().0 - 1
        );
        assert!(on_blocked_cells(&s).is_empty());
        flow_ticks += (1..=8)
            .filter(|&id| matches!(follow(&s, id).state, PathState::Flow { .. }))
            .count();
    }
    assert!(flow_ticks > 8 * 100, "the group followed its field");
    // Every member asked for a path only through the group request and its arrival.
    for id in 1..=8 {
        assert!(
            requests_of(&s, id) <= 2,
            "#{id}: {} requests",
            requests_of(&s, id)
        );
    }
}

#[test]
fn flow_units_look_ahead_along_the_field() {
    // With the lookahead a flow unit steers toward the farthest cell it sees along the field,
    // so it never stops to turn in place at a 45° bend once under way.
    let mut s = flow_group().build();
    let mut last: Vec<FxVec2> = (1..=8).map(|id| pos(&s, id)).collect();
    let mut under_way = [false; 8];
    let mut stops = 0;
    for _ in 0..600 {
        s.run_ticks(1);
        for id in 1..=8 {
            let i = (id - 1) as usize;
            let p = pos(&s, id);
            if matches!(follow(&s, id).state, PathState::Flow { .. }) {
                if p == last[i] && under_way[i] {
                    stops += 1;
                }
                under_way[i] |= p != last[i];
            }
            last[i] = p;
        }
    }
    assert_eq!(stops, 0, "flow units stopped {stops} unit-ticks to turn");
}

// --- every step stays on passable cells (Review Focus 1) ------------------------------------

/// The side of every random map, in cells; a case's own `w × h` area lies in its south-west
/// corner and the rest is water.
const SIDE: u32 = 20;

/// The ASCII character of a random cell value.
fn random_char(v: u8) -> char {
    match v {
        0..=5 => '.',
        6 => 'f',
        7 => 'r',
        8 => 'F',
        _ => '~',
    }
}

/// A random `SIDE × SIDE` map: inside `w × h`, cell `(x, y)` is `cells[y × w + x]` (wrapping)
/// over `.`, `f`, `r`, `F` and `~`; outside, water. Row 0 is the north edge.
fn random_rows(w: u32, h: u32, cells: &[u8]) -> Vec<String> {
    draw(SIDE, SIDE, |x, y| {
        if x >= w || y >= h {
            '~'
        } else {
            random_char(cells[(y * w + x) as usize % cells.len()])
        }
    })
}

/// Whether a unit of any profile (size 1 or 2) may spawn with its centre in cell `(x, y)` of
/// `rows`: the 2 × 2 cells from it toward +x and +y are open ground.
fn open_for_all(rows: &[String], x: u32, y: u32) -> bool {
    let at = |x: u32, y: u32| -> char {
        let row = &rows[(SIDE - 1 - y) as usize];
        row.chars().nth(x as usize).expect("a cell")
    };
    x + 1 < SIDE
        && y + 1 < SIDE
        && [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)]
            .iter()
            .all(|&(x, y)| matches!(at(x, y), '.' | 'f' | 'r'))
}

const UNIT_KINDS: [&str; 4] = ["t.tank", "t.truck", "t.rifleman", "t.wide_tank"];

#[derive(Clone, Debug)]
enum Cmd {
    Move { x: u32, y: u32, queue: bool },
    Stop,
    Rotate { x: u32, y: u32 },
}

/// One random case: the map's own width and height in cells, its cells, the units `(kind, x,
/// y)` and the commands `(tick, unit mask, command)`.
type Case = (
    u32,
    u32,
    Vec<u8>,
    Vec<(usize, u32, u32)>,
    Vec<(u32, u8, Cmd)>,
);

fn cmd() -> impl Strategy<Value = Cmd> {
    prop_oneof![
        4 => (0u32..64, 0u32..64, any::<bool>()).prop_map(|(x, y, queue)| Cmd::Move { x, y, queue }),
        1 => Just(Cmd::Stop),
        1 => (0u32..64, 0u32..64).prop_map(|(x, y)| Cmd::Rotate { x, y }),
    ]
}

fn case() -> impl Strategy<Value = Case> {
    (
        8u32..=SIDE,
        8u32..=SIDE,
        proptest::collection::vec(0u8..12, 400),
        proptest::collection::vec((0usize..4, 0u32..SIDE, 0u32..SIDE), 1..9),
        proptest::collection::vec((0u32..250, any::<u8>(), cmd()), 0..12),
    )
}

/// What every random case shares: the content of an open `SIDE × SIDE` ASCII map and the
/// game's features, loaded once (each case paints its own terrain over the map).
struct Shared {
    content: Arc<ContentDb>,
    features: Vec<Arc<dyn SimFeature>>,
}

impl Shared {
    fn load() -> Shared {
        let s = on_map(&draw(SIDE, SIDE, |_, _| '.'), &[]).build();
        Shared {
            content: Arc::clone(&s.world().resource::<fl_sim::Content>().0),
            features: s.features().to_vec(),
        }
    }
}

/// The terrain id of an ASCII character of [`random_char`].
fn terrain_id(c: char) -> &'static str {
    match c {
        'f' => "t.forest",
        'r' => "t.road",
        'F' => "t.deep_forest",
        '~' => "t.water",
        _ => "t.grass",
    }
}

/// Run one random case for 300 ticks: every tick, every unit with `Mobility` stands on a cell
/// its profile can pass.
fn never_blocked_case(shared: &Shared, case: Case) -> Result<(), TestCaseError> {
    let (w, h, cells, units, commands) = case;
    let rows = random_rows(w, h, &cells);
    let mut placed: Vec<(&str, FxVec2)> = Vec::new();
    for &(kind, x, y) in &units {
        let (x, y) = (x % w, y % h);
        if open_for_all(&rows, x, y) {
            placed.push((UNIT_KINDS[kind % UNIT_KINDS.len()], at((x, y))));
        }
    }
    prop_assume!(!placed.is_empty());
    let n = u32::try_from(placed.len()).expect("few units");
    // No state hashes: this checks positions only.
    let mut b = ScenarioBuilder::with_content(Arc::clone(&shared.content), shared.features.clone())
        .player("t.red", 0)
        .hash_interval(0);
    for (tick, mask, cmd) in commands {
        let ids: Vec<u32> = (1..=n).filter(|id| mask & (1 << (id - 1)) != 0).collect();
        if ids.is_empty() {
            continue;
        }
        let point = |x: u32, y: u32| at((x % SIDE, y % SIDE));
        b = match cmd {
            Cmd::Move { x, y, queue: false } => b.command(
                tick,
                0,
                &ids,
                MoveCmd {
                    target: point(x, y),
                },
            ),
            Cmd::Move { x, y, queue: true } => b.queued_command(
                tick,
                0,
                &ids,
                MoveCmd {
                    target: point(x, y),
                },
            ),
            Cmd::Stop => b.command(tick, 0, &ids, StopCmd),
            Cmd::Rotate { x, y } => b.command(
                tick,
                0,
                &ids,
                RotateCmd {
                    toward: point(x, y),
                },
            ),
        };
    }
    let mut s = b.build();
    // The map, its terrain painted cell by cell, the derived indices, then the units (#1..#n).
    let world = s.world_mut();
    setup_map(world, "t.ascii").expect("the ASCII map sets up");
    {
        let mut terrain = world.resource_mut::<TerrainGrid>();
        for (r, row) in rows.iter().enumerate() {
            let y = SIDE - 1 - u32::try_from(r).expect("a small map");
            for (x, c) in (0..).zip(row.chars()) {
                if c != '.' {
                    let rect = CellRect {
                        min: (x, y),
                        max: (x, y),
                    };
                    terrain.set_rect(rect, terrain_id(c));
                }
            }
        }
    }
    rebuild_spatial(world);
    fl_sim::nav::rebuild_nav(world);
    for (unit, pos) in placed {
        let unit = shared.content.unit_id(unit).expect("an ASCII unit");
        let request = SpawnRequest {
            unit: Some(unit),
            owner: PlayerId(0),
            pos,
            heading: Angle::ZERO,
            init: None,
        };
        setup_spawn(world, &request).expect("an open cell");
    }
    for _ in 0..300 {
        let before = mobile_positions(&s);
        s.run_ticks(1);
        let tick = s.tick().0 - 1;
        let bad = on_blocked_cells(&s);
        prop_assert!(bad.is_empty(), "tick {tick}: {bad:?}");
        let swept = swept_blocked_cells(&s, &before);
        prop_assert!(swept.is_empty(), "tick {tick}: swept {swept:?}");
    }
    Ok(())
}

/// Run `cases` random cases of [`never_blocked_case`].
fn never_blocked(cases: u32) {
    let shared = Shared::load();
    let mut runner = TestRunner::new(ProptestConfig {
        // A case whose units all landed on blocked cells is rejected.
        max_global_rejects: cases * 4,
        ..ProptestConfig::with_cases(cases)
    });
    if let Err(err) = runner.run(&case(), |case| never_blocked_case(&shared, case)) {
        panic!("{err}");
    }
}

/// Random maps, units and move, stop and rotate commands, 300 ticks each: every tick, every
/// unit with `Mobility` stands on a cell its profile can pass (64 cases).
#[test]
fn units_never_enter_blocked_cells() {
    never_blocked(64);
}

/// [`units_never_enter_blocked_cells`] at 1,024 cases, for milestone runs.
#[test]
fn milestone_units_never_enter_blocked_cells_long() {
    never_blocked(1024);
}

/// The content of a small open map: a smoke check that the ASCII mod still loads with the
/// base `"Steering"` rules this file relies on.
#[test]
fn base_steering_rules_are_the_brief_values() {
    let content = ascii_terrain_content(&["...."], CELL_M);
    let r = content.rules::<SteeringRules>();
    assert_eq!(r.separation_gain, Fx::HALF);
    assert_eq!(r.separation_margin_m, Fx::HALF);
    assert_eq!(r.idle_every, 5);
    assert_eq!(r.max_push_frac, Fx::HALF);
}
