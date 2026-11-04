//! Prints the fresh-profile pacing table (runs and battle minutes to each
//! early milestone). Run inside horde-defense/:
//! `cargo run --locked --release -p horde_core --example pacing [max_runs] [--runs]`
#![allow(clippy::print_stdout)]
use horde_core::sim::{Milestone, pacing_profiles, simulate_pacing, tier_cost};
use horde_core::skill_catalog::CampaignFeature;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let max_runs = args
        .iter()
        .find_map(|a| a.parse().ok())
        .unwrap_or(60usize);
    let list_runs = args.iter().any(|a| a == "--runs");
    let mut rows = vec![
        ("First victory".to_owned(), Milestone::FirstVictory),
        ("First liberation".to_owned(), Milestone::FirstLiberation),
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
    let reports: Vec<_> = pacing_profiles(max_runs)
        .iter()
        .map(|p| simulate_pacing(p).expect("pacing profile runs"))
        .collect();
    print!("| Milestone (runs / battle min) |");
    for r in &reports {
        print!(" {} |", r.profile.name);
    }
    println!();
    println!("|---|{}", "---|".repeat(reports.len()));
    for (label, m) in &rows {
        print!("| {label} |");
        for r in &reports {
            match r.milestone(*m) {
                Some((run, minutes)) => print!(" {run} / {minutes:.0} |"),
                None => print!(" > {max_runs} |"),
            }
        }
        println!();
    }
    if list_runs {
        for r in &reports {
            println!("\n## {}", r.profile.name);
            for run in &r.runs {
                println!(
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
    }
}
