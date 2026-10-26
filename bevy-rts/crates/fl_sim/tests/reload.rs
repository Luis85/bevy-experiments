//! Hot reload, sim side (ADR-0012 and its M1 refinement, Task 12): a value-only reload applies
//! in place between ticks, keeping ids, poses, orders and HP ratios, rebuilding the derived
//! navigation data and re-planning units whose movement profile changed; it is refused without
//! cheats or when the ID sets differ. An ID-set change rebuilds through a save loaded with
//! `drop_unknown_units`, and a save naming a terrain kind the content lacks does not load. The
//! threaded runner applies a reload sent as `ToRunner::Control`.

use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::EntityWorldMut;
use fl_content::{ContentDb, DefComponent, SpawnCtx};
use fl_math::{Fx, FxVec2};
use fl_protocol::{MoveCmd, SimId, Tick};
use fl_sim::nav::{HpaGraphs, MoveProfile, NavGrid, PathRules, content_profiles, profile_of};
use fl_sim::{
    Content, Footprint, Health, LoadError, LoadOptions, MatchHeaderRes, Mobility, MoveOrder,
    MovementRules, PathFollow, PathState, Pose, ReloadError, RunConfig, SimBuilder, SimFeature,
    SimIndex, SimRunner, TerrainGrid, TestDriver, apply_content_values, compute_state_hash,
    load_world, load_world_with, save_world,
};
use fl_test_utils::{Scenario, ScenarioBuilder, write_ascii_mod};
use fl_view::{
    FromRunner, OpaqueContent, RunnerControl, SimReply, TickDriver, TickInput, ToRunner,
};
use serde_json::Value;

/// Cell side of the map, in metres.
const CELL_M: i32 = 4;

/// 24 × 8 cells: a road along row y = 2 (centre y = 10 m), forest at cells x 10–11, y 4–5,
/// grass elsewhere.
const ROWS: [&str; 8] = [
    "........................",
    "........................",
    "..........ff............",
    "..........ff............",
    "........................",
    "rrrrrrrrrrrrrrrrrrrrrrrr",
    "........................",
    "........................",
];

/// The tank's file in the ASCII mod.
const TANK: &str = "units/tank.unit.ron";

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

fn move_to(x: i32, y: i32) -> MoveCmd {
    MoveCmd { target: v(x, y) }
}

/// The ASCII map with player 0 (`t.red`).
fn on_map() -> ScenarioBuilder {
    ScenarioBuilder::new()
        .ascii_map(&ROWS, CELL_M)
        .player("t.red", 0)
}

/// A tank (#1) on the road at the centre of cell (1, 2), ordered at tick 0 to drive east along
/// it, and a truck (#2) on the grass below.
fn road_drive() -> ScenarioBuilder {
    on_map()
        .unit("t.tank", 0, 6, 10)
        .unit("t.truck", 0, 6, 2)
        .command(0, 0, &[1], move_to(90, 10))
}

