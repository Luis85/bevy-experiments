//! Expedition and operation auto-sell, hook W-a plus the final sweep (W2-16,
//! loot-economy §7; rulings D13, E5, E11; plan decision 6; acceptance G3.5,
//! G4.4, G4.7 and G4.8). Every expedition drop the parent stores is queued
//! in `ParallelBattle.pending_auto_sell`; the queue is drained once, at the
//! stage's run record, and sold when the rule is enabled with
//! `apply_to_npc_runs`, judged against the expedition's own loadout and NPC.
//! The parent's run record and the worker's report carry the receipt. A
//! queue the rule does not cover is dropped, so a later rule change never
//! acts retroactively, and a drop banked by an attempt deployed before v7
//! (no drop tier) is never queued.
mod support;
use horde_core::{
    campaign::AutoSellRule,
    command::Command,
    content::Mode,
    defenders::DefenderId,
    quality::Quality,
    sim::{Game, ParallelBattle, SAVE_VERSION},
    stats::RunRecord,
    world::world,
};
use support::{advance, enemy, strategic_game};

const FIXTURE_V6: &str = include_str!("fixtures/save_v6.json");

/// An enabled rule that sells Poor and Common drops in every slot and keeps
/// no upgrades, expeditions included.
fn sell_rule() -> AutoSellRule {
    AutoSellRule {
        enabled: true,
        sell_below: Quality::Uncommon,
        keep_upgrades: false,
        apply_to_npc_runs: true,
        ..AutoSellRule::default()
    }
}

fn set_rule(g: &mut Game, rule: AutoSellRule) {
    g.apply(&Command::SetAutoSell(rule)).unwrap();
}

/// A strategic game with one Ready recruit, the rule set, and that recruit
/// on an expedition to the starting country's first stage.
fn expedition(rule: AutoSellRule) -> (Game, u64) {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let recruit = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    set_rule(&mut g, rule);
    let b = g.deploy_survivor(recruit, &country, 0).unwrap();
    (g, b)
}

fn battle(g: &Game, id: u64) -> &ParallelBattle {
    g.battles().iter().find(|b| b.id == id).unwrap()
}

fn worker(g: &mut Game, id: u64) -> &mut Game {
    g.battles_mut()
        .iter_mut()
        .find(|b| b.id == id)
        .unwrap()
        .game_mut()
}

/// A drop banked by the expedition's live attempt: a worker-local item of
/// `definition` at `quality`, counted in the attempt's loot.
fn worker_drop(g: &mut Game, b: u64, definition: usize, quality: Quality) {
    let w = worker(g, b);
    w.campaign_mut()
        .add_item_with_quality(definition, 1, Some(quality))
        .unwrap();
    let a = w.attempt_mut().expect("a live expedition attempt");
    a.loot += 1;
    a.loot_quality_counts[quality] += 1;
}

/// Ends the battle's current stage in victory on the next tick.
fn win_stage(g: &mut Game, b: u64) {
    let w = worker(g, b);
    enemy(w, 5, 20.0, true);
    let a = w.attempt_mut().unwrap();
    a.boss_spawned = true;
    a.enemies[0].injury.unconscious = true;
    advance(g, 1);
}

fn pending(g: &Game, b: u64) -> Vec<u64> {
    battle(g, b).pending_auto_sell.clone()
}

fn owned(g: &Game, id: u64) -> bool {
    g.campaign().item(id).is_some()
}

fn item_ids(g: &Game) -> Vec<u64> {
    g.campaign().items.iter().map(|item| item.id).collect()
}

fn value(g: &Game, id: u64) -> u64 {
    g.campaign().item_sale_value(id).unwrap()
}

fn worker_coins(g: &Game, b: u64) -> u64 {
    battle(g, b).game().campaign().coins
}

/// The parent's run records of expedition `b`, oldest first.
fn records(g: &Game, b: u64) -> Vec<RunRecord> {
    g.campaign()
        .run_history
        .iter()
        .filter(|r| r.worker_id == Some(b))
        .cloned()
        .collect()
}

