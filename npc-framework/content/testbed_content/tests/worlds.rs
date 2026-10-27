use npc_framework::agent_io::{
    api,
    control::{ControlMode, IdlePolicy, attach_now},
};
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::perception::{Sensor, Sensors};
use npc_framework::prelude::*;
use serde_json::json;
use testbed_content::{actors, worlds};

fn build(id: &str) -> bevy_app::App {
    let world = worlds::builtin_preset(id).unwrap();
    let doc = worlds::world_doc(&world, &actors::human_preset()).unwrap();
    let mut app = testbed_content::scenario::load_with(&doc, 1, |_| {})
        .unwrap()
        .app;
    app.finish();
    app.cleanup();
    app
}

#[test]
fn shipped_worlds_validate() {
    for (id, _) in worlds::BUILTIN_PRESETS {
        worlds::validate(&worlds::builtin_preset(id).unwrap()).unwrap();
    }
}

#[test]
fn invalid_worlds_are_reported() {
    let mut preset = worlds::plain_preset();
    preset.needs.hunger.rise_milli_per_tick = i32::MAX;
    preset.needs.energy.low_milli = 100_001;
    preset.day_night.cycle_ticks = 0;
    preset.inhabitants.workers = 9;
    preset.perception.memory_capacity = 0;
    assert!(worlds::validate(&preset).unwrap_err().len() >= 5);
}

#[test]
fn hunger_rises_and_slows() {
    let mut app = build("survival");
    let (id, entity) = actor(&app);
    attach_now(
        app.world_mut(),
        entity,
        "test",
        ControlMode::Direct,
        IdlePolicy::Idle,
    );
    let accepted = api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({"unit":id.0,"controller":"test","steps":[
            {"verb":"order","args":{"order":{"type":"Move","pos":[5000,-6500]}}},
            {"verb":"execute_orders"}
        ]})),
    )
    .unwrap();
    assert_eq!(accepted["accepted"], true, "{accepted}");
    run_ticks(app.world_mut(), 10);
    let before = *app.world().get::<SimPos>(entity).unwrap();
    run_ticks(app.world_mut(), 1);
    let normal = *app.world().get::<SimPos>(entity).unwrap();
    let normal_delta = normal.x - before.x;
    assert!(normal_delta > 0);
    set_badness(&mut app, "Hunger", 59_980);
    run_ticks(app.world_mut(), 1);
    let slowed = *app.world().get::<SimPos>(entity).unwrap();
    assert_eq!(slowed.x - normal.x, normal_delta / 2);
    let hunger = app.world().resource::<NeedRegistry>().id("Hunger").unwrap();
    assert_eq!(
        app.world().get::<Needs>(entity).unwrap().get(hunger).value,
        39_980
    );
    set_badness(&mut app, "Hunger", 100_000);
    run_ticks(app.world_mut(), 3);
    assert_eq!(*app.world().get::<SimPos>(entity).unwrap(), slowed);
}

#[test]
fn inhabitants_spawn_and_act() {
    let mut preset = worlds::plain_preset();
    preset.inhabitants.workers = 2;
    preset.inhabitants.skeletons = 2;
    let doc = worlds::world_doc(&preset, &actors::human_preset()).unwrap();
    let mut app = testbed_content::scenario::load_with(&doc, 1, |_| {})
        .unwrap()
        .app;
    assert_eq!(
        app.world_mut()
            .query::<&testbed_content::TestbedUnit>()
            .iter(app.world())
            .count(),
        5
    );
    set_journal(app.world_mut(), true, false);
    let ids: Vec<_> = ["worker_0", "worker_1", "skeleton_0", "skeleton_1"]
        .into_iter()
        .map(|name| {
            app.world()
                .resource::<testbed_content::TestbedIds>()
                .get(name)
                .unwrap()
        })
        .collect();
    let before: Vec<_> = ids
        .iter()
        .map(|id| {
            *app.world()
                .get::<SimPos>(app.world().resource::<SimIndex>().get(*id).unwrap())
                .unwrap()
        })
        .collect();
    run_ticks(app.world_mut(), 60);
    for (id, before) in ids.iter().zip(before) {
        assert_ne!(
            *app.world()
                .get::<SimPos>(app.world().resource::<SimIndex>().get(*id).unwrap())
                .unwrap(),
            before
        );
    }
    run_ticks(app.world_mut(), 540);
    assert!(app.world().resource::<Journal>().entries.iter().any(|entry|ids[..2].contains(&entry.unit.unwrap_or(SimId(0)))&&matches!(&entry.kind,JournalKind::Interacted{verb,outcome,..} if verb=="use_tool_on"&&outcome=="success")));
}

