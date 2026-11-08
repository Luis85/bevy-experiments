//! Axis 1 of the threat past rank 4 (W2-32; design-enemy-balance Â§2,
//! "Growth knee", "Floors and composition tiers" and "Boss armor"). Enemy
//! health and damage grow on a steeper slope past the knee at rank 4, so
//! the gear/HP ratio stops widening; `pressure_floors` rows 5..=28 carry
//! the composition tiers up to 11 and the boss floors up; and bosses gain
//! armor with rank in both modes. Ranks 0-4 are unchanged.
mod support;
use horde_core::balance::{BalancePreset, BalanceSettings};
use horde_core::campaign::Campaign;
use horde_core::content::{Mode, content};
use horde_core::progression::{FrontPosition, ProgressionCatalog, catalog};
use horde_core::sim::DEFENSE_LINE;
use horde_core::world::world;
use serde_json::Value;
use std::collections::BTreeSet;

const MODES: [Mode; 2] = [Mode::Personal, Mode::Npc];

fn bundled() -> Value {
    serde_json::from_str(include_str!("../progression.json")).unwrap()
}

/// The gear damage multiplier a shop item carries at threat `rank`:
/// `1 + damage_per_level Ã— (item_level - 1)`, the item level being the
/// snapshot's `min(max_item_level, 1 + rank / item_level_rank_step)`.
fn gear(cfg: &ProgressionCatalog, rank: u32) -> f64 {
    let level = cfg.snapshot(rank).item_level;
    1.0 + f64::from(cfg.gear.damage_per_level) * f64::from(level - 1)
}

#[test]
fn snapshot_wave_tiers_at_10_20_40_are_7_10_11() {
    let cfg = catalog();
    let tier = |rank| cfg.snapshot(rank).wave_tier;
    assert_eq!([tier(10), tier(20), tier(40)], [7, 10, 11]);
    // The whole design map: ranks 0-4 keep tiers 0-4, then 4, 5-5, 6-6,
    // 7-7-7, 8-8-8, 9 x4, 10 x8, and 11 from rank 28 on.
    let expected = |rank: u32| match rank {
        0..=4 => rank as usize,
        5 => 4,
        6..=7 => 5,
        8..=9 => 6,
        10..=12 => 7,
        13..=15 => 8,
        16..=19 => 9,
        20..=27 => 10,
        _ => 11,
    };
    for rank in 0..=60 {
        assert_eq!(tier(rank), expected(rank), "rank {rank}");
    }
    // Rows 5..=28 follow the design's floor formulas; the index is the rank.
    assert_eq!(cfg.pressure_floors.len(), 29);
    for rank in 5..=28u32 {
        let floor = &cfg.pressure_floors[rank as usize];
        let steps = (rank - 4) as f32;
        let row = format!("rank {rank}: {floor:?}");
        assert!(
            (floor.start_rate - (0.44 + 0.0025 * steps)).abs() < 1e-6,
            "{row}"
        );
        assert!(
            (floor.rate_cap - (1.76 + 0.01 * steps)).abs() < 1e-6,
            "{row}"
        );
        assert_eq!(floor.boss_health, 6800 + 600 * (rank as i32 - 4), "{row}");
        assert_eq!(floor.npc_boss_health, 67 + 2 * (rank as i32 - 4), "{row}");
        assert_eq!(floor.boss_time, 240.0, "{row}");
    }
    assert_eq!(cfg.pressure_floors[28].boss_health, 21_200);
    // The campaign snapshot carries the floor's tier on every preset, mode
    // and stage.
    let usa = world().country("USA").unwrap().level_count() as u32;
    for preset in [
        BalancePreset::Easy,
        BalancePreset::Normal,
        BalancePreset::Hard,
    ] {
        let mut c = Campaign::default();
        c.set_balance_settings(BalanceSettings::for_preset(preset))
            .unwrap();
        for mode in MODES {
            for front in [
                None,
                Some(FrontPosition {
                    stage: usa - 1,
                    count: usa,
                }),
            ] {
                let tiers = [10, 20, 40].map(|rank| {
                    c.progression_for(mode, rank, c.lane_count(), front)
                        .unwrap()
                        .wave_tier
                });
                assert_eq!(tiers, [7, 10, 11], "{preset:?} {mode:?} {front:?}");
            }
        }
    }
}

