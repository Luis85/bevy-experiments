//! Resident behaviour is authored data: the trees in `trees/*.json` and the
//! `behavior` block of the world file.
//!
//! These tests edit only JSON. They prove that the shipped tree is the
//! behaviour residents run, that a broken tree is refused with a message naming
//! the file and the node, that an edited tree is hashed and kept in the
//! construction data, and that a behaviour added purely in JSON happens.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Villager,
    frontier::{self, LivingWorldSpec, labor::ResidentProfile},
    resident::{self, BehaviorSpec, ResidentTraits, parse_tree},
};
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use serde_json::{Value, json};

const FILE: &str = "resident.json";

fn shipped_tree() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("trees/resident.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("shipped tree")).expect("JSON")
}

fn shipped_world() -> Value {
    island_content::scenario::scenario_doc("living_world")
        .expect("scenario")
        .world["construction"]["world"]["preset"]["living_world"]
        .clone()
}

fn app_with(living_world: &Value) -> Result<App, String> {
    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    doc.world["construction"]["world"]["preset"]["living_world"] = living_world.clone();
    island_content::scenario::load_scenario(&doc, 7).map(|loaded| loaded.app)
}

/// A world whose only edit is the resident tree.
fn world_with_tree(tree: &Value) -> Value {
    let mut world = shipped_world();
    world["behavior"] = json!({"trees": [{"file": FILE, "tree": tree}]});
    world
}

fn refused(tree: &Value) -> String {
    match app_with(&world_with_tree(tree)) {
        Ok(_) => panic!("the edited tree must be refused"),
        Err(error) => error,
    }
}

/// The child of `node` with this name (nested shape).
fn child<'a>(node: &'a mut Value, name: &str) -> &'a mut Value {
    node["children"]
        .as_array_mut()
        .expect("children")
        .iter_mut()
        .find(|child| child["name"] == name)
        .unwrap_or_else(|| panic!("no child named {name}"))
}

fn residents(world: &World) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| world.get::<ResidentProfile>(**entity).is_some())
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

#[test]
fn the_shipped_tree_file_is_the_tree_every_resident_runs() {
    let parsed = parse_tree(FILE, &shipped_tree()).expect("shipped tree parses");
    let built_in = BehaviorSpec::default();
    assert_eq!(built_in.trees.len(), 1);
    assert_eq!(built_in.trees[0].file, FILE);
    assert_eq!(built_in.trees[0].tree, parsed);
    let preset = island_content::worlds::builtin_preset("living_world").expect("preset");
    assert_eq!(
        preset.living_world.expect("regional spec").behavior,
        built_in
    );

    let app = island_content::new_sim_app(7, "living_world");
    let registry = app.world().resource::<AiRegistry>();
    let id = registry
        .tree_id(resident::RESIDENT_TREE)
        .expect("resident tree");
    // The engine prepends its external-control nodes; the rest is the file.
    let registered = registry.tree(id);
    let offset = registered.nodes.len() - parsed.nodes.len();
    assert_eq!(offset, 3);
    for (index, authored) in parsed.nodes.iter().enumerate() {
        let node = &registered.nodes[offset + index];
        assert_eq!(node.name, authored.name);
        assert_eq!(
            serde_json::to_value(&node.kind).expect("kind"),
            serde_json::to_value(&authored.kind).expect("kind")
        );
        let children: Vec<usize> = node.children.iter().map(|c| *c as usize - offset).collect();
        let expected: Vec<usize> = authored.children.iter().map(|c| *c as usize).collect();
        assert_eq!(children, expected);
    }
    // The monolithic decision leaf is not part of the regional profile.
    assert!(registry.action_id("IslandLifeDecision").is_none());
    for info in resident::actions::ACTIONS {
        assert!(registry.action_id(info.name).is_some(), "{}", info.name);
    }
    for info in resident::conditions::CONDITIONS {
        assert!(registry.cond_id(info.name).is_some(), "{}", info.name);
    }
    let people = residents(app.world());
    assert_eq!(people.len(), 42);
    for (_, actor) in people {
        assert_eq!(app.world().get::<Brain>(actor).expect("brain").tree, id);
        assert!(app.world().get::<ResidentTraits>(actor).is_some());
    }
}