fn actor(app: &bevy_app::App) -> (SimId, bevy_ecs::entity::Entity) {
    let id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .get("explorer")
        .unwrap();
    (id, app.world().resource::<SimIndex>().get(id).unwrap())
}
fn set_badness(app: &mut bevy_app::App, name: &str, value: i32) {
    let (_, e) = actor(app);
    let id = app.world().resource::<NeedRegistry>().id(name).unwrap();
    app.world_mut()
        .get_mut::<Needs>(e)
        .unwrap()
        .get_mut(id)
        .value = 100_000 - value;
}
fn recovery(shortcut: bool, robot: bool) {
    let preset = worlds::builtin_preset("survival").unwrap();
    let body = if robot {
        actors::builtin_preset("rover").unwrap()
    } else {
        actors::human_preset()
    };
    let doc = worlds::world_doc(&preset, &body).unwrap();
    let mut app = testbed_content::scenario::load_with(&doc, 1, |_| {})
        .unwrap()
        .app;
    app.finish();
    app.cleanup();
    let (id, e) = actor(&app);
    let source = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .get(if robot { "charger" } else { "bed" })
        .unwrap();
    let target = app.world().resource::<SimIndex>().get(source).unwrap();
    let pos = *app.world().get::<SimPos>(target).unwrap();
    app.world_mut().entity_mut(e).insert(pos);
    attach_now(
        app.world_mut(),
        e,
        "test",
        ControlMode::Direct,
        IdlePolicy::Idle,
    );
    set_journal(app.world_mut(), true, false);
    set_badness(&mut app, if robot { "Battery" } else { "Energy" }, 100_000);
    run_ticks(app.world_mut(), 1);
    assert!(
        app.world()
            .get::<testbed_content::world_runtime::WorldActor>(e)
            .unwrap()
            .collapsed
    );
    let blocked = api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({"unit":id.0,"controller":"test","verb":"walk_to","pos":[2000,0]})),
    )
    .unwrap();
    assert_eq!(blocked["accepted"], false);
    assert_eq!(blocked["reason"], "not_allowed");
    assert_eq!(
        blocked["message"],
        if robot {
            "battery empty"
        } else {
            "too exhausted"
        }
    );
    let verb = if robot { "charge" } else { "rest" };
    let request = if shortcut {
        json!({"unit":id.0,"controller":"test","verb":"interact","args":{"affordance":format!("{verb}@{}",source.0)}})
    } else {
        json!({"unit":id.0,"controller":"test","verb":verb,"target":source.0})
    };
    let accepted = api::call(app.world_mut(), "agent/act", Some(request)).unwrap();
    assert_eq!(accepted["accepted"], true, "{accepted}");
    run_ticks(app.world_mut(), 55);
    assert!(
        !app.world()
            .get::<testbed_content::world_runtime::WorldActor>(e)
            .unwrap()
            .collapsed
    );
    assert_eq!(
        app.world().get::<Commandable>(e).unwrap().speed_mm_per_tick,
        body.speed_mm_per_tick
    );
    let transitions: Vec<_> = app
        .world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter_map(|entry| {
            let value = serde_json::to_value(&entry.kind).unwrap();
            (value["type"] == "Effect" && value["effect"]["type"] == "ExplorerNeedState")
                .then_some(value["effect"]["collapsed"].clone())
        })
        .collect();
    assert_eq!(transitions, vec![json!(true), json!(false)]);
}
#[test]
fn collapse_until_rested() {
    recovery(false, false);
    recovery(true, false);
}
#[test]
fn robot_battery_and_charger() {
    recovery(false, true);
    recovery(true, true);
}

