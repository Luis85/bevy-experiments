//! Property-style invariants over many seeds, without external dependencies.
//! Each property loops a fixed seed range with short runs so the whole file
//! stays fast; a failing seed is named in the assertion message.
mod support;
use horde_core::content::Mode;
use horde_core::rules::Rng;
use horde_core::sim::{BattleId, Game, Input, Outcome, SimEventRecord, smoke_input};
use std::collections::BTreeMap;
use support::{advance, enemy, game, strategic_game};

const SEEDS: std::ops::Range<u64> = 1..121;

/// A personal attempt whose boss and a few hostiles are already beaten, so the
/// next tick finishes the run and pays out.
fn doomed_run(seed: u64) -> Game {
    // Personal bosses die at zero health; NPC bosses are reaped once unconscious.
    let mode = if seed.is_multiple_of(2) {
        Mode::Personal
    } else {
        Mode::Npc
    };
    let mut g = game(mode);
    g.set_seed(seed);
    for n in 0..(seed % 4) {
        enemy(&mut g, (n % 11) as u8, 9.0 + n as f32, false);
        g.attempt_mut().unwrap().enemies.last_mut().unwrap().hp = 0;
    }
    enemy(&mut g, 5, 20.0, true);
    let a = g.attempt_mut().unwrap();
    a.boss_spawned = true;
    let boss = a.enemies.iter_mut().find(|e| e.boss).unwrap();
    boss.injury.unconscious = true;
    boss.hp = 0;
    g
}

#[test]
fn run_rewards_are_granted_exactly_once() {
    for seed in SEEDS {
        let mut g = doomed_run(seed);
        let coins = g.campaign().coins;
        let history = g.campaign().run_history.len();
        for _ in 0..10 {
            g.tick(Input::default());
            if g.attempt().is_none() {
                break;
            }
        }
        assert!(g.attempt().is_none(), "seed {seed}: run did not finish");
        let paid = g.campaign().coins;
        assert!(paid > coins, "seed {seed}: boss kill pays");
        assert_eq!(g.campaign().run_history.len(), history + 1, "seed {seed}");
        let items = g.campaign().items.clone();
        let attempts = g.campaign().attempts;
        // Neither time, nor recalling an already-finished run, pays again.
        advance(&mut g, 30);
        g.withdraw();
        advance(&mut g, 30);
        assert_eq!(g.campaign().coins, paid, "seed {seed}");
        assert_eq!(g.campaign().items, items, "seed {seed}");
        assert_eq!(g.campaign().attempts, attempts, "seed {seed}");
        assert_eq!(g.campaign().run_history.len(), history + 1, "seed {seed}");
    }
}

#[test]
fn coins_never_go_negative_overflow_or_drop_without_a_purchase() {
    let mut rng = Rng::new(0xC0FFEE);
    for seed in SEEDS {
        let mut g = game(Mode::Personal);
        g.set_seed(seed);
        let mut last = g.campaign().coins;
        let mut last_attempt = 0;
        for _ in 0..(120 + seed % 120) {
            let input = smoke_input(&g);
            g.tick(input);
            if let Some(a) = g.attempt() {
                assert!(a.coins >= last_attempt, "seed {seed}: attempt coins fell");
                last_attempt = a.coins;
            }
            assert!(
                g.campaign().coins >= last,
                "seed {seed}: coins fell by play"
            );
            last = g.campaign().coins;
        }
        // Saturating rewards: a near-max balance cannot wrap.
        g.campaign_mut().coins = u64::MAX - rng.below(50);
        g.campaign_mut().reward(1_000_000, 0, Mode::Personal);
        assert_eq!(g.campaign().coins, u64::MAX, "seed {seed}");
        // A purchase either fails and changes nothing, or costs exactly its price.
        let mut buyer = Game::default();
        buyer.campaign_mut().coins = rng.below(400);
        let before = buyer.campaign().coins;
        match buyer.campaign_mut().buy_item(4) {
            Ok(_) => {
                let price = horde_core::content::content().items[4].buy_value;
                assert_eq!(buyer.campaign().coins, before - price, "seed {seed}");
            }
            Err(_) => assert_eq!(buyer.campaign().coins, before, "seed {seed}"),
        }
    }
}

