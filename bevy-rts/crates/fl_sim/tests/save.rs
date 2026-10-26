//! Save and load (spec §5.8, ADR-0006): the versioned format, typed migrations, string-table
//! remapping, the byte-identical round trip and the branch test.

use std::path::PathBuf;
use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::prelude::*;
use fl_content::ContentDb;
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    CommandPayload, DebugSpawnCmd, DebugWanderCmd, HeaderMismatch, MoveCmd, PlayerId, SimId, Tick,
    tables,
};
use fl_sim::{
    Class, Codec, CommandRegistry, Content, DespawnRequest, LoadError, Loaded, MatchHeaderRes,
    MatchSetup, MoveOrder, Order, Players, RunConfig, SAVE_FORMAT_VERSION, SaveError, SaveFile,
    SimBuilder, SimClock, SimEntity, SimFeature, SimHash, SimIndex, SimRunner, SpawnRequest,
    StateHasher, TestDriver, ThinkSet, UnitType, build_sim_app, compute_state_hash, load_content,
    load_world, save_world, setup_scenario, setup_spawn,
};
use fl_view::{CommandDraft, SimQuery, SimReply, TickDriver};
use serde::{Deserialize, Serialize};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn run_config() -> RunConfig {
    RunConfig {
        hash_interval: Some(1),
        ..RunConfig::default()
    }
}

fn content(features: &[Arc<dyn SimFeature>], mod_dir: &str) -> Arc<ContentDb> {
    load_content(features, &[fixture(mod_dir)]).expect("the fixture mod loads")
}

/// The duel (#1 player 0 at (100, 100), #2 player 1 at (900, 900)) with `features` and
/// `mod_dir`, cheats on, the state hash every tick.
fn duel_in(features: &[Arc<dyn SimFeature>], mod_dir: &str) -> SimRunner {
    let setup = MatchSetup {
        seed: 7,
        cheats_allowed: true,
    };
    let mut app = build_sim_app(features, content(features, mod_dir), setup, run_config())
        .expect("the sim builds");
    let ids = setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
    assert_eq!(ids, [SimId(1), SimId(2)]);
    SimRunner::new(app)
}

fn duel() -> SimRunner {
    duel_in(&[], "core_mod")
}

fn load(features: &[Arc<dyn SimFeature>], mod_dir: &str, bytes: &[u8]) -> Loaded {
    match load_world(features, content(features, mod_dir), bytes, run_config()) {
        Ok(loaded) => loaded,
        Err(err) => panic!("the save loads: {err}"),
    }
}

fn load_err(features: &[Arc<dyn SimFeature>], mod_dir: &str, bytes: &[u8]) -> LoadError {
    match load_world(features, content(features, mod_dir), bytes, run_config()) {
        Ok(_) => panic!("the save loaded"),
        Err(err) => err,
    }
}

fn decode(bytes: &[u8]) -> SaveFile {
    postcard::from_bytes(bytes).expect("a SaveFile")
}

fn encode(save: &SaveFile) -> Vec<u8> {
    postcard::to_allocvec(save).expect("a SaveFile encodes")
}

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

fn draft<P: CommandPayload>(
    runner: &SimRunner,
    player: u8,
    units: &[u32],
    payload: &P,
) -> CommandDraft {
    CommandDraft {
        player: PlayerId(player),
        units: units.iter().map(|&id| SimId(id)).collect(),
        kind: runner.world().resource::<CommandRegistry>().encode(payload),
        queue: false,
    }
}

/// Issues the commands of one tick, before the driver polls it.
type Script = fn(Tick, &SimRunner, &mut TestDriver);

/// Step `runner` with `driver` until `until` (exclusive), running `script` before each poll.
/// Returns each tick's `(tick, hash)`.
fn run_until(
    runner: &mut SimRunner,
    driver: &mut TestDriver,
    until: u32,
    script: Script,
) -> Vec<(Tick, u64)> {
    let mut hashes = Vec::new();
    while driver.next_tick().0 < until {
        script(driver.next_tick(), runner, driver);
        let input = driver.poll().expect("the test driver always ticks");
        let out = runner.step(input);
        hashes.push((out.tick, out.hash.expect("hash interval is 1")));
    }
    hashes
}

fn no_script(_tick: Tick, _runner: &SimRunner, _driver: &mut TestDriver) {}

