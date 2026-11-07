//! Pacing harness v2 (W2-01): the profile registry, `stop_after`, the
//! threaded sweep and the milestone statistics.
use horde_core::sim::{
    DEFAULT_PROFILE_ID, LEGACY_PROFILE_IDS, Milestone, MilestoneStats, PacingAim, PacingProfile,
    PacingReport, by_id, pacing_registry, pacing_seed, pacing_sweep_with_threads, simulate_pacing,
};
use std::collections::BTreeSet;

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
