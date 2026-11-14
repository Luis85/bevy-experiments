//! An island save says which version of the island made it, and this build
//! answers in types: current, older, newer, unreadable; only current loads.
use island_content::village::island_save::{self, IslandSaveCheck};
use npc_framework::prelude::*;
use serde_json::Value;

fn island() -> bevy_app::App {
    island_content::new_sim_app(7, "living_world")
}

#[test]
fn a_save_of_this_build_is_current_and_loads() {
    let mut app = island();
    run_ticks(app.world_mut(), 20);
    let snap = island_save::save_island(app.world_mut()).expect("saved");
    assert_eq!(
        island_save::saved_version(&snap),
        Some(island_save::ISLAND_SAVE_VERSION)
    );
    let mut fresh = island();
    assert_eq!(
        island_save::check_island_save(fresh.world_mut(), &snap),
        IslandSaveCheck::Current
    );
    island_save::load_island_save(fresh.world_mut(), &snap).expect("loads as saved");
}

#[test]
fn an_older_or_newer_save_is_named_and_refused() {
    let mut app = island();
    run_ticks(app.world_mut(), 5);
    let mut snap = island_save::save_island(app.world_mut()).expect("saved");
    // As a later build sees this save: its content hash differs.
    snap.content_hash ^= 1;
    let current = island_save::ISLAND_SAVE_VERSION + 1;
    let mut fresh = island();
    let older = island_save::check_against(fresh.world_mut(), &snap, current);
    assert_eq!(
        older,
        IslandSaveCheck::OlderIsland {
            saved: Some(island_save::ISLAND_SAVE_VERSION),
            current,
        }
    );
    assert!(older.sentence().contains("older version of the island"));
    // A save without a version stamp is unreadable (review fix S1.2: it
    // used to count as older; a missing stamp now refuses on its own).
    snap.header.remove(island_save::HEADER_VERSION);
    assert_eq!(
        island_save::check_island_save(fresh.world_mut(), &snap),
        IslandSaveCheck::Unreadable {
            reason: "This save has no readable island version.".into()
        }
    );
    assert!(island_save::load_island_save(fresh.world_mut(), &snap).is_err());
    // A save stamped by a newer island.
    snap.header.insert(
        island_save::HEADER_VERSION.into(),
        serde_json::Value::from(island_save::ISLAND_SAVE_VERSION + 3),
    );
    assert!(matches!(
        island_save::check_island_save(fresh.world_mut(), &snap),
        IslandSaveCheck::NewerIsland { .. }
    ));
}

fn saved() -> (bevy_app::App, npc_framework::snapshot::WorldSnapshot) {
    let mut app = island();
    run_ticks(app.world_mut(), 10);
    let snap = island_save::save_island(app.world_mut()).expect("saved");
    (app, snap)
}

#[test]
fn save_with_text_stamp_is_unreadable() {
    let (_, mut snap) = saved();
    snap.header
        .insert(island_save::HEADER_VERSION.into(), Value::from("three"));
    let mut fresh = island();
    assert_eq!(
        island_save::check_island_save(fresh.world_mut(), &snap),
        IslandSaveCheck::Unreadable {
            reason: "This save has no readable island version.".into()
        }
    );
    let (_, mut snap) = saved();
    snap.header
        .insert(island_save::HEADER_CONTENT.into(), Value::from("not hex"));
    assert_eq!(
        island_save::check_island_save(fresh.world_mut(), &snap),
        IslandSaveCheck::Unreadable {
            reason: "This save has no readable content stamp.".into()
        }
    );
}

#[test]
fn save_from_other_names_is_other_build() {
    let (_, mut snap) = saved();
    let other = format!("{:016x}", snap.content_hash ^ 1);
    snap.header
        .insert(island_save::HEADER_CONTENT.into(), Value::from(other));
    let mut fresh = island();
    let check = island_save::check_island_save(fresh.world_mut(), &snap);
    assert_eq!(check, IslandSaveCheck::OtherBuild);
    assert!(check.sentence().contains("different build"));
    assert!(island_save::load_island_save(fresh.world_mut(), &snap).is_err());
}

#[test]
fn state_hash_mismatch_is_refused() {
    let (_, mut snap) = saved();
    snap.header.insert(
        island_save::HEADER_STATE.into(),
        Value::from("0123456789abcdef"),
    );
    let mut fresh = island();
    assert_eq!(
        island_save::load_island_save(fresh.world_mut(), &snap).err(),
        Some(IslandSaveCheck::Unreadable {
            reason: "This save does not match the world it was made in.".into()
        })
    );
}

fn app_from(preset: &island_content::worlds::WorldPreset) -> bevy_app::App {
    let doc = island_content::construction::from_preset(preset).expect("construction");
    island_content::scenario::load_scenario(&doc, 7)
        .map(|loaded| loaded.app)
        .unwrap_or_else(|error| panic!("{error}"))
}

