//! The incremental spatial grid: cell math, sorted cells, the broad phase against brute
//! force, tracking wandering units, despawns, the rebuild after a load and the work counters.

use std::path::PathBuf;

use bevy_app::App;
use bevy_ecs::prelude::*;
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{DebugSpawnCmd, PlayerCommand, PlayerId, SimId};
use fl_sim::{
    CommandRegistry, MapInfo, MatchSetup, Pose, RunConfig, ScaleScenario, SimClock, SimCounters,
    SimEntity, SimIndex, SpatialCell, SpatialGrid, build_sim_app, load_content, load_world,
    save_world, setup_despawn, step_tick,
};
use fl_view::TickInput;
use proptest::prelude::*;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

fn map(side: i32) -> MapInfo {
    MapInfo::flat(v(side, side), Fx::from_int(4))
}

fn run_config() -> RunConfig {
    RunConfig {
        hash_interval: Some(1),
        ..RunConfig::default()
    }
}

fn scale_sim(units: u32) -> App {
    scale_sim_cheats(units, false)
}

fn scale_sim_cheats(units: u32, cheats_allowed: bool) -> App {
    let content = load_content(&[], &[fixture("core_mod")]).expect("core_mod loads");
    let setup = MatchSetup {
        seed: 3,
        cheats_allowed,
    };
    let mut app = build_sim_app(&[], content, setup, run_config()).expect("the sim builds");
    // Units only: no followers, nothing fired, no sensing.
    let scenario = ScaleScenario {
        units,
        seed: 3,
        followers_per_unit: 0,
        projectiles: 0,
        sensor_every: 0,
        ..ScaleScenario::default()
    };
    scenario
        .populate(app.world_mut())
        .expect("the scale scenario sets up");
    app
}

fn step(app: &mut App, ticks: u32) {
    for _ in 0..ticks {
        let tick = app.world().resource::<SimClock>().tick;
        step_tick(
            app.world_mut(),
            TickInput {
                tick,
                commands: Vec::new(),
            },
        );
    }
}

#[test]
fn insert_keeps_cells_sorted() {
    let mut grid = SpatialGrid::new(&map(1000), Fx::from_int(32));
    for id in [5, 1, 9, 3, 7] {
        grid.insert(SimId(id), 10);
    }
    grid.insert(SimId(3), 10);
    let ids: Vec<u32> = grid.members(10).iter().map(|id| id.0).collect();
    assert_eq!(ids, [1, 3, 5, 7, 9]);
    assert_eq!(grid.len(), 5);
}

#[test]
fn position_on_max_edge_maps_to_last_cell() {
    let grid = SpatialGrid::new(&map(1000), Fx::from_int(32));
    // 1000 / 32 = 31.25: 32 columns and rows, the last one partial.
    let last = 32 * 32 - 1;
    assert_eq!(grid.cell_of(v(1000, 1000)), last);
    assert_eq!(grid.cell_of(v(5000, 5000)), last);
    assert_eq!(grid.cell_of(v(-5, -5)), 0);
    assert_eq!(grid.cell_of(v(31, 0)), 0);
    assert_eq!(grid.cell_of(v(32, 0)), 1);
    assert_eq!(grid.cell_of(v(0, 32)), 32);
    let just_inside = FxVec2::new(Fx::from_int(1000) - Fx::EPSILON, Fx::ZERO);
    assert_eq!(grid.cell_of(just_inside), 31);
}

#[test]
fn relocate_moves_between_cells() {
    let mut grid = SpatialGrid::new(&map(1000), Fx::from_int(32));
    grid.insert(SimId(1), 3);
    grid.insert(SimId(2), 3);
    grid.relocate(SimId(1), 3, 40);
    assert_eq!(grid.members(3), [SimId(2)]);
    assert_eq!(grid.members(40), [SimId(1)]);
    assert_eq!(grid.len(), 2);
    assert!(!grid.remove(SimId(1), 3));
    assert!(grid.remove(SimId(1), 40));
    assert_eq!(grid.len(), 1);
}

proptest! {
    #[test]
    fn query_radius_matches_brute_force(
        points in prop::collection::vec((0..2000i32, 0..2000i32), 1..120),
        cx in 0..2000i32,
        cy in 0..2000i32,
        radius in 1..300i32,
    ) {
        let mut grid = SpatialGrid::new(&map(2000), Fx::from_int(32));
        let mut placed = Vec::new();
        for (i, &(x, y)) in points.iter().enumerate() {
            let id = SimId(u32::try_from(i).unwrap() + 1);
            let pos = v(x, y);
            grid.insert(id, grid.cell_of(pos));
            placed.push((id, pos));
        }
        let center = v(cx, cy);
        let r = Fx::from_int(radius);
        let mut out = vec![SimId(999_999)];
        let candidates = grid.query_radius(center, r, &mut out);

        prop_assert_eq!(usize::try_from(candidates).unwrap(), out.len());
        prop_assert!(out.windows(2).all(|w| w[0] < w[1]), "sorted and unique");
        let inside: Vec<SimId> = placed
            .iter()
            .filter(|(_, pos)| (*pos - center).dot(*pos - center) <= r * r)
            .map(|(id, _)| *id)
            .collect();
        for id in &inside {
            prop_assert!(out.contains(id), "{:?} inside the circle but not a candidate", id);
        }
    }
}

