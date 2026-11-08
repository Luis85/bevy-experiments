//! Two-axis threat (W2-26; design-enemy-balance §1 and §3, rulings W-5, W-6,
//! W-9 and E3, schema v7 §3.10). Axis 1 is the threat rank; axis 2 is the
//! stage's place on its country's front (`FrontPosition`), which ramps
//! enemy health and arrival inside `Campaign::progression_for` and names the
//! stage's boss rank. The preset extras fold into the snapshot there, and the
//! boss damage multiplier grows with rank past 4 at the two boss read sites.
mod support;
use horde_core::balance::{BalancePreset, BalanceSettings, PresetExtras};
use horde_core::campaign::Campaign;
use horde_core::command::Command;
use horde_core::content::{Mode, content};
use horde_core::progression::{
    BossRank, FrontPosition, ProgressionCatalog, ProgressionSnapshot, catalog,
};
use horde_core::sim::{DEFENSE_LINE, Game};
use horde_core::world::world;
use serde_json::Value;
use std::collections::BTreeSet;

const MODES: [Mode; 2] = [Mode::Personal, Mode::Npc];

/// Every key a legacy `ProgressionSnapshot` writes (the W1-28 list).
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

/// A fresh campaign on `preset`.
fn on(preset: BalancePreset) -> Campaign {
    let mut c = Campaign::default();
    c.set_balance_settings(BalanceSettings::for_preset(preset))
        .unwrap();
    c
}

/// A Custom campaign edited from Hard (its extras basis is Hard).
fn custom_from_hard() -> Campaign {
    let mut c = on(BalancePreset::Hard);
    let mut settings = BalanceSettings::for_preset(BalancePreset::Hard);
    settings.start_lanes = 5;
    c.set_balance_settings(settings).unwrap();
    assert_eq!(c.balance_settings.preset, BalancePreset::Custom);
    assert_eq!(c.balance_extras_preset(), BalancePreset::Hard);
    c
}

/// Every preset a campaign can deploy on, Custom included.
fn every_preset() -> Vec<(&'static str, Campaign)> {
    vec![
        ("Easy", on(BalancePreset::Easy)),
        ("Normal", on(BalancePreset::Normal)),
        ("Hard", on(BalancePreset::Hard)),
        ("Custom from Hard", custom_from_hard()),
    ]
}

fn at(c: &Campaign, mode: Mode, rank: u32, front: Option<FrontPosition>) -> ProgressionSnapshot {
    c.progression_for(mode, rank, c.lane_count(), front)
        .unwrap()
}

fn front(stage: u32, count: u32) -> Option<FrontPosition> {
    Some(FrontPosition { stage, count })
}

/// Enemy health times arrival: the encounter pressure the ramp scales.
fn pressure(s: &ProgressionSnapshot) -> f64 {
    f64::from(s.enemy_health) * f64::from(s.arrival_multiplier)
}

/// `(id, stage count)` of every catalogue country with at least 2 stages.
fn multi_stage_countries() -> Vec<(&'static str, u32)> {
    world()
        .countries
        .iter()
        .map(|c| (c.id.as_str(), c.level_count() as u32))
        .filter(|(_, count)| *count >= 2)
        .collect()
}

fn keys(text: &str) -> BTreeSet<String> {
    let value: Value = serde_json::from_str(text).unwrap();
    value.as_object().unwrap().keys().cloned().collect()
}

#[test]
fn front_ramp_final_over_first_normal_is_at_least_1_35() {
    let c = on(BalancePreset::Normal);
    let countries = multi_stage_countries();
    assert!(countries.len() > 100, "{} countries", countries.len());
    for (id, count) in countries {
        for rank in [0, 5, 10, 20, 40] {
            for mode in MODES {
                let first = at(&c, mode, rank, front(0, count));
                let last = at(&c, mode, rank, front(count - 1, count));
                assert_eq!(first.rank, last.rank, "equal rank");
                let ratio = pressure(&last) / pressure(&first);
                assert!(
                    ratio >= 1.35,
                    "{id} ({count} stages) rank {rank} {mode:?}: final/first {ratio}"
                );
                // The ruling's Normal value: (1 + 0.18)^2.
                assert!((ratio - 1.3924).abs() < 1e-4, "{id} rank {rank}: {ratio}");
            }
        }
    }
}

