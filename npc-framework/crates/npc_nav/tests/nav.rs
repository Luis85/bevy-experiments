//! Plan Task C1: `npc_nav` (grid A*, smoothing, budget, replanning, hooks).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_kernel::hash::state_hash;
use npc_kernel::journal::{JournalEntry, JournalKind};
use npc_kernel::spatial::Blocker;
use npc_kernel::spatial::{
    CellCoord, CellFlags, CellInfo, Footprint, GridBlocker, GridBounds, PathFailReason,
    SpatialGrid, SpatialPlugin,
};
use npc_kernel::{Journal, JournalPlugin, NpcKernelPlugin, SimId, SimPos, run_ticks, spawn_sim};
use npc_locomotion::{MoveIntent, MoveStatus, StraightLineMover, set_move_intent};
use npc_nav::{
    NavAgent, NavAppExt, NavBudget, NavPlugin, NavState, PassCost, Path, explain_blocked,
    explain_blocked_within,
};

const WALL: CellInfo = CellInfo {
    flags: CellFlags {
        blocking: true,
        occluding: true,
        tags: 0,
    },
    extra_cost: 0,
};

fn app_with(cells: u32, budget: u32) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(cells, cells)))
        .insert_resource(NavBudget(budget));
    app.add_plugins((
        NpcKernelPlugin::default(),
        SpatialPlugin,
        StraightLineMover,
        NavPlugin,
        JournalPlugin::default(),
    ));
    app.finish();
    app.cleanup();
    app
}

fn app() -> App {
    app_with(32, 4000)
}

fn wall(app: &mut App, cells: impl IntoIterator<Item = (i32, i32)>) {
    let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
    for (x, z) in cells {
        assert!(grid.set_static(CellCoord::new(x, z), WALL));
    }
}

fn agent(app: &mut App, from: SimPos, to: SimPos, speed: i32) -> (SimId, Entity) {
    let (id, e) = spawn_sim(app.world_mut(), (from, NavAgent));
    set_move_intent(app.world_mut(), e, MoveIntent::to(to, speed));
    (id, e)
}

fn journal(app: &App) -> Vec<JournalEntry> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .cloned()
        .collect()
}

fn path_entries(app: &App) -> Vec<(u64, SimId, JournalKind)> {
    journal(app)
        .into_iter()
        .filter(|e| {
            matches!(
                e.kind,
                JournalKind::PathFound { .. } | JournalKind::PathFailed { .. }
            )
        })
        .map(|e| (e.tick, e.unit.unwrap(), e.kind))
        .collect()
}

fn pos(app: &App, e: Entity) -> SimPos {
    *app.world().get::<SimPos>(e).unwrap()
}

fn status(app: &App, e: Entity) -> MoveStatus {
    *app.world().get::<MoveStatus>(e).unwrap()
}

/// Runs until `e` arrives; returns the ticks taken. Asserts it never stands
/// in a blocking cell.
fn run_to_arrival(app: &mut App, e: Entity, max: u64) -> u64 {
    for t in 1..=max {
        run_ticks(app.world_mut(), 1);
        let p = pos(app, e);
        let grid = app.world().resource::<SpatialGrid>();
        assert!(
            !grid.is_blocking(grid.cell_of(p)),
            "agent inside a wall at {p:?}"
        );
        if status(app, e) == MoveStatus::Arrived {
            return t;
        }
    }
    panic!(
        "no arrival within {max} ticks (status {:?})",
        status(app, e)
    );
}

/// Every segment of the path (from `start`) avoids blocking cells.
fn assert_path_clear(app: &App, start: SimPos, path: &Path) {
    let grid = app.world().resource::<SpatialGrid>();
    let mut from = start;
    for &wp in &path.waypoints {
        assert!(
            grid.trace_segment(from, wp, |c| !grid.is_blocking(c)),
            "segment {from:?} -> {wp:?} crosses a wall"
        );
        from = wp;
    }
}

