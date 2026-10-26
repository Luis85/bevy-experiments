//! Unit components, spawning, movement, wandering, scenarios and the scale scenario.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::prelude::*;
use fl_content::ScenarioDef;
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{MoveCmd, PlayerCommand, PlayerId, SimId, Tick, UnitTypeId};
use fl_sim::{
    BuildError, CommandRegistry, Content, MapInfo, MatchSeed, MatchSetup, Mobility, MoveOrder,
    Owner, Players, Pose, RunConfig, ScaleScenario, SetupError, SimBuilder, SimClock, SimEntity,
    SimFeature, SimIndex, SpawnError, SpawnRequest, ThinkSet, Wander, build_sim_app, load_content,
    rolls, setup_despawn, setup_map, setup_scenario, setup_scenario_def, setup_spawn, step_tick,
};
use fl_view::TickInput;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A sim with core_mod loaded and nothing set up.
fn empty_sim() -> App {
    sim_with(Vec::new())
}

/// A sim with `features` and core_mod loaded, seed 1, nothing set up.
fn sim_with(features: Vec<Arc<dyn SimFeature>>) -> App {
    sim_seeded(features, 1)
}

/// A sim with `features`, core_mod loaded and seed `seed`, nothing set up.
fn sim_seeded(features: Vec<Arc<dyn SimFeature>>, seed: u64) -> App {
    let content = load_content(&features, &[fixture("core_mod")]).expect("core_mod loads");
    let setup = MatchSetup {
        seed,
        cheats_allowed: false,
    };
    build_sim_app(&features, content, setup, RunConfig::default()).expect("the sim builds")
}

/// The duel: #1 (player 0) at (100, 100) heading 0, #2 (player 1) at (900, 900).
fn duel() -> App {
    let mut app = empty_sim();
    setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
    app
}

fn step(app: &mut App, ticks: u32) {
    for _ in 0..ticks {
        step_with(app, Vec::new());
    }
}

/// Run one tick with `commands` in its input.
fn step_with(app: &mut App, commands: Vec<PlayerCommand>) {
    let tick = app.world().resource::<SimClock>().tick;
    step_tick(app.world_mut(), TickInput { tick, commands });
}

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

fn tank(app: &App) -> UnitTypeId {
    let content = app.world().resource::<Content>().0.clone();
    content.unit_id("core_test.tank").expect("tank is concrete")
}

fn entity_of(app: &App, id: u32) -> bevy_ecs::entity::Entity {
    app.world()
        .resource::<SimIndex>()
        .get(SimId(id))
        .expect("unit exists")
}

fn pose(app: &App, id: u32) -> Pose {
    *app.world()
        .get::<Pose>(entity_of(app, id))
        .expect("unit has a Pose")
}

/// Player 0's move of `unit` to `target`, for the current tick.
fn move_cmd(app: &App, unit: u32, target: FxVec2) -> PlayerCommand {
    let tick = app.world().resource::<SimClock>().tick;
    let kind = app
        .world()
        .resource::<CommandRegistry>()
        .encode(&MoveCmd { target });
    PlayerCommand {
        exec_tick: tick,
        player: PlayerId(0),
        seq: 0,
        units: vec![SimId(unit)],
        kind,
        queue: false,
    }
}

/// A request for a tank at `pos`, facing 0.
fn tank_at(app: &App, player: u8, pos: FxVec2) -> SpawnRequest {
    SpawnRequest {
        unit: Some(tank(app)),
        owner: PlayerId(player),
        pos,
        heading: Angle::ZERO,
        init: None,
    }
}

#[test]
fn kmh_converts_to_m_per_tick() {
    let app = duel();
    let mobility = app
        .world()
        .get::<Mobility>(entity_of(&app, 1))
        .expect("tank has Mobility");
    assert_eq!(mobility.speed, Fx::from_int(36) / Fx::from_int(72));
    assert_eq!(mobility.speed, Fx::HALF);
    // 90°/s at 20 Hz is 4.5° per tick.
    assert_eq!(
        mobility.turn_rate,
        Angle::from_degrees(Fx::from_ratio(9, 2))
    );
    assert_eq!(mobility.terrain_factor, Fx::HALF);
}