/// Saves through validation and loads.
fn reload(g: &Game) -> Game {
    let payload = g
        .to_save_json()
        .unwrap_or_else(|e| panic!("the save does not validate: {e}"));
    Game::from_payload(serde_json::from_str(&payload).unwrap(), SAVE_VERSION)
        .unwrap_or_else(|e| panic!("the save does not load: {e:?}"))
}

/// What later passes must never change once an expedition has settled.
fn ledgers(g: &Game) -> serde_json::Value {
    serde_json::json!({
        "coins": g.campaign().coins,
        "items": item_ids(g),
        "runs": serde_json::to_value(&*g.campaign().run_history).unwrap(),
        "battles": serde_json::to_value(g.battles()).unwrap(),
    })
}

#[test]
fn worker_loot_sold_exactly_once_in_main_campaign() {
    let (mut g, b) = expedition(sell_rule());
    // The main run is deployed too: a settlement sale skips only the
    // in-battle guard, so the expedition's loot still sells.
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    g.set_paused(false);
    worker_drop(&mut g, b, 5, Quality::Poor);
    worker_drop(&mut g, b, 6, Quality::Common);
    worker_drop(&mut g, b, 7, Quality::Uncommon);
    advance(&mut g, 1);
    let queued = pending(&g, b);
    assert_eq!(queued.len(), 3, "every stored drop is queued");
    assert!(
        queued.iter().all(|id| owned(&g, *id)),
        "nothing sells mid-battle"
    );
    let qualities: Vec<_> = queued
        .iter()
        .map(|id| g.campaign().item(*id).unwrap().quality)
        .collect();
    assert_eq!(
        qualities,
        [
            Some(Quality::Poor),
            Some(Quality::Common),
            Some(Quality::Uncommon)
        ]
    );
    let (cheap, kept) = (&queued[..2], queued[2]);
    let values: Vec<u64> = cheap.iter().map(|id| value(&g, *id)).collect();
    let paid: u64 = values.iter().sum();
    let (wallet, earned) = (g.campaign().coins, worker_coins(&g, b));

    g.recall_survivor(b).unwrap();
    assert!(g.take_settlement_issues().is_empty());
    assert!(battle(&g, b).settled);
    assert!(pending(&g, b).is_empty(), "the queue is drained");
    assert!(
        cheap.iter().all(|id| !owned(&g, *id)),
        "Poor and Common sold"
    );
    assert!(owned(&g, kept), "the Uncommon drop is kept");
    let earned = worker_coins(&g, b) - earned;
    assert_eq!(g.campaign().coins, wallet + earned + paid);
    // One record, one receipt, mirrored on the worker's report.
    let runs = records(&g, b);
    assert_eq!(runs.len(), 1);
    assert_eq!(
        (runs[0].auto_sold_count, runs[0].auto_sold_coins),
        (2, paid)
    );
    let report = battle(&g, b).game().last_result().unwrap();
    assert_eq!(report.attempt_id, runs[0].attempt_id);
    let receipt = &report.auto_sold;
    assert_eq!((receipt.count, receipt.coins), (2, paid));
    assert_eq!(receipt.by_quality[Quality::Poor], 1);
    assert_eq!(receipt.by_quality[Quality::Common], 1);
    let line_coins: Vec<u64> = receipt.lines.iter().map(|l| l.coins).collect();
    let mut expected = values.clone();
    expected.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(line_coins, expected, "highest coins first");

    // Exactly once: the main run ends, then more passes and a reload.
    g.apply(&Command::Withdraw).unwrap();
    let settled = ledgers(&g);
    advance(&mut g, 5);
    let mut g = reload(&g);
    advance(&mut g, 5);
    assert_eq!(ledgers(&g), settled);
}

