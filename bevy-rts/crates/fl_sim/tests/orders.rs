//! `core.rotate`, `core.follow`, Shift-queue semantics for every order kind, and formation
//! offsets for group moves (spec §7, ADR-0007 refinement, Task 9); and when a path request is
//! processed, by the hook that made it.

use std::path::PathBuf;
use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    DebugSpawnCmd, DebugTeleportCmd, DebugWanderCmd, FollowCmd, MoveCmd, PlayerId, RotateCmd,
    SimId, StopCmd, Tick,
};
use fl_sim::{
    MAX_WAYPOINTS, MapInfo, Mobility, MoveOrder, MovementRules, Order, PathFollow, PathState, Pose,
    SimBuilder, SimClock, SimFeature, SimIndex, ThinkSet, compute_state_hash, enqueue_path,
    formation_targets, last_seqs, load_content, setup_despawn,
};
use fl_test_utils::{
    Scenario, ScenarioBuilder, assert_branch, assert_deterministic, assert_shuffle_invariant,
    write_ascii_mod,
};
use fl_view::{JournalEntry, SimEvent};

/// Cell side of every ASCII map here, in metres.
const CELL_M: i32 = 4;

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

/// An open ASCII map of `w × h` cells.
fn open_rows(w: usize, h: usize) -> Vec<String> {
    vec![".".repeat(w); h]
}

/// An open `w × h`-cell map, player 0 (`t.red`) and a `t.tank` at each of `at` (metres).
fn tanks(w: usize, h: usize, at: &[(i32, i32)]) -> ScenarioBuilder {
    let rows = open_rows(w, h);
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut b = ScenarioBuilder::new()
        .ascii_map(&rows, CELL_M)
        .player("t.red", 0);
    for &(x, y) in at {
        b = b.unit("t.tank", 0, x, y);
    }
    b
}

fn entity(s: &Scenario, id: u32) -> Entity {
    s.world()
        .resource::<SimIndex>()
        .get(SimId(id))
        .unwrap_or_else(|| panic!("#{id} is alive"))
}

fn orders(s: &Scenario, id: u32) -> Vec<Order> {
    s.world()
        .get::<MoveOrder>(entity(s, id))
        .expect("a mobile unit has a MoveOrder")
        .orders
        .clone()
}

fn follow(s: &Scenario, id: u32) -> PathFollow {
    s.world()
        .get::<PathFollow>(entity(s, id))
        .cloned()
        .expect("a mobile unit has a PathFollow")
}

fn pos(s: &Scenario, id: u32) -> FxVec2 {
    s.pose(id).expect("alive").pos
}

fn heading(s: &Scenario, id: u32) -> Angle {
    s.pose(id).expect("alive").heading
}

fn rules(s: &Scenario) -> MovementRules {
    s.world()
        .resource::<fl_sim::Content>()
        .0
        .rules::<MovementRules>()
        .clone()
}

fn entries<'a>(s: &'a Scenario, kind: &str) -> Vec<&'a JournalEntry> {
    s.journal().iter().filter(|e| e.kind == kind).collect()
}

/// The `"core.order_changed"` reasons of #`id`, in journal order.
fn order_reasons(s: &Scenario, id: u32) -> Vec<String> {
    entries(s, "core.order_changed")
        .into_iter()
        .filter(|e| e.sim_id == Some(SimId(id)))
        .map(|e| e.data["reason"].as_str().expect("a reason").to_owned())
        .collect()
}

/// The reject codes of the last tick.
fn rejections(s: &Scenario) -> Vec<String> {
    s.last_output()
        .expect("a tick ran")
        .events
        .iter()
        .filter_map(|e| match e {
            SimEvent::CommandRejected { reason, .. } => Some(reason.clone()),
            _ => None,
        })
        .collect()
}

/// Run until #`id` has no orders left, at most `max` ticks; panics if it still has some.
fn run_until_idle(s: &mut Scenario, id: u32, max: u32) {
    for _ in 0..max {
        if orders(s, id).is_empty() {
            return;
        }
        s.run_ticks(1);
    }
    assert!(
        orders(s, id).is_empty(),
        "#{id} still has orders after {max} ticks: {:?}",
        orders(s, id)
    );
}

