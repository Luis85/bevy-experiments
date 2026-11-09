//! Pacing acceptance: goal 5 (W2-03), goal 6 (W2-04) and goal 3 (W2-43).
//! Release only: every test is `#[ignore = "release-only"]`. From
//! horde-defense/, after the debug builds:
//!
//! ```text
//! cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture
//! ```
//!
//! The acceptance sweep is expert-deu (Normal, 60 runs) over 20 seeds. The
//! goal-6 rows (lux-easy, lux-normal, lux-hard, start-aut, start-bgd) are a
//! second sweep over the same 20 seeds, and the strategy rows (strategy,
//! neighbours-solo, neighbours-parallel) a third. Each sweep runs once per
//! test binary (a `OnceLock` cache) and every test reads it. Each test
//! prints its summary line (PASS or FAIL) and fails with the reasons. The
//! evaluators are pinned on synthetic reports in `tests/pacing_harness.rs`
//! and `tests/strategy_pacing.rs`, which run in the normal suite.
#![allow(clippy::print_stdout)] // the acceptance prints its summary lines
use horde_core::balance::BalancePreset;
use horde_core::sim::{
    DEFAULT_PROFILE_ID, GOAL5_FIRST_VICTORY, GOAL5_TIER_WINDOWS, GOAL6_CLAMPED_FIRST_VICTORY,
    GOAL6_LUX_EASY_MAX_RUNS, GOAL6_LUX_HARD_OVER_NORMAL, GOAL6_LUX_PROFILE_IDS,
    GOAL6_START_PROFILE_IDS, GOAL6_STREAK_FROM_RUN, GOAL6_WINDOW_RUNS, Milestone, PacingProfile,
    PacingReport, RunsToLiberate, STRATEGY_PROFILE_ID, STRATEGY_PROFILE_IDS, StrategyReport,
    archetype_coverage, boss_ttk_median_s, by_id, clamped_start_first_victory, defeat_rate_windows,
    liberations_and_coins_per_minute, max_win_streak, median_run_within, outbreak_reopens,
    pacing_archetype_ids, pacing_sweep, parallel_frees_neighbours, runs_to_liberate,
    strategy_sweep, tier_affordable_runs, tree_outlasts_liberation, unlock_order,
};
use std::sync::OnceLock;

/// Seeds of the goal-5 and goal-6 acceptance sweeps (ruling W-1).
const SEEDS: usize = 20;

/// The expert-deu sweep, played once and shared by every test.
fn expert_sweep() -> &'static [PacingReport] {
    static SWEEP: OnceLock<Vec<PacingReport>> = OnceLock::new();
    SWEEP.get_or_init(|| {
        let profile = by_id(DEFAULT_PROFILE_ID).expect("the acceptance profile is registered");
        assert_eq!(profile.difficulty, BalancePreset::Normal);
        assert_eq!(profile.max_runs, 60);
        pacing_sweep(&[profile], SEEDS).expect("the expert-deu sweep runs")
    })
}

/// The goal-6 rows' sweep (ruling W-4), played once and shared.
fn goal6_sweep() -> &'static [PacingReport] {
    static SWEEP: OnceLock<Vec<PacingReport>> = OnceLock::new();
    SWEEP.get_or_init(|| {
        let profiles: Vec<PacingProfile> = GOAL6_LUX_PROFILE_IDS
            .iter()
            .chain(&GOAL6_START_PROFILE_IDS)
            .map(|id| by_id(id).unwrap_or_else(|| panic!("{id} is registered")))
            .collect();
        pacing_sweep(&profiles, SEEDS).expect("the goal-6 sweep runs")
    })
}

/// The reports of one goal-6 row.
fn row(id: &str) -> Vec<PacingReport> {
    let reports: Vec<PacingReport> = goal6_sweep()
        .iter()
        .filter(|r| r.profile.id == id)
        .cloned()
        .collect();
    assert_eq!(reports.len(), SEEDS, "{id}");
    reports
}

fn lux() -> RunsToLiberate {
    let [easy, normal, hard] = GOAL6_LUX_PROFILE_IDS.map(row);
    runs_to_liberate(&easy, &normal, &hard).expect("three LUX sweeps")
}

/// Prints the evaluator's verdict and fails on `Err`.
fn check(name: &str, verdict: Result<String, String>) {
    check_all(name, [verdict]);
}

/// Prints every verdict, then fails if any is `Err`.
fn check_all<const N: usize>(name: &str, verdicts: [Result<String, String>; N]) {
    let mut failures = Vec::new();
    for verdict in verdicts {
        match verdict {
            Ok(summary) => println!("{name}: PASS {summary}"),
            Err(reason) => {
                println!("{name}: FAIL {reason}");
                failures.push(reason);
            }
        }
    }
    assert!(failures.is_empty(), "{name}: {}", failures.join(" | "));
}