/// A new, empty scratch directory for `name`, unique to this process and call (nextest runs
/// tests in parallel processes that may edit the same mod).
fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("reload_{name}_{}_{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// Replace the one occurrence of `from` in `dir/file` with `to`.
fn edit(dir: &Path, file: &str, from: &str, to: &str) {
    let path = dir.join(file);
    let text = std::fs::read_to_string(&path).expect("a mod file");
    assert!(text.contains(from), "{file} holds {from:?}:\n{text}");
    std::fs::write(&path, text.replacen(from, to, 1)).expect("the mod file is written");
}

/// The ASCII mod of `rows` edited by `change`, loaded with `s`'s features: content a running
/// match of [`ROWS`] can reload.
fn content_of(
    s: &Scenario,
    name: &str,
    rows: &[&str],
    change: impl FnOnce(&Path),
) -> Arc<ContentDb> {
    let dir = scratch(name);
    write_ascii_mod(&dir, rows, CELL_M, None);
    change(&dir);
    let content = fl_sim::load_content(s.features(), std::slice::from_ref(&dir))
        .unwrap_or_else(|err| panic!("{name}: the edited mod does not load: {err}"));
    let _ = std::fs::remove_dir_all(&dir);
    content
}

/// [`content_of`] the map [`ROWS`].
fn edited(s: &Scenario, name: &str, change: impl FnOnce(&Path)) -> Arc<ContentDb> {
    content_of(s, name, &ROWS, change)
}

/// The tank driving at 72 km/h instead of 36.
fn faster_tank(s: &Scenario) -> Arc<ContentDb> {
    edited(s, "faster_tank", |dir| {
        edit(dir, TANK, "road_speed_kmh: 36.0", "road_speed_kmh: 72.0");
    })
}

fn entity(world: &bevy_ecs::world::World, id: u32) -> Entity {
    world
        .resource::<SimIndex>()
        .get(SimId(id))
        .unwrap_or_else(|| panic!("#{id} is alive"))
}

fn follow(s: &Scenario, id: u32) -> PathFollow {
    s.world()
        .get::<PathFollow>(entity(s.world(), id))
        .cloned()
        .expect("a mobile unit has a PathFollow")
}

fn ids(world: &bevy_ecs::world::World) -> Vec<SimId> {
    world
        .resource::<SimIndex>()
        .iter()
        .map(|(id, _)| id)
        .collect()
}

fn profile(world: &bevy_ecs::world::World, id: u32) -> MoveProfile {
    let nav = world.resource::<NavGrid>();
    let e = entity(world, id);
    profile_of(
        world.get::<Mobility>(e).expect("mobile"),
        world.get::<Footprint>(e),
        nav.cell_m(),
        nav.max_clearance(),
    )
}

/// The HPA* graphs a from-scratch build makes of the world's terrain for `content`.
fn fresh_graphs(world: &bevy_ecs::world::World, content: &ContentDb) -> HpaGraphs {
    let terrain = world.resource::<TerrainGrid>();
    let cell = Fx::from_int(CELL_M);
    let max_clearance = content.rules::<PathRules>().max_clearance;
    let grid = NavGrid::build(
        terrain,
        content,
        content.rules::<MovementRules>(),
        &content_profiles(content, cell, max_clearance),
        max_clearance,
        cell,
    );
    HpaGraphs::build(&grid, content.rules::<PathRules>().cluster_cells)
}

/// The journal entries of tick `tick` of kind `kind` about unit `id`.
fn journaled<'a>(s: &'a Scenario, tick: Tick, kind: &str, id: u32) -> Vec<&'a Value> {
    s.journal()
        .iter()
        .filter(|e| e.tick == tick && e.kind == kind && e.sim_id == Some(SimId(id)))
        .map(|e| &e.data)
        .collect()
}

/// The rules hash the running match's header names.
fn header_rules_hash(s: &Scenario) -> u64 {
    s.world().resource::<MatchHeaderRes>().0.rules_hash
}

fn run_config() -> RunConfig {
    RunConfig {
        hash_interval: Some(1),
        ..RunConfig::default()
    }
}

#[test]
fn value_reload_keeps_ids_and_hp_ratio() {
    let mut s = road_drive().build();
    s.run_ticks(5);
    let tank = entity(s.world(), 1);
    *s.world_mut().get_mut::<Health>(tank).expect("a hull") = Health { hp: 50, max: 100 };
    let truck = entity(s.world(), 2);
    *s.world_mut().get_mut::<Health>(truck).expect("a hull") = Health { hp: 0, max: 100 };
    let before_ids = ids(s.world());
    let state = |s: &Scenario, id: u32| {
        let e = entity(s.world(), id);
        (
            *s.world().get::<Pose>(e).expect("a pose"),
            s.world().get::<MoveOrder>(e).cloned().expect("orders"),
            s.world().get::<PathFollow>(e).cloned().expect("a path"),
        )
    };
    let before = [state(&s, 1), state(&s, 2)];
    assert_eq!(before[0].2.state, PathState::Following, "the tank drives");

    let content = edited(&s, "hp_200", |dir| edit(dir, TANK, "hp: 100", "hp: 200"));
    let report = apply_content_values(s.world_mut(), Arc::clone(&content)).expect("a value reload");
    assert_eq!(report.tick, Tick(5));
    assert_eq!(report.units_reloaded, 2);
    assert_eq!(report.rules_hash, content.rules_hash());

    assert_eq!(ids(s.world()), before_ids);
    assert_eq!([state(&s, 1), state(&s, 2)], before);
    let health = |s: &Scenario, id: u32| *s.world().get::<Health>(entity(s.world(), id)).unwrap();
    assert_eq!(
        health(&s, 1),
        Health { hp: 100, max: 200 },
        "the ratio is kept"
    );
    assert_eq!(
        health(&s, 2),
        Health { hp: 0, max: 100 },
        "a dead hull stays dead"
    );
    // The match runs on under the new content.
    s.run_ticks(5);
    assert_eq!(health(&s, 1), Health { hp: 100, max: 200 });
}

#[test]
fn value_reload_changes_speed() {
    let mut s = road_drive().build();
    s.run_ticks(5);
    let x = |s: &Scenario| s.pose(1).expect("the tank").pos.x;
    let start = x(&s);
    s.run_ticks(1);
    let before = x(&s) - start;
    assert_eq!(before, Fx::HALF, "36 km/h on a road is 0.5 m per tick");

    let content = faster_tank(&s);
    apply_content_values(s.world_mut(), content).expect("a value reload");
    let start = x(&s);
    s.run_ticks(1);
    assert_eq!(x(&s) - start, before * Fx::from_int(2), "the step doubles");
}

