//! Pacing acceptance (goal 5 from W2-03; goal 6 joins in W2-04). Release
//! only: every test is `#[ignore = "release-only"]`. From horde-defense/,
//! after the debug builds:
//!
//! ```text
//! cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture
//! ```
//!
//! The acceptance sweep is expert-deu (Normal, 60 runs) over 20 seeds. It
//! runs once per test binary (a `OnceLock` cache) and every test reads it.
//! Each test prints its summary line (PASS or FAIL) and fails with the
//! reasons. The evaluators are pinned on synthetic reports in
//! `tests/pacing_harness.rs`, which runs in the normal suite.
#![allow(clippy::print_stdout)] // the acceptance prints its summary lines
use horde_core::balance::BalancePreset;
use horde_core::sim::{
    DEFAULT_PROFILE_ID, GOAL5_FIRST_VICTORY, GOAL5_TIER_WINDOWS, Milestone, PacingReport, by_id,
    median_run_within, pacing_sweep, tier_affordable_runs, tree_outlasts_liberation, unlock_order,
};
use std::sync::OnceLock;

/// Seeds of the goal-5 and goal-6 acceptance sweep (ruling W-1).
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

/// Prints the evaluator's verdict and fails on `Err`.
fn check(name: &str, verdict: Result<String, String>) {
    match verdict {
        Ok(summary) => println!("{name}: PASS {summary}"),
        Err(reason) => {
            println!("{name}: FAIL {reason}");
            panic!("{name}: {reason}");
        }
    }
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
