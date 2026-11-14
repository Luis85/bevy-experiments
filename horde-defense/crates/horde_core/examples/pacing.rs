//! Prints the fresh-profile pacing table (runs and battle minutes to each
//! milestone). Run inside horde-defense/:
//! `cargo run --locked --release -p horde_core --example pacing -- [max_runs] [flags]`
//!
//! - `max_runs` overrides every selected profile's run limit.
//! - `--runs` lists every run under the table.
//! - `--profile ID|all` (repeatable) selects registry profiles; without it
//!   the three legacy profiles are printed.
//! - `--difficulty easy|normal|hard` and `--country ISO3` replace every
//!   selected profile's balance preset and start country; the table column
//!   heading then ends in `[ISO3, Preset]`.
//! - `--seeds N` plays seed indices `0..N` per profile (default 1) and
//!   prints `median [p25, p75] reached/seeds` cells.
//! - `--csv` prints one CSV row per run instead of the table.
//! - `--fit-tiers a,b,c` adds, per tier `d`, the median coins earned by run
//!   `a`, `b`, `c`… divided by the tier's cost. The median covers the seeds
//!   that played that run; a cell that misses some (a `stop_after` row, or
//!   past `max_runs`) names its `n/m seeds` and a note follows the table.
//! - `--values` prints the tree value table instead (CSV: one row per skill
//!   node with its axis, dominance group and ln gain at both reference
//!   ranks; `horde_core::campaign::tree_value`). It plays no run.
//! - `--strategy` plays the strategy rows (W2-43: strategy, neighbours-solo,
//!   neighbours-parallel) and prints the goal-3 table (`median [p90]` over
//!   `--seeds`) and the three goal-3 verdicts instead.
//! - `--out PATH` writes the output to PATH (UTF-8, no BOM, LF) instead of
//!   stdout, so PowerShell's `>` is never needed.
//!
//! With no flag but `max_runs` and `--runs`, the output is the pre-v2 table.
//! Unknown or malformed arguments exit with status 2.
#![allow(clippy::print_stdout, clippy::print_stderr)]
use horde_core::balance::BalancePreset;
use horde_core::defenders::DefenderId;
use horde_core::sim::{
    EarnedAfter, LEGACY_PROFILE_IDS, Milestone, MilestoneStats, PacingProfile, PacingReport,
    STRATEGY_PROFILE_ID, STRATEGY_PROFILE_IDS, StrategyReport, by_id,
    liberations_and_coins_per_minute, outbreak_reopens, pacing_archetype_ids, pacing_registry,
    pacing_sweep, parallel_frees_neighbours, strategy_sweep, strategy_table, tier_cost,
};
use horde_core::skill_catalog::CampaignFeature;
use horde_core::world::world;
use std::fmt::Write as _;

const USAGE: &str = "usage: pacing [max_runs] [--runs] [--csv] [--out PATH] \
[--profile ID|all]... [--difficulty easy|normal|hard] [--country ISO3] [--seeds N] [--fit-tiers a,b,c] [--values] [--strategy]";

