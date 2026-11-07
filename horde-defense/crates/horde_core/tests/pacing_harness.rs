//! Pacing harness v2 (W2-01): the profile registry, `stop_after`, the
//! threaded sweep and the milestone statistics; the goal-5 evaluators
//! (W2-03) on synthetic reports.
use horde_core::balance::BalancePreset;
use horde_core::defenders::DefenderId;
use horde_core::sim::{
    DEFAULT_PROFILE_ID, GOAL5_FIRST_VICTORY, GOAL5_TIER_WINDOWS, LEGACY_PROFILE_IDS, Milestone,
    MilestoneStats, Outcome, PacingAim, PacingProfile, PacingReport, PacingRun, by_id,
    median_run_within, node_retired, nodes_left, pacing_registry, pacing_seed,
    pacing_sweep_with_threads, simulate_pacing, tier_affordable_runs, tree_exhausted,
    tree_outlasts_liberation, unlock_order,
};
use horde_core::skill_catalog::{CampaignFeature, SkillNode, skills};
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
