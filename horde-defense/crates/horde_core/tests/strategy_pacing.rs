//! Strategy pacing (W2-43, design-campaign-defenders 1.13, rulings W-1 and
//! P7): the strategy rows on the progression-tree harness, the bot that acts
//! only through `Game::apply`, and the goal-3 evaluators on synthetic
//! reports. The release acceptance runs the same evaluators on the 20-seed
//! strategy sweep (tests/pacing_targets.rs).
use horde_core::command::Command;
use horde_core::sim::{
    DEFAULT_PROFILE_ID, DEU_THREE_STAGE_NEIGHBOURS, GOAL3_NEIGHBOURS_FREED, GOAL3_REOPEN_LIMIT,
    GOAL3_REOPEN_WINDOW, LEGACY_PROFILE_IDS, Milestone, Overrun, PacingParallel, PacingPlan,
    PacingProfile, PacingReport, PacingRoute, STRATEGY_PROFILE_ID, STRATEGY_PROFILE_IDS,
    StrategyPlan, StrategyReport, StrategyRun, StrategyStep, StrategyUnlock, by_id,
    liberations_and_coins_per_minute, outbreak_reopens, pacing_seed, pacing_start,
    parallel_frees_neighbours, reopens_over_liberations, replay_strategy, runs_to_free_neighbours,
    simulate_pacing, simulate_pacing_seed, simulate_strategy_seed, simulate_strategy_traced,
    strategy_table,
};
use horde_core::sim::{Game, Input};
use horde_core::strategy::{OutbreakOutcome, StrategyState};
use horde_core::world::{Country, world};
use std::collections::BTreeSet;

fn row(id: &str) -> PacingProfile {
    by_id(id).unwrap_or_else(|| panic!("{id} is registered"))
}

#[test]
fn pacing_seeds_vary_strategy_rng() {
    let strategy = row(STRATEGY_PROFILE_ID);
    let plan = strategy
        .plan
        .strategy
        .expect("the strategy row has a strategy plan");
    assert!(
        plan.seed_strategy,
        "the strategy rows seed the strategy RNG"
    );
    // Every seed index of a strategy row, index 0 included, starts its
    // strategy RNG from the game seed: never the shipped default and never
    // zero, and different per seed.
    let default = StrategyState::default().rng;
    let mut states = BTreeSet::new();
    for index in 0..20 {
        let g = pacing_start(&strategy, index);
        assert_eq!(
            g.seed(),
            pacing_seed(strategy.seed, index),
            "seed index {index}"
        );
        assert_ne!(g.strategy().rng, default, "seed index {index}");
        assert_ne!(g.strategy().rng.state, 0, "seed index {index}");
        assert_eq!(
            StrategyState {
                rng: default,
                ..g.strategy().clone()
            },
            StrategyState::default(),
            "only the RNG is seeded"
        );
        states.insert(g.strategy().rng.state);
    }
    assert_eq!(states.len(), 20, "each seed index has its own strategy RNG");
    for id in STRATEGY_PROFILE_IDS {
        assert!(
            row(id).plan.strategy.is_some_and(|p| p.seed_strategy),
            "{id} seeds the strategy RNG"
        );
    }
    // The solo plan keeps W2-01's seam: index 0 is today's default strategy
    // state (so one seed reproduces the legacy table), later indices vary.
    let expert = row(DEFAULT_PROFILE_ID);
    assert_eq!(
        pacing_start(&expert, 0).strategy().clone(),
        StrategyState::default()
    );
    assert_ne!(pacing_start(&expert, 1).strategy().rng, default);
}

