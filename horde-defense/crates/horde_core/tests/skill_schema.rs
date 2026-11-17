//! Skill schema at neutral values (W1-26): the saved `SkillModifiers` fields
//! `class_modifiers`, `armor_penetration` and `hazard_damage_multiplier`
//! inside `Attempt.profile`. Since W2-41 the shipped nodes 170-177 author
//! them; these tests still stage the values as a save would, so the schema
//! is pinned apart from the shipped tree.
mod support;
use horde_core::{
    content::Mode,
    sim::{Game, SAVE_VERSION, SaveError},
    skill_catalog::{ClassModifiers, SkillModifiers},
    weapons::WeaponClass,
};
use serde_json::{Value, json};

const FIXTURE_V6: &str = include_str!("fixtures/save_v6.json");
const NEW_KEYS: [&str; 3] = [
    "class_modifiers",
    "armor_penetration",
    "hazard_damage_multiplier",
];
const BOUNDS: &str = "Saved skill modifiers exceed supported finite bounds";

/// A live Personal attempt as saved text parses (f32 values print shortest).
fn live_attempt() -> Value {
    as_saved(&support::game(Mode::Personal))
}

fn as_saved(game: &Game) -> Value {
    serde_json::from_str(&serde_json::to_string(game).unwrap()).unwrap()
}

fn load(value: Value) -> Result<Game, SaveError> {
    Game::from_payload(value, SAVE_VERSION)
}

/// `live_attempt()` with `edit` applied to its saved skill modifiers.
fn with_modifiers(edit: impl FnOnce(&mut Value)) -> Value {
    let mut value = live_attempt();
    edit(&mut value["attempt"]["profile"]["modifiers"]);
    value
}

fn perk(damage: f64, reload: f64, cadence: f64, weak: f64) -> Value {
    json!({
        "damage_multiplier": damage,
        "reload_multiplier": reload,
        "cadence_multiplier": cadence,
        "weak_bonus_add": weak,
    })
}

fn neutral(modifiers: &SkillModifiers) -> bool {
    modifiers.class_modifiers.is_empty()
        && modifiers.armor_penetration == 0
        && modifiers.hazard_damage_multiplier.to_bits() == 1.0f32.to_bits()
}

#[test]
fn saved_skill_modifiers_reject_nan_class_multipliers_and_penetration_over_1000() {
    // Neutral modifiers write none of the new keys.
    let plain = live_attempt();
    let saved = plain["attempt"]["profile"]["modifiers"]
        .as_object()
        .unwrap();
    for key in NEW_KEYS {
        assert!(!saved.contains_key(key), "{key} written at neutral");
    }
    // In-range values load and round-trip byte-identically.
    let staged = with_modifiers(|m| {
        m["class_modifiers"] = json!({
            "Shotgun": perk(1.0, 0.8, 0.8, 0.0),
            "Marksman": perk(1.15, 1.0, 1.0, 0.15),
        });
        m["armor_penetration"] = json!(1000);
        m["hazard_damage_multiplier"] = json!(0.01);
    });
    let game = load(staged.clone()).unwrap_or_else(|e| panic!("staged modifiers: {e}"));
    assert_eq!(as_saved(&game), staged);
    let modifiers = &game.attempt().unwrap().profile.modifiers;
    assert_eq!(
        modifiers.class_modifiers[&WeaponClass::Shotgun],
        ClassModifiers {
            reload_multiplier: 0.8,
            cadence_multiplier: 0.8,
            ..ClassModifiers::default()
        }
    );
    assert_eq!(modifiers.armor_penetration, 1000);
    // Out-of-range rows fail on the modifier bound itself. 1e39 is beyond
    // f32, so it loads as infinity (JSON cannot carry NaN; see below).
    let rows = [
        ("penetration 1001", json!({"armor_penetration": 1001})),
        ("penetration negative", json!({"armor_penetration": -1})),
        (
            "class damage infinite",
            json!({"class_modifiers": {"Rifle": perk(1e39, 1.0, 1.0, 0.0)}}),
        ),
        (
            "class reload 0",
            json!({"class_modifiers": {"Smg": perk(1.0, 0.0, 1.0, 0.0)}}),
        ),
        (
            "class cadence above 100",
            json!({"class_modifiers": {"Pistol": perk(1.0, 1.0, 100.5, 0.0)}}),
        ),
        (
            "weak bonus above 1",
            json!({"class_modifiers": {"Marksman": perk(1.0, 1.0, 1.0, 1.5)}}),
        ),
        (
            "weak bonus negative",
            json!({"class_modifiers": {"Marksman": perk(1.0, 1.0, 1.0, -0.1)}}),
        ),
        ("hazard above 1", json!({"hazard_damage_multiplier": 1.5})),
        ("hazard 0", json!({"hazard_damage_multiplier": 0.0})),
        ("hazard infinite", json!({"hazard_damage_multiplier": 1e39})),
    ];
    let passed: Vec<String> = rows
        .into_iter()
        .filter_map(|(row, fields)| {
            let value = with_modifiers(|m| {
                for (key, field) in fields.as_object().unwrap() {
                    m[key] = field.clone();
                }
            });
            match load(value) {
                Err(SaveError::Invalid(message)) if message == BOUNDS => None,
                Ok(_) => Some(format!("{row}: loaded")),
                Err(error) => Some(format!("{row}: {error}")),
            }
        })
        .collect();
    assert_eq!(passed, Vec::<String>::new(), "rows that did not fail");
    // NaN cannot be saved as JSON, but the in-memory validation rejects it in
    // every class field and in the hazard multiplier.
    let nan = f32::NAN;
    let class_rows = [
        ClassModifiers {
            damage_multiplier: nan,
            ..ClassModifiers::default()
        },
        ClassModifiers {
            reload_multiplier: nan,
            ..ClassModifiers::default()
        },
        ClassModifiers {
            cadence_multiplier: nan,
            ..ClassModifiers::default()
        },
        ClassModifiers {
            weak_bonus_add: nan,
            ..ClassModifiers::default()
        },
    ];
    for (n, class) in class_rows.into_iter().enumerate() {
        let mut modifiers = SkillModifiers::default();
        modifiers
            .class_modifiers
            .insert(WeaponClass::MachineGun, class);
        assert_eq!(modifiers.validate(), Err(BOUNDS.into()), "class row {n}");
    }
    let modifiers = SkillModifiers {
        hazard_damage_multiplier: nan,
        ..SkillModifiers::default()
    };
    assert_eq!(modifiers.validate(), Err(BOUNDS.into()));
}

#[test]
fn v6_attempts_validate_with_neutral_skill_modifiers() {
    let game = Game::decode_save_generation(FIXTURE_V6.as_bytes()).expect("v6 fixture loads");
    let mut attempts = vec![game.attempt().expect("v6 root attempt")];
    attempts.extend(game.battles().iter().filter_map(|b| b.game().attempt()));
    assert!(attempts.len() >= 2, "root and a live expedition");
    for attempt in attempts {
        assert!(neutral(&attempt.profile.modifiers));
    }
    let saved = serde_json::to_string(&game).unwrap();
    for key in NEW_KEYS {
        assert!(!saved.contains(key), "{key} written for a v6 attempt");
    }
}
