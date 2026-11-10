//! Property-style invariants over many seeds, without external dependencies.
//! Each property loops a fixed seed range with short runs so the whole file
//! stays fast; a failing seed is named in the assertion message.
mod support;
use horde_core::campaign::AutoSellRule;
use horde_core::command::{Command, CommandError, CommandOutcome};
use horde_core::content::{Mode, content};
use horde_core::defenders::DefenderId;
use horde_core::quality::Quality;
use horde_core::rules::Rng;
use horde_core::sim::{AutoSellScope, BattleId, Game, Input, Outcome, SimEventRecord, smoke_input};
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
/// equals the committed victories after every step. Each win is checked to
/// commit exactly one victory and every other step none, so a setup that
/// stopped winning cannot pass at 0 == 0.
#[test]
fn threat_rank_equals_committed_victories() {
    // Actions 0..=5, and 6 for a worker expedition that won.
    let mut seen = [0u32; 7];
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
            let before = committed_victories(&g);
            let mut won = false;
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
                    assert_eq!(
                        g.last_result().map(|r| r.outcome),
                        Some(Outcome::Victory),
                        "seed {seed} step {step} (action {action}): won"
                    );
                    won = true;
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
                        won = true;
                    } else {
                        g.recall_survivor(battle).unwrap();
                    }
                    assert!(
                        g.battles().iter().all(|b| b.settled),
                        "seed {seed} step {step}: settled"
                    );
                    let summary = g.battles().iter().find(|b| b.id == battle).unwrap();
                    let outcome = summary.summary.as_ref().and_then(|s| s.outcome);
                    assert_eq!(
                        outcome == Some(Outcome::Victory),
                        won,
                        "seed {seed} step {step}: worker outcome {outcome:?}"
                    );
                }
            }
            assert_eq!(
                committed_victories(&g),
                before + u64::from(won),
                "seed {seed} step {step} (action {action}): one victory per win"
            );
            seen[if won && action == 5 { 6 } else { action }] += 1;
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

// ---------------------------------------------------------------------------
// Sale guards (W2-17; acceptance G4.1 and G4.5, ruling R11).

/// The generated stash cases: every fifth one runs the same sale path.
const SALE_CASES: u64 = 500;

fn new_id(outcome: CommandOutcome) -> u64 {
    match outcome {
        CommandOutcome::Id(id) => id,
        other => panic!("expected an id, got {other:?}"),
    }
}

/// Every guard [`guard`] names.
const GUARDS: [&str; 9] = [
    "reserved",
    "protected",
    "locked",
    "favourite",
    "npc-issue",
    "player",
    "mara",
    "preset",
    "recruit",
];

/// Why owned item `id` must survive every sale, if it must, read straight
/// from the saved state rather than through the sale rule: reserved by a
/// deployment or a country guard; protected, locked, favourite or
/// NPC-issue; or in the player's, Mara's, a preset's or any roster
/// recruit's loadout.
fn guard(g: &Game, id: u64) -> Option<&'static str> {
    let c = g.campaign();
    let item = c.item(id).expect("an owned item");
    let roster = || {
        g.strategy()
            .roster
            .iter()
            .any(|r| r.npc.loadout.contains(&id))
    };
    [
        c.item_reserved(id),
        item.protected,
        item.locked,
        item.favorite,
        item.definition().npc_only,
        c.player.loadout.contains(&id),
        c.npc.loadout.contains(&id),
        c.presets.iter().any(|p| p.loadout.contains(&id)),
        roster(),
    ]
    .into_iter()
    .zip(GUARDS)
    .find_map(|(held, why)| held.then_some(why))
}

/// A random rule: any valid threshold, slots, `keep_upgrades` and
/// `apply_to_npc_runs`, enabled.
fn random_rule(rng: &mut Rng) -> AutoSellRule {
    AutoSellRule {
        enabled: true,
        sell_below: Quality::ALL[1 + rng.below(4) as usize],
        slots: std::array::from_fn(|_| rng.below(4) != 0),
        keep_upgrades: rng.below(2) == 0,
        apply_to_npc_runs: true,
    }
}

