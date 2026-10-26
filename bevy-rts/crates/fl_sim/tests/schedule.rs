//! The SimTick schedule: phases and hooks, sync points, messages, step_tick housekeeping,
//! the SimId index, ambiguity checks, features, events and counters.

use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_ecs::world::EntityWorldMut;
use fl_content::{DefComponent, SpawnCtx};
use fl_math::{Angle, FxVec2};
use fl_protocol::{
    Controller, FactionId, MoveCmd, PlayerCommand, PlayerId, SimId, TeamId, Tick, tables,
};
use fl_sim::{
    ActSet, CommandLog, CommandRegistry, EconomySet, InputSet, LifecycleSet, MatchHeaderRes,
    MatchSetup, PerceiveSet, PlayerInfo, Players, PublishSet, RejectReason, ResolveSet, RunConfig,
    SimBuilder, SimClock, SimCounters, SimEntity, SimEvents, SimFeature, SimIndex, SpawnRequest,
    ThinkSet, build_sim_app, load_content, setup_map, setup_spawn, sim_registry, step_tick,
    system_order,
};
use fl_view::{SimEvent, TickInput};
use serde::{Deserialize, Serialize};

fn core_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod")
}

fn build(features: Vec<Arc<dyn SimFeature>>) -> App {
    let content = load_content(&features, &[core_mod()]).expect("core_mod loads");
    let setup = MatchSetup {
        seed: 1,
        cheats_allowed: false,
    };
    build_sim_app(&features, content, setup, RunConfig::default()).expect("the sim builds")
}

fn step(app: &mut App, ticks: u32) {
    for _ in 0..ticks {
        let tick = app.world().resource::<SimClock>().tick;
        let input = TickInput {
            tick,
            commands: Vec::new(),
        };
        step_tick(app.world_mut(), input);
    }
}

#[derive(Resource, Default)]
struct Trace(Vec<&'static str>);

fn tracer(name: &'static str) -> impl FnMut(ResMut<Trace>) {
    move |mut trace: ResMut<Trace>| trace.0.push(name)
}

/// One system per hook, each pushing its hook's name.
struct TraceFeature;

impl SimFeature for TraceFeature {
    fn id(&self) -> &'static str {
        "trace"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().init_resource::<Trace>();
        // Added in reverse so only the hook order can put them right.
        sim.add_systems(PublishSet::Events, tracer("Publish/Events"))
            .add_systems(PublishSet::Snapshot, tracer("Publish/Snapshot"))
            .add_systems(LifecycleSet::Despawn, tracer("Lifecycle/Despawn"))
            .add_systems(LifecycleSet::Spawn, tracer("Lifecycle/Spawn"))
            .add_systems(LifecycleSet::Death, tracer("Lifecycle/Death"))
            .add_systems(PerceiveSet::Vision, tracer("Perceive/Vision"))
            .add_systems(PerceiveSet::Spatial, tracer("Perceive/Spatial"))
            .add_systems(EconomySet::Main, tracer("Economy/Main"))
            .add_systems(ResolveSet::Stats, tracer("Resolve/Stats"))
            .add_systems(ResolveSet::Effects, tracer("Resolve/Effects"))
            .add_systems(ResolveSet::Projectiles, tracer("Resolve/Projectiles"))
            .add_systems(ActSet::Fire, tracer("Act/Fire"))
            .add_systems(ActSet::Move, tracer("Act/Move"))
            .add_systems(ThinkSet::Behaviour, tracer("Think/Behaviour"))
            .add_systems(ThinkSet::Orders, tracer("Think/Orders"))
            .add_systems(InputSet::Apply, tracer("Input/Apply"))
            .add_systems(InputSet::Validate, tracer("Input/Validate"))
            .add_systems(InputSet::Housekeeping, tracer("Input/Housekeeping"));
    }
}

