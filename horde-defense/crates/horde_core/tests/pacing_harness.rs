//! Pacing harness v2 (W2-01): the profile registry, `stop_after`, the
//! threaded sweep and the milestone statistics; the goal-5 evaluators
//! (W2-03) and the goal-6 rows and evaluators (W2-04) on synthetic reports.
use horde_core::balance::BalancePreset;
use horde_core::content::{Mode, Slot, content};
use horde_core::defenders::DefenderId;
use horde_core::sim::{
    ArchetypeRun, DEFAULT_PROFILE_ID, DefeatWindow, GOAL5_FIRST_VICTORY, GOAL5_TIER_WINDOWS,
    GOAL6_BOSS_TTK_S, GOAL6_CLAMPED_FIRST_VICTORY, GOAL6_DEFEAT_RATE_PCT, GOAL6_LUX_EASY_MAX_RUNS,
    GOAL6_LUX_HARD_OVER_NORMAL, GOAL6_LUX_PROFILE_IDS, GOAL6_START_PROFILE_IDS,
    GOAL6_STREAK_FROM_RUN, GOAL6_STREAK_LIMIT, GOAL6_WINDOW_RUNS, Game, LEGACY_PROFILE_IDS,
    Milestone, MilestoneStats, Outcome, PacingAim, PacingDefender, PacingFarm, PacingPlan,
    PacingProfile, PacingReport, PacingRun, PacingSell, PacingSpend, archetype_coverage,
    boss_ttk_median_s, by_id, clamped_start_first_victory, defeat_rate_windows, defeat_windows,
    equip_dropped_upgrades, longest_win_streak, max_win_streak, median_run_within, node_retired,
    nodes_left, pacing_registry, pacing_seed, pacing_sweep_with_threads, runs_to_liberate,
    simulate_pacing, tier_affordable_runs, tree_exhausted, tree_outlasts_liberation, unlock_order,
};
use horde_core::skill_catalog::{CampaignFeature, SkillNode, skills};
use horde_core::world::world;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn pacing_registry_ids_are_unique_and_default_is_expert_deu() {
    let ids: Vec<&str> = pacing_registry().iter().map(|p| p.id).collect();
    let unique: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "duplicate pacing ids in {ids:?}");
    assert_eq!(DEFAULT_PROFILE_ID, "expert-deu");
    let default = by_id(DEFAULT_PROFILE_ID).expect("the default profile is registered");
    assert_eq!(default.id, "expert-deu");
    assert_eq!(default.name, "expert, tree first, Germany");
    assert_eq!(default.country, "DEU");
    assert_eq!(default.aim, PacingAim::Expert);
    assert_eq!(default.seed, 0x1A571167);
    assert_eq!(default.max_runs, 60);
    assert_eq!(default.stop_after, None);
    // The default table prints these three, in this order.
    assert_eq!(
        LEGACY_PROFILE_IDS,
        ["expert-deu", "novice-deu", "novice-gear-lux"]
    );
    for id in LEGACY_PROFILE_IDS {
        assert!(by_id(id).is_some(), "legacy profile {id} is registered");
    }
    assert!(by_id("no-such-profile").is_none());
    // Seed index 0 is the profile's own seed, so one seed reproduces the
    // legacy table; later indices differ from it and from each other.
    assert_eq!(pacing_seed(default.seed, 0), default.seed);
    let seeds: BTreeSet<u64> = (0..20).map(|i| pacing_seed(default.seed, i)).collect();
    assert_eq!(seeds.len(), 20);
}

#[test]
fn stop_after_liberation_ends_the_profile_after_that_run() {
    let profile = PacingProfile {
        max_runs: 20,
        stop_after: Some(Milestone::Liberation(1)),
        ..by_id("novice-gear-lux").expect("registered")
    };
    let report = simulate_pacing(&profile).expect("profile runs");
    let (run, _) = report
        .milestone(Milestone::Liberation(1))
        .expect("Luxembourg is liberated inside 20 runs");
    assert!(run < 20, "liberated at run {run}");
    assert_eq!(report.runs.len(), run, "no run is played after the stop");
    assert_eq!(report.runs.last().map(|r| r.index), Some(run));
    // Post-run milestones are counted in runs completed: the liberating run
    // is the last one and its report shows the country freed.
    assert!(report.runs[run - 1].country_freed);
    assert!(report.runs[..run - 1].iter().all(|r| !r.country_freed));
}

#[test]
fn operations_open_is_the_first_camp_count_with_operations_available() {
    let profile = PacingProfile {
        max_runs: 20,
        stop_after: Some(Milestone::OperationsOpen),
        ..by_id("novice-gear-lux").expect("registered")
    };
    let report = simulate_pacing(&profile).expect("profile runs");
    let (open, _) = report
        .milestone(Milestone::OperationsOpen)
        .expect("the Operations page opens inside 20 runs");
    // The solo plan never recruits or sends an expedition, so the page opens
    // with the first of the three features behind it.
    let first_feature = [
        CampaignFeature::Recruitment,
        CampaignFeature::Parallel,
        CampaignFeature::Operations,
    ]
    .into_iter()
    .map(|f| report.milestone_or_missing(Milestone::Feature(f)))
    .min();
    assert_eq!(Some(open), first_feature);
    // A camp milestone: checked before run `open + 1`, the last run played.
    assert_eq!(report.runs.len(), open + 1);
}

