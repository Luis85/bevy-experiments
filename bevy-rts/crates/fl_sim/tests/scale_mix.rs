//! The scale scenario's entity mix (spec §5.7, ADR-0010): units, followers that track them,
//! and projectiles in flight through the spawn and despawn funnels; on the flat open map and on
//! the 2 km terrain map, where the units start on passable cells and wander along paths.

use std::collections::BTreeSet;
use std::path::PathBuf;

use bevy_app::App;
use fl_protocol::SimId;
use fl_sim::{
    Follower, MapInfo, MatchSetup, Pose, Projectile, RunConfig, SCALE_MAP, ScaleScenario,
    SetupError, SimClock, SimIndex, TerrainGrid, build_sim_app, load_content, step_tick,
};
use fl_test_utils::{Scenario, ScenarioBuilder, unit_cells};
use fl_view::TickInput;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A sim with core_mod loaded and `scale` populated, hashing every tick.
fn scale_sim(scale: &ScaleScenario) -> App {
    let content = load_content(&[], &[fixture("core_mod")]).expect("core_mod loads");
    let setup = MatchSetup {
        seed: scale.seed,
        cheats_allowed: false,
    };
    let run = RunConfig {
        hash_interval: Some(1),
        ..RunConfig::default()
    };
    let mut app = build_sim_app(&[], content, setup, run).expect("the sim builds");
    scale
        .populate(app.world_mut())
        .expect("the scale scenario sets up");
    app
}

/// Run one tick; its state hash.
fn step(app: &mut App) -> Option<u64> {
    let tick = app.world().resource::<SimClock>().tick;
    step_tick(
        app.world_mut(),
        TickInput {
            tick,
            commands: Vec::new(),
        },
    )
    .hash
}

fn run(app: &mut App, ticks: u32) {
    for _ in 0..ticks {
        step(app);
    }
}

fn count<C: bevy_ecs::component::Component>(app: &mut App) -> usize {
    let world = app.world_mut();
    let mut query = world.query::<&C>();
    query.iter(world).count()
}

#[test]
fn scale_mix_entity_count() {
    let mut app = scale_sim(&ScaleScenario::with_units(3000));
    run(&mut app, 40);
    let entities = app.world().resource::<SimIndex>().len();
    assert!(
        (8_000..=12_000).contains(&entities),
        "{entities} sim entities after the warmup"
    );
    assert_eq!(count::<Follower>(&mut app), 6000);
}

#[test]
fn projectiles_steady_state() {
    let mut app = scale_sim(&ScaleScenario::with_units(3000));
    run(&mut app, 40);
    for _ in 0..20 {
        step(&mut app);
        let in_flight = count::<Projectile>(&mut app);
        assert!(
            (900..=1100).contains(&in_flight),
            "{in_flight} projectiles in flight, not within 10% of 1000"
        );
    }
}

#[test]
fn followers_track_leaders() {
    let mut app = scale_sim(&ScaleScenario::with_units(100));
    let map = app.world().resource::<MapInfo>().clone();
    // Every follower's position after setup and after each tick; the positions, in query
    // order.
    let check = |app: &mut App| -> Vec<Pose> {
        let world = app.world_mut();
        let mut followers = world.query::<(&Follower, &Pose)>();
        let followers: Vec<(Follower, Pose)> =
            followers.iter(world).map(|(f, p)| (*f, *p)).collect();
        assert_eq!(followers.len(), 200);
        let index = world.resource::<SimIndex>();
        for (follower, pose) in &followers {
            let leader = index
                .get(follower.leader)
                .expect("leaders are never despawned");
            let leader = world.get::<Pose>(leader).expect("leader pose").pos;
            assert_eq!(pose.pos, map.clamp(leader + follower.offset));
        }
        followers.into_iter().map(|(_, pose)| pose).collect()
    };
    let start = check(&mut app);
    let mut end = start.clone();
    for _ in 0..100 {
        step(&mut app);
        end = check(&mut app);
    }
    let moved = start.iter().zip(&end).filter(|(a, b)| a != b).count();
    assert!(
        moved > 100,
        "followers moved with their units ({moved} moved)"
    );
}