#[test]
fn a_world_without_a_behaviour_block_gets_the_built_in_behaviour() {
    // The shipped construction writes the built-in block out; drop it.
    let mut world = shipped_world();
    let written: BehaviorSpec =
        serde_json::from_value(world["behavior"].clone()).expect("written block");
    assert_eq!(written, BehaviorSpec::default());
    world.as_object_mut().expect("world").remove("behavior");
    let spec: LivingWorldSpec = serde_json::from_value(world.clone()).expect("shipped world");
    assert_eq!(spec.behavior, BehaviorSpec::default());
    spec.validate().expect("valid");
    // A partial block keeps the built-in sections it does not mention.
    let mut partial = world;
    partial["behavior"] = json!({"schedule": {"meal_hunger_threshold": 70000}});
    let spec: LivingWorldSpec = serde_json::from_value(partial).expect("partial block");
    assert_eq!(spec.behavior.schedule.meal_hunger_threshold, 70_000);
    assert_eq!(spec.behavior.trees, BehaviorSpec::default().trees);
    spec.validate().expect("valid");
}

#[test]
fn the_flat_and_the_nested_shape_are_the_same_tree() {
    let nested = parse_tree(FILE, &shipped_tree()).expect("nested");
    // Serialising a tree gives the engine's flat TreeDef shape.
    let flat = serde_json::to_value(&nested).expect("flat");
    assert!(flat.get("nodes").is_some() && flat.get("root").is_none());
    assert_eq!(parse_tree(FILE, &flat).expect("flat parses"), nested);
    // The construction data of a world round-trips with its trees.
    let spec: LivingWorldSpec = serde_json::from_value(shipped_world()).expect("world");
    let saved = serde_json::to_value(&spec).expect("saved");
    assert_eq!(saved["behavior"]["trees"][0]["file"], FILE);
    let loaded: LivingWorldSpec = serde_json::from_value(saved).expect("continued");
    assert_eq!(loaded, spec);
}

#[test]
fn an_unknown_action_or_condition_is_refused_naming_the_file_and_the_node() {
    let mut tree = shipped_tree();
    child(&mut tree["root"], "Idle")["kind"]["Leaf"]["action"] = json!("life.nap");
    let error = refused(&tree);
    assert!(
        error.contains("behaviour tree file `resident.json`, node 95 `Idle`")
            && error.contains("unknown action `life.nap`")
            && error.contains("life.sleep"),
        "{error}"
    );
    let mut tree = shipped_tree();
    child(&mut tree["root"], "Tired")["kind"]["Guard"]["cond"] = json!("life.sleepy");
    let error = refused(&tree);
    assert!(
        error.contains("`resident.json`, node 35 `Tired`")
            && error.contains("unknown condition `life.sleepy`")
            && error.contains("life.tired"),
        "{error}"
    );
}