#[test]
fn pacing_sweep_is_order_independent_of_thread_count() {
    let profiles: Vec<PacingProfile> = ["novice-gear-lux", DEFAULT_PROFILE_ID]
        .into_iter()
        .map(|id| PacingProfile {
            max_runs: 1,
            ..by_id(id).expect("registered")
        })
        .collect();
    let one = pacing_sweep_with_threads(&profiles, 3, 1).expect("sweep runs");
    let many = pacing_sweep_with_threads(&profiles, 3, 4).expect("sweep runs");
    assert_eq!(one, many);
    let order: Vec<(&str, usize)> = many.iter().map(|r| (r.profile.id, r.seed_index)).collect();
    assert_eq!(
        order,
        [
            ("novice-gear-lux", 0),
            ("novice-gear-lux", 1),
            ("novice-gear-lux", 2),
            ("expert-deu", 0),
            ("expert-deu", 1),
            ("expert-deu", 2),
        ]
    );
    for r in &many {
        assert_eq!(r.seed, pacing_seed(r.profile.seed, r.seed_index));
        assert!(r.runs.iter().all(|run| run.seed == r.seed));
    }
    // Seed index 0 is exactly the single-profile simulation.
    assert_eq!(
        many[3],
        simulate_pacing(&profiles[1]).expect("profile runs")
    );
}

fn synthetic(max_runs: usize, run: Option<usize>) -> PacingReport {
    PacingReport {
        profile: PacingProfile {
            max_runs,
            ..by_id(DEFAULT_PROFILE_ID).expect("registered")
        },
        seed_index: 0,
        seed: 0,
        runs: vec![],
        milestones: run
            .map(|r| (Milestone::FirstVictory, r, 0.0))
            .into_iter()
            .collect(),
    }
}

#[test]
fn milestone_stats_counts_missing_as_max_runs_plus_one() {
    let reports = [
        synthetic(10, Some(5)),
        synthetic(10, None),
        synthetic(10, Some(2)),
        synthetic(10, None),
    ];
    // Sorted: 2, 5, 11, 11. Lower median is index (n-1)/2 = 1.
    assert_eq!(
        MilestoneStats::of(&reports, Milestone::FirstVictory),
        Some(MilestoneStats {
            median: 5,
            p25: 2,
            p75: 11,
            reached: 2,
            seeds: 4,
        })
    );
    // With every seed missing, each statistic is max_runs + 1.
    let none = MilestoneStats::of(&reports[1..2], Milestone::FirstVictory).expect("one seed");
    assert_eq!((none.median, none.p25, none.p75), (11, 11, 11));
    assert_eq!((none.reached, none.seeds), (0, 1));
    assert_eq!(none.cell(10), ">10 [>10, >10] 0/1");
    let all = MilestoneStats::of(&reports, Milestone::FirstVictory).expect("four seeds");
    assert_eq!(all.cell(10), "5 [2, >10] 2/4");
    assert_eq!(MilestoneStats::of(&[], Milestone::FirstVictory), None);
}

// Goal-5 evaluators (W2-03) on synthetic reports. The release acceptance
// runs the same evaluators on the expert-deu sweep (tests/pacing_targets.rs).

const FORT: Milestone = Milestone::Feature(CampaignFeature::Construction);
const MARA: Milestone = Milestone::Feature(CampaignFeature::Companion);
const OPS: Milestone = Milestone::OperationsOpen;

/// A run of a synthetic report that learned `learned` nodes in camp.
fn camp_run(index: usize, learned: usize) -> PacingRun {
    PacingRun {
        index,
        seed: 0,
        difficulty: BalancePreset::Normal,
        country: "DEU".to_owned(),
        stage: 0,
        outcome: Outcome::Victory,
        country_freed: false,
        elapsed: 60.0,
        coins: 0,
        level_grant_coins: 0,
        milestone_coins: 0,
        cumulative_coins: 0,
        rank: 0,
        level: 1,
        lanes: 3,
        defender: DefenderId::Player,
        kills: 0,
        learned: (0..learned).map(|i| format!("node_{index}_{i}")).collect(),
        spent_tree: 0,
        spent_gear: 0,
        spent_training: 0,
        spent_recruit: 0,
        auto_sold_count: 0,
        auto_sold_coins: 0,
        nodes_owned: 0,
        nodes_left: 0,
        wallet: 0,
        boss_ttk_s: None,
        archetypes: BTreeMap::new(),
    }
}

/// Seed `seed_index` of a 30-run expert-deu sweep with these milestones,
/// learning `learned_per_run` nodes before every run.
fn seed_report(
    seed_index: usize,
    milestones: &[(Milestone, usize)],
    learned_per_run: usize,
) -> PacingReport {
    PacingReport {
        profile: PacingProfile {
            max_runs: 30,
            ..by_id(DEFAULT_PROFILE_ID).expect("registered")
        },
        seed_index,
        seed: pacing_seed(0x1A571167, seed_index),
        runs: (1..=30).map(|i| camp_run(i, learned_per_run)).collect(),
        milestones: milestones.iter().map(|&(m, run)| (m, run, 0.0)).collect(),
    }
}

fn unlocks(seed_index: usize, fort: usize, mara: usize, ops: usize) -> PacingReport {
    seed_report(seed_index, &[(FORT, fort), (MARA, mara), (OPS, ops)], 1)
}