#[test]
fn save_load_save_is_byte_equal() {
    let dir = std::env::temp_dir().join(format!("lastlight-properties-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for seed in SEEDS {
        let mode = if seed.is_multiple_of(2) {
            Mode::Personal
        } else {
            Mode::Npc
        };
        let mut g = game(mode);
        g.set_seed(seed);
        for _ in 0..(seed % 150) {
            let input = smoke_input(&g);
            g.tick(input);
        }
        // Every seed: the serialized payload survives a typed round trip.
        let payload = serde_json::to_string(&g).unwrap();
        let reloaded: Game = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            payload,
            serde_json::to_string(&reloaded).unwrap(),
            "seed {seed}"
        );
        // Every sixth seed: the real on-disk path (fsync makes it the slow one).
        if !seed.is_multiple_of(6) {
            continue;
        }
        let first = dir.join(format!("a-{seed}.json"));
        let second = dir.join(format!("b-{seed}.json"));
        g.save(&first).unwrap();
        Game::load(&first).unwrap().save(&second).unwrap();
        assert_eq!(
            std::fs::read(&first).unwrap(),
            std::fs::read(&second).unwrap(),
            "seed {seed}: save -> load -> save changed bytes"
        );
        assert_eq!(
            g.state_digest(),
            Game::load(&first).unwrap().state_digest(),
            "seed {seed}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn settling_twice_changes_nothing() {
    for seed in SEEDS {
        let mut g = strategic_game();
        g.set_seed(seed);
        let country = g.campaign().world.cleared[0].clone();
        for _ in 0..=(seed % 2) {
            let recruit = g
                .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
                .unwrap();
            g.deploy_survivor(recruit, &country, 0).unwrap();
        }
        for b in g.battles_mut() {
            enemy(b.game_mut(), 5, 20.0, true);
            let a = b.game_mut().attempt_mut().unwrap();
            a.boss_spawned = true;
            a.enemies
                .iter_mut()
                .find(|e| e.boss)
                .expect("the boss was just spawned")
                .injury
                .unconscious = true;
        }
        advance(&mut g, 1);
        assert!(g.battles().iter().all(|b| b.settled), "seed {seed}");
        let settled = (
            g.campaign().coins,
            g.campaign().items.clone(),
            g.campaign().run_history.len(),
            g.campaign().reserved_items.clone(),
        );
        advance(&mut g, 5);
        let again = (
            g.campaign().coins,
            g.campaign().items.clone(),
            g.campaign().run_history.len(),
            g.campaign().reserved_items.clone(),
        );
        assert_eq!(settled, again, "seed {seed}: second settlement pass paid");
        assert!(g.take_settlement_issues().is_empty(), "seed {seed}");
    }
}

/// The victories the run history records (main runs and settled worker
/// stages alike).
fn committed_victories(g: &Game) -> u64 {
    g.campaign()
        .run_history
        .iter()
        .filter(|r| r.outcome == Outcome::Victory)
        .count() as u64
}

/// Deploys the main run in `mode` on the start country's first stage and
/// checks it freezes the rank of the committed victories so far.
fn deploy_at_committed_rank(g: &mut Game, mode: Mode, seed: u64, step: u32) {
    g.deploy_country_stage(0, mode)
        .unwrap_or_else(|e| panic!("seed {seed} step {step}: {e}"));
    g.set_paused(false);
    assert_eq!(
        u64::from(g.attempt().unwrap().progression.rank),
        committed_victories(g),
        "seed {seed} step {step}: deploy rank"
    );
}

/// W2-40 (design enemy-balance §10, acceptance G6.9): over random sequences
/// of victories, defeats, withdrawals and npc retreats by the player and
/// Mara, and of worker expeditions that win or are recalled, the threat rank
/// equals the committed victories after every step.
#[test]
fn threat_rank_equals_committed_victories() {
    let mut seen = [0u32; 6];
    for seed in SEEDS {
        let mut rng = Rng::new(seed);
        let mut g = strategic_game();
        g.set_seed(seed);
        let home = g.campaign().world.cleared[0].clone();
        let recruit = g
            .with_strategy(|strategy, campaign| strategy.recruit(campaign, &home))
            .unwrap();
        for step in 0..8 {
            let action = rng.below(6) as usize;
            match action {
                0 | 1 => {
                    let mode = if action == 0 {
                        Mode::Personal
                    } else {
                        Mode::Npc
                    };
                    deploy_at_committed_rank(&mut g, mode, seed, step);
                    enemy(&mut g, 5, 20.0, true);
                    let a = g.attempt_mut().unwrap();
                    a.boss_spawned = true;
                    let boss = a.enemies.last_mut().unwrap();
                    boss.hp = 0;
                    boss.injury.unconscious = true;
                    advance(&mut g, 1);
                }
                2 => {
                    deploy_at_committed_rank(&mut g, Mode::Personal, seed, step);
                    g.attempt_mut().unwrap().hp = 0;
                    advance(&mut g, 1);
                }
                3 => {
                    let mode = if seed.is_multiple_of(2) {
                        Mode::Personal
                    } else {
                        Mode::Npc
                    };
                    deploy_at_committed_rank(&mut g, mode, seed, step);
                    advance(&mut g, 1 + rng.below(30) as u32);
                    g.withdraw();
                }
                4 => {
                    deploy_at_committed_rank(&mut g, Mode::Npc, seed, step);
                    g.attempt_mut().unwrap().injury.unconscious = true;
                    advance(&mut g, 1);
                }
                _ => {
                    let battle = g
                        .deploy_survivor(recruit, &home, 0)
                        .unwrap_or_else(|e| panic!("seed {seed} step {step}: {e}"));
                    let b = g.battles_mut().iter_mut().find(|b| b.id == battle).unwrap();
                    if rng.below(2) == 0 {
                        enemy(b.game_mut(), 5, 20.0, true);
                        let a = b.game_mut().attempt_mut().unwrap();
                        a.boss_spawned = true;
                        a.enemies.last_mut().unwrap().injury.unconscious = true;
                        advance(&mut g, 1);
                    } else {
                        g.recall_survivor(battle).unwrap();
                    }
                    assert!(
                        g.battles().iter().all(|b| b.settled),
                        "seed {seed} step {step}: settled"
                    );
                }
            }
            seen[action] += 1;
            assert!(g.attempt().is_none(), "seed {seed} step {step}: finished");
            assert_eq!(
                u64::from(g.campaign().progression_rank()),
                committed_victories(&g),
                "seed {seed} step {step} (action {action}): rank"
            );
            assert_eq!(
                g.campaign().victories,
                committed_victories(&g),
                "seed {seed} step {step} (action {action}): victories"
            );
        }
    }
    assert!(seen.iter().all(|n| *n > 0), "every action ran: {seen:?}");
}

#[test]
fn event_sequence_numbers_strictly_increase_per_battle() {
    for seed in SEEDS {
        let mut g = strategic_game();
        g.set_seed(seed);
        let country = g.campaign().world.cleared[0].clone();
        for _ in 0..=(seed % 2) {
            let recruit = g
                .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
                .unwrap();
            g.deploy_survivor(recruit, &country, 0).unwrap();
        }
        g.set_paused(false);
        let mut last: BTreeMap<BattleId, u64> = BTreeMap::new();
        let mut seen = 0;
        for tick in 0..200 {
            if tick == 3 {
                for b in g.battles_mut() {
                    enemy(b.game_mut(), 5, 20.0, true);
                    let a = b.game_mut().attempt_mut().unwrap();
                    a.boss_spawned = true;
                    a.enemies
                        .iter_mut()
                        .find(|e| e.boss)
                        .expect("the boss was just spawned")
                        .injury
                        .unconscious = true;
                }
            }
            g.tick(Input::default());
            let records: Vec<SimEventRecord> = g.drain_events().collect();
            for record in records {
                if let Some(previous) = last.insert(record.battle, record.seq) {
                    assert!(
                        record.seq > previous,
                        "seed {seed} tick {tick}: {:?} seq {} after {previous}",
                        record.battle,
                        record.seq
                    );
                }
                seen += 1;
            }
        }
        assert!(seen > 0, "seed {seed}: no events emitted");
        assert!(!last.is_empty(), "seed {seed}");
        // A root (non-expedition) battle numbers its own stream the same way.
        let mut root = game(Mode::Npc);
        root.set_seed(seed);
        enemy(&mut root, 5, 8.0, false);
        let mut previous = None;
        for _ in 0..200 {
            let input = smoke_input(&root);
            root.tick(input);
            for record in root.drain_events() {
                assert_eq!(record.battle, BattleId::Root);
                assert!(previous.is_none_or(|p| record.seq > p), "seed {seed}");
                previous = Some(record.seq);
            }
        }
        assert!(
            previous.is_some(),
            "seed {seed}: root battle emitted events"
        );
    }
}
