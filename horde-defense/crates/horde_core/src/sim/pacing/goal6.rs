//! Goal-6 acceptance evaluators (W2-04; design-enemy-balance §9, rulings
//! W-1 and W-4). Pure checks over pacing reports, shaped like the goal-5
//! evaluators in [`super::targets`]: `Ok` carries the summary the acceptance
//! prints, `Err` every reason it fails followed by that summary.
//! `tests/pacing_targets.rs` runs them on the release sweeps (expert-deu and
//! the goal-6 rows), and `tests/pacing_harness.rs` pins them on synthetic
//! reports.
//!
//! A run that is not a victory counts as a defeat: in the harness that is a
//! defeat or the 10-minute withdrawal. Run numbers are 1-based, and a missed
//! milestone counts as the profile's `max_runs + 1`, as in
//! [`MilestoneStats`].
use super::targets::{max_runs, median_run_within, run_cell, stats_of, verdict};
use super::{Milestone, MilestoneStats, PacingReport, lower_quantile};
use crate::balance::BalancePreset;
use crate::sim::Outcome;
use std::collections::BTreeMap;
use std::ops::RangeInclusive;

/// The LUX runs-to-liberate rows (ruling W-4), Easy, Normal and Hard.
pub const GOAL6_LUX_PROFILE_IDS: [&str; 3] = ["lux-easy", "lux-normal", "lux-hard"];
/// The start-country rows (ruling W-4): the lowest and highest country
/// difficulty bands, whose start the clamp holds near 1.0.
pub const GOAL6_START_PROFILE_IDS: [&str; 2] = ["start-aut", "start-bgd"];
/// G6.11: the sliding window, in runs after each seed's first victory.
pub const GOAL6_WINDOW_RUNS: usize = 20;
/// G6.11: the pooled defeat rate of every window, in percent (inclusive).
pub const GOAL6_DEFEAT_RATE_PCT: RangeInclusive<usize> = 10..=25;
/// G6.12: win streaks count from this run ("after run 10").
pub const GOAL6_STREAK_FROM_RUN: usize = 11;
/// G6.12: a streak this long fails, in any seed.
pub const GOAL6_STREAK_LIMIT: usize = 15;
/// G6.17: the median boss time-to-kill, in seconds (inclusive).
pub const GOAL6_BOSS_TTK_S: RangeInclusive<f32> = 20.0..=60.0;
/// G6.15: the median runs to liberate Luxembourg on Easy, at most.
pub const GOAL6_LUX_EASY_MAX_RUNS: usize = 6;
/// G6.15: Hard takes at most `num / den` (1.5) times Normal's median runs.
pub const GOAL6_LUX_HARD_OVER_NORMAL: (usize, usize) = (3, 2);
/// G6.18: the median first victory of a clamped start.
pub const GOAL6_CLAMPED_FIRST_VICTORY: RangeInclusive<usize> = 2..=4;

fn lost(outcome: Outcome) -> bool {
    outcome != Outcome::Victory
}
fn no_reports() -> String {
    "no pacing reports to evaluate".to_owned()
}
/// Whether each run after the seed's first victory was lost, in run order;
/// None when the seed never wins.
fn after_first_victory(report: &PacingReport) -> Option<Vec<bool>> {
    let first = report
        .runs
        .iter()
        .position(|r| r.outcome == Outcome::Victory)?;
    Some(
        report.runs[first + 1..]
            .iter()
            .map(|r| lost(r.outcome))
            .collect(),
    )
}

/// One sliding window, pooled over the seeds that have every run of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefeatWindow {
    /// Runs after each seed's first victory skipped before the window (0 is
    /// the run right after it).
    pub offset: usize,
    pub defeats: usize,
    pub runs: usize,
    /// Seeds with the whole window.
    pub seeds: usize,
}
impl DefeatWindow {
    /// The pooled defeat rate in percent.
    pub fn rate_pct(&self) -> f32 {
        if self.runs == 0 {
            0.0
        } else {
            self.defeats as f32 * 100.0 / self.runs as f32
        }
    }
    fn within(&self, pct: &RangeInclusive<usize>) -> bool {
        self.runs > 0
            && self.defeats * 100 >= pct.start() * self.runs
            && self.defeats * 100 <= pct.end() * self.runs
    }
}