/// Wander at 0 (#2, player 1), moves at 10 (#1) and 60 (#1 and #2, player 0 owns only #1).
fn moves_script(tick: Tick, runner: &SimRunner, driver: &mut TestDriver) {
    match tick.0 {
        0 => {
            let radius = Fx::from_int(80);
            driver.issue(draft(runner, 1, &[2], &DebugWanderCmd { radius }));
        }
        10 => {
            driver.issue(draft(
                runner,
                0,
                &[1],
                &MoveCmd {
                    target: v(500, 300),
                },
            ));
        }
        60 => {
            driver.issue(draft(
                runner,
                0,
                &[1, 2],
                &MoveCmd {
                    target: v(200, 700),
                },
            ));
        }
        _ => {}
    }
}

/// Save after tick `split - 1`, load into a fresh sim and run both to `end` with `script`;
/// the hashes of `split..end` must match.
fn branch(
    features: &[Arc<dyn SimFeature>],
    make: fn() -> SimRunner,
    split: u32,
    end: u32,
    script: Script,
) {
    let mut a = make();
    let mut driver_a = TestDriver::new();
    let full = run_until(&mut a, &mut driver_a, end, script);

    let mut b = make();
    let mut driver_b = TestDriver::new();
    run_until(&mut b, &mut driver_b, split, script);
    let bytes = save_world(b.world(), &driver_b.pending()).expect("saves");
    drop(b);

    let loaded = load(features, "core_mod", &bytes);
    let mut resumed = SimRunner::new(loaded.app);
    assert_eq!(resumed.next_tick(), Tick(split));
    let mut driver = TestDriver::new().starting_at(Tick(split));
    driver.restore(loaded.pending, &loaded.last_seq);
    let rest = run_until(&mut resumed, &mut driver, end, script);

    assert_eq!(rest.len(), (end - split) as usize);
    assert_eq!(rest, full[split as usize..]);
}

#[test]
fn round_trip_is_byte_identical() {
    let mut runner = duel();
    let mut driver = TestDriver::new();
    let radius = Fx::from_int(100);
    driver.issue(draft(&runner, 0, &[1, 2], &DebugWanderCmd { radius }));
    run_until(&mut runner, &mut driver, 50, no_script);

    let s1 = save_world(runner.world(), &driver.pending()).expect("saves");
    let save = decode(&s1);
    assert_eq!(save.format_version, SAVE_FORMAT_VERSION);
    assert_eq!(save.sim.tick, Tick(50));
    assert_eq!(save.sim.entities.len(), 2);

    let loaded = load(&[], "core_mod", &s1);
    let s2 = save_world(loaded.app.world(), &loaded.pending).expect("saves");
    assert_eq!(s1, s2);
    assert_eq!(
        compute_state_hash(loaded.app.world()),
        compute_state_hash(runner.world())
    );
}

#[test]
fn save_at_tick_zero_round_trips() {
    let runner = duel();
    let s1 = save_world(runner.world(), &[]).expect("saves");
    assert_eq!(decode(&s1).sim.tick, Tick(0));
    let loaded = load(&[], "core_mod", &s1);
    assert_eq!(
        save_world(loaded.app.world(), &loaded.pending).expect("saves"),
        s1
    );
    // And it runs on exactly like the original.
    branch(&[], duel, 0, 30, moves_script);
}

#[test]
fn branch_test_hashes_match_every_tick() {
    branch(&[], duel, 40, 120, moves_script);
}

/// Despawns #3 and #5 at tick 20.
struct DespawnerFeature;

impl SimFeature for DespawnerFeature {
    fn id(&self) -> &'static str {
        "despawner"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(
            ThinkSet::Behaviour,
            |clock: Res<SimClock>, mut out: MessageWriter<DespawnRequest>| {
                if clock.tick == Tick(20) {
                    out.write(DespawnRequest(SimId(3)));
                    out.write(DespawnRequest(SimId(5)));
                }
            },
        );
    }
}

fn despawner() -> Vec<Arc<dyn SimFeature>> {
    vec![Arc::new(DespawnerFeature)]
}

/// The duel plus four tanks #3..#6 for player 0, all wandering.
fn crowded_duel() -> SimRunner {
    let mut runner = duel_in(&despawner(), "core_mod");
    let world = runner.world_mut();
    let unit = world
        .resource::<Content>()
        .0
        .unit_id("core_test.tank")
        .expect("the tank exists");
    for i in 0..4 {
        let req = SpawnRequest {
            unit: Some(unit),
            owner: PlayerId(0),
            pos: v(200 + 50 * i, 300),
            heading: Angle::ZERO,
            init: None,
        };
        setup_spawn(world, &req).expect("the tank spawns");
    }
    runner
}