#[test]
fn strategy_bot_acts_only_through_commands() {
    let profile = row("neighbours-parallel");
    let trace = simulate_strategy_traced(&profile, 0).expect("the strategy row runs");
    // The bot reaches the strategy layer: it recruits by choice, sends an
    // expedition and spends camp time.
    let commands: Vec<&Command> = trace
        .steps
        .iter()
        .filter_map(|s| match s {
            StrategyStep::Command(c) => Some(c),
            _ => None,
        })
        .collect();
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, Command::RecruitAs { .. })),
        "the bot recruits"
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, Command::DeploySurvivor { .. })),
        "the bot sends an expedition"
    );
    assert!(
        trace
            .steps
            .iter()
            .any(|s| matches!(s, StrategyStep::Idle(n) if *n == 1_800)),
        "the bot waits in camp"
    );
    let last = trace.report.runs.last().expect("runs");
    assert!(last.expedition_coins > 0 && last.expeditions_settled > 0);
    // Its commands and the clock alone rebuild the game it ended with, so
    // nothing it did bypassed `Game::apply`.
    let replayed = replay_strategy(&profile, 0, &trace.steps);
    assert_eq!(replayed.state_digest(), trace.digest);
    // The replay is sensitive: leaving out the first expedition changes it.
    let first = trace
        .steps
        .iter()
        .position(|s| matches!(s, StrategyStep::Command(Command::DeploySurvivor { .. })))
        .expect("an expedition was sent");
    let mut without = trace.steps.clone();
    without.remove(first);
    assert_ne!(
        replay_strategy(&profile, 0, &without).state_digest(),
        trace.digest
    );
}

/// W2 review: the S1 row's grand-strategy paths act only through
/// `Game::apply` too. Seed index 3 of the strategy row reaches them all
/// inside 40 runs (25 before W4-01: the Regional and Capstone boss terms
/// slow the front, and at 25 runs none of seed indices 0-12 still answers
/// an outbreak with the main run and resumes the country it left; 0-3 and
/// 6-11 do at 40): country operations once grand strategy unlocks, an
/// urgent outbreak answered by the main run (`SelectCountry` and
/// `BeginDefense` with no `ContinueJourney`) and by guards, the guards
/// released once it is contained, the interrupted country resumed, and
/// the other countries picked by the fewest-stages route. The bot's steps
/// alone, replayed through `Game::apply` and the clock, rebuild the game
/// it ended with; the state read before each command says which path took
/// it.
#[test]
fn s1_strategy_bot_acts_only_through_commands() {
    assert_eq!(
        s1_paths(3, 40),
        BTreeSet::from([
            "AssignGuard",
            "CountryOperation",
            "FewestStages",
            "ReleaseGuard",
            "resume",
            "urgent BeginDefense",
            "urgent SelectCountry",
        ])
    );
}

/// The grand-strategy paths seed `index` of the S1 row takes inside
/// `max_runs` runs, read from the state before each command while the
/// bot's steps are replayed through `Game::apply` and the clock; the
/// replay must rebuild the game the bot ended with.
fn s1_paths(index: usize, max_runs: usize) -> BTreeSet<&'static str> {
    let profile = PacingProfile {
        max_runs,
        ..row(STRATEGY_PROFILE_ID)
    };
    let route = profile.plan.strategy.expect("S1 plan").route;
    assert_eq!(route, PacingRoute::FewestStages);
    let trace = simulate_strategy_traced(&profile, index).expect("the S1 row runs");
    assert!(
        trace.report.strategy_unlock.is_some(),
        "grand strategy unlocks"
    );
    let mut g = pacing_start(&profile, index);
    let mut seen = BTreeSet::new();
    // A ContinueJourney earlier in this camp, and the country the main run
    // left to answer an outbreak.
    let (mut journey, mut left) = (false, None::<String>);
    for step in &trace.steps {
        let command = match step {
            StrategyStep::Idle(ticks) => {
                journey = false;
                for _ in 0..*ticks {
                    g.tick(Input::default());
                }
                continue;
            }
            StrategyStep::Tick(input) => {
                g.tick(input.clone());
                continue;
            }
            StrategyStep::Command(command) => command,
        };
        match command {
            Command::AssignGuard { .. } => {
                seen.insert("AssignGuard");
            }
            Command::ReleaseGuard(_) => {
                seen.insert("ReleaseGuard");
            }
            Command::CountryOperation { .. } => {
                assert!(g.strategy().grand_strategy_unlocked(g.campaign()));
                seen.insert("CountryOperation");
            }
            Command::ContinueJourney => journey = true,
            Command::SelectCountry(country) if !journey => {
                assert!(
                    g.strategy()
                        .outbreaks
                        .iter()
                        .any(|o| o.country_id == *country && o.outcome == OutbreakOutcome::Active),
                    "an urgent answer selects an active outbreak: {country}"
                );
                let w = &g.campaign().world;
                if left.is_none() {
                    left = w
                        .active
                        .clone()
                        .filter(|a| !w.cleared.contains(a) && a != country);
                }
                seen.insert("urgent SelectCountry");
            }
            Command::SelectCountry(country) => {
                if left.take().as_ref() == Some(country) {
                    seen.insert("resume");
                } else {
                    assert_eq!(*country, fewest_stages_front(&g), "the S1 route");
                    seen.insert("FewestStages");
                }
            }
            Command::BeginDefense { .. } if !journey => {
                seen.insert("urgent BeginDefense");
            }
            _ => {}
        }
        let _ = g.apply(command);
    }
    assert_eq!(
        g.state_digest(),
        trace.digest,
        "the commands and the clock alone rebuild the game"
    );
    seen
}