/// A random ascending subset of `ids`.
fn subset(rng: &mut Rng, ids: &[u64]) -> Vec<u64> {
    ids.iter().copied().filter(|_| rng.below(3) != 0).collect()
}

/// A strategic game whose stash holds 4 to 12 random finds and shop items
/// (any catalogue gear, NPC-issue copies included) besides the starters,
/// each given a random guard: none, locked, favourite, equipped by the
/// player or Mara, saved in a preset, or carried by an idle recruit or by a
/// recruit that may be guarding a country. Returns the game and its idle
/// recruit.
fn guarded_stash(rng: &mut Rng) -> (Game, u64) {
    let mut g = strategic_game();
    let countries = g.campaign().world.cleared.clone();
    let idle = new_id(g.apply(&Command::Recruit(countries[1].clone())).unwrap());
    let guard = new_id(g.apply(&Command::Recruit(countries[2].clone())).unwrap());
    let definitions = content().items.len() as u64;
    for n in 0..4 + rng.below(9) {
        let definition = if rng.below(12) == 0 {
            0
        } else {
            (4 + rng.below(definitions - 4)) as usize
        };
        let level = 1 + rng.below(3) as u32;
        let quality = match rng.below(6) {
            0 => None,
            q => Some(Quality::ALL[q as usize - 1]),
        };
        let id = g
            .campaign_mut()
            .add_item_with_quality(definition, level, quality)
            .unwrap();
        let slot = g.campaign().item(id).unwrap().definition().slot.index();
        // A guard the rules refuse (an untrained weapon class, say) leaves
        // the item unguarded, which the property covers just the same.
        let _ = match rng.below(10) {
            3 => g.apply(&Command::Lock { id, locked: true }),
            4 => g.apply(&Command::Favorite { id, favorite: true }),
            5 => g.apply(&Command::Equip {
                mode: Mode::Personal,
                id,
            }),
            6 => g.apply(&Command::Equip {
                mode: Mode::Npc,
                id,
            }),
            7 => {
                let worn = g.campaign().player.loadout[slot];
                let _ = g.apply(&Command::Equip {
                    mode: Mode::Personal,
                    id,
                });
                let _ = g.apply(&Command::SavePreset {
                    name: format!("Preset {n}"),
                    mode: Mode::Personal,
                });
                g.apply(&Command::Equip {
                    mode: Mode::Personal,
                    id: worn,
                })
            }
            8 => g.apply(&Command::EquipRecruit { id: idle, item: id }),
            9 => g.apply(&Command::EquipRecruit {
                id: guard,
                item: id,
            }),
            _ => Ok(CommandOutcome::Applied),
        };
    }
    if rng.below(2) == 0 {
        g.apply(&Command::AssignGuard {
            id: guard,
            country: countries[2].clone(),
        })
        .unwrap();
    }
    (g, idle)
}

