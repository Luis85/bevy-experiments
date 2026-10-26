//! The state hash (name-keyed, with Hashed resources), the hash interval, content registry
//! checks and inspection.

use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_ecs::world::EntityWorldMut;
use fl_content::{DefComponent, SpawnCtx};
use fl_math::{Angle, FxVec2};
use fl_protocol::{Controller, FactionId, PlayerId, SimId, TeamId, Tick, UnitTypeId};
use fl_sim::{
    BuildError, Class, Content, HashInterval, Invulnerable, MatchHeaderRes, MatchSetup, PlayerInfo,
    Players, RunConfig, SaveFile, SimBuilder, SimClock, SimFeature, SimHash, SimIdAllocator,
    SimIndex, SpawnRequest, StateHasher, UnitType, build_sim_app, compute_state_hash,
    inspect_entity, inspect_world, load_content, load_world, save_world, setup_despawn, setup_map,
    setup_scenario, setup_spawn, step_tick,
};
use fl_view::{TickInput, TickOutput};
use serde::{Deserialize, Serialize};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn core_mod() -> PathBuf {
    fixture("core_mod")
}

#[derive(Component, Debug, Serialize, Deserialize)]
struct Hp(u32);

impl SimHash for Hp {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.0.sim_hash(h);
    }
}

struct HpFeature;

impl SimFeature for HpFeature {
    fn id(&self) -> &'static str {
        "hp"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.register_component::<Hp>("hp.hp", Class::hashed(1));
    }
}

fn build_seeded(seed: u64, hash_interval: Option<u32>) -> App {
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(HpFeature)];
    let content = load_content(&features, &[core_mod()]).expect("core_mod loads");
    let setup = MatchSetup {
        seed,
        cheats_allowed: false,
    };
    let run = RunConfig {
        hash_interval,
        ..Default::default()
    };
    build_sim_app(&features, content, setup, run).expect("the sim builds")
}

fn build_with(hash_interval: Option<u32>) -> App {
    build_seeded(7, hash_interval)
}

/// Add #1 with `Hp(100)` and #2 without. Bare entities with fixed ids, without a map or
/// players: only fl_sim creates a `SimEntity`, so they come in through a save of `app` that
/// lists them, loaded with `app`'s seed and hash interval.
fn populate(app: &mut App) {
    let world = app.world();
    let bytes = save_world(world, &[]).expect("saves");
    let mut save: SaveFile = postcard::from_bytes(&bytes).expect("a SaveFile");
    let hp = postcard::to_allocvec(&Hp(100)).expect("encodes");
    save.sim.entities = vec![
        (SimId(1), vec![("hp.hp".to_owned(), 1, hp)]),
        (SimId(2), Vec::new()),
    ];
    save.sim.sim_id_next = 2;
    let bytes = postcard::to_allocvec(&save).expect("a SaveFile encodes");
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(HpFeature)];
    let content = Arc::clone(&world.resource::<Content>().0);
    let run = RunConfig {
        hash_interval: Some(world.resource::<HashInterval>().0),
        ..Default::default()
    };
    match load_world(&features, content, &bytes, run) {
        Ok(loaded) => *app = loaded.app,
        Err(err) => panic!("the populated save loads: {err}"),
    }
}

/// A sim with two entities: #1 with `Hp(100)`, #2 without.
fn build() -> App {
    let mut app = build_with(None);
    populate(&mut app);
    app
}

fn step(app: &mut App, ticks: u32) -> Vec<TickOutput> {
    (0..ticks)
        .map(|_| {
            let tick = app.world().resource::<SimClock>().tick;
            let input = TickInput {
                tick,
                commands: Vec::new(),
            };
            step_tick(app.world_mut(), input)
        })
        .collect()
}

fn hash(app: &App) -> u64 {
    compute_state_hash(app.world())
}

#[test]
fn equal_worlds_hash_equal() {
    let mut a = build();
    let mut b = build();
    assert_eq!(hash(&a), hash(&b));
    step(&mut a, 5);
    step(&mut b, 5);
    assert_eq!(hash(&a), hash(&b));
}

#[test]
fn component_change_changes_hash() {
    let a = build();
    let mut b = build();
    let before = hash(&b);
    let mut query = b.world_mut().query::<&mut Hp>();
    query.single_mut(b.world_mut()).expect("one Hp").0 = 99;
    assert_ne!(hash(&a), hash(&b));
    assert_ne!(before, hash(&b));
}

#[test]
fn tick_changes_hash() {
    let mut a = build();
    let before = hash(&a);
    step(&mut a, 1);
    assert_ne!(before, hash(&a));
}

#[test]
fn seed_changes_hash() {
    let mut a = build_seeded(7, None);
    let mut b = build_seeded(8, None);
    populate(&mut a);
    populate(&mut b);
    assert_ne!(hash(&a), hash(&b));
}