/// The S1 main run's next country after a liberation: the enterable, not
/// liberated front with the fewest stages, then the lowest difficulty and
/// id, preferring one no expedition holds.
fn fewest_stages_front(g: &Game) -> String {
    let w = &g.campaign().world;
    let live: BTreeSet<&str> = g
        .battles()
        .iter()
        .filter(|b| !b.settled)
        .map(|b| b.country_id.as_str())
        .collect();
    let fronts: Vec<_> = world()
        .countries
        .iter()
        .filter(|c| w.can_enter(&c.id) && !w.cleared.contains(&c.id))
        .collect();
    let order = |a: &&&Country, b: &&&Country| {
        a.level_count()
            .cmp(&b.level_count())
            .then(a.difficulty.total_cmp(&b.difficulty))
            .then(a.id.cmp(&b.id))
    };
    fronts
        .iter()
        .filter(|c| !live.contains(c.id.as_str()))
        .min_by(order)
        .or_else(|| fronts.iter().min_by(order))
        .map(|c| c.id.clone())
        .expect("a front is left")
}

#[test]
fn solo_plan_is_todays_profile() {
    assert_eq!(PacingPlan::SOLO.strategy, None);
    for p in horde_core::sim::pacing_registry() {
        if !STRATEGY_PROFILE_IDS.contains(&p.id) {
            assert_eq!(p.plan.strategy, None, "{} keeps its plan", p.id);
        }
    }
    // With every strategy option off, the command-driven strategy bot plays
    // exactly today's solo profile: same runs, same milestones, at the
    // profile's own seed and at a later one. Luxembourg is liberated inside
    // 8 runs, so the next-country step is covered.
    for (id, max_runs) in [
        (LEGACY_PROFILE_IDS[0], 4),
        (LEGACY_PROFILE_IDS[1], 4),
        (LEGACY_PROFILE_IDS[2], 8),
    ] {
        let solo = PacingProfile {
            max_runs,
            ..row(id)
        };
        let off = PacingProfile {
            plan: PacingPlan {
                strategy: Some(StrategyPlan::OFF),
                ..solo.plan
            },
            ..solo
        };
        for index in [0, 2] {
            let today = simulate_pacing_seed(&solo, index).expect("solo runs");
            let bot = simulate_strategy_seed(&off, index).expect("strategy-off runs");
            assert_eq!(bot.pacing.seed, today.seed, "{id} seed {index}");
            assert_eq!(bot.pacing.runs, today.runs, "{id} seed {index}");
            assert_eq!(bot.pacing.milestones, today.milestones, "{id} seed {index}");
        }
    }
    let lux = simulate_pacing(&PacingProfile {
        max_runs: 8,
        ..row(LEGACY_PROFILE_IDS[2])
    })
    .expect("solo runs");
    assert!(
        lux.milestone(Milestone::Liberation(1)).is_some(),
        "the pin covers a liberation"
    );
    assert!(lux.runs.len() > lux.milestone_or_missing(Milestone::Liberation(1)));
}