#[test]
fn open_field_is_a_straight_line() {
    let mut app = app();
    let (from, to) = (SimPos::new(-5500, -3200), SimPos::new(7300, 4100));
    let (id, e) = agent(&mut app, from, to, 80);
    run_ticks(app.world_mut(), 1);
    let path = app.world().get::<Path>(e).unwrap().clone();
    assert_eq!(path.waypoints, vec![to], "smoothed to the straight line");
    let found = path_entries(&app);
    assert!(
        matches!(found.as_slice(), [(0, u, JournalKind::PathFound { len: 1, .. })] if *u == id)
    );

    // It walks exactly like the straight-line mover.
    let mut straight = from;
    for _ in 0..5 {
        straight = straight.step_towards(to, 80);
    }
    run_ticks(app.world_mut(), 4);
    assert_eq!(pos(&app, e), straight);
    run_to_arrival(&mut app, e, 1000);
    assert_eq!(pos(&app, e), to);
}

#[test]
fn wall_with_a_gap() {
    let mut app = app();
    // Wall at x = 0 from z = -10 to 10, with a gap at z = 6.
    wall(&mut app, (-10..=10).filter(|&z| z != 6).map(|z| (0, z)));
    let (from, to) = (SimPos::new(-4500, 500), SimPos::new(4500, 500));
    let (_, e) = agent(&mut app, from, to, 100);
    run_ticks(app.world_mut(), 1);
    let path = app.world().get::<Path>(e).unwrap().clone();
    assert_path_clear(&app, from, &path);
    assert!(path.waypoints.len() >= 2, "goes through the gap: {path:?}");
    let grid = app.world().resource::<SpatialGrid>();
    assert!(
        path.waypoints.iter().any(|w| grid.cell_of(*w).z >= 5),
        "via the gap at z=6: {path:?}"
    );
    run_to_arrival(&mut app, e, 2000);
    assert_eq!(pos(&app, e), to);
}

#[test]
fn maze() {
    let mut app = app_with(32, 4000);
    // A serpentine: walls at x = -6, -2, 2, 6 with alternating openings.
    let mut cells = Vec::new();
    for (i, x) in [-6, -2, 2, 6].into_iter().enumerate() {
        for z in -16..=15 {
            let open = if i % 2 == 0 { z == 15 } else { z == -16 };
            if !open {
                cells.push((x, z));
            }
        }
    }
    wall(&mut app, cells);
    let (from, to) = (SimPos::new(-9500, 0), SimPos::new(9500, 0));
    let (_, e) = agent(&mut app, from, to, 150);
    run_ticks(app.world_mut(), 1);
    let path = app.world().get::<Path>(e).unwrap().clone();
    assert_path_clear(&app, from, &path);
    assert!(
        path.waypoints.len() >= 8,
        "winds through the maze: {path:?}"
    );
    run_to_arrival(&mut app, e, 5000);
}

#[test]
fn unreachable_goal_fails_cleanly() {
    let mut app = app();
    // A closed box around the goal.
    let mut cells = Vec::new();
    for x in 3..=7 {
        for z in 3..=7 {
            if x == 3 || x == 7 || z == 3 || z == 7 {
                cells.push((x, z));
            }
        }
    }
    wall(&mut app, cells);
    let (id, e) = agent(
        &mut app,
        SimPos::new(-5000, -5000),
        SimPos::new(5500, 5500),
        60,
    );
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        path_entries(&app),
        vec![(
            0,
            id,
            JournalKind::PathFailed {
                reason: PathFailReason::Unreachable
            }
        )]
    );
    assert!(matches!(
        app.world().get::<NavState>(e),
        Some(NavState::Failed {
            reason: PathFailReason::Unreachable,
            ..
        })
    ));
    run_ticks(app.world_mut(), 20);
    assert_eq!(status(&app, e), MoveStatus::Blocked { ticks: 21 });
    assert_eq!(pos(&app, e), SimPos::new(-5000, -5000), "does not move");
    assert_eq!(
        path_entries(&app).len(),
        1,
        "no retry without a grid change"
    );

    // Opening the box retries and succeeds.
    app.world_mut()
        .resource_mut::<SpatialGrid>()
        .set_static(CellCoord::new(3, 5), CellInfo::default());
    run_ticks(app.world_mut(), 2);
    let entries = path_entries(&app);
    assert!(
        matches!(entries.last(), Some((_, _, JournalKind::PathFound { .. }))),
        "{entries:?}"
    );
    run_to_arrival(&mut app, e, 3000);
}