#[test]
fn tank_turns_before_moving() {
    let mut app = duel();
    let mut pending = vec![move_cmd(&app, 1, v(50, 100))];
    let mut heading = pose(&app, 1).heading;
    for _ in 0..10 {
        step_with(&mut app, std::mem::take(&mut pending));
        let now = pose(&app, 1);
        assert_eq!(now.pos, v(100, 100), "turning in place");
        assert_ne!(now.heading, heading, "heading changes every tick");
        heading = now.heading;
    }
    step(&mut app, 300);
    assert_eq!(pose(&app, 1).pos, v(50, 100));
    assert_eq!(pose(&app, 1).heading, Angle::HALF);
}

#[test]
fn move_target_outside_map_is_clamped() {
    let mut app = duel();
    // Off the map to the south-east: the drive to the clamped corner passes nowhere near #2
    // at (900, 900), which separation would push ahead of #1 into the corner (Task 11).
    let cmd = move_cmd(&app, 1, v(5000, -5000));
    let order = app
        .world()
        .get::<MoveOrder>(entity_of(&app, 1))
        .cloned()
        .unwrap_or_default();
    // Commands apply during the tick; nothing queued yet.
    assert!(order.orders.is_empty());
    step_with(&mut app, vec![cmd]);
    step(&mut app, 2999);
    let map = app.world().resource::<MapInfo>().clone();
    let pos = pose(&app, 1).pos;
    assert!(map.contains(pos), "{pos:?} is on the map");
    assert_eq!(
        pos,
        map.clamp(v(5000, -5000)),
        "arrived at the clamped target"
    );
    let order = app
        .world()
        .get::<MoveOrder>(entity_of(&app, 1))
        .expect("tank has a MoveOrder");
    assert!(order.orders.is_empty());
}

#[test]
fn spawn_outside_map_is_error() {
    let mut app = duel();
    for pos in [v(1000, 0), v(0, 1000), v(-1, 5), v(5, -1), v(2000, 2000)] {
        let request = tank_at(&app, 0, pos);
        assert_eq!(
            setup_spawn(app.world_mut(), &request),
            Err(SpawnError::OutsideMap),
            "{pos:?}"
        );
    }
    let edge = FxVec2::new(Fx::from_int(1000) - Fx::EPSILON, Fx::ZERO);
    let request = tank_at(&app, 0, edge);
    assert_eq!(setup_spawn(app.world_mut(), &request), Ok(SimId(3)));
    let request = tank_at(&app, 9, v(5, 5));
    assert_eq!(
        setup_spawn(app.world_mut(), &request),
        Err(SpawnError::UnknownPlayer)
    );

    let mut bare = empty_sim();
    let request = tank_at(&bare, 0, v(5, 5));
    assert_eq!(
        setup_spawn(bare.world_mut(), &request),
        Err(SpawnError::NoMap)
    );
}

#[test]
fn spawn_unknown_unit_is_error() {
    let mut app = duel();
    let content = app.world().resource::<Content>().0.clone();
    let out_of_range = UnitTypeId(u16::try_from(content.units().len()).expect("few units"));
    let request = SpawnRequest {
        unit: Some(out_of_range),
        ..tank_at(&app, 0, v(5, 5))
    };
    assert_eq!(
        setup_spawn(app.world_mut(), &request),
        Err(SpawnError::UnknownUnit)
    );
    assert_eq!(content.unit_id("core_test.abstract.vehicle"), None);
    assert_eq!(app.world().resource::<SimIndex>().len(), 2);
}

#[test]
fn despawn_unit_removes_it() {
    let mut app = duel();
    assert!(setup_despawn(app.world_mut(), SimId(1)));
    assert!(!setup_despawn(app.world_mut(), SimId(1)));
    let ids: Vec<SimId> = app.world().resource::<SimIndex>().ids().collect();
    assert_eq!(ids, [SimId(2)]);
}

fn first_error_file(mod_dir: &Path) -> PathBuf {
    let errors = match load_content(&[], &[mod_dir.to_path_buf()]) {
        Ok(_) => panic!("{} must not load", mod_dir.display()),
        Err(BuildError::Content(errors)) => errors,
        Err(other) => panic!("{} fails to load as content: {other}", mod_dir.display()),
    };
    errors.0[0].file.clone()
}