#[test]
fn hooks_run_in_declared_order() {
    let mut app = build(vec![Arc::new(TraceFeature)]);
    step(&mut app, 1);
    assert_eq!(
        app.world().resource::<Trace>().0,
        [
            "Input/Housekeeping",
            "Input/Validate",
            "Input/Apply",
            "Think/Orders",
            "Think/Behaviour",
            "Act/Move",
            "Act/Fire",
            "Resolve/Projectiles",
            "Resolve/Effects",
            "Resolve/Stats",
            "Economy/Main",
            "Perceive/Spatial",
            "Perceive/Vision",
            "Lifecycle/Death",
            "Lifecycle/Spawn",
            "Lifecycle/Despawn",
            "Publish/Snapshot",
            "Publish/Events",
        ]
    );
}

#[test]
fn clock_advances_per_step() {
    let mut app = build(Vec::new());
    assert_eq!(app.world().resource::<SimClock>().tick, Tick(0));
    step(&mut app, 3);
    assert_eq!(app.world().resource::<SimClock>().tick, Tick(3));
}

#[test]
#[should_panic(expected = "driver out of sync")]
fn input_tick_must_match_clock() {
    let mut app = build(Vec::new());
    let input = TickInput {
        tick: Tick(1),
        commands: Vec::new(),
    };
    step_tick(app.world_mut(), input);
}

#[test]
fn step_output_carries_the_tick() {
    let mut app = build(Vec::new());
    step(&mut app, 2);
    let input = TickInput {
        tick: Tick(2),
        commands: Vec::new(),
    };
    let output = step_tick(app.world_mut(), input);
    assert_eq!(output.tick, Tick(2));
    assert_eq!(output.snapshot.tick, Tick(2));
}

/// Set up the flat map and player 0, so `setup_spawn` can spawn.
fn map_and_player(world: &mut World) {
    setup_map(world, "core_test.flat").expect("the map exists");
    world.resource_mut::<Players>().list.push(PlayerInfo {
        id: PlayerId(0),
        faction: FactionId(0),
        team: TeamId(0),
        controller: Controller::None,
    });
}

/// A bare entity for player 0, spawned through `setup_spawn` (only fl_sim creates a
/// `SimEntity`); the world needs a map and the player ([`map_and_player`]).
fn spawn_bare(world: &mut World) -> Entity {
    let request = SpawnRequest {
        unit: None,
        owner: PlayerId(0),
        pos: FxVec2::ZERO,
        heading: Angle::ZERO,
        init: None,
    };
    let id = setup_spawn(world, &request).expect("spawns");
    world.resource::<SimIndex>().get(id).expect("indexed")
}

#[test]
fn sim_index_tracks_spawn_and_despawn() {
    let mut app = build(Vec::new());
    let world = app.world_mut();
    map_and_player(world);
    let entities: Vec<Entity> = (1..=3).map(|_| spawn_bare(world)).collect();
    world.despawn(entities[1]);

    let index = world.resource::<SimIndex>();
    assert_eq!(index.ids().collect::<Vec<_>>(), [SimId(1), SimId(3)]);
    assert_eq!(index.len(), 2);
    assert_eq!(index.get(SimId(1)), Some(entities[0]));
    assert_eq!(index.get(SimId(2)), None);
    assert_eq!(index.get(SimId(3)), Some(entities[2]));
}

#[derive(Resource, Default)]
struct Counter(u32);

/// Two unordered systems in one hook writing the same resource.
struct AmbiguousFeature;

impl SimFeature for AmbiguousFeature {
    fn id(&self) -> &'static str {
        "ambiguous"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().init_resource::<Counter>();
        sim.add_systems(
            ThinkSet::Orders,
            (
                |mut c: ResMut<Counter>| c.0 += 1,
                |mut c: ResMut<Counter>| c.0 *= 2,
            ),
        );
    }
}

#[test]
#[should_panic(expected = "SimTick schedule failed to build")]
fn ambiguous_systems_fail_schedule_build() {
    let _ = build(vec![Arc::new(AmbiguousFeature)]);
}