#[derive(Default)]
struct Args {
    max_runs: Option<usize>,
    list_runs: bool,
    csv: bool,
    out: Option<String>,
    profiles: Vec<String>,
    seeds: Option<usize>,
    difficulty: Option<BalancePreset>,
    country: Option<&'static str>,
    fit_tiers: Vec<usize>,
    values: bool,
    strategy: bool,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match arg.as_str() {
            "--runs" => out.list_runs = true,
            "--csv" => out.csv = true,
            "--values" => out.values = true,
            "--strategy" => out.strategy = true,
            "--out" => out.out = Some(value("--out")?),
            "--profile" => {
                let id = value("--profile")?;
                if id != "all" && by_id(&id).is_none() {
                    return Err(format!("unknown profile {id}"));
                }
                out.profiles.push(id);
            }
            "--difficulty" => {
                let name = value("--difficulty")?;
                out.difficulty = Some(match name.as_str() {
                    "easy" => BalancePreset::Easy,
                    "normal" => BalancePreset::Normal,
                    "hard" => BalancePreset::Hard,
                    _ => {
                        return Err(format!(
                            "--difficulty needs easy, normal or hard, not {name}"
                        ));
                    }
                });
            }
            "--country" => {
                let id = value("--country")?;
                let country = world().countries.iter().find(|c| c.id == id);
                out.country = Some(
                    country
                        .map(|c| c.id.as_str())
                        .ok_or_else(|| format!("unknown country {id}"))?,
                );
            }
            "--seeds" => {
                let n = value("--seeds")?;
                match n.parse::<usize>() {
                    Ok(n) if n > 0 => out.seeds = Some(n),
                    _ => return Err(format!("--seeds needs a positive count, not {n}")),
                }
            }
            "--fit-tiers" => {
                let list = value("--fit-tiers")?;
                out.fit_tiers = list
                    .split(',')
                    .map(|t| match t.trim().parse::<usize>() {
                        Ok(run) if run > 0 => Ok(run),
                        _ => Err(format!("--fit-tiers needs run numbers, not {t}")),
                    })
                    .collect::<Result<_, _>>()?;
            }
            other => match other.parse::<usize>() {
                Ok(n) if n > 0 && out.max_runs.is_none() => out.max_runs = Some(n),
                _ => return Err(format!("unknown argument {other}")),
            },
        }
    }
    if out.csv && (out.list_runs || !out.fit_tiers.is_empty()) {
        return Err("--csv prints runs only; drop --runs and --fit-tiers".to_owned());
    }
    if out.strategy
        && (out.csv
            || out.list_runs
            || out.values
            || !out.profiles.is_empty()
            || out.difficulty.is_some()
            || out.country.is_some()
            || !out.fit_tiers.is_empty())
    {
        return Err(
            "--strategy prints the goal-3 table only; it takes max_runs, --seeds and --out"
                .to_owned(),
        );
    }
    Ok(out)
}

/// The goal-3 table of the strategy rows and the three acceptance verdicts.
fn strategy_text(args: &Args) -> String {
    let seeds = args.seeds.unwrap_or(1);
    let profiles: Vec<PacingProfile> = STRATEGY_PROFILE_IDS
        .iter()
        .filter_map(|id| by_id(id))
        .map(|p| PacingProfile {
            max_runs: args.max_runs.unwrap_or(p.max_runs),
            ..p
        })
        .collect();
    let reports = strategy_sweep(&profiles, seeds).expect("strategy profiles run");
    let row = |id: &str| -> Vec<StrategyReport> {
        reports
            .iter()
            .filter(|r| r.pacing.profile.id == id)
            .cloned()
            .collect()
    };
    let mut s = strategy_table(&reports);
    s.push('\n');
    for (name, verdict) in [
        (
            "G3.10 liberations 2/4/10 and coins per minute",
            liberations_and_coins_per_minute(&row(STRATEGY_PROFILE_ID)),
        ),
        (
            "G3.11 parallel frees 3-stage neighbours within 2/3 of solo runs",
            parallel_frees_neighbours(&row(STRATEGY_PROFILE_IDS[1]), &row(STRATEGY_PROFILE_IDS[2])),
        ),
        (
            "G3.12 fewer than 5 outbreak reopens over 10 liberations",
            outbreak_reopens(&row(STRATEGY_PROFILE_ID)),
        ),
    ] {
        match verdict {
            Ok(summary) => {
                let _ = writeln!(s, "{name}: PASS {summary}");
            }
            Err(reason) => {
                let _ = writeln!(s, "{name}: FAIL {reason}");
            }
        }
    }
    s
}