#[test]
fn front_ramp_orders_hard_over_normal_over_easy() {
    let count = world().country("USA").unwrap().level_count() as u32;
    let presets = [
        on(BalancePreset::Hard),
        on(BalancePreset::Normal),
        on(BalancePreset::Easy),
    ];
    for stage in 1..count {
        let ramps = presets
            .each_ref()
            .map(|c| at(c, Mode::Personal, 10, front(stage, count)).front_ramp);
        assert!(
            ramps[0] > ramps[1] && ramps[1] > ramps[2] && ramps[2] > 1.0,
            "stage {stage}: {ramps:?}"
        );
    }
    // Final/first pressure at equal rank: Hard 1.27^2, Normal 1.18^2, Easy
    // 1.135^2 (ruling W-5).
    for (c, expected) in presets.iter().zip([1.6129, 1.3924, 1.288225]) {
        for mode in MODES {
            let first = at(c, mode, 20, front(0, count));
            let last = at(c, mode, 20, front(count - 1, count));
            let ratio = pressure(&last) / pressure(&first);
            assert!((ratio - expected).abs() < 1e-4, "{mode:?}: {ratio}");
        }
    }
    // A Custom edit from Hard keeps Hard's ramp.
    let custom = custom_from_hard();
    for stage in 1..count {
        assert_eq!(
            at(&custom, Mode::Npc, 10, front(stage, count)).front_ramp,
            at(&presets[0], Mode::Npc, 10, front(stage, count)).front_ramp,
            "stage {stage}"
        );
    }
}

#[test]
fn boss_damage_multiplier_follows_rank() {
    let cfg = catalog();
    assert_eq!(cfg.boss_damage_per_rank, 0.01);
    assert_eq!(cfg.front_ramp_max, 0.18);
    let at_rank = |rank| cfg.snapshot(rank).boss_damage_multiplier;
    for (rank, expected) in [
        (5, 1.01),
        (6, 1.02),
        (10, 1.06),
        (20, 1.16),
        (40, 1.36),
        (50, 1.46),
        (60, 1.46),
        (1000, 1.46),
    ] {
        let value = at_rank(rank);
        assert!(
            (value - expected).abs() < 1e-5,
            "rank {rank}: {value} (expected {expected})"
        );
    }
    for rank in 5..=70 {
        assert!(at_rank(rank) >= at_rank(rank - 1), "monotone at {rank}");
    }
    // The campaign snapshot carries it on every preset, mode and stage.
    let usa = world().country("USA").unwrap().level_count() as u32;
    for (name, c) in every_preset() {
        for mode in MODES {
            for position in [None, front(0, usa), front(5, usa), front(usa - 1, usa)] {
                assert_eq!(
                    at(&c, mode, 20, position).boss_damage_multiplier,
                    at_rank(20),
                    "{name} {mode:?} {position:?}"
                );
            }
        }
    }
    // Catalogue bounds: boss_damage_per_rank 0..=0.05, front_ramp_max 0..=0.5.
    let bundled: Value = serde_json::from_str(include_str!("../progression.json")).unwrap();
    let with = |key: &str, value: f64| {
        let mut catalogue = bundled.clone();
        catalogue[key] = value.into();
        ProgressionCatalog::from_json_str(&catalogue.to_string())
    };
    for (key, ceiling) in [("boss_damage_per_rank", 0.05), ("front_ramp_max", 0.5)] {
        with(key, ceiling).unwrap_or_else(|e| panic!("{key} = {ceiling}: {e}"));
        with(key, 0.0).unwrap_or_else(|e| panic!("{key} = 0: {e}"));
        for bad in [ceiling + 0.01, -0.01] {
            assert!(with(key, bad).is_err(), "{key} = {bad} accepted");
        }
    }
}

/// The first hit points boss contact takes from a Personal survivor with the
/// attempt's `boss_damage_multiplier` at `multiplier`. A regular enemy when
/// `boss` is false.
fn first_personal_contact_damage(multiplier: f32, boss: bool) -> (i32, i32) {
    let mut g = support::game(Mode::Personal);
    g.attempt_mut().unwrap().progression.boss_damage_multiplier = multiplier;
    support::enemy(&mut g, 5, DEFENSE_LINE + 0.001, boss);
    let a = g.attempt().unwrap();
    let base = if boss {
        content().levels[a.level].boss_damage
    } else {
        a.enemies[0].resolved.as_ref().unwrap().damage
    };
    let scale = a.progression.enemy_damage * a.balance.enemy_damage;
    let expected = ((base as f32 * scale * if boss { multiplier } else { 1.0 }).round() as i32
        - a.profile.personal.armor)
        .max(0);
    let start = a.hp;
    for _ in 0..600 {
        support::advance(&mut g, 1);
        let hp = g.attempt().unwrap().hp;
        if hp < start {
            return (start - hp, expected);
        }
    }
    panic!("no contact damage in 600 ticks");
}