#[derive(Component)]
struct Marker;

/// How many `Marker`s `Think/Behaviour` and `Act/Move` saw.
#[derive(Resource, Default)]
struct Seen(Vec<usize>);

/// Spawns a `Marker` through `Commands` in `Think/Orders`, then counts markers in
/// `Think/Behaviour` (same phase) and `Act/Move` (after the Think sync point).
struct DeferredFeature;

impl SimFeature for DeferredFeature {
    fn id(&self) -> &'static str {
        "deferred"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().init_resource::<Seen>();
        sim.add_systems(ThinkSet::Orders, |mut commands: Commands| {
            commands.spawn(Marker);
        })
        .add_systems(
            ThinkSet::Behaviour,
            |markers: Query<&Marker>, mut seen: ResMut<Seen>| seen.0.push(markers.iter().count()),
        )
        .add_systems(
            ActSet::Move,
            |markers: Query<&Marker>, mut seen: ResMut<Seen>| seen.0.push(markers.iter().count()),
        );
    }
}

#[test]
fn commands_visible_only_after_phase_sync() {
    let mut app = build(vec![Arc::new(DeferredFeature)]);
    step(&mut app, 1);
    assert_eq!(app.world().resource::<Seen>().0, [0, 1]);
}

#[derive(Message, Debug, Clone, PartialEq)]
struct Ping(u32);

#[derive(Resource, Default)]
struct Reads(Vec<u32>);

/// Writes one `Ping` in tick 0 (`Think/Orders`) and reads every tick in `Lifecycle/Death`.
struct MessageFeature;

impl SimFeature for MessageFeature {
    fn id(&self) -> &'static str {
        "message"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().init_resource::<Reads>();
        sim.add_message::<Ping>("message.msg.ping")
            .add_message::<Ping>("message.msg.ping_again")
            .add_systems(
                ThinkSet::Orders,
                |clock: Res<SimClock>, mut pings: MessageWriter<Ping>| {
                    if clock.tick == Tick(0) {
                        pings.write(Ping(7));
                    }
                },
            )
            .add_systems(
                LifecycleSet::Death,
                |mut pings: MessageReader<Ping>, mut reads: ResMut<Reads>| {
                    reads.0.extend(pings.read().map(|ping| ping.0));
                },
            );
    }
}

#[test]
fn messages_are_cleared_at_the_end_of_each_tick() {
    let mut app = build(vec![Arc::new(MessageFeature)]);
    step(&mut app, 1);
    assert_eq!(
        app.world().resource::<Reads>().0,
        [7],
        "read in the tick it was written"
    );
    assert_eq!(
        app.world().resource::<Messages<Ping>>().len(),
        0,
        "cleared by step_tick (R72)"
    );
    step(&mut app, 2);
    assert_eq!(app.world().resource::<Reads>().0, [7], "read exactly once");
}

#[derive(Resource, Default)]
struct LateReads(Vec<u32>);

/// Writes a `Ping` in `Publish/Events` of tick 0, after its reader in `Think/Orders`.
struct LatePingFeature;

impl SimFeature for LatePingFeature {
    fn id(&self) -> &'static str {
        "late_ping"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().init_resource::<LateReads>();
        sim.add_message::<Ping>("late_ping.msg.ping")
            .add_systems(
                PublishSet::Events,
                |clock: Res<SimClock>, mut pings: MessageWriter<Ping>| {
                    if clock.tick == Tick(0) {
                        pings.write(Ping(9));
                    }
                },
            )
            .add_systems(
                ThinkSet::Orders,
                |mut pings: MessageReader<Ping>, mut reads: ResMut<LateReads>| {
                    reads.0.extend(pings.read().map(|ping| ping.0));
                },
            );
    }
}