#[test]
fn unlock_order_rejects_mara_before_fortifications() {
    let good = [
        unlocks(0, 3, 14, 23),
        unlocks(1, 3, 13, 20),
        unlocks(2, 4, 14, 22),
    ];
    assert!(unlock_order(&good).is_ok(), "{:?}", unlock_order(&good));
    // Seed 1 opens Mara (run 5) before Fortifications (run 10). The medians
    // (3 / 14 / 22) still rise, so only the per-seed rule catches it.
    let early_mara = [
        unlocks(0, 3, 14, 23),
        unlocks(1, 10, 5, 20),
        unlocks(2, 4, 14, 22),
    ];
    let err = unlock_order(&early_mara).expect_err("Mara before Fortifications in seed 1");
    assert!(
        err.contains("seed 1: Fortifications 10, Mara 5, Operations 20 out of order"),
        "{err}"
    );
    assert!(!err.contains("seed 0") && !err.contains("seed 2"), "{err}");
    assert!(!err.contains("median"), "the medians rise: {err}");
    // A seed that never builds Fortifications but opens Mara counts the
    // missing unlock as max_runs + 1, so it fails too.
    let no_fort = [
        unlocks(0, 3, 14, 23),
        seed_report(1, &[(MARA, 5), (OPS, 9)], 1),
        unlocks(2, 4, 14, 22),
    ];
    let err = unlock_order(&no_fort).expect_err("Fortifications missing in seed 1");
    assert!(err.contains("seed 1: Fortifications >30, Mara 5"), "{err}");
    assert!(unlock_order(&[]).is_err(), "no reports is not a pass");
}

#[test]
fn unlock_order_rejects_same_run_mara_and_operations_median() {
    // Per seed Fortifications <= Mara <= Operations holds, but Mara and
    // Operations share the median run 14 (the wave-0 shape): medians must
    // rise strictly.
    let same_run = [
        unlocks(0, 3, 14, 14),
        unlocks(1, 3, 13, 15),
        unlocks(2, 4, 14, 14),
    ];
    let err = unlock_order(&same_run).expect_err("Mara and Operations share run 14");
    assert!(
        err.contains("median Mara run 14 is not before median Operations run 14"),
        "{err}"
    );
    assert!(
        !err.contains("seed"),
        "no seed breaks the per-seed rule: {err}"
    );
    let rising = [
        unlocks(0, 3, 13, 15),
        unlocks(1, 3, 13, 15),
        unlocks(2, 4, 14, 16),
    ];
    assert!(unlock_order(&rising).is_ok(), "{:?}", unlock_order(&rising));
}

#[test]
fn tree_outlasts_rejects_exhaustion_before_second_liberation() {
    let lib2 = Milestone::Liberation(2);
    let exhausted = Milestone::TreeExhausted;
    let outlasts = [
        seed_report(0, &[(lib2, 17), (exhausted, 25)], 1),
        seed_report(1, &[(lib2, 16), (exhausted, 24)], 1),
        seed_report(2, &[(lib2, 18)], 1),
    ];
    assert!(
        tree_outlasts_liberation(&outlasts, 2).is_ok(),
        "{:?}",
        tree_outlasts_liberation(&outlasts, 2)
    );
    // The tree runs out (median run 15) before the second liberation
    // (median run 17).
    let early = [
        seed_report(0, &[(lib2, 17), (exhausted, 15)], 1),
        seed_report(1, &[(lib2, 16), (exhausted, 14)], 1),
        seed_report(2, &[(lib2, 18), (exhausted, 20)], 1),
    ];
    let err = tree_outlasts_liberation(&early, 2).expect_err("exhausted at 15, liberated at 17");
    assert!(
        err.contains("Tree exhausted at median run 15, not after Liberation(2) at median run 17"),
        "{err}"
    );
    // A seed that never reaches the second liberation fails and is named.
    let missing = [
        seed_report(0, &[(lib2, 17), (exhausted, 25)], 1),
        seed_report(1, &[(exhausted, 26)], 1),
        seed_report(2, &[(lib2, 18), (exhausted, 27)], 1),
    ];
    let err = tree_outlasts_liberation(&missing, 2).expect_err("seed 1 never liberates twice");
    assert!(
        err.contains("seed 1 never reaches Liberation(2) in 30 runs"),
        "{err}"
    );
    assert!(!err.contains("not after"), "the medians still order: {err}");
    // Nothing bought after the second liberation fails even when the tree
    // is never exhausted.
    let idle = [
        seed_report(0, &[(lib2, 17)], 0),
        seed_report(1, &[(lib2, 16)], 0),
        seed_report(2, &[(lib2, 18)], 0),
    ];
    let err = tree_outlasts_liberation(&idle, 2).expect_err("no node bought after it");
    assert!(
        err.contains("median 0 nodes bought after Liberation(2), need at least 1"),
        "{err}"
    );
}

#[test]
fn tier_affordable_runs_checks_every_tier_window() {
    let tiers = |seed: usize, runs: [usize; 3]| {
        seed_report(
            seed,
            &[
                (Milestone::TierAffordable(1), runs[0]),
                (Milestone::TierAffordable(2), runs[1]),
                (Milestone::TierAffordable(3), runs[2]),
            ],
            1,
        )
    };
    let on_target = [
        tiers(0, [4, 8, 12]),
        tiers(1, [3, 9, 11]),
        tiers(2, [5, 7, 13]),
    ];
    assert!(
        tier_affordable_runs(&on_target, &GOAL5_TIER_WINDOWS).is_ok(),
        "{:?}",
        tier_affordable_runs(&on_target, &GOAL5_TIER_WINDOWS)
    );
    // The wave-0 shape, 3 / 5 / 7: tier 1 passes, tiers 2 and 3 fail.
    let early = [
        tiers(0, [3, 5, 7]),
        tiers(1, [3, 5, 7]),
        tiers(2, [3, 5, 7]),
    ];
    let err = tier_affordable_runs(&early, &GOAL5_TIER_WINDOWS).expect_err("tiers 2-3 early");
    assert!(
        err.contains("Tier 2 affordable at median run 5, outside runs 7-9"),
        "{err}"
    );
    assert!(
        err.contains("Tier 3 affordable at median run 7, outside runs 11-13"),
        "{err}"
    );
    assert!(!err.contains("Tier 1 affordable at median"), "{err}");
    assert_eq!(GOAL5_FIRST_VICTORY, 2..=4);
    let first = |run| seed_report(0, &[(Milestone::FirstVictory, run)], 1);
    assert!(
        median_run_within(&[first(4)], "First victory", Milestone::FirstVictory, 2..=4).is_ok()
    );
    assert!(
        median_run_within(&[first(5)], "First victory", Milestone::FirstVictory, 2..=4).is_err()
    );
}