#[test]
fn personal_boss_contact_damage_scales_with_boss_damage_multiplier() {
    let (neutral, expected) = first_personal_contact_damage(1.0, true);
    assert_eq!(neutral, expected);
    let (doubled, expected) = first_personal_contact_damage(2.0, true);
    assert_eq!(doubled, expected);
    assert!(doubled > neutral, "{doubled} > {neutral}");
    let (rank_50, expected) = first_personal_contact_damage(1.46, true);
    assert_eq!(rank_50, expected);
    // Regular enemies never read the boss multiplier.
    assert_eq!(
        first_personal_contact_damage(4.0, false).0,
        first_personal_contact_damage(1.0, false).0
    );
}

/// `damage N` of a rules-resolution line: the basic damage before DR.
fn basic_damage(line: &str) -> i32 {
    let rest = line.split("; damage ").nth(1).unwrap();
    rest.split(' ').next().unwrap().parse().unwrap()
}

/// The tick and basic damage of the boss's first NPC hit on the survivor,
/// with the attempt's `boss_damage_multiplier` at `multiplier`.
fn first_npc_boss_hit(multiplier: f32) -> (u64, i32) {
    let mut g = support::game(Mode::Npc);
    g.attempt_mut().unwrap().progression.boss_damage_multiplier = multiplier;
    support::enemy(&mut g, 5, DEFENSE_LINE + 0.001, true);
    // The survivor fights back; the boss outlasts it until its first hit.
    let e = &mut g.attempt_mut().unwrap().enemies[0];
    (e.hp, e.max_hp, e.injury.hp, e.injury.max_hp) = (100_000, 100_000, 100_000, 100_000);
    let boss = e.id;
    for _ in 0..6_000 {
        support::advance(&mut g, 1);
        let a = g.attempt().expect("the attempt runs until the boss hits");
        assert!(
            a.enemies.iter().any(|e| e.id == boss),
            "the boss died before it hit"
        );
        if let Some(line) = a
            .log
            .iter()
            .find(|line| line.contains("→ survivor") && basic_damage(line) > 0)
        {
            return (a.tick, basic_damage(line));
        }
    }
    panic!("no boss hit in 6000 ticks");
}

#[test]
fn npc_boss_attack_adds_include_boss_damage_term() {
    // floor((m - 1) x 6) joins the boss's adds (ruling W-6); the dice and
    // every earlier draw are the same, so only the adds differ.
    let (tick, neutral) = first_npc_boss_hit(1.0);
    for (multiplier, term) in [(1.5, 3), (2.0, 6), (1.16, 0), (4.0, 18)] {
        assert_eq!(
            first_npc_boss_hit(multiplier),
            (tick, neutral + term),
            "multiplier {multiplier}"
        );
    }
}

