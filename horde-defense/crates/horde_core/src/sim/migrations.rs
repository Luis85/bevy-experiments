//! Ordered save-format migrations on raw JSON, applied before typed
//! deserialization. Each step freezes the meaning of its source version, so a
//! migration never consults the currently loaded catalogues.
use super::persistence::SaveError;
use serde_json::{Map, Value};

/// Envelope version written by this build.
pub const SAVE_VERSION: u32 = 2;

/// Upgrade a payload saved at envelope `from` to [`SAVE_VERSION`].
pub(super) fn migrate(mut value: Value, from: u32) -> Result<Value, SaveError> {
    if from == 0 || from > SAVE_VERSION {
        return Err(SaveError::UnsupportedVersion(from));
    }
    for version in from..SAVE_VERSION {
        value = match version {
            1 => v1_to_v2(value)?,
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
}