#[test]
fn a_bad_leaf_parameter_is_refused_naming_the_file_and_the_node() {
    // A name the action does not know.
    let mut tree = shipped_tree();
    let chores = child(&mut tree["root"], "Chores");
    child(chores, "Replace clothing")["kind"]["Leaf"]["param"] = json!({"Name": "gold"});
    let error = refused(&tree);
    assert!(
        error.contains("`resident.json`, node 56 `Replace clothing`")
            && error.contains("action `life.buy` does not accept parameter {\"Name\":\"gold\"}")
            && error.contains("provisions"),
        "{error}"
    );
    // A good key is accepted; a parameter where none belongs is not.
    let mut tree = shipped_tree();
    let chores = child(&mut tree["root"], "Chores");
    child(chores, "Replace clothing")["kind"]["Leaf"]["param"] = json!({"Name": "bread"});
    let _app = app_with(&world_with_tree(&tree)).expect("a good key is a valid thing to buy");
    child(&mut tree["root"], "Idle")["kind"]["Leaf"]["param"] = json!({"Int": 3});
    let error = refused(&tree);
    assert!(
        error.contains("node 95 `Idle`") && error.contains("expected no parameter"),
        "{error}"
    );
    // A parameter of a shape the format does not have.
    let mut tree = shipped_tree();
    child(&mut tree["root"], "Idle")["kind"]["Leaf"]["param"] = json!({"Number": 3});
    let error = refused(&tree);
    assert!(
        error.contains("`resident.json`, node 95 `Idle`") && error.contains("unknown variant"),
        "{error}"
    );
    // A place nobody defined.
    let mut tree = shipped_tree();
    child(&mut tree["root"], "Night")["children"][0]["kind"]["Leaf"]["param"] =
        json!({"Name": "lighthouse"});
    let error = refused(&tree);
    assert!(
        error.contains("node 91 `Stay home`") && error.contains("centre, home, meal, work"),
        "{error}"
    );
}

#[test]
fn a_guard_that_could_abort_work_without_being_able_to_start_is_refused() {
    // A schedule condition promises nothing about its branch.
    let mut tree = shipped_tree();
    child(&mut tree["root"], "Work hours")["kind"]["Guard"]["abort"] = json!("LowerPriority");
    let error = refused(&tree);
    assert!(
        error.contains("`resident.json`, node 73 `Work hours`")
            && error.contains("`life.in_work_window` does not promise that its branch can start")
            && error.contains("\"abort\": \"None\""),
        "{error}"
    );
    // `life.thirsty` holds when water can be drunk, bought or fetched; a branch
    // that cannot fetch would abort work and then fail, over and over.
    let mut tree = shipped_tree();
    let thirst = child(&mut tree["root"], "Thirst");
    child(thirst, "Quench thirst")["children"]
        .as_array_mut()
        .expect("children")
        .pop();
    let error = refused(&tree);
    assert!(
        error.contains("node 17 `Thirst`")
            && error.contains("cannot start `life.gather water` first"),
        "{error}"
    );
    // With an always-starting last resort the same guard is fine.
    child(thirst_of(&mut tree), "Quench thirst")["children"]
        .as_array_mut()
        .expect("children")
        .push(json!({"name": "Wait", "kind": {"Leaf": {"action": "life.idle", "param": "None"}}}));
    let _app =
        app_with(&world_with_tree(&tree)).expect("an always-starting leaf satisfies the guard");
    // A guard has exactly one child.
    let mut tree = shipped_tree();
    child(&mut tree["root"], "Tired")["children"]
        .as_array_mut()
        .expect("children")
        .push(json!({"name": "Wait", "kind": {"Leaf": {"action": "life.idle", "param": "None"}}}));
    let error = refused(&tree);
    assert!(
        error.contains("node 35 `Tired`") && error.contains("exactly one child, found 2"),
        "{error}"
    );
}

fn thirst_of(tree: &mut Value) -> &mut Value {
    child(&mut tree["root"], "Thirst")
}

#[test]
fn structural_mistakes_and_a_missing_resident_tree_are_refused() {
    // Flat shape with a child index that skips a node.
    let nested = parse_tree(FILE, &shipped_tree()).expect("tree");
    let mut flat = serde_json::to_value(&nested).expect("flat");
    flat["nodes"][1]["children"] = json!([3]);
    let error = refused(&flat);
    assert!(
        error.contains("node 1 `Freezing`") && error.contains("expected child index 2, found 3"),
        "{error}"
    );
    // A misspelt field is not silently ignored.
    let mut tree = shipped_tree();
    child(&mut tree["root"], "Idle")["kinds"] = json!("Selector");
    let error = refused(&tree);
    assert!(
        error.contains("node 95 `Idle`") && error.contains("unknown field `kinds`"),
        "{error}"
    );
    // Residents need their tree.
    let mut tree = shipped_tree();
    tree["name"] = json!("island_resident_spare");
    let error = refused(&tree);
    assert!(
        error.contains("needs a tree named `island_life`"),
        "{error}"
    );
    // Unknown fields of the block itself.
    let mut world = shipped_world();
    world["behavior"] = json!({"scedule": {}});
    assert!(app_with(&world).is_err(), "unknown field");
}