#[test]
fn repeated_settle_parallel_is_idempotent() {
    let (mut g, b) = expedition(sell_rule());
    worker_drop(&mut g, b, 5, Quality::Poor);
    worker_drop(&mut g, b, 6, Quality::Common);
    advance(&mut g, 1);
    let queued = pending(&g, b);
    assert_eq!(queued.len(), 2);
    // Passes that store nothing new queue nothing twice.
    let items = item_ids(&g);
    advance(&mut g, 5);
    assert_eq!(pending(&g, b), queued);
    assert_eq!(item_ids(&g), items);

    g.recall_survivor(b).unwrap();
    assert!(queued.iter().all(|id| !owned(&g, *id)), "the queue sells");
    assert_eq!(records(&g, b)[0].auto_sold_count, 2);
    let settled = ledgers(&g);
    // Later passes (ticks, a recall of the settled battle, a reload) skip it.
    advance(&mut g, 5);
    assert!(g.recall_survivor(b).is_err());
    let mut g = reload(&g);
    advance(&mut g, 5);
    assert!(g.take_settlement_issues().is_empty());
    assert_eq!(ledgers(&g), settled);
}

#[test]
fn save_mid_expedition_preserves_pending_auto_sell_and_sells_once() {
    let (mut g, b) = expedition(sell_rule());
    worker_drop(&mut g, b, 5, Quality::Poor);
    worker_drop(&mut g, b, 9, Quality::Common);
    advance(&mut g, 1);
    let queued = pending(&g, b);
    assert_eq!(queued.len(), 2);
    let payload: serde_json::Value = serde_json::from_str(&g.to_save_json().unwrap()).unwrap();
    let saved = payload["battles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == b)
        .unwrap();
    assert_eq!(saved["pending_auto_sell"], serde_json::json!(queued));

    let mut loaded = reload(&g);
    assert_eq!(pending(&loaded, b), queued, "the queue survives the reload");
    advance(&mut loaded, 3);
    assert_eq!(pending(&loaded, b), queued);
    let paid: u64 = queued.iter().map(|id| value(&loaded, *id)).sum();
    loaded.recall_survivor(b).unwrap();
    assert!(queued.iter().all(|id| !owned(&loaded, *id)));
    let runs = records(&loaded, b);
    assert_eq!(runs.len(), 1);
    assert_eq!(
        (runs[0].auto_sold_count, runs[0].auto_sold_coins),
        (2, paid)
    );
    // The reload changed nothing: the same game settled without one ends
    // with the same stash and wallet.
    advance(&mut g, 3);
    g.recall_survivor(b).unwrap();
    assert_eq!(item_ids(&g), item_ids(&loaded));
    assert_eq!(g.campaign().coins, loaded.campaign().coins);
    // And nothing sells twice after another reload.
    let settled = ledgers(&loaded);
    let mut loaded = reload(&loaded);
    advance(&mut loaded, 3);
    assert_eq!(ledgers(&loaded), settled);
}

#[test]
fn apply_to_npc_runs_false_leaves_expedition_loot() {
    let main_only = AutoSellRule {
        apply_to_npc_runs: false,
        ..sell_rule()
    };
    let (mut on, b) = expedition(sell_rule());
    let (mut off, b_off) = expedition(main_only);
    assert_eq!(b, b_off);
    for g in [&mut on, &mut off] {
        worker_drop(g, b, 5, Quality::Poor);
        worker_drop(g, b, 6, Quality::Common);
        advance(g, 1);
    }
    let queued = pending(&off, b);
    assert_eq!(queued.len(), 2);
    assert_eq!(pending(&on, b), queued);
    on.recall_survivor(b).unwrap();
    off.recall_survivor(b).unwrap();
    assert!(
        queued.iter().all(|id| !owned(&on, *id)),
        "the contrast rule sells the expedition's loot"
    );
    assert!(queued.iter().all(|id| owned(&off, *id)), "the loot is kept");
    assert!(pending(&off, b).is_empty(), "the queue is dropped");
    let runs = records(&off, b);
    assert_eq!((runs[0].auto_sold_count, runs[0].auto_sold_coins), (0, 0));
    assert!(
        battle(&off, b)
            .game()
            .last_result()
            .unwrap()
            .auto_sold
            .is_empty()
    );

    // Turning the scope on later never sells the dropped queue.
    set_rule(&mut off, sell_rule());
    let settled = ledgers(&off);
    advance(&mut off, 3);
    let mut off = reload(&off);
    advance(&mut off, 3);
    assert_eq!(ledgers(&off), settled);

    // The scope covers expeditions and operations only: the main run's own
    // drops still sell under the main-only rule.
    set_rule(&mut off, main_only);
    off.deploy_country_stage(0, Mode::Personal).unwrap();
    let drop = off
        .campaign_mut()
        .add_item_with_quality(5, 1, Some(Quality::Poor))
        .unwrap();
    let a = off.attempt_mut().unwrap();
    a.drop_ids.push(drop);
    a.loot += 1;
    a.loot_quality_counts[Quality::Poor] += 1;
    off.apply(&Command::Withdraw).unwrap();
    assert!(!owned(&off, drop));
    assert_eq!(off.last_result().unwrap().auto_sold.count, 1);
}

#[test]
fn chained_operation_gets_one_receipt_per_stage_with_count_le_loot() {
    let mut g = strategic_game();
    let home = g.campaign().world.cleared[0].clone();
    let recruit = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &home))
        .unwrap();
    let rule = AutoSellRule {
        sell_below: Quality::Rare,
        ..sell_rule()
    };
    set_rule(&mut g, rule);
    let country = world()
        .countries
        .iter()
        .find(|c| {
            g.campaign().world.can_enter(&c.id)
                && !g.campaign().world.cleared.contains(&c.id)
                && c.level_count() > 1
        })
        .unwrap();
    let b = g.deploy_country_operation(recruit, &country.id).unwrap();
    let mut stages = 0;
    while !battle(&g, b).settled {
        assert!(stages < country.level_count(), "the operation ends");
        worker_drop(&mut g, b, 5, Quality::Poor);
        worker_drop(&mut g, b, 6, Quality::Uncommon);
        worker_drop(&mut g, b, 7, Quality::Rare);
        advance(&mut g, 1);
        assert_eq!(pending(&g, b).len(), 3, "stage {stages}");
        win_stage(&mut g, b);
        assert!(g.take_settlement_issues().is_empty());
        stages += 1;
        // One record per stage, each with its own receipt.
        let runs = records(&g, b);
        assert_eq!(runs.len(), stages);
        let record = runs.last().unwrap();
        assert!(
            pending(&g, b).is_empty(),
            "stage {stages} drained its queue"
        );
        // Every fresh drop below Rare is sold: the three staged ones less the
        // Rare, plus the boss's forced drop when it rolled below Rare.
        let sellable: u32 = Quality::ALL
            .into_iter()
            .filter(|q| *q < rule.sell_below)
            .map(|q| record.loot_by_quality[q])
            .sum();
        assert_eq!(record.auto_sold_count, sellable, "stage {stages}");
        assert!(record.auto_sold_count >= 2, "stage {stages}");
        assert!(record.auto_sold_count <= record.loot, "stage {stages}");
        assert!(record.auto_sold_coins > 0, "stage {stages}");
    }
    assert!(stages >= 2, "a chained operation");
    let runs = records(&g, b);
    let report = battle(&g, b).game().last_result().unwrap();
    let last = runs.last().unwrap();
    assert_eq!(report.attempt_id, last.attempt_id);
    assert_eq!(
        (report.auto_sold.count, report.auto_sold.coins),
        (last.auto_sold_count, last.auto_sold_coins),
        "the final stage's receipt is mirrored on the worker's report"
    );
    assert!(report.auto_sold.count + report.auto_sold.kept_on_overflow <= report.loot);
    let settled = ledgers(&g);
    let mut g = reload(&g);
    advance(&mut g, 3);
    assert_eq!(ledgers(&g), settled);
}