#[test]
fn formation_targets_pinned() {
    let map = MapInfo::flat(v(1000, 1000), Fx::from_int(CELL_M));
    let spread = Fx::from_int(40);
    assert_eq!(
        formation_targets(&[v(0, 0), v(10, 0)], v(100, 50), spread, &map),
        [v(95, 50), v(105, 50)]
    );
    // Spread 200 m around the centroid (100, 0): scaled to 40 m.
    assert_eq!(
        formation_targets(&[v(-100, 0), v(300, 0)], v(500, 500), spread, &map),
        [v(460, 500), v(540, 500)]
    );
    // Slots off the map are clamped onto it.
    let edge = Fx::from_int(1000) - Fx::EPSILON;
    assert_eq!(
        formation_targets(&[v(0, 0), v(20, 0)], v(995, 0), spread, &map),
        [v(985, 0), FxVec2::new(edge, Fx::ZERO)]
    );
    assert_eq!(
        formation_targets(&[v(7, 7)], v(100, 50), spread, &map),
        [v(100, 50)]
    );
    assert!(formation_targets(&[], v(100, 50), spread, &map).is_empty());
}

#[test]
fn formation_keeps_relative_offsets() {
    let start = [(20, 40), (30, 40), (20, 50)];
    let mut s = tanks(96, 24, &start).build();
    let centroid = FxVec2::new(
        Fx::from_int(70) / Fx::from_int(3),
        Fx::from_int(130) / Fx::from_int(3),
    );
    let target = centroid + v(300, 0);
    s.issue(0, &[1, 2, 3], MoveCmd { target }, false);
    s.run_ticks(1);
    for id in 1..=3 {
        run_until_idle(&mut s, id, 1000);
    }
    let start: Vec<FxVec2> = start.iter().map(|&(x, y)| v(x, y)).collect();
    let end: Vec<FxVec2> = (1..=3).map(|id| pos(&s, id)).collect();
    assert!(
        (end[0] - start[0]).length() > Fx::from_int(290),
        "they moved"
    );
    for i in 0..3 {
        for j in i + 1..3 {
            let drift = ((end[i] - end[j]) - (start[i] - start[j])).length();
            assert!(
                drift <= Fx::ONE,
                "#{} − #{}: offset drifted {drift} m",
                i + 1,
                j + 1
            );
        }
    }
}

#[test]
fn formation_spread_is_capped() {
    let mut s = tanks(96, 32, &[(20, 40), (220, 40), (120, 90)]).build();
    let target = v(200, 60);
    s.issue(0, &[1, 2, 3], MoveCmd { target }, false);
    s.run_ticks(1);
    let cap = rules(&s).formation_max_spread_m;
    assert_eq!(cap, Fx::from_int(40));
    for id in 1..=3 {
        let [Order::Move { target: slot }] = orders(&s, id)[..] else {
            panic!("#{id}: one move, not {:?}", orders(&s, id));
        };
        assert!(
            (slot - target).length() <= cap,
            "#{id}: slot {slot:?} is {} m from the target",
            (slot - target).length()
        );
    }
    // The widest pair keeps its direction: #1 west of #2.
    let slot = |id| match orders(&s, id)[..] {
        [Order::Move { target }] => target,
        _ => unreachable!(),
    };
    assert!(slot(1).x < slot(2).x);
}

#[test]
fn rotate_turns_in_place() {
    let mut s = tanks(32, 32, &[(64, 64)]).build();
    let start = pos(&s, 1);
    let toward = v(40, 88);
    let facing = (toward - start).angle();
    let turn_rate = s
        .world()
        .get::<Mobility>(entity(&s, 1))
        .expect("mobile")
        .turn_rate;
    let diff = heading(&s, 1).signed_diff(facing).unsigned_abs();
    let within = diff.div_ceil(u32::from(turn_rate.raw()));
    assert!(within > 1, "the turn takes several ticks");
    s.issue(0, &[1], RotateCmd { toward }, false);
    s.run_ticks(1);
    assert_eq!(orders(&s, 1), [Order::Rotate { facing }]);
    let mut faced = None;
    for _ in 0..within + 5 {
        assert_eq!(pos(&s, 1), start, "tick {}: turning in place", s.tick().0);
        if faced.is_none() && heading(&s, 1) == facing {
            faced = Some(s.tick().0);
        }
        if orders(&s, 1).is_empty() {
            break;
        }
        s.run_ticks(1);
    }
    let faced = faced.expect("the heading reached the facing");
    assert!(
        faced <= within,
        "faced after {faced} ticks, more than {within}"
    );
    assert!(orders(&s, 1).is_empty(), "the rotate popped");
    assert_eq!(heading(&s, 1), facing);
    assert_eq!(pos(&s, 1), start);
    assert_eq!(order_reasons(&s, 1), ["command", "completed"]);
}