#[test]
fn mobility_rejects_absurd_speed() {
    let file = first_error_file(&fixture("absurd_speed"));
    assert!(file.ends_with("units/tank.unit.ron"), "{}", file.display());
}

/// Copy core_mod to a scratch dir with the tank's Mobility line replaced.
fn core_mod_with_mobility(name: &str, mobility: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(&fixture("core_mod"), &dir);
    let tank = dir.join("units/tank.unit.ron");
    let text = std::fs::read_to_string(&tank).expect("tank fixture");
    let line = text
        .lines()
        .find(|l| l.contains("\"Mobility\""))
        .expect("tank has Mobility");
    let text = text.replace(line, &format!("        \"Mobility\": {mobility},"));
    std::fs::write(&tank, text).expect("write tank");
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy file");
        }
    }
}

#[test]
fn mobility_rejects_out_of_range_values() {
    let dir = core_mod_with_mobility(
        "mobility_out_of_range",
        "(class: Wheeled, road_speed_kmh: 500.0, terrain_factor: 1.5, turn_rate_deg_s: 0.0)",
    );
    let Err(BuildError::Content(errors)) = load_content(&[], &[dir]) else {
        panic!("must not load");
    };
    let mut paths: Vec<&str> = errors.0.iter().map(|e| e.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        [
            "core_test.tank/components.Mobility.road_speed_kmh",
            "core_test.tank/components.Mobility.terrain_factor",
            "core_test.tank/components.Mobility.turn_rate_deg_s",
        ]
    );
}

#[test]
fn mobility_rejects_rates_that_round_to_zero_per_tick() {
    // Positive, but below one raw unit (speed) or one angle unit (turn) per tick.
    let dir = core_mod_with_mobility(
        "mobility_zero_per_tick",
        "(class: Wheeled, road_speed_kmh: 0.000000001, terrain_factor: 1.0, turn_rate_deg_s: 0.001)",
    );
    let Err(BuildError::Content(errors)) = load_content(&[], &[dir]) else {
        panic!("must not load");
    };
    let mut paths: Vec<&str> = errors.0.iter().map(|e| e.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        [
            "core_test.tank/components.Mobility.road_speed_kmh",
            "core_test.tank/components.Mobility.turn_rate_deg_s",
        ]
    );
}

#[test]
fn wander_stays_within_radius_and_map() {
    let mut app = empty_sim();
    let scale = ScaleScenario {
        units: 200,
        ..ScaleScenario::default()
    };
    let ids = scale.populate(app.world_mut()).expect("scale populates");
    assert_eq!(ids.len(), 200);
    step(&mut app, 400);

    let map = app.world().resource::<MapInfo>().clone();
    let radius = Fx::from_int(scale.wander_m);
    // radius·√2, plus a millimetre for fixed-point rounding along the path.
    let limit_sq = radius * radius * Fx::from_int(2) + Fx::from_ratio(1, 1000);
    let mut moved = 0;
    for id in ids {
        let entity = app.world().resource::<SimIndex>().get(id).expect("alive");
        let pos = app.world().get::<Pose>(entity).expect("pose").pos;
        let wander = app.world().get::<Wander>(entity).expect("wander");
        assert!(map.contains(pos), "{id}: {pos:?} is on the map");
        assert!(
            (pos - wander.home).length_sq() <= limit_sq,
            "{id}: {pos:?} strayed from {:?}",
            wander.home
        );
        if pos != wander.home {
            moved += 1;
        }
    }
    assert!(moved > 150, "most units wandered ({moved} moved)");
}

fn poses(app: &App) -> Vec<(SimId, Pose)> {
    let world = app.world();
    world
        .resource::<SimIndex>()
        .iter()
        .map(|(id, e)| (id, *world.get::<Pose>(e).expect("pose")))
        .collect()
}

fn scale_poses(seed: u64) -> (Vec<(SimId, Pose)>, App) {
    let mut app = sim_seeded(Vec::new(), seed);
    // Units only: no followers, nothing fired, no sensing.
    let scale = ScaleScenario {
        units: 100,
        seed,
        followers_per_unit: 0,
        projectiles: 0,
        sensor_every: 0,
        ..ScaleScenario::default()
    };
    scale.populate(app.world_mut()).expect("scale populates");
    (poses(&app), app)
}