/// Every sliding `window`-run window after the first victory. Windows are
/// aligned per seed on its own first victory; window `offset` pools the
/// seeds that played all `window` runs from `offset`. Seeds that never win
/// add nothing.
pub fn defeat_windows(reports: &[PacingReport], window: usize) -> Vec<DefeatWindow> {
    let seeds: Vec<Vec<bool>> = reports.iter().filter_map(after_first_victory).collect();
    let longest = seeds.iter().map(Vec::len).max().unwrap_or(0);
    if window == 0 || longest < window {
        return Vec::new();
    }
    (0..=longest - window)
        .map(|offset| {
            let mut w = DefeatWindow {
                offset,
                defeats: 0,
                runs: 0,
                seeds: 0,
            };
            for runs in &seeds {
                if let Some(slice) = runs.get(offset..offset + window) {
                    w.defeats += slice.iter().filter(|lost| **lost).count();
                    w.runs += window;
                    w.seeds += 1;
                }
            }
            w
        })
        .collect()
}

/// G6.11: the pooled defeat rate of every [`defeat_windows`] window lies in
/// [`GOAL6_DEFEAT_RATE_PCT`]. A seed that never wins fails and is named, and
/// no full window at all is not a pass. The summary names the seeds the
/// lowest and highest windows pool and the range over every window, since
/// tail windows pool only the seeds that play that long; no seed floor
/// applies.
pub fn defeat_rate_windows(reports: &[PacingReport], window: usize) -> Result<String, String> {
    if reports.is_empty() {
        return Err(no_reports());
    }
    let pct = GOAL6_DEFEAT_RATE_PCT;
    let mut problems = Vec::new();
    for r in reports {
        if after_first_victory(r).is_none() {
            problems.push(format!(
                "seed {} never wins in {} runs",
                r.seed_index,
                r.runs.len()
            ));
        }
    }
    let windows = defeat_windows(reports, window);
    let winners = reports
        .iter()
        .filter(|r| after_first_victory(r).is_some())
        .count();
    let Some(first) = windows.first() else {
        problems.push(format!("no seed has {window} runs after its first victory"));
        return verdict(problems, format!("0 windows over {winners} winning seeds"));
    };
    let (mut lowest, mut highest) = (first, first);
    for w in &windows {
        if w.rate_pct() < lowest.rate_pct() {
            lowest = w;
        }
        if w.rate_pct() > highest.rate_pct() {
            highest = w;
        }
    }
    let pooled = |w: &DefeatWindow| {
        format!(
            "{:.1}% at offset {} over {} seed{}",
            w.rate_pct(),
            w.offset,
            w.seeds,
            if w.seeds == 1 { "" } else { "s" }
        )
    };
    let extremes = format!("lowest {}, highest {}", pooled(lowest), pooled(highest));
    // Tail windows pool only the seeds that play that long after their
    // first victory; the range shows how thin the thinnest window is.
    let fewest = windows.iter().map(|w| w.seeds).min().unwrap_or(0);
    let most = windows.iter().map(|w| w.seeds).max().unwrap_or(0);
    let outside = windows.iter().filter(|w| !w.within(&pct)).count();
    if outside > 0 {
        problems.push(format!(
            "{outside} of {} windows outside {}-{}%: {extremes}",
            windows.len(),
            pct.start(),
            pct.end()
        ));
    }
    let summary = format!(
        "{} windows of {window} runs after the first victory over {winners} winning seeds: {extremes}; windows pool {fewest}-{most} seeds (target {}-{}%)",
        windows.len(),
        pct.start(),
        pct.end()
    );
    verdict(problems, summary)
}

/// The longest run of consecutive victories among runs `from_run` onwards.
pub fn longest_win_streak(report: &PacingReport, from_run: usize) -> usize {
    let (mut best, mut current) = (0, 0);
    for run in report.runs.iter().filter(|r| r.index >= from_run) {
        if run.outcome == Outcome::Victory {
            current += 1;
            best = best.max(current);
        } else {
            current = 0;
        }
    }
    best
}

/// G6.12: no seed has a [`GOAL6_STREAK_LIMIT`]-win streak from `from_run`
/// on. The rule is per seed, not a median; every offending seed is named.
pub fn max_win_streak(reports: &[PacingReport], from_run: usize) -> Result<String, String> {
    if reports.is_empty() {
        return Err(no_reports());
    }
    let streaks: Vec<(usize, usize)> = reports
        .iter()
        .map(|r| (r.seed_index, longest_win_streak(r, from_run)))
        .collect();
    let mut sorted: Vec<usize> = streaks.iter().map(|(_, s)| *s).collect();
    sorted.sort_unstable();
    let (worst_seed, worst) =
        streaks.iter().fold(
            (0, 0),
            |best, &(seed, s)| if s > best.1 { (seed, s) } else { best },
        );
    let problems = streaks
        .iter()
        .filter(|(_, s)| *s >= GOAL6_STREAK_LIMIT)
        .map(|(seed, s)| format!("seed {seed}: {s}-win streak from run {from_run}"))
        .collect();
    let summary = format!(
        "longest win streak from run {from_run}: median {}, max {worst} (seed {worst_seed}) over {} seeds; a {GOAL6_STREAK_LIMIT}-win streak fails",
        lower_quantile(&sorted, 1, 2),
        sorted.len()
    );
    verdict(problems, summary)
}