#[test]
fn scale_mix_twin_run() {
    let hashes = || {
        let mut s = on_terrain(500, 1);
        s.run_ticks(200);
        s.hashes()
    };
    let a = hashes();
    assert_eq!(a.len(), 200, "hashed every tick");
    assert_eq!(a, hashes());
}

/// [`ScaleScenario::on_terrain`]`(units)` on the base content, a state hash every
/// `hash_interval` ticks.
fn on_terrain(units: u32, hash_interval: u32) -> Scenario {
    ScenarioBuilder::new()
        .scale(ScaleScenario::on_terrain(units))
        .hash_interval(hash_interval)
        .build()
}

/// The units (entities with `Mobility`) whose cell their movement profile cannot stand on, by
/// id.
fn units_on_blocked_cells(s: &mut Scenario) -> Vec<SimId> {
    unit_cells(s.world_mut())
        .into_iter()
        .filter(|unit| !unit.passable)
        .map(|unit| unit.id)
        .collect()
}

/// The ids of every unit (entity with `Mobility`).
fn unit_ids(s: &mut Scenario) -> BTreeSet<SimId> {
    unit_cells(s.world_mut())
        .into_iter()
        .map(|unit| unit.id)
        .collect()
}

#[test]
fn scale_units_start_on_passable_cells() {
    let mut s = on_terrain(3000, 20);
    assert_eq!(unit_ids(&mut s).len(), 3000);
    // The map has cells some unit cannot stand on (rivers, swamps, deep forest), so placement
    // had something to avoid.
    let terrain = s.world().resource::<TerrainGrid>();
    let palette = &terrain.palette;
    assert!(
        terrain.types.iter().any(|&t| palette
            .get(usize::from(t))
            .is_some_and(|n| n == "base.water")),
        "the scale map has water: {palette:?}"
    );
    assert_eq!(units_on_blocked_cells(&mut s), Vec::<SimId>::new());
}

#[test]
fn scale_pathing_reaches_targets() {
    let mut s = on_terrain(500, 20);
    let units = unit_ids(&mut s);
    assert_eq!(units.len(), 500);
    for _ in 0..400 {
        s.run_ticks(1);
        let blocked = units_on_blocked_cells(&mut s);
        assert!(
            blocked.is_empty(),
            "tick {:?}: units on blocked cells: {blocked:?}",
            s.tick()
        );
    }
    let planned: BTreeSet<SimId> = s
        .journal()
        .iter()
        .filter(|e| e.kind == "core.path_result" && e.data["status"] != "stale")
        .filter_map(|e| e.sim_id)
        .collect();
    let missing: Vec<&SimId> = units.difference(&planned).collect();
    assert!(
        missing.is_empty(),
        "{} units got no path result in 400 ticks: {missing:?}",
        missing.len()
    );
}

#[test]
fn flat_scale_has_open_terrain() {
    let app = scale_sim(&ScaleScenario::default());
    let map = app.world().resource::<MapInfo>();
    assert_eq!((map.cells_x, map.cells_y), (500, 500));
    assert_eq!(
        app.world().resource::<TerrainGrid>(),
        &TerrainGrid::open(map.cells_x, map.cells_y)
    );
}

#[test]
fn scale_map_size_must_match() {
    let mut s = ScenarioBuilder::new()
        .seed(ScaleScenario::default().seed)
        .build();
    let scale = ScaleScenario {
        map_m: 1000,
        ..ScaleScenario::on_terrain(100)
    };
    assert_eq!(
        scale.populate(s.world_mut()),
        Err(SetupError::MapSizeMismatch {
            map: SCALE_MAP.to_owned(),
            map_m: 1000,
        })
    );
    assert!(
        s.world().get_resource::<MapInfo>().is_none(),
        "the world is unchanged"
    );
}