#[test]
fn value_reload_updates_passability() {
    let mut s = road_drive().build();
    s.run_ticks(1);
    let tank = profile(s.world(), 1);
    assert!(
        s.world().resource::<NavGrid>().passable(tank, 10, 4),
        "forest is open"
    );

    let content = edited(&s, "forest_blocks", |dir| {
        edit(
            dir,
            "terrain/forest.terrain.ron",
            "blocks: []",
            "blocks: [Tracked]",
        );
    });
    apply_content_values(s.world_mut(), Arc::clone(&content)).expect("a value reload");
    let nav = s.world().resource::<NavGrid>();
    assert!(!nav.passable(tank, 10, 4), "forest blocks tanks now");
    assert!(nav.passable(tank, 9, 4), "grass stays open");
    assert_eq!(
        *s.world().resource::<HpaGraphs>(),
        fresh_graphs(s.world(), &content),
        "the HPA* graphs equal a from-scratch build"
    );
    s.run_ticks(10);
}

#[test]
fn reload_refused_without_cheats() {
    let mut s = road_drive().cheats(false).build();
    s.run_ticks(3);
    let hash = compute_state_hash(s.world());
    let rules_hash = header_rules_hash(&s);
    let old = Arc::clone(&s.world().resource::<Content>().0);
    let content = faster_tank(&s);
    assert_eq!(
        apply_content_values(s.world_mut(), content),
        Err(ReloadError::CheatsOff)
    );
    assert_eq!(compute_state_hash(s.world()), hash);
    assert_eq!(
        header_rules_hash(&s),
        rules_hash,
        "the header keeps its rules hash"
    );
    assert!(Arc::ptr_eq(&s.world().resource::<Content>().0, &old));
}

#[test]
fn reload_refused_when_ids_differ() {
    let mut s = road_drive().build();
    s.run_ticks(3);
    let hash = compute_state_hash(s.world());
    let rules_hash = header_rules_hash(&s);
    let content = edited(&s, "extra_unit", |dir| {
        let tank = std::fs::read_to_string(dir.join(TANK)).expect("the tank");
        std::fs::write(
            dir.join("units/extra.unit.ron"),
            tank.replace("\"t.tank\"", "\"t.extra\""),
        )
        .expect("a unit file");
    });
    assert_eq!(
        apply_content_values(s.world_mut(), content),
        Err(ReloadError::IdSetsDiffer(vec![
            "content.unit: +t.extra".to_owned()
        ]))
    );
    assert_eq!(compute_state_hash(s.world()), hash);
    assert_eq!(
        header_rules_hash(&s),
        rules_hash,
        "the header keeps its rules hash"
    );
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtraDef;

impl DefComponent for ExtraDef {
    fn instantiate(&self, _ctx: &SpawnCtx<'_>, _entity: &mut EntityWorldMut<'_>) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Registers one more def component, so content loaded with it has another registry.
struct ExtraFeature;

impl SimFeature for ExtraFeature {
    fn id(&self) -> &'static str {
        "extra"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.register_def_component::<ExtraDef>("Extra");
    }
}

#[test]
fn reload_refused_for_another_feature_set() {
    let mut s = road_drive().build();
    let dir = scratch("extra_feature");
    write_ascii_mod(&dir, &ROWS, CELL_M, None);
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(ExtraFeature)];
    let content =
        fl_sim::load_content(&features, std::slice::from_ref(&dir)).expect("the mod loads");
    let _ = std::fs::remove_dir_all(&dir);
    let hash = compute_state_hash(s.world());
    let rules_hash = header_rules_hash(&s);
    assert_eq!(
        apply_content_values(s.world_mut(), content),
        Err(ReloadError::RegistryMismatch)
    );
    assert_eq!(compute_state_hash(s.world()), hash);
    assert_eq!(
        header_rules_hash(&s),
        rules_hash,
        "the header keeps its rules hash"
    );
}

#[test]
fn reload_journals_content_reload() {
    let mut s = road_drive().build();
    s.run_ticks(4);
    let old = s.world().resource::<MatchHeaderRes>().0.rules_hash;
    let content = faster_tank(&s);
    let report = apply_content_values(s.world_mut(), Arc::clone(&content)).expect("a value reload");
    assert_eq!(report.old_rules_hash, old);
    assert_ne!(report.rules_hash, old);
    assert_eq!(
        s.world().resource::<MatchHeaderRes>().0.rules_hash,
        content.rules_hash(),
        "the header names the new content"
    );
    assert!(Arc::ptr_eq(&s.world().resource::<Content>().0, &content));
    s.run_ticks(1);
    let records: Vec<&Value> = s
        .journal()
        .iter()
        .filter(|e| e.kind == "core.content_reload")
        .map(|e| {
            assert_eq!(e.tick, Tick(4));
            assert_eq!(e.sim_id, None);
            &e.data
        })
        .collect();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0]["old_rules_hash"], old);
    assert_eq!(records[0]["rules_hash"], content.rules_hash());
    assert_eq!(records[0]["units"], 2);
}

