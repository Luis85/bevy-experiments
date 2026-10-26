//! Wave E1: the interaction glue (`npc_framework::glue`, ADR 0011).
//!
//! - interaction × rules: a skill check is a `roll_skill`; a failed roll
//!   spends the time and changes nothing;
//! - interaction × perception: doors gate line of sight, door noises are
//!   heard;
//! - interaction × agent-io: affordances with `why_not`, the `inventory`
//!   section, and `agent/act` (Direct and Advisor) taking the same path as
//!   the BT leaf and the `Interact` command.

#![cfg(all(
    feature = "interaction",
    feature = "perception",
    feature = "rules",
    feature = "agent-io"
))]

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io;
use npc_framework::interaction::builtin::builtin_verbs;
use npc_framework::kernel::journal::JournalKind;
use npc_framework::kernel::spatial::{CellCoord, GridBounds};
use npc_framework::prelude::*;
use serde_json::{Value, json};

const WALL: CellInfo = CellInfo {
    flags: CellFlags {
        blocking: true,
        occluding: true,
        tags: 0,
    },
    extra_cost: 0,
};

fn hold_start(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Running
}
fn hold_stop(_: &mut World, _: Entity, _: StopReason) {}

fn app_with(seed: u64, setup: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(32, 32)))
        .insert_resource(PerceptionConfig {
            stagger: 1,
            ..Default::default()
        });
    app.add_plugins(
        NpcPlugins::new()
            .seed(seed)
            .without_driver()
            .with_interaction()
            .with_rules()
            .with_perception()
            .with_agent_io(),
    )
    .add_plugins(JournalPlugin::default())
    .register_skill(
        SkillDef::new("Lockpicking", Stat::Dx, Difficulty::Average).default_stat(Stat::Dx, -5),
    )
    .register_item_kind(
        ItemKindDef::new("axe")
            .weight_g(1500)
            .tag("tool:axe")
            .equip_slot("main_hand"),
    )
    .register_item_kind(ItemKindDef::new("gate key").weight_g(50).tag("key:gate"))
    .register_action("Hold", hold_start, hold_stop)
    .register_tree("hold", |b| {
        b.leaf("Hold", "Hold", ());
    })
    .register_tree("grab", |b| {
        b.sequence("Root", |b| {
            b.leaf("Grab", "Interact", "pick_up");
            b.leaf("Hold", "Hold", ());
        });
    });
    setup(&mut app);
    app.finish();
    app.cleanup();
    app
}

fn run(app: &mut App, n: u64) {
    run_ticks(app.world_mut(), n);
}

fn actor(app: &mut App, pos: SimPos) -> (SimId, Entity) {
    spawn_sim(
        app.world_mut(),
        (
            pos,
            Interactor,
            Inventory::new(4, 10_000),
            Equipment::hands(),
        ),
    )
}

fn door_at(app: &mut App, x: i32, z: i32, door: Door) -> (SimId, Entity) {
    spawn_sim(
        app.world_mut(),
        (SimPos::new(x * 1000 + 500, z * 1000 + 500), door),
    )
}

fn wall(app: &mut App) {
    let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
    for z in -16..16 {
        if z != 0 {
            grid.set_static(CellCoord::new(0, z), WALL);
        }
    }
}

fn interact(app: &mut App, unit: SimId, verb: &str, target: SimId) {
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&Interact::new(unit, verb, target)),
    )
    .unwrap_or_else(|e| panic!("{verb}: {e}"));
}

fn journal(app: &App) -> Vec<(u64, Option<SimId>, JournalKind)> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .map(|e| (e.tick, e.unit, e.kind.clone()))
        .collect()
}

fn call(app: &mut App, m: &str, p: Value) -> Value {
    agent_io::api::call(app.world_mut(), m, Some(p)).unwrap_or_else(|e| panic!("{m}: {e:?}"))
}

// ---------------------------------------------------------------- rules

