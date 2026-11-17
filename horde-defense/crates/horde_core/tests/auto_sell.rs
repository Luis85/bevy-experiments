//! Main-run auto-sell, hook H1 (W2-14, loot-economy §7; rulings D13, E2,
//! E5 and E15; plan decision 6). When the campaign's rule is enabled, a main
//! run's settlement sells this run's drops (`Attempt.drop_ids`) of a quality
//! strictly below `sell_below` in the selected slots, after the takeover
//! release and before the run record and the report. Every sale guard
//! (`SaleOrigin::Settlement`, which skips only the in-battle guard) still
//! applies, quality-None items are never sold, `keep_upgrades` keeps a drop
//! that would upgrade the defender who ran the battle, an item that would
//! overflow the wallet is kept, and no RNG is consumed. The run record and
//! the report carry one bounded receipt with exact totals. W2-17 adds the
//! pure Results read, `Game::results_loot`, and the last-run preview scope.
mod support;
use horde_core::{
    campaign::{AutoSellRule, AutoSoldLine, MAX_AUTO_SOLD_LINES, UpgradeJudge},
    command::Command,
    content::{Mode, Slot},
    defenders::DefenderId,
    kind_map::KindMap,
    quality::Quality,
    sim::{AutoSellPreview, AutoSellScope, Game, Input, Report, SAVE_VERSION},
    stats::RunRecord,
    strategy::RecruitStatus,
};
use support::{advance, enemy, strategic_game};

/// An enabled rule that sells Poor and Common drops in every slot and keeps
/// no upgrades, so only the threshold, the slots and the guards decide.
fn sell_rule() -> AutoSellRule {
    AutoSellRule {
        enabled: true,
        keep_upgrades: false,
        ..AutoSellRule::default()
    }
}

fn set_rule(g: &mut Game, rule: AutoSellRule) {
    g.apply(&Command::SetAutoSell(rule)).unwrap();
}

/// A strategic game (four cleared countries, 10k coins) with the player on
/// the active country's first stage.
fn personal_run() -> Game {
    let mut g = strategic_game();
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    g
}

/// A level-1 item of `definition` at `quality`, owned but not yet a drop.
fn found(g: &mut Game, definition: usize, quality: Quality) -> u64 {
    g.campaign_mut()
        .add_item_with_quality(definition, 1, Some(quality))
        .unwrap()
}

/// Lists the owned item `id` as one of this run's drops, as the drop site
/// does: `drop_ids` (ascending), `loot` and the quality counts.
fn mark_dropped(g: &mut Game, id: u64) {
    let quality = g.campaign().item(id).unwrap().quality;
    let a = g.attempt_mut().expect("a live run");
    assert!(a.drop_ids.last().is_none_or(|last| *last < id));
    a.drop_ids.push(id);
    a.loot += 1;
    if let Some(quality) = quality {
        a.loot_quality_counts[quality] += 1;
    }
}

/// A new drop of `definition` at `quality` in the live run.
fn drop_item(g: &mut Game, definition: usize, quality: Quality) -> u64 {
    let id = found(g, definition, quality);
    mark_dropped(g, id);
    id
}