#[test]
fn an_edited_tree_is_hashed_and_kept_as_construction_data() {
    let mut plain = app_with(&shipped_world()).expect("shipped");
    let mut tree = shipped_tree();
    // Swap two options of a selector: structure, not just a name.
    let profession = child(child(&mut tree["root"], "Work hours"), "Profession");
    profession["children"]
        .as_array_mut()
        .expect("children")
        .swap(3, 4);
    let mut edited = app_with(&world_with_tree(&tree)).expect("edited tree loads");
    assert_ne!(
        state_hash(plain.world_mut()),
        state_hash(edited.world_mut()),
        "behaviour data must be covered by the state hash"
    );
    let kept = &frontier::spec(edited.world())
        .expect("construction data")
        .behavior
        .trees[0]
        .tree;
    assert_eq!(kept, &parse_tree(FILE, &tree).expect("edited tree"));
    assert_ne!(kept, &BehaviorSpec::default().trees[0].tree);
}

/// The whole change is JSON: a named place, an early leisure block so the test
/// is short, and one new branch in the tree.
fn add_evening_walk(world: &mut Value) {
    let mut tree = shipped_tree();
    let branches = tree["root"]["children"].as_array_mut().expect("branches");
    let leisure = branches
        .iter()
        .position(|branch| branch["name"] == "Leisure")
        .expect("leisure branch");
    branches.insert(
        leisure,
        json!({"name": "Evening walk",
        "kind": {"Guard": {"cond": "life.wants_leisure", "abort": "None"}},
        "children": [
            // Everybody has an evening since the review fix pass
            // (npc-ai#3); the walk is for the sociable ones.
            {"name": "Sociable walker",
             "kind": {"Guard": {"cond": "life.sociable", "abort": "None"}},
             "children": [
                {"name": "Walk to the pier",
                 "kind": {"Leaf": {"action": "life.go_to", "param": {"Name": "pier"}}}}
             ]}
        ]}),
    );
    world["behavior"] = json!({
        "trees": [{"file": "resident_with_walk.json", "tree": tree}],
        "places": [{"key": "pier", "position": {"x": 500, "z": -43000}}],
        "schedule": {"blocks": [
            {"kind": "work", "from": 0, "to": 700},
            {"kind": "leisure", "from": 20, "to": 300},
            {"kind": "sleep", "from": 700, "to": 1000}
        ]}
    });
}