/// The world rules travel with the save (review fix S1.3): Continue builds
/// from the saved rules, never from an edited world file, and a save loaded
/// into other rules is refused.
#[test]
fn continue_uses_the_saved_rules_and_refuses_others() {
    let (mut app, snap) = saved();
    let rules = island_save::saved_rules(&snap).expect("rules in the save");
    // An edited world file: bread costs more.
    let mut edited = rules.clone();
    for good in edited
        .living_world
        .as_mut()
        .expect("living world")
        .economy
        .goods
        .iter_mut()
    {
        if good.key == "bread" {
            good.base_price += 1;
        }
    }
    let mut other = app_from(&edited);
    assert_eq!(
        island_save::load_island_save(other.world_mut(), &snap).err(),
        Some(IslandSaveCheck::Unreadable {
            reason: "This save does not match the world it was made in.".into()
        })
    );
    // Built from the saved rules, it continues exactly.
    let mut again = app_from(&rules);
    island_save::load_island_save(again.world_mut(), &snap).expect("continues");
    for _ in 0..6 {
        run_ticks(app.world_mut(), 100);
        run_ticks(again.world_mut(), 100);
        assert_eq!(state_hash(app.world_mut()), state_hash(again.world_mut()));
    }
}

/// The first JSON path where `a` and `b` differ.
fn first_difference(a: &Value, b: &Value, path: String) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter()
                .find_map(|key| match (x.get(key), y.get(key)) {
                    (Some(p), Some(q)) => first_difference(p, q, format!("{path}.{key}")),
                    _ => Some(format!("{path}.{key} (only on one side)")),
                })
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (p, q))| first_difference(p, q, format!("{path}[{i}]"))),
        _ => (a != b).then(|| format!("{path}: {a} vs {b}")),
    }
}

/// A world exported from a running island is a world the game can start:
/// its save continues in the same build (built from the saved rules).
#[test]
fn a_save_of_an_exported_world_continues() {
    let mut source = island();
    run_ticks(source.world_mut(), 5);
    let exported = island_content::village::frontier::export_spec(source.world()).expect("export");
    let mut preset = island_content::worlds::builtin_preset("living_world").expect("preset");
    preset.living_world = Some(exported);
    let mut app = app_from(&preset);
    run_ticks(app.world_mut(), 20);
    let snap = island_save::save_island(app.world_mut()).expect("saved");
    let rules = island_save::saved_rules(&snap).expect("rules");
    let mut again = app_from(&rules);
    let built = serde_json::to_value(
        &again
            .world()
            .resource::<island_content::world_runtime::ActiveWorld>()
            .0,
    )
    .expect("json");
    let saved = serde_json::to_value(&rules).expect("json");
    if let Err(check) = island_save::load_island_save(again.world_mut(), &snap) {
        panic!(
            "{check:?}; rules differ at {:?}",
            first_difference(&saved, &built, String::new())
        );
    }
    run_ticks(app.world_mut(), 50);
    run_ticks(again.world_mut(), 50);
    assert_eq!(state_hash(app.world_mut()), state_hash(again.world_mut()));
}

/// New Game on a chosen world, as the game does it: the window's world was
/// built from the shipped rules and the chosen world is loaded into it from
/// a snapshot. The world's construction travels in that snapshot, so the save
/// made afterwards carries it and Continue (built from the saved rules)
/// loads it.
#[test]
fn a_chosen_world_loaded_into_a_running_window_saves_and_continues() {
    let mut source = island();
    run_ticks(source.world_mut(), 5);
    let mut exported =
        island_content::village::frontier::export_spec(source.world()).expect("export");
    // A shaped world: its first settlement is somewhere else.
    exported.settlements[0].label.push_str(" (shaped)");
    let mut preset = island_content::worlds::builtin_preset("living_world").expect("preset");
    preset.living_world = Some(exported);
    let text = serde_json::to_string(&preset).expect("json");
    let preset: island_content::worlds::WorldPreset = serde_json::from_str(&text).expect("parsed");
    let mut staged = app_from(&preset);
    let prepared = npc_framework::snapshot::save(staged.world_mut()).expect("prepared");
    // The running window, built from the shipped rules.
    let mut window = island();
    npc_framework::snapshot::check(window.world_mut(), &prepared).expect("same build");
    npc_framework::snapshot::load(
        window.world_mut(),
        &prepared,
        npc_framework::snapshot::LoadMode::Strict,
    )
    .expect("replaced");
    window.insert_resource(island_content::world_runtime::ActiveWorld(preset.clone()));
    assert_eq!(
        island_content::village::frontier::spec(window.world()),
        island_content::village::frontier::spec(staged.world()),
        "the window plays the chosen world's construction"
    );
    run_ticks(window.world_mut(), 20);
    let snap = island_save::save_island(window.world_mut()).expect("saved");
    let rules = island_save::saved_rules(&snap).expect("rules");
    let mut again = app_from(&rules);
    island_save::load_island_save(again.world_mut(), &snap).expect("continues");
    run_ticks(window.world_mut(), 50);
    run_ticks(again.world_mut(), 50);
    assert_eq!(
        state_hash(window.world_mut()),
        state_hash(again.world_mut())
    );
}
