//! Behaviour is data (review fix S3): a tree per trade or life stage,
//! guards with parameters and `not`, and the recorded command that replaces
//! the behaviour document at a tick boundary.
mod life_support;

use bevy_ecs::prelude::*;
use island_content::village::{self, Villager, resident};
use life_support::*;
use npc_framework::ai::AiRegistry;
use npc_framework::kernel::commands::{CommandError, ContentCommand};
use npc_framework::prelude::*;
use serde_json::{Value, json};

const RESIDENT: &str = include_str!("../trees/resident.json");

fn shipped_tree() -> Value {
    serde_json::from_str(RESIDENT).expect("tree")
}

/// The shipped tree under another name, with one branch's guard written
/// with parameters and `not`.
fn miller_tree() -> Value {
    let mut tree = shipped_tree();
    tree["name"] = json!("island_miller");
    tree["root"]["name"] = json!("Miller life");
    let branches = tree["root"]["children"].as_array_mut().expect("branches");
    let idle = branches.pop().expect("idle");
    branches.push(json!({"name": "Not at night", "kind": {"Guard": {
        "if": "life.night", "not": true, "abort": "None"}}, "children": [
        {"name": "Fed enough", "kind": {"Guard": {
            "if": "need_below", "need": "Hunger", "value": 100000, "not": true, "abort": "None"}},
         "children": [{"name": "Miller strolls", "kind": {"Leaf": {"action": "life.go_to",
            "param": {"Name": "centre"}}}}]}]}));
    branches.push(idle);
    tree
}

fn with_miller_tree(world: &mut Value) {
    only_weather(world, "clear");
    world["living_world"]["behavior"] = json!({
        "trees": [
            {"file": "resident.json", "tree": shipped_tree()},
            {"file": "miller.json", "tree": miller_tree()}
        ]
    });
    for profession in economy(world)["professions"]
        .as_array_mut()
        .expect("professions")
    {
        if profession["role"] == "miller" {
            profession["tree"] = json!("island_miller");
        }
    }
}

#[test]
fn a_trade_named_in_data_runs_its_own_tree() {
    let mut app = world_with(with_miller_tree);
    run_ticks(app.world_mut(), 2);
    let world = app.world_mut();
    let miller_tree = world
        .resource::<AiRegistry>()
        .tree_id("island_miller")
        .expect("the miller's tree is registered");
    let mut query = world.query::<(Entity, &Villager, &npc_framework::ai::Brain)>();
    let mut millers = 0;
    for (_, person, brain) in query.iter(world) {
        if person.role == village::Role::Miller {
            millers += 1;
            assert_eq!(brain.tree, miller_tree, "a miller runs the miller's tree");
        } else {
            assert_ne!(brain.tree, miller_tree, "others keep the default tree");
        }
    }
    assert!(millers > 0);
    // The parameterised guards were resolved to plain registered names.
    let world = app.world();
    assert!(
        world
            .resource::<AiRegistry>()
            .cond_id("not:life.night")
            .is_some()
    );
    assert!(
        world
            .resource::<AiRegistry>()
            .cond_id("not:need_below:Hunger:100000")
            .is_some()
    );
}

#[test]
fn bad_parameters_and_aborting_not_guards_are_refused() {
    let load = |tree: Value| {
        let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
        let preset = &mut doc.world["construction"]["world"]["preset"];
        preset["living_world"]["behavior"] = json!({
            "trees": [{"file": "resident.json", "tree": tree}]
        });
        island_content::scenario::load_scenario(&doc, 7).map(|_| ())
    };
    let mut tree = shipped_tree();
    tree["root"]["children"]
        .as_array_mut()
        .expect("branches")
        .insert(
            0,
            json!({"name": "Odd", "kind": {"Guard": {"if": "need_below", "need": "Courage",
            "value": 5, "abort": "None"}}, "children": [
            {"name": "Idle", "kind": {"Leaf": {"action": "life.idle", "param": "None"}}}]}),
        );
    let error = load(tree).expect_err("refused");
    assert!(error.contains("unknown need `Courage`"), "{error}");
    let mut tree = shipped_tree();
    tree["root"]["children"]
        .as_array_mut()
        .expect("branches")
        .insert(
            0,
            json!({"name": "Daytime", "kind": {"Guard": {"if": "life.night", "not": true,
            "abort": "LowerPriority"}}, "children": [
            {"name": "Idle", "kind": {"Leaf": {"action": "life.idle", "param": "None"}}}]}),
        );
    let error = load(tree).expect_err("refused");
    assert!(
        error.contains("a `not` guard cannot abort lower-priority work"),
        "{error}"
    );
}