/// Ruling E11 and acceptance G4.8: the v6 fixture's live root attempt and
/// live expedition were deployed before v7 (no drop tier), so enabling the
/// widest rule sells none of what they bank. The contrast leg gives the same
/// expedition attempt a drop tier, as a v7 deploy would, and its drops sell.
#[test]
fn v6_inflight_attempt_and_expedition_sell_nothing_after_enabling_rule() {
    let mut g = Game::decode_save_generation(FIXTURE_V6.as_bytes()).expect("v6 fixture");
    let live = g.battles().iter().find(|b| !b.settled).unwrap().id;
    assert_eq!(g.attempt().unwrap().drop_tier, None, "deployed before v7");
    assert_eq!(battle(&g, live).game().attempt().unwrap().drop_tier, None);
    set_rule(
        &mut g,
        AutoSellRule {
            sell_below: Quality::Legendary,
            ..sell_rule()
        },
    );
    g.set_paused(false);
    let mut v7 = g.clone();
    worker(&mut v7, live).attempt_mut().unwrap().drop_tier = Some(1);
    let mut stored = Vec::new();
    for (game, legacy) in [(&mut g, true), (&mut v7, false)] {
        let before = item_ids(game);
        worker_drop(game, live, 5, Quality::Poor);
        worker_drop(game, live, 6, Quality::Common);
        advance(game, 1);
        let new: Vec<u64> = item_ids(game)
            .into_iter()
            .filter(|id| !before.contains(id))
            .collect();
        assert_eq!(new.len(), 2, "legacy {legacy}: both drops are stored");
        assert_eq!(pending(game, live).is_empty(), legacy, "legacy {legacy}");
        game.recall_survivor(live).unwrap();
        game.apply(&Command::Withdraw).unwrap();
        assert!(game.take_settlement_issues().is_empty());
        assert!(pending(game, live).is_empty());
        stored.push((before, new));
    }
    let (before, new) = &stored[0];
    assert!(
        before.iter().chain(new).all(|id| owned(&g, *id)),
        "nothing the v6 game owned or banked is sold"
    );
    assert!(g.last_result().unwrap().auto_sold.is_empty());
    let report = battle(&g, live).game().last_result().unwrap();
    assert!(report.auto_sold.is_empty());
    assert!(
        records(&g, live)
            .iter()
            .all(|r| (r.auto_sold_count, r.auto_sold_coins) == (0, 0))
    );
    reload(&g);
    let (_, new) = &stored[1];
    assert!(
        new.iter().all(|id| !owned(&v7, *id)),
        "the contrast expedition's drops sell"
    );
}