fn selected(args: &Args) -> Vec<PacingProfile> {
    let mut profiles: Vec<PacingProfile> = Vec::new();
    if args.profiles.is_empty() {
        profiles.extend(LEGACY_PROFILE_IDS.iter().filter_map(|id| by_id(id)));
    }
    for id in &args.profiles {
        if id == "all" {
            profiles.extend(pacing_registry().iter().copied());
        } else {
            profiles.extend(by_id(id));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    profiles.retain(|p| seen.insert(p.id));
    for p in &mut profiles {
        if let Some(max_runs) = args.max_runs {
            p.max_runs = max_runs;
        }
        if args.difficulty.is_none() && args.country.is_none() {
            continue;
        }
        p.difficulty = args.difficulty.unwrap_or(p.difficulty);
        p.country = args.country.unwrap_or(p.country);
        // The example's few profiles live until it exits.
        p.name = Box::leak(
            format!("{} [{}, {}]", p.name, p.country, p.difficulty.name()).into_boxed_str(),
        );
    }
    profiles
}

fn milestone_rows(extended: bool) -> Vec<(String, Milestone)> {
    let mut rows = vec![
        ("First victory".to_owned(), Milestone::FirstVictory),
        ("First liberation".to_owned(), Milestone::Liberation(1)),
    ];
    for d in 1..=4 {
        rows.push((
            format!("Tier {d} affordable ({} coins)", tier_cost(d)),
            Milestone::TierAffordable(d),
        ));
        rows.push((format!("Tier {d} first node"), Milestone::TierFirstNode(d)));
    }
    for f in CampaignFeature::ALL {
        rows.push((format!("{} unlocked", f.label()), Milestone::Feature(f)));
    }
    if extended {
        rows.push(("Second liberation".to_owned(), Milestone::Liberation(2)));
        for d in 1..=4 {
            rows.push((format!("Tier {d} owned"), Milestone::TierOwned(d)));
        }
        rows.push(("Tree exhausted".to_owned(), Milestone::TreeExhausted));
        for rank in [5, 10, 15, 20] {
            rows.push((format!("Rank {rank}"), Milestone::Rank(rank)));
        }
        rows.push(("First takeover".to_owned(), Milestone::FirstTakeover));
        rows.push(("Operations open".to_owned(), Milestone::OperationsOpen));
    }
    rows
}

fn table(groups: &[&[PacingReport]], seeds: usize, extended: bool) -> String {
    let mut s = String::new();
    if seeds == 1 {
        s.push_str("| Milestone (runs / battle min) |");
    } else {
        s.push_str("| Milestone (runs: median [p25, p75] reached/seeds) |");
    }
    for g in groups {
        let _ = write!(s, " {} |", g[0].profile.name);
    }
    s.push('\n');
    let _ = writeln!(s, "|---|{}", "---|".repeat(groups.len()));
    for (label, m) in milestone_rows(extended) {
        let _ = write!(s, "| {label} |");
        for g in groups {
            let max_runs = g[0].profile.max_runs;
            if seeds == 1 {
                match g[0].milestone(m) {
                    Some((run, minutes)) => {
                        let _ = write!(s, " {run} / {minutes:.0} |");
                    }
                    None => {
                        let _ = write!(s, " > {max_runs} |");
                    }
                }
            } else if let Some(stats) = MilestoneStats::of(g, m) {
                let _ = write!(s, " {} |", stats.cell(max_runs));
            }
        }
        s.push('\n');
    }
    s
}

fn run_list(reports: &[PacingReport], seeds: usize) -> String {
    let mut s = String::new();
    for r in reports {
        if seeds == 1 {
            let _ = writeln!(s, "\n## {}", r.profile.name);
        } else {
            let _ = writeln!(
                s,
                "\n## {}, seed {} ({:#x})",
                r.profile.name, r.seed_index, r.seed
            );
        }
        for run in &r.runs {
            let _ = writeln!(
                s,
                "{:>3} {} s{} {:?} {:.0}s +{} coins learned {:?}",
                run.index,
                run.country,
                run.stage + 1,
                run.outcome,
                run.elapsed,
                run.coins,
                run.learned
            );
        }
    }
    s
}

/// Printed under the fit table when a cell covers fewer than every seed.
const FIT_PARTIAL_NOTE: &str = "\nA cell with n/m seeds: only n of the m seeds played that run; \
the others stopped earlier (stop_after or max_runs), so its median leans toward slow seeds.\n";

/// One fit-table cell: median earned ÷ tier cost and the median, plus the
/// seed count when some seeds did not play the run.
fn fit_cell(earned: EarnedAfter, cost: u64) -> String {
    let ratio = earned.median as f64 / cost as f64;
    if earned.partial() {
        format!(
            " {ratio:.2} ({}, {}/{} seeds) |",
            earned.median, earned.sampled, earned.seeds
        )
    } else {
        format!(" {ratio:.2} ({}) |", earned.median)
    }
}

fn fit_table(groups: &[&[PacingReport]], targets: &[usize]) -> String {
    let mut s = String::from("\n| Tier (target run, cost) |");
    for g in groups {
        let _ = write!(s, " {} |", g[0].profile.name);
    }
    s.push('\n');
    let _ = writeln!(s, "|---|{}", "---|".repeat(groups.len()));
    let mut partial = false;
    for (i, run) in targets.iter().enumerate() {
        let tier = i + 1;
        let cost = tier_cost(tier);
        let _ = write!(s, "| Tier {tier} (run {run}, {cost} coins) |");
        for g in groups {
            match EarnedAfter::of(g, *run) {
                Some(earned) if cost > 0 => {
                    partial |= earned.partial();
                    s.push_str(&fit_cell(earned, cost));
                }
                _ => s.push_str(" n/a |"),
            }
        }
        s.push('\n');
    }
    if partial {
        s.push_str(FIT_PARTIAL_NOTE);
    }
    s
}

fn defender_label(d: DefenderId) -> String {
    match d {
        DefenderId::Player => "player".to_owned(),
        DefenderId::Mara => "mara".to_owned(),
        DefenderId::Recruit(id) => format!("recruit-{id}"),
    }
}

fn csv(reports: &[PacingReport]) -> String {
    let archetypes = pacing_archetype_ids();
    let mut s = String::from(
        "profile,seed,difficulty,run,country,stage,outcome,elapsed_s,coins,\
level_grant_coins,milestone_coins,cumulative_coins,rank,level,lanes,defender,kills,\
learned_count,learned_ids,spent_tree,spent_gear,spent_training,spent_recruit,\
auto_sold_count,auto_sold_coins,nodes_owned,nodes_left,wallet,boss_ttk_s",
    );
    for id in &archetypes {
        let _ = write!(s, ",{id}_spawned,{id}_killed,{id}_damage_taken");
    }
    s.push('\n');
    for r in reports {
        for run in &r.runs {
            let _ = write!(
                s,
                "{},{},{},{},{},{},{:?},{:.2},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                r.profile.id,
                run.seed,
                run.difficulty.name(),
                run.index,
                run.country,
                run.stage + 1,
                run.outcome,
                run.elapsed,
                run.coins,
                run.level_grant_coins,
                run.milestone_coins,
                run.cumulative_coins,
                run.rank,
                run.level,
                run.lanes,
                defender_label(run.defender),
                run.kills,
                run.learned.len(),
                run.learned.join(";"),
                run.spent_tree,
                run.spent_gear,
                run.spent_training,
                run.spent_recruit,
                run.auto_sold_count,
                run.auto_sold_coins,
                run.nodes_owned,
                run.nodes_left,
                run.wallet,
                run.boss_ttk_s.map_or(String::new(), |t| format!("{t:.2}")),
            );
            for id in &archetypes {
                let a = run.archetypes.get(id).copied().unwrap_or_default();
                let _ = write!(s, ",{},{},{}", a.spawned, a.killed, a.damage_taken);
            }
            s.push('\n');
        }
    }
    s
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse(&raw) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("pacing: {e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    let text = if args.values {
        horde_core::campaign::tree_value::value_table()
    } else if args.strategy {
        strategy_text(&args)
    } else {
        let profiles = selected(&args);
        let seeds = args.seeds.unwrap_or(1);
        let extended = !args.profiles.is_empty() || args.seeds.is_some();
        let reports = pacing_sweep(&profiles, seeds).expect("pacing profiles run");
        if args.csv {
            csv(&reports)
        } else {
            let groups: Vec<&[PacingReport]> = reports.chunks(seeds).collect();
            let mut s = table(&groups, seeds, extended);
            if args.list_runs {
                s.push_str(&run_list(&reports, seeds));
            }
            if !args.fit_tiers.is_empty() {
                s.push_str(&fit_table(&groups, &args.fit_tiers));
            }
            s
        }
    };
    match &args.out {
        Some(path) => {
            if let Err(e) = std::fs::write(path, text.as_bytes()) {
                eprintln!("pacing: cannot write {path}: {e}");
                std::process::exit(3);
            }
        }
        None => print!("{text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_cell_names_the_seed_count_when_some_seeds_stopped() {
        let all = EarnedAfter {
            median: 1500,
            sampled: 20,
            seeds: 20,
        };
        assert_eq!(fit_cell(all, 1000), " 1.50 (1500) |");
        let partial = EarnedAfter { sampled: 7, ..all };
        assert_eq!(fit_cell(partial, 1000), " 1.50 (1500, 7/20 seeds) |");
        assert!(FIT_PARTIAL_NOTE.contains("leans toward slow seeds"));
    }

    /// design-progression-tree §11's `--csv` header, written out in the spec.
    const DESIGN: &str =
        include_str!("../../../docs/plans/production-pass/design-progression-tree.md");

    /// The fixed columns of §11's CSV header (before the per-archetype ones).
    fn design_fixed_columns() -> Vec<&'static str> {
        let line = DESIGN
            .lines()
            .find_map(|l| l.trim_start().strip_prefix("- **CSV header:** "))
            .expect("design-progression-tree §11 writes out the CSV header");
        let fixed = line
            .split(", then")
            .next()
            .expect("the fixed columns precede the per-archetype ones");
        fixed.split(',').map(str::trim).collect()
    }

    fn csv_run(index: usize, killed: u32) -> horde_core::sim::PacingRun {
        use horde_core::sim::{ArchetypeRun, Outcome};
        let first = pacing_archetype_ids()
            .into_iter()
            .next()
            .expect("the catalogue has archetypes");
        horde_core::sim::PacingRun {
            index,
            seed: 7,
            difficulty: BalancePreset::Normal,
            country: "DEU".to_owned(),
            stage: 0,
            outcome: Outcome::Victory,
            country_freed: false,
            elapsed: 61.5,
            coins: 120,
            level_grant_coins: 0,
            milestone_coins: 0,
            cumulative_coins: 120 * index as u64,
            rank: 1,
            level: 2,
            lanes: 3,
            defender: DefenderId::Player,
            kills: killed,
            learned: vec!["a".to_owned(), "b".to_owned()],
            spent_tree: 0,
            spent_gear: 0,
            spent_training: 0,
            spent_recruit: 0,
            auto_sold_count: 0,
            auto_sold_coins: 0,
            items_owned: 4,
            nodes_owned: 2,
            nodes_left: 10,
            wallet: 40,
            boss_ttk_s: None,
            archetypes: [(
                first,
                ArchetypeRun {
                    spawned: killed + 1,
                    killed,
                    damage_taken: 9,
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn pacing_csv_has_header_and_one_row_per_run() {
        let profile = by_id("expert-deu").expect("registered");
        let report = |seed_index: usize, runs: usize| PacingReport {
            profile,
            seed_index,
            seed: 7,
            runs: (1..=runs).map(|i| csv_run(i, 3 * i as u32)).collect(),
            milestones: vec![],
        };
        let text = csv(&[report(0, 1), report(1, 2)]);
        let mut lines = text.lines();
        let header: Vec<&str> = lines.next().expect("a header line").split(',').collect();
        let fixed = design_fixed_columns();
        assert_eq!(fixed.len(), 29, "§11 names 29 fixed columns");
        let mut expected: Vec<String> = fixed.iter().map(|c| (*c).to_owned()).collect();
        for id in pacing_archetype_ids() {
            for suffix in ["spawned", "killed", "damage_taken"] {
                expected.push(format!("{id}_{suffix}"));
            }
        }
        assert_eq!(
            header, expected,
            "the header is section 11's, archetypes in catalogue order"
        );
        let rows: Vec<&str> = lines.collect();
        assert_eq!(rows.len(), 3, "one row per run across every report");
        for row in &rows {
            assert_eq!(
                row.split(',').count(),
                header.len(),
                "every row has the header's field count: {row}"
            );
        }
        assert!(
            rows[2].starts_with("expert-deu,7,Normal,2,DEU,1,Victory,61.50,120,"),
            "{}",
            rows[2]
        );
        assert!(text.ends_with('\n'), "the last row ends its line");
    }

    /// docs/BALANCE_AND_RUN_STATS.md, where the CLI is documented (W7-05).
    const BALANCE_DOC: &str = include_str!("../../../docs/BALANCE_AND_RUN_STATS.md");

    /// The flags `USAGE` names, in order.
    fn usage_flags() -> Vec<&'static str> {
        USAGE
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .filter(|word| word.starts_with("--"))
            .collect()
    }

    #[test]
    fn pacing_cli_doc_names_every_flag_profile_and_csv_column() {
        let start = BALANCE_DOC
            .lines()
            .position(|l| l.trim_end() == "## Pacing CLI")
            .expect("BALANCE_AND_RUN_STATS.md has a \"## Pacing CLI\" section");
        let section: String = BALANCE_DOC
            .lines()
            .skip(start + 1)
            .take_while(|l| !l.starts_with("## "))
            .collect::<Vec<_>>()
            .join("\n");
        let flags = usage_flags();
        assert_eq!(flags.len(), 10, "USAGE names the ten flags: {flags:?}");
        for flag in &flags {
            // Every USAGE flag is one the parser takes.
            let mut probe = vec![(*flag).to_owned()];
            match *flag {
                "--out" => probe.push("x.txt".to_owned()),
                "--profile" => probe.push("all".to_owned()),
                "--difficulty" => probe.push("hard".to_owned()),
                "--country" => probe.push("DEU".to_owned()),
                "--seeds" => probe.push("2".to_owned()),
                "--fit-tiers" => probe.push("4,8,12".to_owned()),
                _ => {}
            }
            assert!(parse(&probe).is_ok(), "{flag} parses");
        }
        let mut missing: Vec<String> = flags
            .iter()
            .filter(|flag| !section.contains(&format!("`{flag}")))
            .map(|flag| (*flag).to_owned())
            .collect();
        missing.extend(
            pacing_registry()
                .iter()
                .filter(|p| !section.contains(&format!("`{}`", p.id)))
                .map(|p| format!("profile {}", p.id)),
        );
        let fixed = design_fixed_columns().join(",");
        if !section.contains(&fixed) {
            missing.push(format!("the CSV header {fixed}"));
        }
        assert!(
            missing.is_empty(),
            "\"## Pacing CLI\" leaves out {missing:?}"
        );
    }

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn difficulty_and_country_override_every_selected_profile() {
        let args = parse(&argv(&["--difficulty", "hard", "--country", "AUT"])).expect("parses");
        let profiles = selected(&args);
        assert_eq!(profiles.len(), LEGACY_PROFILE_IDS.len());
        for p in &profiles {
            assert_eq!((p.difficulty, p.country), (BalancePreset::Hard, "AUT"));
            assert!(p.name.ends_with("[AUT, Hard]"), "{}", p.name);
        }
        let easy =
            parse(&argv(&["--profile", "lux-hard", "--difficulty", "easy"])).expect("parses");
        let lux = selected(&easy);
        assert_eq!(
            (lux[0].difficulty, lux[0].country),
            (BalancePreset::Easy, "LUX")
        );
        let plain = selected(&parse(&[]).expect("parses"));
        assert!(plain.iter().all(|p| !p.name.contains('[')));
        for bad in [
            &["--difficulty", "custom"][..],
            &["--difficulty"],
            &["--country", "XXX"],
            &["--country"],
            &["--strategy", "--country", "AUT"],
            &["--strategy", "--difficulty", "hard"],
        ] {
            assert!(parse(&argv(bad)).is_err(), "{bad:?}");
        }
    }
}
