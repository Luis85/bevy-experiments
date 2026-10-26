//! The spawn and despawn funnels: `SpawnRequest`s in `Lifecycle/Spawn` in write order,
//! `DespawnRequest`s in `Lifecycle/Despawn` in `SimId` order, and the setup path that shares
//! the spawn code.

use std::path::PathBuf;
use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_ecs::world::EntityWorldMut;
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{DebugSpawnCmd, PlayerCommand, PlayerId, SimId, Tick};
use fl_sim::{
    CommandRegistry, Content, DespawnRequest, Health, MatchSetup, Owner, Pose, RunConfig, SimClock,
    SimCounters, SimEvents, SimFeature, SimIndex, SpawnRequest, UnitType, build_sim_app,
    inspect_entity, load_content, setup_scenario, setup_spawn, step_tick,
};
use fl_view::{SimEvent, TickInput};

fn core_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod")
}

/// The duel (#1 for player 0, #2 for player 1), cheats on.
fn duel() -> App {
    let features: Vec<Arc<dyn SimFeature>> = Vec::new();
    let content = load_content(&features, &[core_mod()]).expect("core_mod loads");
    let setup = MatchSetup {
        seed: 1,
        cheats_allowed: true,
    };
    let mut app =
        build_sim_app(&features, content, setup, RunConfig::default()).expect("the sim builds");
    setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
    app
}

fn tick(app: &mut App, commands: Vec<PlayerCommand>) {
    let tick = app.world().resource::<SimClock>().tick;
    step_tick(app.world_mut(), TickInput { tick, commands });
}

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

fn entity_of(app: &App, id: u32) -> Entity {
    app.world()
        .resource::<SimIndex>()
        .get(SimId(id))
        .expect("entity exists")
}

fn tank_request(app: &App, owner: u8, pos: FxVec2) -> SpawnRequest {
    let tank = app
        .world()
        .resource::<Content>()
        .0
        .unit_id("core_test.tank");
    SpawnRequest {
        unit: tank,
        owner: PlayerId(owner),
        pos,
        heading: Angle::QUARTER,
        init: None,
    }
}

fn counter(app: &App, name: &str) -> u64 {
    app.world().resource::<SimCounters>().get(name)
}

fn events(app: &App) -> Vec<SimEvent> {
    app.world()
        .resource::<SimEvents<SimEvent>>()
        .iter()
        .cloned()
        .collect()
}

#[test]
fn spawn_requests_processed_in_write_order() {
    let mut app = duel();
    let spawn = |player: u8| {
        let payload = DebugSpawnCmd {
            unit: "core_test.tank".to_owned(),
            owner: PlayerId(player),
            pos: v(500, 500),
            heading: Angle::ZERO,
        };
        PlayerCommand {
            exec_tick: Tick(0),
            player: PlayerId(player),
            seq: 0,
            units: Vec::new(),
            kind: app.world().resource::<CommandRegistry>().encode(&payload),
            queue: false,
        }
    };
    let commands = vec![spawn(1), spawn(0)];
    tick(&mut app, commands);
    let owner = |id| {
        app.world()
            .get::<Owner>(entity_of(&app, id))
            .expect("owner")
            .0
    };
    assert_eq!(owner(3), PlayerId(0), "P0's command applied first");
    assert_eq!(owner(4), PlayerId(1));
    assert_eq!(counter(&app, "spawn.entities"), 2);
    let spawned: Vec<SimId> = events(&app)
        .iter()
        .filter_map(|e| match e {
            SimEvent::Spawned { sim_id, .. } => Some(*sim_id),
            _ => None,
        })
        .collect();
    assert_eq!(spawned, [SimId(3), SimId(4)]);
}