#[test]
fn boss_rank_derivation_usa_24() {
    let usa = world().country("USA").unwrap();
    assert_eq!(usa.level_count(), 24);
    let ranks: Vec<BossRank> = (0..24)
        .map(|stage| usa.front_position(stage).boss_rank())
        .collect();
    let expected: Vec<BossRank> = (0..24)
        .map(|stage| match stage {
            23 => BossRank::Capstone,
            5 | 11 | 17 => BossRank::Regional,
            _ => BossRank::Sector,
        })
        .collect();
    assert_eq!(ranks, expected);
    assert_eq!(usa.regional_checkpoints(), vec![5, 11, 17]);
    assert!(usa.front_position(5).is_checkpoint());
    assert!(!usa.front_position(23).is_checkpoint());
    assert_eq!(
        usa.front_position(23),
        FrontPosition {
            stage: 23,
            count: 24
        }
    );
    // The Capstone wins over a sixth stage; a one-stage country is its
    // Capstone at depth 0.
    let single = FrontPosition { stage: 0, count: 1 };
    assert_eq!(single.boss_rank(), BossRank::Capstone);
    assert_eq!(single.depth(), 0.0);
    assert_eq!(
        FrontPosition { stage: 5, count: 6 }.boss_rank(),
        BossRank::Capstone
    );
    assert_eq!(
        FrontPosition {
            stage: 0,
            count: 24
        }
        .depth(),
        0.0
    );
    assert_eq!(
        FrontPosition {
            stage: 23,
            count: 24
        }
        .depth(),
        1.0
    );

    // Deploying stamps the stage's boss rank into the frozen snapshot, and
    // the report and run record copy it; no country is a Sector boss.
    let mut g = support::country_game("USA");
    g.campaign_mut()
        .world
        .country_levels
        .insert("USA".into(), 23);
    for (stage, rank) in [
        (0, BossRank::Sector),
        (5, BossRank::Regional),
        (11, BossRank::Regional),
        (23, BossRank::Capstone),
    ] {
        g.deploy_country_stage(stage, Mode::Personal).unwrap();
        let a = g.attempt().unwrap();
        assert_eq!(a.progression.boss_rank, rank, "stage {stage}");
        let ramp = 1.0 + 0.18 * stage as f32 / 23.0;
        assert!(
            (a.progression.front_ramp - ramp).abs() < 1e-6,
            "stage {stage}: {}",
            a.progression.front_ramp
        );
        g.apply(&Command::Withdraw).unwrap();
        assert_eq!(g.last_result().unwrap().boss_rank, rank, "stage {stage}");
        assert_eq!(
            g.campaign().run_history.last().unwrap().boss_rank,
            rank,
            "stage {stage}"
        );
    }
    let mut g = support::legacy_game();
    g.deploy(0, Mode::Personal).unwrap();
    assert_eq!(g.attempt().unwrap().progression.boss_rank, BossRank::Sector);
}

#[test]
fn regional_checkpoint_distribution_is_219_12_4_7() {
    let mut distribution = [0; 4];
    for country in &world().countries {
        let checkpoints = country.regional_checkpoints();
        assert!(checkpoints.len() <= 3, "{}", country.id);
        let count = country.level_count();
        for stage in &checkpoints {
            assert!((stage + 1) % 6 == 0 && stage + 1 < count, "{}", country.id);
        }
        distribution[checkpoints.len()] += 1;
    }
    assert_eq!(distribution, [219, 12, 4, 7]);
}

/// Pin: without a country the ramp is the literal 1.0 and changes nothing;
/// stage 0 of a multi-stage front is the same encounter.
#[test]
fn front_ramp_is_exactly_one_without_country() {
    let one = 1.0f32.to_bits();
    let counts: BTreeSet<u32> = multi_stage_countries().into_iter().map(|c| c.1).collect();
    for (name, c) in every_preset() {
        for mode in MODES {
            for rank in 0..=60 {
                let none = at(&c, mode, rank, None);
                assert_eq!(none.front_ramp.to_bits(), one, "{name} {mode:?} {rank}");
                assert_eq!(none.boss_rank, BossRank::Sector);
                for count in &counts {
                    let first = at(&c, mode, rank, front(0, *count));
                    assert_eq!(first, none, "{name} {mode:?} rank {rank}, {count} stages");
                }
            }
        }
    }
    // A deploy without a country (GOLDEN and smoke) freezes the neutral ramp.
    let mut g = Game::default();
    g.campaign_mut().attempts = 1;
    g.deploy(0, Mode::Personal).unwrap();
    let p = &g.attempt().unwrap().progression;
    assert_eq!(p.front_ramp.to_bits(), one);
    assert_eq!(
        *p,
        g.campaign()
            .progression_for(
                Mode::Personal,
                g.campaign().progression_rank(),
                g.campaign().lane_count(),
                None
            )
            .unwrap()
    );
}