/// Wander everything at 0, spawn a tank at 50 (after the load: it must get #7), move at 60.
fn crowded_script(tick: Tick, runner: &SimRunner, driver: &mut TestDriver) {
    match tick.0 {
        0 => {
            let radius = Fx::from_int(60);
            let wander = DebugWanderCmd { radius };
            driver.issue(draft(runner, 0, &[1, 2, 3, 4, 5, 6], &wander));
        }
        50 => {
            let spawn = DebugSpawnCmd {
                unit: "core_test.tank".to_owned(),
                owner: PlayerId(1),
                pos: v(600, 600),
                heading: Angle::ZERO,
            };
            driver.issue(draft(runner, 1, &[], &spawn));
        }
        60 => {
            driver.issue(draft(
                runner,
                0,
                &[1, 4, 6],
                &MoveCmd {
                    target: v(800, 100),
                },
            ));
        }
        _ => {}
    }
}

#[test]
fn branch_after_despawns() {
    // The script really leaves gaps, and spawns #7 after the split.
    let mut runner = crowded_duel();
    let mut driver = TestDriver::new();
    run_until(&mut runner, &mut driver, 40, crowded_script);
    let ids: Vec<SimId> = runner.world().resource::<SimIndex>().ids().collect();
    assert_eq!(ids, [SimId(1), SimId(2), SimId(4), SimId(6)]);
    run_until(&mut runner, &mut driver, 52, crowded_script);
    assert!(
        runner
            .world()
            .resource::<SimIndex>()
            .get(SimId(7))
            .is_some()
    );

    branch(&despawner(), crowded_duel, 40, 120, crowded_script);
}

#[test]
fn pending_commands_survive_save() {
    let script: Script = |tick, runner, driver| {
        if tick == Tick(10) {
            driver.issue(draft(
                runner,
                0,
                &[1],
                &MoveCmd {
                    target: v(400, 400),
                },
            ));
        }
    };
    let second_move = |runner: &SimRunner| {
        draft(
            runner,
            0,
            &[1],
            &MoveCmd {
                target: v(150, 600),
            },
        )
    };
    let mut a = duel();
    let mut driver_a = TestDriver::with_delay(2);
    let mut full = run_until(&mut a, &mut driver_a, 20, script);
    driver_a.issue(second_move(&a));
    full.extend(run_until(&mut a, &mut driver_a, 30, script));

    let mut b = duel();
    let mut driver_b = TestDriver::with_delay(2);
    run_until(&mut b, &mut driver_b, 11, script);
    let pending = driver_b.pending();
    assert_eq!(pending.len(), 1, "the move waits for tick 12");
    assert_eq!(pending[0].exec_tick, Tick(12));
    let bytes = save_world(b.world(), &pending).expect("saves");

    let loaded = load(&[], "core_mod", &bytes);
    assert_eq!(loaded.pending, pending);
    assert!(loaded.last_seq.is_empty(), "no command has applied yet");
    let mut resumed = SimRunner::new(loaded.app);
    let mut driver = TestDriver::with_delay(2).starting_at(Tick(11));
    driver.restore(loaded.pending, &loaded.last_seq);
    let mut rest = run_until(&mut resumed, &mut driver, 20, script);
    // The stamper continues above the pending command's seq, so the second move is not a
    // duplicate.
    let second = driver.issue(second_move(&resumed));
    assert_eq!(second.seq, pending[0].seq + 1);
    assert_eq!(second.exec_tick, Tick(22));
    rest.extend(run_until(&mut resumed, &mut driver, 30, script));
    assert_eq!(rest, full[11..]);

    // Both moves applied: #1 left (100, 100) and now heads for the second target.
    let world = resumed.world();
    let unit = world.resource::<SimIndex>().get(SimId(1)).expect("alive");
    let pose = world.get::<fl_sim::Pose>(unit).expect("posed");
    assert_ne!(pose.pos, v(100, 100));
    let order = world.get::<MoveOrder>(unit).expect("has a move order");
    assert_eq!(
        order.orders,
        [Order::Move {
            target: v(150, 600)
        }]
    );
}

#[test]
fn load_rejects_unknown_component_name() {
    let runner = duel();
    let mut save = decode(&save_world(runner.world(), &[]).expect("saves"));
    let mut renamed = 0;
    for (_, components) in &mut save.sim.entities {
        for (name, _, _) in components.iter_mut() {
            if name == "core.pose" {
                "core.posee".clone_into(name);
                renamed += 1;
            }
        }
    }
    assert_eq!(renamed, 2);
    match load_err(&[], "core_mod", &encode(&save)) {
        LoadError::UnknownComponent(name) => assert_eq!(name, "core.posee"),
        other => panic!("expected UnknownComponent, got {other}"),
    }
}