#[test]
#[ignore = "release-only"]
fn goal5_first_victory_run_2_to_4() {
    check(
        "goal5_first_victory_run_2_to_4",
        median_run_within(
            expert_sweep(),
            "First victory",
            Milestone::FirstVictory,
            GOAL5_FIRST_VICTORY,
        ),
    );
}

#[test]
#[ignore = "release-only"]
fn goal5_tiers_affordable_3_5_7_9_11_13() {
    check(
        "goal5_tiers_affordable_3_5_7_9_11_13",
        tier_affordable_runs(expert_sweep(), &GOAL5_TIER_WINDOWS),
    );
}

#[test]
#[ignore = "release-only"]
fn goal5_tree_outlasts_second_liberation() {
    check(
        "goal5_tree_outlasts_second_liberation",
        tree_outlasts_liberation(expert_sweep(), 2),
    );
}

#[test]
#[ignore = "release-only"]
fn goal5_unlock_order_fortifications_mara_operations() {
    check(
        "goal5_unlock_order_fortifications_mara_operations",
        unlock_order(expert_sweep()),
    );
}

#[test]
#[ignore = "release-only"]
fn goal6_defeat_rate_windows() {
    check(
        "goal6_defeat_rate_windows",
        defeat_rate_windows(expert_sweep(), GOAL6_WINDOW_RUNS),
    );
}

#[test]
#[ignore = "release-only"]
fn goal6_no_15_streak_after_run_10() {
    check(
        "goal6_no_15_streak_after_run_10",
        max_win_streak(expert_sweep(), GOAL6_STREAK_FROM_RUN),
    );
}

#[test]
#[ignore = "release-only"]
fn goal6_lux_easy_at_most_6_runs() {
    check(
        "goal6_lux_easy_at_most_6_runs",
        lux().easy_within(GOAL6_LUX_EASY_MAX_RUNS),
    );
}

#[test]
#[ignore = "release-only"]
fn goal6_lux_hard_at_most_1_5x_normal() {
    check(
        "goal6_lux_hard_at_most_1_5x_normal",
        lux().hard_within_ratio(GOAL6_LUX_HARD_OVER_NORMAL),
    );
}

#[test]
#[ignore = "release-only"]
fn goal6_archetype_coverage() {
    check(
        "goal6_archetype_coverage",
        archetype_coverage(expert_sweep(), &pacing_archetype_ids()),
    );
}

#[test]
#[ignore = "release-only"]
fn goal6_boss_ttk_median_20_60_s() {
    check(
        "goal6_boss_ttk_median_20_60_s",
        boss_ttk_median_s(expert_sweep()),
    );
}

#[test]
#[ignore = "release-only"]
fn goal6_clamped_starts_first_victory_runs_2_to_4() {
    check_all(
        "goal6_clamped_starts_first_victory_runs_2_to_4",
        GOAL6_START_PROFILE_IDS
            .map(|id| clamped_start_first_victory(&row(id), GOAL6_CLAMPED_FIRST_VICTORY)),
    );
}

// Goal 3 (W2-43, design-campaign-defenders 1.13): the strategy rows, a third
// sweep over the same 20 seeds. W4-c makes them pass.

/// The strategy rows' sweep, played once and shared.
fn strategy() -> &'static [StrategyReport] {
    static SWEEP: OnceLock<Vec<StrategyReport>> = OnceLock::new();
    SWEEP.get_or_init(|| {
        let profiles: Vec<PacingProfile> = STRATEGY_PROFILE_IDS
            .iter()
            .map(|id| by_id(id).unwrap_or_else(|| panic!("{id} is registered")))
            .collect();
        strategy_sweep(&profiles, SEEDS).expect("the strategy sweep runs")
    })
}

/// The reports of one strategy row.
fn strategy_row(id: &str) -> Vec<StrategyReport> {
    let reports: Vec<StrategyReport> = strategy()
        .iter()
        .filter(|r| r.pacing.profile.id == id)
        .cloned()
        .collect();
    assert_eq!(reports.len(), SEEDS, "{id}");
    reports
}

#[test]
#[ignore = "release-only"]
fn strategy_profile_reports_liberations_2_4_10_and_coins_per_minute() {
    check(
        "strategy_profile_reports_liberations_2_4_10_and_coins_per_minute",
        liberations_and_coins_per_minute(&strategy_row(STRATEGY_PROFILE_ID)),
    );
}

#[test]
#[ignore = "release-only"]
fn parallel_frees_three_stage_neighbours_within_two_thirds_of_solo_runs() {
    check(
        "parallel_frees_three_stage_neighbours_within_two_thirds_of_solo_runs",
        parallel_frees_neighbours(
            &strategy_row(STRATEGY_PROFILE_IDS[1]),
            &strategy_row(STRATEGY_PROFILE_IDS[2]),
        ),
    );
}

#[test]
#[ignore = "release-only"]
fn fewer_than_five_outbreak_reopens_over_ten_liberations() {
    check(
        "fewer_than_five_outbreak_reopens_over_ten_liberations",
        outbreak_reopens(&strategy_row(STRATEGY_PROFILE_ID)),
    );
}