#[test]
fn reload_is_deterministic() {
    // Each run loads the edited mod itself, as each peer loads its own files.
    let run = |reload: bool| {
        let mut s = road_drive().build();
        s.run_ticks(50);
        if reload {
            let content = faster_tank(&s);
            apply_content_values(s.world_mut(), content).expect("a value reload");
        }
        s.run_ticks(150);
        s.hashes()
    };
    let first = run(true);
    assert_eq!(first.len(), 200);
    assert_eq!(first, run(true));
    let plain = run(false);
    assert_eq!(first[..50], plain[..50]);
    assert_ne!(first[50..], plain[50..], "the reload changed the match");
}

/// Two tanks (#1, #3) and two trucks (#2, #4), all ordered east, run 20 ticks, and the content
/// without trucks.
fn trucks_vanish() -> (Scenario, Arc<ContentDb>) {
    trucks_vanish_with(true)
}

/// [`trucks_vanish`] in a match with `cheats_allowed` = `cheats`.
fn trucks_vanish_with(cheats: bool) -> (Scenario, Arc<ContentDb>) {
    let mut s = on_map()
        .cheats(cheats)
        .unit("t.tank", 0, 6, 10)
        .unit("t.truck", 0, 6, 2)
        .unit("t.tank", 0, 6, 26)
        .unit("t.truck", 0, 10, 30)
        .command(0, 0, &[1], move_to(90, 10))
        .command(0, 0, &[2], move_to(90, 2))
        .command(0, 0, &[3, 4], move_to(80, 28))
        .build();
    s.run_ticks(20);
    let content = edited(&s, "no_trucks", |dir| {
        std::fs::remove_file(dir.join("units/truck.unit.ron")).expect("the truck's file");
        for faction in ["red", "blue"] {
            edit(
                dir,
                &format!("factions/{faction}.faction.ron"),
                "\"t.truck\", ",
                "",
            );
        }
    });
    (s, content)
}

#[test]
fn rebuild_drops_vanished_unit_types() {
    let (s, content) = trucks_vanish();
    let bytes = save_world(s.world(), &s.driver.pending()).expect("no pending spawns");
    let load = || {
        load_world_with(
            s.features(),
            Arc::clone(&content),
            &bytes,
            run_config(),
            LoadOptions {
                drop_unknown_units: true,
            },
        )
        .unwrap_or_else(|err| panic!("the save loads without its trucks: {err}"))
    };
    let mut twins = Vec::new();
    for _ in 0..2 {
        let loaded = load();
        assert_eq!(
            loaded.dropped,
            [
                (SimId(2), "t.truck".to_owned()),
                (SimId(4), "t.truck".to_owned())
            ]
        );
        let world = loaded.app.world();
        assert_eq!(ids(world), [SimId(1), SimId(3)], "survivors keep their ids");
        for id in [1, 3] {
            assert_eq!(
                world.get::<Pose>(entity(world, id)),
                s.pose(id).as_ref(),
                "#{id} is where it was"
            );
        }
        let runner = SimRunner::new(loaded.app);
        let mut driver = TestDriver::new().starting_at(runner.next_tick());
        driver.restore(loaded.pending, &loaded.last_seq);
        twins.push((runner, driver));
    }
    let hashes: Vec<Vec<Option<u64>>> = twins
        .iter_mut()
        .map(|(runner, driver)| runner.run(driver, 50).iter().map(|o| o.hash).collect())
        .collect();
    assert_eq!(hashes[0].len(), 50);
    assert!(hashes[0].iter().all(Option::is_some));
    assert_eq!(
        hashes[0], hashes[1],
        "the rebuilt match runs on deterministically"
    );
    // The dropped ids stay unused; both survivors still drive.
    let world = twins[0].0.world();
    assert!(world.resource::<SimIndex>().get(SimId(2)).is_none());
    assert_ne!(
        world.get::<Pose>(entity(world, 1)),
        s.pose(1).as_ref(),
        "#1 drove on"
    );
}