#[test]
fn tree_exhausted_ignores_retired_nodes() {
    let nodes = &skills().nodes;
    // The innate root is never sold, so the tree is exhausted once every
    // other node is owned, whether or not the root is.
    let root = nodes
        .iter()
        .find(|n| n.price == 0)
        .expect("the starter root");
    assert!(node_retired(root));
    assert!(
        nodes
            .iter()
            .filter(|n| n.price > 0)
            .all(|n| !node_retired(n))
    );
    assert!(tree_exhausted(nodes, |n| n.id != root.id, node_retired));
    assert_eq!(nodes_left(nodes, |n| n.id != root.id, node_retired), 0);
    // One unowned node that is for sale keeps the tree open.
    let last = nodes.last().expect("a catalogue");
    assert!(!tree_exhausted(nodes, |n| n.id != last.id, node_retired));
    assert_eq!(nodes_left(nodes, |n| n.id != last.id, node_retired), 1);
    // The same node, once retired, no longer counts.
    let retired = |n: &SkillNode| node_retired(n) || n.id == last.id;
    assert!(tree_exhausted(nodes, |n| n.id != last.id, retired));
    assert!(tree_exhausted(nodes, |_| true, node_retired));
    assert!(!tree_exhausted(nodes, |_| false, node_retired));
}

// Goal-6 rows and evaluators (W2-04) on synthetic reports. The release
// acceptance runs the same evaluators on the expert-deu sweep and the goal-6
// rows (tests/pacing_targets.rs).

#[test]
fn goal6_rows_are_registered_with_their_stops() {
    let presets = [
        BalancePreset::Easy,
        BalancePreset::Normal,
        BalancePreset::Hard,
    ];
    assert_eq!(
        GOAL6_LUX_PROFILE_IDS,
        ["lux-easy", "lux-normal", "lux-hard"]
    );
    for (id, preset) in GOAL6_LUX_PROFILE_IDS.into_iter().zip(presets) {
        let p = by_id(id).unwrap_or_else(|| panic!("{id} is registered"));
        assert_eq!(p.country, "LUX", "{id}");
        assert_eq!(p.difficulty, preset, "{id}");
        assert_eq!(p.stop_after, Some(Milestone::Liberation(1)), "{id}");
        assert_eq!(p.max_runs, 20, "{id}");
        assert_eq!(
            (p.aim, p.spend),
            (PacingAim::Expert, PacingSpend::Recommended)
        );
    }
    assert_eq!(GOAL6_START_PROFILE_IDS, ["start-aut", "start-bgd"]);
    for (id, country) in GOAL6_START_PROFILE_IDS.into_iter().zip(["AUT", "BGD"]) {
        let p = by_id(id).unwrap_or_else(|| panic!("{id} is registered"));
        assert_eq!(p.country, country, "{id}");
        assert_eq!(p.difficulty, BalancePreset::Normal, "{id}");
        assert_eq!(p.stop_after, Some(Milestone::FirstVictory), "{id}");
        assert_eq!(p.max_runs, 20, "{id}");
        assert_eq!(
            (p.aim, p.spend),
            (PacingAim::Expert, PacingSpend::Recommended)
        );
    }
    // Appended rows: the legacy table and the acceptance profile are unchanged.
    let ids: Vec<&str> = pacing_registry().iter().map(|p| p.id).collect();
    assert_eq!(&ids[..3], &LEGACY_PROFILE_IDS);
    for id in GOAL6_LUX_PROFILE_IDS.iter().chain(&GOAL6_START_PROFILE_IDS) {
        assert!(!LEGACY_PROFILE_IDS.contains(id), "{id}");
    }
}

/// A synthetic report of `profile` whose runs have these outcomes (`V`
/// victory, `D` defeat, `W` withdrawal), with its first victory recorded.
fn outcome_report(profile: &str, seed_index: usize, outcomes: &str) -> PacingReport {
    let runs: Vec<PacingRun> = outcomes
        .chars()
        .enumerate()
        .map(|(i, c)| PacingRun {
            outcome: match c {
                'V' => Outcome::Victory,
                'D' => Outcome::Defeat,
                'W' => Outcome::Withdrawal,
                other => panic!("unknown outcome {other}"),
            },
            ..camp_run(i + 1, 0)
        })
        .collect();
    let milestones = runs
        .iter()
        .find(|r| r.outcome == Outcome::Victory)
        .map(|r| (Milestone::FirstVictory, r.index, 0.0))
        .into_iter()
        .collect();
    PacingReport {
        profile: PacingProfile {
            max_runs: outcomes.len(),
            ..by_id(profile).expect("registered")
        },
        seed_index,
        seed: 0,
        runs,
        milestones,
    }
}