#[test]
fn growth_matches_gear_within_band() {
    let cfg = catalog();
    assert_eq!(cfg.growth_knee_rank, 4);
    assert_eq!(cfg.health_per_rank_late, 0.04);
    assert_eq!(cfg.damage_per_rank_late, 0.018);
    let health = |rank| f64::from(cfg.snapshot(rank).enemy_health);
    // From the knee on, shop gear outgrows enemy health by 5-15% at most
    // (the design table: 1.11 at rank 4, 1.07 at 5, 1.12 from 10 to 58).
    for rank in 4..=58 {
        let ratio = gear(cfg, rank) / health(rank);
        assert!(
            (1.05..=1.15).contains(&ratio),
            "rank {rank}: gear {} / enemy health {} = {ratio}",
            gear(cfg, rank),
            health(rank)
        );
    }
    for (rank, expected) in [
        (4, 1.11),
        (5, 1.07),
        (10, 1.12),
        (20, 1.12),
        (40, 1.12),
        (58, 1.12),
    ] {
        let ratio = gear(cfg, rank) / health(rank);
        assert!((ratio - expected).abs() < 0.006, "rank {rank}: {ratio}");
    }
    // The snapshot formulas: `1 + per_rank Ã— min(r, k) + late Ã— max(r - k, 0)`.
    for rank in 0..=200u32 {
        let (early, late) = (rank.min(4) as f64, rank.saturating_sub(4) as f64);
        let s = cfg.snapshot(rank);
        let want_health = 1.0 + 0.015 * early + 0.04 * late;
        let want_damage = 1.0 + 0.008 * early + 0.018 * late;
        assert!(
            (f64::from(s.enemy_health) - want_health).abs() < 1e-4,
            "rank {rank}"
        );
        assert!(
            (f64::from(s.enemy_damage) - want_damage).abs() < 1e-4,
            "rank {rank}"
        );
    }
    // HP x arrival against gear (design Â§2): .99 / 1.07 / 1.25 at ranks
    // 10 / 20 / 40.
    for (rank, expected) in [(10, 0.99), (20, 1.07), (40, 1.25)] {
        let s = cfg.snapshot(rank);
        let pressure = health(rank) * f64::from(s.arrival_multiplier) / gear(cfg, rank);
        assert!(
            (pressure - expected).abs() < 0.006,
            "rank {rank}: {pressure}"
        );
    }
    // Threat growth still scales the whole growth term on the campaign
    // snapshot (Easy 0.8, Hard 1.25).
    for (preset, growth) in [(BalancePreset::Easy, 0.8), (BalancePreset::Hard, 1.25)] {
        let mut c = Campaign::default();
        c.set_balance_settings(BalanceSettings::for_preset(preset))
            .unwrap();
        let s = c
            .progression_for(Mode::Personal, 20, c.lane_count(), None)
            .unwrap();
        let want = 1.0 + (health(20) - 1.0) * growth;
        assert!(
            (f64::from(s.enemy_health) - want).abs() < 1e-4,
            "{preset:?}"
        );
    }
    // A catalogue without the knee keys (a legacy LASTLIGHT_PROGRESSION
    // override) keeps the single slope bit for bit at every rank.
    let mut legacy = bundled();
    for key in [
        "growth_knee_rank",
        "health_per_rank_late",
        "damage_per_rank_late",
    ] {
        assert!(
            legacy.as_object_mut().unwrap().remove(key).is_some(),
            "{key}"
        );
    }
    let legacy = ProgressionCatalog::from_json_str(&legacy.to_string()).unwrap();
    for rank in 0..=200u32 {
        let s = legacy.snapshot(rank);
        assert_eq!(
            s.enemy_health.to_bits(),
            (1.0 + rank as f32 * 0.015).to_bits()
        );
        assert_eq!(
            s.enemy_damage.to_bits(),
            (1.0 + rank as f32 * 0.008).to_bits()
        );
    }
    // Catalogue bounds: the late slopes 0..=0.25 like the early ones.
    let with = |key: &str, value: Value| {
        let mut catalogue = bundled();
        catalogue[key] = value;
        ProgressionCatalog::from_json_str(&catalogue.to_string())
    };
    for key in ["health_per_rank_late", "damage_per_rank_late"] {
        with(key, 0.25.into()).unwrap_or_else(|e| panic!("{key} = 0.25: {e}"));
        with(key, 0.0.into()).unwrap_or_else(|e| panic!("{key} = 0: {e}"));
        for bad in [0.26, -0.01] {
            assert!(with(key, bad.into()).is_err(), "{key} = {bad} accepted");
        }
    }
}

