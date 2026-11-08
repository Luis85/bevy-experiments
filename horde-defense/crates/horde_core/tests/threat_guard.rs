//! W2-40 (design enemy-balance §10, acceptance G6.9, finding EB12): the
//! victory-to-threat guard, tested on every path and not re-implemented.
//!
//! The rank a deploy freezes is the count of committed victories
//! (`Campaign::progression_rank`), and the one place that count moves is
//! `Campaign::first_clear` (campaign/world.rs). This file is table-driven:
//! every positive row plays one victory through its real path and asserts
//! the count moves by exactly one, that the deploy after it sits at the rank
//! `next_victory_progression` forecast before it, and that more ticks,
//! settlement passes and a save/load add nothing. Every negative row plays a
//! run or a world event that is not a victory and asserts nothing moves.

mod support;
use horde_core::balance::{BalancePreset, BalanceSettings};
use horde_core::command::Command;
use horde_core::content::{EnemyKind, Mode};
use horde_core::defenders::DefenderId;
use horde_core::enemy_catalog::enemy_catalog;
use horde_core::progression::BossRank;
use horde_core::sim::{Attempt, Game, Input, Outcome, SAVE_VERSION};
use horde_core::strategy::OutbreakOutcome;
use horde_core::world::world;
use std::panic::AssertUnwindSafe;
use support::{advance, enemy, legacy_game, strategic_game};

/// One path through which a run (or a world event) ends.
struct Row {
    name: &'static str,
    /// The mode whose forecast and next deploy are read.
    mode: Mode,
    /// Run records the outcome appends (the record-less settlement of a
    /// v6-era worker report appends none).
    records: usize,
    /// The game just before the outcome.
    stage: fn() -> Game,
    /// Plays the outcome, settlement included.
    play: fn(&mut Game),
}

/// The committed victory count, the rank it gives and the rank `mode`'s
/// next deploy freezes.
fn ledger(g: &Game, mode: Mode) -> (u64, u32, u32) {
    let c = g.campaign();
    (
        c.victories,
        c.progression_rank(),
        c.deployment_progression(mode)
            .expect("the rank is below the guard")
            .rank,
    )
}

/// Saves and loads through validation.
fn reload(g: &Game) -> Game {
    Game::from_payload(serde_json::to_value(g).unwrap(), SAVE_VERSION)
        .unwrap_or_else(|e| panic!("reload: {e:?}"))
}

/// A legacy campaign started in `country` whose first `cleared` stages are
/// already secured, at threat `rank`.
fn front(country: &str, cleared: usize, rank: u64) -> Game {
    let mut g = legacy_game();
    g.campaign_mut().attempts = 1;
    g.campaign_mut().choose_start_country(country).unwrap();
    g.campaign_mut()
        .world
        .country_levels
        .insert(country.into(), cleared);
    g.campaign_mut().victories = rank;
    g
}

/// A country whose front has a Regional checkpoint short of its Capstone.
fn regional_country() -> (&'static str, usize) {
    let country = world()
        .countries
        .iter()
        .find(|c| !c.regional_checkpoints().is_empty())
        .expect("a front with a Regional checkpoint");
    (&country.id, country.regional_checkpoints()[0])
}

/// Deploys `defender` on the active country's `stage` and ends it in
/// victory on the next tick: `extra` stages more of the encounter, then the
/// boss falls (an npc's boss is reaped once unconscious).
fn win_as(g: &mut Game, stage: usize, defender: DefenderId, extra: fn(&mut Game, &mut Attempt)) {
    g.deploy_country_stage_as(stage, defender)
        .unwrap_or_else(|e| panic!("deploy stage {stage}: {e}"));
    finish_in_victory(g, extra);
}

/// Ends the live attempt in victory on the next tick.
fn finish_in_victory(g: &mut Game, extra: fn(&mut Game, &mut Attempt)) {
    g.set_paused(false);
    let mut a = g.attempt_slot_mut().take().expect("a live attempt");
    extra(g, &mut a);
    let lane = a.lane_count / 2;
    g.spawn_enemy(&mut a, EnemyKind::Brute, lane, true);
    a.boss_spawned = true;
    let boss = a.enemies.last_mut().unwrap();
    boss.hp = 0;
    boss.injury.unconscious = true;
    *g.attempt_slot_mut() = Some(a);
    g.tick(Input::default());
    assert_eq!(g.last_result().unwrap().outcome, Outcome::Victory);
}

fn nothing(_: &mut Game, _: &mut Attempt) {}