#[test]
fn a_message_never_reaches_the_next_tick() {
    // A message is read in the tick it was written: one written after its reader ran is
    // dropped at the end of the tick, so a save between ticks loses nothing.
    let mut app = build(vec![Arc::new(LatePingFeature)]);
    step(&mut app, 3);
    assert!(app.world().resource::<LateReads>().0.is_empty());
}

#[derive(Component)]
struct Steer;

#[derive(Resource, Default)]
struct SteerSeen(Vec<bool>);

/// Classifies `Steer` Transient, inserts it on #1 in `Think/Orders` of tick 0 only, and
/// records in `Act/Move` of every tick whether #1 has it.
struct SteerFeature;

impl SimFeature for SteerFeature {
    fn id(&self) -> &'static str {
        "steer"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().init_resource::<SteerSeen>();
        sim.register_component::<Steer>("steer.steer", fl_sim::Class::Transient)
            .add_systems(
                ThinkSet::Orders,
                |clock: Res<SimClock>, index: Res<SimIndex>, mut commands: Commands| {
                    if clock.tick == Tick(0)
                        && let Some(entity) = index.get(SimId(1))
                    {
                        commands.entity(entity).insert(Steer);
                    }
                },
            )
            .add_systems(
                ActSet::Move,
                |index: Res<SimIndex>,
                 steer: Query<(), With<Steer>>,
                 mut seen: ResMut<SteerSeen>| {
                    let entity = index.get(SimId(1)).expect("#1");
                    seen.0.push(steer.contains(entity));
                },
            );
    }
}

#[test]
fn transient_components_are_removed_every_tick() {
    let mut app = build(vec![Arc::new(SteerFeature)]);
    let world = app.world_mut();
    map_and_player(world);
    spawn_bare(world);
    step(&mut app, 3);
    assert_eq!(
        app.world().resource::<SteerSeen>().0,
        [true, false, false],
        "Housekeeping removes it the next tick"
    );
}

#[test]
fn message_buffers_are_named_without_rust_paths() {
    let app = build(vec![Arc::new(MessageFeature)]);
    let header = &app.world().resource::<MatchHeaderRes>().0;
    let resources = header
        .string_tables
        .get(tables::SIM_RESOURCES)
        .expect("sim_resources table");
    assert!(
        resources.iter().any(|n| n == "message.msg.ping"),
        "{resources:?}"
    );
    assert!(
        !resources.iter().any(|n| n == "message.msg.ping_again"),
        "a second add_message of the same type does nothing: {resources:?}"
    );
    assert!(
        resources
            .iter()
            .all(|n| !n.contains("::") && !n.contains('<')),
        "no Rust paths: {resources:?}"
    );
}

/// What the feature systems saw after the core systems of their hooks ran.
#[derive(Resource, Default)]
struct AfterCore {
    rejected_in_apply: Vec<usize>,
}

/// A system in `Input/Apply` (after the exclusive core command apply) and one in
/// `Input/Housekeeping` (after the core counter clear).
struct AfterCoreFeature;

impl SimFeature for AfterCoreFeature {
    fn id(&self) -> &'static str {
        "after_core"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().init_resource::<AfterCore>();
        sim.add_systems(
            InputSet::Apply,
            |log: Res<CommandLog>, mut seen: ResMut<AfterCore>| {
                seen.rejected_in_apply.push(log.rejected.len());
            },
        )
        .add_systems(
            InputSet::Housekeeping,
            |mut counters: ResMut<SimCounters>| counters.add("after_core.housekeeping", 1),
        );
    }
}