#[test]
fn load_rejects_unknown_resource_name() {
    let runner = duel();
    let mut save = decode(&save_world(runner.world(), &[]).expect("saves"));
    let entry = save
        .sim
        .resources
        .iter_mut()
        .find(|(name, _, _)| name == "core.players")
        .expect("the players are saved");
    "core.playerz".clone_into(&mut entry.0);
    match load_err(&[], "core_mod", &encode(&save)) {
        LoadError::UnknownResource(name) => assert_eq!(name, "core.playerz"),
        other => panic!("expected UnknownResource, got {other}"),
    }
}

/// Registers nothing: only its presence in the header differs.
struct ExtraFeature;

impl SimFeature for ExtraFeature {
    fn id(&self) -> &'static str {
        "extra"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, _sim: &mut SimBuilder) {}
}

#[test]
fn load_rejects_feature_mismatch() {
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(ExtraFeature)];
    let runner = duel_in(&features, "core_mod");
    let bytes = save_world(runner.world(), &[]).expect("saves");
    assert!(matches!(
        load_err(&[], "core_mod", &bytes),
        LoadError::Header(HeaderMismatch::Features)
    ));
}

#[test]
fn load_rejects_other_format_versions() {
    let runner = duel();
    let mut save = decode(&save_world(runner.world(), &[]).expect("saves"));
    save.format_version = SAVE_FORMAT_VERSION + 1;
    match load_err(&[], "core_mod", &encode(&save)) {
        LoadError::FormatVersion { found, supported } => {
            assert_eq!(
                (found, supported),
                (SAVE_FORMAT_VERSION + 1, SAVE_FORMAT_VERSION)
            );
        }
        other => panic!("expected FormatVersion, got {other}"),
    }
    assert!(matches!(
        load_err(&[], "core_mod", &[1, 2, 3]),
        LoadError::Decode(_)
    ));
}

mod v1 {
    use serde::{Deserialize, Serialize};

    /// `TestHp` as saved at version 1: one field.
    #[derive(Serialize, Deserialize)]
    pub struct TestHp {
        pub hp: u16,
    }
}

/// Version 2: `hp` and `max`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct TestHp {
    hp: u32,
    max: u32,
}

impl From<v1::TestHp> for TestHp {
    fn from(old: v1::TestHp) -> Self {
        TestHp {
            hp: old.hp.into(),
            max: old.hp.into(),
        }
    }
}

impl SimHash for TestHp {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.hp.sim_hash(h);
        self.max.sim_hash(h);
    }
}

struct TestHpFeature;

impl SimFeature for TestHpFeature {
    fn id(&self) -> &'static str {
        "test_hp"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.register_component::<TestHp>(
            "test_hp.hp",
            Class::Hashed(Codec::new(2).migrate_from::<v1::TestHp>(1)),
        );
    }
}

/// A save of the duel where #1 carries `test_hp.hp` at `version` with `bytes`.
fn save_with_test_hp(version: u32, bytes: Vec<u8>) -> Vec<u8> {
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(TestHpFeature)];
    let mut runner = duel_in(&features, "core_mod");
    let world = runner.world_mut();
    let unit = world.resource::<SimIndex>().get(SimId(1)).expect("alive");
    world.entity_mut(unit).insert(TestHp { hp: 5, max: 9 });
    let mut save = decode(&save_world(runner.world(), &[]).expect("saves"));
    let (_, components) = &mut save.sim.entities[0];
    let entry = components
        .iter_mut()
        .find(|(name, _, _)| name == "test_hp.hp")
        .expect("#1 has test_hp.hp");
    assert_eq!(entry.1, 2);
    entry.1 = version;
    entry.2 = bytes;
    encode(&save)
}

fn test_hp_of(app: &App, id: u32) -> TestHp {
    let world = app.world();
    let unit = world.resource::<SimIndex>().get(SimId(id)).expect("alive");
    *world.get::<TestHp>(unit).expect("has TestHp")
}