/// Schema v7 §3.6: a queue is disjoint from `reserved_items`. A queued drop
/// is an ordinary stash item while its expedition runs, so the player may
/// equip it on another Ready recruit and send that recruit on duty: as a
/// guard, on a second expedition or defending the main run. The reservation
/// takes the drop out of the queue, so every save stays valid and the
/// deployed item is never sold; the rest of the queue still sells.
#[test]
fn queued_drop_reserved_by_another_deployment_keeps_the_save_valid() {
    let (mut g, b) = expedition(sell_rule());
    let country = g.campaign().world.cleared[0].clone();
    let second = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    worker_drop(&mut g, b, 5, Quality::Poor);
    worker_drop(&mut g, b, 6, Quality::Common);
    advance(&mut g, 1);
    let queued = pending(&g, b);
    assert_eq!(queued.len(), 2);
    let (taken, sold) = (queued[0], queued[1]);
    g.apply(&Command::EquipRecruit {
        id: second,
        item: taken,
    })
    .unwrap();
    for duty in ["guard", "expedition", "takeover"] {
        let mut g = g.clone();
        match duty {
            "guard" => {
                g.apply(&Command::AssignGuard {
                    id: second,
                    country: country.clone(),
                })
                .unwrap();
            }
            "expedition" => {
                g.deploy_survivor(second, &country, 0).unwrap();
            }
            _ => g
                .deploy_country_stage_as(0, DefenderId::Recruit(second))
                .unwrap(),
        }
        assert!(g.campaign().item_reserved(taken), "{duty}");
        let mut g = reload(&g);
        assert_eq!(pending(&g, b), [sold], "{duty}: the reserved drop leaves");
        let paid = value(&g, sold);
        g.recall_survivor(b).unwrap();
        assert!(g.take_settlement_issues().is_empty(), "{duty}");
        assert!(owned(&g, taken), "{duty}: the deployed drop is never sold");
        assert!(!owned(&g, sold), "{duty}: the rest of the queue sells");
        let runs = records(&g, b);
        assert_eq!(
            (runs[0].auto_sold_count, runs[0].auto_sold_coins),
            (1, paid),
            "{duty}"
        );
        reload(&g);
    }
}

