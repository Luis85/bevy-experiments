//! Plan Task C1: the kernel's spatial grid (queries, index maintenance in
//! `NpcSet::Inputs`, blockers and `GridChanged`).

use bevy_app::App;
use bevy_ecs::message::Messages;
use bevy_ecs::prelude::*;
use npc_kernel::spatial::{
    CellCoord, CellFlags, CellInfo, Footprint, GridBlocker, GridBounds, GridChanged, SpatialGrid,
    SpatialPlugin,
};
use npc_kernel::{NpcKernelPlugin, SimCommand, SimId, SimPos, run_ticks, spawn_sim, submit};

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((NpcKernelPlugin::default(), SpatialPlugin));
    app.finish();
    app.cleanup();
    app
}

fn grid(app: &App) -> &SpatialGrid {
    app.world().resource::<SpatialGrid>()
}

#[test]
fn cell_of_and_center_round_trip() {
    let g = SpatialGrid::default();
    assert_eq!(g.cell_mm(), 1000);
    assert_eq!(g.cell_of(SimPos::new(0, 0)), CellCoord::new(0, 0));
    assert_eq!(g.cell_of(SimPos::new(999, 1000)), CellCoord::new(0, 1));
    // Floor division: negative coordinates go to the lower cell.
    assert_eq!(g.cell_of(SimPos::new(-1, -1000)), CellCoord::new(-1, -1));
    assert_eq!(g.cell_of(SimPos::new(-1001, 0)), CellCoord::new(-2, 0));
    assert_eq!(g.center(CellCoord::new(0, 0)), SimPos::new(500, 500));
    assert_eq!(g.center(CellCoord::new(-1, 2)), SimPos::new(-500, 2500));
    for c in [CellCoord::new(3, -7), CellCoord::new(-64, 63)] {
        assert_eq!(g.cell_of(g.center(c)), c);
    }
}

#[test]
fn queries_are_sorted_and_exact() {
    let mut app = app();
    let w = app.world_mut();
    // Spawned in a scrambled order so SimIds and positions do not correlate.
    let (far, _) = spawn_sim(w, SimPos::new(9000, 9000));
    let (edge, _) = spawn_sim(w, SimPos::new(3000, 4000)); // exactly 5 m away
    let (near_b, _) = spawn_sim(w, SimPos::new(-200, 100));
    let (near_a, _) = spawn_sim(w, SimPos::new(100, -200));
    let (just_out, _) = spawn_sim(w, SimPos::new(3001, 4000));
    run_ticks(app.world_mut(), 1);
    let g = grid(&app);
    assert_eq!(g.len(), 5);

    let hits = g.in_radius(SimPos::new(0, 0), 5000);
    let mut want = vec![edge, near_b, near_a];
    want.sort();
    assert_eq!(hits, want, "in_radius: inclusive, sorted by SimId");
    assert!(!hits.contains(&just_out) && !hits.contains(&far));
    assert!(hits.windows(2).all(|w| w[0] < w[1]));

    // A huge radius falls back to a scan of every member: same result.
    let all = g.in_radius(SimPos::new(0, 0), i32::MAX / 2);
    let mut every = vec![far, edge, near_b, near_a, just_out];
    every.sort();
    assert_eq!(all, every);

    let boxed = g.in_box(SimPos::new(-200, -200), SimPos::new(3000, 4000));
    let mut want = vec![edge, near_b, near_a];
    want.sort();
    assert_eq!(boxed, want, "in_box: inclusive bounds, sorted");
    assert_eq!(g.position_of(edge), Some(SimPos::new(3000, 4000)));
}

