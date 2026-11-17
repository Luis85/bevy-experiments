//! Loot-economy evaluators (W4-l): the goal-4 auto-sell pacing gate (G4.13,
//! design-loot-economy "Numbers to measure") and the stash-growth report
//! (ruling R39). Each one is a pure check over seed sweeps, like the goal-5
//! evaluators in [`super::targets`]: `Ok` carries the summary the
//! acceptance prints, `Err` every reason it fails, followed by that summary.
//! `tests/pacing_targets.rs` runs them on the release auto-sell sweeps, and
//! `tests/pacing_harness.rs` pins them on synthetic reports.
use super::targets::{max_runs, run_cell, stats_of, verdict};
use super::{Milestone, PacingReport, PacingSell, lower_quantile};

/// The registry key of the auto-sell row: the acceptance profile with the
/// default auto-sell rule enabled ([`PacingSell::Auto`]).
pub const AUTO_SELL_PROFILE_ID: &str = "auto-sell";
/// G4.13: the tiers whose [`Milestone::TierAffordable`] median the gate
/// compares.
pub const GOAL4_AUTO_SELL_TIERS: [usize; 3] = [1, 2, 3];
/// G4.13: the auto-sell median may sit this many runs either side of the
/// same build's no-sale median.
pub const GOAL4_AUTO_SELL_SLACK_RUNS: usize = 1;
/// R39: the runs after which the stash size is reported.
pub const STASH_GROWTH_RUNS: [usize; 2] = [60, 150];
/// R39: the auto-sold coins are reported per run for runs 1 to this.
pub const AUTO_SOLD_EARLY_RUNS: usize = 6;

/// G4.13: for each tier of `tiers`, the median run at which the tier is
/// affordable with auto-sell lies within `slack` runs of the no-sale
/// median. `default` is the sweep of a profile that never sells and
/// `auto_sell` the sweep of one that stores the auto-sell rule, over the
/// same number of seeds; anything else is an error, not a pass. A missed
/// tier counts as its profile's `max_runs + 1`, as in
/// [`super::MilestoneStats`].
pub fn auto_sell_within_default(
    default: &[PacingReport],
    auto_sell: &[PacingReport],
    tiers: &[usize],
    slack: usize,
) -> Result<String, String> {
    if default.is_empty() || auto_sell.is_empty() {
        return Err("no pacing reports to evaluate".to_owned());
    }
    if let Some(r) = default
        .iter()
        .find(|r| r.profile.plan.sell != PacingSell::Never)
    {
        return Err(format!(
            "the baseline {} sells; compare against a profile that never sells",
            r.profile.id
        ));
    }
    if let Some(r) = auto_sell
        .iter()
        .find(|r| r.profile.plan.sell != PacingSell::Auto)
    {
        return Err(format!(
            "{} does not auto-sell; the gate measures an auto-selling profile",
            r.profile.id
        ));
    }
    if default.len() != auto_sell.len() {
        return Err(format!(
            "{} default seeds against {} auto-sell seeds; sweep both over the same seeds",
            default.len(),
            auto_sell.len()
        ));
    }
    let (max_default, max_auto) = (max_runs(default), max_runs(auto_sell));
    let mut problems = Vec::new();
    let mut cells = Vec::with_capacity(tiers.len());
    for &tier in tiers {
        let m = Milestone::TierAffordable(tier);
        let (base, sold) = (stats_of(default, m)?, stats_of(auto_sell, m)?);
        let apart = base.median.abs_diff(sold.median);
        if apart > slack {
            problems.push(format!(
                "Tier {tier} affordable at median run {} with auto-sell is {apart} runs from run {} without (at most {slack})",
                run_cell(sold.median, max_auto),
                run_cell(base.median, max_default)
            ));
        }
        cells.push(format!(
            "Tier {tier} affordable {} with auto-sell, {} without",
            sold.cell(max_auto),
            base.cell(max_default)
        ));
    }
    verdict(problems, cells.join("; "))
}

/// `median [p25, p75]` of `values` (lower quantiles), or None when empty.
fn quartile_cell(mut values: Vec<u64>) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(format!(
        "{} [{}, {}]",
        lower_quantile(&values, 1, 2),
        lower_quantile(&values, 1, 4),
        lower_quantile(&values, 3, 4)
    ))
}

/// R39: the stash size after each run of `runs` and the auto-sold coins of
/// each of the first `early_runs` runs, as lower medians over the seeds.
///
/// The stash size is [`super::PacingRun::items_owned`]: every item the
/// campaign holds after the run, worn ones included, which is what a save
/// stores and a save clone copies. Beside it the report gives the items
/// auto-sold up to that run.
///
/// It is a report, so it fails only when it cannot be made: no reports, a
/// seed that did not play every requested run (named), or a sweep that
/// auto-sold nothing (it is then not an auto-sell measurement).
pub fn stash_growth(
    reports: &[PacingReport],
    runs: &[usize],
    early_runs: usize,
) -> Result<String, String> {
    if reports.is_empty() {
        return Err("no pacing reports to evaluate".to_owned());
    }
    let last = runs.iter().copied().max().unwrap_or(0).max(early_runs);
    let mut problems: Vec<String> = reports
        .iter()
        .filter(|r| r.runs.len() < last)
        .map(|r| {
            format!(
                "seed {} played {} runs, not {last}",
                r.seed_index,
                r.runs.len()
            )
        })
        .collect();
    if reports
        .iter()
        .flat_map(|r| &r.runs)
        .all(|x| x.auto_sold_count == 0)
    {
        problems.push("no item was auto-sold in any run".to_owned());
    }
    let mut cells = Vec::with_capacity(runs.len() + 1);
    for &run in runs {
        let owned = reports
            .iter()
            .filter_map(|r| r.runs.iter().find(|x| x.index == run))
            .map(|x| x.items_owned as u64)
            .collect();
        let sold = reports
            .iter()
            .filter(|r| r.runs.iter().any(|x| x.index == run))
            .map(|r| {
                r.runs
                    .iter()
                    .filter(|x| x.index <= run)
                    .map(|x| u64::from(x.auto_sold_count))
                    .sum()
            })
            .collect();
        let cell = |values| quartile_cell(values).unwrap_or_else(|| "-".to_owned());
        cells.push(format!(
            "after run {run}: {} items owned, {} auto-sold so far",
            cell(owned),
            cell(sold)
        ));
    }
    let early: Vec<String> = (1..=early_runs)
        .map(|run| {
            let mut coins: Vec<u64> = reports
                .iter()
                .filter_map(|r| r.runs.iter().find(|x| x.index == run))
                .map(|x| x.auto_sold_coins)
                .collect();
            coins.sort_unstable();
            if coins.is_empty() {
                "-".to_owned()
            } else {
                lower_quantile(&coins, 1, 2).to_string()
            }
        })
        .collect();
    cells.push(format!(
        "auto-sold coins per run, runs 1-{early_runs}: {}",
        early.join(" / ")
    ));
    let summary = format!(
        "{} (lower median [p25, p75] over {} seeds)",
        cells.join("; "),
        reports.len()
    );
    verdict(problems, summary)
}
