#![allow(clippy::print_stdout, clippy::print_stderr)] // CLI example / test diagnostics
//! CPU-only peak-encounter measurement. Fixture creation and resets are excluded;
//! cap replenishment and the complete authoritative fixed step are included.
use horde_core::{
    campaign::{Placement, Skill},
    content::{EnemyKind, Mode, Slot, StructureKind, content},
    sim::{Game, Input, MAX_EVENTS, smoke_input},
    world::world,
};
use serde::Serialize;
use std::time::Instant;

#[derive(Serialize)]
struct Measurement {
    case: &'static str,
    samples: usize,
    simulated_seconds: f64,
    mean_us: f64,
    p50_us: f64,
    p95_us: f64,
    p99_us: f64,
    max_us: f64,
    peak_live_enemies: usize,
    peak_structures: usize,
    peak_events: usize,
    peak_log_lines: usize,
    fixture_resets: u64,
}
#[derive(Serialize)]
struct Report {
    cpu: String,
    profile: &'static str,
    notes: Vec<&'static str>,
    measurements: Vec<Measurement>,
}

fn prepare() -> Result<Game, String> {
    let mut g = Game::default();
    for level in 0..5 {
        g.campaign_mut().first_clear(level, Mode::Personal);
        g.campaign_mut().finish_attempt();
    }
    g.campaign_mut().reward(50_000, 0, Mode::Personal);
    for skill in Skill::ALL {
        g.campaign_mut().purchase_skill(skill)?;
    }
    for kind in StructureKind::ALL {
        g.campaign_mut().purchase_blueprint(kind)?;
        for _ in 0..3 {
            g.campaign_mut().upgrade_structure(kind)?;
        }
    }
    let mut plan = Vec::new();
    for lane in 0..8 {
        plan.push(Placement {
            kind: StructureKind::Wall,
            lane,
            column: 0,
        });
    }
    plan.push(Placement {
        kind: StructureKind::Turret,
        lane: 5,
        column: 1,
    });
    plan.push(Placement {
        kind: StructureKind::Trap,
        lane: 5,
        column: 4,
    });
    g.campaign_mut().save_layout(plan, 4)?;
    Ok(g)
}
fn best_gear(g: &mut Game, mode: Mode) -> Result<(), String> {
    for slot in Slot::ALL {
        let definition = content()
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.slot == slot && item.purchasable())
            .max_by_key(|(_, item)| item.tier)
            .map(|(n, _)| n)
            .ok_or("Missing market gear")?;
        let id = g.campaign_mut().buy_item(definition)?;
        g.campaign_mut().equip(mode, id)?;
    }
    Ok(())
}
fn populate(g: &mut Game, initial: bool) {
    let Some(mut a) = g.attempt_slot_mut().take() else {
        return;
    };
    if initial {
        a.tick = (content().levels[a.level].boss_time * 60.0).round() as u64;
    }
    let cap = content().levels[a.level].enemy_cap;
    while a.enemies.iter().filter(|e| !e.boss).count() < cap {
        let n = a.enemies.iter().filter(|e| !e.boss).count();
        g.spawn_enemy(
            &mut a,
            EnemyKind::ALL[n % EnemyKind::ALL.len()],
            (n % 11) as u8,
            false,
        );
        if initial {
            a.enemies.last_mut().unwrap().x = 15.5 + (n / 11) as f32 * 2.1;
        }
    }
    if !a.boss_spawned {
        g.spawn_enemy(&mut a, EnemyKind::Brute, 5, true);
        a.boss_spawned = true;
    }
    *g.attempt_slot_mut() = Some(a);
}
fn single(mode: Mode) -> Result<Game, String> {
    let mut g = prepare()?;
    best_gear(&mut g, mode)?;
    g.deploy(4, mode)?;
    g.set_paused(false);
    populate(&mut g, true);
    Ok(g)
}
fn parallel() -> Result<Game, String> {
    let mut g = prepare()?;
    g.campaign_mut().choose_start_country("USA")?;
    for id in ["USA", "CAN", "MEX", "BRA"] {
        let count = world()
            .country(id)
            .ok_or("Missing benchmark country")?
            .level_count();
        g.campaign_mut()
            .world
            .country_levels
            .insert(id.into(), count);
        g.campaign_mut().world.cleared.push(id.into());
        g.campaign_mut().world.unlocked.push(id.into());
    }
    for _ in 0..4 {
        let recruit = g.with_strategy(|strategy, campaign| strategy.recruit(campaign, "USA"))?;
        for slot in Slot::ALL {
            let definition = content()
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.slot == slot && item.purchasable())
                .max_by_key(|(_, item)| item.tier)
                .map(|(n, _)| n)
                .ok_or("Missing market gear")?;
            let id = g.campaign_mut().buy_item(definition)?;
            g.with_strategy(|strategy, campaign| strategy.equip_recruit(campaign, recruit, id))?;
        }
        g.deploy_survivor(recruit, "USA", 4)?;
    }
    for battle in g.battles_mut() {
        populate(battle.game_mut(), true);
    }
    Ok(g)
}
fn step(g: &mut Game) {
    populate(g, false);
    for battle in g.battles_mut() {
        if !battle.settled {
            populate(battle.game_mut(), false);
        }
    }
    let input = if g.attempt().is_some_and(|a| a.mode == Mode::Personal) {
        smoke_input(g)
    } else {
        Input::default()
    };
    g.tick(input);
}
fn measure(case: &'static str, fixture: Game, samples: usize) -> Measurement {
    let is_parallel = !fixture.battles().is_empty();
    let mut g = fixture.clone();
    let mut timings = Vec::with_capacity(samples);
    let mut resets = 0;
    let initial_live: usize = fixture
        .attempt()
        .into_iter()
        .chain(fixture.battles().iter().filter_map(|b| b.game().attempt()))
        .map(|a| a.enemies.len())
        .sum();
    let (mut max_live, mut max_structures, mut max_events, mut max_logs) = (initial_live, 0, 0, 0);
    for n in 0..samples + 600 {
        if (!is_parallel && g.attempt().is_none())
            || (is_parallel && g.battles().iter().any(|b| b.settled))
        {
            g = fixture.clone();
            resets += 1;
        }
        let start = Instant::now();
        step(&mut g);
        let elapsed = start.elapsed().as_secs_f64() * 1_000_000.0;
        if n >= 600 {
            timings.push(elapsed);
        }
        let attempts = g
            .attempt()
            .into_iter()
            .chain(g.battles().iter().filter_map(|b| b.game().attempt()));
        let mut live = 0;
        let mut structures = 0;
        let mut events = 0;
        let mut logs = 0;
        for a in attempts {
            assert!(a.enemies.len() <= content().levels[a.level].enemy_cap + 1);
            assert!(a.events.len() <= MAX_EVENTS);
            assert!(a.log.len() <= 80);
            live += a.enemies.len();
            structures += a.structures.len();
            events += a.events.len();
            logs += a.log.len();
        }
        max_live = max_live.max(live);
        max_structures = max_structures.max(structures);
        max_events = max_events.max(events);
        max_logs = max_logs.max(logs);
    }
    timings.sort_by(f64::total_cmp);
    let mean = timings.iter().sum::<f64>() / samples as f64;
    Measurement {
        case,
        samples,
        simulated_seconds: samples as f64 / 60.0,
        mean_us: mean,
        p50_us: timings[samples / 2],
        p95_us: timings[samples * 95 / 100],
        p99_us: timings[samples * 99 / 100],
        max_us: timings[samples - 1],
        peak_live_enemies: max_live,
        peak_structures: max_structures,
        peak_events: max_events,
        peak_log_lines: max_logs,
        fixture_resets: resets,
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let samples = args
        .windows(2)
        .find(|pair| pair[0] == "--ticks")
        .map(|pair| pair[1].parse::<usize>())
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or(12_000)
        .clamp(100, 1_000_000);
    let report = Report {
        cpu: std::env::var("LASTLIGHT_BENCH_CPU")
            .or_else(|_| std::env::var("PROCESSOR_IDENTIFIER"))
            .unwrap_or_else(|_| "not reported".into()),
        profile: if cfg!(debug_assertions) {
            "dev opt-level 1; dependencies opt-level 2"
        } else {
            "release"
        },
        notes: vec![
            "CPU-only simulation; no renderer, audio, UI or GUI frame-rate claim.",
            "72 ordinary enemies plus one reserved boss per field; purchased fortifications and ordinary market gear.",
            "Fixture creation and reset clones excluded; maintaining peak population and the complete fixed step included.",
            "600 untimed warmup steps; repeated seeded fixtures after a terminal encounter; no balance or survival claim.",
        ],
        measurements: vec![
            measure("personal_peak", single(Mode::Personal)?, samples),
            measure("npc_peak", single(Mode::Npc)?, samples),
            measure("four_parallel_peaks", parallel()?, samples),
        ],
    };
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(pair) = args.windows(2).find(|pair| pair[0] == "--output") {
        let path = std::path::Path::new(&pair[1]);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, &json).map_err(|e| e.to_string())?;
    }
    println!("{json}");
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Benchmark failed: {error}");
        std::process::exit(1);
    }
}