#[test]
fn migration_from_v1() {
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(TestHpFeature)];
    let old = postcard::to_allocvec(&v1::TestHp { hp: 77 }).expect("encodes");
    let loaded = load(&features, "core_mod", &save_with_test_hp(1, old));
    assert_eq!(test_hp_of(&loaded.app, 1), TestHp { hp: 77, max: 77 });

    // The current version decodes as is; a resave writes it at version 2.
    let current = postcard::to_allocvec(&TestHp { hp: 3, max: 4 }).expect("encodes");
    let loaded = load(&features, "core_mod", &save_with_test_hp(2, current));
    assert_eq!(test_hp_of(&loaded.app, 1), TestHp { hp: 3, max: 4 });

    // No migration from version 0, and bad bytes are a codec error.
    match load_err(&features, "core_mod", &save_with_test_hp(0, vec![1])) {
        LoadError::NoMigration { name, from, to } => {
            assert_eq!((name.as_str(), from, to), ("test_hp.hp", 0, 2));
        }
        other => panic!("expected NoMigration, got {other}"),
    }
    match load_err(&features, "core_mod", &save_with_test_hp(1, vec![0xFF; 5])) {
        LoadError::Codec { name, .. } => assert_eq!(name, "test_hp.hp"),
        other => panic!("expected Codec, got {other}"),
    }
}

fn unit_name(app: &App, id: u32) -> String {
    let world = app.world();
    let unit = world.resource::<SimIndex>().get(SimId(id)).expect("alive");
    let unit_type = world.get::<UnitType>(unit).expect("a unit").0;
    let content = &world.resource::<Content>().0;
    content.unit(unit_type).expect("a known unit").id.clone()
}

#[test]
fn load_remaps_unit_type_ids() {
    let runner = duel();
    let before = save_world(runner.world(), &[]).expect("saves");
    let original = SimRunner::new(load(&[], "core_mod", &before).app);
    let loaded = load(&[], "core_mod_extra", &before);

    let old_id = content(&[], "core_mod").unit_id("core_test.tank");
    let new_id = content(&[], "core_mod_extra").unit_id("core_test.tank");
    assert_ne!(old_id, new_id, "core_mod_extra renumbers the tank");
    for id in [1, 2] {
        assert_eq!(unit_name(&loaded.app, id), "core_test.tank");
    }
    assert_eq!(
        compute_state_hash(loaded.app.world()),
        compute_state_hash(original.world())
    );
}

#[test]
fn load_rejects_missing_string_table() {
    let runner = duel();
    let mut save = decode(&save_world(runner.world(), &[]).expect("saves"));
    save.header.string_tables.tables.remove(tables::UNITS);
    match load_err(&[], "core_mod", &encode(&save)) {
        LoadError::Codec { name, message } => {
            assert_eq!(name, "core.unit_type");
            assert!(message.contains(tables::UNITS), "{message}");
        }
        other => panic!("expected Codec, got {other}"),
    }
}

#[test]
fn derived_state_rebuilt_on_load() {
    let mut runner = crowded_duel();
    let mut driver = TestDriver::new();
    run_until(&mut runner, &mut driver, 40, crowded_script);
    let before: Vec<SimId> = runner.world().resource::<SimIndex>().ids().collect();
    assert_eq!(before.len(), 4);

    let loaded = load(
        &despawner(),
        "core_mod",
        &save_world(runner.world(), &[]).expect("saves"),
    );
    let world = loaded.app.world();
    let index = world.resource::<SimIndex>();
    assert_eq!(index.ids().collect::<Vec<_>>(), before);
    for (id, entity) in index.iter() {
        assert_eq!(world.get::<SimEntity>(entity).map(SimEntity::id), Some(id));
    }
}

fn waiting_spawn() -> SpawnRequest {
    SpawnRequest {
        unit: None,
        owner: PlayerId(0),
        pos: v(10, 10),
        heading: Angle::ZERO,
        init: None,
    }
}

#[test]
fn save_with_pending_spawn_requests_is_an_error() {
    let mut runner = duel();
    runner.world_mut().write_message(waiting_spawn());
    assert_eq!(
        save_world(runner.world(), &[]),
        Err(SaveError::PendingRequests {
            spawns: 1,
            despawns: 0
        })
    );
}

#[test]
fn save_with_waiting_despawn_request_is_an_error() {
    let mut runner = duel();
    runner.world_mut().write_message(DespawnRequest(SimId(1)));
    let err = save_world(runner.world(), &[]).expect_err("a request waits");
    assert!(err.to_string().contains("1 despawn request"), "{err}");
}

#[test]
fn failed_save_query_replies_with_an_error_and_the_match_runs_on() {
    let mut runner = duel();
    runner.world_mut().write_message(waiting_spawn());
    let reply = runner.query(SimQuery::Save {
        pending: Vec::new(),
    });
    let SimReply::Error(message) = reply else {
        panic!("expected SimReply::Error, got {reply:?}");
    };
    assert!(message.contains("1 spawn request"), "{message}");
    // The runner is still usable: the next tick's funnel takes the request.
    let mut driver = TestDriver::new();
    run_until(&mut runner, &mut driver, 1, no_script);
    let reply = runner.query(SimQuery::Save {
        pending: Vec::new(),
    });
    assert!(matches!(reply, SimReply::Save(_)), "{reply:?}");
}