#[test]
fn index_follows_movement_and_despawn() {
    let mut app = app();
    let (a, ea) = spawn_sim(app.world_mut(), SimPos::new(500, 500));
    let (b, _) = spawn_sim(app.world_mut(), SimPos::new(1500, 500));
    run_ticks(app.world_mut(), 1);
    assert_eq!(grid(&app).ids_in_cell(CellCoord::new(0, 0)), &[a]);

    // Movement is picked up in the next tick's Inputs.
    *app.world_mut().get_mut::<SimPos>(ea).unwrap() = SimPos::new(1600, 400);
    assert_eq!(
        grid(&app).ids_in_cell(CellCoord::new(0, 0)),
        &[a],
        "lags a tick"
    );
    run_ticks(app.world_mut(), 1);
    assert!(grid(&app).ids_in_cell(CellCoord::new(0, 0)).is_empty());
    let mut both = vec![a, b];
    both.sort();
    assert_eq!(
        grid(&app).ids_in_cell(CellCoord::new(1, 0)),
        both.as_slice()
    );

    // Despawn through the command bus (as replays do).
    submit(app.world_mut(), None, SimCommand::Despawn { unit: a.0 }).unwrap();
    run_ticks(app.world_mut(), 1);
    assert_eq!(grid(&app).ids_in_cell(CellCoord::new(1, 0)), &[b]);
    assert_eq!(grid(&app).position_of(a), None);
    assert_eq!(grid(&app).len(), 1);

    // Plain despawn and SimPos removal also leave the index.
    let eb = app
        .world()
        .resource::<npc_kernel::SimIndex>()
        .get(b)
        .unwrap();
    app.world_mut().entity_mut(eb).remove::<SimPos>();
    run_ticks(app.world_mut(), 1);
    assert_eq!(grid(&app).len(), 0);
}

#[test]
fn blockers_mark_footprints_and_emit_grid_changed() {
    let mut app = app();
    run_ticks(app.world_mut(), 1);
    assert!(
        !grid(&app).has_blocking(),
        "no blockers: nothing is blocking"
    );

    // A 3 m × 1 m wall centred on (2500, 500): cells x 1..=3, z 0.
    let (_, wall) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(2500, 500),
            GridBlocker(Footprint {
                half_x_mm: 1500,
                half_z_mm: 0,
            }),
        ),
    );
    run_ticks(app.world_mut(), 1);
    let g = grid(&app);
    assert!(g.has_blocking());
    for x in 1..=4 {
        assert!(g.is_blocking(CellCoord::new(x, 0)), "x={x}");
    }
    assert!(!g.is_blocking(CellCoord::new(0, 0)));
    assert!(!g.is_blocking(CellCoord::new(5, 0)));
    let changed = last_changed(&app);
    assert_eq!(
        changed,
        (1..=4).map(|x| CellCoord::new(x, 0)).collect::<Vec<_>>(),
        "sorted list of every cell whose effective flags changed"
    );

    // Nothing changes: no new message.
    run_ticks(app.world_mut(), 1);
    assert!(changed_this_tick(&app).is_empty());

    // Removing the blocker frees its cells (and reports them).
    app.world_mut().entity_mut(wall).remove::<GridBlocker>();
    run_ticks(app.world_mut(), 1);
    assert!(!grid(&app).has_blocking());
    assert_eq!(changed_this_tick(&app).len(), 4);
}

#[test]
fn static_cells_and_trace_segment() {
    let mut g = SpatialGrid::new(1000, GridBounds::centered(16, 16));
    let wall = CellInfo {
        flags: CellFlags {
            blocking: true,
            occluding: true,
            tags: 0,
        },
        extra_cost: 0,
    };
    g.set_static(CellCoord::new(2, 0), wall);
    assert!(g.is_blocking(CellCoord::new(2, 0)) && g.is_occluding(CellCoord::new(2, 0)));
    assert!(g.has_blocking());
    // Out of bounds: open ground.
    assert_eq!(g.cell(CellCoord::new(100, 0)), CellInfo::default());

    let mut seen = Vec::new();
    g.trace_segment(SimPos::new(500, 500), SimPos::new(3500, 500), |c| {
        seen.push(c);
        true
    });
    assert_eq!(
        seen,
        (0..=3).map(|x| CellCoord::new(x, 0)).collect::<Vec<_>>()
    );

    // Exactly through a corner: both side cells are visited (conservative).
    let mut seen = Vec::new();
    g.trace_segment(SimPos::new(500, 500), SimPos::new(1500, 1500), |c| {
        seen.push(c);
        true
    });
    assert_eq!(
        seen,
        vec![
            CellCoord::new(0, 0),
            CellCoord::new(1, 0),
            CellCoord::new(0, 1),
            CellCoord::new(1, 1)
        ]
    );
    // Early stop.
    let clear = g.trace_segment(SimPos::new(500, 500), SimPos::new(5500, 500), |c| {
        !g.is_blocking(c)
    });
    assert!(!clear);
    // Reverse direction and negative cells end on the end cell.
    let mut last = None;
    g.trace_segment(SimPos::new(-1, -1), SimPos::new(-2500, -700), |c| {
        last = Some(c);
        true
    });
    assert_eq!(last, Some(CellCoord::new(-3, -1)));
}