/// Wave H2: `retry_failed_path` re-queues one failed request (what a
/// pass-cost hook decides per agent, e.g. a key, is no `GridChanged`).
#[test]
fn retry_failed_path_requeues_one_failed_request() {
    let mut app = app();
    wall(&mut app, (-16..16).map(|z| (0, z)));
    let goal = SimPos::new(3500, 500);
    let (id, e) = agent(&mut app, SimPos::new(-3500, 500), goal, 100);
    let (_, idle) = spawn_sim(app.world_mut(), (SimPos::new(-3500, 3500), NavAgent));
    run_ticks(app.world_mut(), 2);
    assert!(matches!(
        app.world().get::<NavState>(e),
        Some(NavState::Failed { .. })
    ));
    let w = app.world_mut();
    assert!(!npc_nav::retry_failed_path(w, idle), "not failed: nothing");
    let tick = w.resource::<npc_kernel::SimClock>().tick;
    assert!(npc_nav::retry_failed_path(w, e));
    assert_eq!(
        w.get::<NavState>(e).copied(),
        Some(NavState::Pending { goal, tick })
    );
    assert!(!npc_nav::retry_failed_path(w, e), "already pending");
    // Still no way through: it fails again, once.
    run_ticks(app.world_mut(), 3);
    let failed = path_entries(&app)
        .into_iter()
        .filter(|(_, u, k)| *u == id && matches!(k, JournalKind::PathFailed { .. }))
        .count();
    assert_eq!(failed, 2);
    assert!(matches!(
        app.world().get::<NavState>(e),
        Some(NavState::Failed { .. })
    ));
}

#[test]
fn start_inside_a_blocker_fails() {
    let mut app = app();
    let (id, e) = spawn_sim(app.world_mut(), (SimPos::new(500, 500), NavAgent));
    spawn_sim(
        app.world_mut(),
        (SimPos::new(500, 500), GridBlocker(Footprint::default())),
    );
    run_ticks(app.world_mut(), 1); // the blocker lands in the grid
    set_move_intent(
        app.world_mut(),
        e,
        MoveIntent::to(SimPos::new(5000, 500), 60),
    );
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        path_entries(&app),
        vec![(
            1,
            id,
            JournalKind::PathFailed {
                reason: PathFailReason::StartBlocked
            }
        )]
    );
    // Goal in a wall / out of bounds.
    wall(&mut app, [(9, 9)]);
    let (b, _) = agent(
        &mut app,
        SimPos::new(-500, -500),
        SimPos::new(9500, 9500),
        60,
    );
    let (c, _) = agent(
        &mut app,
        SimPos::new(-500, -500),
        SimPos::new(99_000, 0),
        60,
    );
    run_ticks(app.world_mut(), 1);
    let entries = path_entries(&app);
    assert!(entries.contains(&(
        2,
        b,
        JournalKind::PathFailed {
            reason: PathFailReason::GoalBlocked
        }
    )));
    assert!(entries.contains(&(
        2,
        c,
        JournalKind::PathFailed {
            reason: PathFailReason::OutOfBounds
        }
    )));
}

#[test]
fn no_corner_cutting_between_diagonal_walls() {
    let mut app = app();
    // Two walls touching only at a corner; the straight line passes the corner.
    wall(&mut app, [(1, 0), (0, 1)]);
    let (from, to) = (SimPos::new(500, 500), SimPos::new(1500, 1500));
    let (_, e) = agent(&mut app, from, to, 50);
    run_ticks(app.world_mut(), 1);
    let path = app.world().get::<Path>(e).unwrap().clone();
    assert_path_clear(&app, from, &path);
    assert!(
        path.waypoints.len() > 1,
        "must go around, not through the corner: {path:?}"
    );
    run_to_arrival(&mut app, e, 2000);
}

/// 50 agents, budget far below what they need in one tick.
fn crowd(seed_shift: i32) -> (App, Vec<(SimId, Entity)>) {
    let mut app = app_with(128, 600);
    let mut cells = Vec::new();
    for x in [-40i32, -10, 20, 50] {
        for z in -60..=60 {
            if (z + x).rem_euclid(17) != 0 {
                cells.push((x, z));
            }
        }
    }
    wall(&mut app, cells);
    let agents = (0..50)
        .map(|i| {
            let from = SimPos::new(
                -60_000 + i * 300,
                -50_000 + (i * 1977 + seed_shift) % 100_000,
            );
            let to = SimPos::new(60_000 - i * 200, -50_000 + (i * 3511) % 100_000);
            agent(&mut app, from, to, 400)
        })
        .collect();
    (app, agents)
}

