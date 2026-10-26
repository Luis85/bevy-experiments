//! Wave E2: the capability glue (`npc_framework::glue`, ADR 0011).
//!
//! - perception × rules: the `per_roll` detection check (Per rolls,
//!   deterministic, journaled with purpose `perception.detect`);
//! - perception × agent-io: `agent/observe` shows only what the unit
//!   perceives (anti-goal test: an occluded entity is absent from `observe`
//!   but present in `world/state`);
//! - rules × agent-io: the `sheet` section, `spend_points` and the recorded
//!   `SpendPoints` command;
//! - the `NpcPlugins` builder conveniences.

#![cfg(all(feature = "perception", feature = "rules", feature = "agent-io"))]

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::{self, AgentIoPlugin};
use npc_framework::glue::{
    DETECT_PURPOSE, PER_ROLL, PerRollConfig, PerceptionAgentPlugin, PerceptionRulesPlugin,
    RulesAgentPlugin, Stealthy,
};
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::JournalKind;
use npc_framework::kernel::spatial::{Footprint, GridBounds, GridOccluder};
use npc_framework::perception::DetectionChecks;
use npc_framework::prelude::*;
use npc_framework::rules::SpendPoints;
use serde_json::{Value, json};

const SOUTH: Facing = Facing::new(0, 1);

/// `NpcPlugins` (no driver) built by `plugins`, a 64 × 64 m grid, scans
/// every tick, a big journal; `setup` runs before the freeze.
fn app_with(seed: u64, plugins: NpcPlugins, setup: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)))
        .insert_resource(PerceptionConfig {
            stagger: 1,
            ..Default::default()
        });
    app.add_plugins(plugins.seed(seed).without_driver());
    app.add_plugins(JournalPlugin {
        verbose: false,
        capacity: 200_000,
    });
    setup(&mut app);
    app.finish();
    app.cleanup();
    app
}

fn sheet_with_per(per_delta: i32) -> CharacterSheet {
    let mut sheet = CharacterSheet::default();
    sheet.secondary.per = per_delta;
    sheet
}

fn observer(app: &mut App, pos: SimPos, sheet: Option<CharacterSheet>) -> (SimId, Entity) {
    let (id, e) = spawn_sim(
        app.world_mut(),
        (pos, SOUTH, FixedFacing, Sensors::new().vision(20_000, 45)),
    );
    if let Some(sheet) = sheet {
        app.world_mut().entity_mut(e).insert(sheet);
    }
    (id, e)
}

fn thing(app: &mut App, pos: SimPos) -> (SimId, Entity) {
    spawn_sim(app.world_mut(), (pos, Perceivable::new(1)))
}

/// `(unit, success)` of every `Roll` with purpose `perception.detect`.
fn detect_rolls(app: &App) -> Vec<(u64, SimId, bool, i32, i32)> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            JournalKind::Roll {
                skill,
                purpose,
                success,
                roll,
                target,
                ..
            } if purpose == DETECT_PURPOSE => {
                assert_eq!(skill, "Per");
                Some((e.tick, e.unit.unwrap(), *success, *roll, *target))
            }
            _ => None,
        })
        .collect()
}

fn senses(app: &App, eye: Entity, target: SimId) -> bool {
    app.world()
        .get::<PerceptionMemory>(eye)
        .and_then(|m| m.get(target))
        .is_some_and(|p| p.sensed)
}

// ------------------------------------------------------------ perception × rules

/// Two observers 40 m apart, each 8 m north of its own target: Per 14
/// versus Per 7 (both −2 for the 5..10 m band). Re-rolled every scan.
fn rates_app(seed: u64) -> (App, [(SimId, Entity, SimId); 2]) {
    let mut out = Vec::new();
    let app = {
        let mut app = app_with(seed, NpcPlugins::new().with_per_roll_detection(), |app| {
            app.insert_resource(PerRollConfig {
                reroll_sensed: true,
                ..Default::default()
            });
        });
        for (x, per) in [(-20_000, 4), (20_000, -3)] {
            let (id, e) = observer(&mut app, SimPos::new(x, 0), Some(sheet_with_per(per)));
            let (t, _) = thing(&mut app, SimPos::new(x, 8_000));
            out.push((id, e, t));
        }
        app
    };
    (app, [out[0], out[1]])
}