/// Each player's faction id, as its name in the sim's header.
fn faction_names(app: &App) -> Vec<String> {
    let world = app.world();
    let names = world
        .resource::<MatchHeaderRes>()
        .0
        .string_tables
        .get("content.faction")
        .expect("the faction table")
        .to_vec();
    world
        .resource::<Players>()
        .list
        .iter()
        .map(|p| names[usize::from(p.faction.0)].clone())
        .collect()
}

fn faction_ids(app: &App) -> Vec<u16> {
    let players = app.world().resource::<Players>();
    players.list.iter().map(|p| p.faction.0).collect()
}

#[test]
fn faction_renumbering_keeps_the_hash() {
    // core_mod_factions adds a faction that sorts first: red and blue get new dense ids.
    let mut plain = duel();
    let mut renumbered = duel_in(&[], "core_mod_factions");
    let plain_app = load(
        &[],
        "core_mod",
        &save_world(plain.world(), &[]).expect("saves"),
    );
    let renumbered_app = load(
        &[],
        "core_mod_factions",
        &save_world(renumbered.world(), &[]).expect("saves"),
    );
    assert_ne!(
        faction_ids(&plain_app.app),
        faction_ids(&renumbered_app.app)
    );
    assert_eq!(
        faction_names(&plain_app.app),
        faction_names(&renumbered_app.app)
    );

    let a = run_until(&mut plain, &mut TestDriver::new(), 10, moves_script);
    let b = run_until(&mut renumbered, &mut TestDriver::new(), 10, moves_script);
    assert_eq!(a, b);
}

#[test]
fn load_remaps_faction_ids() {
    let runner = duel();
    let before = save_world(runner.world(), &[]).expect("saves");
    let original = load(&[], "core_mod", &before);
    let loaded = load(&[], "core_mod_factions", &before);

    assert_eq!(
        faction_names(&loaded.app),
        ["core_test.red", "core_test.blue"]
    );
    assert_eq!(faction_names(&original.app), faction_names(&loaded.app));
    assert_ne!(faction_ids(&original.app), faction_ids(&loaded.app));
    assert_eq!(
        compute_state_hash(loaded.app.world()),
        compute_state_hash(original.app.world())
    );
}

#[test]
fn runner_save_query() {
    let mut runner = duel();
    let mut driver = TestDriver::new();
    run_until(&mut runner, &mut driver, 5, moves_script);
    let reply = runner.query(SimQuery::Save {
        pending: Vec::new(),
    });
    let SimReply::Save(bytes) = reply else {
        panic!("expected SimReply::Save, got {reply:?}");
    };
    assert_eq!(bytes, save_world(runner.world(), &[]).expect("saves"));
    let loaded = load(&[], "core_mod", &bytes);
    assert_eq!(loaded.app.world().resource::<SimClock>().tick, Tick(5));
}

/// `"aaa.noop"`: does nothing; its name sorts before every core command.
#[derive(Debug, Serialize, Deserialize)]
struct NoopCmd;

impl CommandPayload for NoopCmd {
    const NAME: &'static str = "aaa.noop";
    const NEEDS_UNITS: bool = false;
}

struct NoopHandler;

impl fl_sim::CommandHandler<NoopCmd> for NoopHandler {
    fn apply(
        &self,
        _payload: &NoopCmd,
        _world: &mut World,
        _ctx: &fl_sim::CommandCtx<'_>,
    ) -> Result<(), fl_sim::RejectReason> {
        Ok(())
    }
}

/// Feature `"aaa"` (version 1), which registers `"aaa.noop"` only when `.0` is set: the same
/// feature list with a different command table.
struct NoopFeature(bool);

impl SimFeature for NoopFeature {
    fn id(&self) -> &'static str {
        "aaa"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        if self.0 {
            sim.register_command::<NoopCmd>(NoopHandler);
        }
    }
}

/// A save of the duel (with `NoopFeature(false)`) taken while a move of #1 waits in the
/// driver (input delay 2).
fn save_with_pending_move() -> Vec<u8> {
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(NoopFeature(false))];
    let mut runner = duel_in(&features, "core_mod");
    let mut driver = TestDriver::with_delay(2);
    driver.issue(draft(
        &runner,
        0,
        &[1],
        &MoveCmd {
            target: v(400, 400),
        },
    ));
    run_until(&mut runner, &mut driver, 1, no_script);
    assert_eq!(driver.pending().len(), 1);
    save_world(runner.world(), &driver.pending()).expect("saves")
}