#[test]
fn nav_budget_carries_over_deterministically() {
    let (mut a, agents) = crowd(0);
    run_ticks(a.world_mut(), 1);
    let first = path_entries(&a);
    assert!(
        first.len() < 50,
        "600 nodes cannot serve 50 requests in one tick ({} served)",
        first.len()
    );
    run_ticks(a.world_mut(), 299);
    let served = path_entries(&a);
    assert_eq!(served.len(), 50, "every request served eventually");
    // Simultaneous requests are served in SimId order, across ticks.
    let order: Vec<SimId> = served.iter().map(|(_, u, _)| *u).collect();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted);
    assert!(served.last().unwrap().0 > 0, "carried over to later ticks");
    let ids: Vec<SimId> = agents.iter().map(|a| a.0).collect();
    assert_eq!(sorted, ids);

    // A second run gives the same journal and the same state hash.
    let (mut b, _) = crowd(0);
    run_ticks(b.world_mut(), 300);
    assert_eq!(journal(&a), journal(&b));
    assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
    // And a different world gives a different one (the hash sees nav state).
    let (mut c, _) = crowd(1);
    run_ticks(c.world_mut(), 300);
    assert_ne!(state_hash(a.world_mut()), state_hash(c.world_mut()));
}

#[test]
fn nav_replans_when_blocked() {
    let mut app = app();
    let (from, to) = (SimPos::new(-8500, 500), SimPos::new(8500, 500));
    let (id, e) = agent(&mut app, from, to, 100);
    run_ticks(app.world_mut(), 20);
    assert_eq!(app.world().get::<Path>(e).unwrap().waypoints, vec![to]);
    // A wall drops right across the path ahead of the agent.
    spawn_sim(
        app.world_mut(),
        (
            SimPos::new(2500, 500),
            GridBlocker(Footprint {
                half_x_mm: 0,
                half_z_mm: 4000,
            }),
        ),
    );
    run_ticks(app.world_mut(), 1);
    let found: Vec<_> = path_entries(&app)
        .into_iter()
        .filter(|(_, u, k)| *u == id && matches!(k, JournalKind::PathFound { .. }))
        .collect();
    assert_eq!(found.len(), 2, "replanned on GridChanged: {found:?}");
    let path = app.world().get::<Path>(e).unwrap().clone();
    assert!(path.waypoints.len() >= 2, "around the new wall: {path:?}");
    assert_path_clear(&app, pos(&app, e), &path);
    run_to_arrival(&mut app, e, 2000);
    assert_eq!(pos(&app, e), to);
}

#[test]
fn replans_after_blocked_ticks() {
    // A path that runs into a wall without a GridChanged (here: a stale path
    // put back by hand after the change was handled) is caught by
    // `Blocked { ticks }` reaching 10.
    let mut app = app();
    let (from, to) = (SimPos::new(-3500, 500), SimPos::new(3500, 500));
    let (id, e) = agent(&mut app, from, to, 100);
    run_ticks(app.world_mut(), 1);
    wall(&mut app, (-5..=5).map(|z| (0, z)));
    run_ticks(app.world_mut(), 1); // GridChanged -> replan
    let replans = path_entries(&app)
        .into_iter()
        .filter(|(_, u, _)| *u == id)
        .count();
    assert_eq!(replans, 2);
    let stale = Path {
        waypoints: vec![to],
        markers: vec![],
        next: 0,
    };
    app.world_mut().entity_mut(e).insert(stale);
    let mut blocked_ticks = 0;
    for _ in 0..80 {
        run_ticks(app.world_mut(), 1);
        if matches!(status(&app, e), MoveStatus::Blocked { .. }) {
            blocked_ticks += 1;
        }
    }
    assert!(blocked_ticks >= 10, "ran into the wall first");
    let found = path_entries(&app)
        .into_iter()
        .filter(|(_, u, _)| *u == id)
        .count();
    assert_eq!(found, 3, "replanned after 10 blocked ticks");
    run_to_arrival(&mut app, e, 2000);
}

const DOOR_TAG: u8 = 0;

#[derive(Component)]
struct MayOpenDoors;

fn door_cost(world: &World, agent: Entity, _: CellCoord, _: CellInfo) -> PassCost {
    if world.get::<MayOpenDoors>(agent).is_some() {
        PassCost::Marked(20)
    } else {
        PassCost::Blocked
    }
}