/// Pin: ranks 0-4 keep the literal 1.0 boss damage, on every preset, mode
/// and stage, and a catalogue without the key keeps it at every rank.
#[test]
fn boss_damage_multiplier_is_exactly_one_through_rank_4() {
    let one = 1.0f32.to_bits();
    let usa = world().country("USA").unwrap().level_count() as u32;
    for (name, c) in every_preset() {
        for mode in MODES {
            for rank in 0..=4 {
                for stage in [None, front(0, usa), front(5, usa), front(usa - 1, usa)] {
                    let value = at(&c, mode, rank, stage).boss_damage_multiplier;
                    assert_eq!(value.to_bits(), one, "{name} {mode:?} {rank} {stage:?}");
                }
            }
        }
    }
    let mut bundled: Value = serde_json::from_str(include_str!("../progression.json")).unwrap();
    bundled
        .as_object_mut()
        .unwrap()
        .remove("boss_damage_per_rank");
    let legacy = ProgressionCatalog::from_json_str(&bundled.to_string()).unwrap();
    for rank in 0..=100 {
        assert_eq!(
            legacy.snapshot(rank).boss_damage_multiplier.to_bits(),
            one,
            "rank {rank}"
        );
    }
}

/// Pin: ranks 0-4 at depth 0 serialize only the legacy snapshot keys on
/// every preset (no ramp, no boss rank, no boss damage, no extras), plus
/// the elite rate from rank 3 (W2-33, design enemy-balance §5), and
/// round-trip byte-identically. Depth 0 here means no country or stage 0
/// of a multi-stage front. A one-stage front is also at depth 0, but its
/// only stage is its Capstone (ruling W-9), so it adds exactly the
/// `boss_rank` key and, since W2-36, the Capstone's one boss phase
/// transition (`boss_phases` 1, design enemy-balance §6); that exception is
/// pinned below, not skipped.
#[test]
fn neutral_snapshot_serializes_no_new_keys_for_ranks_0_to_4_on_every_preset_at_depth_0() {
    let expected: BTreeSet<String> = SNAPSHOT_KEYS.into_iter().map(String::from).collect();
    let counts: BTreeSet<u32> = multi_stage_countries().into_iter().map(|c| c.1).collect();
    let fronts = std::iter::once(None).chain(counts.iter().map(|count| front(0, *count)));
    for position in fronts {
        for (name, c) in every_preset() {
            for mode in MODES {
                for rank in 0..=4 {
                    let text = serde_json::to_string(&at(&c, mode, rank, position)).unwrap();
                    assert_eq!(
                        keys(&text),
                        with_elites(&expected, rank),
                        "{name} {mode:?} rank {rank} {position:?}"
                    );
                    let restored: ProgressionSnapshot = serde_json::from_str(&text).unwrap();
                    assert_eq!(serde_json::to_string(&restored).unwrap(), text);
                }
            }
        }
    }
    let mut capstone = expected.clone();
    capstone.insert("boss_rank".into());
    capstone.insert("boss_phases".into());
    for (name, c) in every_preset() {
        for mode in MODES {
            for rank in 0..=4 {
                let snapshot = at(&c, mode, rank, front(0, 1));
                let text = serde_json::to_string(&snapshot).unwrap();
                assert_eq!(
                    keys(&text),
                    with_elites(&capstone, rank),
                    "{name} {mode:?} rank {rank} one stage"
                );
                assert!(
                    text.contains("\"boss_rank\":\"Capstone\",\"boss_phases\":1"),
                    "{name} {mode:?} rank {rank}: {text}"
                );
                let restored: ProgressionSnapshot = serde_json::from_str(&text).unwrap();
                assert_eq!(serde_json::to_string(&restored).unwrap(), text);
            }
        }
    }
}

/// `keys` plus `elite_rate_bp` from rank 3, where elites start (W2-33).
fn with_elites(keys: &BTreeSet<String>, rank: u32) -> BTreeSet<String> {
    let mut keys = keys.clone();
    if rank >= 3 {
        keys.insert("elite_rate_bp".into());
    }
    keys
}

/// Pin: the country-stage render fixtures (and the practice scenes built
/// on them) carry their stage's front ramp since W2-26. Both deploy stage
/// 4 of the active country on Normal, so the ramp is
/// `1 + 0.18 x 4 / (count - 1)`; peak's USA front (24 stages) gives
/// about 1.031 on enemy health and arrival.
#[test]
fn render_fixtures_carry_their_stage_front_ramp() {
    let fixtures = [
        ("peak", horde_core::sim::peak_render_fixture().unwrap()),
        (
            "progressed-peak",
            horde_core::sim::maximum_progression_render_fixture().unwrap(),
        ),
    ];
    for (name, g) in fixtures {
        let country = g.campaign().world.active_country().unwrap();
        let count = country.level_count() as f32;
        let progression = &g.attempt().unwrap().progression;
        let ramp = 1.0 + 0.18 * 4.0 / (count - 1.0);
        assert!(
            (progression.front_ramp - ramp).abs() < 1e-6,
            "{name} on {}: {} (expected {ramp})",
            country.id,
            progression.front_ramp
        );
        assert_eq!(progression.boss_rank, BossRank::Sector, "{name}");
    }
}