#[test]
fn night_shortens_vision_without_compounding_or_affecting_robot_range_sensors() {
    let preset = worlds::builtin_preset("night_shift").unwrap();
    let mut body = actors::builtin_preset("rover").unwrap();
    body.sensors.push(actors::SensorSpec::Vision {
        range_mm: 10000,
        half_fov_deg: 60,
    });
    body.sensors.push(actors::SensorSpec::Radar {
        range_mm: 10000,
        half_fov_deg: 60,
        range_res_mm: 100,
        bearing_res_mdeg: 1000,
    });
    let doc = worlds::world_doc(&preset, &body).unwrap();
    let mut app = testbed_content::scenario::load_with(&doc, 1, |_| {})
        .unwrap()
        .app;
    let (_, e) = actor(&app);
    let base = app.world().get::<Sensors>(e).unwrap().clone();
    app.world_mut().resource_mut::<SimClock>().tick = 900;
    run_ticks(app.world_mut(), 2);
    let night = app.world().get::<Sensors>(e).unwrap();
    for (before, after) in base.sensors.iter().zip(&night.sensors) {
        match (before, after) {
            (Sensor::Vision(a), Sensor::Vision(b)) => assert_eq!(b.range_mm, a.range_mm * 40 / 100),
            (Sensor::Camera(a), Sensor::Camera(b)) => assert_eq!(b.range_mm, a.range_mm * 40 / 100),
            _ => assert_eq!(before, after),
        }
    }
    app.world_mut().resource_mut::<SimClock>().tick = 1200;
    run_ticks(app.world_mut(), 1);
    assert_eq!(app.world().get::<Sensors>(e).unwrap(), &base);
}

#[test]
fn needs_section_in_observation() {
    let mut app = build("survival");
    let (id, _) = actor(&app);
    run_ticks(app.world_mut(), 10);
    let (observation, lines) =
        npc_framework::agent_io::observe::observe(app.world(), id, 0).unwrap();
    assert_eq!(observation["sections"]["needs"]["hunger"]["value"], 400);
    assert!(lines.iter().any(|line| line.contains("hunger")));
}

#[test]
fn survival_world_resume_equivalent() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(
        || build("survival"),
        150,
        400,
        20,
    );
}

#[test]
fn valid_threshold_extremes_and_zero_speed_do_not_panic() {
    let mut p = worlds::plain_preset();
    p.needs.energy.on = true;
    p.needs.energy.low_milli = 0;
    p.needs.energy.out_milli = 1;
    p.needs.energy.low_speed_pct = 0;
    worlds::validate(&p).unwrap();
    let doc = worlds::world_doc(&p, &actors::human_preset()).unwrap();
    let mut app = testbed_content::scenario::load_with(&doc, 1, |_| {})
        .unwrap()
        .app;
    run_ticks(app.world_mut(), 1);
    let (_, e) = actor(&app);
    assert!(
        app.world()
            .get::<testbed_content::world_runtime::WorldActor>(e)
            .unwrap()
            .collapsed
    );
    assert_eq!(
        app.world().get::<Commandable>(e).unwrap().speed_mm_per_tick,
        0
    );
}

#[test]
fn systems_off_remove_their_things_and_progression() {
    let mut p = worlds::plain_preset();
    p.systems.vehicles = false;
    p.systems.crafting = false;
    p.systems.locked_doors = false;
    p.systems.progression = false;
    let doc = worlds::world_doc(&p, &actors::human_preset()).unwrap();
    let mut app = testbed_content::scenario::load_with(&doc, 1, |_| {})
        .unwrap()
        .app;
    assert_eq!(
        app.world_mut()
            .query::<&Vehicle>()
            .iter(app.world())
            .count(),
        0
    );
    assert_eq!(
        app.world_mut()
            .query::<&Workstation>()
            .iter(app.world())
            .count(),
        0
    );
    let (_, e) = actor(&app);
    assert!(app.world().get::<Experience>(e).is_none());
    let store = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .get("store_door")
        .unwrap();
    let entity = app.world().resource::<SimIndex>().get(store).unwrap();
    assert!(app.world().get::<Door>(entity).unwrap().locked_by.is_none());
    assert!(app.world().get::<Door>(entity).unwrap().key.is_none());
    assert!(
        app.world()
            .resource::<testbed_content::TestbedIds>()
            .get("store_key")
            .is_none()
    );
}