/// An elite of the first elite-eligible kind, already beaten.
fn beaten_elite(g: &mut Game, a: &mut Attempt) {
    let lane = a.lane_count / 2;
    g.spawn_enemy(a, EnemyKind::Shambler, lane, false);
    let e = a.enemies.last_mut().unwrap();
    let base = e.resolved.clone().expect("a resolved spawn");
    e.resolved = Some(
        enemy_catalog()
            .elite_profile(&base, 7)
            .expect("the shambler is elite-eligible"),
    );
    e.hp = 0;
    e.injury.unconscious = true;
    a.telemetry.elites_spawned = 1;
}

/// The last run record's boss rank.
fn record_boss_rank(g: &Game) -> BossRank {
    g.campaign().run_history.last().unwrap().boss_rank
}

/// Ends worker battle `index`'s stage in victory on the next parent tick.
fn arm_worker_victory(g: &mut Game, index: usize) {
    let b = &mut g.battles_mut()[index];
    enemy(b.game_mut(), 5, 20.0, true);
    let a = b.game_mut().attempt_mut().unwrap();
    a.boss_spawned = true;
    a.enemies.last_mut().unwrap().injury.unconscious = true;
}

/// A front the recruit can open a country operation on.
fn operation_front(g: &Game) -> String {
    world()
        .countries
        .iter()
        .find(|c| {
            g.campaign().world.can_enter(&c.id)
                && !g.campaign().world.cleared.contains(&c.id)
                && c.level_count() > 1
        })
        .expect("an enterable front")
        .id
        .clone()
}

/// A strategic game with one recruit, Alex, ready in camp.
fn with_recruit() -> (Game, u64) {
    let mut g = strategic_game();
    let home = g.campaign().world.cleared[0].clone();
    let id = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &home))
        .unwrap();
    (g, id)
}

/// A strategic game whose second liberated country fell to an outbreak, so
/// its final stage must be secured again.
fn overrun_outbreak() -> (Game, String) {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[1].clone();
    g.with_strategy(|s, c| s.trigger_outbreak(c, &country))
        .unwrap();
    let strategy = g.strategy_mut();
    let deadline = strategy.tick + 1;
    strategy
        .outbreaks
        .iter_mut()
        .find(|o| o.country_id == country)
        .unwrap()
        .deadline_tick = deadline;
    advance(&mut g, 1);
    assert!(
        !g.campaign().world.cleared.contains(&country),
        "the overrun reopened the country"
    );
    (g, country)
}