#[test]
fn door_hook_marks_the_door_waypoint() {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(32, 32)));
    app.register_pass_cost(DOOR_TAG, door_cost);
    app.add_plugins((
        NpcKernelPlugin::default(),
        SpatialPlugin,
        StraightLineMover,
        NavPlugin,
    ));
    app.add_plugins(JournalPlugin::default());
    app.finish();
    // A long wall at x = 0 with a closed door at z = 0.
    wall(&mut app, (-16..=15).filter(|&z| z != 0).map(|z| (0, z)));
    let door = CellInfo {
        flags: CellFlags {
            blocking: true,
            occluding: true,
            tags: 1 << DOOR_TAG,
        },
        extra_cost: 0,
    };
    app.world_mut()
        .resource_mut::<SpatialGrid>()
        .set_static(CellCoord::new(0, 0), door);

    let (from, to) = (SimPos::new(-4500, 500), SimPos::new(4500, 500));
    let (_, opener) = agent(&mut app, from, to, 100);
    app.world_mut().entity_mut(opener).insert(MayOpenDoors);
    let (other, _) = agent(&mut app, from, to, 100);
    run_ticks(app.world_mut(), 1);
    let path = app.world().get::<Path>(opener).unwrap().clone();
    assert_eq!(path.markers.len(), 1, "{path:?}");
    let m = path.markers[0];
    assert_eq!((m.cell, m.tag), (CellCoord::new(0, 0), DOOR_TAG));
    assert_eq!(path.waypoints[m.waypoint as usize], SimPos::new(500, 500));
    // Without permission the door blocks: the only way is closed.
    assert!(path_entries(&app).contains(&(
        0,
        other,
        JournalKind::PathFailed {
            reason: PathFailReason::Unreachable
        }
    )));
    // The opener waits in front of the closed door (wave E opens it).
    run_ticks(app.world_mut(), 80);
    assert!(matches!(status(&app, opener), MoveStatus::Blocked { .. }));
    let grid = app.world().resource::<SpatialGrid>();
    assert_eq!(grid.cell_of(pos(&app, opener)).x, -1);
    let opener_paths = path_entries(&app)
        .into_iter()
        .filter(|(_, u, _)| app.world().resource::<npc_kernel::SimIndex>().get(*u) == Some(opener))
        .count();
    assert_eq!(opener_paths, 1, "no replanning while waiting at a door");
}

// ------------------------------------------------- final review FX1 fixes

fn cell_centre(x: i32, z: i32) -> SimPos {
    SimPos::new(x * 1000 + 500, z * 1000 + 500)
}

/// Encloses cell (40, 40) with walls: a request for it must expand every
/// reachable cell of a 128 × 128 grid (~16k, more than the 4000 budget).
fn enclose_40_40(app: &mut App) {
    let mut cells = Vec::new();
    for x in 39..=41 {
        for z in 39..=41 {
            if (x, z) != (40, 40) {
                cells.push((x, z));
            }
        }
    }
    wall(app, cells);
}

/// A long head search (A, unreachable goal), a trivial request behind it
/// (B), and optionally a cart (`GridBlocker`) changing grid cells every tick
/// far away from both.
fn starvation_run(moving_cart: bool, ticks: u64) -> (App, Entity, Entity) {
    let mut app = app_with(128, 4000);
    enclose_40_40(&mut app);
    let (_, a) = agent(&mut app, cell_centre(0, 0), cell_centre(40, 40), 100);
    let (_, b) = agent(&mut app, cell_centre(-5, -5), cell_centre(-1, -5), 100);
    if moving_cart {
        let (_, cart) = spawn_sim(
            app.world_mut(),
            (cell_centre(-60, -60), GridBlocker::default()),
        );
        set_move_intent(
            app.world_mut(),
            cart,
            MoveIntent::to(cell_centre(60, -60), 1000),
        );
    }
    run_ticks(app.world_mut(), ticks);
    (app, a, b)
}

fn nav_state(app: &App, e: Entity) -> NavState {
    *app.world().get::<NavState>(e).unwrap()
}