/// The ticks whose output carries a hash.
fn hashed_ticks(outputs: &[TickOutput]) -> Vec<Tick> {
    outputs
        .iter()
        .filter(|out| out.hash.is_some())
        .map(|out| out.tick)
        .collect()
}

#[test]
fn hash_every_interval() {
    let mut app = build_with(Some(5));
    let outputs = step(&mut app, 20);
    assert_eq!(
        hashed_ticks(&outputs),
        [Tick(0), Tick(5), Tick(10), Tick(15)]
    );
}

#[test]
fn hash_interval_defaults_to_rules() {
    let mut app = build_with(None);
    assert_eq!(*app.world().resource::<HashInterval>(), HashInterval(20));
    let outputs = step(&mut app, 21);
    assert_eq!(hashed_ticks(&outputs), [Tick(0), Tick(20)]);
}

#[test]
fn zero_interval_turns_hashing_off() {
    let mut app = build_with(Some(0));
    assert!(hashed_ticks(&step(&mut app, 3)).is_empty());
}

#[test]
fn output_hash_is_taken_before_the_clock_advances() {
    let mut a = build_with(Some(1));
    let mut b = build_with(Some(1));
    let output = step(&mut a, 1).remove(0);
    // The same state with the clock still at tick 0: what a tick with no systems leaves.
    step(&mut b, 1);
    b.world_mut().resource_mut::<SimClock>().tick = Tick(0);
    assert_eq!(output.hash, Some(hash(&b)));
}

#[test]
fn resources_are_hashed() {
    let a = build();
    let mut b = build();
    b.world_mut()
        .resource_mut::<Players>()
        .list
        .push(PlayerInfo {
            id: PlayerId(0),
            faction: FactionId(0),
            team: TeamId(0),
            controller: Controller::None,
        });
    assert_ne!(hash(&a), hash(&b));
}

#[test]
fn external_resources_not_hashed() {
    let mut a = build_with(Some(1));
    let mut b = build_with(Some(2));
    populate(&mut a);
    populate(&mut b);
    let ha = step(&mut a, 1)[0].hash;
    let hb = step(&mut b, 1)[0].hash;
    assert!(ha.is_some());
    assert_eq!(ha, hb);
}

/// The duel, loaded from fixture mod `mod_dir`.
fn duel_from(mod_dir: &str) -> App {
    let content = load_content(&[], &[fixture(mod_dir)]).expect("the mod loads");
    let setup = MatchSetup {
        seed: 3,
        cheats_allowed: false,
    };
    let mut app = build_sim_app(&[], content, setup, RunConfig::default()).expect("the sim builds");
    setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
    app
}

fn entity_of(app: &App, id: u32) -> Entity {
    app.world()
        .resource::<SimIndex>()
        .get(SimId(id))
        .expect("unit exists")
}

/// A zero-sized Hashed marker (`Invulnerable`) has no data: its presence alone moves the
/// hash, and which entity carries it matters.
#[test]
fn presence_of_a_marker_changes_hash() {
    let mut app = duel_from("core_mod");
    let plain = hash(&app);
    let first = entity_of(&app, 1);
    app.world_mut().entity_mut(first).insert(Invulnerable);
    let on_first = hash(&app);
    assert_ne!(on_first, plain, "inserting the marker changes the hash");

    app.world_mut().entity_mut(first).remove::<Invulnerable>();
    assert_eq!(hash(&app), plain, "removing it restores the hash");

    let second = entity_of(&app, 2);
    app.world_mut().entity_mut(second).insert(Invulnerable);
    let on_second = hash(&app);
    assert_ne!(on_second, plain);
    assert_ne!(on_second, on_first, "the marker's owner matters");
}

fn unit_type_of(app: &App, id: u32) -> UnitTypeId {
    app.world()
        .get::<UnitType>(entity_of(app, id))
        .expect("unit type")
        .0
}