#[test]
fn rotate_queued_after_move() {
    let mut s = tanks(32, 32, &[(20, 20)]).build();
    let a = v(100, 20);
    let toward = v(100, 100);
    s.issue(0, &[1], MoveCmd { target: a }, false);
    s.issue(0, &[1], RotateCmd { toward }, true);
    s.run_ticks(1);
    // The queued rotate faces the point from where the move ends: due north, not north-east
    // as seen from the start.
    let facing = Angle::QUARTER;
    assert_eq!(
        orders(&s, 1),
        [Order::Move { target: a }, Order::Rotate { facing }]
    );
    let arrive = rules(&s).arrive_m;
    let mut arrived = None;
    for _ in 0..600 {
        if arrived.is_none() && (pos(&s, 1) - a).length() <= arrive {
            arrived = Some(s.tick());
            assert_ne!(
                heading(&s, 1),
                facing,
                "it faces the point only after arriving"
            );
        }
        if orders(&s, 1).is_empty() {
            break;
        }
        s.run_ticks(1);
    }
    assert!(arrived.is_some(), "it arrived at A");
    assert!(orders(&s, 1).is_empty());
    assert_eq!(heading(&s, 1), facing);
    assert!(
        (pos(&s, 1) - a).length() <= arrive,
        "it turned where it arrived"
    );
    assert_eq!(
        order_reasons(&s, 1),
        ["command", "command", "completed", "completed"]
    );
}

/// Leader #1 at (40, 60) ordered 200 m east at tick 0; #2 at (40, 48) follows it from tick 0.
fn leader_and_follower() -> Scenario {
    let mut s = tanks(80, 32, &[(40, 60), (40, 48)]).build();
    s.issue(0, &[1], MoveCmd { target: v(240, 60) }, false);
    s.issue(0, &[2], FollowCmd { leader: SimId(1) }, false);
    s
}

#[test]
fn follow_tracks_moving_leader() {
    let mut s = leader_and_follower();
    s.run_ticks(1);
    let r = rules(&s);
    // 12 m apart: the offset is scaled to the follow distance, keeping its direction.
    assert_eq!(
        orders(&s, 2),
        [Order::Follow {
            leader: SimId(1),
            offset: FxVec2::new(Fx::ZERO, -r.follow_distance_m),
        }]
    );
    let ticks = 600;
    s.run_ticks(ticks - 1);
    assert!(orders(&s, 1).is_empty(), "the leader arrived");
    assert!((pos(&s, 1) - v(240, 60)).length() <= r.arrive_m);
    let gap = (pos(&s, 2) - pos(&s, 1)).length();
    assert!(
        gap <= r.follow_distance_m + Fx::from_int(4),
        "the follower is {gap} m from its leader"
    );
    assert!(
        matches!(orders(&s, 2)[..], [Order::Follow { .. }]),
        "following goes on"
    );
    let requests = entries(&s, "core.path_request").len();
    let bound = usize::try_from(ticks / r.follow_repath_ticks + 2).expect("small");
    assert!(
        requests <= bound,
        "{requests} path requests in {ticks} ticks"
    );
    assert!(
        requests >= 3,
        "the follower re-planned on the way ({requests})"
    );
}

#[test]
fn follow_ends_when_leader_is_gone() {
    let mut s = leader_and_follower();
    s.run_ticks(60);
    assert_eq!(follow(&s, 2).state, PathState::Following);
    assert!(setup_despawn(s.world_mut(), SimId(1)));
    s.run_ticks(1);
    assert!(orders(&s, 2).is_empty(), "the follow order popped");
    assert_eq!(follow(&s, 2).state, PathState::Idle);
    assert_eq!(order_reasons(&s, 2), ["command", "completed"]);
    let stopped = pos(&s, 2);
    s.run_ticks(5);
    assert_eq!(pos(&s, 2), stopped, "it stopped");
}