/// One `unlock` with a hard Lockpicking check; returns (roll success,
/// interaction outcome, door still locked, outcome tick - start tick).
fn unlock_attempt(seed: u64) -> (bool, String, bool, u64) {
    let mut app = app_with(seed, |app| {
        app.override_interaction(
            builtin_verbs()
                .into_iter()
                .find(|d| d.name == "unlock")
                .unwrap()
                .check("Lockpicking", -3),
        );
    });
    let here = SimPos::new(-500, 500);
    let (d, de) = door_at(&mut app, 0, 0, Door::locked("key:gate"));
    let (a, ae) = actor(&mut app, here);
    app.world_mut()
        .entity_mut(ae)
        .insert(CharacterSheet::default());
    let (key, _) = spawn_sim(app.world_mut(), (here, Item::new("gate key")));
    run(&mut app, 1);
    interact(&mut app, a, "pick_up", key);
    run(&mut app, 11);
    let start = app.world().resource::<SimClock>().tick;
    interact(&mut app, a, "unlock", d);
    run(&mut app, 21);
    let j = journal(&app);
    let roll = j
        .iter()
        .find_map(|(_, u, k)| match k {
            JournalKind::Roll {
                skill,
                purpose,
                success,
                ..
            } if *u == Some(a) && skill == "Lockpicking" => {
                assert_eq!(purpose, "interaction.unlock");
                Some(*success)
            }
            _ => None,
        })
        .expect("the check was rolled");
    let (tick, outcome) = j
        .iter()
        .find_map(|(t, _, k)| match k {
            JournalKind::Interacted { verb, outcome, .. }
                if verb == "unlock" && outcome != "started" =>
            {
                Some((*t, outcome.clone()))
            }
            _ => None,
        })
        .unwrap();
    let locked = app.world().get::<Door>(de).unwrap().is_locked();
    (roll, outcome, locked, tick - start)
}

#[test]
fn a_failed_skill_roll_spends_the_time_and_changes_nothing() {
    let mut failures = 0;
    for seed in 0..12 {
        let (roll, outcome, locked, took) = unlock_attempt(seed);
        assert_eq!(took, 20, "the full duration either way");
        if roll {
            assert_eq!(
                (outcome.as_str(), locked),
                ("success", false),
                "seed {seed}"
            );
        } else {
            failures += 1;
            assert_eq!((outcome.as_str(), locked), ("failed", true), "seed {seed}");
        }
    }
    assert!(failures > 0, "DX 10 - 5 - 3 = 2: most rolls fail");
    assert_eq!(unlock_attempt(3), unlock_attempt(3), "deterministic");
}

// ---------------------------------------------------------------- perception

#[test]
fn doors_gate_line_of_sight_and_their_noise_is_heard() {
    let mut app = app_with(1, |_| {});
    wall(&mut app);
    let (d, _) = door_at(&mut app, 0, 0, Door::closed());
    let (eye, _) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(-4500, 500),
            Facing::new(1, 0),
            FixedFacing,
            Sensors::new().vision(20_000, 45).hearing(10_000),
        ),
    );
    let (target, _) = spawn_sim(
        app.world_mut(),
        (SimPos::new(4500, 500), Perceivable::new(1)),
    );
    let (opener, _) = actor(&mut app, SimPos::new(-500, 500));
    run(&mut app, 5);
    let seen = |app: &App| {
        journal(app).iter().any(|(_, u, k)| {
            *u == Some(eye)
                && matches!(k, JournalKind::Perceived { target: t, via } if *t == target && via == "vision")
        })
    };
    assert!(!seen(&app), "the closed door hides the target");
    interact(&mut app, opener, "open", d);
    run(&mut app, 15);
    assert!(seen(&app), "the open door does not");
    assert!(
        journal(&app).iter().any(|(_, u, k)| *u == Some(eye)
            && matches!(k, JournalKind::Perceived { target: t, via } if *t == opener && via == "hearing")),
        "the door noise was heard"
    );
}

// ---------------------------------------------------------------- agent-io