#[test]
fn feature_systems_run_after_the_core_systems_of_their_hook() {
    let mut app = build(vec![Arc::new(AfterCoreFeature)]);
    // No players are set up, so the command is rejected by the core apply system.
    let kind = app.world().resource::<CommandRegistry>().encode(&MoveCmd {
        target: fl_math::FxVec2::ZERO,
    });
    let cmd = PlayerCommand {
        exec_tick: Tick(0),
        player: PlayerId(5),
        seq: 0,
        units: Vec::new(),
        kind,
        queue: false,
    };
    let input = TickInput {
        tick: Tick(0),
        commands: vec![cmd],
    };
    step_tick(app.world_mut(), input);
    assert_eq!(
        app.world().resource::<CommandLog>().rejected[0].1,
        RejectReason::UnknownPlayer
    );
    step(&mut app, 1);
    let world = app.world();
    assert_eq!(
        world.resource::<AfterCore>().rejected_in_apply,
        [1, 0],
        "applied (and rejected) before the feature system ran; the log holds one tick"
    );
    assert_eq!(
        world
            .resource::<SimCounters>()
            .get("after_core.housekeeping"),
        1,
        "counted after the core clear"
    );
    let order = system_order(&mut app);
    let apply: Vec<&str> = order
        .lines()
        .filter(|l| l.starts_with("Input/Apply "))
        .collect();
    assert_eq!(apply.len(), 2, "{order}");
    assert!(apply[0].contains("apply_commands"), "core first: {order}");
}

/// Despawns `SimId(1)` in tick 1, through `Commands` in `Lifecycle/Despawn`.
struct DespawnFeature;

impl SimFeature for DespawnFeature {
    fn id(&self) -> &'static str {
        "despawn"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(
            LifecycleSet::Despawn,
            |clock: Res<SimClock>, index: Res<SimIndex>, mut commands: Commands| {
                if clock.tick == Tick(1)
                    && let Some(entity) = index.get(SimId(1))
                {
                    commands.entity(entity).despawn();
                }
            },
        );
    }
}

fn removed_sim_entities(app: &App) -> usize {
    let world = app.world();
    let id = world
        .component_id::<SimEntity>()
        .expect("SimEntity is registered");
    world.removed_components().get(id).map_or(0, |m| m.len())
}

#[test]
fn clear_trackers_runs_each_step() {
    let mut app = build(vec![Arc::new(DespawnFeature)]);
    let world = app.world_mut();
    map_and_player(world);
    spawn_bare(world);
    step(&mut app, 1);
    assert_eq!(removed_sim_entities(&app), 0);
    step(&mut app, 1);
    assert!(
        app.world().resource::<SimIndex>().is_empty(),
        "despawned in tick 1"
    );
    assert_eq!(removed_sim_entities(&app), 1, "kept for one more step");
    step(&mut app, 1);
    assert_eq!(removed_sim_entities(&app), 0, "dropped by clear_trackers");
}

/// Counts 5 of `"test.n"` every tick.
struct CountFeature;

impl SimFeature for CountFeature {
    fn id(&self) -> &'static str {
        "count"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(ResolveSet::Stats, |mut counters: ResMut<SimCounters>| {
            counters.add("test.n", 5);
        });
    }
}

#[test]
fn counters_reset_each_tick() {
    let mut app = build(vec![Arc::new(CountFeature)]);
    step(&mut app, 3);
    let counters = app.world().resource::<SimCounters>();
    assert_eq!(counters.get("test.n"), 5);
    assert_eq!(counters.snapshot(), [("test.n", 5)]);
}

#[test]
fn system_order_lists_core_systems() {
    let mut app = build(Vec::new());
    let order = system_order(&mut app);
    let lines: Vec<&str> = order.lines().collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("Input/Apply ") && l.contains("apply_commands")),
        "{order}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("Act/Move ") && l.contains("move_system")),
        "{order}"
    );
    assert_eq!(
        lines.iter().filter(|l| l.contains("/Sync ")).count(),
        8,
        "one sync point per phase: {order}"
    );
    assert!(!order.contains("? "), "every system is in a hook: {order}");
    assert_eq!(order, system_order(&mut app), "stable across calls");
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestDef {
    value: u32,
}

impl DefComponent for TestDef {
    fn instantiate(&self, _ctx: &SpawnCtx<'_>, _entity: &mut EntityWorldMut<'_>) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct DefFeature;

impl SimFeature for DefFeature {
    fn id(&self) -> &'static str {
        "def"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.register_def_component::<TestDef>("Test");
    }
}