/// #1 moving from tick 0; at tick 5 player 0 sends `bad` follow commands, or (in the clean
/// twin) as many commands that name no units, which take their seqs and change nothing.
fn follow_rejection_run(bad: bool) -> (Scenario, Vec<Vec<String>>) {
    let mut s = tanks(32, 32, &[(20, 20), (40, 20)]).build();
    s.issue(
        0,
        &[1],
        MoveCmd {
            target: v(100, 100),
        },
        false,
    );
    s.run_ticks(5);
    let mut codes = Vec::new();
    let sends: [(&[u32], SimId); 2] = [(&[1], SimId(1)), (&[1, 2], SimId(99))];
    for (units, leader) in sends {
        let units: &[u32] = if bad { units } else { &[] };
        s.issue(0, units, FollowCmd { leader }, false);
        s.run_ticks(1);
        codes.push(rejections(&s));
    }
    s.run_ticks(5);
    (s, codes)
}

#[test]
fn follow_self_or_missing_leader_rejected() {
    let (clean, _) = follow_rejection_run(false);
    let (dirty, codes) = follow_rejection_run(true);
    assert_eq!(
        codes,
        [["core.no_valid_units"], ["core.invalid_target"]],
        "following itself leaves no units; a missing leader is an invalid target"
    );
    assert_eq!(
        last_seqs(dirty.world()),
        [(PlayerId(0), 2)].into(),
        "both commands took their seqs"
    );
    assert_eq!(orders(&dirty, 2), []);
    assert!(matches!(orders(&dirty, 1)[..], [Order::Move { .. }]));
    assert_eq!(
        compute_state_hash(dirty.world()),
        compute_state_hash(clean.world()),
        "the full state hash, LastSeq included"
    );
}

#[test]
fn shift_queue_mixed_orders() {
    let mut s = tanks(32, 32, &[(20, 20)]).build();
    let (a, b) = (v(60, 20), v(60, 80));
    let toward = v(20, 60);
    s.issue(0, &[1], MoveCmd { target: a }, false);
    s.issue(0, &[1], RotateCmd { toward }, true);
    s.issue(0, &[1], MoveCmd { target: b }, true);
    s.run_ticks(1);
    let facing = (toward - a).angle();
    assert_eq!(
        orders(&s, 1),
        [
            Order::Move { target: a },
            Order::Rotate { facing },
            Order::Move { target: b },
        ]
    );
    let arrive = rules(&s).arrive_m;
    let (mut at_a, mut faced, mut at_b) = (None, None, None);
    for _ in 0..800 {
        let here = pos(&s, 1);
        if at_a.is_none() && (here - a).length() <= arrive {
            at_a = Some(s.tick());
        }
        if at_a.is_some() && faced.is_none() && heading(&s, 1) == facing {
            faced = Some(s.tick());
            assert!((here - a).length() <= arrive, "it turned at A");
        }
        if at_b.is_none() && (here - b).length() <= arrive {
            at_b = Some(s.tick());
        }
        if orders(&s, 1).is_empty() {
            break;
        }
        s.run_ticks(1);
    }
    let (at_a, faced, at_b) = (
        at_a.expect("reached A"),
        faced.expect("faced the point"),
        at_b.expect("reached B"),
    );
    assert!(
        at_a <= faced && faced < at_b,
        "A {at_a:?}, facing {faced:?}, B {at_b:?}"
    );
    assert!(orders(&s, 1).is_empty());
    assert_eq!(
        order_reasons(&s, 1),
        [
            "command",
            "command",
            "command",
            "completed",
            "completed",
            "completed"
        ]
    );
    let currents: Vec<&str> = entries(&s, "core.order_changed")
        .into_iter()
        .filter(|e| e.data["reason"] == "completed")
        .map(|e| e.data["current"].as_str().expect("a kind"))
        .collect();
    assert_eq!(currents, ["rotate", "move", "none"]);
}