#[test]
fn strategy_rows_follow_the_design_plans() {
    assert_eq!(
        STRATEGY_PROFILE_IDS,
        ["strategy", "neighbours-solo", "neighbours-parallel"]
    );
    assert_eq!(STRATEGY_PROFILE_ID, "strategy");
    let s1 = row("strategy");
    let lux = row("novice-gear-lux");
    assert_eq!(
        (s1.seed, s1.country, s1.aim, s1.spend, s1.max_runs),
        (lux.seed, lux.country, lux.aim, lux.spend, 120)
    );
    assert_eq!(
        s1.plan.strategy,
        Some(StrategyPlan {
            recruit_up_to: 4,
            parallel: PacingParallel::Operations,
            guards: true,
            camp_ticks: 1_800,
            route: PacingRoute::FewestStages,
            countries: &[],
            seed_strategy: true,
        })
    );
    let (solo, parallel) = (row("neighbours-solo"), row("neighbours-parallel"));
    let expert = row(DEFAULT_PROFILE_ID);
    for p in [solo, parallel] {
        assert_eq!(
            (p.seed, p.country, p.aim, p.spend, p.difficulty),
            (
                expert.seed,
                expert.country,
                expert.aim,
                expert.spend,
                expert.difficulty
            )
        );
        // Germany plus K neighbours.
        assert_eq!(
            p.stop_after,
            Some(Milestone::Liberation(1 + GOAL3_NEIGHBOURS_FREED as u32))
        );
        let plan = p.plan.strategy.expect("strategy plan");
        assert_eq!(plan.camp_ticks, 1_800);
        assert_eq!(plan.countries, &DEU_THREE_STAGE_NEIGHBOURS[..]);
    }
    assert_eq!(
        DEU_THREE_STAGE_NEIGHBOURS,
        ["AUT", "BEL", "CHE", "CZE", "DNK", "LUX", "NLD"]
    );
    for id in DEU_THREE_STAGE_NEIGHBOURS {
        let c = horde_core::world::world().country(id).expect("known");
        assert_eq!(c.level_count(), 3, "{id}");
        assert!(c.neighbors.iter().any(|n| n == "DEU"), "{id}");
    }
    let (a, b) = (
        solo.plan.strategy.expect("plan"),
        parallel.plan.strategy.expect("plan"),
    );
    assert_eq!((a.recruit_up_to, a.parallel), (0, PacingParallel::Off));
    assert_eq!(
        (b.recruit_up_to, b.parallel),
        (3, PacingParallel::Operations)
    );
    // Only the parallel options differ between the two.
    assert_eq!(
        StrategyPlan {
            recruit_up_to: 0,
            parallel: PacingParallel::Off,
            ..b
        },
        a
    );
    // Every strategy row is accepted at zero runs.
    for id in STRATEGY_PROFILE_IDS {
        let p = PacingProfile {
            max_runs: 0,
            ..row(id)
        };
        assert!(simulate_strategy_seed(&p, 0).is_ok(), "{id}");
        assert!(simulate_pacing(&p).is_ok(), "{id}");
    }
    // A strategy plan cannot be combined with the solo-only W2-42 options.
    let mixed = PacingProfile {
        plan: PacingPlan {
            farm: horde_core::sim::PacingFarm::ReplayAfterDefeats(2),
            ..s1.plan
        },
        max_runs: 0,
        ..s1
    };
    assert!(simulate_strategy_seed(&mixed, 0).is_err());
}