/// Final review bug 1: with any cell changing every tick, the paused head
/// search restarted every tick and every request behind it stayed
/// `Pending` forever.
#[test]
fn nav_queue_does_not_starve_behind_a_long_search_under_grid_churn() {
    let (app, a, b) = starvation_run(false, 20);
    assert!(matches!(nav_state(&app, a), NavState::Failed { .. }));
    assert!(matches!(nav_state(&app, b), NavState::Following { .. }));

    let (mut app, a, b) = starvation_run(true, 100);
    let (sa, sb) = (nav_state(&app, a), nav_state(&app, b));
    assert!(
        matches!(sb, NavState::Following { .. }),
        "B starved behind A: A={sa:?} B={sb:?}"
    );
    assert!(
        !matches!(sa, NavState::Following { .. }),
        "A's goal is enclosed: {sa:?}"
    );
    // Deterministic.
    let (mut again, _, _) = starvation_run(true, 100);
    assert_eq!(
        state_hash(app.world_mut()),
        state_hash(again.world_mut()),
        "same run, same hash"
    );
}

/// Bug 1, first half: a paused search restarts only when a changed cell is
/// one it has looked at (opened, or next to an expanded cell); a change
/// elsewhere lets it resume.
#[test]
fn paused_search_resumes_unless_a_change_touches_what_it_saw() {
    let mut app = app_with(128, 1000);
    enclose_40_40(&mut app);
    let (_, _a) = agent(&mut app, cell_centre(0, 0), cell_centre(40, 40), 100);
    run_ticks(app.world_mut(), 1);
    let nodes = |app: &App| app.world().resource::<npc_nav::NavQueue>().paused_nodes();
    assert_eq!(nodes(&app), 1000, "paused after one budget");
    // A cell far away (the search expands nearest first) changes.
    wall(&mut app, [(-60, -60)]);
    run_ticks(app.world_mut(), 1);
    assert_eq!(nodes(&app), 2000, "resumed: the far change is not seen");
    // A cell next to the start (expanded) changes: restart from scratch.
    wall(&mut app, [(1, 1)]);
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        nodes(&app),
        1000,
        "restarted: the change touches the search"
    );
}

/// Bug 1, second half: the work one request may take over all its
/// restarts is capped, so it fails with `SearchLimit` instead of holding
/// the head of the queue forever.
#[test]
fn a_request_restarted_by_churn_fails_with_search_limit() {
    let mut app = app_with(128, 4000);
    enclose_40_40(&mut app);
    let (a, ae) = agent(&mut app, cell_centre(0, 0), cell_centre(40, 40), 100);
    // A blocker hopping right next to A's start every tick: every change is
    // seen, so the search restarts every tick.
    let (_, hop) = spawn_sim(app.world_mut(), (cell_centre(2, 2), GridBlocker::default()));
    for t in 0..40 {
        let p = if t % 2 == 0 {
            cell_centre(2, 3)
        } else {
            cell_centre(2, 2)
        };
        *app.world_mut().get_mut::<SimPos>(hop).unwrap() = p;
        run_ticks(app.world_mut(), 1);
    }
    let entries: Vec<_> = path_entries(&app)
        .into_iter()
        .filter(|(_, u, _)| *u == a)
        .collect();
    assert!(
        entries.iter().any(|(_, _, k)| *k
            == JournalKind::PathFailed {
                reason: PathFailReason::SearchLimit
            }),
        "{entries:?} / {:?}",
        nav_state(&app, ae)
    );
}

/// Final review bug 5: a `NavAgent` with its own `GridBlocker` (e.g. a
/// `Direct` vehicle) stood in a blocking cell and every request failed
/// `StartBlocked`. Its own footprint is open to its own search and mover.
#[test]
fn nav_agent_with_its_own_grid_blocker_plans_and_moves() {
    for footprint in [
        Footprint::default(),
        Footprint {
            half_x_mm: 900,
            half_z_mm: 900,
        },
    ] {
        let mut app = app_with(64, 4000);
        // A wall between start and goal: the path must go around it.
        wall(&mut app, (-3..=3).map(|z| (5, z)));
        let (_, e) = spawn_sim(
            app.world_mut(),
            (cell_centre(0, 0), NavAgent, GridBlocker(footprint)),
        );
        run_ticks(app.world_mut(), 1); // its blocker lands in the grid
        let goal = cell_centre(10, 0);
        set_move_intent(app.world_mut(), e, MoveIntent::to(goal, 200));
        let mut arrived = false;
        for _ in 0..400 {
            run_ticks(app.world_mut(), 1);
            let s = nav_state(&app, e);
            assert!(
                !matches!(s, NavState::Failed { .. }),
                "{footprint:?}: {s:?}"
            );
            let p = pos(&app, e);
            let grid = app.world().resource::<SpatialGrid>();
            assert!(
                !grid.static_cell(grid.cell_of(p)).flags.blocking,
                "inside the wall at {p:?}"
            );
            if status(&app, e) == MoveStatus::Arrived {
                arrived = true;
                break;
            }
        }
        assert!(arrived, "{footprint:?}: {:?}", status(&app, e));
        assert_eq!(pos(&app, e), goal);
    }
}

