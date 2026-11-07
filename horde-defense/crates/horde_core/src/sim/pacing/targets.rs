//! Goal-5 acceptance evaluators (W2-03; design-progression-tree §11 and
//! rulings W-2 and W-3). Each one is a pure check over the reports of one
//! profile's seed sweep. `Ok` carries the summary the acceptance prints;
//! `Err` carries every reason it fails, followed by that summary.
//! `tests/pacing_targets.rs` runs them on the release expert-deu sweep, and
//! `tests/pacing_harness.rs` pins them on synthetic reports.
//!
//! Run counts are the harness's "runs completed when first held"; a seed
//! that misses a milestone counts as its profile's `max_runs + 1`, as in
//! [`MilestoneStats`].
use super::{Milestone, MilestoneStats, PacingReport};
use crate::skill_catalog::CampaignFeature;
use std::ops::RangeInclusive;

/// The goal-5 unlock order (ruling W-2): Fortifications, then Mara, then
/// the Operations page ([`crate::sim::Game::operations_available`]).
pub const UNLOCK_ORDER: [(&str, Milestone); 3] = [
    (
        "Fortifications",
        Milestone::Feature(CampaignFeature::Construction),
    ),
    ("Mara", Milestone::Feature(CampaignFeature::Companion)),
    ("Operations", Milestone::OperationsOpen),
];
/// Goal 5: the first victory on run 2-4 (median).
pub const GOAL5_FIRST_VICTORY: RangeInclusive<usize> = 2..=4;
/// Goal 5: tiers 1-3 affordable at about runs 4 / 8 / 12, one run either
/// side (median of [`Milestone::TierAffordable`]).
pub const GOAL5_TIER_WINDOWS: [(usize, RangeInclusive<usize>); 3] =
    [(1, 3..=5), (2, 7..=9), (3, 11..=13)];

fn stats_of(reports: &[PacingReport], m: Milestone) -> Result<MilestoneStats, String> {
    MilestoneStats::of(reports, m).ok_or_else(|| "no pacing reports to evaluate".to_owned())
}
/// The largest `max_runs` of the sweep, for printing missed milestones.
fn max_runs(reports: &[PacingReport]) -> usize {
    reports
        .iter()
        .map(|r| r.profile.max_runs)
        .max()
        .unwrap_or(0)
}
/// A run count as a table prints it: above `max_runs` is `>max_runs`.
fn run_cell(run: usize, max_runs: usize) -> String {
    if run > max_runs {
        format!(">{max_runs}")
    } else {
        run.to_string()
    }
}
fn verdict(problems: Vec<String>, summary: String) -> Result<String, String> {
    if problems.is_empty() {
        Ok(summary)
    } else {
        Err(format!("{} ({summary})", problems.join("; ")))
    }
}

/// Ruling W-2: in every seed Fortifications <= Mara <= Operations, and the
/// medians rise strictly. A missed unlock counts as `max_runs + 1`, so a
/// seed that opens Mara but never builds Fortifications fails.
pub fn unlock_order(reports: &[PacingReport]) -> Result<String, String> {
    let mut stats = Vec::with_capacity(UNLOCK_ORDER.len());
    for (label, m) in UNLOCK_ORDER {
        stats.push((label, stats_of(reports, m)?));
    }
    let max = max_runs(reports);
    let summary = stats
        .iter()
        .map(|(label, s)| format!("{label} {}", s.cell(max)))
        .collect::<Vec<_>>()
        .join(" / ");
    let mut problems = Vec::new();
    for r in reports {
        let runs = UNLOCK_ORDER.map(|(_, m)| r.milestone_or_missing(m));
        if runs.windows(2).any(|w| w[0] > w[1]) {
            let order = UNLOCK_ORDER
                .iter()
                .zip(runs)
                .map(|((label, _), run)| format!("{label} {}", run_cell(run, max)))
                .collect::<Vec<_>>()
                .join(", ");
            problems.push(format!("seed {}: {order} out of order", r.seed_index));
        }
    }
    for pair in stats.windows(2) {
        let ((before, a), (after, b)) = (&pair[0], &pair[1]);
        if a.median >= b.median {
            problems.push(format!(
                "median {before} run {} is not before median {after} run {}",
                run_cell(a.median, max),
                run_cell(b.median, max)
            ));
        }
    }
    verdict(problems, summary)
}