#[test]
fn neighbours_solo_frees_the_route_countries_in_route_order() {
    let report = simulate_strategy_seed(&row("neighbours-solo"), 0).expect("runs");
    let freed = runs_to_free_neighbours(&report, GOAL3_NEIGHBOURS_FREED)
        .expect("the solo route frees three neighbours inside 60 runs");
    assert!(freed > 0);
    // Germany first, then only the listed neighbours; camp time is counted.
    let countries: Vec<&str> = report
        .pacing
        .runs
        .iter()
        .map(|r| r.country.as_str())
        .collect();
    assert_eq!(countries[0], "DEU");
    assert!(
        countries
            .iter()
            .all(|c| *c == "DEU" || DEU_THREE_STAGE_NEIGHBOURS.contains(c)),
        "{countries:?}"
    );
    let last = report.runs.last().expect("runs");
    assert_eq!(last.route_freed, GOAL3_NEIGHBOURS_FREED as u32);
    let camp_runs = report.runs.len() - 1;
    assert!((last.camp_s - camp_runs as f32 * 30.0).abs() < 0.01);
    assert!(last.minutes_total > report.pacing.runs.iter().map(|r| r.elapsed).sum::<f32>() / 60.0);
}

// Goal-3 evaluators on synthetic reports.

/// A synthetic strategy report: one run per entry of `liberated` (the
/// liberation count after it), each run 60 battle seconds and 30 camp
/// seconds, earning `coins` per run, with the route's freed count from
/// `route_freed`.
fn synthetic(
    profile: &str,
    seed_index: usize,
    liberated: &[u32],
    route_freed: &[u32],
    coins: u64,
) -> StrategyReport {
    let base = row(profile);
    let mut milestones: Vec<(Milestone, usize, f32)> = Vec::new();
    for (i, n) in liberated.iter().enumerate() {
        for k in 1..=*n {
            if !milestones
                .iter()
                .any(|(m, ..)| *m == Milestone::Liberation(k))
            {
                milestones.push((Milestone::Liberation(k), i + 1, (i + 1) as f32));
            }
        }
    }
    let runs = liberated
        .iter()
        .enumerate()
        .map(|(i, n)| StrategyRun {
            index: i + 1,
            camp_s: 30.0 * i as f32,
            expedition_coins: 0,
            expeditions_settled: 0,
            expedition_stages: 0,
            outbreaks: 0,
            coins_total: coins * (i as u64 + 1),
            minutes_total: 1.5 * (i + 1) as f32,
            liberated: *n,
            route_freed: route_freed.get(i).copied().unwrap_or(0),
            overruns: 0,
            roster: 0,
            guards: 0,
            expeditions: 0,
        })
        .collect();
    StrategyReport {
        pacing: PacingReport {
            profile: base,
            seed_index,
            seed: pacing_seed(base.seed, seed_index),
            runs: vec![],
            milestones,
        },
        runs,
        overruns: vec![],
        strategy_unlock: None,
    }
}

#[test]
fn liberation_and_coin_rate_report_needs_most_seeds_to_reach_each_liberation() {
    // 10 liberations by run 12 in every seed; 90 coins per 1.5 minutes.
    let lib: Vec<u32> = (1..=12).map(|r: u32| r.saturating_sub(2)).collect();
    let reports: Vec<StrategyReport> = (0..4)
        .map(|s| synthetic("strategy", s, &lib, &[], 90))
        .collect();
    let summary = liberations_and_coins_per_minute(&reports).expect("every seed reports");
    assert!(summary.contains("liberation 2: runs 4"), "{summary}");
    assert!(summary.contains("liberation 10: runs 12"), "{summary}");
    assert!(summary.contains("coins/min 60.0"), "{summary}");
    // Three seeds in four short of 10 liberations: the lower median is
    // missing.
    let short: Vec<u32> = (1..=12).map(|r: u32| r.saturating_sub(2).min(9)).collect();
    let mixed: Vec<StrategyReport> = (0..4)
        .map(|s| synthetic("strategy", s, if s < 1 { &lib } else { &short }, &[], 90))
        .collect();
    let err = liberations_and_coins_per_minute(&mixed).expect_err("liberation 10 is missing");
    assert!(err.contains("liberation 10"), "{err}");
    assert!(liberations_and_coins_per_minute(&[]).is_err());
}