/// Another unit's blocker still blocks a blocker agent (only its own
/// footprint is open to it).
#[test]
fn a_blocker_agent_still_avoids_other_blockers() {
    let mut app = app_with(64, 4000);
    let (_, e) = spawn_sim(
        app.world_mut(),
        (cell_centre(0, 0), NavAgent, GridBlocker::default()),
    );
    spawn_sim(
        app.world_mut(),
        (cell_centre(10, 0), GridBlocker::default()),
    );
    run_ticks(app.world_mut(), 1);
    set_move_intent(app.world_mut(), e, MoveIntent::to(cell_centre(10, 0), 200));
    run_ticks(app.world_mut(), 2);
    assert_eq!(
        nav_state(&app, e),
        NavState::Failed {
            goal: cell_centre(10, 0),
            reason: PathFailReason::GoalBlocked
        }
    );
}

/// Snapshot load order is the registry's sorted name order (ADR 0013), and two
/// loads rely on it: the `brain` loader inserts `Brain` (whose required
/// components fill in only what is missing), so `blackboard` < `brain` <
/// `brain_state` decides which values survive; and `spatial_grid` clears the
/// `Messages<GridChanged>` that a `nav_queue` load may have left, so
/// `nav_queue` must load first. Renaming any of them must fail here, loudly.
#[test]
fn snapshot_load_order_pins_brain_and_grid_names() {
    let app = app();
    let registry = app
        .world()
        .resource::<npc_kernel::snapshot::SnapshotRegistry>();
    let components = registry.component_names();
    let resources = registry.resource_names();
    let pos = |names: &[&str], n: &str| {
        names
            .iter()
            .position(|x| *x == n)
            .unwrap_or_else(|| panic!("`{n}` is not registered: {names:?}"))
    };
    assert!(pos(&components, "blackboard") < pos(&components, "brain"));
    assert!(pos(&components, "brain") < pos(&components, "brain_state"));
    assert!(pos(&resources, "nav_queue") < pos(&resources, "spatial_grid"));
}

// ------------------------------------------- Task 25: why a walk is blocked

fn door_blocker(_: &World, cell: CellCoord) -> Option<Blocker> {
    (cell == CellCoord::new(0, 0)).then_some(Blocker::Door {
        id: SimId(77),
        locked: false,
    })
}