#[test]
fn plain_explorer_is_todays_world() {
    let body = actors::human_preset();
    let plain = worlds::world_doc(&worlds::plain_preset(), &body).unwrap();
    let mut incumbent = plain.clone();
    incumbent
        .world
        .as_object_mut()
        .unwrap()
        .remove("explorer_context");
    testbed_content::explorer::context::ExplorerContext::new(
        "explorer",
        "human",
        &body,
        npc_framework::perception::PerceptionConfig {
            forget_after_ticks: 1200,
            memory_capacity: 128,
            ..Default::default()
        },
    )
    .write_to(&mut incumbent)
    .unwrap();
    let mut a = testbed_content::scenario::load_with(&plain, 1, |_| {})
        .unwrap()
        .app;
    let mut b = testbed_content::scenario::load_with(&incumbent, 1, |_| {})
        .unwrap()
        .app;
    assert_eq!(content_hash(a.world_mut()), content_hash(b.world_mut()));
    assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
    run_ticks(a.world_mut(), 600);
    run_ticks(b.world_mut(), 600);
    assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
}

#[test]
fn collapsed_wrappers_and_parked_orders_cannot_bypass_the_gate() {
    let mut app = build("survival");
    let (id, e) = actor(&app);
    attach_now(
        app.world_mut(),
        e,
        "test",
        ControlMode::Direct,
        IdlePolicy::Idle,
    );
    set_badness(&mut app, "Energy", 100_000);
    run_ticks(app.world_mut(), 1);
    for order in [
        json!({"type":"Move","pos":[5000,0]}),
        json!({"type":"AttackMove","pos":[5000,0]}),
        json!({"type":"Patrol","points":[[1000,0],[2000,0]]}),
        json!({"type":"Follow","target":id.0}),
        json!({"type":"Hold"}),
    ] {
        let result = api::call(
            app.world_mut(),
            "agent/act",
            Some(json!({"unit":id.0,"controller":"test","verb":"order","args":{"order":order}})),
        )
        .unwrap();
        assert_eq!(result["accepted"], false);
        assert_eq!(result["reason"], "not_allowed");
        assert_eq!(result["message"], "too exhausted");
    }
    // Simulate an existing developer-issued order, after the collapse tick.
    app.world_mut()
        .get_mut::<OrderQueue>(e)
        .unwrap()
        .queued
        .push(npc_framework::command::QueuedOrder {
            order: UnitOrder::Move { pos: (5000, 0) },
            group: None,
            player: PlayerId(1),
            issued: 1,
        });
    let result = api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({"unit":id.0,"controller":"test","verb":"execute_orders"})),
    )
    .unwrap();
    assert_eq!(result["accepted"], false);
    assert_eq!(result["reason"], "not_allowed");
    assert_eq!(result["message"], "too exhausted");
}

#[test]
fn plain_and_unlocked_order_primitives_remain_available() {
    for mut app in [
        build("plain_explorer"),
        testbed_content::scenario::new_sim_app(1, "explorer"),
    ] {
        let (id, e) = actor(&app);
        attach_now(
            app.world_mut(),
            e,
            "test",
            ControlMode::Direct,
            IdlePolicy::Idle,
        );
        for order in [
            json!({"type":"Move","pos":[5000,0]}),
            json!({"type":"Patrol","points":[[1000,0],[2000,0]]}),
            json!({"type":"Hold"}),
            json!({"type":"Stop"}),
        ] {
            let accepted = api::call(
                app.world_mut(),
                "agent/act",
                Some(
                    json!({"unit":id.0,"controller":"test","verb":"order","args":{"order":order}}),
                ),
            )
            .unwrap();
            assert_eq!(accepted["accepted"], true, "{accepted}");
        }
    }
}