#[test]
fn scale_populate_is_deterministic() {
    let (a, app) = scale_poses(1);
    let (b, _) = scale_poses(1);
    let (c, _) = scale_poses(2);
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(a.len(), 100);

    let world = app.world();
    assert_eq!(*world.resource::<MatchSeed>(), MatchSeed(1));
    let players = world.resource::<Players>();
    assert_eq!(players.list.len(), 4);
    let map = world.resource::<MapInfo>();
    assert_eq!(map.size, v(2000, 2000));
    // Player i's units start in quadrant i.
    for (id, pose) in &a {
        let entity = world.resource::<SimIndex>().get(*id).expect("alive");
        let owner = world.get::<Owner>(entity).expect("owner").0;
        let half = Fx::from_int(1000);
        let quadrant = u8::from(pose.pos.x >= half) + 2 * u8::from(pose.pos.y >= half);
        assert_eq!(quadrant, owner.0 % 4, "{id} at {:?}", pose.pos);
    }
}

#[test]
fn scale_seed_must_be_the_match_seed() {
    let mut app = empty_sim();
    let scale = ScaleScenario {
        units: 10,
        seed: 2,
        ..ScaleScenario::default()
    };
    assert_eq!(
        scale.populate(app.world_mut()),
        Err(SetupError::SeedMismatch {
            scale: 2,
            matched: 1
        })
    );
    let world = app.world();
    assert_eq!(*world.resource::<MatchSeed>(), MatchSeed(1));
    assert!(
        world.get_resource::<MapInfo>().is_none(),
        "nothing was set up"
    );
    assert!(world.resource::<SimIndex>().is_empty());
}

#[derive(Resource, Default)]
struct Noise(u64);

/// Registers the stream "aaa.noise" (sorting before every core stream) and draws from it
/// for every entity, every tick.
struct NoiseFeature;

impl SimFeature for NoiseFeature {
    fn id(&self) -> &'static str {
        "aaa"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().init_resource::<Noise>();
        let stream = sim.register_stream("aaa.noise");
        sim.add_systems(
            ThinkSet::Orders,
            move |clock: Res<SimClock>,
                  seed: Res<MatchSeed>,
                  units: Query<&SimEntity>,
                  mut noise: ResMut<Noise>| {
                for unit in &units {
                    let draw = rolls(&seed, clock.tick, unit.id(), stream).next_u64();
                    noise.0 = noise.0.wrapping_add(draw);
                }
            },
        );
    }
}

fn wander_poses(features: Vec<Arc<dyn SimFeature>>) -> Vec<(SimId, Pose)> {
    let mut app = sim_with(features);
    let scale = ScaleScenario {
        units: 50,
        ..ScaleScenario::default()
    };
    scale.populate(app.world_mut()).expect("scale populates");
    step(&mut app, 200);
    poses(&app)
}

#[test]
fn adding_a_stream_does_not_move_wander() {
    let without = wander_poses(Vec::new());
    let with = wander_poses(vec![Arc::new(NoiseFeature)]);
    assert_eq!(without, with);
    let homes = {
        let mut app = empty_sim();
        let scale = ScaleScenario {
            units: 50,
            ..ScaleScenario::default()
        };
        scale.populate(app.world_mut()).expect("scale populates");
        poses(&app)
    };
    assert_ne!(without, homes, "the units wandered");
}

#[test]
fn setup_scenario_spawns_units() {
    let mut app = empty_sim();
    let ids = setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
    assert_eq!(ids, [SimId(1), SimId(2)]);
    let world = app.world();
    assert_eq!(world.resource::<Players>().list.len(), 2);
    assert_eq!(world.resource::<MapInfo>().size, v(1000, 1000));
    let second = world.resource::<SimIndex>().get(SimId(2)).expect("#2");
    assert_eq!(world.get::<Owner>(second).expect("owner").0, PlayerId(1));
    assert_eq!(
        *world.get::<Pose>(second).expect("pose"),
        Pose {
            pos: v(900, 900),
            heading: Angle::HALF
        }
    );
    assert_eq!(world.resource::<SimClock>().tick, Tick(0));
    assert_eq!(
        *world.resource::<MatchSeed>(),
        MatchSeed(1),
        "setup never reseeds: the seed is the MatchSetup's"
    );
}

