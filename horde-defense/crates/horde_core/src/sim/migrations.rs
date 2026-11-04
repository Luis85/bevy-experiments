//! Ordered save-format migrations on raw JSON, applied before typed
//! deserialization. Each step freezes the meaning of its source version, so a
//! migration never consults the currently loaded catalogues.
use super::persistence::SaveError;
use serde_json::{Map, Value};

/// Envelope version written by this build.
pub const SAVE_VERSION: u32 = 5;

/// Upgrade a payload saved at envelope `from` to [`SAVE_VERSION`].
pub(super) fn migrate(mut value: Value, from: u32) -> Result<Value, SaveError> {
    if from == 0 || from > SAVE_VERSION {
        return Err(SaveError::UnsupportedVersion(from));
    }
    for version in from..SAVE_VERSION {
        value = match version {
            1 => v1_to_v2(value)?,
            2 => v2_to_v3(value),
            3 => v3_to_v4(value)?,
            4 => v4_to_v5(value)?,
            _ => unreachable!("every version below SAVE_VERSION has a step"),
        };
    }
    Ok(value)
}

/// v1 item definition order (`content.json` items at save format v1). A v1
/// `definition: n` means `V1_ITEM_IDS[n]`.
const V1_ITEM_IDS: [&str; 29] = [
    "service_carbine",
    "crowbar",
    "work_jacket",
    "field_webbing",
    "marksman_rifle",
    "fire_axe",
    "padded_vest",
    "first_aid_rig",
    "patrol_rifle",
    "forged_machete",
    "scavenged_ballistic_vest",
    "rangefinder",
    "longwatch_rifle",
    "breach_hammer",
    "riot_armor",
    "survival_harness",
    "checkpoint_rifle",
    "tempered_saber",
    "fortress_armor",
    "veteran_kit",
    "worn_service_pistol",
    "compact_service_pistol",
    "compact_smg",
    "pump_shotgun",
    "patrol_rifle_tier1",
    "scout_marksman_rifle",
    "belt_fed_machine_gun",
    "heavy_revolver",
    "double_barrel_shotgun",
];
const V1_ENEMY_KINDS: &[&str] = &["Shambler", "Runner", "Brute", "Armored", "Demolisher"];
const V1_STRUCTURE_KINDS: &[&str] = &["Trap", "Wall", "Turret"];
const V1_BODY_ATTRIBUTES: &[&str] = &[
    "Vitality",
    "Strength",
    "Coordination",
    "Endurance",
    "Perception",
];
const V1_NPC_TRAINING: &[&str] = &[
    "Strength",
    "Dexterity",
    "Intelligence",
    "Health",
    "Guns",
    "Melee",
    "HitPoints",
    "Fatigue",
];
const V1_NPC_MANEUVERS: &[&str] = &[
    "DoNothing",
    "Aim",
    "Attack",
    "Ready",
    "Reload",
    "Recover",
    "Posture",
    "AllOutDefense",
];

/// v1 positional arrays, by field name, and the kind order their slots meant.
/// These names are unique to these fields across the whole save tree.
const V1_KIND_ARRAYS: &[(&str, &[&str])] = &[
    ("kills_by_kind", V1_ENEMY_KINDS),
    ("npc_decisions", V1_NPC_MANEUVERS),
    ("blueprints", V1_STRUCTURE_KINDS),
    ("structure_upgrades", V1_STRUCTURE_KINDS),
    ("structure_ranks", V1_STRUCTURE_KINDS),
    ("body", V1_BODY_ATTRIBUTES),
    ("body_development", V1_BODY_ATTRIBUTES),
    ("investments", V1_NPC_TRAINING),
];

/// v1 -> v2: positional item `definition` indices become stable item IDs and
/// enum-indexed arrays become kind-keyed maps, wherever they occur (campaign,
/// attempts, reports, run history, recruits and nested expedition games).
/// Values already in v2 shape are left untouched, so the step is idempotent.
fn v1_to_v2(mut value: Value) -> Result<Value, SaveError> {
    rewrite_v1(&mut value)?;
    Ok(value)
}