fn positive_rows() -> Vec<Row> {
    vec![
        Row {
            name: "Personal first clear",
            mode: Mode::Personal,
            records: 1,
            stage: || front("USA", 0, 0),
            play: |g| {
                win_as(g, 0, DefenderId::Player, nothing);
                assert!(g.last_result().unwrap().first_clear);
            },
        },
        Row {
            name: "Personal replay",
            mode: Mode::Personal,
            records: 1,
            stage: || {
                let mut g = front("USA", 0, 0);
                win_as(&mut g, 0, DefenderId::Player, nothing);
                g
            },
            play: |g| {
                win_as(g, 0, DefenderId::Player, nothing);
                assert!(!g.last_result().unwrap().first_clear);
            },
        },
        Row {
            name: "NPC replay",
            mode: Mode::Npc,
            records: 1,
            stage: || {
                let mut g = front("USA", 0, 0);
                win_as(&mut g, 0, DefenderId::Mara, nothing);
                g
            },
            play: |g| {
                win_as(g, 0, DefenderId::Mara, nothing);
                assert!(!g.last_result().unwrap().first_clear);
                assert_eq!(g.last_result().unwrap().mode, Mode::Npc);
            },
        },
        Row {
            name: "reopened outbreak",
            mode: Mode::Npc,
            records: 1,
            stage: || overrun_outbreak().0,
            play: |g| {
                let country = g.strategy().outbreaks[0].country_id.clone();
                let last = world().country(&country).unwrap().level_count() - 1;
                g.campaign_mut().enter_country(&country).unwrap();
                win_as(g, last, DefenderId::Mara, nothing);
                assert!(g.campaign().world.cleared.contains(&country));
            },
        },
        Row {
            name: "worker last_result (record-less settlement)",
            mode: Mode::Personal,
            records: 0,
            stage: || {
                let (mut g, id) = with_recruit();
                let home = g.campaign().world.cleared[0].clone();
                g.deploy_survivor(id, &home, 0).unwrap();
                g
            },
            play: |g| {
                arm_worker_victory(g, 0);
                // The worker finishes on its own tick, then its record is
                // dropped: a worker report with no pending run (a v6 save).
                let worker = g.battles_mut()[0].game_mut();
                worker.tick(Input::default());
                assert_eq!(worker.last_result().unwrap().outcome, Outcome::Victory);
                worker.pending_runs_mut().clear();
                advance(g, 1);
                assert!(g.battles()[0].settled);
            },
        },
        Row {
            name: "worker pending_runs (chained operation)",
            mode: Mode::Personal,
            records: 1,
            stage: || {
                let (mut g, id) = with_recruit();
                let country = operation_front(&g);
                g.deploy_country_operation(id, &country).unwrap();
                g
            },
            play: |g| {
                arm_worker_victory(g, 0);
                advance(g, 1);
                let b = &g.battles()[0];
                assert!(!b.settled && b.stage == 1, "the operation chained on");
                assert!(b.game().attempt().is_some());
            },
        },
        Row {
            name: "Regional",
            mode: Mode::Personal,
            records: 1,
            stage: || {
                let (country, stage) = regional_country();
                front(country, stage, 0)
            },
            play: |g| {
                let (_, stage) = regional_country();
                win_as(g, stage, DefenderId::Player, nothing);
                assert_eq!(record_boss_rank(g), BossRank::Regional);
            },
        },
        Row {
            name: "Capstone",
            mode: Mode::Personal,
            records: 1,
            stage: || {
                let count = world().country("USA").unwrap().level_count();
                front("USA", count - 1, 0)
            },
            play: |g| {
                let count = world().country("USA").unwrap().level_count();
                win_as(g, count - 1, DefenderId::Player, nothing);
                assert_eq!(record_boss_rank(g), BossRank::Capstone);
                assert!(g.campaign().world.cleared.iter().any(|c| c == "USA"));
            },
        },
        Row {
            name: "Hard win",
            mode: Mode::Personal,
            records: 1,
            stage: || {
                let mut g = front("USA", 0, 0);
                g.campaign_mut()
                    .set_balance_settings(BalanceSettings::for_preset(BalancePreset::Hard))
                    .unwrap();
                g
            },
            play: |g| {
                win_as(g, 0, DefenderId::Player, nothing);
                assert_eq!(
                    g.campaign().run_history.last().unwrap().balance.preset,
                    BalancePreset::Hard
                );
            },
        },
        Row {
            name: "elite-kill run",
            mode: Mode::Personal,
            records: 1,
            // Elites need rank 3 (design enemy-balance §5).
            stage: || front("USA", 0, 3),
            play: |g| {
                win_as(g, 0, DefenderId::Player, beaten_elite);
                assert_eq!(g.campaign().run_history.last().unwrap().elite_kills, 1);
            },
        },
        Row {
            name: "takeover",
            mode: Mode::Npc,
            records: 1,
            stage: || {
                let (mut g, id) = with_recruit();
                g.apply(&Command::SetNextDefender(DefenderId::Recruit(id)))
                    .unwrap();
                g
            },
            play: |g| {
                let DefenderId::Recruit(id) = g.next_defender() else {
                    panic!("the recruit defends next");
                };
                g.apply(&Command::BeginDefense { stage: 0 }).unwrap();
                assert_eq!(g.attempt().unwrap().defender(), DefenderId::Recruit(id));
                finish_in_victory(g, nothing);
                assert_eq!(g.last_result().unwrap().defender(), DefenderId::Recruit(id));
            },
        },
    ]
}

fn negative_rows() -> Vec<Row> {
    vec![
        Row {
            name: "defeat",
            mode: Mode::Personal,
            records: 1,
            stage: || front("USA", 0, 2),
            play: |g| {
                g.deploy_country_stage(0, Mode::Personal).unwrap();
                g.set_paused(false);
                g.attempt_mut().unwrap().hp = 0;
                advance(g, 1);
                assert_eq!(g.last_result().unwrap().outcome, Outcome::Defeat);
            },
        },
        Row {
            name: "withdraw",
            mode: Mode::Personal,
            records: 1,
            stage: || front("USA", 0, 2),
            play: |g| {
                g.deploy_country_stage(0, Mode::Personal).unwrap();
                advance(g, 30);
                g.apply(&Command::Withdraw).unwrap();
                assert_eq!(g.last_result().unwrap().outcome, Outcome::Withdrawal);
            },
        },
        Row {
            name: "Retreat & train",
            mode: Mode::Npc,
            records: 1,
            stage: || front("USA", 0, 2),
            play: |g| {
                g.deploy_country_stage(0, Mode::Npc).unwrap();
                g.set_paused(false);
                g.attempt_mut().unwrap().injury.unconscious = true;
                advance(g, 1);
                let report = g.last_result().unwrap();
                assert_eq!(report.outcome, Outcome::Defeat);
                assert!(report.cause.contains("rest and train"), "{}", report.cause);
            },
        },
        Row {
            name: "abstract garrison containment",
            mode: Mode::Personal,
            records: 0,
            stage: || {
                let mut g = strategic_game();
                let country = g.campaign().world.cleared[1].clone();
                g.with_strategy(|s, c| s.trigger_outbreak(c, &country))
                    .unwrap();
                let home = g.campaign().world.cleared[0].clone();
                let guard = g
                    .with_strategy(|strategy, campaign| strategy.recruit(campaign, &home))
                    .unwrap();
                g.apply(&Command::AssignGuard { id: guard, country })
                    .unwrap();
                g.strategy_mut().outbreaks[0].strength = 1;
                g
            },
            play: |g| {
                let mut turns = 0;
                while g.strategy().outbreaks[0].outcome == OutbreakOutcome::Active {
                    advance(g, 60);
                    turns += 1;
                    assert!(turns < 400, "the guard never contained it");
                }
                assert_eq!(
                    g.strategy().outbreaks[0].outcome,
                    OutbreakOutcome::Contained
                );
                assert_eq!(g.strategy().roster[0].containments, 1);
            },
        },
    ]
}

