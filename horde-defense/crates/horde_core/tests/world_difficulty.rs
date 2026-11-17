//! Country difficulty bands, the start clamp and what a regeneration of
//! world.json must leave alone (design-enemy-balance §8, ruling W-10; W2-39).
#![allow(clippy::print_stdout, clippy::print_stderr)] // CLI example / test diagnostics
mod support;
use horde_core::{
    command::Command,
    sim::Game,
    survivors::SurvivorId,
    world::{Country, WorldProgress, world},
};
use support::legacy_game;

/// FNV-1a 64, written out here so the test cross-checks the generator's
/// formula instead of reusing the crate's hash.
fn fnv1a64(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// `scripts/generate_world.py`: the band, in hundredths, by stage count.
fn band(stages: usize) -> (u64, u64) {
    match stages {
        0..=3 => (85, 115),
        4..=9 => (100, 135),
        _ => (120, 160),
    }
}

/// `difficulty = (lo + fnv1a64("difficulty:" + id) % (hi - lo + 1)) / 100`.
fn band_hundredths(country: &Country) -> u64 {
    let (lo, hi) = band(country.level_count());
    lo + fnv1a64(&format!("difficulty:{}", country.id)) % (hi - lo + 1)
}

fn raw(id: &str) -> f32 {
    world().country(id).unwrap().difficulty
}

#[test]
fn world_difficulty_matches_band_formula() {
    let countries = &world().countries;
    assert_eq!(countries.len(), 242);
    let mut hundredths = Vec::new();
    for country in countries {
        let expected = band_hundredths(country);
        // Both sides are the nearest f32 to expected / 100.
        assert_eq!(
            country.difficulty,
            expected as f32 / 100.0,
            "{} ({} stages): regenerate world.json with scripts/generate_world.py",
            country.id,
            country.level_count()
        );
        let (lo, hi) = band(country.level_count());
        assert!((lo..=hi).contains(&expected), "{}", country.id);
        hundredths.push(expected);
    }
    // The design's measured spread (design-enemy-balance §8).
    let distinct: std::collections::BTreeSet<_> = hundredths.iter().collect();
    assert_eq!(hundredths.iter().min(), Some(&85));
    assert_eq!(hundredths.iter().max(), Some(&160));
    assert_eq!(distinct.len(), 50);
    let mean = hundredths.iter().sum::<u64>() as f64 / hundredths.len() as f64 / 100.0;
    assert_eq!(format!("{mean:.3}"), "1.049");
    for (id, value) in [
        ("LUX", 0.92),
        ("DEU", 1.04),
        ("BEL", 0.97),
        ("USA", 1.27),
        ("CHN", 1.52),
        ("AUT", 0.85),
        ("BGD", 1.60),
    ] {
        assert_eq!(raw(id), value, "{id}");
    }
}

/// A campaign with a survivor and `start` chosen through the command path,
/// which deploys the start country's first stage.
fn started(start: &str) -> Game {
    let mut g = legacy_game();
    g.apply(&Command::ChooseSurvivor(SurvivorId::Rook)).unwrap();
    g.apply(&Command::ChooseStart(start.into())).unwrap();
    g
}

#[test]
fn start_country_difficulty_is_clamped() {
    // Raw values on both sides of the [0.95, 1.05] window, and one inside it.
    assert_eq!((raw("AUT"), raw("BGD"), raw("DEU")), (0.85, 1.60, 1.04));
    for (start, effective) in [("AUT", 0.95), ("BGD", 1.05), ("DEU", 1.04)] {
        let g = started(start);
        let before = serde_json::to_value(&g).unwrap();
        assert_eq!(
            g.effective_country_difficulty(start),
            Some(effective),
            "{start}"
        );
        // Other countries keep the raw value.
        for other in ["AUT", "BGD", "DEU", "LUX"] {
            if other != start {
                assert_eq!(
                    g.effective_country_difficulty(other),
                    Some(raw(other)),
                    "{other} beside start {start}"
                );
            }
        }
        assert_eq!(g.effective_country_difficulty("ZZZ"), None);
        assert_eq!(
            serde_json::to_value(&g).unwrap(),
            before,
            "the read is pure"
        );
        // The deployment terms use the effective value.
        let country = world().country(start).unwrap();
        let attempt = g.attempt().expect("ChooseStart deploys stage 0");
        assert_eq!(attempt.country_id.as_deref(), Some(start));
        assert_eq!(
            attempt.modifiers.enemy_health,
            country.modifiers.enemy_health * effective,
            "{start}"
        );
        assert_eq!(
            attempt.modifiers.reward,
            country.modifiers.reward * effective,
            "{start}"
        );
        assert_eq!(
            attempt.modifiers.enemy_speed,
            country.modifiers.enemy_speed * effective.sqrt(),
            "{start}"
        );
    }
    // Country::effective_difficulty reads the start from the progress.
    // Before a start is chosen every country is a candidate start, so the
    // start picker shows the clamped value its first defense plays.
    let mut progress = WorldProgress::default();
    let aut = world().country("AUT").unwrap();
    let bgd = world().country("BGD").unwrap();
    assert_eq!(aut.effective_difficulty(&progress), 0.95);
    assert_eq!(bgd.effective_difficulty(&progress), 1.05);
    assert_eq!(
        Game::default().effective_country_difficulty("BGD"),
        Some(1.05)
    );
    progress.choose_start("BGD").unwrap();
    assert_eq!(bgd.effective_difficulty(&progress), 1.05);
    assert_eq!(aut.effective_difficulty(&progress), 0.85);
}

/// Every key but `difficulty`, as one canonical JSON digest (serde_json
/// sorts object keys), so a regeneration can only ever move difficulty.
fn digest(value: &serde_json::Value) -> String {
    horde_core::sha256::digest(&serde_json::to_vec(value).unwrap())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn world_stage_themes_and_strategy_config_survive_regeneration() {
    let mut committed: serde_json::Value =
        serde_json::from_str(include_str!("../src/world.json")).unwrap();
    // The pins were taken on the W2-39 parent (30fbfd7), before the bands.
    assert_eq!(
        digest(&committed["stage_themes"]),
        STAGE_THEMES_SHA256,
        "stage_themes"
    );
    assert_eq!(
        digest(&committed["strategy_config"]),
        STRATEGY_CONFIG_SHA256,
        "strategy_config"
    );
    for country in committed["countries"].as_array_mut().unwrap() {
        assert!(
            country
                .as_object_mut()
                .unwrap()
                .remove("difficulty")
                .is_some(),
            "every country has a difficulty"
        );
    }
    assert_eq!(
        digest(&committed),
        EVERYTHING_BUT_DIFFICULTY_SHA256,
        "a key other than difficulty moved"
    );
}
const STAGE_THEMES_SHA256: &str =
    "a2e0880df9ed09eec821cd28d2b5e517816a46a4531a5c184ce8108fd004de02";
const STRATEGY_CONFIG_SHA256: &str =
    "746006388a33eb8ff01ae6cfbec453dd09754186e74a62c03a197b84de2515eb";
const EVERYTHING_BUT_DIFFICULTY_SHA256: &str =
    "933dc67227c9186d898736726f1e3015e18b02fbada367c21b01610b75f1be04";