#[test]
fn per_roll_rates_are_deterministic_and_ordered_by_per() {
    const TICKS: u64 = 400;
    let run = |seed| {
        let (mut app, units) = rates_app(seed);
        let mut sensed_ticks = [0u32; 2];
        for _ in 0..TICKS {
            run_ticks(app.world_mut(), 1);
            for (i, (_, eye, target)) in units.iter().enumerate() {
                sensed_ticks[i] += u32::from(senses(&app, *eye, *target));
            }
        }
        let rolls = detect_rolls(&app);
        (units, rolls, sensed_ticks, state_hash(app.world_mut()))
    };
    let (units, rolls, sensed, hash) = run(7);
    let successes = |unit: SimId| rolls.iter().filter(|r| r.1 == unit && r.2).count();
    let (high, low) = (units[0].0, units[1].0);
    // One roll per observer per scan (every tick), against Per − 2.
    assert_eq!(rolls.iter().filter(|r| r.1 == high).count(), TICKS as usize);
    assert_eq!(rolls.iter().filter(|r| r.1 == low).count(), TICKS as usize);
    assert!(rolls.iter().filter(|r| r.1 == high).all(|r| r.4 == 12));
    assert!(rolls.iter().filter(|r| r.1 == low).all(|r| r.4 == 5));
    let (h, l) = (successes(high), successes(low));
    // 3d6 ≤ 12 is 74 %, ≤ 5 is 4.6 %.
    assert!(h > 250 && h < 350, "high Per detected {h} / {TICKS}");
    assert!(l < 50, "low Per detected {l} / {TICKS}");
    assert!(h > l * 4, "high {h} vs low {l}");
    // A success is exactly a tick where the target is sensed.
    assert_eq!(sensed, [h as u32, l as u32]);

    // Same seed: the same dice, journal and hash; another seed differs.
    let (_, rolls2, sensed2, hash2) = run(7);
    assert_eq!(rolls, rolls2);
    assert_eq!(sensed, sensed2);
    assert_eq!(hash, hash2);
    let (_, rolls3, _, _) = run(8);
    assert_ne!(rolls, rolls3, "the world seed keys the dice");
}

#[test]
fn per_roll_modifiers_stealth_and_tracking() {
    let mut app = app_with(3, NpcPlugins::new().with_per_roll_detection(), |app| {
        app.insert_resource(PerRollConfig {
            reroll_sensed: true,
            ..Default::default()
        });
    });
    // Same observer stats; the second target is stealthy (−6).
    let (a, _) = observer(&mut app, SimPos::new(-20_000, 0), Some(sheet_with_per(4)));
    thing(&mut app, SimPos::new(-20_000, 8_000));
    let (b, _) = observer(&mut app, SimPos::new(20_000, 0), Some(sheet_with_per(4)));
    let (_, hidden) = thing(&mut app, SimPos::new(20_000, 8_000));
    app.world_mut()
        .entity_mut(hidden)
        .insert(Stealthy { penalty: 6 });
    // No sheet: automatic detection, no roll.
    let (c, c_eye) = observer(&mut app, SimPos::new(0, -20_000), None);
    let (c_target, _) = thing(&mut app, SimPos::new(0, -12_000));
    run_ticks(app.world_mut(), 200);
    let rolls = detect_rolls(&app);
    let targets = |u: SimId| {
        rolls
            .iter()
            .filter(|r| r.1 == u)
            .map(|r| r.4)
            .collect::<Vec<_>>()
    };
    assert!(targets(a).iter().all(|&t| t == 12));
    assert!(
        targets(b).iter().all(|&t| t == 6),
        "Per 14 − 2 (band) − 6 (stealthy)"
    );
    let ok = |u: SimId| rolls.iter().filter(|r| r.1 == u && r.2).count();
    assert!(ok(a) > ok(b) * 3, "{} vs {}", ok(a), ok(b));
    assert!(targets(c).is_empty());
    assert!(senses(&app, c_eye, c_target));

    // Default config: a spotted target is tracked without new rolls.
    let mut app = app_with(3, NpcPlugins::new().with_per_roll_detection(), |_| {});
    let (eye_id, eye) = observer(&mut app, SimPos::new(0, 0), Some(sheet_with_per(4)));
    let (target, _) = thing(&mut app, SimPos::new(0, 3_000));
    run_ticks(app.world_mut(), 100);
    let rolls = detect_rolls(&app);
    let first_success = rolls
        .iter()
        .position(|r| r.2)
        .expect("Per 14 at 3 m spots it");
    assert_eq!(rolls.len(), first_success + 1, "no rolls once spotted");
    assert!(rolls.iter().all(|r| r.1 == eye_id));
    assert!(senses(&app, eye, target));
}