#[test]
fn replace_behaviour_changes_the_schedule_and_refuses_bad_documents() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 20);
    let mut document = resident::replace::export_behavior(app.world()).expect("behaviour");
    document
        .schedule
        .blocks
        .retain(|block| block.kind != resident::BlockKind::Leisure);
    let command = resident::replace::ReplaceBehavior {
        behavior: document.clone(),
    };
    if let Err(error) = command.validate(app.world()) {
        panic!("{error:?}");
    }
    let before = state_hash(app.world_mut());
    command.apply(app.world_mut());
    assert_ne!(
        state_hash(app.world_mut()),
        before,
        "the document is part of the world"
    );
    assert_eq!(resident::spec(app.world()), Some(&document));
    // Nobody has an evening any more.
    let (_, actor) = all_residents(app.world())[0];
    for _ in 0..50 {
        run_ticks(app.world_mut(), 480);
        assert!(!resident::in_block(
            app.world(),
            actor,
            resident::BlockKind::Leisure
        ));
    }
    // A document naming an unknown action is refused, the world unchanged.
    let mut bad = document.clone();
    let mut tree = shipped_tree();
    tree["root"]["children"]
        .as_array_mut()
        .expect("branches")
        .insert(
            0,
            json!({"name": "Nap", "kind": {"Leaf": {"action": "life.nap", "param": "None"}}}),
        );
    bad.trees[0] =
        serde_json::from_value(json!({"file": "resident.json", "tree": tree})).expect("tree file");
    let before = state_hash(app.world_mut());
    let refused = resident::replace::ReplaceBehavior { behavior: bad };
    match refused.validate(app.world()) {
        Err(CommandError::Unavailable { reason }) => {
            assert!(reason.starts_with("Behaviour not changed:"), "{reason}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert_eq!(state_hash(app.world_mut()), before);
}

/// The shipped tree without its leisure branch.
fn tree_without_leisure() -> Value {
    let mut tree = shipped_tree();
    tree["root"]["children"]
        .as_array_mut()
        .expect("branches")
        .retain(|branch| branch["name"] != "Leisure");
    tree
}

/// One run: 2400 ticks, a `ReplaceBehavior` with an edited tree, 2400
/// more; the state hash every 200 ticks and the leaves started after it.
fn run_with_replacement(seed: u64) -> (Vec<u64>, usize, u64) {
    run_with_replacement_manifest(seed).0
}

type Manifest = Vec<(String, String)>;

fn run_with_replacement_manifest(seed: u64) -> ((Vec<u64>, usize, u64), Manifest) {
    let mut app = world_for(seed, |world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2400);
    let mut document = resident::replace::export_behavior(app.world()).expect("behaviour");
    document.trees[0] =
        serde_json::from_value(json!({"file": "resident.json", "tree": tree_without_leisure()}))
            .expect("tree file");
    let command = resident::replace::ReplaceBehavior { behavior: document };
    if let Err(error) = command.validate(app.world()) {
        panic!("{error:?}");
    }
    let before = npc_framework::kernel::registry::content_hash(app.world_mut());
    npc_framework::prelude::submit(
        app.world_mut(),
        None,
        npc_framework::prelude::SimCommand::content(&command),
    )
    .expect("submitted");
    let mut hashes = Vec::new();
    let mut leisure = 0;
    for _ in 0..12 {
        run_ticks(app.world_mut(), 200);
        hashes.push(state_hash(app.world_mut()));
        // Nobody runs a branch the tree no longer has.
        let world = app.world();
        leisure += all_residents(world)
            .into_iter()
            .filter_map(|(_, actor)| village::view::latest_decision(world, actor))
            .filter(|decision| decision.branch.iter().any(|node| node == "Leisure"))
            .count();
    }
    let after = npc_framework::kernel::registry::content_hash(app.world_mut());
    assert_ne!(before, after, "the content hash follows the trees");
    let manifest = app
        .world()
        .resource::<npc_framework::kernel::registry::ContentHash>()
        .manifest
        .clone();
    ((hashes, leisure, after), manifest)
}

#[test]
fn replacing_a_tree_takes_effect_and_is_deterministic() {
    let (a, leisure, content) = run_with_replacement(7);
    let (b, _, again) = run_with_replacement(7);
    assert_eq!(
        a, b,
        "the same command at the same tick gives the same world"
    );
    assert_eq!(content, again);
    assert_eq!(leisure, 0, "no decision runs the removed branch");
    // A fresh island built from the edited document has the same content
    // hash, so a save made after the replacement continues there.
    let mut fresh = world_for(7, |world| {
        only_weather(world, "clear");
        world["living_world"]["behavior"] = json!({
            "trees": [{"file": "resident.json", "tree": tree_without_leisure()}]
        });
    });
    let fresh_hash = npc_framework::kernel::registry::content_hash(fresh.world_mut());
    if fresh_hash != content {
        let (_, replaced) = run_with_replacement_manifest(7);
        let built = fresh
            .world()
            .resource::<npc_framework::kernel::registry::ContentHash>()
            .manifest
            .clone();
        let only_replaced: Vec<_> = replaced.iter().filter(|e| !built.contains(e)).collect();
        let only_built: Vec<_> = built.iter().filter(|e| !replaced.contains(e)).collect();
        panic!("manifests differ: replaced only {only_replaced:?}; fresh only {only_built:?}");
    }
}

/// One recorded replay: 1200 ticks, a `ReplaceBehavior` with an edited tree
/// submitted for tick 1201, 1200 more (ADR 0045).
fn record_replacement() -> npc_framework::kernel::replay::Replay {
    let mut app = island_content::replay::try_new_replay_app(7, "island:living_world", 1)
        .expect("island replay app");
    run_ticks(app.world_mut(), 1200);
    let mut document = resident::replace::export_behavior(app.world()).expect("behaviour");
    document.trees[0] =
        serde_json::from_value(json!({"file": "resident.json", "tree": tree_without_leisure()}))
            .expect("tree file");
    let command = resident::replace::ReplaceBehavior { behavior: document };
    npc_framework::prelude::submit(
        app.world_mut(),
        Some(1201),
        npc_framework::prelude::SimCommand::content(&command),
    )
    .expect("submitted");
    npc_framework::kernel::replay::record(&mut app, 1200, 200)
}

#[test]
fn a_recorded_replay_with_a_tree_replacement_verifies_and_rerecords() {
    let replay = record_replacement();
    assert_eq!(replay.commands.len(), 1, "the replacement is in the replay");
    let report = island_content::replay::verify(&replay);
    assert!(report.ok, "{report:?}");
    let text = serde_json::to_string(&replay).expect("json");
    let again = serde_json::to_string(&record_replacement()).expect("json");
    assert_eq!(text, again, "re-recorded byte for byte");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/island-split/replace-behaviour");
    std::fs::create_dir_all(&dir).expect("folder");
    std::fs::write(dir.join("replace_behaviour_s7.json"), &text).expect("written");
    std::fs::write(dir.join("replace_behaviour_s7.again.json"), &again).expect("written");
}

/// The day's rhythm (S3.3, S3.4): residents sit down to a meal once a meal
/// block, meetings last and pay at their end, and buyers keep their seller.
#[test]
fn meals_meetings_and_errands_take_their_time() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    npc_framework::kernel::journal::set_journal(app.world_mut(), true, false);
    let day = app.world().resource::<SimConfig>().day_length;
    let mut started = 0;
    let mut paid = 0;
    let mut errands = 0;
    let mut sat = 0;
    for _ in 0..48 {
        run_ticks(app.world_mut(), day / 48);
        started += effects(app.world(), "VillageSocialMeetingStarted").len();
        paid += effects(app.world(), "VillageSocialMeeting")
            .iter()
            .filter(|effect| effect["paid"] == true)
            .count();
        let world = app.world_mut();
        errands += world
            .query::<&island_content::village::resident::DayRhythm>()
            .iter(world)
            .filter(|rhythm| rhythm.last_meal.is_some())
            .count()
            .min(1);
        let world = app.world();
        sat += all_residents(world)
            .into_iter()
            .filter(|(_, actor)| {
                village::view::latest_decision(world, *actor)
                    .is_some_and(|decision| decision.branch.iter().any(|node| node == "Meal time"))
            })
            .count();
    }
    assert!(sat > 0, "residents sit down to meals");
    assert!(errands > 0, "a meal block is remembered as eaten");
    assert!(started > 0, "meetings start");
    assert!(paid > 0, "a meeting that lasts pays at its end");
}

#[test]
fn every_tagged_drink_and_garment_counts() {
    let app = world_with(|world| {
        only_weather(world, "clear");
        let goods = economy(world)["goods"].as_array_mut().expect("goods");
        goods.push(json!({"key": "tea", "display": "tea", "base_price": 2,
            "tags": ["drink"], "item": {"weight_g": 300, "max_stack": 20}}));
        goods.push(
            json!({"key": "cloak", "display": "cloaks", "base_price": 30,
            "tags": ["clothing"], "durability": 200,
            "item": {"weight_g": 1500, "max_stack": 1, "equip_slot": "body"}}),
        );
    });
    let roles = village::roles::roles(app.world());
    assert!(roles.drinks.contains(&"tea"), "{:?}", roles.drinks);
    assert!(roles.garments.contains(&"cloak"), "{:?}", roles.garments);
}