#[test]
fn affordances_explain_why_not_and_the_inventory_is_observed() {
    let mut app = app_with(1, |_| {});
    let here = SimPos::new(-500, 500);
    let (d, _) = door_at(&mut app, 0, 0, Door::locked("key:gate"));
    let (tree, _) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(-500, 1500),
            ToolTarget::new("tool:axe").duration(30),
        ),
    );
    let (a, ae) = actor(&mut app, here);
    let hold = app
        .world()
        .resource::<AiRegistry>()
        .tree_id("hold")
        .unwrap();
    app.world_mut().entity_mut(ae).insert(Brain { tree: hold });
    let (axe, _) = spawn_sim(app.world_mut(), (here, Item::new("axe")));
    call(
        &mut app,
        "agent/attach",
        json!({"unit": a.0, "controller": "c", "mode": "Direct"}),
    );
    run(&mut app, 1);
    let obs = call(&mut app, "agent/observe", json!({"unit": a.0}));
    let affs = obs["affordances"].as_array().unwrap();
    let find = |id: String| {
        affs.iter()
            .find(|a| a["id"] == json!(id))
            .unwrap_or_else(|| panic!("no {id} in {affs:?}"))
            .clone()
    };
    let open = find(format!("open@{}", d.0));
    assert_eq!(open["ok"], json!(false));
    assert_eq!(open["why_not"], json!("door is locked"));
    let unlock = find(format!("unlock@{}", d.0));
    assert_eq!(unlock["why_not"], json!("door is locked: needs key:gate"));
    assert_eq!(unlock["requires"], json!(["key:gate"]));
    let chop = find(format!("use_tool_on@{}", tree.0));
    assert_eq!(chop["why_not"], json!("needs tool:axe"));
    assert_eq!(chop["est_ticks"], json!(30));
    let pick = find(format!("pick_up@{}", axe.0));
    assert_eq!(pick["ok"], json!(true));
    assert!(pick["label"].as_str().unwrap().contains("axe"));
    assert_eq!(obs["sections"]["inventory"]["items"], json!([]));

    call(
        &mut app,
        "agent/act",
        json!({"unit": a.0, "controller": "c", "affordance": format!("pick_up@{}", axe.0)}),
    );
    run(&mut app, 12);
    let obs = call(
        &mut app,
        "agent/observe",
        json!({"unit": a.0, "format": "text"}),
    );
    let text = obs["text"].as_str().unwrap();
    assert!(text.contains("You carry: the axe"), "{text}");
    let obs = call(&mut app, "agent/observe", json!({"unit": a.0}));
    assert_eq!(
        obs["sections"]["inventory"]["items"][0]["kind"],
        json!("axe")
    );
    let chop = obs["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == json!(format!("use_tool_on@{}", tree.0)))
        .unwrap()
        .clone();
    assert_eq!(chop["why_not"], json!("equip tool:axe first"));

    // A refused act says why.
    let err = agent_io::api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({"unit": a.0, "controller": "c", "verb": "open", "target": d.0})),
    );
    let msg = format!("{err:?}");
    assert!(msg.contains("door is locked"), "{msg}");
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Via {
    Tree,
    Command,
    Direct,
    Advisor,
}

/// A pick-up through `via`; the interaction journal with ticks relative to
/// its first entry.
fn pick_up_via(via: Via) -> Vec<(u64, Option<SimId>, JournalKind)> {
    let mut app = app_with(1, |_| {});
    let here = SimPos::new(500, 500);
    let (axe, _) = spawn_sim(app.world_mut(), (here, Item::new("axe")));
    let name = if via == Via::Tree { "grab" } else { "hold" };
    let tree = app.world().resource::<AiRegistry>().tree_id(name).unwrap();
    let (a, ae) = actor(&mut app, here);
    app.world_mut().entity_mut(ae).insert(Brain { tree });
    let slot = app.world().resource::<InteractionConfig>().target_slot;
    app.world_mut()
        .get_mut::<Blackboard>(ae)
        .unwrap()
        .set(BbKey(slot), BbValue::U32(axe.0));
    match via {
        Via::Tree => {}
        Via::Command => interact(&mut app, a, "pick_up", axe),
        Via::Direct | Via::Advisor => {
            let mode = if via == Via::Direct {
                "Direct"
            } else {
                "Advisor"
            };
            call(
                &mut app,
                "agent/attach",
                json!({"unit": a.0, "controller": "c", "mode": mode}),
            );
            run(&mut app, 1);
            let r = call(
                &mut app,
                "agent/act",
                json!({"unit": a.0, "controller": "c", "verb": "pick_up", "target": axe.0}),
            );
            assert_eq!(r["accepted"], json!(true), "{r}");
        }
    }
    run(&mut app, 20);
    let entries: Vec<_> = journal(&app)
        .into_iter()
        .filter(|(_, _, k)| {
            matches!(
                k,
                JournalKind::Interacted { .. }
                    | JournalKind::ItemMoved { .. }
                    | JournalKind::Reserve { .. }
                    | JournalKind::Release { .. }
            )
        })
        .collect();
    let t0 = entries.first().map_or(0, |e| e.0);
    let carried = app
        .world()
        .get::<Inventory>(ae)
        .unwrap()
        .items
        .contains(&axe);
    assert!(carried, "{via:?}: picked up");
    entries
        .into_iter()
        .map(|(t, u, k)| (t - t0, u, k))
        .collect()
}

#[test]
fn bt_leaf_command_and_agent_act_take_the_same_path() {
    let tree = pick_up_via(Via::Tree);
    assert_eq!(
        tree.len(),
        5,
        "reserve, started, moved, success, release: {tree:?}"
    );
    assert_eq!(pick_up_via(Via::Command), tree);
    assert_eq!(pick_up_via(Via::Direct), tree);
    assert_eq!(pick_up_via(Via::Advisor), tree);
}