#[test]
fn spawn_outside_map_rejected_not_panicking() {
    let mut app = duel();
    let request = tank_request(&app, 0, v(5000, 5000));
    app.world_mut().write_message(request);
    tick(&mut app, Vec::new());
    assert_eq!(app.world().resource::<SimIndex>().len(), 2, "no entity");
    assert_eq!(counter(&app, "spawn.rejected"), 1);
    assert_eq!(counter(&app, "spawn.entities"), 0);
    assert!(
        !events(&app)
            .iter()
            .any(|e| matches!(e, SimEvent::Spawned { .. }))
    );
}

#[test]
fn despawns_dedupe_and_sort() {
    let mut app = duel();
    for x in [300, 400, 500] {
        let request = tank_request(&app, 0, v(x, 500));
        setup_spawn(app.world_mut(), &request).expect("spawns");
    }
    for id in [5, 3, 5] {
        app.world_mut().write_message(DespawnRequest(SimId(id)));
    }
    tick(&mut app, Vec::new());
    let despawned: Vec<SimId> = events(&app)
        .iter()
        .filter_map(|e| match e {
            SimEvent::Despawned { sim_id } => Some(*sim_id),
            _ => None,
        })
        .collect();
    assert_eq!(despawned, [SimId(3), SimId(5)]);
    assert_eq!(counter(&app, "despawn.entities"), 2);
    let ids: Vec<SimId> = app.world().resource::<SimIndex>().ids().collect();
    assert_eq!(ids, [SimId(1), SimId(2), SimId(4)]);
}

#[test]
fn setup_and_funnel_share_code() {
    let mut app = duel();
    let request = tank_request(&app, 1, v(500, 500));
    let direct = setup_spawn(app.world_mut(), &request).expect("spawns");
    assert_eq!(direct, SimId(3));
    app.world_mut().write_message(request);
    tick(&mut app, Vec::new());
    let strip = |id: SimId| {
        let mut json = inspect_entity(app.world(), id).expect("entity exists");
        json.as_object_mut().expect("an object").remove("sim_id");
        json
    };
    let via_setup = strip(SimId(3));
    assert_eq!(via_setup, strip(SimId(4)));
    let components = via_setup["components"]
        .as_object()
        .expect("components")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        components.contains(&"core.health".to_owned()),
        "{components:?}"
    );
}

/// Copies `Health::max` into a tag: it sees the defs' components.
#[derive(Component, Debug, PartialEq, Eq)]
struct TestTag(u32);

fn tag_with_max_hp(entity: &mut EntityWorldMut<'_>, _request: &SpawnRequest) {
    let max = entity.get::<Health>().map_or(0, |h| h.max);
    entity.insert(TestTag(max));
}

#[test]
fn spawn_init_runs_after_defs() {
    let mut app = duel();
    let request = SpawnRequest {
        init: Some(tag_with_max_hp),
        ..tank_request(&app, 0, v(500, 500))
    };
    app.world_mut().write_message(request);
    tick(&mut app, Vec::new());
    let tag = app.world().get::<TestTag>(entity_of(&app, 3));
    assert_eq!(tag, Some(&TestTag(100)));
}

#[test]
fn non_unit_spawn_has_owner_and_pose_only() {
    let mut app = duel();
    let request = SpawnRequest {
        unit: None,
        init: Some(tag_with_max_hp),
        ..tank_request(&app, 1, v(500, 500))
    };
    app.world_mut().write_message(request);
    tick(&mut app, Vec::new());
    let entity = entity_of(&app, 3);
    let world = app.world();
    assert_eq!(world.get::<Owner>(entity).expect("owner").0, PlayerId(1));
    assert_eq!(
        *world.get::<Pose>(entity).expect("pose"),
        Pose {
            pos: v(500, 500),
            heading: Angle::QUARTER
        }
    );
    assert!(world.get::<UnitType>(entity).is_none());
    assert!(world.get::<Health>(entity).is_none());
    assert_eq!(world.get::<TestTag>(entity), Some(&TestTag(0)));
    assert!(events(&app).contains(&SimEvent::Spawned {
        sim_id: SimId(3),
        unit: None,
        owner: PlayerId(1),
    }));
}