/// A tank (#1) and a truck (#2) driving, and six tanks (#3–#8) on one flow field, with
/// `cheats_allowed`, after 3 ticks.
fn tanks_on_a_field() -> Scenario {
    let mut s = on_map()
        .unit("t.tank", 0, 6, 10)
        .unit("t.truck", 0, 6, 2)
        .unit("t.tank", 0, 6, 30)
        .unit("t.tank", 0, 14, 30)
        .unit("t.tank", 0, 22, 30)
        .unit("t.tank", 0, 6, 22)
        .unit("t.tank", 0, 14, 22)
        .unit("t.tank", 0, 22, 22)
        .command(0, 0, &[1], move_to(90, 10))
        .command(0, 0, &[2], move_to(90, 2))
        .command(0, 0, &[3, 4, 5, 6, 7, 8], move_to(70, 26))
        .build();
    s.run_ticks(3);
    for id in 3..=8 {
        assert!(
            matches!(follow(&s, id).state, PathState::Flow { .. }),
            "#{id} is on the flow field: {:?}",
            follow(&s, id).state
        );
    }
    s
}

#[test]
fn rebuild_replans_flow_units() {
    // Task 12 review Minor 2: the ID-set rebuild (a load with drop_unknown_units) re-plans
    // flow units with the value reload's helper. The save does not say what the content was,
    // so a saved field may name a profile or a cluster layout the rebuilt graphs lack: every
    // unit on a field asks for a new path.
    let s = tanks_on_a_field();
    let bytes = save_world(s.world(), &s.driver.pending()).expect("no pending spawns");
    let content = edited(&s, "no_trucks_flow", |dir| {
        std::fs::remove_file(dir.join("units/truck.unit.ron")).expect("the truck's file");
        for faction in ["red", "blue"] {
            edit(
                dir,
                &format!("factions/{faction}.faction.ron"),
                "\"t.truck\", ",
                "",
            );
        }
    });
    let load = || {
        load_world_with(
            s.features(),
            Arc::clone(&content),
            &bytes,
            run_config(),
            LoadOptions {
                drop_unknown_units: true,
            },
        )
        .unwrap_or_else(|err| panic!("the save loads without its truck: {err}"))
    };
    let mut twins = Vec::new();
    for _ in 0..2 {
        let loaded = load();
        assert_eq!(loaded.dropped, [(SimId(2), "t.truck".to_owned())]);
        let world = loaded.app.world();
        let hpa = world.resource::<HpaGraphs>();
        for id in 3..=8 {
            let state = world
                .get::<PathFollow>(entity(world, id))
                .expect("a mobile unit has a PathFollow")
                .state;
            assert_eq!(state, PathState::Idle, "#{id} re-plans");
        }
        for (_, e) in world.resource::<SimIndex>().iter() {
            if let Some(PathState::Flow { key }) = world.get::<PathFollow>(e).map(|f| f.state) {
                assert!(
                    hpa.graph(key.profile).is_some(),
                    "a loaded field names {:?}, which the rebuilt graphs lack",
                    key.profile
                );
            }
        }
        let runner = SimRunner::new(loaded.app);
        let mut driver = TestDriver::new().starting_at(runner.next_tick());
        driver.restore(loaded.pending, &loaded.last_seq);
        twins.push((runner, driver));
    }
    let mut outputs: Vec<_> = twins
        .iter_mut()
        .map(|(runner, driver)| runner.run(driver, 3))
        .collect();
    // What the re-plan leads to: `advance_orders` asks a single path for each unit whose
    // current move went back to `Idle` (as after a value reload or a terrain change), so
    // #3–#8 drive on along single paths; nothing regroups them onto one field.
    let world = twins[0].0.world();
    let states: Vec<PathState> = (3..=8)
        .map(|id| {
            world
                .get::<PathFollow>(entity(world, id))
                .expect("a mobile unit has a PathFollow")
                .state
        })
        .collect();
    assert_eq!(
        states,
        [PathState::Following; 6],
        "#3–#8 follow single paths"
    );
    for ((runner, driver), run) in twins.iter_mut().zip(&mut outputs) {
        run.extend(runner.run(driver, 27));
    }
    let replanned: Vec<u32> = outputs[0][0]
        .journal
        .iter()
        .filter(|e| e.kind == "core.order_changed" && e.data["reason"] == "replanned")
        .filter_map(|e| e.sim_id.map(|id| id.0))
        .collect();
    assert_eq!(replanned, [3, 4, 5, 6, 7, 8], "journaled at the load");
    let hashes = |run: &Vec<fl_view::TickOutput>| -> Vec<Option<u64>> {
        run.iter().map(|o| o.hash).collect()
    };
    assert_eq!(
        hashes(&outputs[0]),
        hashes(&outputs[1]),
        "the rebuilt match runs on deterministically"
    );
    let world = twins[0].0.world();
    for id in 3..=8 {
        assert_ne!(
            world.get::<Pose>(entity(world, id)),
            s.pose(id).as_ref(),
            "#{id} drove on"
        );
    }
}