#[test]
fn ids_sorted_after_many_moves() {
    let mut app = app();
    let mut ids: Vec<(SimId, Entity)> = (0..20)
        .map(|i| spawn_sim(app.world_mut(), SimPos::new(i * 37 % 900, i * 53 % 900)))
        .collect();
    run_ticks(app.world_mut(), 1);
    ids.reverse();
    for &(_, e) in &ids {
        app.world_mut().get_mut::<SimPos>(e).unwrap().x += 1;
    }
    run_ticks(app.world_mut(), 1);
    let cell = grid(&app).ids_in_cell(CellCoord::new(0, 0)).to_vec();
    let mut sorted = cell.clone();
    sorted.sort();
    assert_eq!(cell, sorted);
    assert_eq!(cell.len(), 20);
}

/// Cells of the `GridChanged` messages written this tick.
fn changed_this_tick(app: &App) -> Vec<CellCoord> {
    let messages = app.world().resource::<Messages<GridChanged>>();
    let mut cursor = messages.get_cursor();
    // Messages of the previous tick are still buffered; keep the newest tick's.
    let mut out = Vec::new();
    for m in cursor.read(messages) {
        if m.tick + 1 == app.world().resource::<npc_kernel::SimClock>().tick {
            out.extend(m.cells.iter().copied());
        }
    }
    out
}

fn last_changed(app: &App) -> Vec<CellCoord> {
    changed_this_tick(app)
}

/// Final review cleanup 8: a footprint's size was not clamped, so a huge
/// one iterated (and allocated) far more cells than the grid has.
#[test]
fn huge_footprints_are_clamped_to_the_grid() {
    let g = SpatialGrid::new(1000, GridBounds::centered(32, 32));
    let huge = Footprint {
        half_x_mm: 2_000_000,
        half_z_mm: 2_000_000,
    };
    let cells = g.footprint_cells(SimPos::new(0, 0), huge);
    assert!(cells.len() <= 65 * 65, "{} cells", cells.len());
    // In the grid, it blocks exactly the in-bounds cells.
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(32, 32)));
    app.add_plugins((NpcKernelPlugin::default(), SpatialPlugin));
    app.finish();
    app.cleanup();
    spawn_sim(app.world_mut(), (SimPos::new(0, 0), GridBlocker(huge)));
    run_ticks(app.world_mut(), 1);
    let g = grid(&app);
    let bounds = g.bounds();
    let blocking = (0..bounds.len() as u32)
        .filter(|&i| g.cell_at(i).flags.blocking)
        .count();
    assert_eq!(blocking, bounds.len());
}

/// Final review cleanup 6: `Brain.tree` joins the state hash only when an
/// app opts in (`register_brain_hash`): hashing it by default would change
/// every village hash and replay.
#[test]
fn brain_tree_is_hashed_only_when_opted_in() {
    use npc_kernel::hash::state_hash;
    use npc_kernel::registry::KernelAppExt;
    fn run(opt_in: bool, tree: &'static str) -> u64 {
        let mut app = App::new();
        app.add_plugins(NpcKernelPlugin::default())
            .register_action("Idle", |_, _, _| npc_ai::Status::Running, |_, _, _| {})
            .register_tree("a", |b| {
                b.leaf("Idle", "Idle", ());
            })
            .register_tree("b", |b| {
                b.leaf("Idle", "Idle", ());
            });
        if opt_in {
            npc_kernel::hash::register_brain_hash(&mut app);
        }
        app.finish();
        app.cleanup();
        let id = app
            .world()
            .resource::<npc_ai::AiRegistry>()
            .tree_id(tree)
            .unwrap();
        spawn_sim(
            app.world_mut(),
            (SimPos::new(0, 0), npc_ai::Brain { tree: id }),
        );
        state_hash(app.world_mut())
    }
    assert_eq!(run(false, "a"), run(false, "b"), "default: not hashed");
    assert_ne!(run(true, "a"), run(true, "b"), "opted in: hashed");
}