/// The final sweep (loot-economy §7): a battle that settles with drops no
/// run record drained (the record-less path) sells them under the same gate
/// before it is marked settled. The receipt goes only to the worker's report.
#[test]
fn final_sweep_sells_a_queue_no_record_drained() {
    let (mut g, b) = expedition(sell_rule());
    worker_drop(&mut g, b, 5, Quality::Poor);
    worker_drop(&mut g, b, 6, Quality::Common);
    advance(&mut g, 1);
    let queued = pending(&g, b);
    assert_eq!(queued.len(), 2);
    let paid: u64 = queued.iter().map(|id| value(&g, *id)).sum();
    // The attempt ends without a record reaching the parent.
    worker(&mut g, b).withdraw();
    worker(&mut g, b).pending_runs_mut().clear();
    let wallet = g.campaign().coins;
    advance(&mut g, 1);
    assert!(g.take_settlement_issues().is_empty());
    assert!(battle(&g, b).settled);
    assert!(pending(&g, b).is_empty(), "a settled battle holds no queue");
    assert!(queued.iter().all(|id| !owned(&g, *id)), "the sweep sells");
    assert_eq!(g.campaign().coins, wallet + paid);
    assert!(records(&g, b).is_empty(), "no record carries the receipt");
    let receipt = &battle(&g, b).game().last_result().unwrap().auto_sold;
    assert_eq!((receipt.count, receipt.coins), (2, paid));
    assert_eq!(receipt.by_quality[Quality::Poor], 1);
    assert_eq!(receipt.by_quality[Quality::Common], 1);
    let settled = ledgers(&g);
    let mut g = reload(&g);
    advance(&mut g, 3);
    assert_eq!(ledgers(&g), settled);
}

/// Ruling D13: `keep_upgrades` judges an expedition's drops against the
/// expedition's own loadout and NPC. Mara holds tier-4 melee and armor; the
/// recruit keeps its starters except a tier-4 rifle. A Common fire axe and
/// padded vest beat the recruit's crowbar and work jacket (kept); a Common
/// marksman rifle beats only Mara's carbine (sold).
#[test]
fn expedition_keep_upgrades_judges_the_expedition_loadout() {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let recruit = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    for definition in [17, 18] {
        let item = g.campaign_mut().add_item(definition).unwrap();
        g.campaign_mut().equip(Mode::Npc, item).unwrap();
    }
    let rifle = g.campaign_mut().add_item(16).unwrap();
    g.apply(&Command::EquipRecruit {
        id: recruit,
        item: rifle,
    })
    .unwrap();
    set_rule(
        &mut g,
        AutoSellRule {
            keep_upgrades: true,
            ..sell_rule()
        },
    );
    let b = g.deploy_survivor(recruit, &country, 0).unwrap();
    for definition in [5, 6, 4] {
        worker_drop(&mut g, b, definition, Quality::Common);
    }
    advance(&mut g, 1);
    let queued = pending(&g, b);
    g.recall_survivor(b).unwrap();
    let kept: Vec<bool> = queued.iter().map(|id| owned(&g, *id)).collect();
    assert_eq!(
        kept,
        [true, true, false],
        "the recruit's upgrades are kept and Mara's sold"
    );
    assert_eq!(records(&g, b)[0].auto_sold_count, 1);
}