#[test]
fn load_of_unchanged_content_keeps_flow_units() {
    // The re-plan after a load fires only for a stale field: a save loaded under the content
    // it was made with keeps every unit on its field.
    let s = tanks_on_a_field();
    let bytes = save_world(s.world(), &s.driver.pending()).expect("no pending spawns");
    let content = Arc::clone(&s.world().resource::<Content>().0);
    let loaded = load_world(s.features(), content, &bytes, run_config()).expect("the save loads");
    let world = loaded.app.world();
    for id in 1..=8 {
        assert_eq!(
            world.get::<PathFollow>(entity(world, id)),
            Some(&follow(&s, id)),
            "#{id} keeps its path state"
        );
    }
}

#[test]
fn load_without_drop_rejects_unknown_unit() {
    let (s, content) = trucks_vanish();
    let bytes = save_world(s.world(), &[]).expect("no pending spawns");
    match load_world(s.features(), content, &bytes, run_config()) {
        Err(LoadError::Codec { name, .. }) => assert_eq!(name, "core.unit_type"),
        Err(other) => panic!("expected a core.unit_type codec error, got {other}"),
        Ok(_) => panic!("a save with unknown unit types loads without dropping them"),
    }
}

/// The next message from the runner, waiting at most 10 s.
fn recv(port: &fl_view::RunnerPort) -> FromRunner {
    port.rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the runner answers within 10 s")
}

/// Step the threaded runner at `tick`: the tank's x.
fn tick_x(port: &fl_view::RunnerPort, tick: Tick) -> Fx {
    port.tx
        .send(ToRunner::Tick(TickInput {
            tick,
            commands: Vec::new(),
        }))
        .expect("the runner is alive");
    match recv(port) {
        FromRunner::Tick(out) => {
            assert_eq!(out.tick, tick);
            out.snapshot.get(SimId(1)).expect("the tank").pos.x
        }
        other => panic!("expected a tick, got {other:?}"),
    }
}

#[test]
fn threaded_runner_applies_reload() {
    let mut s = road_drive().build();
    s.run_ticks(5);
    let content = faster_tank(&s);
    let (port, handle) = s.runner.spawn_threaded();

    let x0 = tick_x(&port, Tick(5));
    let x1 = tick_x(&port, Tick(6));
    let before = x1 - x0;
    assert_eq!(before, Fx::HALF);

    // A payload that is no content is an error reply; the match runs on.
    let not_content: Arc<dyn Any + Send + Sync> = Arc::new("not content");
    port.tx
        .send(ToRunner::Control {
            id: 4,
            control: RunnerControl::ReloadContent(OpaqueContent(not_content)),
        })
        .expect("the runner is alive");
    match recv(&port) {
        FromRunner::Reply {
            id: 4,
            reply: SimReply::Error(text),
        } => assert!(text.contains("ContentDb"), "{text}"),
        other => panic!("expected an error reply, got {other:?}"),
    }

    let payload: Arc<dyn Any + Send + Sync> = Arc::clone(&content) as _;
    port.tx
        .send(ToRunner::Control {
            id: 9,
            control: RunnerControl::ReloadContent(OpaqueContent(payload)),
        })
        .expect("the runner is alive");
    match recv(&port) {
        FromRunner::Reply {
            id: 9,
            reply: SimReply::Reloaded { tick, rules_hash },
        } => {
            assert_eq!(tick, Tick(7));
            assert_eq!(rules_hash, content.rules_hash());
        }
        other => panic!("expected Reloaded, got {other:?}"),
    }
    let x2 = tick_x(&port, Tick(7));
    assert_eq!(
        x2 - x1,
        before * Fx::from_int(2),
        "the next tick uses the new speed"
    );

    port.tx
        .send(ToRunner::Shutdown)
        .expect("the runner is alive");
    handle.join().expect("the runner thread exits cleanly");
}

#[test]
fn rebuild_with_vanished_terrain_is_load_error() {
    let mut s = road_drive().build();
    s.run_ticks(2);
    let palette = &s.world().resource::<TerrainGrid>().palette;
    assert!(palette.iter().any(|name| name == "t.swamp"), "{palette:?}");
    let bytes = save_world(s.world(), &[]).expect("no pending spawns");
    let content = edited(&s, "no_swamp", |dir| {
        std::fs::remove_file(dir.join("terrain/swamp.terrain.ron")).expect("the swamp's file");
        edit(
            dir,
            "maps/ascii.map.ron",
            ", ((80, 90, 60), \"t.swamp\")",
            "",
        );
    });
    for opts in [
        LoadOptions::default(),
        LoadOptions {
            drop_unknown_units: true,
        },
    ] {
        match load_world_with(
            s.features(),
            Arc::clone(&content),
            &bytes,
            run_config(),
            opts,
        ) {
            Err(LoadError::Codec { name, message }) => {
                assert_eq!(name, "core.terrain");
                assert!(message.contains("unknown terrain 't.swamp'"), "{message}");
            }
            Err(other) => panic!("expected a core.terrain codec error, got {other}"),
            Ok(_) => panic!("a save naming a vanished terrain kind loads"),
        }
    }
}