#[test]
fn liberation_minutes_count_a_missed_seed_like_the_runs_do() {
    // One seed in four reaches liberation 10 (run 12, 18 minutes); three
    // stop at 9. The runs median counts the misses as max_runs + 1, so the
    // minutes median over the same four seeds is missed too, not the one
    // seed's 18 minutes.
    let lib: Vec<u32> = (1..=12).map(|r: u32| r.saturating_sub(2)).collect();
    let short: Vec<u32> = (1..=12).map(|r: u32| r.saturating_sub(2).min(9)).collect();
    let mixed: Vec<StrategyReport> = (0..4)
        .map(|s| synthetic("strategy", s, if s < 1 { &lib } else { &short }, &[], 90))
        .collect();
    let err = liberations_and_coins_per_minute(&mixed).expect_err("liberation 10 is missing");
    assert!(
        err.contains("1/4, minutes missed [p90 missed]"),
        "liberation 10 minutes over every seed: {err}"
    );
    // Liberation 2, reached by every seed at run 4 (6 minutes), is unchanged.
    assert!(
        err.contains("liberation 2: runs 4 [p90 4] 4/4, minutes 6 [p90 6]"),
        "{err}"
    );
    let table = strategy_table(&mixed);
    let row = table
        .lines()
        .find(|l| l.starts_with("| Liberation 10 (minutes)"))
        .expect("minutes row");
    assert_eq!(
        row, "| Liberation 10 (minutes) | missed [missed] |",
        "{table}"
    );
    // Two seeds in four reach it: the lower median is theirs, the p90 is
    // missed.
    let two: Vec<StrategyReport> = (0..4)
        .map(|s| synthetic("strategy", s, if s < 2 { &lib } else { &short }, &[], 90))
        .collect();
    let row = strategy_table(&two)
        .lines()
        .find(|l| l.starts_with("| Liberation 10 (minutes)"))
        .map(str::to_owned)
        .expect("minutes row");
    assert_eq!(row, "| Liberation 10 (minutes) | 18 [missed] |");
}

#[test]
fn parallel_must_free_neighbours_within_two_thirds_of_solo_runs() {
    // Germany liberated at run 3; the solo route frees the third neighbour
    // 9 runs later.
    let solo_lib = [0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4];
    let solo_free = [0, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 3];
    let solo: Vec<StrategyReport> = (0..3)
        .map(|s| synthetic("neighbours-solo", s, &solo_lib, &solo_free, 10))
        .collect();
    assert_eq!(runs_to_free_neighbours(&solo[0], 3), Some(9));
    // 6 runs is exactly 2/3 of 9: accepted.
    let fast_lib = [0, 0, 1, 1, 2, 3, 3, 3, 4];
    let fast_free = [0, 0, 0, 0, 1, 2, 2, 2, 3];
    let fast: Vec<StrategyReport> = (0..3)
        .map(|s| synthetic("neighbours-parallel", s, &fast_lib, &fast_free, 10))
        .collect();
    assert_eq!(runs_to_free_neighbours(&fast[0], 3), Some(6));
    let ok = parallel_frees_neighbours(&solo, &fast).expect("6 <= 9 * 2/3");
    assert!(ok.contains("parallel 6"), "{ok}");
    assert!(ok.contains("solo 9"), "{ok}");
    // 7 runs is not.
    let slow_lib = [0, 0, 1, 1, 2, 2, 3, 3, 3, 4];
    let slow_free = [0, 0, 0, 0, 1, 1, 2, 2, 2, 3];
    let slow: Vec<StrategyReport> = (0..3)
        .map(|s| synthetic("neighbours-parallel", s, &slow_lib, &slow_free, 10))
        .collect();
    let err = parallel_frees_neighbours(&solo, &slow).expect_err("7 > 6");
    assert!(err.contains("parallel 7"), "{err}");
    // A solo route that never frees K neighbours cannot be compared.
    let stuck: Vec<StrategyReport> = (0..3)
        .map(|s| synthetic("neighbours-solo", s, &[0, 1, 1, 2], &[0, 0, 0, 1], 10))
        .collect();
    assert_eq!(runs_to_free_neighbours(&stuck[0], 3), None);
    assert!(parallel_frees_neighbours(&stuck, &fast).is_err());
}