/// G6.17: the lower median of every boss time-to-kill in the sweep (pooled
/// over seeds and runs) lies in [`GOAL6_BOSS_TTK_S`]. The summary adds p25
/// and p90.
pub fn boss_ttk_median_s(reports: &[PacingReport]) -> Result<String, String> {
    if reports.is_empty() {
        return Err(no_reports());
    }
    let mut ttk: Vec<f32> = reports
        .iter()
        .flat_map(|r| r.runs.iter().filter_map(|x| x.boss_ttk_s))
        .collect();
    if ttk.is_empty() {
        let runs: usize = reports.iter().map(|r| r.runs.len()).sum();
        return Err(format!("no boss fell in {runs} runs"));
    }
    ttk.sort_by(f32::total_cmp);
    let median = lower_quantile(&ttk, 1, 2);
    let target = GOAL6_BOSS_TTK_S;
    let mut problems = Vec::new();
    if !target.contains(&median) {
        problems.push(format!(
            "Boss TTK median {median:.1} s, outside {}-{} s",
            target.start(),
            target.end()
        ));
    }
    let summary = format!(
        "Boss TTK median {median:.1} s [p25 {:.1}, p90 {:.1}] over {} boss kills (target {}-{} s)",
        lower_quantile(&ttk, 1, 4),
        lower_quantile(&ttk, 9, 10),
        ttk.len(),
        target.start(),
        target.end()
    );
    verdict(problems, summary)
}