#[test]
fn night_applies_to_inhabitants_too() {
    let mut p = worlds::builtin_preset("night_shift").unwrap();
    p.inhabitants.workers = 1;
    let doc = worlds::world_doc(&p, &actors::human_preset()).unwrap();
    let mut app = testbed_content::scenario::load_with(&doc, 1, |_| {})
        .unwrap()
        .app;
    let id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .get("worker_0")
        .unwrap();
    let e = app.world().resource::<SimIndex>().get(id).unwrap();
    let base = app.world().get::<Sensors>(e).unwrap().clone();
    app.world_mut().resource_mut::<SimClock>().tick = 900;
    run_ticks(app.world_mut(), 2);
    let after = app.world().get::<Sensors>(e).unwrap();
    for (a, b) in base.sensors.iter().zip(&after.sensors) {
        if let (Sensor::Vision(a), Sensor::Vision(b)) = (a, b) {
            assert_eq!(b.range_mm, a.range_mm * 40 / 100);
        }
    }
}

#[test]
fn zero_night_vision_is_valid_and_restores_at_dawn() {
    let mut p = worlds::builtin_preset("night_shift").unwrap();
    p.day_night.night_vision_pct = 0;
    let doc = worlds::world_doc(&p, &actors::human_preset()).unwrap();
    let mut app = testbed_content::scenario::load_with(&doc, 1, |_| {})
        .unwrap()
        .app;
    let (_, e) = actor(&app);
    let base = app.world().get::<Sensors>(e).unwrap().clone();
    app.world_mut().resource_mut::<SimClock>().tick = 900;
    run_ticks(app.world_mut(), 2);
    assert!(
        app.world()
            .get::<Sensors>(e)
            .unwrap()
            .sensors
            .iter()
            .any(|s| matches!(s,Sensor::Vision(v) if v.range_mm==0))
    );
    app.world_mut().resource_mut::<SimClock>().tick = 1200;
    run_ticks(app.world_mut(), 1);
    assert_eq!(app.world().get::<Sensors>(e).unwrap(), &base);
}

#[test]
fn fresh_world_with_inhabitants_records_and_replays_from_its_descriptor() {
    use npc_framework::agent_io::scenario::{ScenarioLoaders, SessionRecord, verify_session};
    use npc_framework::kernel::commands::{CommandLog, CommandQueue};
    use npc_framework::kernel::replay::Recorder;
    let mut preset = worlds::builtin_preset("survival").unwrap();
    preset.inhabitants.workers = 1;
    preset.inhabitants.skeletons = 2;
    let mut doc = worlds::world_doc(&preset, &actors::human_preset()).unwrap();
    // The production host normalizes away the old scenario controller.
    doc.controllers.clear();
    let mut app = testbed_content::scenario::load_with(&doc, 1, |_| {})
        .unwrap()
        .app;
    app.finish();
    app.cleanup();
    assert!(app.world().resource::<CommandQueue>().pending.is_empty());
    assert!(app.world().resource::<CommandLog>().entries.is_empty());
    for name in ["skeleton_0", "skeleton_1"] {
        let id = app
            .world()
            .resource::<testbed_content::TestbedIds>()
            .get(name)
            .unwrap();
        let entity = app.world().resource::<SimIndex>().get(id).unwrap();
        assert!(matches!(
            &app.world().get::<OrderQueue>(entity).unwrap().queued[0].order,
            UnitOrder::Patrol { .. }
        ));
    }
    let mut recorder = Recorder::live(app.world_mut(), 15);
    let (id, _) = actor(&app);
    let accepted = api::call(
        app.world_mut(),
        "agent/attach",
        Some(json!({"unit":id.0,"controller":"replay-test"})),
    )
    .unwrap();
    assert_eq!(accepted["unit"], id.0, "{accepted}");
    assert_eq!(accepted["controller"], "replay-test", "{accepted}");
    assert_eq!(accepted["mode"], "Direct", "{accepted}");
    for _ in 0..180 {
        run_ticks(app.world_mut(), 1);
        recorder.after_tick(app.world_mut());
    }
    let replay = recorder.finish(app.world_mut()).unwrap();
    assert_eq!(replay.commands.len(), 1);
    assert!(
        matches!(&replay.commands[0].2,SimCommand::Content{kind,..} if kind=="AttachController")
    );
    let mut loaders = ScenarioLoaders::default();
    testbed_content::scenario::register_scenario_loader(&mut loaders);
    let result = verify_session(
        &loaders,
        &SessionRecord {
            version: 1,
            scenario: doc,
            seed: 1,
            replay,
        },
    );
    assert!(result.ok, "{result:?}");
}