fn window(offset: usize, defeats: usize, runs: usize, seeds: usize) -> DefeatWindow {
    DefeatWindow {
        offset,
        defeats,
        runs,
        seeds,
    }
}

#[test]
fn summary_windows_and_per_seed_streaks_on_synthetic_runs() {
    assert_eq!(GOAL6_WINDOW_RUNS, 20);
    assert_eq!(GOAL6_DEFEAT_RATE_PCT, 10..=25);
    let expert = DEFAULT_PROFILE_ID;
    // Seed 0 wins first on run 3; the two defeats before it are not counted.
    // After it, every 10-run window holds 2 defeats (one is a withdrawal).
    let seed0 = outcome_report(expert, 0, &format!("DDV{}", "VVVVDVVVVW".repeat(2)));
    // Seed 1 wins on run 1; after it every 10-run window holds 1 defeat, and
    // it has 25 runs after its first victory, so 16 windows.
    let seed1 = outcome_report(expert, 1, &format!("V{}VVVVV", "VVVVVVVVVD".repeat(2)));
    let pooled = [seed0.clone(), seed1];
    let mut expected: Vec<DefeatWindow> = (0..=10).map(|o| window(o, 3, 20, 2)).collect();
    expected.extend((11..=15).map(|o| window(o, 1, 10, 1)));
    assert_eq!(defeat_windows(&pooled, 10), expected);
    assert_eq!(expected[0].rate_pct(), 15.0);
    let ok = defeat_rate_windows(&pooled, 10);
    assert!(ok.is_ok(), "{ok:?}");
    // The summary names how many seeds the extreme windows pool, and the
    // range over every window: the tail windows here are seed 1's alone.
    let ok = ok.unwrap();
    assert!(
        ok.contains(
            "lowest 10.0% at offset 11 over 1 seed, highest 15.0% at offset 0 over 2 seeds"
        ),
        "{ok}"
    );
    assert!(ok.contains("windows pool 1-2 seeds"), "{ok}");
    // Seed 1 without defeats: offsets 0-10 pool 2 / 20 (10%, inside), the
    // five windows only seed 1 reaches are 0%.
    let flawless = [seed0.clone(), outcome_report(expert, 1, &"V".repeat(26))];
    let err = defeat_rate_windows(&flawless, 10).expect_err("five windows at 0%");
    assert!(
        err.contains("5 of 16 windows outside 10-25%: lowest 0.0% at offset 11 over 1 seed"),
        "{err}"
    );
    // A seed that never wins fails and is named.
    let never = [seed0.clone(), outcome_report(expert, 2, &"D".repeat(26))];
    let err = defeat_rate_windows(&never, 10).expect_err("seed 2 never wins");
    assert!(err.contains("seed 2 never wins in 26 runs"), "{err}");
    // No full window at all is not a pass.
    let short = [outcome_report(expert, 0, "DVVD")];
    let err = defeat_rate_windows(&short, 10).expect_err("no window");
    assert!(
        err.contains("no seed has 10 runs after its first victory"),
        "{err}"
    );

    // Streaks are per seed and counted from run 11. Seed 0 wins runs 1-10,
    // loses run 11, then wins 14 in a row: its longest streak from run 11 is
    // 14 (the 10 before do not count).
    assert_eq!(GOAL6_STREAK_FROM_RUN, 11);
    assert_eq!(GOAL6_STREAK_LIMIT, 15);
    let s0 = format!("{}D{}DVVVV", "V".repeat(10), "V".repeat(14));
    // Seed 1 wins runs 6-25: 20 in a row, of which 15 from run 11.
    let s1 = format!("DDDDD{}DDDDD", "V".repeat(20));
    let (a, b) = (
        outcome_report(expert, 0, &s0),
        outcome_report(expert, 1, &s1),
    );
    assert_eq!(longest_win_streak(&a, 11), 14);
    assert_eq!(longest_win_streak(&a, 1), 14);
    assert_eq!(longest_win_streak(&b, 11), 15);
    assert_eq!(longest_win_streak(&b, 1), 20);
    assert_eq!(longest_win_streak(&b, 26), 0);
    let ok = max_win_streak(&[a.clone(), a.clone()], 11);
    assert!(ok.is_ok(), "{ok:?}");
    // The median streak is 14, but one seed reaches 15: the rule is per seed.
    let mut c = a.clone();
    c.seed_index = 2;
    let err = max_win_streak(&[a, b, c], 11).expect_err("seed 1 has a 15-win streak");
    assert!(err.contains("seed 1: 15-win streak from run 11"), "{err}");
    assert!(!err.contains("seed 0") && !err.contains("seed 2:"), "{err}");
    assert!(err.contains("median 14"), "{err}");
}

/// Synthetic liberation reports of `profile`: one seed per entry, None for
/// a seed that never liberates in its 20 runs.
fn liberations(profile: &str, runs: &[Option<usize>]) -> Vec<PacingReport> {
    runs.iter()
        .enumerate()
        .map(|(i, run)| PacingReport {
            profile: by_id(profile).expect("registered"),
            seed_index: i,
            seed: 0,
            runs: vec![],
            milestones: run
                .map(|r| (Milestone::Liberation(1), r, 0.0))
                .into_iter()
                .collect(),
        })
        .collect()
}