#[test]
fn for_scenario_uses_scenario_seed() {
    let content = load_content(&[], &[fixture("core_mod")]).expect("core_mod loads");
    let setup = MatchSetup::for_scenario(&content, "core_test.duel").expect("the duel exists");
    assert_eq!(setup.seed, 42);
    assert!(!setup.cheats_allowed);
    assert!(matches!(
        MatchSetup::for_scenario(&content, "core_test.nope"),
        Err(SetupError::UnknownScenario(_))
    ));
}

/// The duel's definition with `edit` applied.
fn duel_def(app: &App, edit: impl FnOnce(&mut ScenarioDef)) -> ScenarioDef {
    let mut def = app
        .world()
        .resource::<Content>()
        .0
        .kind::<ScenarioDef>()
        .get("core_test.duel")
        .cloned()
        .expect("the duel exists");
    edit(&mut def);
    def
}

#[test]
fn scenario_setup_is_all_or_nothing() {
    let mut app = empty_sim();
    let mut unknown = duel_def(&app, |_| {});
    unknown.units[1].unit = "core_test.nope".to_owned();
    let off_map = duel_def(&app, |def| def.units[1].pos = v(5000, 5000));
    let bad_player = duel_def(&app, |def| def.units[1].player = 7);
    let cases = [
        (unknown, SpawnError::UnknownUnit, "core_test.nope"),
        (off_map, SpawnError::OutsideMap, "core_test.tank"),
        (bad_player, SpawnError::UnknownPlayer, "core_test.tank"),
    ];
    for (def, expected, unit) in cases {
        let err = setup_scenario_def(app.world_mut(), &def).expect_err("must fail");
        assert_eq!(
            err,
            SetupError::Unit {
                index: 1,
                unit: unit.to_owned(),
                error: expected
            }
        );
        assert!(err.to_string().contains(unit), "{err}");
        let world = app.world();
        assert!(world.resource::<SimIndex>().is_empty(), "nothing spawned");
        assert!(world.get_resource::<MapInfo>().is_none(), "no map inserted");
        assert!(world.resource::<Players>().list.is_empty(), "no players");
        assert_eq!(world.resource::<fl_sim::SimIdAllocator>().issued(), 0);
    }
}

#[test]
fn setup_unknown_ids_are_errors() {
    let mut app = empty_sim();
    assert!(matches!(
        setup_scenario(app.world_mut(), "core_test.nope"),
        Err(fl_sim::SetupError::UnknownScenario(_))
    ));
    assert!(matches!(
        setup_map(app.world_mut(), "core_test.nope"),
        Err(fl_sim::SetupError::UnknownMap(_))
    ));
}

/// Every tick's state hash over 50 ticks of the scale scenario (with [`NoiseFeature`]), with
/// `SimTick` shuffled by `shuffle_seed`.
fn scale_hashes(shuffle_seed: Option<u64>) -> Vec<Option<u64>> {
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(NoiseFeature)];
    let content = load_content(&features, &[fixture("core_mod")]).expect("core_mod loads");
    let run = RunConfig {
        hash_interval: Some(1),
        shuffle_seed,
        ..RunConfig::default()
    };
    let setup = MatchSetup {
        seed: ScaleScenario::default().seed,
        ..MatchSetup::default()
    };
    let mut app = build_sim_app(&features, content, setup, run).expect("the sim builds");
    let scale = ScaleScenario {
        units: 50,
        ..ScaleScenario::default()
    };
    scale.populate(app.world_mut()).expect("scale populates");
    (0..50)
        .map(|_| {
            let tick = app.world().resource::<SimClock>().tick;
            let input = TickInput {
                tick,
                commands: Vec::new(),
            };
            step_tick(app.world_mut(), input).hash
        })
        .collect()
}

#[test]
fn shuffled_schedule_gives_the_same_hashes() {
    let plain = scale_hashes(None);
    assert!(plain.iter().all(Option::is_some));
    for seed in [1, 2] {
        assert_eq!(scale_hashes(Some(seed)), plain, "shuffle seed {seed}");
    }
}
