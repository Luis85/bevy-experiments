//! Save format 7 schema guards (W1-30; schema-v7.md sections 1 and 3, risk 1
//! of the save-determinism design).
//!
//! Every field save format 7 added is serde-defaulted and skipped at its
//! default (convention C1), so:
//! - the v6 fixture, migrated, decodes into the v7 types and re-encodes to the
//!   very same JSON (no field lost, renamed or written at its default);
//! - the default game and every GOLDEN run write no v7 key, so the v7 schema
//!   fields move no digest (the keys a wave-2 writer populates, listed with
//!   their commit, are the only exception);
//! - every v7 field keeps a non-default value through a save round trip;
//! - one v6-shaped object per touched type decodes and re-encodes unchanged;
//! - `support::V7_SAVED_KEYS` lists exactly the fields the types serialize
//!   beyond the v6 shape. The field lists come from the types themselves (the
//!   names serde derive hands to `deserialize_struct`), so a new field that is
//!   not in the registry fails here even before anything writes it;
//! - every struct the v6 fixture holds has those field lists checked, not
//!   only the types section 3 touched.
mod support;
use horde_core::{
    balance::BalanceSettings,
    campaign::{Campaign, DefenderProfile, Item, Npc, NpcCreation, Player, Preset},
    command::Command,
    content::{BossTiming, Mode, NpcProfile, PersonalProfile},
    enemy_catalog::{EnemyVisual, ResolvedEnemy},
    ordnance::OrdnanceState,
    progression::ProgressionSnapshot,
    quality::LootEvent,
    rules::{InjuryState, Rng, Stats},
    sim::{
        Attempt, CombatTelemetry, CorpseRecord, Enemy, Event, Game, ParallelBattle, Report,
        Structure,
    },
    skill_catalog::SkillModifiers,
    stats::{DamageBucket, DamageTotals, DpsTracker, GearRecord, RunRecord},
    strategy::{GuardAssignment, Recruit, StrategyState},
    survivors::{EquipmentVisual, EquipmentVisualItem},
    weapons::WeaponSpec,
    world::{Modifiers, WorldProgress},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use support::{V7_SAVED_KEYS, WORKER_GAME_PREFIX, game_key_paths, values_at_key_path};

const FIXTURE_V6: &str = include_str!("fixtures/save_v6.json");

/// The GOLDEN runs of `tests/determinism.rs`: 1800 smoke ticks per seed, in
/// both modes.
const GOLDEN_SEEDS: [u64; 3] = [1, 0x1A57_1167, 0xDEAD_BEEF_2026];
const GOLDEN_TICKS: u32 = 1800;

/// The raw payload of the v6 fixture, exactly as the v6 build wrote it.
fn v6_payload() -> Value {
    let envelope: Value = serde_json::from_str(FIXTURE_V6).unwrap();
    serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap()
}

/// The v6 payload after the raw-JSON migrations, before any typed decode.
fn migrated_v6() -> Value {
    Game::migrate_payload(v6_payload(), 6).expect("the v6 fixture migrates")
}

/// `value` written as save text and parsed back (f32 fields print shortest,
/// exactly as a save holds them).
fn as_saved<T: Serialize>(value: &T) -> Value {
    serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap()
}

/// Decode `value` as `T` and encode it again as save text.
fn round_trip<T: Serialize + DeserializeOwned>(value: &Value) -> Result<Value, String> {
    serde_json::from_value::<T>(value.clone())
        .map(|decoded| as_saved(&decoded))
        .map_err(|error| error.to_string())
}

/// The key paths where `left` and `right` differ (keys present on one side
/// only, or leaves with different values), for readable failures.
fn differences(left: &Value, right: &Value) -> Vec<String> {
    fn walk(left: &Value, right: &Value, path: String, out: &mut Vec<String>) {
        match (left, right) {
            (Value::Object(l), Value::Object(r)) => {
                let keys: BTreeSet<&String> = l.keys().chain(r.keys()).collect();
                for key in keys {
                    let child = format!("{path}/{key}");
                    match (l.get(key), r.get(key)) {
                        (Some(a), Some(b)) => walk(a, b, child, out),
                        (Some(_), None) => out.push(format!("{child}: only before")),
                        (None, _) => out.push(format!("{child}: only after")),
                    }
                }
            }
            (Value::Array(l), Value::Array(r)) if l.len() == r.len() => {
                for (n, (a, b)) in l.iter().zip(r).enumerate() {
                    walk(a, b, format!("{path}/{n}"), out);
                }
            }
            _ if left != right => out.push(format!("{path}: {left} became {right}")),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(left, right, String::new(), &mut out);
    out
}

/// The registry paths, as a set.
fn registry() -> BTreeSet<&'static str> {
    V7_SAVED_KEYS.iter().map(|(_, path)| *path).collect()
}

/// The registry paths `paths` holds, or holds a key below.
fn v7_paths_in(paths: &BTreeSet<String>) -> Vec<String> {
    paths
        .iter()
        .filter(|path| {
            registry().iter().any(|key| {
                path.as_str() == *key
                    || path
                        .strip_prefix(key)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Serde field lists, read from the derived `Deserialize` impls.
// ---------------------------------------------------------------------------

/// The field names a derived `Deserialize` asks `deserialize_struct` for.
#[derive(Debug)]
struct Captured(Option<&'static [&'static str]>, String);
impl std::fmt::Display for Captured {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.1)
    }
}
impl std::error::Error for Captured {}
impl serde::de::Error for Captured {
    fn custom<M: std::fmt::Display>(message: M) -> Self {
        Self(None, message.to_string())
    }
}

/// A deserializer that answers every request with an error, except that a
/// struct request reports its serde field names (renames applied, skipped
/// fields left out).
struct FieldCapture;
impl<'de> serde::Deserializer<'de> for FieldCapture {
    type Error = Captured;
    fn deserialize_any<V: serde::de::Visitor<'de>>(self, _: V) -> Result<V::Value, Captured> {
        Err(Captured(None, "not a struct".into()))
    }
    fn deserialize_struct<V: serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Captured> {
        Err(Captured(Some(fields), String::new()))
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map enum identifier ignored_any
    }
}

/// The serialized field names of the derived struct `T`.
fn serde_fields<T: DeserializeOwned>() -> BTreeSet<&'static str> {
    match T::deserialize(FieldCapture) {
        Err(Captured(Some(fields), _)) => fields.iter().copied().collect(),
        Err(Captured(None, error)) => panic!("not a derived struct: {error}"),
        Ok(_) => panic!("the field capture never decodes a value"),
    }
}

/// A saved struct: its name, serde field names and
/// every key path (as [`game_key_paths`] spells it) where a save holds it.
struct SavedType {
    name: &'static str,
    fields: fn() -> BTreeSet<&'static str>,
    places: &'static [&'static str],
}

/// The section 3 owners, plus `Game` itself (which gained no field) and every
/// other struct the v6 fixture holds (W1 review), so a later field on any of
/// them must be registered too.
const SAVED_TYPES: &[SavedType] = &[
    SavedType {
        name: "Game",
        fields: serde_fields::<Game>,
        places: &[""],
    },
    SavedType {
        name: "Campaign",
        fields: serde_fields::<Campaign>,
        places: &["/campaign"],
    },
    SavedType {
        name: "Player",
        fields: serde_fields::<Player>,
        places: &["/campaign/player"],
    },
    SavedType {
        name: "Attempt",
        fields: serde_fields::<Attempt>,
        places: &["/attempt"],
    },
    SavedType {
        name: "Report",
        fields: serde_fields::<Report>,
        places: &["/last_result"],
    },
    SavedType {
        name: "RunRecord",
        fields: serde_fields::<RunRecord>,
        places: &["/campaign/run_history", "/pending_runs"],
    },
    SavedType {
        name: "ParallelBattle",
        fields: serde_fields::<ParallelBattle>,
        places: &["/battles"],
    },
    SavedType {
        name: "StrategyState",
        fields: serde_fields::<StrategyState>,
        places: &["/strategy"],
    },
    SavedType {
        name: "Recruit",
        fields: serde_fields::<Recruit>,
        places: &["/strategy/roster"],
    },
    SavedType {
        name: "Enemy",
        fields: serde_fields::<Enemy>,
        places: &["/attempt/enemies"],
    },
    SavedType {
        name: "ResolvedEnemy",
        fields: serde_fields::<ResolvedEnemy>,
        places: &["/attempt/enemies/resolved"],
    },
    SavedType {
        name: "ProgressionSnapshot",
        fields: serde_fields::<ProgressionSnapshot>,
        places: &["/attempt/progression"],
    },
    SavedType {
        name: "SkillModifiers",
        fields: serde_fields::<SkillModifiers>,
        places: &["/attempt/profile/modifiers"],
    },
    SavedType {
        name: "CombatTelemetry",
        fields: serde_fields::<CombatTelemetry>,
        places: &["/attempt/telemetry", "/last_result/telemetry"],
    },
    SavedType {
        name: "BalanceSettings",
        fields: serde_fields::<BalanceSettings>,
        places: &[
            "/attempt/balance",
            "/campaign/balance_settings",
            "/campaign/run_history/balance",
        ],
    },
    SavedType {
        name: "CorpseRecord",
        fields: serde_fields::<CorpseRecord>,
        places: &["/attempt/corpses"],
    },
    SavedType {
        name: "EnemyVisual",
        fields: serde_fields::<EnemyVisual>,
        places: &[
            "/attempt/corpses/visual",
            "/attempt/enemies/resolved/visual",
        ],
    },
    SavedType {
        name: "DpsTracker",
        fields: serde_fields::<DpsTracker>,
        places: &["/attempt/dps"],
    },
    SavedType {
        name: "DamageBucket",
        fields: serde_fields::<DamageBucket>,
        places: &["/attempt/dps/buckets"],
    },
    SavedType {
        name: "DamageTotals",
        fields: serde_fields::<DamageTotals>,
        places: &[
            "/attempt/dps/buckets/damage",
            "/attempt/dps/totals",
            "/campaign/run_history/damage_by_source",
        ],
    },
    SavedType {
        name: "BossTiming",
        fields: serde_fields::<BossTiming>,
        places: &["/attempt/enemies/boss_timing"],
    },
    SavedType {
        name: "InjuryState",
        fields: serde_fields::<InjuryState>,
        places: &[
            "/attempt/enemies/injury",
            "/attempt/injury",
            "/strategy/guards/injury",
        ],
    },
    SavedType {
        name: "Event",
        fields: serde_fields::<Event>,
        places: &["/attempt/events"],
    },
    SavedType {
        name: "GearRecord",
        fields: serde_fields::<GearRecord>,
        places: &["/attempt/gear_snapshot", "/campaign/run_history/gear"],
    },
    SavedType {
        name: "LootEvent",
        fields: serde_fields::<LootEvent>,
        places: &["/attempt/loot_events", "/last_result/loot_events"],
    },
    SavedType {
        name: "Modifiers",
        fields: serde_fields::<Modifiers>,
        places: &["/attempt/modifiers"],
    },
    SavedType {
        name: "OrdnanceState",
        fields: serde_fields::<OrdnanceState>,
        places: &["/attempt/ordnance"],
    },
    SavedType {
        name: "DefenderProfile",
        fields: serde_fields::<DefenderProfile>,
        places: &["/attempt/profile"],
    },
    SavedType {
        name: "EquipmentVisual",
        fields: serde_fields::<EquipmentVisual>,
        places: &["/attempt/profile/equipment_visual"],
    },
    SavedType {
        name: "EquipmentVisualItem",
        fields: serde_fields::<EquipmentVisualItem>,
        places: &[
            "/attempt/profile/equipment_visual/accessory",
            "/attempt/profile/equipment_visual/melee",
            "/attempt/profile/equipment_visual/outfit",
            "/attempt/profile/equipment_visual/ranged",
        ],
    },
    SavedType {
        name: "NpcProfile",
        fields: serde_fields::<NpcProfile>,
        places: &["/attempt/profile/npc"],
    },
    SavedType {
        name: "PersonalProfile",
        fields: serde_fields::<PersonalProfile>,
        places: &["/attempt/profile/personal"],
    },
    SavedType {
        name: "Stats",
        fields: serde_fields::<Stats>,
        places: &["/attempt/profile/stats"],
    },
    SavedType {
        name: "WeaponSpec",
        fields: serde_fields::<WeaponSpec>,
        places: &["/attempt/profile/weapon"],
    },
    SavedType {
        name: "Rng",
        fields: serde_fields::<Rng>,
        places: &["/attempt/rng", "/strategy/rng"],
    },
    SavedType {
        name: "Structure",
        fields: serde_fields::<Structure>,
        places: &["/attempt/structures"],
    },
    SavedType {
        name: "Item",
        fields: serde_fields::<Item>,
        places: &["/campaign/items"],
    },
    SavedType {
        name: "Npc",
        fields: serde_fields::<Npc>,
        places: &["/campaign/npc", "/strategy/roster/npc"],
    },
    SavedType {
        name: "NpcCreation",
        fields: serde_fields::<NpcCreation>,
        places: &["/campaign/npc/creation", "/strategy/roster/npc/creation"],
    },
    SavedType {
        name: "Preset",
        fields: serde_fields::<Preset>,
        places: &["/campaign/presets"],
    },
    SavedType {
        name: "WorldProgress",
        fields: serde_fields::<WorldProgress>,
        places: &["/campaign/world"],
    },
    SavedType {
        name: "GuardAssignment",
        fields: serde_fields::<GuardAssignment>,
        places: &["/strategy/guards"],
    },
];

/// v6 fields the v6 fixture never wrote, because the v6 build already
/// skipped them at their default. They are v6 shape, not v7 keys.
const V6_FIELDS_THE_FIXTURE_SKIPS: &[(&str, &str)] =
    &[("Attempt", "statuses"), ("Enemy", "statuses")];

/// The keys the v6 fixture writes for `ty` at any of its places (root and
/// worker games alike), plus its [`V6_FIELDS_THE_FIXTURE_SKIPS`].
fn v6_fields(ty: &SavedType, v6_paths: &BTreeSet<String>) -> BTreeSet<String> {
    let mut fields: BTreeSet<String> = V6_FIELDS_THE_FIXTURE_SKIPS
        .iter()
        .filter(|(name, _)| *name == ty.name)
        .map(|(_, key)| (*key).to_owned())
        .collect();
    for place in ty.places {
        let prefix = format!("{place}/");
        fields.extend(
            v6_paths
                .iter()
                .filter_map(|path| path.strip_prefix(&prefix))
                .filter(|rest| !rest.contains('/'))
                .map(str::to_owned),
        );
    }
    fields
}

// ---------------------------------------------------------------------------
// Non-default samples for every registry key.
// ---------------------------------------------------------------------------

/// Every quality, as a `KindMap<Quality, u32>` writes it.
fn by_quality(common: u32, rare: u32) -> Value {
    json!({"Poor": 0, "Common": common, "Uncommon": 0, "Rare": rare, "Epic": 0, "Legendary": 0})
}

/// A non-default value for the registry field `key` (the last path segment).
/// f32 samples are exact binary fractions, so the saved text is unchanged.
fn sample(key: &str) -> Value {
    match key {
        "auto_sell" => json!({
            "enabled": true,
            "sell_below": "Rare",
            "slots": [true, false, true, true],
            "keep_upgrades": false,
            "apply_to_npc_runs": false,
        }),
        "next_defender" | "defender" => json!({"Recruit": 2}),
        "reserved_recruit" => json!(2),
        "milestones_claimed" => json!([5, 10, 15]),
        "balance_extras_basis" => json!("Hard"),
        "level_grant_paid" => json!(11),
        "drop_ids" => json!([3, 7]),
        "drop_tier" => json!(3),
        "loot_by_quality" => by_quality(1, 1),
        "auto_sold" => json!({
            "count": 2,
            "coins": 40,
            "by_quality": by_quality(2, 0),
            "lines": [{"definition": "crowbar", "level": 2, "quality": "Common", "coins": 25}],
            "kept_on_overflow": 1,
        }),
        "progression_rewards" => json!({
            "level_from": 11,
            "level_to": 12,
            "level_coins": 120,
            "milestones": [
                {"rank": 15, "coins": 300, "item_id": 9, "item_definition": "crowbar"},
                {"rank": 20, "coins": 400},
            ],
        }),
        "boss_rank" => json!("Capstone"),
        "auto_sold_count" => json!(2),
        "auto_sold_coins" => json!(40),
        "level_grant_coins" => json!(120),
        "milestone_coins" => json!(300),
        "elite_kills" => json!(1),
        "boss_ttk_ticks" => json!(240),
        "summary" => json!({
            "outcome": "Victory",
            "stages_cleared": 2,
            "liberated": true,
            "fronts": ["fra"],
            "coins": 80,
            "loot_count": 3,
            "acknowledged": false,
        }),
        "pending_auto_sell" => json!([11, 12]),
        "folded_outcomes" => json!(2),
        "containments" => json!(1),
        "split_child" => json!(true),
        "boss_state" => json!({
            "profile": {
                "name": "Enraged",
                "speed_mult": 1.25,
                "armor_mult": 0.5,
                "call_interval_mult": 0.75,
                "siege_recovery_mult": 0.5,
                "hold_ticks": 60,
            },
            "transitions": 2,
            "entered": 1,
            "hold_until": 900,
        }),
        "reach" => json!(1.5),
        "ranged_armor" => json!(10),
        "npc_ranged_armor" => json!(2),
        "stagger_immune" => json!(true),
        "group_size" => json!(3),
        "death_effect" => json!({"Split": {"archetype_id": "shambler", "count": 2}}),
        "tags" => json!(["armored", "slow"]),
        "elite" => json!({"affix_id": "vampiric", "name": "Vampiric Brute"}),
        "elite_rate_bp" => json!(500),
        "front_ramp" => json!(1.125),
        "boss_phases" => json!(2),
        "boss_armor_multiplier" => json!(1.5),
        "npc_boss_armor_add" => json!(2),
        "boss_health_multiplier" => json!(1.25),
        "boss_reward_multiplier" => json!(1.375),
        "boss_damage_multiplier" => json!(1.0625),
        "class_modifiers" => json!({
            "Shotgun": {
                "damage_multiplier": 1.25,
                "reload_multiplier": 0.75,
                "cadence_multiplier": 1.0,
                "weak_bonus_add": 0.25,
            },
        }),
        "armor_penetration" => json!(5),
        "hazard_damage_multiplier" => json!(0.75),
        "spawned_by_archetype" => json!({"shambler": 3, "runner": 1}),
        "kills_by_archetype" => json!({"shambler": 2}),
        "damage_taken_by_archetype" => json!({"shambler": 40}),
        "elites_spawned" => json!(1),
        "elites_killed" => json!(1),
        "hazard_damage_taken" => json!(7),
        "boss_phase_ticks" => json!([120, 240]),
        other => panic!("V7_SAVED_KEYS key {other} has no non-default sample here"),
    }
}

/// Set `key` to `sample` on every object at `parent` (walking arrays).
fn set_everywhere(value: &mut Value, parent: &[&str], key: &str, sample: &Value) {
    match value {
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| set_everywhere(item, parent, key, sample)),
        Value::Object(map) => match parent.split_first() {
            None => {
                map.insert(key.to_owned(), sample.clone());
            }
            Some((first, rest)) => {
                if let Some(child) = map.get_mut(*first) {
                    set_everywhere(child, rest, key, sample);
                }
            }
        },
        _ => {}
    }
}