#[test]
fn an_evening_walk_added_in_json_is_walked_by_sociable_residents() {
    let mut world = shipped_world();
    add_evening_walk(&mut world);
    let mut app = app_with(&world).expect("the edited behaviour loads");
    set_journal(app.world_mut(), true, false);
    let pier = SimPos::new(500, -43_000);
    let sociable_min = resident::spec(app.world())
        .expect("behaviour")
        .traits
        .sociable_min;
    let people = residents(app.world());
    let harbour: Vec<(SimId, Entity, bool)> = people
        .iter()
        .filter(|(_, actor)| {
            let home = app.world().get::<Villager>(*actor).expect("resident").home;
            home.dist2(pier) < 40_000_i64.pow(2)
        })
        .map(|(id, actor)| {
            let sociable = app
                .world()
                .get::<ResidentTraits>(*actor)
                .expect("traits")
                .sociability
                >= sociable_min;
            (*id, *actor, sociable)
        })
        .collect();
    assert!(harbour.iter().any(|(.., sociable)| *sociable));
    assert!(harbour.iter().any(|(.., sociable)| !*sociable));
    let mut walked = std::collections::BTreeSet::new();
    let mut arrived = std::collections::BTreeSet::new();
    let mut recorded = false;
    // Leisure runs from 2% to 30% of the 24000-tick day.
    while app.world().resource::<SimClock>().tick < 4_800 {
        let since = app.world().resource::<SimClock>().tick;
        run_ticks(app.world_mut(), 20);
        // The decision record names the authored branch. The journal is a ring
        // buffer, so read each step's entries before they can be evicted.
        recorded |= app
            .world()
            .resource::<Journal>()
            .entries
            .iter()
            .rev()
            .take_while(|entry| entry.tick >= since)
            .any(|entry| {
                matches!(&entry.kind, JournalKind::Effect { effect, applied: true }
                    if effect.get("type").and_then(Value::as_str) == Some("VillageDecision")
                        && effect.get("branch").and_then(Value::as_str)
                            == Some("Resident life > Evening walk > Sociable walker > Walk to the pier"))
            });
        for (id, actor, sociable) in &harbour {
            if village::behavior::decision(app.world(), *actor) == Some("taking a walk") {
                assert!(*sociable, "unsociable resident {} took the walk", id.0);
                walked.insert(*id);
                let here = *app.world().get::<SimPos>(*actor).expect("position");
                if here.dist2(pier) <= 1_000_i64.pow(2) {
                    arrived.insert(*id);
                }
            }
        }
    }
    println!(
        "EVENING_WALK harbour={} walked={} arrived={}",
        harbour.len(),
        walked.len(),
        arrived.len()
    );
    assert!(walked.len() >= 2, "only {} residents set out", walked.len());
    assert!(!arrived.is_empty(), "nobody reached the pier");
    assert!(recorded, "no decision record names the new branch");
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

#[test]
fn the_authoring_guide_lists_every_registered_name() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/island-ai-authoring.md");
    let guide = std::fs::read_to_string(path).expect("authoring guide");
    let names = resident::actions::ACTIONS
        .iter()
        .map(|info| info.name)
        .chain(
            resident::conditions::CONDITIONS
                .iter()
                .map(|info| info.name),
        )
        .chain(resident::conditions::SCORERS.iter().map(|(name, ..)| *name));
    for name in names {
        assert!(
            guide.contains(&format!("`{name}`")),
            "docs/island-ai-authoring.md does not document `{name}`"
        );
    }
}

fn leaf(name: &str, action: &str, param: Value) -> Value {
    json!({"name": name, "kind": {"Leaf": {"action": action, "param": param}}})
}

fn guard(name: &str, cond: &str, child: Value) -> Value {
    json!({"name": name, "kind": {"Guard": {"cond": cond, "abort": "None"}}, "children": [child]})
}