fn rewrite_v1(value: &mut Value) -> Result<(), SaveError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                if key == "definition"
                    && let Value::Number(index) = child
                {
                    let id = index
                        .as_u64()
                        .and_then(|n| V1_ITEM_IDS.get(usize::try_from(n).ok()?))
                        .ok_or_else(|| {
                            SaveError::Migration(format!("Unknown v1 item definition {index}"))
                        })?;
                    *child = Value::String((*id).to_owned());
                    continue;
                }
                if let Some((_, kinds)) = V1_KIND_ARRAYS.iter().find(|(name, _)| name == key)
                    && let Value::Array(slots) = child
                {
                    if slots.len() != kinds.len() {
                        return Err(SaveError::Migration(format!(
                            "v1 field {key} has {} entries; expected {}",
                            slots.len(),
                            kinds.len()
                        )));
                    }
                    let map: Map<String, Value> = kinds
                        .iter()
                        .map(|kind| (*kind).to_owned())
                        .zip(std::mem::take(slots))
                        .collect();
                    *child = Value::Object(map);
                    continue;
                }
                rewrite_v1(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_v1(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// v2 boolean skill unlock flags and the unlock IDs that replaced them.
const V2_UNLOCK_FLAGS: &[(&str, &str)] = &[
    ("piercing", "piercing"),
    ("sweep", "sweep"),
    ("heal", "heal"),
    ("brace", "brace"),
    ("grenade_unlocked", "grenade"),
    ("molotov_unlocked", "molotov"),
];
/// v2 per-actor tick counters and the status IDs that replaced them.
const V2_STATUS_COUNTERS: &[(&str, &str)] =
    &[("hit_stop_left", "HitStop"), ("brace_left", "Brace")];
/// NPC target urgency every v2 hostile category implied (only Runners were
/// prioritised, by 3 yards).
const V2_RUNNER_TARGET_URGENCY: f64 = -3.0;

/// v2 -> v3, wherever the shapes occur (attempts, nested expedition games):
/// - enemy `hit_stop_left` / attempt `brace_left` counters become a sorted
///   `statuses` list (a zero counter is an absent status);
/// - skill-modifier unlock booleans become the sorted `unlocks` ID set;
/// - resolved enemy snapshots gain the `target_urgency` their category implied.
///
/// Values already in v3 shape are left untouched, so the step is idempotent.
fn v2_to_v3(mut value: Value) -> Value {
    rewrite_v2(&mut value);
    value
}

fn rewrite_v2(value: &mut Value) {
    match value {
        Value::Object(object) => {
            migrate_status_counters(object);
            if object.contains_key("cadence_multiplier") {
                migrate_unlock_flags(object);
            }
            if object.contains_key("archetype_id")
                && object.contains_key("npc_damage_dice")
                && !object.contains_key("target_urgency")
            {
                let runner = object.get("kind").and_then(Value::as_str) == Some("Runner");
                let urgency = if runner {
                    V2_RUNNER_TARGET_URGENCY
                } else {
                    0.0
                };
                object.insert("target_urgency".into(), urgency.into());
            }
            for child in object.values_mut() {
                rewrite_v2(child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(rewrite_v2),
        _ => {}
    }
}

fn migrate_status_counters(object: &mut Map<String, Value>) {
    let mut statuses = Vec::new();
    // V2_STATUS_COUNTERS is in StatusId order, so the list stays sorted.
    for (field, id) in V2_STATUS_COUNTERS {
        if let Some(left) = object.remove(*field)
            && let Some(left) = left.as_u64().filter(|left| *left > 0)
        {
            statuses.push(serde_json::json!({
                "id": id, "remaining_ticks": left, "stacks": 1, "magnitude": 0
            }));
        }
    }
    if !statuses.is_empty() {
        object.insert("statuses".into(), Value::Array(statuses));
    }
}

fn migrate_unlock_flags(object: &mut Map<String, Value>) {
    let mut unlocks: Vec<&str> = Vec::new();
    for (flag, id) in V2_UNLOCK_FLAGS {
        if object.remove(*flag).and_then(|v| v.as_bool()) == Some(true) {
            unlocks.push(id);
        }
    }
    unlocks.sort_unstable();
    if !unlocks.is_empty() || !object.contains_key("unlocks") {
        object.insert("unlocks".into(), unlocks.into());
    }
}

/// v3 quality order (`Quality` declaration order at save format v3). A v3
/// `loot_by_quality` / `loot_quality_counts` slot `n` meant `V3_QUALITIES[n]`.
const V3_QUALITIES: &[&str] = &["Poor", "Common", "Uncommon", "Rare", "Epic", "Legendary"];
/// v3 positional quality arrays, by field name (unique across the save tree).
const V3_QUALITY_ARRAYS: &[&str] = &["loot_by_quality", "loot_quality_counts"];

/// v3 -> v4: the quality-indexed loot counters (attempts, reports, run
/// history, nested expedition games) become quality-keyed maps. Values already
/// in v4 shape are left untouched, so the step is idempotent.
fn v3_to_v4(mut value: Value) -> Result<Value, SaveError> {
    rewrite_v3(&mut value)?;
    Ok(value)
}

fn rewrite_v3(value: &mut Value) -> Result<(), SaveError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                if V3_QUALITY_ARRAYS.contains(&key.as_str())
                    && let Value::Array(slots) = child
                {
                    if slots.len() != V3_QUALITIES.len() {
                        return Err(SaveError::Migration(format!(
                            "v3 field {key} has {} entries; expected {}",
                            slots.len(),
                            V3_QUALITIES.len()
                        )));
                    }
                    let map: Map<String, Value> = V3_QUALITIES
                        .iter()
                        .map(|quality| (*quality).to_owned())
                        .zip(std::mem::take(slots))
                        .collect();
                    *child = Value::Object(map);
                    continue;
                }
                rewrite_v3(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_v3(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// v4 `OrdnanceKind` order (declaration order at save format v4). A v4
/// ordnance counter slot `n` meant `V4_ORDNANCE_KINDS[n]`.
const V4_ORDNANCE_KINDS: &[&str] = &["Grenade", "Molotov"];
/// v4 positional per-kind counters of an attempt's `ordnance` state.
const V4_ORDNANCE_ARRAYS: &[&str] = &["cooldowns", "charges", "issued", "spent"];

/// v4 -> v5: the per-kind ordnance counters of every attempt's `ordnance`
/// state (including nested expedition games) become kind-keyed maps. Values
/// already in v5 shape are left untouched, so the step is idempotent.
fn v4_to_v5(mut value: Value) -> Result<Value, SaveError> {
    rewrite_v4(&mut value)?;
    Ok(value)
}

fn rewrite_v4(value: &mut Value) -> Result<(), SaveError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                if key == "ordnance"
                    && let Value::Object(state) = child
                {
                    for field in V4_ORDNANCE_ARRAYS {
                        if let Some(slots) = state.get_mut(*field)
                            && let Value::Array(entries) = slots
                        {
                            if entries.len() != V4_ORDNANCE_KINDS.len() {
                                return Err(SaveError::Migration(format!(
                                    "v4 ordnance {field} has {} entries; expected {}",
                                    entries.len(),
                                    V4_ORDNANCE_KINDS.len()
                                )));
                            }
                            let map: Map<String, Value> = V4_ORDNANCE_KINDS
                                .iter()
                                .map(|kind| (*kind).to_owned())
                                .zip(std::mem::take(entries))
                                .collect();
                            *slots = Value::Object(map);
                        }
                    }
                    continue;
                }
                rewrite_v4(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_v4(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::{BodyAttribute, NpcTraining};
    use crate::content::{EnemyKind, StructureKind, content};
    use crate::sim::NpcManeuver;
    use serde_json::json;

    fn serde_names<T: serde::Serialize>(kinds: &[T]) -> Vec<String> {
        kinds
            .iter()
            .map(|kind| json!(kind).as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn frozen_v1_orders_match_the_kinds_they_replaced() {
        // The constants are frozen on purpose; this documents that the v1 slot
        // order equals the declaration order the v1 arrays were indexed by.
        assert_eq!(serde_names(&EnemyKind::ALL), V1_ENEMY_KINDS);
        assert_eq!(serde_names(&StructureKind::ALL), V1_STRUCTURE_KINDS);
        assert_eq!(serde_names(&BodyAttribute::ALL), V1_BODY_ATTRIBUTES);
        assert_eq!(serde_names(&NpcTraining::ALL), V1_NPC_TRAINING);
        assert_eq!(
            serde_names(<NpcManeuver as crate::kind_map::StableKind>::ALL),
            V1_NPC_MANEUVERS
        );
        // Every frozen v1 item still resolves in the bundled catalogue.
        for id in V1_ITEM_IDS {
            assert!(content().item_index(&id.into()).is_some(), "{id}");
        }
    }

    #[test]
    fn v1_positions_become_ids_and_maps_everywhere_and_the_step_is_idempotent() {
        let v1 = json!({
            "campaign": {
                "items": [{"id": 9, "definition": 24}],
                "blueprints": [true, false, true],
                "structure_upgrades": [0, 2, 1],
                "player": {"body": [1, 0, 0, 0, 2]},
                "npc": {"investments": [1, 0, 0, 0, 3, 0, 0, 0]},
                "run_history": [{"gear": [{"definition": 0}]}]
            },
            "attempt": {
                "telemetry": {"kills_by_kind": [4, 0, 1, 0, 0], "npc_decisions": [0, 5, 0, 0, 0, 0, 0, 1]},
                "loot_events": [{"definition": 12}],
                "profile": {
                    "modifiers": {"body_development": [0, 0, 1, 0, 0], "blueprints": [false, false, true], "structure_ranks": [0, 0, 3]},
                    "equipment_visual": {"outfit": {"definition": 2}}
                }
            },
            "battles": [{"game": {"campaign": {"items": [{"definition": 3}]}}}]
        });
        let v2 = migrate(v1, 1).unwrap();
        assert_eq!(
            v2["campaign"]["items"][0]["definition"],
            "patrol_rifle_tier1"
        );
        assert_eq!(v2["campaign"]["items"][0]["id"], 9);
        assert_eq!(
            v2["campaign"]["blueprints"],
            json!({"Trap": true, "Wall": false, "Turret": true})
        );
        assert_eq!(v2["campaign"]["structure_upgrades"]["Wall"], 2);
        assert_eq!(v2["campaign"]["player"]["body"]["Perception"], 2);
        assert_eq!(v2["campaign"]["npc"]["investments"]["Guns"], 3);
        assert_eq!(
            v2["campaign"]["run_history"][0]["gear"][0]["definition"],
            "service_carbine"
        );
        assert_eq!(v2["attempt"]["telemetry"]["kills_by_kind"]["Brute"], 1);
        assert_eq!(
            v2["attempt"]["telemetry"]["npc_decisions"]["AllOutDefense"],
            1
        );
        assert_eq!(
            v2["attempt"]["loot_events"][0]["definition"],
            "longwatch_rifle"
        );
        assert_eq!(
            v2["attempt"]["profile"]["modifiers"]["structure_ranks"]["Turret"],
            3
        );
        assert_eq!(
            v2["attempt"]["profile"]["equipment_visual"]["outfit"]["definition"],
            "work_jacket"
        );
        assert_eq!(
            v2["battles"][0]["game"]["campaign"]["items"][0]["definition"],
            "field_webbing"
        );
        assert_eq!(v1_to_v2(v2.clone()).unwrap(), v2);
    }

    #[test]
    fn malformed_v1_shapes_and_versions_are_typed_errors() {
        assert!(matches!(
            migrate(json!({"definition": 29}), 1),
            Err(SaveError::Migration(_))
        ));
        assert!(matches!(
            migrate(json!({"blueprints": [true, true]}), 1),
            Err(SaveError::Migration(_))
        ));
        assert!(matches!(
            migrate(json!({}), 0),
            Err(SaveError::UnsupportedVersion(0))
        ));
        assert!(matches!(
            migrate(json!({}), SAVE_VERSION + 1),
            Err(SaveError::UnsupportedVersion(_))
        ));
        assert_eq!(
            migrate(json!({"x": 1}), SAVE_VERSION).unwrap(),
            json!({"x": 1})
        );
    }

    #[test]
    fn frozen_v2_names_match_the_ids_that_replaced_them() {
        use crate::skill_catalog::unlocks;
        use crate::status::StatusId;
        let ids: Vec<_> = V2_UNLOCK_FLAGS.iter().map(|(_, id)| *id).collect();
        assert_eq!(ids, unlocks::ALL);
        assert_eq!(
            V2_STATUS_COUNTERS
                .iter()
                .map(|(_, id)| *id)
                .collect::<Vec<_>>(),
            serde_names(&[StatusId::HitStop, StatusId::Brace])
        );
    }

    #[test]
    fn v2_counters_flags_and_snapshots_become_v3_and_the_step_is_idempotent() {
        let snapshot =
            |kind: &str| json!({"archetype_id": "x", "kind": kind, "npc_damage_dice": 1});
        let v2 = json!({
            "attempt": {
                "brace_left": 12,
                "profile": {
                    "brace": true,
                    "modifiers": {
                        "cadence_multiplier": 1.0, "piercing": true, "sweep": false,
                        "heal": false, "brace": true, "grenade_unlocked": true,
                        "molotov_unlocked": false
                    }
                },
                "enemies": [
                    {"hit_stop_left": 4, "resolved": snapshot("Runner")},
                    {"hit_stop_left": 0, "resolved": snapshot("Brute")},
                    {"hit_stop_left": 0, "resolved": null}
                ]
            },
            "battles": [{"game": {"attempt": {"brace_left": 0, "enemies": []}}}]
        });
        let v3 = migrate(v2, 2).unwrap();
        let a = &v3["attempt"];
        assert_eq!(
            a["statuses"],
            json!([{"id": "Brace", "remaining_ticks": 12, "stacks": 1, "magnitude": 0}])
        );
        assert!(a.get("brace_left").is_none());
        // Only skill modifiers lose their flags; the profile's own `brace` stays.
        assert_eq!(a["profile"]["brace"], true);
        assert_eq!(
            a["profile"]["modifiers"],
            json!({"cadence_multiplier": 1.0, "unlocks": ["brace", "grenade", "piercing"]})
        );
        assert_eq!(
            a["enemies"][0]["statuses"][0],
            json!({"id": "HitStop", "remaining_ticks": 4, "stacks": 1, "magnitude": 0})
        );
        assert_eq!(a["enemies"][0]["resolved"]["target_urgency"], -3.0);
        assert_eq!(a["enemies"][1]["resolved"]["target_urgency"], 0.0);
        assert_eq!(
            a["enemies"][1],
            json!({"resolved": {"archetype_id": "x", "kind": "Brute", "npc_damage_dice": 1, "target_urgency": 0.0}})
        );
        assert_eq!(v3["battles"][0]["game"]["attempt"], json!({"enemies": []}));
        assert_eq!(v2_to_v3(v3.clone()), v3);
    }

    #[test]
    fn frozen_v3_quality_order_matches_the_kind_that_replaced_it() {
        use crate::quality::Quality;
        assert_eq!(serde_names(&Quality::ALL), V3_QUALITIES);
    }

    #[test]
    fn v3_quality_arrays_become_maps_everywhere_and_the_step_is_idempotent() {
        let v3 = json!({
            "attempt": {"loot_quality_counts": [0, 2, 1, 0, 1, 3]},
            "campaign": {"run_history": [{"loot_by_quality": [1, 0, 0, 0, 0, 5]}]},
            "battles": [{"game": {"attempt": {"loot_quality_counts": [0, 0, 0, 9, 0, 0]}}}]
        });
        let v4 = migrate(v3, 3).unwrap();
        assert_eq!(
            v4["attempt"]["loot_quality_counts"],
            json!({"Poor": 0, "Common": 2, "Uncommon": 1, "Rare": 0, "Epic": 1, "Legendary": 3})
        );
        assert_eq!(
            v4["campaign"]["run_history"][0]["loot_by_quality"]["Legendary"],
            5
        );
        assert_eq!(
            v4["battles"][0]["game"]["attempt"]["loot_quality_counts"]["Rare"],
            9
        );
        assert_eq!(v3_to_v4(v4.clone()).unwrap(), v4);
        assert!(matches!(
            migrate(json!({"loot_by_quality": [1, 2]}), 3),
            Err(SaveError::Migration(_))
        ));
    }

    #[test]
    fn frozen_v4_ordnance_order_matches_the_kind_that_replaced_it() {
        use crate::kind_map::StableKind;
        use crate::ordnance::OrdnanceKind;
        assert_eq!(serde_names(OrdnanceKind::ALL), V4_ORDNANCE_KINDS);
    }

    #[test]
    fn v4_ordnance_arrays_become_maps_everywhere_and_the_step_is_idempotent() {
        let v4 = json!({
            "attempt": {"ordnance": {
                "projectiles": [], "next_id": 3,
                "cooldowns": [10, 61], "charges": [1, 4], "issued": [2, 5], "spent": [1, 1]
            }},
            "battles": [{"game": {"attempt": {"ordnance": {"charges": [0, 7]}}}}]
        });
        let v5 = migrate(v4, 4).unwrap();
        let o = &v5["attempt"]["ordnance"];
        assert_eq!(o["cooldowns"], json!({"Grenade": 10, "Molotov": 61}));
        assert_eq!(o["charges"], json!({"Grenade": 1, "Molotov": 4}));
        assert_eq!(o["issued"], json!({"Grenade": 2, "Molotov": 5}));
        assert_eq!(o["spent"], json!({"Grenade": 1, "Molotov": 1}));
        assert_eq!(o["next_id"], 3);
        assert_eq!(
            v5["battles"][0]["game"]["attempt"]["ordnance"]["charges"]["Molotov"],
            7
        );
        assert_eq!(v4_to_v5(v5.clone()).unwrap(), v5);
        assert!(matches!(
            migrate(json!({"ordnance": {"charges": [1]}}), 4),
            Err(SaveError::Migration(_))
        ));
        // Arrays with these names outside an ordnance state are not ordnance.
        let other = json!({"npc": {"spent": [1, 2, 3]}});
        assert_eq!(v4_to_v5(other.clone()).unwrap(), other);
    }
}