#[test]
fn reopens_count_overruns_in_the_ten_liberations_after_the_strategy_unlock() {
    let lib: Vec<u32> = (1..=20).collect();
    let mut report = synthetic("strategy", 0, &lib, &[], 10);
    report.strategy_unlock = Some(StrategyUnlock {
        run: 4,
        liberated: 4,
    });
    let overrun = |run: usize, liberated: u32| Overrun {
        run,
        liberated,
        country: "LUX".to_owned(),
    };
    // Liberations 4..14 are the window: two inside, one before (impossible
    // in play, but excluded), one at the window's end (excluded).
    report.overruns = vec![
        overrun(3, 3),
        overrun(5, 4),
        overrun(9, 13),
        overrun(15, 14),
    ];
    assert_eq!(
        reopens_over_liberations(&report, GOAL3_REOPEN_WINDOW),
        Some(2)
    );
    // A window that never completes is missing.
    let mut short = report.clone();
    short.runs.truncate(12);
    assert_eq!(reopens_over_liberations(&short, GOAL3_REOPEN_WINDOW), None);
    let mut never = report.clone();
    never.strategy_unlock = None;
    assert_eq!(reopens_over_liberations(&never, GOAL3_REOPEN_WINDOW), None);
    // The median over seeds must stay below the limit; a missing window
    // counts as the limit.
    let pass = vec![report.clone(), report.clone(), short.clone()];
    let ok = outbreak_reopens(&pass).expect("median 2 < 5");
    assert!(ok.contains("median 2"), "{ok}");
    let mut many = report.clone();
    many.overruns = (0..GOAL3_REOPEN_LIMIT as usize)
        .map(|i| overrun(5 + i, 5 + i as u32))
        .collect();
    let fail = vec![many.clone(), many, short];
    let err = outbreak_reopens(&fail).expect_err("median 5 is not < 5");
    assert!(err.contains("median 5"), "{err}");
}

#[test]
fn strategy_table_prints_every_goal3_row() {
    let lib: Vec<u32> = (1..=12).map(|r: u32| r.saturating_sub(2)).collect();
    let free: Vec<u32> = (1..=12).map(|r: u32| r.saturating_sub(3).min(3)).collect();
    let reports: Vec<StrategyReport> = STRATEGY_PROFILE_IDS
        .iter()
        .flat_map(|id| (0..2).map(|s| synthetic(id, s, &lib, &free, 30)))
        .collect();
    let table = strategy_table(&reports);
    for line in [
        "Liberation 2 (runs)",
        "Liberation 4 (minutes)",
        "Liberation 10 (runs)",
        "Coins per minute",
        "Battle minutes",
        "Camp minutes",
        "Expeditions settled",
        "Expedition stages cleared",
        "Outbreaks / overruns (whole profile)",
        "Runs to free 3 neighbours",
        "Reopens over 10 liberations",
        "Parallel / solo runs to free 3 neighbours",
    ] {
        assert!(table.contains(line), "{line} missing from\n{table}");
    }
    for id in STRATEGY_PROFILE_IDS {
        assert!(table.contains(row(id).name), "{id} column");
    }
}

/// W4-15 (ruling R61): Survivor network is priced so that the S2b bot owns
/// it before the camp that opens Parallel (the camp after the second
/// liberation), so its recruits are on the roster when the first
/// expedition can leave. At the W4-p price (1,700 coins) seed index 0
/// learns it in that camp, a camp too late (plan section 9.6, "W4-c").
/// Camp milestones count the runs completed before the camp that first
/// held them.
#[test]
fn survivor_network_opens_a_camp_earlier_than_the_w4_p_price() {
    use horde_core::skill_catalog::CampaignFeature;
    let profile = PacingProfile {
        stop_after: Some(Milestone::Liberation(2)),
        ..row("neighbours-parallel")
    };
    let report = simulate_strategy_seed(&profile, 0).expect("the strategy row runs");
    let second = report.pacing.milestone_or_missing(Milestone::Liberation(2));
    let network = report
        .pacing
        .milestone_or_missing(Milestone::Feature(CampaignFeature::Recruitment));
    assert!(
        network < second,
        "Survivor network owned after {network} runs, the second liberation after {second}"
    );
}