/// `explain_blocked` keeps the reachable cell closest to the goal and
/// names what is in the way: a door (by its hook), a wall, the world's
/// edge; nothing when the goal is reachable. It reads the world only.
#[test]
fn blocked_move_reports_nearest_reachable_and_blocker() {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(32, 32)));
    app.register_pass_cost(DOOR_TAG, door_cost)
        .register_cell_blocker(DOOR_TAG, door_blocker);
    app.add_plugins((
        NpcKernelPlugin::default(),
        SpatialPlugin,
        StraightLineMover,
        NavPlugin,
    ));
    app.finish();
    // A long wall at x = 0 with a closed door at z = 0.
    wall(&mut app, (-16..=15).filter(|&z| z != 0).map(|z| (0, z)));
    let door = CellInfo {
        flags: CellFlags {
            blocking: true,
            occluding: true,
            tags: 1 << DOOR_TAG,
        },
        extra_cost: 0,
    };
    app.world_mut()
        .resource_mut::<SpatialGrid>()
        .set_static(CellCoord::new(0, 0), door);
    let (_, e) = spawn_sim(app.world_mut(), (SimPos::new(-4500, 500), NavAgent));
    run_ticks(app.world_mut(), 1);
    let before = state_hash(app.world_mut());
    let report = explain_blocked(app.world(), e, SimPos::new(6500, 3500)).unwrap();
    assert_eq!(report.nearest_reachable, Some(SimPos::new(-500, 3500)));
    assert_eq!(
        report.blocker,
        Some(Blocker::Door {
            id: SimId(77),
            locked: false
        })
    );
    // An agent that may open it (a marked cell): the planner's rules reach
    // the goal through it, so the walk stalled there; the strict pass (a
    // closed door blocks) names the door it stalled at.
    app.world_mut().entity_mut(e).insert(MayOpenDoors);
    let report = explain_blocked(app.world(), e, SimPos::new(6500, 3500)).unwrap();
    assert_eq!(report.nearest_reachable, Some(SimPos::new(-500, 3500)));
    assert_eq!(
        report.blocker,
        Some(Blocker::Door {
            id: SimId(77),
            locked: false
        })
    );
    // The cell budget: past it there is no report.
    assert_eq!(
        explain_blocked_within(app.world(), e, SimPos::new(6500, 3500), 10),
        None
    );
    // A goal inside the wall: the wall cell.
    let report = explain_blocked(app.world(), e, SimPos::new(500, 8500)).unwrap();
    assert_eq!(report.nearest_reachable, Some(SimPos::new(-500, 8500)));
    assert_eq!(
        report.blocker,
        Some(Blocker::Wall {
            cell: [0, 8],
            pos: [500, 8500]
        })
    );
    // Past the edge: the nearest reachable cell towards it.
    let report = explain_blocked(app.world(), e, SimPos::new(-99_000, 500)).unwrap();
    assert_eq!(report.blocker, Some(Blocker::OutOfBounds));
    assert_eq!(report.nearest_reachable, Some(SimPos::new(-15_500, 500)));
    // A reachable goal has nothing to explain.
    assert_eq!(
        explain_blocked(app.world(), e, SimPos::new(-8500, 500)),
        None
    );
    assert_eq!(state_hash(app.world_mut()), before, "read-only");
}

const INNER_DOOR_TAG: u8 = 1;

fn locked_cost(_: &World, _: Entity, _: CellCoord, _: CellInfo) -> PassCost {
    PassCost::Blocked
}

fn inner_door_blocker(_: &World, cell: CellCoord) -> Option<Blocker> {
    (cell == CellCoord::new(4, 0)).then_some(Blocker::Door {
        id: SimId(78),
        locked: true,
    })
}

/// An unlocked outer door the agent may open, then a locked inner door: the
/// planner gets through the outer one, so the report names the inner door
/// and a nearest point inside the hallway between them.
#[test]
fn blocked_move_names_the_inner_door_past_an_openable_one() {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(32, 32)));
    app.register_pass_cost(DOOR_TAG, door_cost)
        .register_cell_blocker(DOOR_TAG, door_blocker)
        .register_pass_cost(INNER_DOOR_TAG, locked_cost)
        .register_cell_blocker(INNER_DOOR_TAG, inner_door_blocker);
    app.add_plugins((
        NpcKernelPlugin::default(),
        SpatialPlugin,
        StraightLineMover,
        NavPlugin,
    ));
    app.finish();
    // Outer wall at x = 0 (door at z = 0), inner wall at x = 4 (door at z = 0).
    wall(&mut app, (-16..=15).filter(|&z| z != 0).map(|z| (0, z)));
    wall(&mut app, (-16..=15).filter(|&z| z != 0).map(|z| (4, z)));
    let closed = |tag: u8| CellInfo {
        flags: CellFlags {
            blocking: true,
            occluding: true,
            tags: 1 << tag,
        },
        extra_cost: 0,
    };
    {
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        grid.set_static(CellCoord::new(0, 0), closed(DOOR_TAG));
        grid.set_static(CellCoord::new(4, 0), closed(INNER_DOOR_TAG));
    }
    let (_, e) = spawn_sim(
        app.world_mut(),
        (SimPos::new(-4500, 500), NavAgent, MayOpenDoors),
    );
    run_ticks(app.world_mut(), 1);
    let report = explain_blocked(app.world(), e, SimPos::new(8500, 3500)).unwrap();
    assert_eq!(
        report.blocker,
        Some(Blocker::Door {
            id: SimId(78),
            locked: true
        })
    );
    // The hallway (x = 1..=3) cell closest to the goal.
    assert_eq!(report.nearest_reachable, Some(SimPos::new(3500, 3500)));
}