/// The migrated v6 save with every registry field set to its sample, in the
/// root game and in every worker game. A run record is copied into the empty
/// pending list first, so its fields have a place to go.
fn staged_v7_payload() -> Value {
    let mut value = migrated_v6();
    let record = value["campaign"]["run_history"][0].clone();
    assert!(record.is_object(), "the v6 fixture has a run record");
    value["pending_runs"] = json!([record]);
    for (_, path) in V7_SAVED_KEYS {
        let segments: Vec<&str> = path.split('/').skip(1).collect();
        let (key, parent) = segments.split_last().unwrap();
        let mut places = vec![parent.to_vec()];
        if !path.starts_with("/battles/") {
            let worker: Vec<&str> = WORKER_GAME_PREFIX.split('/').skip(1).collect();
            places.push([worker.as_slice(), parent].concat());
        }
        for place in places {
            set_everywhere(&mut value, &place, key, &sample(key));
        }
    }
    value
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Risk 1: a v6 save decodes into the v7 types and re-encodes to exactly the
/// migrated JSON. A missing serde default fails the decode; a dropped or
/// renamed v6 field, or a v7 field written at its default, fails the
/// comparison. The only keys the migration itself adds are registry keys.
#[test]
fn v6_payload_round_trips_unchanged_through_v7_types() {
    let raw = v6_payload();
    let migrated = migrated_v6();
    let game: Game = serde_json::from_value(migrated.clone())
        .unwrap_or_else(|e| panic!("the migrated v6 payload decodes: {e}"));
    let resaved = as_saved(&game);
    assert_eq!(
        differences(&migrated, &resaved),
        Vec::<String>::new(),
        "the v7 types changed the migrated v6 payload"
    );
    assert_eq!(resaved, migrated);
    // The migration adds v7 keys only (sub-steps (a) to (e)).
    let before = game_key_paths(&raw);
    let added: BTreeSet<String> = game_key_paths(&migrated)
        .difference(&before)
        .cloned()
        .collect();
    let registered: BTreeSet<String> = v7_paths_in(&added).into_iter().collect();
    let unregistered: Vec<&String> = added.difference(&registered).collect();
    assert!(
        unregistered.is_empty(),
        "the migration added keys outside V7_SAVED_KEYS: {unregistered:?}"
    );
    assert!(
        !added.is_empty(),
        "the fixture migration adds the v7 ledgers and seats"
    );
}

/// The v7 keys a GOLDEN run may write once a wave-2 writer populates them,
/// each with the commit whose provisional GOLDEN move (`tests/determinism.rs`,
/// ruling P3) names it. W2-05 is the per-archetype telemetry writer, class T;
/// W2-06 is the drop tracking writer (the frozen drop tier, the root drop
/// list and the report's loot fields), class F. W2-52 is the neutral
/// level-grant ledger (a Personal level-up keeps `level_grant_paid` at the
/// level), class F. W2-54 authors the level grant in `progression.json`
/// (R32, which accepted this guard's update with the §2.5 default), so a
/// Personal GOLDEN run that levels 1 -> 2 owes the 5-coin grant and its
/// withdrawal pays it: the run record's `level_grant_coins` and the report's
/// receipt.
const WRITTEN_V7_KEYS: [(&str, &str); 13] = [
    ("W2-05", "/attempt/telemetry/spawned_by_archetype"),
    ("W2-05", "/attempt/telemetry/kills_by_archetype"),
    ("W2-05", "/attempt/telemetry/damage_taken_by_archetype"),
    ("W2-05", "/last_result/telemetry/spawned_by_archetype"),
    ("W2-05", "/last_result/telemetry/kills_by_archetype"),
    ("W2-05", "/last_result/telemetry/damage_taken_by_archetype"),
    ("W2-06", "/attempt/drop_tier"),
    ("W2-06", "/attempt/drop_ids"),
    ("W2-06", "/last_result/drop_ids"),
    ("W2-06", "/last_result/loot_by_quality"),
    ("W2-52", "/campaign/player/level_grant_paid"),
    ("W2-54", "/campaign/run_history/level_grant_coins"),
    ("W2-54", "/last_result/progression_rewards"),
];

/// Whether `path` is one of [`WRITTEN_V7_KEYS`] or lies below one.
fn written_by_a_writer(path: &str) -> bool {
    WRITTEN_V7_KEYS.iter().any(|(_, key)| {
        path == *key
            || path
                .strip_prefix(key)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// The v7 schema fields move no digest (schema-v7.md section 12): the
/// default game and every GOLDEN run, live and after a withdrawal settles it
/// into a report, write no registry key and no key path the v6 build did not
/// write, apart from the keys a wave-2 writer populates in a GOLDEN run
/// ([`WRITTEN_V7_KEYS`]; the default game gets no exemption).
#[test]
fn default_and_golden_games_serialize_no_v7_key() {
    let v6_paths = game_key_paths(&v6_payload());
    let check = |game: &Game, what: &str, golden: bool| {
        let (v7, unknown) = unexpected_keys(&as_saved(game), &v6_paths, golden);
        assert!(v7.is_empty(), "{what} wrote v7 keys: {v7:?}");
        assert!(
            unknown.is_empty(),
            "{what} wrote key paths the v6 build never wrote: {unknown:?}"
        );
    };
    check(&Game::default(), "the default game", false);
    let mut runs = 0;
    for seed in GOLDEN_SEEDS {
        for mode in [Mode::Personal, Mode::Npc] {
            let mut game = support::smoke_run(seed, mode, GOLDEN_TICKS, |_| {});
            let what = format!("GOLDEN seed {seed:#x} {mode:?}");
            assert!(game.attempt().is_some(), "{what} is live");
            check(&game, &what, true);
            game.apply(&Command::Withdraw).unwrap();
            assert!(game.last_result().is_some(), "{what} reported");
            check(&game, &format!("{what} after a withdrawal"), true);
            runs += 1;
        }
    }
    assert_eq!(runs, 6);
}

/// The registry key paths and the key paths the v6 build never wrote that
/// the saved game `saved` holds. `golden` exempts [`WRITTEN_V7_KEYS`]: only
/// a GOLDEN run plays, so only it may hold what a wave-2 writer populates.
fn unexpected_keys(
    saved: &Value,
    v6_paths: &BTreeSet<String>,
    golden: bool,
) -> (Vec<String>, Vec<String>) {
    let paths: BTreeSet<String> = game_key_paths(saved)
        .into_iter()
        .filter(|path| !(golden && written_by_a_writer(path)))
        .collect();
    let v7 = v7_paths_in(&paths);
    let unknown = paths.difference(v6_paths).cloned().collect();
    (v7, unknown)
}

/// The default game is checked without the writer exemption: a default game
/// that wrote a writer's key (the level-grant ledger, say) fails.
#[test]
fn the_default_game_check_does_not_exempt_writer_keys() {
    let v6_paths = game_key_paths(&v6_payload());
    let clean = as_saved(&Game::default());
    assert_eq!(
        unexpected_keys(&clean, &v6_paths, false),
        (Vec::new(), Vec::new())
    );
    for (_, key) in WRITTEN_V7_KEYS {
        let mut wrote = clean.clone();
        let mut at = &mut wrote;
        for part in key.trim_start_matches('/').split('/') {
            // A missing (null) section becomes an object, and a list in the
            // path is written with one element.
            if at.is_null() {
                *at = json!({});
            }
            if let Value::Array(list) = at {
                if list.is_empty() {
                    list.push(json!({}));
                }
                at = &mut list[0];
            }
            at = at
                .as_object_mut()
                .unwrap_or_else(|| panic!("{key}: {part} is under an object"))
                .entry(part)
                .or_insert(Value::Null);
        }
        *at = json!(1);
        let reported = |golden: bool| {
            let (v7, unknown) = unexpected_keys(&wrote, &v6_paths, golden);
            v7.iter().chain(&unknown).any(|path| path == key)
        };
        assert!(reported(false), "a default game that wrote {key} passes");
        assert!(!reported(true), "a GOLDEN run may write {key}");
    }
}

/// Every registry field, set to a non-default sample in the root game and in
/// the worker games, survives a save round trip unchanged. A sample at its
/// default would be skipped on encode, and a field the types do not know
/// would be dropped on decode; both fail the comparison.
#[test]
fn v7_fields_round_trip_at_non_default_values() {
    let staged = staged_v7_payload();
    // Every row is staged in the root game (or on the root's battles).
    for (section, path) in V7_SAVED_KEYS {
        let key = path.rsplit('/').next().unwrap();
        let found = values_at_key_path(&staged, path);
        assert!(
            !found.is_empty() && found.iter().all(|value| **value == sample(key)),
            "{section} {path} is not staged in the root game"
        );
    }
    // The worker games carry the samples too: their seats, ledgers, live
    // attempt and settled report.
    let worker_rows = V7_SAVED_KEYS
        .iter()
        .filter(|(_, path)| {
            !values_at_key_path(&staged, &format!("{WORKER_GAME_PREFIX}{path}")).is_empty()
        })
        .count();
    assert!(
        worker_rows >= 40,
        "only {worker_rows} rows staged in worker games"
    );
    let game: Game = serde_json::from_value(staged.clone())
        .unwrap_or_else(|e| panic!("the staged v7 save decodes: {e}"));
    let resaved = as_saved(&game);
    assert_eq!(
        differences(&staged, &resaved),
        Vec::<String>::new(),
        "v7 fields did not survive the round trip"
    );
    // Byte-identical: a second save of the decoded game writes the same text.
    let text = serde_json::to_string(&game).unwrap();
    let again: Game = serde_json::from_str(&text).unwrap();
    assert_eq!(serde_json::to_string(&again).unwrap(), text);
    // The typed values are the samples.
    let attempt = game.attempt().unwrap();
    assert_eq!(attempt.drop_ids, vec![3, 7]);
    assert_eq!(attempt.drop_tier, Some(3));
    assert_eq!(attempt.progression.elite_rate_bp, 500);
    assert_eq!(attempt.telemetry.boss_phase_ticks, vec![120, 240]);
    assert_eq!(game.strategy().folded_outcomes, 2);
    assert!(game.campaign().auto_sell.enabled);
}

/// One v6-shaped object per touched type, as the v6 build wrote it (the
/// first instance in the v6 fixture), decodes into the v7 type and re-encodes
/// unchanged: every new field defaults on decode and is skipped on encode.
#[test]
fn v6_shaped_objects_decode_into_every_touched_type() {
    type Check = fn(&Value) -> Result<Value, String>;
    let rows: [(&str, &str, Check); 10] = [
        (
            "RunRecord",
            "/campaign/run_history/0",
            round_trip::<RunRecord>,
        ),
        (
            "CombatTelemetry",
            "/attempt/telemetry",
            round_trip::<CombatTelemetry>,
        ),
        ("Recruit", "/strategy/roster/0", round_trip::<Recruit>),
        ("StrategyState", "/strategy", round_trip::<StrategyState>),
        ("Player", "/campaign/player", round_trip::<Player>),
        (
            "ResolvedEnemy",
            "/attempt/enemies/0/resolved",
            round_trip::<ResolvedEnemy>,
        ),
        ("Report", "/last_result", round_trip::<Report>),
        ("Attempt", "/attempt", round_trip::<Attempt>),
        ("ParallelBattle", "/battles/0", round_trip::<ParallelBattle>),
        (
            "ProgressionSnapshot",
            "/attempt/progression",
            round_trip::<ProgressionSnapshot>,
        ),
    ];
    let raw = v6_payload();
    let failures: Vec<String> = rows
        .iter()
        .filter_map(|(name, pointer, check)| {
            let object = raw.pointer(pointer).filter(|v| v.is_object());
            let Some(object) = object else {
                return Some(format!("{name}: the v6 fixture has no object at {pointer}"));
            };
            match check(object) {
                Ok(resaved) if resaved == *object => None,
                Ok(resaved) => Some(format!(
                    "{name}: re-encoded differently: {:?}",
                    differences(object, &resaved)
                )),
                Err(error) => Some(format!("{name}: does not decode: {error}")),
            }
        })
        .collect();
    assert_eq!(failures, Vec::<String>::new());
}

/// Whether `key` reads as a serde field name (snake case), not a map key: a
/// `KindMap` writes its variant names and a data-keyed map collapses to `*`.
fn is_field_name(key: &str) -> bool {
    key.starts_with(|c: char| c.is_ascii_lowercase())
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Every struct the v6 fixture holds (an object whose keys are all field
/// names) is a place of a [`SAVED_TYPES`] entry, so a later serde-skipped
/// field on any saved struct, not only the section 3 owners, must reach
/// `V7_SAVED_KEYS` (the W1-63 freeze rule).
#[test]
fn saved_types_cover_every_struct_the_v6_fixture_holds() {
    let places: BTreeSet<&str> = SAVED_TYPES
        .iter()
        .flat_map(|ty| ty.places.iter().copied())
        .collect();
    let mut keys_at: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in game_key_paths(&v6_payload()) {
        let (parent, key) = path.rsplit_once('/').unwrap();
        keys_at
            .entry(parent.to_owned())
            .or_default()
            .push(key.to_owned());
    }
    let uncovered: Vec<&String> = keys_at
        .iter()
        .filter(|(parent, keys)| {
            keys.iter().all(|key| is_field_name(key)) && !places.contains(parent.as_str())
        })
        .map(|(parent, _)| parent)
        .collect();
    assert_eq!(
        uncovered,
        Vec::<&String>::new(),
        "saved structs with no SAVED_TYPES entry"
    );
}

/// `V7_SAVED_KEYS` is exactly the set of fields the saved types serialize
/// beyond the v6 shape, at every place a save holds each type. The field
/// lists come from the types, so a new saved field fails here until the
/// registry (and its sample above) lists it, and a stale row fails too.
#[test]
fn v7_saved_keys_registry_matches_types() {
    let v6_paths = game_key_paths(&v6_payload());
    let mut expected: BTreeMap<String, &str> = BTreeMap::new();
    for ty in SAVED_TYPES {
        let fields = (ty.fields)();
        let v6 = v6_fields(ty, &v6_paths);
        let lost: Vec<&String> = v6.iter().filter(|k| !fields.contains(k.as_str())).collect();
        assert!(
            lost.is_empty(),
            "{}: v6 fields the type no longer has: {lost:?}",
            ty.name
        );
        for field in fields.iter().filter(|f| !v6.contains(**f)) {
            for place in ty.places {
                expected.insert(format!("{place}/{field}"), ty.name);
            }
        }
    }
    for (name, key) in V6_FIELDS_THE_FIXTURE_SKIPS {
        assert!(
            SAVED_TYPES.iter().any(|ty| ty.name == *name),
            "{name} is not a saved type here"
        );
        let ty = SAVED_TYPES.iter().find(|ty| ty.name == *name).unwrap();
        assert!(
            ty.places
                .iter()
                .all(|place| !v6_paths.contains(&format!("{place}/{key}"))),
            "{name}.{key} is written by the v6 fixture: drop it from V6_FIELDS_THE_FIXTURE_SKIPS"
        );
    }
    let registry = registry();
    let missing: Vec<String> = expected
        .iter()
        .filter(|(path, _)| !registry.contains(path.as_str()))
        .map(|(path, ty)| format!("{ty} {path}"))
        .collect();
    let stale: Vec<&&str> = registry
        .iter()
        .filter(|path| !expected.contains_key(**path))
        .collect();
    assert!(
        missing.is_empty() && stale.is_empty(),
        "V7_SAVED_KEYS does not match the types.\n  missing: {missing:?}\n  stale: {stale:?}"
    );
    assert_eq!(
        registry.len(),
        V7_SAVED_KEYS.len(),
        "V7_SAVED_KEYS lists a path twice"
    );
    // Every row names its schema-v7.md section and its type.
    for (section, path) in V7_SAVED_KEYS {
        assert!(
            section.ends_with(expected[*path]),
            "{path} is listed under {section}, but it is a {} field",
            expected[*path]
        );
    }
}