/// Every sale path (strict SellItem, SellUnprotected, the stash apply, the
/// main run's settlement hook H1 and an expedition's hook W-a with its final
/// sweep) over [`SALE_CASES`] generated stashes: no guarded item is ever
/// sold, and the coins realised equal the quote (the per-item eligibility,
/// the sell-extras quote, the stash preview or the receipt).
#[test]
fn sale_paths_never_sell_guarded_items() {
    // How often each path met each guard, and how many items it sold: the
    // property is only as good as its coverage.
    let mut met: BTreeMap<(u64, &str), u32> = BTreeMap::new();
    let mut sold_by_path = [0u32; 5];
    for case in 0..SALE_CASES {
        let mut rng = Rng::new(0x5A1E_0000 + case);
        let (mut g, idle) = guarded_stash(&mut rng);
        let mut owned: Vec<u64> = g.campaign().items.iter().map(|item| item.id).collect();
        owned.sort_unstable();
        let guards: BTreeMap<u64, &str> = owned
            .iter()
            .filter_map(|id| guard(&g, *id).map(|why| (*id, why)))
            .collect();
        // The items the path may sell: the whole stash, or the run's drops
        // or the expedition's queue.
        let mut exposed = owned.clone();
        let value: BTreeMap<u64, u64> = owned
            .iter()
            .map(|id| (*id, g.campaign().item_sale_value(*id).unwrap()))
            .collect();
        let wallet = g.campaign().coins;
        let path = case % 5;
        let paid = match path {
            // Strict SellItem in a random order: it sells exactly what the
            // eligibility read accepts, for the quoted coins.
            0 => {
                let mut order = owned.clone();
                for i in (1..order.len()).rev() {
                    order.swap(i, rng.below(i as u64 + 1) as usize);
                }
                let mut paid = 0;
                for id in order {
                    let quote = g.sale_eligibility(id);
                    let before = g.campaign().coins;
                    let result = g.apply(&Command::SellItem(id));
                    match quote {
                        Ok(quote) => {
                            assert_eq!(
                                result,
                                Ok(CommandOutcome::Amount(quote.coins)),
                                "case {case}"
                            );
                            assert_eq!(g.campaign().coins, before + quote.coins, "case {case}");
                            assert_eq!(quote.coins, value[&id], "case {case}");
                            paid += quote.coins;
                        }
                        Err(block) => {
                            assert_eq!(
                                result,
                                Err(CommandError::SaleBlocked(block)),
                                "case {case}"
                            );
                            assert!(g.campaign().item(id).is_some(), "case {case}");
                        }
                    }
                }
                paid
            }
            // SellUnprotected pays exactly the sell-extras quote.
            1 => {
                let quote = g.sell_extras_quote();
                match g.apply(&Command::SellUnprotected) {
                    Ok(outcome) => {
                        assert_eq!(outcome, CommandOutcome::Amount(quote), "case {case}")
                    }
                    Err(e) => assert_eq!(quote, 0, "case {case}: {e}"),
                }
                quote
            }
            // The stash apply sells exactly the confirmed preview.
            2 => {
                let rule = random_rule(&mut rng);
                g.apply(&Command::SetAutoSell(rule)).unwrap();
                let preview = g.auto_sell_preview(&rule, AutoSellScope::Stash);
                let result = g.apply(&Command::ApplyAutoSellToStash {
                    expect_count: preview.count,
                    expect_coins: preview.coins,
                });
                if preview.count == 0 {
                    assert_eq!(result, Err(CommandError::NothingToSell), "case {case}");
                } else {
                    assert_eq!(
                        result,
                        Ok(CommandOutcome::Sold {
                            count: preview.count,
                            coins: preview.coins
                        }),
                        "case {case}"
                    );
                }
                let sold: Vec<u64> = owned
                    .iter()
                    .copied()
                    .filter(|id| g.campaign().item(*id).is_none())
                    .collect();
                assert_eq!(sold, preview.ids, "case {case}");
                preview.coins
            }
            // Hook H1: a main run (the player, Mara or the idle recruit)
            // lists a random part of the stash as its drops and settles.
            3 => {
                let defender = [
                    DefenderId::Player,
                    DefenderId::Mara,
                    DefenderId::Recruit(idle),
                ][rng.below(3) as usize];
                if g.deploy_country_stage_as(0, defender).is_err() {
                    g.deploy_country_stage_as(0, DefenderId::Player).unwrap();
                }
                let rule = random_rule(&mut rng);
                g.apply(&Command::SetAutoSell(rule)).unwrap();
                let drops = subset(&mut rng, &owned);
                exposed.clone_from(&drops);
                for &id in &drops {
                    let quality = g.campaign().item(id).unwrap().quality;
                    let a = g.attempt_mut().unwrap();
                    a.drop_ids.push(id);
                    a.loot += 1;
                    if let Some(quality) = quality {
                        a.loot_quality_counts[quality] += 1;
                    }
                }
                g.apply(&Command::Withdraw).unwrap();
                let report = g.last_result().unwrap();
                assert_eq!(report.drop_ids, drops, "case {case}");
                let receipt = &report.auto_sold;
                let record = g.campaign().run_history.last().unwrap();
                assert_eq!(
                    (record.auto_sold_count, record.auto_sold_coins),
                    (receipt.count, receipt.coins),
                    "case {case}"
                );
                receipt.coins
            }
            // Hook W-a: an expedition's queue (any unreserved part of the
            // stash) drains at its recall, through the final sweep.
            _ => {
                let countries = g.campaign().world.cleared.clone();
                let runner = new_id(g.apply(&Command::Recruit(countries[3].clone())).unwrap());
                let b = g.deploy_survivor(runner, &countries[0], 0).unwrap();
                let rule = random_rule(&mut rng);
                g.apply(&Command::SetAutoSell(rule)).unwrap();
                let free: Vec<u64> = owned
                    .iter()
                    .copied()
                    .filter(|id| !g.campaign().item_reserved(*id))
                    .collect();
                let queue = subset(&mut rng, &free);
                exposed.clone_from(&queue);
                let qualities: Vec<Option<Quality>> = queue
                    .iter()
                    .map(|id| g.campaign().item(*id).unwrap().quality)
                    .collect();
                let wallet = g.campaign().coins;
                let battle = g.battles_mut().iter_mut().find(|x| x.id == b).unwrap();
                // The queue stands for drops this expedition's attempt
                // banked, so its loot counts them.
                let a = battle.game_mut().attempt_mut().unwrap();
                a.loot += queue.len() as u32;
                for quality in qualities.into_iter().flatten() {
                    a.loot_quality_counts[quality] += 1;
                }
                battle.pending_auto_sell = queue;
                let earned = battle.game().campaign().coins;
                g.recall_survivor(b).unwrap();
                assert!(g.take_settlement_issues().is_empty(), "case {case}");
                let battle = g.battles().iter().find(|x| x.id == b).unwrap();
                assert!(
                    battle.settled && battle.pending_auto_sell.is_empty(),
                    "case {case}"
                );
                let earned = battle.game().campaign().coins - earned;
                let receipt = &battle.game().last_result().unwrap().auto_sold;
                assert_eq!(
                    g.campaign().coins,
                    wallet + earned + receipt.coins,
                    "case {case}"
                );
                receipt.coins
            }
        };
        // Nothing guarded was sold, and the coins sold items realised equal
        // what the path quoted and paid.
        let sold: Vec<u64> = owned
            .iter()
            .copied()
            .filter(|id| g.campaign().item(*id).is_none())
            .collect();
        for id in &sold {
            assert!(
                exposed.contains(id),
                "case {case} path {path}: sold item {id} it was not given"
            );
            if let Some(why) = guards.get(id) {
                panic!("case {case} path {path}: sold item {id}, guarded as {why}");
            }
        }
        let realised: u64 = sold.iter().map(|id| value[id]).sum();
        assert_eq!(
            realised, paid,
            "case {case} path {path}: quote and coins differ"
        );
        if path != 4 {
            assert_eq!(g.campaign().coins, wallet + paid, "case {case} path {path}");
        }
        g.to_save_json()
            .unwrap_or_else(|e| panic!("case {case} path {path}: the save does not validate: {e}"));
        for id in &exposed {
            if let Some(why) = guards.get(id) {
                *met.entry((path, why)).or_default() += 1;
            }
        }
        sold_by_path[path as usize] += sold.len() as u32;
    }
    // Every path sold something and met every guard it can be offered (an
    // expedition's queue never holds a reserved item, schema v7 §3.6).
    for path in 0..5 {
        assert!(sold_by_path[path as usize] > 0, "path {path} sold nothing");
        for why in GUARDS {
            if path == 4 && why == "reserved" {
                continue;
            }
            assert!(
                met.get(&(path, why)).is_some_and(|n| *n > 0),
                "path {path} never met a {why} item"
            );
        }
    }
}