#[test]
fn runs_to_liberate_ratio_rejects_hard_above_1_5x_normal() {
    assert_eq!(GOAL6_LUX_EASY_MAX_RUNS, 6);
    assert_eq!(GOAL6_LUX_HARD_OVER_NORMAL, (3, 2));
    let easy = liberations("lux-easy", &[Some(4), Some(5), Some(6)]);
    let normal = liberations("lux-normal", &[Some(8), Some(8), Some(9)]);
    // Hard at exactly 1.5x Normal (12 against 8) passes.
    let hard = liberations("lux-hard", &[Some(12), Some(12), Some(13)]);
    let lux = runs_to_liberate(&easy, &normal, &hard).expect("three LUX sweeps");
    assert_eq!(
        (lux.easy.median, lux.normal.median, lux.hard.median),
        (5, 8, 12)
    );
    let ok = lux.easy_within(GOAL6_LUX_EASY_MAX_RUNS);
    assert!(ok.is_ok(), "{ok:?}");
    let ok = lux.hard_within_ratio(GOAL6_LUX_HARD_OVER_NORMAL);
    assert!(ok.is_ok(), "{ok:?}");
    // One run more and Hard is above 1.5x Normal.
    let harder = liberations("lux-hard", &[Some(13), Some(13), Some(14)]);
    let lux = runs_to_liberate(&easy, &normal, &harder).expect("three LUX sweeps");
    let err = lux
        .hard_within_ratio(GOAL6_LUX_HARD_OVER_NORMAL)
        .expect_err("13 runs against 1.5 x 8");
    assert!(
        err.contains("Hard median run 13 is above 1.5x Normal median run 8 (12)"),
        "{err}"
    );
    // A seed that never liberates counts as max_runs + 1 = 21.
    let stuck = liberations("lux-hard", &[None, None, Some(10)]);
    let lux = runs_to_liberate(&easy, &normal, &stuck).expect("three LUX sweeps");
    assert_eq!(lux.hard.median, 21);
    let err = lux
        .hard_within_ratio(GOAL6_LUX_HARD_OVER_NORMAL)
        .expect_err("two seeds never liberate");
    assert!(err.contains("Hard median run >20"), "{err}");
    // A censored Hard median is no run count: 21 against Normal 14 sits at
    // exactly 1.5x, but a Hard sweep whose median seed never liberates
    // cannot show it is within the ratio.
    let late = liberations("lux-normal", &[Some(14), Some(14), Some(15)]);
    let lux = runs_to_liberate(&easy, &late, &stuck).expect("three LUX sweeps");
    assert_eq!((lux.normal.median, lux.hard.median), (14, 21));
    let err = lux
        .hard_within_ratio(GOAL6_LUX_HARD_OVER_NORMAL)
        .expect_err("the Hard median seed never liberates");
    assert!(
        err.contains("Hard median run >20: the median Hard seed never liberates in 20 runs"),
        "{err}"
    );
    // A censored Normal median with a measured Hard one is still a pass:
    // Hard 10 is below any Normal run past 20.
    let never = liberations("lux-normal", &[None, None, Some(3)]);
    let measured = liberations("lux-hard", &[Some(10), Some(10), Some(11)]);
    let lux = runs_to_liberate(&easy, &never, &measured).expect("three LUX sweeps");
    let ok = lux.hard_within_ratio(GOAL6_LUX_HARD_OVER_NORMAL);
    assert!(ok.is_ok(), "{ok:?}");
    // Easy above 6 runs fails its own check, not the ratio.
    let slow = liberations("lux-easy", &[Some(7), Some(7), Some(8)]);
    let lux = runs_to_liberate(&slow, &normal, &hard).expect("three LUX sweeps");
    let err = lux
        .easy_within(GOAL6_LUX_EASY_MAX_RUNS)
        .expect_err("7 runs");
    assert!(err.contains("Easy median run 7 is above 6"), "{err}");
    assert!(lux.hard_within_ratio(GOAL6_LUX_HARD_OVER_NORMAL).is_ok());
    // The sweeps must be the preset they are evaluated as.
    let err = runs_to_liberate(&normal, &normal, &hard).expect_err("Normal as Easy");
    assert!(err.contains("lux-normal"), "{err}");
    assert!(runs_to_liberate(&[], &normal, &hard).is_err());
}