/// Runs `check` on every row and names every row that failed, so a mutation
/// shows which paths it breaks rather than stopping at the first.
fn each_row(rows: Vec<Row>, check: fn(&Row)) {
    let failed: Vec<_> = rows
        .iter()
        .filter(|row| std::panic::catch_unwind(AssertUnwindSafe(|| check(row))).is_err())
        .map(|row| row.name)
        .collect();
    assert!(failed.is_empty(), "rows failed: {failed:?}");
}

/// The rank the next real deploy of `mode` on the active country's first
/// stage freezes (on a copy, after recalling any live main run).
fn next_deploy_rank(g: &Game, mode: Mode) -> u32 {
    let mut next = g.clone();
    next.withdraw();
    next.deploy_country_stage(0, mode)
        .unwrap_or_else(|e| panic!("next deploy: {e}"));
    next.attempt().unwrap().progression.rank
}

fn victory_moves_the_rank_once(row: &Row) {
    let name = row.name;
    let mut g = (row.stage)();
    let (victories, rank, deploy) = ledger(&g, row.mode);
    assert_eq!(deploy, rank, "{name}: the deploy sits at the rank");
    let forecast = g
        .campaign()
        .next_victory_progression(row.mode)
        .unwrap_or_else(|e| panic!("{name}: forecast: {e}"));
    assert_eq!(forecast.rank, rank + 1, "{name}: forecast");
    let history = g.campaign().run_history.len();

    (row.play)(&mut g);
    let after = (victories + 1, rank + 1, forecast.rank);
    assert_eq!(ledger(&g, row.mode), after, "{name}: +1");
    assert_eq!(
        next_deploy_rank(&g, row.mode),
        forecast.rank,
        "{name}: the next deploy meets the forecast"
    );
    assert_eq!(
        g.campaign().run_history.len(),
        history + row.records,
        "{name}: run records"
    );
    assert_eq!(
        g.campaign()
            .next_victory_progression(row.mode)
            .unwrap()
            .rank,
        rank + 2,
        "{name}: the forecast moves on"
    );

    // Exactly once: time, settlement passes and a save/load add nothing.
    advance(&mut g, 5);
    assert_eq!(ledger(&g, row.mode), after, "{name}: later ticks");
    let loaded = reload(&g);
    assert_eq!(ledger(&loaded, row.mode), after, "{name}: reload");
}

fn no_victory_leaves_the_rank(row: &Row) {
    let name = row.name;
    let mut g = (row.stage)();
    let before = ledger(&g, row.mode);
    let forecast = g
        .campaign()
        .next_victory_progression(row.mode)
        .unwrap()
        .rank;
    let history = g.campaign().run_history.len();
    (row.play)(&mut g);
    assert_eq!(ledger(&g, row.mode), before, "{name}");
    assert_eq!(
        next_deploy_rank(&g, row.mode),
        before.1,
        "{name}: next deploy"
    );
    assert_eq!(
        g.campaign().run_history.len(),
        history + row.records,
        "{name}: run records"
    );
    assert_eq!(
        g.campaign()
            .next_victory_progression(row.mode)
            .unwrap()
            .rank,
        forecast,
        "{name}: forecast"
    );
    advance(&mut g, 5);
    assert_eq!(ledger(&reload(&g), row.mode), before, "{name}: reload");
}

#[test]
fn every_committed_victory_raises_the_threat_rank_exactly_once() {
    each_row(positive_rows(), victory_moves_the_rank_once);
}

#[test]
fn runs_and_world_events_that_are_not_victories_leave_the_threat_rank() {
    each_row(negative_rows(), no_victory_leaves_the_rank);
}