#[test]
fn builder_registers_per_roll_but_activates_it_only_on_request() {
    let app = app_with(1, NpcPlugins::new().with_perception().with_rules(), |_| {});
    let checks = app.world().resource::<DetectionChecks>();
    assert!(checks.checks.contains_key(PER_ROLL));
    assert_eq!(checks.active, "automatic");
    assert!(app.is_plugin_added::<PerceptionRulesPlugin>());
    assert!(!app.is_plugin_added::<AgentIoPlugin>());

    let app = app_with(1, NpcPlugins::new().with_per_roll_detection(), |_| {});
    assert_eq!(app.world().resource::<DetectionChecks>().active, PER_ROLL);

    // Nothing opt-in without asking.
    let app = app_with(1, NpcPlugins::new(), |_| {});
    assert!(!app.is_plugin_added::<PerceptionPlugin>());
    assert!(!app.is_plugin_added::<RulesPlugin>());
    assert!(!app.is_plugin_added::<PerceptionRulesPlugin>());
    assert!(app.world().get_resource::<DetectionChecks>().is_none());

    let app = app_with(
        1,
        NpcPlugins::new()
            .with_perception()
            .with_rules()
            .with_agent_io(),
        |_| {},
    );
    assert!(app.is_plugin_added::<PerceptionAgentPlugin>());
    assert!(app.is_plugin_added::<RulesAgentPlugin>());
}

// ------------------------------------------------------------ perception × agent-io

fn call(app: &mut App, method: &str, params: Value) -> Value {
    agent_io::call(app.world_mut(), method, Some(params))
        .unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

fn ids(list: &Value) -> Vec<u64> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_u64().unwrap())
        .collect()
}

#[test]
fn observe_hides_occluded_entities_that_world_state_shows() {
    let mut app = app_with(
        1,
        NpcPlugins::new().with_perception().with_agent_io(),
        |_| {},
    );
    let (eye_id, _) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(500, 500),
            SOUTH,
            FixedFacing,
            Sensors::new().vision(20_000, 60),
        ),
    );
    let (target, _) = thing(&mut app, SimPos::new(500, 8_500));
    let (wall, _) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(500, 4_500),
            GridOccluder(Footprint {
                half_x_mm: 2_000,
                half_z_mm: 0,
            }),
        ),
    );
    run_ticks(app.world_mut(), 3);

    let obs = call(&mut app, "agent/observe", json!({"unit": eye_id.0}));
    assert_eq!(obs["omniscient"], json!(false));
    let p = &obs["sections"]["perception"];
    assert_eq!(p["sensors"], json!(true));
    assert!(!ids(&p["sensed"]).contains(&u64::from(target.0)), "{p}");
    assert!(!ids(&p["remembered"]).contains(&u64::from(target.0)), "{p}");
    let state = call(&mut app, "world/state", json!({}));
    let in_state = state["entities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["sim_id"] == json!(target.0));
    assert!(in_state, "world/state (debug) shows everything");

    // Remove the occluder (a recorded command): now perceived.
    submit(app.world_mut(), None, SimCommand::Despawn { unit: wall.0 }).unwrap();
    run_ticks(app.world_mut(), 2);
    let obs = call(&mut app, "agent/observe", json!({"unit": eye_id.0}));
    let sensed = &obs["sections"]["perception"]["sensed"];
    assert_eq!(ids(sensed), vec![u64::from(target.0)], "{sensed}");
    assert_eq!(sensed[0]["via"], json!("vision"));
    assert_eq!(sensed[0]["pos"], json!([500, 8_500]));
    assert_eq!(sensed[0]["dist_mm"], json!(8_000));
    let text = call(
        &mut app,
        "agent/observe",
        json!({"unit": eye_id.0, "format": "text"}),
    );
    let text = text["text"].as_str().unwrap();
    assert!(
        text.contains(&format!(
            "#{} at (0.50 m, 8.50 m), 8.00 m away (vision)",
            target.0
        )),
        "{text}"
    );
    assert!(!text.contains("No perception model"), "{text}");

    // Put a wall back: no longer sensed, remembered with its last-seen tick.
    spawn_sim(
        app.world_mut(),
        (SimPos::new(500, 6_500), GridOccluder::default()),
    );
    run_ticks(app.world_mut(), 5);
    let obs = call(&mut app, "agent/observe", json!({"unit": eye_id.0}));
    let p = &obs["sections"]["perception"];
    assert!(ids(&p["sensed"]).is_empty(), "{p}");
    assert_eq!(ids(&p["remembered"]), vec![u64::from(target.0)]);
    let r = &p["remembered"][0];
    assert_eq!(
        r["age_ticks"].as_u64().unwrap() + r["last_seen_tick"].as_u64().unwrap(),
        obs["tick"].as_u64().unwrap()
    );
}