#[test]
fn boss_ttk_coverage_and_clamped_starts_on_synthetic_runs() {
    assert_eq!(GOAL6_BOSS_TTK_S, 20.0..=60.0);
    let ttk = |seed: usize, values: &[Option<f32>]| PacingReport {
        runs: values
            .iter()
            .enumerate()
            .map(|(i, s)| PacingRun {
                boss_ttk_s: *s,
                ..camp_run(i + 1, 0)
            })
            .collect(),
        ..outcome_report(DEFAULT_PROFILE_ID, seed, "")
    };
    // Pooled over every boss kill: 6, 25, 30, 40, 90, so the lower median
    // is 30.
    let fell = [
        ttk(0, &[Some(6.0), None, Some(30.0)]),
        ttk(1, &[Some(90.0), Some(25.0), Some(40.0)]),
    ];
    let ok = boss_ttk_median_s(&fell).expect("median 30 s");
    assert!(ok.contains("median 30.0 s"), "{ok}");
    assert!(ok.contains("over 5 boss kills"), "{ok}");
    let fast = [ttk(0, &[Some(6.0), Some(8.0), Some(30.0)])];
    let err = boss_ttk_median_s(&fast).expect_err("median 8 s");
    assert!(err.contains("median 8.0 s, outside 20-60 s"), "{err}");
    let err = boss_ttk_median_s(&[ttk(0, &[None])]).expect_err("no boss fell");
    assert!(err.contains("no boss fell in 1 runs"), "{err}");

    let with = |spawned: u32, killed: u32| ArchetypeRun {
        spawned,
        killed,
        damage_taken: 0,
    };
    let mut run = camp_run(1, 0);
    run.archetypes.insert("walker".to_owned(), with(10, 9));
    run.archetypes.insert("runner".to_owned(), with(3, 0));
    let mut later = camp_run(2, 0);
    later.archetypes.insert("walker".to_owned(), with(5, 5));
    let report = PacingReport {
        runs: vec![run, later],
        ..outcome_report(DEFAULT_PROFILE_ID, 0, "")
    };
    let ids = ["walker", "runner", "brute"].map(str::to_owned);
    let err = archetype_coverage(std::slice::from_ref(&report), &ids)
        .expect_err("runner never dies, brute never spawns");
    assert!(err.contains("never spawned: brute"), "{err}");
    assert!(err.contains("spawned but never killed: runner"), "{err}");
    assert!(err.contains("walker 15/14"), "{err}");
    let ok = archetype_coverage(std::slice::from_ref(&report), &ids[..1]);
    assert!(ok.is_ok(), "{ok:?}");
    assert!(archetype_coverage(&[], &ids).is_err());

    assert_eq!(GOAL6_CLAMPED_FIRST_VICTORY, 2..=4);
    let starts = [
        outcome_report("start-aut", 0, "DV"),
        outcome_report("start-aut", 1, "V"),
        outcome_report("start-aut", 2, "DDV"),
    ];
    let ok =
        clamped_start_first_victory(&starts, GOAL6_CLAMPED_FIRST_VICTORY).expect("median run 2");
    assert!(ok.contains("AUT start"), "{ok}");
    // Run-1 victories are only reported (ruling W-4).
    assert!(ok.contains("run-1 victories: seed 1"), "{ok}");
    let late = [
        outcome_report("start-bgd", 0, "DDDDV"),
        outcome_report("start-bgd", 1, "DDDDDV"),
    ];
    let err =
        clamped_start_first_victory(&late, GOAL6_CLAMPED_FIRST_VICTORY).expect_err("median run 5");
    assert!(err.contains("BGD start"), "{err}");
    assert!(err.contains("median run 5, outside runs 2-4"), "{err}");
}

// W2-42 (W2-I): the profiles that need other remits' commands. The
// auto-sell and takeover rows play the real game, each kept short.

#[test]
fn auto_sell_profile_records_auto_sold_coins() {
    let profile = PacingProfile {
        max_runs: 3,
        ..by_id("auto-sell").expect("auto-sell is registered")
    };
    let report = simulate_pacing(&profile).expect("profile runs");
    let count: u32 = report.runs.iter().map(|r| r.auto_sold_count).sum();
    let coins: u64 = report.runs.iter().map(|r| r.auto_sold_coins).sum();
    assert!(
        count > 0 && coins > 0,
        "auto-sold {count} items for {coins}"
    );
    // `earned` counts the auto-sold coins with the run, grant and milestone
    // coins (design-progression-tree §11).
    let mut earned = 0;
    for run in &report.runs {
        earned += run.coins + run.level_grant_coins + run.milestone_coins + run.auto_sold_coins;
        assert_eq!(run.cumulative_coins, earned, "run {}", run.index);
    }
    // The same player without the rule sells nothing, and the rule acts only
    // at settlement: run 1 is the same fight.
    let default = simulate_pacing(&PacingProfile {
        max_runs: 3,
        ..by_id(DEFAULT_PROFILE_ID).expect("registered")
    })
    .expect("profile runs");
    assert!(
        default
            .runs
            .iter()
            .all(|r| r.auto_sold_count == 0 && r.auto_sold_coins == 0)
    );
    let (a, b) = (&report.runs[0], &default.runs[0]);
    assert_eq!(
        (a.outcome, a.kills, a.coins, a.elapsed),
        (b.outcome, b.kills, b.coins, b.elapsed)
    );
}

#[test]
fn equip_drops_profile_wears_dropped_upgrades() {
    // The row varies the acceptance profile by `equip_drops` alone.
    let expert = by_id(DEFAULT_PROFILE_ID).expect("registered");
    let row = by_id("equip-drops").expect("equip-drops is registered");
    assert_eq!(
        PacingProfile {
            id: expert.id,
            name: expert.name,
            ..row
        },
        PacingProfile {
            plan: PacingPlan {
                equip_drops: true,
                ..PacingPlan::SOLO
            },
            ..expert
        }
    );
    // The camp step equips an owned drop that beats the survivor's slot by
    // the recommender's test, and leaves one the survivor cannot use.
    let mut game = Game::default();
    let start = world().countries[0].id.clone();
    let c = game.staging().campaign();
    c.choose_start_country(&start).expect("start country");
    let index = |id: &str| {
        content()
            .items
            .iter()
            .position(|i| i.id.as_str() == id)
            .unwrap_or_else(|| panic!("{id} ships"))
    };
    let axe = c.add_item(index("fire_axe")).expect("owned");
    let rifle = c.add_item(index("marksman_rifle")).expect("owned");
    let before = game.campaign().player.loadout;
    assert!(
        game.campaign()
            .upgrade_gain(Mode::Personal, before, &game.campaign().npc, axe)
            .is_some()
    );
    assert_eq!(equip_dropped_upgrades(&mut game), Ok(vec![axe]));
    let after = game.campaign().player.loadout;
    assert_eq!(after[Slot::Melee.index()], axe);
    assert_eq!(after[Slot::Ranged.index()], before[Slot::Ranged.index()]);
    assert!(!after.contains(&rifle));
    // Nothing left beats the new loadout.
    assert_eq!(equip_dropped_upgrades(&mut game), Ok(vec![]));
    // The profile runs.
    let report = simulate_pacing(&PacingProfile { max_runs: 2, ..row }).expect("profile runs");
    assert_eq!(report.runs.len(), 2);
}

