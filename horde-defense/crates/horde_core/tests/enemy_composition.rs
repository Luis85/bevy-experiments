//! W2-31 (design enemy-balance §4 "Records" and "Waves 5-11", rulings P1
//! and W-14): the composition catalogue grows from 5 to 12 wave tiers, the
//! 7 new archetype records appear only from tier 5 on, tiers 0-4 stay
//! byte-identical, and every progression floor names an authored tier.
mod support;
use horde_core::{
    catalog::init_catalogs,
    content::{BossKind, Mode},
    enemy_catalog::enemy_catalog,
    progression::{self, MAX_WAVE_TIER, ProgressionCatalog},
};

const ENEMY_CATALOG_JSON: &str = include_str!("../enemy_catalog.json");
const PROGRESSION_JSON: &str = include_str!("../progression.json");

/// The archetypes W2-31 appends (6 plus the splitter's child).
const NEW_ARCHETYPES: [&str; 7] = [
    "exploder",
    "splitter",
    "splitter-brood",
    "spitter",
    "shield-bearer",
    "sprinter",
    "juggernaut",
];

/// Design enemy-balance §4 "Waves 5-11": mean Personal HP per pick with the
/// boss variants locked and with all unlocked, tiers 4..=11.
const MEAN_HP: [(usize, f32, f32); 8] = [
    (4, 51.7, 68.7),
    (5, 53.3, 69.4),
    (6, 55.4, 70.4),
    (7, 67.6, 79.6),
    (8, 69.1, 80.4),
    (9, 74.6, 84.5),
    (10, 78.3, 87.4),
    (11, 84.0, 91.9),
];

const ALL_BOSSES: [BossKind; 3] = [BossKind::Breacher, BossKind::Caller, BossKind::SiegeBrute];

#[test]
fn catalogue_has_twelve_tiers_with_monotone_mean_health() {
    let catalog = enemy_catalog();
    assert_eq!(catalog.wave_count(), 12, "tiers 0..=11");
    for (tier, locked, all) in MEAN_HP {
        let mean = |unlocked: &[BossKind]| {
            (catalog.mean_pool_health(tier, unlocked, Mode::Personal) * 10.0).round() / 10.0
        };
        assert_eq!(mean(&[]), locked, "tier {tier}, boss variants locked");
        assert_eq!(mean(&ALL_BOSSES), all, "tier {tier}, all unlocked");
    }
    for unlocked in [&[][..], &ALL_BOSSES[..]] {
        let personal: Vec<f32> = (0..12)
            .map(|tier| catalog.mean_pool_health(tier, unlocked, Mode::Personal))
            .collect();
        assert!(
            personal.windows(2).all(|p| p[1] >= p[0]),
            "Personal mean HP per pick never falls: {personal:?}"
        );
        // NPC hit points dip from tier 3 to the frozen tier 4 (10.46 to
        // 10.35 with the variants locked); the new tiers never fall.
        let npc: Vec<f32> = (4..12)
            .map(|tier| catalog.mean_pool_health(tier, unlocked, Mode::Npc))
            .collect();
        assert!(
            npc.windows(2).all(|p| p[1] >= p[0]),
            "NPC mean HP per pick never falls from tier 4: {npc:?}"
        );
    }
    // The new archetypes appear only in waves 5 and above, and every one but
    // the splitter's child is in some wave.
    for tier in 0..12 {
        let pool = catalog.wave_pool(tier, &ALL_BOSSES);
        for id in NEW_ARCHETYPES {
            let present = pool.iter().any(|entry| entry.archetype_id == id);
            if tier < 5 || id == "splitter-brood" {
                assert!(!present, "{id} in tier {tier}");
            }
        }
    }
    for id in NEW_ARCHETYPES.iter().filter(|id| **id != "splitter-brood") {
        assert!(
            (5..12).any(|tier| catalog
                .wave_pool(tier, &[])
                .iter()
                .any(|entry| entry.archetype_id == *id)),
            "{id} joins a wave"
        );
    }
    assert!(!catalog.elite_eligible("splitter-brood"));
    assert!(catalog.elite_eligible("juggernaut"));
    // Levels past the last tier still wrap (only the floors are checked).
    assert_eq!(catalog.wave_pool(12, &[]), catalog.wave_pool(0, &[]));
}