#[test]
fn pending_command_ids_remap_on_load() {
    let bytes = save_with_pending_move();
    let saved_id = decode(&bytes).sim.pending_commands[0].kind.type_id;

    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(NoopFeature(true))];
    let loaded = load(&features, "core_mod", &bytes);
    let registry = loaded.app.world().resource::<CommandRegistry>();
    let move_id = registry.type_id::<MoveCmd>();
    assert_ne!(
        saved_id, move_id,
        "aaa.noop sorts first and shifts core.move"
    );
    assert_eq!(loaded.pending[0].kind.type_id, move_id);

    // Delivered, the pending command is still the move.
    let mut resumed = SimRunner::new(loaded.app);
    let mut driver = TestDriver::with_delay(2).starting_at(Tick(1));
    driver.restore(loaded.pending, &loaded.last_seq);
    run_until(&mut resumed, &mut driver, 3, no_script);
    let world = resumed.world();
    let unit = world.resource::<SimIndex>().get(SimId(1)).expect("alive");
    let order = world.get::<MoveOrder>(unit).expect("has a move order");
    assert_eq!(
        order.orders,
        [Order::Move {
            target: v(400, 400)
        }]
    );
}

#[test]
fn pending_command_with_unknown_name_fails_to_load() {
    let mut save = decode(&save_with_pending_move());
    let commands = save
        .header
        .string_tables
        .tables
        .get_mut(tables::COMMANDS)
        .expect("the commands table");
    let name = commands
        .iter_mut()
        .find(|name| *name == "core.move")
        .expect("core.move");
    "core.gone".clone_into(name);
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(NoopFeature(false))];
    match load_err(&features, "core_mod", &encode(&save)) {
        LoadError::Command(message) => assert!(message.contains("core.gone"), "{message}"),
        other => panic!("expected Command, got {other}"),
    }
}

/// [`TestHpFeature`] one feature version later: the same registrations, version 2.
struct TestHpFeatureV2;

impl SimFeature for TestHpFeatureV2 {
    fn id(&self) -> &'static str {
        "test_hp"
    }

    fn version(&self) -> u32 {
        2
    }

    fn build(&self, sim: &mut SimBuilder) {
        TestHpFeature.build(sim);
    }
}

#[test]
fn save_of_an_older_feature_version_loads() {
    // R70: the feature version may be older; the codec versions and migrations decide.
    let old = postcard::to_allocvec(&v1::TestHp { hp: 12 }).expect("encodes");
    let bytes = save_with_test_hp(1, old);
    assert_eq!(decode(&bytes).header.features[1], ("test_hp".to_owned(), 1));
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(TestHpFeatureV2)];
    let loaded = load(&features, "core_mod", &bytes);
    assert_eq!(test_hp_of(&loaded.app, 1), TestHp { hp: 12, max: 12 });

    // A save of a newer feature version than the build's does not.
    let mut newer = decode(&bytes);
    newer.header.features[1].1 = 3;
    assert!(matches!(
        load_err(&features, "core_mod", &encode(&newer)),
        LoadError::Header(HeaderMismatch::Features)
    ));
}

/// The order the `Derived` rebuilds ran in after a load.
#[derive(Resource, Default)]
struct RebuildOrder(Vec<&'static str>);

fn rebuild_zzz(world: &mut World) {
    world.resource_mut::<RebuildOrder>().0.push("zzz");
}

fn rebuild_aaa(world: &mut World) {
    world.resource_mut::<RebuildOrder>().0.push("aaa");
}

/// A feature with one rebuild: `"aaa"` depends on `"zzz"`, so it loads after it although its
/// name sorts first.
struct RebuildFeature {
    id: &'static str,
    deps: &'static [&'static str],
    rebuild: fn(&mut World),
}

impl SimFeature for RebuildFeature {
    fn id(&self) -> &'static str {
        self.id
    }

    fn version(&self) -> u32 {
        1
    }

    fn deps(&self) -> &'static [&'static str] {
        self.deps
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().init_resource::<RebuildOrder>();
        sim.register_rebuild(self.rebuild);
    }
}

fn rebuild_features() -> Vec<Arc<dyn SimFeature>> {
    vec![
        Arc::new(RebuildFeature {
            id: "aaa",
            deps: &["zzz"],
            rebuild: rebuild_aaa,
        }),
        Arc::new(RebuildFeature {
            id: "zzz",
            deps: &[],
            rebuild: rebuild_zzz,
        }),
    ]
}