/// The extras fold (ruling E3): the elite rate scales by the preset's
/// integer percent (capped at 10000 bp) and the phase bonus adds past the
/// rank-4 phase gate (capped at 2); Normal's extras change nothing.
#[test]
fn preset_extras_fold_into_the_snapshot() {
    let staged = |rank: u32| ProgressionSnapshot {
        rank,
        enemy_cap: 72,
        elite_rate_bp: 1_000,
        boss_phases: 1,
        ..ProgressionSnapshot::default()
    };
    let fold = |rank, extras: PresetExtras| {
        let mut s = staged(rank);
        s.fold_extras(extras);
        (s.elite_rate_bp, s.boss_phases)
    };
    let extras = |elite_rate_pct, boss_phase_bonus| PresetExtras {
        elite_rate_pct,
        front_ramp_pct: 100,
        boss_phase_bonus,
    };
    assert_eq!(fold(10, PresetExtras::default()), (1_000, 1));
    assert_eq!(fold(10, extras(50, 0)), (500, 1));
    assert_eq!(fold(10, extras(150, 1)), (1_500, 2));
    assert_eq!(fold(10, extras(300, 2)), (3_000, 2));
    assert_eq!(fold(4, extras(150, 1)), (1_500, 1), "no bonus at rank 4");
    let mut capped = staged(30);
    capped.elite_rate_bp = 9_000;
    capped.fold_extras(extras(300, 0));
    assert_eq!(capped.elite_rate_bp, 10_000);
    // On the bundled catalogue the shipped presets fold into the deploy
    // snapshot: past rank 4 the rank term gives one transition (W2-36) and
    // Hard's bonus a second; nothing at ranks 0-4.
    for rank in [0, 4, 5, 20] {
        let hard = at(&on(BalancePreset::Hard), Mode::Personal, rank, None);
        assert_eq!(hard.boss_phases, 2 * u8::from(rank > 4), "rank {rank}");
        for preset in [BalancePreset::Easy, BalancePreset::Normal] {
            assert_eq!(
                at(&on(preset), Mode::Personal, rank, None).boss_phases,
                u8::from(rank > 4)
            );
        }
    }
}

/// Pin: a checkpoint stage keeps the stage theme it had (no theme change,
/// ruling W-9). Every Regional and Capstone stage of every country, as id,
/// stage, theme id, lighting, weather and variation.
#[test]
fn stage_theme_with_is_unchanged_on_checkpoint_stages() {
    let mut lines = String::new();
    let mut checkpoints = 0;
    for country in &world().countries {
        for stage in 0..country.level_count() {
            if country.front_position(stage).boss_rank() == BossRank::Sector {
                continue;
            }
            checkpoints += 1;
            let theme = country.stage_theme_with(stage, world());
            assert_eq!(theme, country.stage_theme(stage));
            lines.push_str(&format!(
                "{} {stage} {} {} {} {}\n",
                country.id, theme.id, theme.lighting, theme.weather, theme.variation
            ));
        }
    }
    assert_eq!(checkpoints, 242 + 12 + 2 * 4 + 3 * 7);
    let usa: Vec<String> = [5, 11, 17, 23]
        .map(|stage| world().country("USA").unwrap().stage_theme(stage).id)
        .to_vec();
    assert_eq!(usa, STAGE_THEMES_USA);
    assert_eq!(
        support::fnv1a(lines.as_bytes()),
        CHECKPOINT_THEMES_FNV,
        "a checkpoint stage's theme moved:\n{lines}"
    );
}

/// USA's Regional stages 5, 11 and 17 and its Capstone 23.
const STAGE_THEMES_USA: [&str; 4] = ["roadside", "suburb", "waterfront", "industrial"];
/// FNV-1a of the checkpoint theme lines, recorded on the parent's theme code
/// (W2-26 changes no theme).
const CHECKPOINT_THEMES_FNV: u64 = 408_677_868_466_915_946;