/// FNV-1a of the canonical JSON of the authored wave tiers 0..=4, as the
/// parent of W2-31 ships them.
const WAVES_0_TO_4_FNV: u64 = 0x7d54_f126_a23a_1725;

#[test]
fn waves_0_to_4_byte_identical() {
    let authored: serde_json::Value = serde_json::from_str(ENEMY_CATALOG_JSON).unwrap();
    let waves = authored["waves"].as_array().unwrap();
    let frozen: Vec<&serde_json::Value> = waves
        .iter()
        .filter(|wave| wave["level"].as_u64().unwrap() <= 4)
        .collect();
    assert_eq!(frozen.len(), 5);
    let text = serde_json::to_string(&frozen).unwrap();
    assert_eq!(
        support::fnv1a(text.as_bytes()),
        WAVES_0_TO_4_FNV,
        "wave tiers 0..=4 are frozen (append-only):\n{text}"
    );
}

#[test]
fn every_floor_tier_exists_in_enemy_catalog() {
    // The bundled catalogues pass the strict load, which runs the check.
    init_catalogs().expect("bundled catalogues are valid");
    progression::catalog()
        .check_wave_tiers(enemy_catalog())
        .unwrap();
    // A floor may name the last authored tier...
    let with_last_tier = |tier: usize| {
        let mut value: serde_json::Value = serde_json::from_str(PROGRESSION_JSON).unwrap();
        let floors = value["pressure_floors"].as_array_mut().unwrap();
        let mut floor = floors.last().unwrap().clone();
        floor["wave_tier"] = tier.into();
        floors.push(floor);
        ProgressionCatalog::from_json_str(&value.to_string()).unwrap()
    };
    let last = enemy_catalog().wave_count() - 1;
    assert_eq!(last, 11);
    with_last_tier(last)
        .check_wave_tiers(enemy_catalog())
        .unwrap();
    // ...but not a tier past it, though MAX_WAVE_TIER allows it.
    for tier in last + 1..=MAX_WAVE_TIER {
        let message = with_last_tier(tier)
            .check_wave_tiers(enemy_catalog())
            .unwrap_err();
        assert!(message.contains(&format!("wave tier {tier}")), "{message}");
    }
}

/// Set in the child process of
/// `strict_load_rejects_a_progression_override_past_the_last_wave_tier`.
const STRICT_LOAD_CHILD: &str = "HORDE_TEST_STRICT_LOAD_CHILD";

/// W2 review: `init_catalogs` itself runs the floor check. A process whose
/// `LASTLIGHT_PROGRESSION` override names a floor one tier past the enemy
/// catalogue (a catalogue that parses and validates on its own) fails the
/// strict load as an invalid progression catalogue. The catalogues are
/// process-wide, so the load runs in a fresh child process of this test.
#[test]
fn strict_load_rejects_a_progression_override_past_the_last_wave_tier() {
    if std::env::var_os(STRICT_LOAD_CHILD).is_some() {
        let error = init_catalogs().expect_err("a floor past the last wave tier");
        let horde_core::catalog::ContentError::Invalid { catalog, message } = &error else {
            panic!("not an invalid catalogue: {error}");
        };
        assert_eq!(*catalog, "progression", "{error}");
        assert!(message.contains("wave tier 12"), "{message}");
        return;
    }
    let mut value: serde_json::Value = serde_json::from_str(PROGRESSION_JSON).unwrap();
    let floors = value["pressure_floors"].as_array_mut().unwrap();
    let mut floor = floors.last().unwrap().clone();
    let tier = enemy_catalog().wave_count();
    assert_eq!(tier, 12);
    floor["wave_tier"] = tier.into();
    floors.push(floor);
    let text = value.to_string();
    ProgressionCatalog::from_json_str(&text).expect("the override is valid on its own");
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("strict-load-floor-past-last-wave-tier.json");
    std::fs::write(&path, text).unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child.args([
        "strict_load_rejects_a_progression_override_past_the_last_wave_tier",
        "--exact",
        "--nocapture",
        "--test-threads=1",
    ]);
    for (_, var) in horde_core::catalog::CATALOG_OVERRIDES {
        child.env_remove(var);
    }
    let output = child
        .env("LASTLIGHT_PROGRESSION", &path)
        .env(STRICT_LOAD_CHILD, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "the child's strict load accepted the override:\n{stdout}\n{stderr}"
    );
}