/// The armor of a boss spawned in `mode` at threat `rank` on level 0 (a
/// legacy campaign, which fields the companion without a liberation).
fn boss_armor(mode: Mode, rank: u32) -> i32 {
    let mut g = support::legacy_game();
    g.campaign_mut().attempts = 1;
    g.campaign_mut().victories = u64::from(rank);
    assert_eq!(g.campaign().progression_rank(), rank);
    g.deploy(0, mode).unwrap();
    support::enemy(&mut g, 5, DEFENSE_LINE + 4.0, true);
    let a = g.attempt().unwrap();
    assert_eq!(a.progression.rank, rank);
    let boss = a.enemies.iter().find(|e| e.boss).expect("the boss spawned");
    boss.armor
}

#[test]
fn rank_20_boss_armor_exceeds_rank_0_both_modes() {
    let level = &content().levels[0];
    for mode in MODES {
        let template = match mode {
            Mode::Personal => level.boss_armor,
            Mode::Npc => level.boss_npc_armor,
        };
        assert!(template > 0, "{mode:?}");
        // Ranks 0-4 keep the template armor.
        for rank in 0..=4 {
            assert_eq!(boss_armor(mode, rank), template, "{mode:?} rank {rank}");
        }
        let rank_20 = boss_armor(mode, 20);
        assert!(
            rank_20 > boss_armor(mode, 0),
            "{mode:?}: rank 20 armor {rank_20}"
        );
    }
    // The design values: Personal x(1 + 0.02 x 16) rounded, NPC +16/8.
    assert_eq!(
        boss_armor(Mode::Personal, 20),
        (20.0f32 * 1.32).round() as i32
    );
    assert_eq!(boss_armor(Mode::Npc, 20), level.boss_npc_armor + 2);
    // The snapshot fields: `1 + .02 Ã— (min(r, 50) - 4)` in 1..=4 and
    // `(r - 4) / 8` in 0..=20.
    let cfg = catalog();
    for (rank, multiplier, add) in [
        (0, 1.0, 0),
        (4, 1.0, 0),
        (5, 1.02, 0),
        (11, 1.14, 0),
        (12, 1.16, 1),
        (20, 1.32, 2),
        (50, 1.92, 5),
        (60, 1.92, 7),
        (1000, 1.92, 20),
    ] {
        let s = cfg.snapshot(rank);
        assert!(
            (s.boss_armor_multiplier - multiplier).abs() < 1e-5,
            "rank {rank}: {}",
            s.boss_armor_multiplier
        );
        assert_eq!(s.npc_boss_armor_add, add, "rank {rank}");
    }
    // A catalogue without the boss armor keys keeps bosses at the template.
    let mut legacy = bundled();
    for key in ["boss_armor_per_rank", "npc_boss_armor_rank_step"] {
        assert!(
            legacy.as_object_mut().unwrap().remove(key).is_some(),
            "{key}"
        );
    }
    let legacy = ProgressionCatalog::from_json_str(&legacy.to_string()).unwrap();
    for rank in 0..=100 {
        let s = legacy.snapshot(rank);
        assert_eq!(
            s.boss_armor_multiplier.to_bits(),
            1.0f32.to_bits(),
            "{rank}"
        );
        assert_eq!(s.npc_boss_armor_add, 0, "{rank}");
    }
}