#[test]
fn reload_changing_profile_replans() {
    let mut s = on_map()
        .unit("t.tank", 0, 6, 10)
        .unit("t.truck", 0, 6, 2)
        // Six tanks: a group move gives them one flow field.
        .unit("t.tank", 0, 6, 30)
        .unit("t.tank", 0, 14, 30)
        .unit("t.tank", 0, 22, 30)
        .unit("t.tank", 0, 6, 22)
        .unit("t.tank", 0, 14, 22)
        .unit("t.tank", 0, 22, 22)
        .command(0, 0, &[1], move_to(90, 10))
        .command(0, 0, &[2], move_to(90, 2))
        .command(0, 0, &[3, 4, 5, 6, 7, 8], move_to(70, 26))
        .build();
    s.run_ticks(3);
    assert_eq!(profile(s.world(), 1).size, 1);
    assert_eq!(follow(&s, 1).state, PathState::Following);
    assert_eq!(follow(&s, 2).state, PathState::Following);
    for id in 3..=8 {
        assert!(
            matches!(follow(&s, id).state, PathState::Flow { .. }),
            "#{id} is on the flow field: {:?}",
            follow(&s, id).state
        );
    }

    let content = edited(&s, "wide_tank", |dir| {
        edit(dir, TANK, "footprint: (3.0, 6.0)", "footprint: (7.0, 6.0)");
    });
    apply_content_values(s.world_mut(), Arc::clone(&content)).expect("a value reload");
    assert_eq!(profile(s.world(), 1).size, 2);
    for id in [1, 3, 4, 5, 6, 7, 8] {
        assert_eq!(follow(&s, id).state, PathState::Idle, "#{id} re-plans");
    }
    assert_eq!(
        follow(&s, 2).state,
        PathState::Following,
        "the truck's profile did not change"
    );
    assert_eq!(
        *s.world().resource::<HpaGraphs>(),
        fresh_graphs(s.world(), &content),
        "the HPA* graphs equal a from-scratch build"
    );

    s.run_ticks(1);
    let tick = Tick(3);
    for id in [1, 3, 4, 5, 6, 7, 8] {
        let changed = journaled(&s, tick, "core.order_changed", id);
        assert!(
            changed.iter().any(|d| d["reason"] == "replanned"),
            "#{id}: {changed:?}"
        );
    }
    assert!(
        journaled(&s, tick, "core.order_changed", 2)
            .iter()
            .all(|d| d["reason"] != "replanned"),
        "the truck is not re-planned"
    );
    let requests = journaled(&s, tick, "core.path_request", 1);
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(
        requests[0]["profile"]["size"], 2,
        "the next plan uses size 2"
    );
    assert_eq!(follow(&s, 1).state, PathState::Following, "#1 drives on");
    s.run_ticks(20);
}

#[test]
fn value_reload_keeps_terrain_grid() {
    let mut s = road_drive().build();
    s.run_ticks(2);
    let terrain = s.world().resource::<TerrainGrid>().clone();
    let mut rows = ROWS;
    rows[0] = "f.......................";
    let content = content_of(&s, "one_pixel", &rows, |_| {});
    let report = apply_content_values(s.world_mut(), content).expect("a value reload");
    assert_ne!(
        report.rules_hash, report.old_rules_hash,
        "the map is content"
    );
    assert_eq!(
        *s.world().resource::<TerrainGrid>(),
        terrain,
        "the running match keeps its terrain"
    );
    s.run_ticks(2);
}

/// The base rules with `Path.budget_per_tick` 1 (one request per tick) and `flow_min_group`
/// above any group here, so requests wait in the queue.
fn one_request_per_tick() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/base/rules.ron");
    let base = std::fs::read_to_string(&path).expect("the base rules");
    let rules = base
        .replace("budget_per_tick: 8000,", "budget_per_tick: 1,")
        .replace("flow_min_group: 6,", "flow_min_group: 64,");
    assert!(
        rules.contains("budget_per_tick: 1,") && rules.contains("flow_min_group: 64,"),
        "the base rules name budget_per_tick: 8000 and flow_min_group: 6"
    );
    rules
}