#[test]
fn unit_type_hashes_by_name_not_dense_id() {
    let plain = duel_from("core_mod");
    let mut extra = duel_from("core_mod_extra");
    // "core_test.aaa_first" sorts before the tank, so the tank's dense id moves.
    assert_eq!(unit_type_of(&plain, 1), UnitTypeId(0));
    assert_eq!(unit_type_of(&extra, 1), UnitTypeId(1));
    assert_eq!(hash(&plain), hash(&extra));

    // The unit type is part of the hash: retyping #1 as aaa_first changes it.
    let aaa = extra
        .world()
        .resource::<Content>()
        .0
        .unit_id("core_test.aaa_first")
        .expect("aaa_first is concrete");
    let entity = entity_of(&extra, 1);
    extra.world_mut().entity_mut(entity).insert(UnitType(aaa));
    assert_ne!(hash(&plain), hash(&extra));
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtraDef;

impl DefComponent for ExtraDef {
    fn instantiate(&self, _ctx: &SpawnCtx<'_>, _entity: &mut EntityWorldMut<'_>) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

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
fn content_registry_mismatch_is_error() {
    let content = load_content(&[], &[core_mod()]).expect("core_mod loads");
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(ExtraFeature)];
    let result = build_sim_app(
        &features,
        content,
        MatchSetup::default(),
        RunConfig::default(),
    );
    assert!(
        matches!(result, Err(BuildError::RegistryMismatch)),
        "{:?}",
        result.err()
    );
}

#[test]
fn inspect_entity_dumps_registered_components() {
    let app = build();
    let first = inspect_entity(app.world(), SimId(1)).expect("#1 exists");
    assert_eq!(
        first,
        serde_json::json!({ "sim_id": 1, "components": { "hp.hp": 100 } })
    );
    let second = inspect_entity(app.world(), SimId(2)).expect("#2 exists");
    assert_eq!(second, serde_json::json!({ "sim_id": 2, "components": {} }));
    assert_eq!(inspect_entity(app.world(), SimId(3)), None);
}

#[test]
fn inspect_world_summarizes_state() {
    let mut app = build();
    step(&mut app, 2);
    let world = inspect_world(app.world());
    assert_eq!(world["tick"], 2);
    assert_eq!(world["seed"], 7);
    let header = &app.world().resource::<MatchHeaderRes>().0;
    assert_eq!(
        world["header"],
        serde_json::json!({
            "features": [["core", 2], ["hp", 1]],
            "mods": [["core_test", "0.1.0"]],
            "rules_hash": header.rules_hash,
        })
    );
    assert_eq!(world["state_hash"], hash(&app));
    assert_eq!(world["entities"], serde_json::json!([1, 2]));
    assert_eq!(world["players"], serde_json::json!([]));
    assert!(world["map"].is_null());
    assert!(world.get("rng").is_none(), "no RNG state exists");
}

/// Spawn #1 (with `Hp(100)`) and #2 through `setup_spawn`.
fn spawn_two(world: &mut World) {
    let request = SpawnRequest {
        unit: None,
        owner: PlayerId(0),
        pos: FxVec2::ZERO,
        heading: Angle::ZERO,
        init: None,
    };
    let first = setup_spawn(world, &request).expect("spawns");
    let second = setup_spawn(world, &request).expect("spawns");
    assert_eq!((first, second), (SimId(1), SimId(2)));
    let entity = world.resource::<SimIndex>().get(first).expect("#1");
    world.entity_mut(entity).insert(Hp(100));
}

#[test]
fn hash_independent_of_entity_bits() {
    let mut a = with_map_and_player(build_with(None));
    spawn_two(a.world_mut());
    // Same SimIds and components, but filler entities shift every Entity index (the opposite
    // spawn order is a unit test in fl_sim, where a SimEntity can be made directly).
    let mut b = with_map_and_player(build_with(None));
    let world = b.world_mut();
    let filler = world.spawn_empty().id();
    world.despawn(filler);
    let filler = world.spawn(Hp(5)).id();
    world.despawn(filler);
    spawn_two(world);
    let (ea, eb) = (
        a.world().resource::<SimIndex>().get(SimId(1)),
        b.world().resource::<SimIndex>().get(SimId(1)),
    );
    assert_ne!(ea, eb, "the worlds allocate different entities");
    assert_eq!(hash(&a), hash(&b));
}

/// Set up the flat map and one player.
fn with_map_and_player(mut app: App) -> App {
    setup_map(app.world_mut(), "core_test.flat").expect("the map exists");
    app.world_mut()
        .resource_mut::<Players>()
        .list
        .push(PlayerInfo {
            id: PlayerId(0),
            faction: FactionId(0),
            team: TeamId(0),
            controller: Controller::None,
        });
    app
}

#[test]
fn sim_id_allocator_changes_hash() {
    // No populate(): its fixed ids would collide with allocated ones.
    let a = with_map_and_player(build_with(None));
    let mut b = with_map_and_player(build_with(None));
    // Spawning and despawning leaves only the allocator changed.
    let request = SpawnRequest {
        unit: None,
        owner: PlayerId(0),
        pos: FxVec2::ZERO,
        heading: Angle::ZERO,
        init: None,
    };
    let id = setup_spawn(b.world_mut(), &request).expect("spawns");
    assert!(setup_despawn(b.world_mut(), id));
    assert_eq!(b.world().resource::<SimIdAllocator>().issued(), 1);
    assert_ne!(hash(&a), hash(&b));
}