#[test]
fn queued_orders_share_the_cap() {
    let mut s = tanks(32, 32, &[(20, 20), (40, 20), (60, 20)]).build();
    s.issue(0, &[1], MoveCmd { target: v(100, 20) }, false);
    for i in 1..MAX_WAYPOINTS {
        let n = i32::try_from(i).expect("small");
        match i % 3 {
            0 => s.issue(
                0,
                &[1],
                MoveCmd {
                    target: v(100, 20 + n),
                },
                true,
            ),
            1 => s.issue(0, &[1], RotateCmd { toward: v(n, 100) }, true),
            _ => s.issue(0, &[1], FollowCmd { leader: SimId(2) }, true),
        }
    }
    s.run_ticks(1);
    assert!(rejections(&s).is_empty());
    let full = orders(&s, 1);
    assert_eq!(full.len(), MAX_WAYPOINTS);
    for kind in ["move", "rotate", "follow"] {
        assert!(
            full.iter().any(|o| match o {
                Order::Move { .. } => kind == "move",
                Order::Rotate { .. } => kind == "rotate",
                Order::Follow { .. } => kind == "follow",
            }),
            "the queue holds a {kind}"
        );
    }

    // A queued rotate or follow naming the full unit is rejected as a whole: #3, which has
    // room, gets nothing either.
    for rotate in [true, false] {
        let before = (orders(&s, 1), orders(&s, 3), follow(&s, 3));
        if rotate {
            s.issue(0, &[1, 3], RotateCmd { toward: v(0, 0) }, true);
        } else {
            s.issue(0, &[1, 3], FollowCmd { leader: SimId(2) }, true);
        }
        s.run_ticks(1);
        assert_eq!(rejections(&s), ["core.queue_full"]);
        assert_eq!((orders(&s, 1), orders(&s, 3), follow(&s, 3)), before);
    }
    // An unqueued rotate replaces the full queue.
    s.issue(0, &[1], RotateCmd { toward: v(20, 100) }, false);
    s.run_ticks(1);
    assert!(rejections(&s).is_empty());
    assert_eq!(orders(&s, 1).len(), 1);
}

/// Six tanks on an open map: a formation move of #1–#3 with a queued rotate and a queued
/// move, #4 following #1, #5 and #6 following each other, a rotate of #3 replacing its
/// orders, then a queued follow of #2 by #3.
fn mixed_orders() -> ScenarioBuilder {
    tanks(
        48,
        32,
        &[
            (20, 20),
            (30, 24),
            (24, 32),
            (12, 12),
            (150, 100),
            (170, 110),
        ],
    )
    .command(0, 0, &[1, 2, 3], MoveCmd { target: v(120, 60) })
    .queued_command(
        0,
        0,
        &[1, 2, 3],
        RotateCmd {
            toward: v(160, 120),
        },
    )
    .queued_command(0, 0, &[1, 2], MoveCmd { target: v(60, 100) })
    .command(2, 0, &[4], FollowCmd { leader: SimId(1) })
    // An A→B→A follow cycle.
    .command(3, 0, &[5], FollowCmd { leader: SimId(6) })
    .command(3, 0, &[6], FollowCmd { leader: SimId(5) })
    .command(50, 0, &[3], RotateCmd { toward: v(0, 0) })
    .queued_command(60, 0, &[3], FollowCmd { leader: SimId(2) })
}

#[test]
fn orders_twin_and_shuffle() {
    assert_deterministic(mixed_orders, 300);
    assert_shuffle_invariant(mixed_orders, 300);
}

/// Save and load with followers re-planning on their cadence, a follow cycle and a queued
/// follow behind a rotate (tick 70 is after the queued follow of tick 60).
#[test]
fn branch_mid_follow() {
    assert_branch(mixed_orders, 70, 300);
}

/// A test feature asking for a path for #1 to (200, 100) in tick 2 from `ThinkSet::Orders`
/// and to (100, 200) in tick 4 from `ThinkSet::Behaviour`.
struct EnqueueFeature;

fn enqueue_at(world: &mut World, at: u32, goal: FxVec2) {
    if world.resource::<SimClock>().tick == Tick(at) {
        enqueue_path(world, SimId(1), goal);
    }
}

fn enqueue_in_orders(world: &mut World) {
    enqueue_at(world, 2, v(200, 100));
}

fn enqueue_in_behaviour(world: &mut World) {
    enqueue_at(world, 4, v(100, 200));
}

impl SimFeature for EnqueueFeature {
    fn id(&self) -> &'static str {
        "enqueue"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(ThinkSet::Orders, enqueue_in_orders)
            .add_systems(ThinkSet::Behaviour, enqueue_in_behaviour);
    }
}