/// Every snapshot ranks 0-4 can deploy with: the raw catalogue snapshot and
/// the campaign snapshot on every preset (a Custom edit from Hard and the
/// legacy balance included), in both modes, without a country and on the
/// first, a Regional and the Capstone stage of USA.
fn ranks_zero_to_four_lines() -> String {
    let mut presets: Vec<(&str, Campaign)> = Vec::new();
    for (name, preset) in [
        ("Easy", BalancePreset::Easy),
        ("Normal", BalancePreset::Normal),
        ("Hard", BalancePreset::Hard),
    ] {
        let mut c = Campaign::default();
        c.set_balance_settings(BalanceSettings::for_preset(preset))
            .unwrap();
        presets.push((name, c));
    }
    let mut custom = presets[2].1.clone();
    let mut settings = BalanceSettings::for_preset(BalancePreset::Hard);
    settings.start_lanes = 5;
    custom.set_balance_settings(settings).unwrap();
    presets.push(("Custom from Hard", custom));
    let mut legacy = Campaign::default();
    legacy.balance_settings = BalanceSettings::legacy();
    presets.push(("legacy", legacy));
    let usa = world().country("USA").unwrap().level_count() as u32;
    let fronts = [None, Some(0), Some(5), Some(usa - 1)]
        .map(|stage| stage.map(|stage| FrontPosition { stage, count: usa }));
    let mut lines = String::new();
    for rank in 0..=4 {
        let raw = serde_json::to_string(&catalog().snapshot(rank)).unwrap();
        lines.push_str(&format!("catalogue {rank} {raw}\n"));
        for (name, c) in &presets {
            for mode in MODES {
                for front in fronts {
                    let s = c
                        .progression_for(mode, rank, c.lane_count(), front)
                        .unwrap();
                    let text = serde_json::to_string(&s).unwrap();
                    lines.push_str(&format!("{name} {mode:?} {rank} {front:?} {text}\n"));
                }
            }
        }
    }
    lines
}

/// Pin: every rank 0-4 snapshot serializes byte for byte as on the parent
/// of W2-32 (the knee sits at rank 4 and rows 0-4 are frozen), apart from
/// the one key W2-33 adds on purpose: elites roll from rank 3 (design
/// enemy-balance §5), so ranks 3 and 4 write `elite_rate_bp` (200 and 265
/// bp on Normal, times the preset's percent). W2-36 adds the other: a
/// Capstone boss gains one phase transition on every preset (ruling W-9,
/// design enemy-balance §6), so the Capstone lines, and only they, write
/// `boss_phases` 1. With both keys cut out the lines hash to the W2-32
/// value, and the keys are pinned on their own.
#[test]
fn ranks_zero_to_four_snapshot_pinned() {
    let lines = ranks_zero_to_four_lines();
    assert_eq!(lines.lines().count(), 5 * (1 + 5 * 2 * 4));
    const KEY: &str = ",\"elite_rate_bp\":";
    const PHASES: &str = ",\"boss_phases\":1";
    let mut rates = BTreeSet::new();
    let mut stripped = String::new();
    let mut capstones = 0;
    for line in lines.lines() {
        let capstone = line.contains("\"boss_rank\":\"Capstone\"");
        assert_eq!(line.contains(PHASES), capstone, "{line}");
        assert_eq!(line.matches("boss_phases").count(), usize::from(capstone));
        capstones += usize::from(capstone);
        let owned = line.replacen(PHASES, "", 1);
        let line = owned.as_str();
        // The JSON object, after the front position's Debug braces.
        let json = &line[line.find("{\"").unwrap()..];
        let value: Value = serde_json::from_str(json).unwrap();
        let rank = value["rank"].as_u64().unwrap();
        match line.find(KEY) {
            Some(start) => {
                let rest = &line[start + KEY.len()..];
                let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
                rates.insert((rank, rest[..digits].parse::<u32>().unwrap()));
                stripped.push_str(&line[..start]);
                stripped.push_str(&rest[digits..]);
            }
            None => {
                assert!(rank < 3, "rank {rank} without an elite rate: {line}");
                stripped.push_str(line);
            }
        }
        stripped.push('\n');
    }
    // Easy 50%, Normal (the catalogue and the legacy balance) 100%, Hard
    // and Custom from Hard 150%.
    let expected = [(3, 100), (3, 200), (3, 300), (4, 132), (4, 265), (4, 397)];
    assert_eq!(rates, BTreeSet::from(expected));
    assert_eq!(
        capstones,
        5 * 5 * 2,
        "a Capstone front per rank, preset and mode"
    );
    assert_eq!(
        support::fnv1a(stripped.as_bytes()),
        RANKS_ZERO_TO_FOUR_FNV,
        "a rank 0-4 snapshot moved:\n{lines}"
    );
}

/// FNV-1a of [`ranks_zero_to_four_lines`], recorded on the parent of W2-32.
const RANKS_ZERO_TO_FOUR_FNV: u64 = 1_345_734_136_285_417_498;