#[test]
fn grid_tracks_wandering_units() {
    let mut app = scale_sim(500);
    step(&mut app, 300);
    let world = app.world_mut();
    let grid = world.resource::<SpatialGrid>().clone();
    assert_eq!(grid.len(), 500);
    let index: Vec<(SimId, Entity)> = world.resource::<SimIndex>().iter().collect();
    assert_eq!(index.len(), 500);
    for (id, entity) in index {
        let pose = *world.get::<Pose>(entity).expect("pose");
        let cell = world.get::<SpatialCell>(entity).expect("cell").0;
        assert_eq!(cell, grid.cell_of(pose.pos), "{id:?}");
        assert!(grid.members(cell).contains(&id), "{id:?} is in its cell");
    }
    let total: usize = (0..grid.cols() * grid.rows())
        .map(|c| grid.members(c).len())
        .sum();
    assert_eq!(total, 500, "every unit is in exactly one cell");
}

#[test]
fn despawn_removes_from_grid() {
    let mut app = scale_sim(20);
    step(&mut app, 3);
    assert_eq!(app.world().resource::<SpatialGrid>().len(), 20);
    let victim = SimId(7);
    let entity = app.world().resource::<SimIndex>().get(victim).unwrap();
    let cell = app.world().get::<SpatialCell>(entity).unwrap().0;
    assert!(
        app.world()
            .resource::<SpatialGrid>()
            .members(cell)
            .contains(&victim)
    );

    setup_despawn(app.world_mut(), victim);
    let grid = app.world().resource::<SpatialGrid>();
    assert_eq!(grid.len(), 19);
    assert!(!grid.members(cell).contains(&victim));
    step(&mut app, 2);
    assert_eq!(app.world().resource::<SpatialGrid>().len(), 19);
}

#[test]
fn grid_rebuilt_after_load() {
    let mut app = scale_sim(200);
    step(&mut app, 100);
    let bytes = save_world(app.world(), &[]).expect("saves");
    let live = app.world().resource::<SpatialGrid>().clone();

    let content = load_content(&[], &[fixture("core_mod")]).expect("core_mod loads");
    let loaded = match load_world(&[], content, &bytes, run_config()) {
        Ok(loaded) => loaded,
        Err(err) => panic!("the save loads: {err}"),
    };
    let mut loaded_app = loaded.app;
    assert_eq!(loaded_app.world().resource::<SpatialGrid>(), &live);
    let world = loaded_app.world_mut();
    let mut cells = world.query::<(&SimEntity, &SpatialCell)>();
    assert_eq!(cells.iter(world).count(), 200);

    // And it keeps tracking in step with the live sim.
    step(&mut app, 20);
    step(&mut loaded_app, 20);
    assert_eq!(
        loaded_app.world().resource::<SpatialGrid>(),
        app.world().resource::<SpatialGrid>()
    );
}

#[test]
fn spatial_counters_recorded() {
    let mut app = scale_sim(64);
    assert_eq!(
        app.world().resource::<SpatialGrid>().len(),
        64,
        "filed at spawn"
    );
    step(&mut app, 100);
    let mut relocations = 0;
    for _ in 0..100 {
        step(&mut app, 1);
        let counters = app.world().resource::<SimCounters>();
        assert_eq!(counters.get("spatial.inserts"), 0);
        // The tracker's scan visits every indexed entity, every tick.
        assert_eq!(counters.get("spatial.checked"), 64);
        relocations += counters.get("spatial.relocations");
    }
    assert!(relocations > 0, "wandering units change cells");
}

#[test]
fn spawn_tick_save_loads_to_the_live_grid() {
    let mut app = scale_sim_cheats(50, true);
    step(&mut app, 5);
    let tick = app.world().resource::<SimClock>().tick;
    let spawn = DebugSpawnCmd {
        unit: "core_test.tank".to_owned(),
        owner: PlayerId(1),
        pos: v(600, 600),
        heading: Angle::ZERO,
    };
    let kind = app.world().resource::<CommandRegistry>().encode(&spawn);
    let command = PlayerCommand {
        exec_tick: tick,
        player: PlayerId(1),
        seq: 1,
        units: Vec::new(),
        kind,
        queue: false,
    };
    step_tick(
        app.world_mut(),
        TickInput {
            tick,
            commands: vec![command],
        },
    );
    assert_eq!(
        app.world().resource::<SimCounters>().get("spatial.inserts"),
        1
    );
    assert_eq!(app.world().resource::<SpatialGrid>().len(), 51);
    let bytes = save_world(app.world(), &[]).expect("saves");

    let content = load_content(&[], &[fixture("core_mod")]).expect("core_mod loads");
    let loaded = match load_world(&[], content, &bytes, run_config()) {
        Ok(loaded) => loaded,
        Err(err) => panic!("the save loads: {err}"),
    };
    let mut loaded_app = loaded.app;
    assert_eq!(
        loaded_app.world().resource::<SpatialGrid>(),
        app.world().resource::<SpatialGrid>()
    );
    for _ in 0..10 {
        step(&mut app, 1);
        step(&mut loaded_app, 1);
        assert_eq!(
            loaded_app.world().resource::<SpatialGrid>(),
            app.world().resource::<SpatialGrid>()
        );
    }
}