#[test]
fn feature_registers_def_component() {
    let registry = sim_registry(&[Arc::new(DefFeature)]).expect("features sort");
    assert_eq!(registry.component_name::<TestDef>(), Some("Test"));
    let core_only = sim_registry(&[]).expect("features sort");
    assert_eq!(core_only.component_name::<TestDef>(), None);
}

#[test]
#[should_panic(expected = "DefRegistry is frozen")]
fn registering_after_content_load_panics() {
    let mut app = build(Vec::new());
    app.world_mut()
        .resource_mut::<fl_content::DefRegistry>()
        .register_component::<TestDef>("Test");
}

fn ping(tick: Tick) -> SimEvent {
    SimEvent::Custom {
        channel: "ping".to_owned(),
        data: tick.0.to_le_bytes().to_vec(),
    }
}

/// Pushes one ping event per tick, in `Act/Fire`.
struct PingFeature;

impl SimFeature for PingFeature {
    fn id(&self) -> &'static str {
        "ping"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(
            ActSet::Fire,
            |clock: Res<SimClock>, mut events: ResMut<SimEvents<SimEvent>>| {
                events.push(ping(clock.tick));
            },
        );
    }
}

#[test]
fn sim_events_cleared_each_tick() {
    let mut app = build(vec![Arc::new(PingFeature)]);
    step(&mut app, 1);
    let events: Vec<&SimEvent> = app
        .world()
        .resource::<SimEvents<SimEvent>>()
        .iter()
        .collect();
    assert_eq!(events, [&ping(Tick(0))]);

    step(&mut app, 1);
    let events: Vec<&SimEvent> = app
        .world()
        .resource::<SimEvents<SimEvent>>()
        .iter()
        .collect();
    assert_eq!(events, [&ping(Tick(1))]);

    let drained: Vec<SimEvent> = app
        .world_mut()
        .resource_mut::<SimEvents<SimEvent>>()
        .drain()
        .collect();
    assert_eq!(drained, [ping(Tick(1))]);
    assert!(app.world().resource::<SimEvents<SimEvent>>().is_empty());
}

/// Two unordered systems in one hook, both counting into `SimCounters`.
struct TwoCountersFeature;

impl SimFeature for TwoCountersFeature {
    fn id(&self) -> &'static str {
        "two_counters"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(
            ThinkSet::Behaviour,
            (
                |mut counters: ResMut<SimCounters>| counters.add("two_counters.n", 2),
                |mut counters: ResMut<SimCounters>| counters.add("two_counters.n", 3),
            ),
        );
    }
}

#[test]
fn counting_systems_need_no_order() {
    // Counter adds commute (spec §15: every mechanic counts its work), so SimCounters is
    // exempt from ambiguity detection.
    let mut app = build(vec![Arc::new(TwoCountersFeature)]);
    step(&mut app, 1);
    assert_eq!(
        app.world().resource::<SimCounters>().get("two_counters.n"),
        5
    );
}

/// Writes a `SpawnRequest` in `Publish/Events` of tick 0: after the spawn funnel ran.
struct LateSpawnFeature;

impl SimFeature for LateSpawnFeature {
    fn id(&self) -> &'static str {
        "late_spawn"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(
            PublishSet::Events,
            |clock: Res<SimClock>, mut spawns: MessageWriter<SpawnRequest>| {
                if clock.tick == Tick(0) {
                    spawns.write(SpawnRequest {
                        unit: None,
                        owner: PlayerId(0),
                        pos: FxVec2::ZERO,
                        heading: Angle::ZERO,
                        init: None,
                    });
                }
            },
        );
    }
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "were written after the Lifecycle funnels ran")]
fn spawn_request_after_the_funnel_fails_its_tick() {
    let mut app = build(vec![Arc::new(LateSpawnFeature)]);
    step(&mut app, 1);
}