/// Ends the live run (a withdrawal settles like any other outcome).
fn settle(g: &mut Game) -> (Report, RunRecord) {
    g.apply(&Command::Withdraw).unwrap();
    assert!(g.attempt().is_none());
    let report = g.last_result().unwrap().clone();
    let record = g.campaign().run_history.last().unwrap().clone();
    assert_eq!(record.attempt_id, report.attempt_id);
    (report, record)
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

/// Saves and loads through validation.
fn reload(g: &Game) -> Game {
    let payload = g
        .to_save_json()
        .unwrap_or_else(|e| panic!("the save does not validate: {e}"));
    Game::from_payload(serde_json::from_str(&payload).unwrap(), SAVE_VERSION)
        .unwrap_or_else(|e| panic!("the save does not load: {e:?}"))
}

/// A run with four sellable drops, before settlement.
fn run_with_cheap_drops() -> (Game, Vec<u64>) {
    let mut g = personal_run();
    let ids = vec![
        drop_item(&mut g, 5, Quality::Poor),
        drop_item(&mut g, 6, Quality::Common),
        drop_item(&mut g, 7, Quality::Poor),
        drop_item(&mut g, 9, Quality::Common),
    ];
    (g, ids)
}

#[test]
fn rule_off_moves_nothing() {
    // The default rule, and a disabled rule that would sell everything.
    let wide_but_off = AutoSellRule {
        enabled: false,
        sell_below: Quality::Legendary,
        keep_upgrades: false,
        ..AutoSellRule::default()
    };
    for rule in [AutoSellRule::default(), wide_but_off] {
        let (mut g, ids) = run_with_cheap_drops();
        set_rule(&mut g, rule);
        let coins = g.campaign().coins;
        let items = item_ids(&g);
        let (report, record) = settle(&mut g);
        assert_eq!(g.campaign().coins, coins, "{rule:?}");
        assert_eq!(item_ids(&g), items, "{rule:?}");
        assert!(ids.iter().all(|id| owned(&g, *id)), "{rule:?}");
        assert!(report.auto_sold.is_empty(), "{rule:?}");
        assert_eq!(report.drop_ids, ids, "{rule:?}");
        assert_eq!((record.auto_sold_count, record.auto_sold_coins), (0, 0));
    }
    // The same run with the rule enabled does sell: the off rule is what
    // keeps the drops, not an empty candidate list.
    let (mut g, ids) = run_with_cheap_drops();
    set_rule(&mut g, sell_rule());
    let (report, _) = settle(&mut g);
    assert_eq!(report.auto_sold.count, 4);
    assert!(ids.iter().all(|id| !owned(&g, *id)));
}

#[test]
fn rule_sells_only_this_runs_drops_below_threshold() {
    let mut g = strategic_game();
    // A Poor find from an earlier run is in the stash, not in this run.
    let earlier = found(&mut g, 5, Quality::Poor);
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    let poor = drop_item(&mut g, 5, Quality::Poor);
    let common = drop_item(&mut g, 6, Quality::Common);
    let uncommon = drop_item(&mut g, 9, Quality::Uncommon);
    let rare = drop_item(&mut g, 10, Quality::Rare);
    let accessory = drop_item(&mut g, 7, Quality::Poor);
    set_rule(
        &mut g,
        AutoSellRule {
            slots: [true, true, true, false],
            ..sell_rule()
        },
    );
    let paid = value(&g, poor) + value(&g, common);
    let coins = g.campaign().coins;
    let (report, record) = settle(&mut g);

    assert!(!owned(&g, poor) && !owned(&g, common), "below Uncommon");
    for (id, why) in [
        (earlier, "an earlier run's drop"),
        (uncommon, "at the threshold"),
        (rare, "above the threshold"),
        (accessory, "an unselected slot"),
    ] {
        assert!(owned(&g, id), "{why} is kept");
    }
    assert_eq!(g.campaign().coins, coins + paid);
    let receipt = &report.auto_sold;
    assert_eq!((receipt.count, receipt.coins), (2, paid));
    assert_eq!(receipt.by_quality[Quality::Poor], 1);
    assert_eq!(receipt.by_quality[Quality::Common], 1);
    assert_eq!(receipt.kept_on_overflow, 0);
    assert_eq!(receipt.lines.len(), 2);
    assert_eq!(
        report.drop_ids,
        vec![poor, common, uncommon, rare, accessory],
        "the report lists every drop, sold ones included"
    );
    assert_eq!(
        (record.auto_sold_count, record.auto_sold_coins),
        (2, paid),
        "the run record carries the receipt totals"
    );
    // The settled state saves and loads.
    let loaded = reload(&g);
    assert_eq!(loaded.last_result().unwrap().auto_sold, *receipt);

    // A higher threshold sells the Uncommon drop of the same run too.
    let mut g = personal_run();
    let uncommon = drop_item(&mut g, 9, Quality::Uncommon);
    let rare = drop_item(&mut g, 10, Quality::Rare);
    set_rule(
        &mut g,
        AutoSellRule {
            sell_below: Quality::Rare,
            ..sell_rule()
        },
    );
    let (report, _) = settle(&mut g);
    assert!(!owned(&g, uncommon) && owned(&g, rare));
    assert_eq!(report.auto_sold.by_quality[Quality::Uncommon], 1);
}

#[test]
fn guarded_drops_are_kept() {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let recruit = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    // Drops the player marked or handed out before settlement: the items
    // exist before the run here, and the run lists them as its drops.
    let locked = found(&mut g, 5, Quality::Poor);
    g.campaign_mut().set_lock(locked, true).unwrap();
    let favorite = found(&mut g, 6, Quality::Poor);
    g.campaign_mut().set_favorite(favorite, true).unwrap();
    let maras = found(&mut g, 5, Quality::Common);
    g.campaign_mut().equip(Mode::Npc, maras).unwrap();
    let recruits = found(&mut g, 6, Quality::Common);
    g.apply(&Command::EquipRecruit {
        id: recruit,
        item: recruits,
    })
    .unwrap();
    let shop = g.campaign_mut().add_item(7).unwrap();
    let gone = found(&mut g, 9, Quality::Poor);
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    for id in [locked, favorite, maras, recruits, shop, gone] {
        mark_dropped(&mut g, id);
    }
    let sold = drop_item(&mut g, 7, Quality::Poor);
    // A listed drop that is no longer owned is skipped, not an error.
    g.campaign_mut().items.retain(|item| item.id != gone);
    set_rule(
        &mut g,
        AutoSellRule {
            sell_below: Quality::Legendary,
            ..sell_rule()
        },
    );
    let (report, record) = settle(&mut g);

    assert!(!owned(&g, sold), "the unguarded drop is sold");
    for (id, why) in [
        (locked, "locked"),
        (favorite, "favourite"),
        (maras, "equipped by Mara"),
        (recruits, "equipped by a recruit"),
        (shop, "quality None"),
    ] {
        assert!(owned(&g, id), "a {why} drop is kept");
    }
    assert_eq!(report.auto_sold.count, 1);
    assert_eq!(record.auto_sold_count, 1);
    reload(&g);
}

/// Mara holds tier-4 melee and armor; recruit 1 keeps its starters except a
/// tier-4 rifle. Returns the game between defenses and the recruit id.
fn judged_game() -> (Game, u64) {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let id = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    for definition in [17, 18] {
        let item = g.campaign_mut().add_item(definition).unwrap();
        g.campaign_mut().equip(Mode::Npc, item).unwrap();
    }
    let rifle = g.campaign_mut().add_item(16).unwrap();
    g.apply(&Command::EquipRecruit { id, item: rifle }).unwrap();
    (g, id)
}

#[test]
fn keep_upgrades_judges_the_takeover_recruits_loadout_not_maras() {
    // A Common fire axe and padded vest beat the recruit's crowbar and work
    // jacket but not Mara's saber and fortress armor; a Common marksman
    // rifle beats Mara's service carbine but not the recruit's rifle.
    let probe = |g: &mut Game| -> [u64; 3] {
        [
            found(g, 5, Quality::Common),
            found(g, 6, Quality::Common),
            found(g, 4, Quality::Common),
        ]
    };
    {
        let (mut g, id) = judged_game();
        let [axe, vest, rifle] = probe(&mut g);
        let (c, s) = (g.campaign(), g.strategy());
        let recruit = UpgradeJudge::Defender(DefenderId::Recruit(id));
        let mara = UpgradeJudge::Defender(DefenderId::Mara);
        assert!(c.is_upgrade_for(axe, &recruit, s) && c.is_upgrade_for(vest, &recruit, s));
        assert!(!c.is_upgrade_for(rifle, &recruit, s));
        assert!(!c.is_upgrade_for(axe, &mara, s) && !c.is_upgrade_for(vest, &mara, s));
        assert!(c.is_upgrade_for(rifle, &mara, s));
    }
    let keep = AutoSellRule {
        keep_upgrades: true,
        ..sell_rule()
    };
    for defender in ["recruit", "mara"] {
        let (mut g, id) = judged_game();
        let target = if defender == "recruit" {
            DefenderId::Recruit(id)
        } else {
            DefenderId::Mara
        };
        g.deploy_country_stage_as(0, target).unwrap();
        let [axe, vest, rifle] = probe(&mut g);
        for item in [axe, vest, rifle] {
            mark_dropped(&mut g, item);
        }
        set_rule(&mut g, keep);
        let (report, _) = settle(&mut g);
        assert_eq!(report.defender(), target);
        let kept: Vec<bool> = [axe, vest, rifle].iter().map(|i| owned(&g, *i)).collect();
        if defender == "recruit" {
            assert_eq!(
                kept,
                [true, true, false],
                "the takeover keeps the recruit's upgrades and sells Mara's"
            );
        } else {
            assert_eq!(
                kept,
                [false, false, true],
                "Mara's run keeps Mara's upgrades"
            );
        }
        assert_eq!(
            report.auto_sold.count,
            if defender == "recruit" { 1 } else { 2 }
        );
    }
}

#[test]
fn takeover_auto_sell_runs_after_release_and_never_sells_the_recruits_loadout() {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let id = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    // The recruit carries two Poor finds, which the run also lists as its
    // drops: the widest rule must still leave the recruit's loadout alone.
    let axe = found(&mut g, 5, Quality::Poor);
    let vest = found(&mut g, 6, Quality::Poor);
    for item in [axe, vest] {
        g.apply(&Command::EquipRecruit { id, item }).unwrap();
    }
    let loadout = g
        .strategy()
        .roster
        .iter()
        .find(|r| r.id == id)
        .unwrap()
        .npc
        .loadout;
    g.deploy_country_stage_as(0, DefenderId::Recruit(id))
        .unwrap();
    assert!(g.campaign().item_reserved(axe), "held by the takeover");
    mark_dropped(&mut g, axe);
    mark_dropped(&mut g, vest);
    let sold = [
        drop_item(&mut g, 7, Quality::Poor),
        drop_item(&mut g, 9, Quality::Common),
    ];
    set_rule(
        &mut g,
        AutoSellRule {
            sell_below: Quality::Legendary,
            ..sell_rule()
        },
    );
    let paid: u64 = sold.iter().map(|i| value(&g, *i)).sum();
    let (report, record) = settle(&mut g);

    // The release came first: the recruit is back in camp and nothing it
    // holds is reserved, yet the sale skipped its loadout.
    let r = g.strategy().roster.iter().find(|r| r.id == id).unwrap();
    assert_eq!(r.status, RecruitStatus::Ready);
    assert_eq!(r.npc.loadout, loadout);
    assert!(g.campaign().reserved.is_none());
    assert!(g.campaign().reserved_recruit.is_none());
    assert!(loadout.iter().all(|item| owned(&g, *item)));
    assert!(
        loadout
            .iter()
            .all(|item| !g.campaign().item_reserved(*item))
    );
    assert!(sold.iter().all(|item| !owned(&g, *item)));
    // One receipt, on the record and the report that name the recruit.
    assert_eq!(report.defender(), DefenderId::Recruit(id));
    assert_eq!(record.recruit_id, Some(id));
    assert_eq!((report.auto_sold.count, report.auto_sold.coins), (2, paid));
    assert_eq!((record.auto_sold_count, record.auto_sold_coins), (2, paid));
    reload(&g);
}

/// A legacy-features game's run on the v7 drop path (`drop_tier` Some, so
/// `drop_ids` records each drop) whose staged kills drop `drops` real items.
fn run_with_real_drops(drops: usize) -> Game {
    let mut g = support::game(Mode::Personal);
    for _ in 0..drops {
        enemy(&mut g, 1, 10.0, false);
        let a = g.attempt_mut().unwrap();
        // Every twentieth kill drops; the staged hostile is already down.
        a.enemies.last_mut().unwrap().hp = 0;
        a.kills += 19 - a.kills % 20;
        g.tick(Input::default());
    }
    assert_eq!(g.attempt().unwrap().drop_ids.len(), drops);
    g
}

#[test]
fn auto_sell_consumes_no_rng() {
    let mut on = run_with_real_drops(6);
    let mut off = on.clone();
    let strategy_rng = on.strategy().rng;
    set_rule(
        &mut on,
        AutoSellRule {
            sell_below: Quality::Legendary,
            ..sell_rule()
        },
    );
    let (sold, _) = settle(&mut on);
    let (kept, _) = settle(&mut off);
    assert_eq!(sold.auto_sold.count, 6, "every drop is sold");
    assert!(kept.auto_sold.is_empty());
    assert_eq!(sold.log, kept.log);
    assert_eq!(sold.telemetry, kept.telemetry);
    // Settlement leaves the strategy layer's random state where it was.
    assert_eq!(
        on.strategy().rng,
        off.strategy().rng,
        "the sale drew strategy RNG"
    );
    assert_eq!(
        on.strategy().rng,
        strategy_rng,
        "settlement drew strategy RNG"
    );
    // The next run starts from the same random state and plays out the same
    // way, tick for tick.
    for g in [&mut on, &mut off] {
        g.deploy(0, Mode::Personal).unwrap();
        g.set_paused(false);
        enemy(g, 1, 40.0, false);
    }
    assert_eq!(on.attempt().unwrap().rng, off.attempt().unwrap().rng);
    for step in 0..10 {
        advance(&mut on, 60);
        advance(&mut off, 60);
        assert_eq!(
            serde_json::to_value(on.attempt()).unwrap(),
            serde_json::to_value(off.attempt()).unwrap(),
            "step {step}"
        );
    }
}

#[test]
fn wallet_overflow_keeps_items() {
    let mut g = personal_run();
    let first = drop_item(&mut g, 5, Quality::Poor);
    let dear = drop_item(&mut g, 14, Quality::Common);
    let last = drop_item(&mut g, 6, Quality::Poor);
    let (v_first, v_dear, v_last) = (value(&g, first), value(&g, dear), value(&g, last));
    assert!(v_dear > v_last && v_first > 0 && v_last > 0);
    // Room for the first and last drops, not for the dear one between them.
    let wallet = u64::MAX - v_first - v_last;
    g.campaign_mut().coins = wallet;
    set_rule(&mut g, sell_rule());
    let (report, record) = settle(&mut g);

    assert_eq!(g.campaign().coins, u64::MAX, "the wallet never wraps");
    assert!(!owned(&g, first) && !owned(&g, last));
    assert!(owned(&g, dear), "the drop that would overflow is kept");
    let receipt = &report.auto_sold;
    assert_eq!(receipt.count, 2);
    assert_eq!(receipt.coins, v_first + v_last);
    assert_eq!(receipt.kept_on_overflow, 1);
    assert_eq!(
        (record.auto_sold_count, record.auto_sold_coins),
        (2, v_first + v_last)
    );
    assert!(receipt.validate(report.loot).is_ok());
    // A full wallet keeps every drop and counts each one.
    let mut g = personal_run();
    let ids = [
        drop_item(&mut g, 5, Quality::Poor),
        drop_item(&mut g, 6, Quality::Common),
    ];
    g.campaign_mut().coins = u64::MAX;
    set_rule(&mut g, sell_rule());
    let (report, record) = settle(&mut g);
    assert!(ids.iter().all(|id| owned(&g, *id)));
    assert_eq!(report.auto_sold.count, 0);
    assert_eq!(report.auto_sold.coins, 0);
    assert_eq!(report.auto_sold.kept_on_overflow, 2);
    assert_eq!((record.auto_sold_count, record.auto_sold_coins), (0, 0));
    reload(&g);
}

#[test]
fn fifty_sales_one_receipt_12_lines_exact_totals() {
    let mut g = personal_run();
    let records = g.campaign().run_history.len();
    // Fifty Poor and Common drops over every slot of tiers 1 to 4 at
    // levels 1 to 3; equal definitions, levels and qualities tie on coins.
    let definitions = [4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19];
    let mut ids = Vec::new();
    for n in 0..50usize {
        let quality = if n % 3 == 0 {
            Quality::Poor
        } else {
            Quality::Common
        };
        let level = 1 + (n % 3) as u32;
        let id = g
            .campaign_mut()
            .add_item_with_quality(definitions[n % definitions.len()], level, Some(quality))
            .unwrap();
        mark_dropped(&mut g, id);
        ids.push(id);
    }
    let mut expected: Vec<(u64, u64, AutoSoldLine)> = ids
        .iter()
        .map(|id| {
            let item = g.campaign().item(*id).unwrap();
            let coins = value(&g, *id);
            let line = AutoSoldLine {
                definition: item.definition.clone(),
                level: item.level,
                quality: item.quality.unwrap(),
                coins,
            };
            (coins, *id, line)
        })
        .collect();
    let total: u64 = expected.iter().map(|(coins, ..)| coins).sum();
    let mut by_quality: KindMap<Quality, u32> = KindMap::default();
    for (.., line) in &expected {
        by_quality[line.quality] += 1;
    }
    // Highest coins first, ties to the lower item id.
    expected.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let lines: Vec<AutoSoldLine> = expected
        .into_iter()
        .take(MAX_AUTO_SOLD_LINES)
        .map(|(.., line)| line)
        .collect();
    set_rule(&mut g, sell_rule());
    let coins = g.campaign().coins;
    let (report, record) = settle(&mut g);

    assert_eq!(g.campaign().run_history.len(), records + 1, "one record");
    assert!(ids.iter().all(|id| !owned(&g, *id)));
    assert_eq!(g.campaign().coins, coins + total);
    let receipt = &report.auto_sold;
    assert_eq!(receipt.count, 50);
    assert_eq!(receipt.coins, total, "the total is exact past the line cap");
    assert_eq!(receipt.by_quality, by_quality);
    assert_eq!(receipt.lines.len(), MAX_AUTO_SOLD_LINES);
    assert_eq!(receipt.lines, lines);
    assert_eq!(receipt.kept_on_overflow, 0);
    assert!(receipt.validate(report.loot).is_ok());
    assert_eq!(
        (record.auto_sold_count, record.auto_sold_coins),
        (50, total)
    );
    let loaded = reload(&g);
    assert_eq!(loaded.last_result().unwrap().auto_sold, *receipt);
}

// ---------------------------------------------------------------------------
// `Game::results_loot` (W2-17, loot-economy §8): the Results view-model of
// the reported run's loot, read from the saved report and the live stash.

/// `defender` holds the judged game's first stage and finds a Common and an
/// Uncommon fire axe, a Common padded vest, a Common marksman rifle and a
/// Poor accessory; the enabled rule sells Poor only. Returns the settled
/// game and the drops in drop order.
fn settled_results_run(mut g: Game, defender: DefenderId) -> (Game, [u64; 5]) {
    g.deploy_country_stage_as(0, defender).unwrap();
    let drops = [
        drop_item(&mut g, 5, Quality::Common),
        drop_item(&mut g, 5, Quality::Uncommon),
        drop_item(&mut g, 6, Quality::Common),
        drop_item(&mut g, 4, Quality::Common),
        drop_item(&mut g, 7, Quality::Poor),
    ];
    set_rule(
        &mut g,
        AutoSellRule {
            sell_below: Quality::Common,
            ..sell_rule()
        },
    );
    settle(&mut g);
    (g, drops)
}

#[test]
fn results_loot_survives_reload_and_offers_upgrades_for_defender() {
    let (g, recruit) = judged_game();
    let (mut g, [axe, better_axe, vest, rifle, poor]) =
        settled_results_run(g, DefenderId::Recruit(recruit));
    let report = g.last_result().unwrap().clone();
    let loot = g.results_loot().expect("a reported run has a loot view");
    assert_eq!(loot.attempt_id, report.attempt_id);
    assert_eq!(loot.total, 5);
    assert_eq!(loot.total, report.loot);
    assert_eq!(loot.by_quality, report.loot_by_quality);
    assert_eq!(loot.by_quality[Quality::Common], 3);
    assert_eq!(loot.by_quality[Quality::Uncommon], 1);
    assert_eq!(loot.by_quality[Quality::Poor], 1);
    // The Poor accessory was auto-sold; the four kept drops are listed in
    // drop order with what the Results row shows.
    assert!(!owned(&g, poor));
    assert_eq!(loot.auto_sold, report.auto_sold);
    assert_eq!(loot.auto_sold.count, 1);
    let kept: Vec<u64> = loot.kept.iter().map(|k| k.item_id).collect();
    assert_eq!(kept, [axe, better_axe, vest, rifle]);
    for k in &loot.kept {
        let item = g.campaign().item(k.item_id).unwrap();
        assert_eq!(k.definition, item.definition);
        assert_eq!(k.quality, item.quality);
        assert_eq!(k.level, item.level);
    }
    // Upgrades are judged for the recruit who defended, at most one a slot
    // (the best), highest gain first: the Uncommon axe beats the Common
    // one, the vest beats the work jacket, and the marksman rifle (Mara's
    // upgrade) is no upgrade over the recruit's tier-4 rifle.
    let mut slots: Vec<(Slot, u64)> = loot.upgrades.iter().map(|u| (u.slot, u.item_id)).collect();
    slots.sort_by_key(|(slot, _)| slot.index());
    assert_eq!(slots, [(Slot::Melee, better_axe), (Slot::Outfit, vest)]);
    assert!(
        loot.upgrades
            .windows(2)
            .all(|pair| pair[0].gain >= pair[1].gain)
    );
    let r = g
        .strategy()
        .roster
        .iter()
        .find(|r| r.id == recruit)
        .unwrap();
    for offer in &loot.upgrades {
        assert!(offer.gain > 0.0);
        assert_eq!(
            Some(offer.gain),
            g.campaign()
                .upgrade_gain(Mode::Npc, r.npc.loadout, &r.npc, offer.item_id)
        );
        assert_eq!(
            offer.command,
            Command::EquipRecruit {
                id: recruit,
                item: offer.item_id
            }
        );
    }
    // A save and load reads back the same view.
    assert_eq!(reload(&g).results_loot(), Some(loot.clone()));
    // The offer's command equips it, and the slot is no longer offered: the
    // Common axe is no upgrade over the Uncommon one.
    let melee = loot
        .upgrades
        .iter()
        .find(|u| u.slot == Slot::Melee)
        .unwrap();
    g.apply(&melee.command).unwrap();
    let offered = |g: &Game| -> Vec<u64> {
        g.results_loot()
            .unwrap()
            .upgrades
            .iter()
            .map(|u| u.item_id)
            .collect()
    };
    assert_eq!(g.results_loot().unwrap().kept, loot.kept);
    assert_eq!(offered(&g), [vest]);
    // A recruit away on an expedition cannot change equipment, so nothing
    // is offered until it is back.
    let country = g.campaign().world.cleared[0].clone();
    let b = g.deploy_survivor(recruit, &country, 0).unwrap();
    assert_eq!(offered(&g), Vec::<u64>::new());
    g.recall_survivor(b).unwrap();
    assert_eq!(offered(&g), [vest]);
    // An item another deployment holds is not offered, though the defender
    // is free: a second recruit takes the vest on an expedition.
    let other = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    g.apply(&Command::EquipRecruit {
        id: other,
        item: vest,
    })
    .unwrap();
    let away = g.deploy_survivor(other, &country, 0).unwrap();
    assert!(g.campaign().item_reserved(vest), "held by the expedition");
    let r = g
        .strategy()
        .roster
        .iter()
        .find(|r| r.id == recruit)
        .unwrap();
    assert_eq!(r.status, RecruitStatus::Ready, "the defender is in camp");
    assert_eq!(offered(&g), Vec::<u64>::new());
    g.recall_survivor(away).unwrap();
    assert_eq!(offered(&g), [vest]);

    // The same drops after Mara's run are judged for Mara: the marksman
    // rifle beats her carbine, the axes and the vest do not beat her tier-4
    // gear, and the offer equips her (Npc) loadout.
    let (g, _) = judged_game();
    let (g, [.., rifle, _]) = settled_results_run(g, DefenderId::Mara);
    let loot = g.results_loot().unwrap();
    assert_eq!(loot.kept.len(), 4);
    assert_eq!(loot.upgrades.len(), 1);
    assert_eq!(loot.upgrades[0].slot, Slot::Ranged);
    assert_eq!(loot.upgrades[0].item_id, rifle);
    assert_eq!(
        loot.upgrades[0].command,
        Command::Equip {
            mode: Mode::Npc,
            id: rifle
        }
    );
    assert_eq!(reload(&g).results_loot(), Some(loot));
    // No reported run, no loot view.
    assert_eq!(strategic_game().results_loot(), None);
}

#[test]
fn previews_and_results_loot_do_not_mutate_game() {
    let (g, recruit) = judged_game();
    let (mut g, _) = settled_results_run(g, DefenderId::Recruit(recruit));
    // Earlier finds in the stash, one of them locked.
    let stash = found(&mut g, 9, Quality::Poor);
    g.campaign_mut().set_lock(stash, true).unwrap();
    found(&mut g, 10, Quality::Common);
    let rules = [
        AutoSellRule::default(),
        AutoSellRule {
            sell_below: Quality::Legendary,
            ..sell_rule()
        },
        AutoSellRule {
            keep_upgrades: true,
            sell_below: Quality::Rare,
            ..sell_rule()
        },
    ];
    let mut deployed = g.clone();
    deployed.deploy_country_stage(0, Mode::Personal).unwrap();
    for g in [g, deployed] {
        let before = serde_json::to_value(&g).unwrap();
        let digest = g.state_digest();
        let strategy_rng = g.strategy().rng.state;
        let attempt_rng = g.attempt().map(|a| a.rng.state);
        for rule in &rules {
            for scope in [AutoSellScope::Stash, AutoSellScope::LastRun] {
                let _ = g.auto_sell_preview(rule, scope);
            }
        }
        let _ = g.results_loot();
        let _ = g.results_loot();
        assert_eq!(serde_json::to_value(&g).unwrap(), before);
        assert_eq!(g.state_digest(), digest);
        assert_eq!(g.strategy().rng.state, strategy_rng);
        assert_eq!(g.attempt().map(|a| a.rng.state), attempt_rng);
    }
}

#[test]
fn last_run_preview_considers_only_the_reported_runs_kept_drops() {
    let mut g = strategic_game();
    // A find from before the run stays out of the last-run scope.
    let earlier = found(&mut g, 9, Quality::Poor);
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    let poor = drop_item(&mut g, 5, Quality::Poor);
    let common = drop_item(&mut g, 6, Quality::Common);
    let rare = drop_item(&mut g, 10, Quality::Rare);
    let locked = drop_item(&mut g, 7, Quality::Poor);
    // The rule is off at settlement, so every drop is kept.
    let (report, _) = settle(&mut g);
    assert!(report.auto_sold.is_empty());
    g.campaign_mut().set_lock(locked, true).unwrap();
    let rule = AutoSellRule {
        sell_below: Quality::Uncommon,
        ..sell_rule()
    };
    let last = g.auto_sell_preview(&rule, AutoSellScope::LastRun);
    assert_eq!(last.ids, [poor, common]);
    assert_eq!(last.coins, value(&g, poor) + value(&g, common));
    assert_eq!(last.blocked, 1, "the locked drop");
    let stash = g.auto_sell_preview(&rule, AutoSellScope::Stash);
    assert_eq!(stash.ids, [earlier, poor, common]);
    // The panel's "raise the threshold" hint reads this: once the Poor drop
    // is gone the last run has nothing below Common, but more below
    // Legendary.
    g.apply(&Command::SellItem(poor)).unwrap();
    let narrow = AutoSellRule {
        sell_below: Quality::Common,
        ..sell_rule()
    };
    assert_eq!(
        g.auto_sell_preview(&narrow, AutoSellScope::LastRun).count,
        0
    );
    let wider = AutoSellRule {
        sell_below: Quality::Legendary,
        ..sell_rule()
    };
    assert_eq!(
        g.auto_sell_preview(&wider, AutoSellScope::LastRun).ids,
        [common, rare]
    );
    // No reported run, no last-run scope.
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    assert!(g.last_result().is_none());
    assert_eq!(
        g.auto_sell_preview(&wider, AutoSellScope::LastRun),
        AutoSellPreview::default()
    );
}