/// G6.16: every archetype of `ids` (the catalogue's, in catalogue order) is
/// spawned and killed somewhere in the sweep. The summary lists spawned /
/// killed per id.
pub fn archetype_coverage(reports: &[PacingReport], ids: &[String]) -> Result<String, String> {
    if reports.is_empty() {
        return Err(no_reports());
    }
    let mut totals: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    for run in reports.iter().flat_map(|r| &r.runs) {
        for (id, a) in &run.archetypes {
            let t = totals.entry(id.as_str()).or_default();
            t.0 += u64::from(a.spawned);
            t.1 += u64::from(a.killed);
        }
    }
    let of = |id: &str| totals.get(id).copied().unwrap_or_default();
    let never_spawned: Vec<&str> = ids
        .iter()
        .map(String::as_str)
        .filter(|id| of(id).0 == 0)
        .collect();
    let never_killed: Vec<&str> = ids
        .iter()
        .map(String::as_str)
        .filter(|id| of(id).0 > 0 && of(id).1 == 0)
        .collect();
    let mut problems = Vec::new();
    if ids.is_empty() {
        problems.push("no archetype ids to check".to_owned());
    }
    if !never_spawned.is_empty() {
        problems.push(format!("never spawned: {}", never_spawned.join(", ")));
    }
    if !never_killed.is_empty() {
        problems.push(format!(
            "spawned but never killed: {}",
            never_killed.join(", ")
        ));
    }
    let covered = ids.len() - never_spawned.len() - never_killed.len();
    let counts = ids
        .iter()
        .map(|id| {
            let (spawned, killed) = of(id);
            format!("{id} {spawned}/{killed}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let summary = format!(
        "{covered}/{} archetypes spawned and killed over {} seeds (spawned/killed: {counts})",
        ids.len(),
        reports.len()
    );
    verdict(problems, summary)
}

/// G6.15: median runs to the first liberation of the three LUX sweeps
/// ([`GOAL6_LUX_PROFILE_IDS`]); a seed that never liberates counts as
/// `max_runs + 1` (21 for the 20-run rows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunsToLiberate {
    pub easy: MilestoneStats,
    pub normal: MilestoneStats,
    pub hard: MilestoneStats,
    /// The largest `max_runs` of the three sweeps.
    pub max_runs: usize,
}
/// [`RunsToLiberate`] of the Easy, Normal and Hard sweeps. Each sweep must
/// be non-empty and played on its preset.
pub fn runs_to_liberate(
    easy: &[PacingReport],
    normal: &[PacingReport],
    hard: &[PacingReport],
) -> Result<RunsToLiberate, String> {
    let sweeps = [
        ("Easy", BalancePreset::Easy, easy),
        ("Normal", BalancePreset::Normal, normal),
        ("Hard", BalancePreset::Hard, hard),
    ];
    for (label, preset, reports) in sweeps {
        if reports.is_empty() {
            return Err(format!("no {label} reports to evaluate"));
        }
        if let Some(r) = reports.iter().find(|r| r.profile.difficulty != preset) {
            return Err(format!(
                "the {label} sweep holds {} on {:?}, not {preset:?}",
                r.profile.id, r.profile.difficulty
            ));
        }
    }
    let liberation = Milestone::Liberation(1);
    Ok(RunsToLiberate {
        easy: stats_of(easy, liberation)?,
        normal: stats_of(normal, liberation)?,
        hard: stats_of(hard, liberation)?,
        max_runs: max_runs(easy).max(max_runs(normal)).max(max_runs(hard)),
    })
}
impl RunsToLiberate {
    fn summary(&self) -> String {
        format!(
            "LUX runs to liberate Easy {} / Normal {} / Hard {}",
            self.easy.cell(self.max_runs),
            self.normal.cell(self.max_runs),
            self.hard.cell(self.max_runs)
        )
    }
    /// The Easy median is at most `max` runs.
    pub fn easy_within(&self, max: usize) -> Result<String, String> {
        let mut problems = Vec::new();
        if self.easy.median > max {
            problems.push(format!(
                "Easy median run {} is above {max}",
                run_cell(self.easy.median, self.max_runs)
            ));
        }
        verdict(problems, self.summary())
    }
    /// The Hard median is at most `num / den` times the Normal median. A
    /// censored Hard median (its median seed never liberates, counted as
    /// `max_runs + 1`) is no run count and fails: it only bounds the true
    /// median from below. A censored Normal median against a measured Hard
    /// one passes, since Hard is then below any Normal run.
    pub fn hard_within_ratio(&self, (num, den): (usize, usize)) -> Result<String, String> {
        let mut problems = Vec::new();
        // The lower median sits at index (seeds - 1) / 2 of the sorted runs,
        // and every miss sorts after every hit.
        if self.hard.reached <= self.hard.seeds.saturating_sub(1) / 2 {
            problems.push(format!(
                "Hard median run {}: the median Hard seed never liberates in {} runs, so the {}x Normal ratio is unmeasured",
                run_cell(self.hard.median, self.max_runs),
                self.max_runs,
                num as f32 / den as f32
            ));
        } else if self.hard.median * den > self.normal.median * num {
            let ratio = num as f32 / den as f32;
            problems.push(format!(
                "Hard median run {} is above {ratio}x Normal median run {} ({})",
                run_cell(self.hard.median, self.max_runs),
                run_cell(self.normal.median, self.max_runs),
                self.normal.median as f32 * ratio
            ));
        }
        verdict(problems, self.summary())
    }
}

/// G6.18: a clamped start's median first victory lies in `window`. Seeds
/// that win run 1 are only reported (ruling W-4). Every report must be of
/// one profile, the start the label names.
pub fn clamped_start_first_victory(
    reports: &[PacingReport],
    window: RangeInclusive<usize>,
) -> Result<String, String> {
    let Some(first) = reports.first() else {
        return Err(no_reports());
    };
    if let Some(r) = reports.iter().find(|r| r.profile.id != first.profile.id) {
        return Err(format!(
            "the clamped start mixes {} ({}) with {} ({}); evaluate one start at a time",
            r.profile.id, r.profile.country, first.profile.id, first.profile.country
        ));
    }
    let label = format!("First victory, {} start", first.profile.country);
    let run_one: Vec<String> = reports
        .iter()
        .filter(|r| {
            r.runs
                .first()
                .is_some_and(|x| x.index == 1 && x.outcome == Outcome::Victory)
        })
        .map(|r| format!("seed {}", r.seed_index))
        .collect();
    let note = if run_one.is_empty() {
        "no run-1 victory".to_owned()
    } else {
        format!("run-1 victories: {} (reported only)", run_one.join(", "))
    };
    median_run_within(reports, &label, Milestone::FirstVictory, window)
        .map(|summary| format!("{summary}; {note}"))
        .map_err(|reason| format!("{reason}; {note}"))
}