/// The shipped tree plus every action, parameter class, condition and scorer
/// the shipped tree does not use, placed where residents reach them.
fn tree_using_everything() -> Value {
    let mut tree = shipped_tree();
    let used = tree.to_string();
    // Every condition the shipped tree does not ask guards a leaf that rarely
    // starts, so all of them are evaluated at every decision that gets here.
    let probes: Vec<Value> = resident::conditions::CONDITIONS
        .iter()
        .filter(|info| !info.external && !used.contains(&format!("\"{}\"", info.name)))
        .map(|info| {
            guard(
                &format!("Probe {}", info.name),
                info.name,
                leaf("Nothing to pick up", "life.collect_found", json!("None")),
            )
        })
        .collect();
    assert!(probes.len() > 15, "{} unused conditions", probes.len());
    let mut extras = probes;
    extras.extend([
        leaf("Deliver goods", "life.sell", json!({"Name": "surplus"})),
        leaf("Deliver planks", "life.sell", json!({"Name": "plank"})),
        leaf("Carry fuel home", "life.haul", json!({"Name": "fuel"})),
        leaf("Bake", "life.craft", json!({"Name": "village_bake"})),
        leaf("Buy a tool", "life.buy", json!({"Name": "tool"})),
        leaf("Stock everything", "life.buy", json!({"Name": "reserves"})),
        guard(
            "Snack",
            "life.in_meal_window",
            leaf("Eat bread", "life.eat", json!({"Name": "bread"})),
        ),
        guard(
            "Lunch break",
            "life.in_meal_window",
            leaf("Go to work", "life.go_to", json!({"Name": "work"})),
        ),
        json!({"name": "Most urgent need",
        "kind": {"UtilitySelector": {"scorers": [
            "life.urgency_thirst", "life.urgency_hunger",
            "life.urgency_rest", "life.urgency_warmth"]}},
        "children": [
            guard("Thirsty enough", "life.thirsty",
                leaf("Drink", "life.drink", json!("None"))),
            guard("Hungry enough", "life.hungry",
                leaf("Eat", "life.eat", json!("None"))),
            guard("Tired enough", "life.tired",
                leaf("Sleep", "life.sleep", json!("None"))),
            guard("Cold enough", "life.cold",
                leaf("Warm up", "life.warm_up", json!("None")))
        ]}),
        guard(
            "Night stroll",
            "life.night",
            leaf("Walk to a point", "life.go_to", json!({"Pos": [0, -30000]})),
        ),
    ]);
    let branches = tree["root"]["children"].as_array_mut().expect("branches");
    let at = branches
        .iter()
        .position(|branch| branch["name"] == "Night")
        .expect("night branch");
    branches.insert(
        at,
        json!({"name": "Everything else", "kind": "Selector", "children": extras}),
    );
    // Every registered name is now in the tree.
    let text = tree.to_string();
    for name in resident::actions::ACTIONS
        .iter()
        .map(|info| info.name)
        .chain(
            resident::conditions::CONDITIONS
                .iter()
                .map(|info| info.name),
        )
        .chain(resident::conditions::SCORERS.iter().map(|(name, ..)| *name))
    {
        assert!(
            text.contains(&format!("\"{name}\"")),
            "{name} is not exercised"
        );
    }
    tree
}

#[test]
fn a_tree_using_every_action_condition_and_scorer_runs_and_conserves_currency() {
    let mut app = app_with(&world_with_tree(&tree_using_everything())).expect("the tree loads");
    set_journal(app.world_mut(), true, false);
    let money = |world: &World| -> u64 {
        world
            .resource::<SimIndex>()
            .map
            .values()
            .map(|entity| {
                u64::from(world.get::<village::Wallet>(*entity).map_or(0, |w| w.coins))
                    + u64::from(
                        world
                            .get::<village::CoinPurse>(*entity)
                            .map_or(0, |p| p.coins),
                    )
            })
            .sum::<u64>()
            + u64::from(world.resource::<village::VillageState>().fund)
    };
    let total = money(app.world());
    let mut seen = std::collections::BTreeSet::new();
    while app.world().resource::<SimClock>().tick < 2_400 {
        let since = app.world().resource::<SimClock>().tick;
        run_ticks(app.world_mut(), 100);
        assert_eq!(money(app.world()), total, "currency drift");
        for entry in app
            .world()
            .resource::<Journal>()
            .entries
            .iter()
            .rev()
            .take_while(|entry| entry.tick >= since)
        {
            if let JournalKind::Effect {
                effect,
                applied: true,
            } = &entry.kind
                && effect.get("type").and_then(Value::as_str) == Some("VillageDecision")
                && let Some(action) = effect.get("action").and_then(Value::as_str)
            {
                seen.insert(action.to_owned());
            }
        }
    }
    println!("EVERY_LEAF actions_chosen={seen:?}");
    for (_, actor) in residents(app.world()) {
        assert!(village::living(app.world(), actor), "a resident died");
    }
    assert!(seen.len() >= 8, "only {seen:?} were ever chosen");
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}
