//! Prints the fresh-profile pacing table (runs and battle minutes to each
//! milestone). Run inside horde-defense/:
//! `cargo run --locked --release -p horde_core --example pacing -- [max_runs] [flags]`
//!
//! - `max_runs` overrides every selected profile's run limit.
//! - `--runs` lists every run under the table.
//! - `--profile ID|all` (repeatable) selects registry profiles; without it
//!   the three legacy profiles are printed.
//! - `--seeds N` plays seed indices `0..N` per profile (default 1) and
//!   prints `median [p25, p75] reached/seeds` cells.
//! - `--csv` prints one CSV row per run instead of the table.
//! - `--fit-tiers a,b,c` adds, per tier `d`, the median coins earned by run
//!   `a`, `b`, `c`… divided by the tier's cost.
//! - `--values` prints the tree value table (a stub until it lands).
//! - `--out PATH` writes the output to PATH (UTF-8, no BOM, LF) instead of
//!   stdout, so PowerShell's `>` is never needed.
//!
//! With no flag but `max_runs` and `--runs`, the output is the pre-v2 table.
//! Unknown or malformed arguments exit with status 2.
#![allow(clippy::print_stdout, clippy::print_stderr)]
use horde_core::defenders::DefenderId;
use horde_core::sim::{
    LEGACY_PROFILE_IDS, Milestone, MilestoneStats, PacingProfile, PacingReport, by_id,
    median_earned_after, pacing_archetype_ids, pacing_registry, pacing_sweep, tier_cost,
};
use horde_core::skill_catalog::CampaignFeature;
use std::fmt::Write as _;

const USAGE: &str = "usage: pacing [max_runs] [--runs] [--csv] [--out PATH] \
[--profile ID|all]... [--seeds N] [--fit-tiers a,b,c] [--values]";

#[derive(Default)]
struct Args {
    max_runs: Option<usize>,
    list_runs: bool,
    csv: bool,
    out: Option<String>,
    profiles: Vec<String>,
    seeds: Option<usize>,
    fit_tiers: Vec<usize>,
    values: bool,
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
            "--out" => out.out = Some(value("--out")?),
            "--profile" => {
                let id = value("--profile")?;
                if id != "all" && by_id(&id).is_none() {
                    return Err(format!("unknown profile {id}"));
                }
                out.profiles.push(id);
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
    Ok(out)
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
    if let Some(max_runs) = args.max_runs {
        for p in &mut profiles {
            p.max_runs = max_runs;
        }
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

fn fit_table(groups: &[&[PacingReport]], targets: &[usize]) -> String {
    let mut s = String::from("\n| Tier (target run, cost) |");
    for g in groups {
        let _ = write!(s, " {} |", g[0].profile.name);
    }
    s.push('\n');
    let _ = writeln!(s, "|---|{}", "---|".repeat(groups.len()));
    for (i, run) in targets.iter().enumerate() {
        let tier = i + 1;
        let cost = tier_cost(tier);
        let _ = write!(s, "| Tier {tier} (run {run}, {cost} coins) |");
        for g in groups {
            match median_earned_after(g, *run) {
                Some(earned) if cost > 0 => {
                    let _ = write!(s, " {:.2} ({earned}) |", earned as f64 / cost as f64);
                }
                _ => s.push_str(" n/a |"),
            }
        }
        s.push('\n');
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
        "The tree value table is not implemented yet (--values is a stub).\n".to_owned()
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