#[test]
fn derived_rebuilds_run_once_per_feature_in_load_order() {
    let features = rebuild_features();
    let runner = duel_in(&features, "core_mod");
    let loaded = load(
        &features,
        "core_mod",
        &save_world(runner.world(), &[]).expect("saves"),
    );
    assert_eq!(
        loaded.app.world().resource::<RebuildOrder>().0,
        ["zzz", "aaa"],
        "feature load order, not name order"
    );
}

/// Registers two rebuilds: a feature has one.
struct TwoRebuildsFeature;

impl SimFeature for TwoRebuildsFeature {
    fn id(&self) -> &'static str {
        "two"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.register_rebuild(rebuild_aaa)
            .register_rebuild(rebuild_zzz);
    }
}

#[test]
#[should_panic(expected = "'two' registers a second Derived rebuild")]
fn a_second_rebuild_panics() {
    let _ = fl_sim::sim_registry(&[Arc::new(TwoRebuildsFeature)]);
}

#[test]
fn pending_command_without_a_commands_table_fails_to_load() {
    let mut save = decode(&save_with_pending_move());
    save.header.string_tables.tables.remove(tables::COMMANDS);
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(NoopFeature(false))];
    match load_err(&features, "core_mod", &encode(&save)) {
        LoadError::Command(message) => {
            assert!(message.contains("no string table 'commands'"), "{message}");
        }
        other => panic!("expected Command, got {other}"),
    }
}

/// `"aaa.extra"`: state that version 2 of feature `"aaa"` adds.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Extra(u32);

impl SimHash for Extra {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.0.sim_hash(h);
    }
}

fn fill_extra(world: &mut World) -> Result<(), String> {
    world.insert_resource(Extra(42));
    Ok(())
}

fn fail_extra(_world: &mut World) -> Result<(), String> {
    Err("x".to_owned())
}

/// Feature `"aaa"`: version 1 has no state; version 2 adds Hashed `"aaa.extra"` with `fill`.
struct AaaFeature {
    version: u32,
    fill: fl_sim::FillFn,
}

impl SimFeature for AaaFeature {
    fn id(&self) -> &'static str {
        "aaa"
    }

    fn version(&self) -> u32 {
        self.version
    }

    fn build(&self, sim: &mut SimBuilder) {
        if self.version >= 2 {
            sim.register_resource::<Extra>(
                "aaa.extra",
                Class::Hashed(Codec::new(1).fill_missing(self.fill)),
            );
        }
    }
}

fn aaa(version: u32, fill: fl_sim::FillFn) -> Vec<Arc<dyn SimFeature>> {
    vec![Arc::new(AaaFeature { version, fill })]
}

#[test]
fn fill_missing_runs_for_an_older_feature_save() {
    let old = duel_in(&aaa(1, fill_extra), "core_mod");
    let bytes = save_world(old.world(), &[]).expect("saves");
    assert_eq!(decode(&bytes).header.features[1], ("aaa".to_owned(), 1));
    let loaded = load(&aaa(2, fill_extra), "core_mod", &bytes);
    assert_eq!(loaded.app.world().get_resource::<Extra>(), Some(&Extra(42)));

    // The filled value is saved like any other: the next load reads it.
    let resaved = save_world(loaded.app.world(), &[]).expect("saves");
    let again = load(&aaa(2, fail_extra), "core_mod", &resaved);
    assert_eq!(again.app.world().get_resource::<Extra>(), Some(&Extra(42)));
}

#[test]
fn fill_missing_skipped_for_the_same_version() {
    // A v2 save without "aaa.extra" (the resource was never inserted): M0 behaviour.
    let features = aaa(2, fill_extra);
    let runner = duel_in(&features, "core_mod");
    assert!(runner.world().get_resource::<Extra>().is_none());
    let bytes = save_world(runner.world(), &[]).expect("saves");
    let loaded = load(&features, "core_mod", &bytes);
    assert!(loaded.app.world().get_resource::<Extra>().is_none());
}

#[test]
fn fill_error_is_a_codec_load_error() {
    let old = duel_in(&aaa(1, fail_extra), "core_mod");
    let bytes = save_world(old.world(), &[]).expect("saves");
    match load_err(&aaa(2, fail_extra), "core_mod", &bytes) {
        LoadError::Codec { name, message } => {
            assert_eq!((name.as_str(), message.as_str()), ("aaa.extra", "x"));
        }
        other => panic!("expected Codec, got {other}"),
    }
}