#[test]
fn takeover_profile_records_first_takeover() {
    let profile = PacingProfile {
        max_runs: 20,
        stop_after: Some(Milestone::FirstTakeover),
        ..by_id("takeover").expect("takeover is registered")
    };
    let report = simulate_pacing(&profile).expect("profile runs");
    let (run, _) = report
        .milestone(Milestone::FirstTakeover)
        .expect("a recruit defends the main run inside 20 runs");
    assert_eq!(report.runs.len(), run, "the profile stops at the takeover");
    assert!(
        matches!(report.runs[run - 1].defender, DefenderId::Recruit(_)),
        "{:?}",
        report.runs[run - 1].defender
    );
    assert!(
        report.runs[..run - 1]
            .iter()
            .all(|r| r.defender == DefenderId::Player)
    );
    // Recruiting needs a liberated country and is paid in camp.
    let (liberated, _) = report
        .milestone(Milestone::Liberation(1))
        .expect("a country is liberated before the takeover");
    assert!(liberated < run, "liberated {liberated}, took over {run}");
    let spent: u64 = report.runs.iter().map(|r| r.spent_recruit).sum();
    assert!(spent > 0, "the recruit was paid for");
}

#[test]
fn w2_42_rows_vary_the_acceptance_profile_by_one_option() {
    let expert = by_id(DEFAULT_PROFILE_ID).expect("registered");
    let row = |id: &str| by_id(id).unwrap_or_else(|| panic!("{id} is registered"));
    let plan = |p: PacingPlan| PacingProfile {
        id: expert.id,
        name: expert.name,
        plan: p,
        ..expert
    };
    let same = |id: &str, expected: PacingProfile| {
        let p = row(id);
        assert_eq!(
            PacingProfile {
                id: expert.id,
                name: expert.name,
                ..p
            },
            expected,
            "{id}"
        );
    };
    same(
        "manual-sell",
        plan(PacingPlan {
            sell: PacingSell::ManualExtras,
            ..PacingPlan::SOLO
        }),
    );
    same(
        "auto-sell",
        plan(PacingPlan {
            sell: PacingSell::Auto,
            ..PacingPlan::SOLO
        }),
    );
    same(
        "takeover",
        PacingProfile {
            country: "LUX",
            ..plan(PacingPlan {
                defender: PacingDefender::RecruitWhenReady,
                ..PacingPlan::SOLO
            })
        },
    );
    same(
        "farm",
        PacingProfile {
            aim: PacingAim::Novice,
            ..plan(PacingPlan {
                farm: PacingFarm::ReplayAfterDefeats(2),
                ..PacingPlan::SOLO
            })
        },
    );
    same(
        "easy",
        PacingProfile {
            difficulty: BalancePreset::Easy,
            ..expert
        },
    );
    same(
        "hard",
        PacingProfile {
            difficulty: BalancePreset::Hard,
            ..expert
        },
    );
    same(
        "long",
        PacingProfile {
            max_runs: 150,
            ..expert
        },
    );
    // Every registered plan is implemented: a zero-run profile is accepted.
    for p in pacing_registry() {
        let report = simulate_pacing(&PacingProfile { max_runs: 0, ..*p });
        assert!(report.is_ok(), "{}: {report:?}", p.id);
    }
}

#[test]
fn takeover_survivor_retries_a_stage_the_recruit_lost() {
    let profile = PacingProfile {
        max_runs: 12,
        ..by_id("takeover").expect("registered")
    };
    let report = simulate_pacing(&profile).expect("profile runs");
    let lost: Vec<usize> = report
        .runs
        .windows(2)
        .filter(|w| {
            matches!(w[0].defender, DefenderId::Recruit(_)) && w[0].outcome != Outcome::Victory
        })
        .map(|w| {
            assert_eq!(w[1].defender, DefenderId::Player, "run {}", w[1].index);
            assert_eq!((&w[1].country, w[1].stage), (&w[0].country, w[0].stage));
            w[0].index
        })
        .collect();
    assert!(!lost.is_empty(), "a recruit loses inside 12 runs");
    // After the survivor's win the recruit defends again.
    let back = report.runs.windows(2).any(|w| {
        w[0].defender == DefenderId::Player
            && w[0].outcome == Outcome::Victory
            && matches!(w[1].defender, DefenderId::Recruit(_))
    });
    assert!(back, "the recruit takes over again after a survivor win");
}

#[test]
fn farm_profile_replays_the_previous_stage_after_two_frontier_defeats() {
    let profile = PacingProfile {
        max_runs: 20,
        ..by_id("farm").expect("registered")
    };
    let report = simulate_pacing(&profile).expect("profile runs");
    let runs = &report.runs;
    let mut replays = 0;
    for i in 2..runs.len() {
        let (before, run) = (&runs[i - 1], &runs[i]);
        if run.country != before.country || run.stage >= before.stage {
            continue;
        }
        replays += 1;
        assert_eq!(
            run.stage + 1,
            before.stage,
            "run {} replays one back",
            run.index
        );
        for lost in [&runs[i - 2], before] {
            assert_ne!(lost.outcome, Outcome::Victory, "run {}", lost.index);
            assert_eq!(lost.stage, before.stage, "run {}", lost.index);
        }
        if let Some(next) = runs.get(i + 1) {
            assert_eq!(next.stage, before.stage, "run {} returns", next.index);
        }
    }
    assert!(replays > 0, "the novice farms inside 20 runs");
}