#[test]
fn reload_changing_profile_discards_queued_request() {
    let rules = one_request_per_tick();
    let mut s = ScenarioBuilder::new()
        .ascii_map(&ROWS, CELL_M)
        .ascii_rules(&rules)
        .player("t.red", 0)
        .unit("t.tank", 0, 6, 10)
        .unit("t.tank", 0, 6, 22)
        .unit("t.tank", 0, 6, 30)
        .command(0, 0, &[1, 2, 3], move_to(90, 18))
        .build();
    s.run_ticks(1);
    let waiting: Vec<u32> = (1..=3)
        .filter_map(|id| match follow(&s, id).state {
            PathState::Waiting { request } => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(waiting.len(), 2, "two requests wait behind the budget");

    let content = edited(&s, "wide_tank_queued", |dir| {
        std::fs::write(dir.join("rules.ron"), &rules).expect("the rules");
        edit(dir, TANK, "footprint: (3.0, 6.0)", "footprint: (7.0, 6.0)");
    });
    apply_content_values(s.world_mut(), content).expect("a value reload");
    for id in 1..=3 {
        assert_eq!(follow(&s, id).state, PathState::Idle, "#{id} re-plans");
    }
    // The queued requests turn stale at no charge; every new one plans for size 2.
    s.run_ticks(6);
    let results: Vec<&Value> = s
        .journal()
        .iter()
        .filter(|e| e.kind == "core.path_result" && e.tick >= Tick(1))
        .map(|e| &e.data)
        .collect();
    for request in waiting {
        let result = results
            .iter()
            .find(|d| d["request"] == request)
            .unwrap_or_else(|| panic!("request {request} was processed: {results:?}"));
        assert_eq!(result["status"], "stale");
        assert_eq!(result["charge"], 0);
    }
    let sizes: Vec<&Value> = s
        .journal()
        .iter()
        .filter(|e| e.kind == "core.path_request" && e.tick >= Tick(1))
        .map(|e| &e.data["profile"]["size"])
        .collect();
    assert_eq!(sizes.len(), 3, "each tank asks again once");
    assert!(sizes.iter().all(|&size| *size == 2), "{sizes:?}");
}

#[test]
fn reload_changing_cluster_size_replans_flow_units() {
    let mut s = on_map()
        .unit("t.tank", 0, 6, 10)
        .unit("t.tank", 0, 6, 30)
        .unit("t.tank", 0, 14, 30)
        .unit("t.tank", 0, 22, 30)
        .unit("t.tank", 0, 6, 22)
        .unit("t.tank", 0, 14, 22)
        .unit("t.tank", 0, 22, 22)
        .command(0, 0, &[1], move_to(90, 10))
        .command(0, 0, &[2, 3, 4, 5, 6, 7], move_to(70, 26))
        .build();
    s.run_ticks(3);
    assert_eq!(follow(&s, 1).state, PathState::Following);
    for id in 2..=7 {
        assert!(
            matches!(follow(&s, id).state, PathState::Flow { .. }),
            "#{id} is on the flow field: {:?}",
            follow(&s, id).state
        );
    }
    let base = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/base/rules.ron"),
    )
    .expect("the base rules");
    let rules = base.replacen("cluster_cells: 16,", "cluster_cells: 8,", 1);
    assert_ne!(rules, base, "the base rules name cluster_cells: 16");
    let content = edited(&s, "small_clusters", |dir| {
        std::fs::write(dir.join("rules.ron"), &rules).expect("the rules");
    });
    apply_content_values(s.world_mut(), Arc::clone(&content)).expect("a value reload");
    // The profiles are the same, but a flow key's cluster names another region now.
    for id in 2..=7 {
        assert_eq!(follow(&s, id).state, PathState::Idle, "#{id} re-plans");
    }
    assert_eq!(
        follow(&s, 1).state,
        PathState::Following,
        "a path's corners are positions: it stays valid"
    );
    assert_eq!(
        *s.world().resource::<HpaGraphs>(),
        fresh_graphs(s.world(), &content)
    );
    s.run_ticks(1);
    for id in 2..=7 {
        assert!(
            journaled(&s, Tick(3), "core.order_changed", id)
                .iter()
                .any(|d| d["reason"] == "replanned"),
            "#{id} is journaled replanned"
        );
    }
    assert!(
        journaled(&s, Tick(3), "core.order_changed", 1)
            .iter()
            .all(|d| d["reason"] != "replanned")
    );
    s.run_ticks(20);
}

#[test]
fn drop_unknown_units_needs_cheats() {
    let (s, content) = trucks_vanish_with(false);
    let bytes = save_world(s.world(), &[]).expect("no pending spawns");
    let drop = LoadOptions {
        drop_unknown_units: true,
    };
    match load_world_with(
        s.features(),
        Arc::clone(&content),
        &bytes,
        run_config(),
        drop,
    ) {
        Err(LoadError::DropNeedsCheats) => {}
        Err(other) => panic!("expected DropNeedsCheats, got {other}"),
        Ok(_) => panic!("a save without cheats loads with dropped units"),
    }
    // The strict load reports the unknown unit type as before.
    assert!(matches!(
        load_world(s.features(), content, &bytes, run_config()),
        Err(LoadError::Codec { .. })
    ));
}