#[test]
fn without_perception_glue_observations_stay_omniscient() {
    let mut app = app_with(1, NpcPlugins::new().with_agent_io(), |_| {});
    let (id, _) = spawn_sim(app.world_mut(), SimPos::new(0, 0));
    let obs = call(&mut app, "agent/observe", json!({"unit": id.0}));
    assert_eq!(obs["omniscient"], json!(true));
    assert!(obs["sections"].get("perception").is_none());
    assert!(obs["sections"].get("sheet").is_none());
}

// ------------------------------------------------------------ rules × agent-io

fn rules_app(seed: u64) -> App {
    app_with(
        seed,
        NpcPlugins::new().with_rules().with_agent_io(),
        |app| {
            app.register_skill(
                SkillDef::new("Climbing", Stat::Dx, Difficulty::Average).default_stat(Stat::Dx, -5),
            );
        },
    )
}

fn climber(app: &mut App) -> SimId {
    let mut sheet = CharacterSheet::default();
    sheet.attributes.dx = 12;
    sheet.skills.insert("Climbing".into(), 2);
    sheet.unspent = 10;
    spawn_sim(app.world_mut(), (SimPos::new(0, 0), sheet)).0
}

fn sheet_of(app: &App, unit: SimId) -> CharacterSheet {
    let e = app.world().resource::<SimIndex>().get(unit).unwrap();
    app.world().get::<CharacterSheet>(e).unwrap().clone()
}

#[test]
fn sheet_section_and_spend_points_are_recorded_and_replayable() {
    let mut app = rules_app(5);
    let unit = climber(&mut app);
    call(
        &mut app,
        "agent/attach",
        json!({"unit": unit.0, "controller": "c", "mode": "Advisor"}),
    );
    run_ticks(app.world_mut(), 1);

    let obs = call(&mut app, "agent/observe", json!({"unit": unit.0}));
    let sheet = &obs["sections"]["sheet"];
    assert_eq!(sheet["attributes"]["DX"], json!(12));
    assert_eq!(sheet["points"]["unspent"], json!(10));
    assert_eq!(sheet["skills"][0]["name"], json!("Climbing"));
    assert_eq!(sheet["skills"][0]["level"], json!(12));
    assert_eq!(sheet["skills"][0]["relative"], json!("DX"));
    let aff = obs["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["verb"] == json!("spend_points"))
        .expect("spend_points offered")
        .clone();
    assert_eq!(aff["ok"], json!(true), "{aff}");
    let text = call(
        &mut app,
        "agent/observe",
        json!({"unit": unit.0, "format": "text"}),
    );
    let text = text["text"].as_str().unwrap();
    assert!(text.contains("Skills: Climbing 12 (DX)."), "{text}");
    assert!(text.contains("Unspent character points: 10."), "{text}");

    // Too expensive: rejected with a reason, nothing submitted.
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": unit.0, "controller": "c", "verb": "spend_points",
               "args": {"purchase": {"type": "Stat", "stat": "Dx", "levels": 5}}}),
    );
    assert_eq!(r["accepted"], json!(false), "{r}");
    assert_eq!(r["reason"], json!("not_enough_points"), "{r}");
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": unit.0, "controller": "c", "verb": "spend_points", "args": {}}),
    );
    assert_eq!(r["reason"], json!("invalid_args"), "{r}");

    // Accepted: a SpendPoints command applied at the next tick.
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": unit.0, "controller": "c", "affordance": "spend_points",
               "args": {"purchase": {"type": "Skill", "skill": "Climbing", "points": 2}}}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    assert_eq!(sheet_of(&app, unit).unspent, 10, "not before its tick");
    run_ticks(app.world_mut(), 1);
    let after = sheet_of(&app, unit);
    assert_eq!(after.unspent, 8);
    assert_eq!(after.skills["Climbing"], 4);
    let log = agent_io::api::command_log(app.world());
    let spend = log
        .iter()
        .find_map(|(t, _, c)| match c {
            SimCommand::Content { kind, .. } if kind == "SpendPoints" => Some((*t, c.clone())),
            _ => None,
        })
        .expect("SpendPoints is recorded");
    let hash = state_hash(app.world_mut());

    // Replay without the agent: the recorded commands give the same state.
    let mut replay = rules_app(5);
    let unit2 = climber(&mut replay);
    assert_eq!(unit, unit2);
    for (t, _, c) in &log {
        submit(replay.world_mut(), Some(*t), c.clone()).unwrap();
    }
    let ticks = app.world().resource::<SimClock>().tick;
    run_ticks(replay.world_mut(), ticks);
    assert_eq!(sheet_of(&replay, unit2), after);
    assert_eq!(state_hash(replay.world_mut()), hash);

    // The command validates on its own too (npc/command, command files).
    let broke = SpendPoints {
        unit: unit.0,
        purchase: npc_framework::rules::Purchase::Skill {
            skill: "Climbing".into(),
            points: 9,
        },
    };
    assert!(submit(app.world_mut(), None, SimCommand::content(&broke)).is_err());
    assert_eq!(spend.0, 1, "applied at tick 1");
}