/// Ruling W-3 for `liberations` (goal 5 uses 2): the median
/// [`Milestone::TreeExhausted`] comes after the median
/// [`Milestone::Liberation`], and the median number of nodes bought after
/// that liberation is at least 1. A seed that never reaches the liberation
/// fails and is named.
///
/// Nodes bought after liberation `l` (runs completed) are the nodes learned
/// in the camps before runs `l + 1` onwards.
pub fn tree_outlasts_liberation(
    reports: &[PacingReport],
    liberations: u32,
) -> Result<String, String> {
    let liberation = Milestone::Liberation(liberations);
    let lib = stats_of(reports, liberation)?;
    let tree = stats_of(reports, Milestone::TreeExhausted)?;
    let max = max_runs(reports);
    let mut problems = Vec::new();
    let mut bought_after = Vec::new();
    for r in reports {
        match r.milestone(liberation) {
            None => problems.push(format!(
                "seed {} never reaches Liberation({liberations}) in {} runs",
                r.seed_index, r.profile.max_runs
            )),
            Some((run, _)) => bought_after.push(
                r.runs
                    .iter()
                    .filter(|x| x.index > run)
                    .map(|x| x.learned.len())
                    .sum::<usize>(),
            ),
        }
    }
    bought_after.sort_unstable();
    let bought = |num: usize, den: usize| {
        if bought_after.is_empty() {
            0
        } else {
            bought_after[(bought_after.len() - 1) * num / den]
        }
    };
    let (median, p25, p75) = (bought(1, 2), bought(1, 4), bought(3, 4));
    if tree.median <= lib.median {
        problems.push(format!(
            "Tree exhausted at median run {}, not after Liberation({liberations}) at median run {}",
            run_cell(tree.median, max),
            run_cell(lib.median, max)
        ));
    }
    if median < 1 {
        problems.push(format!(
            "median {median} nodes bought after Liberation({liberations}), need at least 1"
        ));
    }
    let summary = format!(
        "Liberation({liberations}) {} / Tree exhausted {} / nodes bought after it {median} [{p25}, {p75}] over {} reaching it",
        lib.cell(max),
        tree.cell(max),
        bought_after.len()
    );
    verdict(problems, summary)
}

/// The median run of `m` lies in `window`. `label` names the milestone.
pub fn median_run_within(
    reports: &[PacingReport],
    label: &str,
    m: Milestone,
    window: RangeInclusive<usize>,
) -> Result<String, String> {
    let s = stats_of(reports, m)?;
    let max = max_runs(reports);
    let summary = format!(
        "{label} {} (target runs {}-{})",
        s.cell(max),
        window.start(),
        window.end()
    );
    let mut problems = Vec::new();
    if !window.contains(&s.median) {
        problems.push(format!(
            "{label} at median run {}, outside runs {}-{}",
            run_cell(s.median, max),
            window.start(),
            window.end()
        ));
    }
    verdict(problems, summary)
}

/// Goal 5's tier rows: the median run of [`Milestone::TierAffordable`] of
/// each tier lies in its window (see [`GOAL5_TIER_WINDOWS`]). Every tier is
/// checked, and every miss is reported.
pub fn tier_affordable_runs(
    reports: &[PacingReport],
    windows: &[(usize, RangeInclusive<usize>)],
) -> Result<String, String> {
    let mut passed = Vec::new();
    let mut failed = Vec::new();
    for (tier, window) in windows {
        let label = format!("Tier {tier} affordable");
        match median_run_within(
            reports,
            &label,
            Milestone::TierAffordable(*tier),
            window.clone(),
        ) {
            Ok(summary) => passed.push(summary),
            Err(reason) => failed.push(reason),
        }
    }
    if failed.is_empty() {
        Ok(passed.join("; "))
    } else {
        failed.extend(passed);
        Err(failed.join("; "))
    }
}