/// When a request is processed depends only on the hook that made it (Task 8 review Minor
/// 9): the queue is processed in the core slot of `ThinkSet::Behaviour`, after every
/// `Input` and `ThinkSet::Orders` system and before every feature `Behaviour` system.
#[test]
fn path_request_timing_follows_the_hook() {
    let core_mod = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod");
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(EnqueueFeature)];
    let content = load_content(&features, &[core_mod]).expect("core_mod loads");
    for shuffle in [None, Some(0x5EED_0001)] {
        let mut s = ScenarioBuilder::with_content(Arc::clone(&content), features.clone())
            .scenario("core_test.duel")
            .shuffle_seed(shuffle)
            .build();
        s.run_ticks(6);
        let results: Vec<(u32, u64, u64)> = entries(&s, "core.path_result")
            .into_iter()
            .map(|e| {
                (
                    e.tick.0,
                    e.data["request"].as_u64().expect("an id"),
                    e.data["waited"].as_u64().expect("ticks"),
                )
            })
            .collect();
        assert_eq!(
            results,
            [(2, 0, 0), (5, 1, 1)],
            "an Orders request is processed in its tick, a feature Behaviour request in the next"
        );
    }
}

/// The error loading the ASCII mod with the base rules' `from` replaced by `to`.
fn rules_error(name: &str, from: &str, to: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/base/rules.ron");
    let base = std::fs::read_to_string(&path).expect("the base rules");
    assert!(base.contains(from), "the base rules hold {from:?}");
    let rules = base.replacen(from, to, 1);
    let dir = std::env::temp_dir().join(format!("fl_sim_orders_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temporary mod directory");
    write_ascii_mod(&dir, &["...."], CELL_M, Some(&rules));
    let result = load_content(&[], std::slice::from_ref(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(_) => panic!("{name}: the mod loads"),
        Err(err) => err.to_string(),
    }
}

#[test]
fn group_order_rules_are_range_checked() {
    for (name, from, to, want) in [
        (
            "spread",
            "formation_max_spread_m: 40.0",
            "formation_max_spread_m: -1.0",
            "Movement/formation_max_spread_m",
        ),
        (
            "distance",
            "follow_distance_m: 8.0",
            "follow_distance_m: -0.5",
            "Movement/follow_distance_m",
        ),
        (
            "repath",
            "follow_repath_ticks: 20",
            "follow_repath_ticks: 0",
            "Movement/follow_repath_ticks",
        ),
    ] {
        let err = rules_error(name, from, to);
        assert!(err.contains(want), "{name}: {want:?} not in {err}");
    }
}

/// The `"core.path_request"` entries of #`id`.
fn requests_of(s: &Scenario, id: u32) -> usize {
    entries(s, "core.path_request")
        .into_iter()
        .filter(|e| e.sim_id == Some(SimId(id)))
        .count()
}

/// A follower's ended path (`Done`, `Failed`) asks for a new one on the
/// `follow_repath_ticks` cadence, never every tick (Task 8 review Minor 7).
#[test]
fn follow_replans_ended_paths_on_a_cadence() {
    // #1 stands still; #2, 30 m south of it, follows it to 8 m and stops there.
    let mut s = tanks(32, 32, &[(60, 60), (60, 30)]).build();
    s.issue(0, &[2], FollowCmd { leader: SimId(1) }, false);
    s.run_ticks(200);
    assert_eq!(follow(&s, 2).state, PathState::Done);
    assert!((pos(&s, 2) - v(60, 52)).length() <= rules(&s).arrive_m);
    let settled = requests_of(&s, 2);
    s.run_ticks(100);
    assert_eq!(
        requests_of(&s, 2),
        settled,
        "at its goal it asks for nothing"
    );

    // A failed path is retried once `follow_repath_ticks` have passed since it was planned.
    let repath = rules(&s).follow_repath_ticks;
    let now = s.tick();
    let e = entity(&s, 2);
    {
        let mut f = s.world_mut().get_mut::<PathFollow>(e).expect("mobile");
        f.state = PathState::Failed;
        f.planned = now;
    }
    s.run_ticks(repath);
    assert_eq!(requests_of(&s, 2), settled, "not before the cadence");
    s.run_ticks(1);
    assert_eq!(requests_of(&s, 2), settled + 1, "retried on the cadence");
    s.run_ticks(60);
    assert_eq!(follow(&s, 2).state, PathState::Done);
    let settled = requests_of(&s, 2);

    // A stopped follower whose leader drives off re-plans as soon as the leader is more than
    // half a cell away (its plan is old; Task 11 hysteresis), then at most once per
    // `follow_repath_ticks`.
    s.issue(0, &[1], MoveCmd { target: v(100, 60) }, false);
    s.run_ticks(8);
    assert_eq!(
        requests_of(&s, 2),
        settled + 1,
        "it set off after the leader"
    );
    s.run_ticks(92);
    let more = requests_of(&s, 2) - settled;
    assert!(
        (2..=1 + 100 / repath as usize).contains(&more),
        "{more} requests in 100 ticks"
    );
}

/// Payload coordinates come from the wire: extreme values are clamped before any arithmetic
/// (R104), so nothing panics on overflow in debug builds or wraps in release builds.
#[test]
fn extreme_payload_values_never_panic() {
    let mut s = tanks(32, 32, &[(20, 20), (40, 20)]).build();
    let map = s.world().resource::<MapInfo>().clone();
    let extremes = [Fx::MIN, Fx::MAX, Fx::from_raw(i64::MAX), Fx::ZERO];
    for &x in &extremes {
        for &y in &extremes {
            let p = FxVec2::new(x, y);
            let clamped = map.clamp(p);
            let from = pos(&s, 1);
            s.issue(0, &[1], RotateCmd { toward: p }, false);
            s.run_ticks(1);
            assert!(rejections(&s).is_empty(), "rotate toward {p:?}");
            let facing = if clamped == from {
                None
            } else {
                Some((clamped - from).angle())
            };
            if let (Some(facing), [Order::Rotate { facing: got }]) = (facing, &orders(&s, 1)[..]) {
                assert_eq!(*got, facing, "rotate toward {p:?} faces the clamped point");
            }
            s.issue(0, &[1], RotateCmd { toward: p }, true);
            s.issue(0, &[1, 2], MoveCmd { target: p }, false);
            s.issue(0, &[1, 2], MoveCmd { target: p }, true);
            s.issue(0, &[2], DebugTeleportCmd { pos: p }, false);
            s.issue(
                0,
                &[],
                DebugSpawnCmd {
                    unit: "t.tank".to_owned(),
                    owner: PlayerId(0),
                    pos: p,
                    heading: Angle::ZERO,
                },
                false,
            );
            s.run_ticks(1);
            for code in rejections(&s) {
                assert_eq!(
                    code, "core.invalid_target",
                    "only blocked cells are refused"
                );
            }
            for id in [1, 2] {
                assert!(map.contains(pos(&s, id)), "#{id} is on the map");
            }
        }
    }
    for radius in [Fx::MAX, Fx::MIN] {
        s.issue(0, &[1, 2], DebugWanderCmd { radius }, false);
        s.run_ticks(30);
        assert!(rejections(&s).is_empty());
    }
}

/// A follower standing exactly on its leader gets a non-zero offset: `follow_distance_m`
/// behind the leader's heading.
#[test]
fn follower_on_its_leader_keeps_behind_it() {
    let mut s = tanks(32, 32, &[(40, 40), (40, 40)]).build();
    s.issue(0, &[2], FollowCmd { leader: SimId(1) }, false);
    s.run_ticks(1);
    let distance = rules(&s).follow_distance_m;
    assert_eq!(
        orders(&s, 2),
        [Order::Follow {
            leader: SimId(1),
            offset: FxVec2::new(-distance, Fx::ZERO),
        }]
    );
}

/// `core.stop` is never queued: with `queue` set it still drops every order at once.
#[test]
fn stop_with_queue_is_never_queued() {
    let mut s = tanks(32, 32, &[(20, 20)]).build();
    s.issue(0, &[1], MoveCmd { target: v(100, 20) }, false);
    s.issue(0, &[1], RotateCmd { toward: v(0, 0) }, true);
    s.run_ticks(3);
    assert_eq!(orders(&s, 1).len(), 2);
    s.issue(0, &[1], StopCmd, true);
    s.run_ticks(1);
    assert!(orders(&s, 1).is_empty());
    assert_eq!(follow(&s, 1).state, PathState::Idle);
    assert_eq!(order_reasons(&s, 1), ["command", "command", "stopped"]);
}

/// A rotate is given like a move or a follow even to a unit without a [`Pose`]; with no
/// heading to turn, the order completes at once.
#[test]
fn rotate_without_pose_applies_and_completes() {
    let mut s = tanks(32, 32, &[(20, 20)]).build();
    let e = entity(&s, 1);
    s.world_mut().entity_mut(e).remove::<Pose>();
    s.issue(0, &[1], RotateCmd { toward: v(0, 0) }, false);
    s.run_ticks(1);
    assert!(rejections(&s).is_empty());
    assert_eq!(order_reasons(&s, 1), ["command", "completed"]);
    assert!(orders(&s, 1).is_empty());
}