#[test]
fn spend_points_is_unavailable_without_points_or_for_observers() {
    let mut app = rules_app(1);
    let unit = climber(&mut app);
    call(
        &mut app,
        "agent/attach",
        json!({"unit": unit.0, "controller": "o", "mode": "Observer"}),
    );
    run_ticks(app.world_mut(), 1);
    let obs = call(&mut app, "agent/observe", json!({"unit": unit.0}));
    let aff = obs["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["verb"] == json!("spend_points"))
        .unwrap()
        .clone();
    assert_eq!(aff["ok"], json!(false));
    assert_eq!(aff["why_not"], json!("observers cannot act"));

    let e = app.world().resource::<SimIndex>().get(unit).unwrap();
    app.world_mut()
        .get_mut::<CharacterSheet>(e)
        .unwrap()
        .unspent = 0;
    call(
        &mut app,
        "agent/attach",
        json!({"unit": unit.0, "controller": "d", "mode": "Advisor", "force": true}),
    );
    run_ticks(app.world_mut(), 1);
    let obs = call(&mut app, "agent/observe", json!({"unit": unit.0}));
    let aff = obs["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["verb"] == json!("spend_points"))
        .unwrap()
        .clone();
    assert_eq!(aff["why_not"], json!("no unspent character points"));
}

// ------------------------------------------------- final review FX1 fixes

/// The `Roll` values (dice totals) of `unit`'s detection rolls, per tick.
fn rolls_per_tick(app: &App, unit: SimId) -> std::collections::BTreeMap<u64, Vec<i32>> {
    let mut out: std::collections::BTreeMap<u64, Vec<i32>> = Default::default();
    for (tick, u, _, roll, _) in detect_rolls(app) {
        if u == unit {
            out.entry(tick).or_default().push(roll);
        }
    }
    out
}

/// Final review cleanup 4: the `per_roll` dice key collided for noises
/// without a source (every one rolled the same dice) ...
#[test]
fn per_roll_dice_differ_for_two_sourceless_noises_in_one_tick() {
    let mut app = app_with(5, NpcPlugins::new().with_per_roll_detection(), |_| {});
    let (ear, ee) = spawn_sim(
        app.world_mut(),
        (SimPos::new(0, 0), SOUTH, Sensors::new().hearing(10_000)),
    );
    app.world_mut().entity_mut(ee).insert(sheet_with_per(0));
    for _ in 0..60 {
        emit_noise(app.world_mut(), Noise::at(SimPos::new(1000, 0), 5000));
        emit_noise(app.world_mut(), Noise::at(SimPos::new(-1000, 0), 5000));
        run_ticks(app.world_mut(), 1);
    }
    let per_tick = rolls_per_tick(&app, ear);
    assert_eq!(per_tick.len(), 60);
    assert!(per_tick.values().all(|r| r.len() == 2));
    assert!(
        per_tick.values().any(|r| r[0] != r[1]),
        "two noises, two independent rolls: {per_tick:?}"
    );
}

/// ... and for targets whose `SimId`s differ by a multiple of 2^30.
#[test]
fn per_roll_dice_differ_for_targets_2_pow_30_apart() {
    let mut app = app_with(5, NpcPlugins::new().with_per_roll_detection(), |app| {
        app.insert_resource(PerRollConfig {
            reroll_sensed: true,
            ..Default::default()
        });
    });
    let (eye, _) = observer(&mut app, SimPos::new(0, 0), Some(sheet_with_per(0)));
    let (low, _) = thing(&mut app, SimPos::new(-1000, 8_000));
    app.world_mut()
        .resource_mut::<npc_framework::kernel::SimIdAllocator>()
        .next = low.0 + (1 << 30);
    let (high, _) = thing(&mut app, SimPos::new(1000, 8_000));
    assert_eq!(high.0, low.0 + (1 << 30));
    run_ticks(app.world_mut(), 60);
    let per_tick = rolls_per_tick(&app, eye);
    assert!(per_tick.values().all(|r| r.len() == 2), "{per_tick:?}");
    assert!(
        per_tick.values().any(|r| r[0] != r[1]),
        "two targets, two independent rolls: {per_tick:?}"
    );
}
