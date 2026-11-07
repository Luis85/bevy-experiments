//! Combat telemetry schema (W1-27): the per-archetype maps, elite counters,
//! hazard damage and boss phase ticks on `CombatTelemetry` (schema v7 §3.12).
//! These tests stage the values as a save would, on the live attempt and on
//! the frozen report; `tests/archetype_telemetry.rs` covers the W2-05 writer.
mod support;
use horde_core::{
    command::Command,
    content::Mode,
    sim::{CombatTelemetry, Game, SAVE_VERSION, SaveError},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use support::strategic_game;

/// Every saved telemetry key this commit adds.
const NEW_KEYS: [&str; 7] = [
    "spawned_by_archetype",
    "kills_by_archetype",
    "damage_taken_by_archetype",
    "elites_spawned",
    "elites_killed",
    "hazard_damage_taken",
    "boss_phase_ticks",
];

/// A game with a frozen report (a withdrawn run) and a live Personal attempt,
/// each with `kills` set to 5.
fn telemetry_game() -> Game {
    let mut g = strategic_game();
    g.deploy(0, Mode::Personal).unwrap();
    g.apply(&Command::Withdraw).unwrap();
    // Deploying clears the report; restore it as a save made later would hold.
    let report = g.last_result().cloned();
    g.deploy(0, Mode::Personal).unwrap();
    *g.last_result_mut() = report;
    g.attempt_mut().unwrap().kills = 5;
    g.last_result_mut().as_mut().unwrap().kills = 5;
    g
}

/// The populated telemetry block both the attempt and the report carry.
fn populated() -> Value {
    json!({
        "spawned_by_archetype": {"shambler": 4, "runner": 2},
        "kills_by_archetype": {"shambler": 3, "runner": 2},
        "damage_taken_by_archetype": {"shambler": 40, "runner": 7},
        "elites_spawned": 1,
        "elites_killed": 1,
        "hazard_damage_taken": 12,
        "boss_phase_ticks": [300, 600]
    })
}

/// `telemetry_game()` with the populated block merged into the attempt's and
/// the report's telemetry.
fn staged() -> Value {
    let mut value = serde_json::to_value(telemetry_game()).unwrap();
    for owner in ["attempt", "last_result"] {
        let telemetry = value[owner]["telemetry"].as_object_mut().unwrap();
        for (key, field) in populated().as_object().unwrap() {
            telemetry.insert(key.clone(), field.clone());
        }
    }
    value
}

fn load(value: Value) -> Result<Game, SaveError> {
    Game::from_payload(value, SAVE_VERSION)
}

/// `staged()` with `field` of `owner`'s telemetry replaced by `v`.
fn with(owner: &str, field: &str, v: Value) -> Value {
    let mut value = staged();
    value[owner]["telemetry"][field] = v;
    value
}

/// `staged()` with the `extra` entries added to the `field` map of `owner`'s
/// telemetry. The staged entries stay, so spawns still cover kills and the
/// row breaks only the key rule; returns the map's new length too.
fn with_added(owner: &str, field: &str, extra: &serde_json::Map<String, Value>) -> (Value, usize) {
    let mut value = staged();
    let map = value[owner]["telemetry"][field].as_object_mut().unwrap();
    map.extend(extra.clone());
    let len = map.len();
    (value, len)
}

/// The message for a bad archetype key, too many keys or too many boss
/// phase ticks.
const KEYS_MESSAGE: &str = "Invalid saved combat telemetry archetype keys or boss phases";
/// The message for archetype kills above the owner's kills or above spawns.
const KILLS_MESSAGE: &str = "Saved combat telemetry kills exceed kills or spawns";

/// Loads every row and fails with the names of the rows that loaded or
/// failed with anything but `SaveError::Invalid(expected)`, so a row that
/// also breaks the other telemetry rule cannot pass on that rule's message.
fn assert_each_invalid(rows: Vec<(String, Value)>, expected: &str) {
    let loaded: Vec<String> = rows
        .into_iter()
        .filter_map(|(name, value)| match load(value) {
            Err(SaveError::Invalid(message)) if message == expected => None,
            Ok(_) => Some(format!("{name}: loaded")),
            Err(error) => Some(format!("{name}: {error}")),
        })
        .collect();
    assert_eq!(loaded, Vec::<String>::new(), "rows that did not fail");
}

#[test]
fn telemetry_maps_round_trip_and_skip_when_empty() {
    // Default telemetry and an unstaged game write none of the new keys.
    let default = serde_json::to_value(CombatTelemetry::default()).unwrap();
    let plain = serde_json::to_value(telemetry_game()).unwrap();
    for (name, telemetry) in [
        ("default", &default),
        ("attempt", &plain["attempt"]["telemetry"]),
        ("report", &plain["last_result"]["telemetry"]),
    ] {
        let object = telemetry.as_object().unwrap();
        let present: Vec<_> = NEW_KEYS
            .iter()
            .filter(|k| object.contains_key(**k))
            .collect();
        assert!(present.is_empty(), "{name} telemetry writes {present:?}");
    }
    // A telemetry block saved before the keys existed reads as empty.
    let legacy: CombatTelemetry = serde_json::from_value(default).unwrap();
    assert_eq!(legacy, CombatTelemetry::default());
    assert!(legacy.spawned_by_archetype.is_empty() && legacy.boss_phase_ticks.is_empty());

    // The populated block loads and re-serializes byte-identically.
    let value = staged();
    let game = load(value.clone()).unwrap_or_else(|e| panic!("staged telemetry save: {e}"));
    assert_eq!(serde_json::to_value(&game).unwrap(), value);
    for telemetry in [
        &game.attempt().unwrap().telemetry,
        &game.last_result().unwrap().telemetry,
    ] {
        assert_eq!(
            telemetry.spawned_by_archetype,
            BTreeMap::from([("runner".to_owned(), 2), ("shambler".to_owned(), 4)])
        );
        assert_eq!(
            telemetry.kills_by_archetype,
            BTreeMap::from([("runner".to_owned(), 2), ("shambler".to_owned(), 3)])
        );
        assert_eq!(
            telemetry.damage_taken_by_archetype,
            BTreeMap::from([("runner".to_owned(), 7u64), ("shambler".to_owned(), 40u64)])
        );
        assert_eq!((telemetry.elites_spawned, telemetry.elites_killed), (1, 1));
        assert_eq!(telemetry.hazard_damage_taken, 12);
        assert_eq!(telemetry.boss_phase_ticks, vec![300, 600]);
    }

    // 256 keys per map is the bound and still loads.
    let many: serde_json::Map<String, Value> = (0..256)
        .map(|i| (format!("archetype-{i}"), json!(0)))
        .collect();
    let mut value = staged();
    for field in [
        "spawned_by_archetype",
        "kills_by_archetype",
        "damage_taken_by_archetype",
    ] {
        value["attempt"]["telemetry"][field] = Value::Object(many.clone());
    }
    load(value).unwrap_or_else(|e| panic!("256 keys per map: {e}"));
}

#[test]
fn rejects_invalid_archetype_keys_and_three_boss_phase_ticks() {
    let too_many: serde_json::Map<String, Value> = (0..255)
        .map(|i| (format!("archetype-{i}"), json!(0)))
        .collect();
    let mut rows = Vec::new();
    for owner in ["attempt", "last_result"] {
        for field in [
            "spawned_by_archetype",
            "kills_by_archetype",
            "damage_taken_by_archetype",
        ] {
            // The staged two keys plus 255 more.
            let (value, len) = with_added(owner, field, &too_many);
            assert_eq!(len, 257, "{owner} {field}");
            rows.push((format!("{owner} {field} 257 keys"), value));
            for key in ["", "Shambler", "boss:horde", &"a".repeat(65)] {
                let bad = serde_json::Map::from_iter([(key.to_owned(), json!(0))]);
                rows.push((
                    format!("{owner} {field} key {key:?}"),
                    with_added(owner, field, &bad).0,
                ));
            }
        }
        rows.push((
            format!("{owner} three boss phase ticks"),
            with(owner, "boss_phase_ticks", json!([100, 200, 300])),
        ));
    }
    assert_each_invalid(rows, KEYS_MESSAGE);
}

#[test]
fn rejects_kills_by_archetype_sum_exceeding_kills() {
    // The sum may equal kills (5 = 3 + 2 in the staged block).
    load(staged()).unwrap_or_else(|e| panic!("sum equal to kills: {e}"));
    let mut rows = Vec::new();
    for owner in ["attempt", "last_result"] {
        // Each archetype stays within its own spawns; only the sum is over.
        let mut value = with(
            owner,
            "spawned_by_archetype",
            json!({"shambler": 4, "runner": 3}),
        );
        value[owner]["telemetry"]["kills_by_archetype"] = json!({"shambler": 3, "runner": 3});
        rows.push((format!("{owner} sum 6 over kills 5"), value));
        let mut value = staged();
        value[owner]["kills"] = json!(4);
        rows.push((format!("{owner} kills 4 under sum 5"), value));
    }
    assert_each_invalid(rows, KILLS_MESSAGE);
}

#[test]
fn rejects_archetype_kills_above_spawned() {
    let mut rows = Vec::new();
    for owner in ["attempt", "last_result"] {
        rows.push((
            format!("{owner} shambler kills above spawns"),
            with(
                owner,
                "spawned_by_archetype",
                json!({"shambler": 2, "runner": 2}),
            ),
        ));
        rows.push((
            format!("{owner} runner kills with no spawn entry"),
            with(owner, "spawned_by_archetype", json!({"shambler": 4})),
        ));
    }
    assert_each_invalid(rows, KILLS_MESSAGE);
}
