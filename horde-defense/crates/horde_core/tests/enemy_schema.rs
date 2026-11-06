//! Enemy-side schema at neutral values (W1-28, schema v7 §2, §3.4-3.5,
//! §3.8-3.10): the new `ResolvedEnemy` fields, `Enemy.split_child` and
//! `boss_state`, the enemy `ProgressionSnapshot` fields, the RunRecord and
//! Report boss fields, the wave-tier bound and the gameplay enemy cap. Nothing
//! writes a non-neutral value yet (enemy-balance, wave 2), so these tests
//! stage the values as a save would.
mod support;
use horde_core::{
    command::Command,
    content::Mode,
    enemy_catalog::{ResolvedEnemy, enemy_catalog},
    progression::{ProgressionSnapshot, catalog},
    sim::{Game, SAVE_VERSION, SaveError},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Every key a legacy `ResolvedEnemy` snapshot writes.
const RESOLVED_KEYS: [&str; 22] = [
    "archetype_id",
    "kind",
    "name",
    "health",
    "speed",
    "damage",
    "armor",
    "windup",
    "reward",
    "xp",
    "npc_hp",
    "npc_ht",
    "npc_skill",
    "npc_dodge",
    "npc_armor",
    "npc_damage_dice",
    "npc_damage_add",
    "structure_damage_multiplier",
    "contact_armor_fraction",
    "target_urgency",
    "visual",
    "unlocked_by",
];
/// Every key a legacy `ProgressionSnapshot` writes.
const SNAPSHOT_KEYS: [&str; 17] = [
    "enemy_cap",
    "rank",
    "batch_size",
    "item_level",
    "enemy_health",
    "enemy_damage",
    "enemy_speed",
    "arrival_multiplier",
    "reward_multiplier",
    "fortification_multiplier",
    "supply_bonus",
    "start_rate_floor",
    "rate_cap_floor",
    "boss_time_floor",
    "boss_health_floor",
    "npc_boss_health_floor",
    "wave_tier",
];
/// Every saved key this commit adds, quoted as it would appear in a save.
const NEW_KEYS: [&str; 21] = [
    // ResolvedEnemy
    "\"reach\":",
    "\"ranged_armor\":",
    "\"npc_ranged_armor\":",
    "\"stagger_immune\":",
    "\"group_size\":",
    "\"death_effect\":",
    "\"tags\":",
    "\"elite\":",
    // Enemy
    "\"split_child\":",
    "\"boss_state\":",
    // ProgressionSnapshot
    "\"elite_rate_bp\":",
    "\"front_ramp\":",
    "\"boss_phases\":",
    "\"boss_armor_multiplier\":",
    "\"npc_boss_armor_add\":",
    "\"boss_health_multiplier\":",
    "\"boss_reward_multiplier\":",
    "\"boss_damage_multiplier\":",
    // RunRecord and Report (boss_rank is shared with the snapshot)
    "\"boss_rank\":",
    "\"elite_kills\":",
    "\"boss_ttk_ticks\":",
];

/// `value` as saved text parses back (f32 values print shortest).
fn as_saved<T: Serialize>(value: &T) -> Value {
    serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap()
}

fn parse<T: DeserializeOwned>(value: &Value) -> Result<T, String> {
    serde_json::from_value(value.clone()).map_err(|e| e.to_string())
}

fn load(value: Value) -> Result<Game, SaveError> {
    Game::from_payload(value, SAVE_VERSION)
}

fn keys(value: &Value) -> BTreeSet<String> {
    value.as_object().unwrap().keys().cloned().collect()
}

fn shambler() -> Value {
    as_saved(&enemy_catalog().resolve("shambler").unwrap())
}

/// The shambler snapshot with every new field at a non-neutral value.
fn populated_enemy() -> Value {
    let mut value = shambler();
    let object = value.as_object_mut().unwrap();
    for (key, field) in json!({
        "reach": 4.0,
        "ranged_armor": 14,
        "npc_ranged_armor": 4,
        "stagger_immune": true,
        "group_size": 4,
        "death_effect": {"Burst": {
            "radius": 1.6, "damage": 30, "structure_damage": 40,
            "npc_dice": 3, "npc_add": 0, "fuse": true
        }},
        "tags": ["volatile", "hazard"],
        "elite": {"affix_id": "hardened", "name": "Hardened Shambler"}
    })
    .as_object()
    .unwrap()
    {
        object.insert(key.clone(), field.clone());
    }
    value
}

fn split(archetype_id: &str, count: u8) -> Value {
    json!({"Split": {"archetype_id": archetype_id, "count": count}})
}

#[test]
fn resolved_enemy_with_reach_elite_and_death_effect_round_trips() {
    for death_effect in [None, Some(split("splitter-brood", 3))] {
        let mut value = populated_enemy();
        if let Some(effect) = death_effect {
            value["death_effect"] = effect;
        }
        let enemy: ResolvedEnemy = parse(&value).expect("the new fields parse");
        enemy.validate().expect("in-bounds values validate");
        assert_eq!(as_saved(&enemy), value, "round trip");
    }
    // A save written by a later build with an unknown field is still refused.
    let mut unknown = populated_enemy();
    unknown["unknown_field"] = 1.into();
    assert!(parse::<ResolvedEnemy>(&unknown).is_err());
}

#[test]
fn resolved_enemy_validate_rejects_reach_13_group_7_split_count_0_empty_elite_name() {
    let base: ResolvedEnemy = parse(&populated_enemy()).unwrap();
    base.validate().unwrap();
    let long_name = "x".repeat(61);
    let nine_tags: Vec<String> = (0..9).map(|n| format!("tag-{n}")).collect();
    let rows: Vec<(&str, &str, Value)> = vec![
        ("reach 13", "/reach", json!(13.0)),
        ("reach negative", "/reach", json!(-0.5)),
        ("ranged armor 1001", "/ranged_armor", json!(1001)),
        ("ranged armor negative", "/ranged_armor", json!(-1)),
        ("npc ranged armor 101", "/npc_ranged_armor", json!(101)),
        ("npc ranged armor negative", "/npc_ranged_armor", json!(-1)),
        ("group 0", "/group_size", json!(0)),
        ("group 7", "/group_size", json!(7)),
        ("burst radius 0", "/death_effect/Burst/radius", json!(0.0)),
        ("burst radius 3.5", "/death_effect/Burst/radius", json!(3.5)),
        ("burst damage 0", "/death_effect/Burst/damage", json!(0)),
        (
            "burst damage 10001",
            "/death_effect/Burst/damage",
            json!(10_001),
        ),
        (
            "burst structure damage -1",
            "/death_effect/Burst/structure_damage",
            json!(-1),
        ),
        (
            "burst structure damage 10001",
            "/death_effect/Burst/structure_damage",
            json!(10_001),
        ),
        ("burst dice 0", "/death_effect/Burst/npc_dice", json!(0)),
        ("burst dice 21", "/death_effect/Burst/npc_dice", json!(21)),
        ("burst add -31", "/death_effect/Burst/npc_add", json!(-31)),
        ("burst add 101", "/death_effect/Burst/npc_add", json!(101)),
        ("split count 0", "/death_effect", split("splitter-brood", 0)),
        ("split count 5", "/death_effect", split("splitter-brood", 5)),
        ("split bad id", "/death_effect", split("Brood Husk", 2)),
        ("split empty id", "/death_effect", split("", 2)),
        ("nine tags", "/tags", json!(nine_tags)),
        ("duplicate tag", "/tags", json!(["swarm", "swarm"])),
        ("invalid tag", "/tags", json!(["Swarm"])),
        ("empty elite name", "/elite/name", json!("")),
        ("blank elite name", "/elite/name", json!("   ")),
        ("elite name 61", "/elite/name", json!(long_name)),
        ("invalid affix id", "/elite/affix_id", json!("Hardened!")),
    ];
    let passed: Vec<String> = rows
        .into_iter()
        .filter_map(|(row, pointer, field)| {
            let mut value = populated_enemy();
            *value.pointer_mut(pointer).unwrap() = field;
            match parse::<ResolvedEnemy>(&value) {
                Err(error) => Some(format!("{row}: did not parse: {error}")),
                Ok(enemy) => enemy
                    .validate()
                    .is_ok()
                    .then(|| format!("{row}: validated")),
            }
        })
        .collect();
    assert_eq!(passed, Vec::<String>::new(), "rows that did not fail");
    // The inclusive bounds validate.
    let edges: Vec<(&str, Value)> = vec![
        ("/reach", json!(12.0)),
        ("/ranged_armor", json!(1000)),
        ("/npc_ranged_armor", json!(100)),
        ("/group_size", json!(6)),
        ("/group_size", json!(1)),
        ("/death_effect/Burst/radius", json!(3.0)),
        ("/death_effect/Burst/npc_dice", json!(20)),
        ("/death_effect/Burst/npc_add", json!(-30)),
        ("/death_effect", split("splitter-brood", 4)),
        ("/death_effect", split("unknown-but-valid-id", 1)),
        ("/tags", json!(["a", "b", "c", "d", "e", "f", "g", "h"])),
        ("/elite/name", json!("x".repeat(60))),
        ("/elite/affix_id", json!("an-affix-no-catalogue-knows")),
    ];
    for (pointer, field) in edges {
        let mut value = populated_enemy();
        *value.pointer_mut(pointer).unwrap() = field.clone();
        let enemy: ResolvedEnemy = parse(&value).unwrap();
        assert!(enemy.validate().is_ok(), "{pointer} = {field} rejected");
    }
}

/// The shipped rank-0 snapshot as saved text.
fn snapshot() -> Value {
    as_saved(&catalog().snapshot(0))
}

fn last_floor_tier() -> usize {
    catalog().pressure_floors.last().unwrap().wave_tier
}

/// The shipped catalogue with one more pressure floor at `tier`.
fn with_floor_tier(tier: usize) -> horde_core::progression::ProgressionCatalog {
    let mut authored = catalog().clone();
    let mut floor = authored.pressure_floors.last().unwrap().clone();
    floor.wave_tier = tier;
    authored.pressure_floors.push(floor);
    authored
}

#[test]
fn progression_snapshot_accepts_wave_tier_up_to_max_wave_tier() {
    for tier in 0..=15 {
        let mut value = snapshot();
        value["wave_tier"] = tier.into();
        let snapshot: ProgressionSnapshot = parse(&value).unwrap();
        assert_eq!(snapshot.validate(), Ok(()), "tier {tier}");
    }
    // The authored floors accept the same tiers (appended, so they stay monotone).
    for tier in last_floor_tier()..=15 {
        assert_eq!(
            with_floor_tier(tier).validate(),
            Ok(()),
            "floor tier {tier}"
        );
    }
}

#[test]
fn progression_snapshot_rejects_wave_tier_above_max() {
    for tier in [16usize, 17, 63, 64, 1000] {
        let mut value = snapshot();
        value["wave_tier"] = tier.into();
        let snapshot: ProgressionSnapshot = parse(&value).unwrap();
        assert!(snapshot.validate().is_err(), "tier {tier} validated");
        assert!(
            with_floor_tier(tier).validate().is_err(),
            "floor tier {tier} validated"
        );
    }
}

/// The rank-0 snapshot with every new field at a non-neutral value.
fn populated_snapshot() -> Value {
    let mut value = snapshot();
    let object = value.as_object_mut().unwrap();
    for (key, field) in json!({
        "rank": 40,
        "enemy_cap": 145,
        "elite_rate_bp": 1500,
        "front_ramp": 1.18,
        "boss_rank": "Capstone",
        "boss_phases": 2,
        "boss_armor_multiplier": 2.15,
        "npc_boss_armor_add": 6,
        "boss_health_multiplier": 1.4,
        "boss_reward_multiplier": 1.4,
        "boss_damage_multiplier": 1.5,
        "wave_tier": 11
    })
    .as_object()
    .unwrap()
    {
        object.insert(key.clone(), field.clone());
    }
    value
}

#[test]
fn progression_snapshot_enemy_fields_round_trip_and_validate_bounds() {
    let value = populated_snapshot();
    let snapshot: ProgressionSnapshot = parse(&value).unwrap();
    assert_eq!(snapshot.validate(), Ok(()));
    assert_eq!(as_saved(&snapshot), value, "round trip");
    let rows: Vec<(&str, Value)> = vec![
        ("elite_rate_bp", json!(10_001)),
        ("front_ramp", json!(0.99)),
        ("front_ramp", json!(2.01)),
        ("boss_phases", json!(3)),
        ("boss_armor_multiplier", json!(0.99)),
        ("boss_armor_multiplier", json!(4.01)),
        ("npc_boss_armor_add", json!(-1)),
        ("npc_boss_armor_add", json!(21)),
        ("boss_health_multiplier", json!(0.99)),
        ("boss_health_multiplier", json!(2.01)),
        ("boss_reward_multiplier", json!(0.99)),
        ("boss_reward_multiplier", json!(2.01)),
        ("boss_damage_multiplier", json!(0.99)),
        ("boss_damage_multiplier", json!(4.01)),
    ];
    let passed: Vec<String> = rows
        .into_iter()
        .filter_map(|(key, field)| {
            let mut value = populated_snapshot();
            value[key] = field.clone();
            let snapshot: ProgressionSnapshot = parse(&value).unwrap();
            snapshot
                .validate()
                .is_ok()
                .then(|| format!("{key} = {field}: validated"))
        })
        .collect();
    assert_eq!(passed, Vec::<String>::new(), "rows that did not fail");
}

/// A deployed Personal game with a live regular enemy and a live boss.
fn live_battle() -> Game {
    let mut g = support::game(Mode::Personal);
    stage_enemies(&mut g);
    g
}

/// Spawn a regular enemy and the boss into `g`'s live attempt.
fn stage_enemies(g: &mut Game) {
    support::enemy(g, 3, 12.0, false);
    support::enemy(g, 5, 14.0, true);
    let a = g.attempt_mut().unwrap();
    a.boss_spawned = true;
    a.telemetry.boss_arrival_tick = Some(a.tick);
}

#[test]
fn legacy_resolved_enemy_and_snapshot_serialize_byte_identically() {
    let expected: BTreeSet<String> = RESOLVED_KEYS.into_iter().map(String::from).collect();
    for id in enemy_catalog().archetype_ids() {
        let enemy = enemy_catalog().resolve(id).unwrap();
        let text = serde_json::to_string(&enemy).unwrap();
        assert_eq!(
            keys(&serde_json::from_str(&text).unwrap()),
            expected,
            "{id}"
        );
        let restored: ResolvedEnemy = serde_json::from_str(&text).unwrap();
        assert_eq!(serde_json::to_string(&restored).unwrap(), text, "{id}");
    }
    let expected: BTreeSet<String> = SNAPSHOT_KEYS.into_iter().map(String::from).collect();
    for rank in 0..=60 {
        let text = serde_json::to_string(&catalog().snapshot(rank)).unwrap();
        assert_eq!(
            keys(&serde_json::from_str(&text).unwrap()),
            expected,
            "{rank}"
        );
        let restored: ProgressionSnapshot = serde_json::from_str(&text).unwrap();
        assert_eq!(serde_json::to_string(&restored).unwrap(), text, "{rank}");
    }
    // A whole game with live enemies, a boss, a report and a run record
    // writes none of the new keys and reloads byte-identically.
    let mut g = live_battle();
    support::advance(&mut g, 120);
    g.apply(&Command::Withdraw).unwrap();
    let report = g.last_result().cloned();
    g.deploy(0, Mode::Personal).unwrap();
    stage_enemies(&mut g);
    *g.last_result_mut() = report;
    assert!(!g.campaign().run_history.is_empty());
    let text = serde_json::to_string(&g).unwrap();
    let found: Vec<&str> = NEW_KEYS.into_iter().filter(|k| text.contains(k)).collect();
    assert_eq!(found, Vec::<&str>::new(), "new keys in a legacy game");
    let reloaded = load(serde_json::from_str(&text).unwrap()).unwrap();
    assert_eq!(serde_json::to_string(&reloaded).unwrap(), text);
}

#[test]
fn production_validation_rejects_enemy_cap_225() {
    let mut value = snapshot();
    value["rank"] = 40.into();
    for (cap, ok) in [(224, true), (225, false), (399, false), (400, false)] {
        value["enemy_cap"] = cap.into();
        let snapshot: ProgressionSnapshot = parse(&value).unwrap();
        assert_eq!(snapshot.validate().is_ok(), ok, "snapshot cap {cap}");
        let mut authored = catalog().clone();
        authored.population_global_cap = cap;
        authored.population_cap = cap.min(authored.population_cap);
        assert_eq!(authored.validate().is_ok(), ok, "catalogue cap {cap}");
    }
    // A saved attempt carrying the staged stress cap never loads.
    let mut saved = as_saved(&live_battle());
    saved["attempt"]["progression"]["enemy_cap"] = 225.into();
    assert!(load(saved).is_err(), "a 225 cap loaded");
}

/// The first run record and the frozen report of a withdrawn run, as saved.
fn finished() -> Value {
    let mut g = live_battle();
    support::advance(&mut g, 60);
    g.apply(&Command::Withdraw).unwrap();
    as_saved(&g)
}

#[test]
fn run_record_without_new_fields_deserializes() {
    let legacy = finished();
    let record = &legacy["campaign"]["run_history"][0];
    for key in ["elite_kills", "boss_ttk_ticks", "boss_rank"] {
        assert!(record.get(key).is_none(), "{key} written at neutral");
        assert!(legacy["last_result"].get(key).is_none(), "{key} on report");
    }
    let game = load(legacy.clone()).expect("a legacy run record loads");
    assert_eq!(as_saved(&game), legacy);

    // Populated values round trip on the record and the report.
    let kills = record["kills"].as_u64().unwrap().max(3);
    let duration = record["duration_ticks"].as_u64().unwrap();
    assert!(duration > 0);
    let mut populated = legacy.clone();
    let record = &mut populated["campaign"]["run_history"][0];
    record["kills"] = kills.into();
    record["elite_kills"] = kills.into();
    record["boss_ttk_ticks"] = duration.into();
    record["boss_rank"] = "Regional".into();
    populated["last_result"]["boss_rank"] = "Capstone".into();
    let game = load(populated.clone()).expect("populated boss fields load");
    assert_eq!(as_saved(&game), populated, "round trip");

    // elite_kills <= kills and boss_ttk_ticks <= duration_ticks.
    for (key, field) in [
        ("elite_kills", json!(kills + 1)),
        ("boss_ttk_ticks", json!(duration + 1)),
    ] {
        let mut value = populated.clone();
        value["campaign"]["run_history"][0][key] = field;
        assert!(load(value).is_err(), "{key} out of bounds loaded");
    }
}

/// `live_battle()` as saved, with `edit` applied to the enemy at `index`
/// (0 is the regular enemy, 1 the boss).
fn with_enemy(index: usize, edit: impl FnOnce(&mut Value)) -> Value {
    let mut value = as_saved(&live_battle());
    edit(&mut value["attempt"]["enemies"][index]);
    value
}

fn boss_state(transitions: u8, entered: u8) -> Value {
    json!({
        "profile": {
            "name": "Breach", "speed_mult": 1.4, "armor_mult": 0.6,
            "call_interval_mult": 1.0, "siege_recovery_mult": 1.0, "hold_ticks": 90
        },
        "transitions": transitions,
        "entered": entered,
        "hold_until": if entered == 0 { 0 } else { 90 }
    })
}

/// `live_battle()` as saved, its boss holding `state` until `hold_until`.
fn boss_holding(mut state: Value, hold_until: Value) -> Value {
    state["hold_until"] = hold_until;
    with_enemy(1, |e| e["boss_state"] = state)
}

#[test]
fn enemy_split_child_and_boss_state_round_trip_and_validate() {
    let populated = {
        let mut value = with_enemy(0, |e| e["split_child"] = true.into());
        value["attempt"]["enemies"][1]["boss_state"] = boss_state(2, 1);
        value
    };
    let game = load(populated.clone()).expect("a split child and a phased boss load");
    assert_eq!(as_saved(&game), populated, "round trip");
    // A hold ends at most `hold_ticks` after the save tick, and a boss that
    // entered no transition holds nothing.
    let tick = populated["attempt"]["tick"].as_u64().unwrap();
    load(boss_holding(boss_state(2, 1), json!(tick + 90)))
        .expect("a hold ending hold_ticks after the save tick loads");
    load(boss_holding(boss_state(2, 0), json!(0))).expect("a boss before its first transition");

    let rows: Vec<(&str, Value)> = vec![
        (
            "boss split child",
            with_enemy(1, |e| e["split_child"] = true.into()),
        ),
        (
            "elite split child",
            with_enemy(0, |e| {
                e["split_child"] = true.into();
                e["resolved"]["elite"] = json!({"affix_id": "hardened", "name": "Hardened"});
            }),
        ),
        (
            "splitting split child",
            with_enemy(0, |e| {
                e["split_child"] = true.into();
                e["resolved"]["death_effect"] = split("splitter-brood", 2);
            }),
        ),
        (
            "regular enemy boss state",
            with_enemy(0, |e| e["boss_state"] = boss_state(1, 0)),
        ),
        (
            "three transitions",
            with_enemy(1, |e| e["boss_state"] = boss_state(3, 0)),
        ),
        (
            "entered above transitions",
            with_enemy(1, |e| e["boss_state"] = boss_state(1, 2)),
        ),
        (
            "hold past the save tick",
            boss_holding(boss_state(2, 1), json!(tick + 91)),
        ),
        (
            "unbounded hold",
            boss_holding(boss_state(2, 1), json!(u64::MAX)),
        ),
        (
            "hold before any transition",
            boss_holding(boss_state(2, 0), json!(1)),
        ),
    ];
    let mut rows = rows;
    for (field, bad) in [
        ("name", json!("")),
        ("name", json!("x".repeat(41))),
        ("speed_mult", json!(0.99)),
        ("speed_mult", json!(2.01)),
        ("armor_mult", json!(0.24)),
        ("armor_mult", json!(1.01)),
        ("call_interval_mult", json!(0.29)),
        ("call_interval_mult", json!(1.01)),
        ("siege_recovery_mult", json!(0.29)),
        ("siege_recovery_mult", json!(1.01)),
        ("hold_ticks", json!(181)),
    ] {
        let mut state = boss_state(2, 1);
        state["profile"][field] = bad;
        rows.push((field, with_enemy(1, |e| e["boss_state"] = state)));
    }
    let passed: Vec<String> = rows
        .into_iter()
        .filter_map(|(row, value)| load(value).is_ok().then(|| format!("{row}: loaded")))
        .collect();
    assert_eq!(passed, Vec::<String>::new(), "rows that did not fail");
}
