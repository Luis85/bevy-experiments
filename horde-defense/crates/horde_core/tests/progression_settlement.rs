//! Progression settlement, W2-C (W2-22; design-progression-tree §6-7,
//! rulings E1 and E2; plan decision 14, PT7, PT8, SD15; acceptance G5.2).
//! `Game::settle_progression_rewards` pays the level-up grants above
//! `Player.level_grant_paid` and the reached, unclaimed rank milestones once,
//! at the root only: in `finish()` after the H1 auto-sell and before the run
//! record and the report, and at the end of `settle_parallel` when no main
//! attempt is live. It never pays mid-battle or in a worker. The shipped
//! catalogue authors no grant and no milestone yet, so these tests author
//! the design values on their own thread (`override_rewards`).
mod support;
use horde_core::{
    campaign::AutoSellRule,
    command::Command,
    content::Mode,
    progression::{
        MilestoneDefinition, ProgressionRewardCatalog, RewardsOverride, catalog, level_grant,
        override_rewards, xp_to_advance_from,
    },
    quality::Quality,
    sim::{
        Game, Input, MilestoneReward, Outcome, ParallelBattle, ProgressionRewards, SAVE_VERSION,
        SettlementIssue,
    },
    stats::RunRecord,
    world::world,
};
use support::{advance, enemy, strategic_game};

fn milestone(rank: u32, coins: u64, item_quality: Quality) -> MilestoneDefinition {
    MilestoneDefinition {
        rank,
        coins,
        item_quality,
    }
}

/// The shipped reward block with the design's level grant (0.25) and the
/// given milestones authored for this thread.
fn authored(milestones: Vec<MilestoneDefinition>) -> RewardsOverride {
    override_rewards(ProgressionRewardCatalog {
        level_grant_xp_fraction: 0.25,
        milestones,
        ..catalog().rewards.clone()
    })
}

/// The design's four milestones (§7).
fn design_milestones() -> Vec<MilestoneDefinition> {
    vec![
        milestone(5, 500, Quality::Uncommon),
        milestone(10, 1000, Quality::Uncommon),
        milestone(15, 1500, Quality::Rare),
        milestone(20, 2000, Quality::Rare),
    ]
}

/// A legacy game with a live, unpaused Personal attempt on level 0.
fn personal_run() -> Game {
    support::game(Mode::Personal)
}

/// Ends the live main run in victory on the next tick.
fn win_main(g: &mut Game) {
    enemy(g, 0, 20.0, true);
    let a = g.attempt_mut().unwrap();
    a.boss_spawned = true;
    a.enemies.last_mut().unwrap().hp = 0;
    advance(g, 1);
    assert!(g.attempt().is_none());
    assert_eq!(g.last_result().unwrap().outcome, Outcome::Victory);
}

/// Withdraws the live main run.
fn withdraw(g: &mut Game) {
    g.apply(&Command::Withdraw).unwrap();
    assert!(g.attempt().is_none());
}

fn receipt(g: &Game) -> ProgressionRewards {
    g.last_result().unwrap().progression_rewards.clone()
}

fn last_record(g: &Game) -> RunRecord {
    g.campaign().run_history.last().unwrap().clone()
}

fn ledger(g: &Game) -> (u32, u32) {
    let player = &g.campaign().player;
    (player.level, player.level_grant_paid)
}

/// Saves through validation and loads.
fn reload(g: &Game) -> Game {
    let payload = g
        .to_save_json()
        .unwrap_or_else(|e| panic!("the save does not validate: {e}"));
    Game::from_payload(serde_json::from_str(&payload).unwrap(), SAVE_VERSION)
        .unwrap_or_else(|e| panic!("the save does not load: {e:?}"))
}

/// Kill XP credited mid-run, enough to take a level-1 player to `level`.
fn level_to(g: &mut Game, level: u32) {
    let from = g.campaign().player.level;
    let xp: u64 = (from..level).map(xp_to_advance_from).sum();
    g.campaign_mut().reward(0, xp, Mode::Personal);
    assert_eq!(g.campaign().player.level, level);
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

/// A strategic game whose player is one XP short of a level-up, with a Ready
/// recruit on an expedition to the starting country's first stage.
fn expedition_one_xp_short() -> (Game, u64) {
    let mut g = strategic_game();
    let next = g.campaign().player.next_level_xp();
    g.campaign_mut().player.xp = next - 1;
    let country = g.campaign().world.cleared[0].clone();
    let recruit = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    let b = g.deploy_survivor(recruit, &country, 0).unwrap();
    (g, b)
}

/// Ends the expedition's current stage in victory on the next tick.
fn win_stage(g: &mut Game, b: u64) {
    let w = worker(g, b);
    enemy(w, 5, 20.0, true);
    let a = w.attempt_mut().unwrap();
    a.boss_spawned = true;
    a.enemies[0].injury.unconscious = true;
    advance(g, 1);
}

/// Decision 14: a run that takes the player from level 1 to 6 pays
/// 5 + 9 + 14 + 21 + 28 = 77 coins once, at the root finish, with one receipt
/// on the report and the run record. The next run owes nothing.
#[test]
fn level_grant_is_paid_once_at_root_finish() {
    let _rewards = authored(Vec::new());
    assert_eq!((2..=6).map(level_grant).sum::<u64>(), 77);
    let mut g = personal_run();
    level_to(&mut g, 6);
    let coins = g.campaign().coins;
    withdraw(&mut g);
    assert_eq!(
        g.campaign().coins,
        coins + 77,
        "the grant is paid at finish"
    );
    assert_eq!(ledger(&g), (6, 6));
    assert_eq!(
        receipt(&g),
        ProgressionRewards {
            level_from: 1,
            level_to: 6,
            level_coins: 77,
            milestones: Vec::new(),
        }
    );
    let record = last_record(&g);
    assert_eq!((record.level_grant_coins, record.milestone_coins), (77, 0));

    let g = reload(&g);
    assert_eq!(ledger(&g), (6, 6), "the ledger is saved");
    let mut g = g;
    g.deploy(0, Mode::Personal).unwrap();
    let coins = g.campaign().coins;
    withdraw(&mut g);
    assert_eq!(g.campaign().coins, coins, "nothing is owed twice");
    assert!(receipt(&g).is_empty());
    assert_eq!(last_record(&g).level_grant_coins, 0);
}

/// The level-up itself pays nothing: kill XP mid-battle, ticks and a save
/// and reload of the live attempt leave the ledger and the wallet alone.
/// The reloaded attempt's finish pays the grant once.
#[test]
fn grant_is_not_paid_while_the_attempt_is_live() {
    let _rewards = authored(Vec::new());
    let mut g = personal_run();
    let coins = g.campaign().coins;
    level_to(&mut g, 4);
    assert_eq!(ledger(&g), (4, 0), "a live level-up pays nothing");
    advance(&mut g, 5);
    let mut g = reload(&g);
    assert!(g.attempt().is_some());
    assert_eq!(ledger(&g), (4, 0), "nor does a mid-battle save");
    assert_eq!(g.campaign().coins, coins);
    withdraw(&mut g);
    let owed: u64 = (2..=4).map(level_grant).sum();
    assert_eq!(owed, 5 + 9 + 14);
    assert_eq!(g.campaign().coins, coins + owed);
    assert_eq!(ledger(&g), (4, 4));
    assert_eq!(receipt(&g).level_coins, owed);
}

/// Ruling E2 (the double-pay the ledger fixes): a worker victory levels the
/// worker's own copy of the player and, at settlement, the parent's player.
/// The worker pays nothing; the parent pays the grant once, at the end of the
/// pass (no main attempt is live), with the receipt on the worker's report
/// and the run record just appended. Later passes and a reload pay nothing.
#[test]
fn worker_victory_level_up_pays_the_grant_once_in_the_parent_and_never_in_the_worker() {
    let _rewards = authored(Vec::new());
    let (mut g, b) = expedition_one_xp_short();
    let (level, paid) = ledger(&g);
    assert_eq!(paid, 0);
    let worker_ledger = battle(&g, b).game().campaign().player.level_grant_paid;
    let coins = g.campaign().coins;
    win_stage(&mut g, b);

    let w = battle(&g, b).game();
    let report = w.last_result().unwrap();
    assert_eq!(report.outcome, Outcome::Victory);
    assert!(w.campaign().player.level > level, "the worker levelled");
    assert_eq!(
        w.campaign().player.level_grant_paid,
        worker_ledger,
        "a worker never settles its ledger"
    );
    assert_eq!(
        w.campaign().coins,
        report.coins,
        "the worker's wallet holds only its run's coins"
    );
    let worker_coins = w.campaign().coins;

    let (new_level, paid) = ledger(&g);
    assert!(new_level > level, "the parent levelled at settlement");
    assert_eq!(paid, new_level);
    let owed: u64 = (level.max(1) + 1..=new_level).map(level_grant).sum();
    assert!(owed > 0);
    assert_eq!(g.campaign().coins, coins + worker_coins + owed);
    let expected = ProgressionRewards {
        level_from: level.max(1),
        level_to: new_level,
        level_coins: owed,
        milestones: Vec::new(),
    };
    assert_eq!(report.progression_rewards, expected);
    let records: Vec<RunRecord> = g
        .campaign()
        .run_history
        .iter()
        .filter(|r| r.worker_id == Some(b))
        .cloned()
        .collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].level_grant_coins, owed);

    let coins = g.campaign().coins;
    advance(&mut g, 3);
    let mut g = reload(&g);
    advance(&mut g, 3);
    assert_eq!(g.campaign().coins, coins, "paid exactly once");
    assert_eq!(ledger(&g), (new_level, new_level));
}

/// Milestones pay once per rank, against the saved claims: rank 5's coins
/// and item at the victory that reaches it, nothing at rank 6 after a save
/// and reload, and rank 10 alone when it is reached.
#[test]
fn milestone_paid_once_per_rank_across_save_reload() {
    let _rewards = authored(design_milestones());
    let mut g = personal_run();
    g.campaign_mut().victories = 4;
    let coins = g.campaign().coins;
    let items = g.campaign().next_item_id - 1;
    win_main(&mut g);
    assert_eq!(g.campaign().progression_rank(), 5);
    assert_eq!(g.campaign().milestones_claimed, [5].into());
    let paid = receipt(&g);
    let grant = paid.level_coins;
    let [rank_5] = paid.milestones.as_slice() else {
        panic!("one milestone: {paid:?}");
    };
    assert_eq!((rank_5.rank, rank_5.coins), (5, 500));
    let item = g.campaign().item(rank_5.item_id.unwrap()).unwrap().clone();
    assert_eq!(Some(item.definition.clone()), rank_5.item_definition);
    assert!(item.id > items, "a new item");
    assert!(!g.last_result().unwrap().drop_ids.contains(&item.id));
    assert_eq!(item.level, g.campaign().shop_item_level());
    assert!(item.quality.unwrap() >= Quality::Uncommon);
    let run_coins = g.last_result().unwrap().coins;
    assert_eq!(g.campaign().coins, coins + run_coins + 500 + grant);
    assert_eq!(last_record(&g).milestone_coins, 500);

    let mut g = reload(&g);
    assert_eq!(g.campaign().milestones_claimed, [5].into());
    g.deploy(0, Mode::Personal).unwrap();
    g.set_paused(false);
    win_main(&mut g);
    assert_eq!(g.campaign().progression_rank(), 6);
    assert!(receipt(&g).milestones.is_empty(), "rank 5 is paid once");
    assert_eq!(last_record(&g).milestone_coins, 0);

    g.campaign_mut().victories = 9;
    g.deploy(0, Mode::Personal).unwrap();
    g.set_paused(false);
    let coins = g.campaign().coins;
    win_main(&mut g);
    let paid = receipt(&g);
    assert_eq!(
        paid.milestones
            .iter()
            .map(|m| (m.rank, m.coins))
            .collect::<Vec<_>>(),
        [(10, 1000)]
    );
    assert_eq!(g.campaign().milestones_claimed, [5, 10].into());
    let run_coins = g.last_result().unwrap().coins;
    assert_eq!(
        g.campaign().coins,
        coins + run_coins + 1000 + paid.level_coins
    );
}

/// The milestone item is a keyed pick (`keyed(seed, rank, 0, MILESTONE)`),
/// never an RNG draw: the strategy RNG is untouched, a second game with the
/// same seed but another strategy RNG state is granted the same item, and the
/// item is the campaign's pure `milestone_item_definition`.
#[test]
fn milestone_item_pick_draws_no_rng() {
    let _rewards = authored(design_milestones());
    let run = |strategy_seed: u64| {
        let mut g = personal_run();
        g.set_seed(0x5EED);
        g.with_strategy(|strategy, _| strategy.rng = horde_core::rules::Rng::new(strategy_seed));
        g.campaign_mut().victories = 4;
        let strategy = serde_json::to_value(g.strategy()).unwrap();
        win_main(&mut g);
        assert_eq!(
            serde_json::to_value(g.strategy()).unwrap(),
            strategy,
            "settlement draws no strategy RNG"
        );
        let paid = receipt(&g);
        let item = g
            .campaign()
            .item(paid.milestones[0].item_id.unwrap())
            .unwrap();
        // The pick reads the settled campaign (its first clear may unlock a
        // tier), the seed and the rank only.
        let expected = g.campaign().milestone_item_definition(g.seed(), 5);
        assert_eq!(Some(item.definition_index()), expected);
        item.definition.clone()
    };
    assert_eq!(run(1), run(0xFFFF_0000));
}

/// Ruling E2's order: H1 auto-sell runs before the milestone. A wallet one
/// sale short of full sells the run's Common drop first, and only then do the
/// milestone's coins saturate it; paying the milestone first would make the
/// sale overflow and keep the drop. The milestone item is never auto-sold.
#[test]
fn finish_settlement_order_auto_sell_runs_before_milestone_item() {
    let _rewards = authored(vec![milestone(5, 500, Quality::Common)]);
    let mut g = personal_run();
    g.campaign_mut().victories = 5;
    g.apply(&Command::SetAutoSell(AutoSellRule {
        enabled: true,
        keep_upgrades: false,
        ..AutoSellRule::default()
    }))
    .unwrap();
    let drop = g
        .campaign_mut()
        .add_item_with_quality(21, 1, Some(Quality::Common))
        .unwrap();
    let a = g.attempt_mut().unwrap();
    a.drop_ids.push(drop);
    a.loot += 1;
    a.loot_quality_counts[Quality::Common] += 1;
    let value = g.campaign().item_sale_value(drop).unwrap();
    g.campaign_mut().coins = u64::MAX - value;
    withdraw(&mut g);

    let report = g.last_result().unwrap();
    assert!(g.campaign().item(drop).is_none(), "the drop sold first");
    assert_eq!(
        (report.auto_sold.count, report.auto_sold.kept_on_overflow),
        (1, 0)
    );
    let [
        MilestoneReward {
            rank: 5,
            coins: 500,
            item_id: Some(item),
            ..
        },
    ] = report.progression_rewards.milestones.as_slice()
    else {
        panic!("rank 5 paid: {:?}", report.progression_rewards);
    };
    assert!(
        g.campaign().item(*item).is_some(),
        "the milestone item is kept"
    );
    assert_eq!(g.campaign().coins, u64::MAX, "the milestone coins saturate");
    let record = last_record(&g);
    assert_eq!((record.auto_sold_count, record.milestone_coins), (1, 500));
}

/// A worker victory while the main run is live owes the grant, but the pass
/// defers it: nothing is paid and the worker's report carries no receipt.
/// The main run's finish then pays it once, on the main report and record.
#[test]
fn settle_parallel_defers_rewards_while_main_attempt_is_live() {
    let _rewards = authored(Vec::new());
    let (mut g, b) = expedition_one_xp_short();
    let (level, _) = ledger(&g);
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    g.set_paused(false);
    win_stage(&mut g, b);
    assert!(g.attempt().is_some(), "the main run is still live");
    let w = battle(&g, b).game();
    assert_eq!(w.last_result().unwrap().outcome, Outcome::Victory);
    assert!(w.last_result().unwrap().progression_rewards.is_empty());
    let (new_level, paid) = ledger(&g);
    assert!(new_level > level, "the worker victory levelled the parent");
    assert_eq!(paid, 0, "deferred while the main run is live");
    let record = g.campaign().run_history.last().unwrap();
    assert_eq!(record.worker_id, Some(b));
    assert_eq!(record.level_grant_coins, 0);

    let mut g = reload(&g);
    advance(&mut g, 2);
    assert_eq!(ledger(&g).1, 0, "later passes defer too");
    let coins = g.campaign().coins;
    withdraw(&mut g);
    let owed: u64 = (2..=new_level).map(level_grant).sum();
    assert_eq!(g.campaign().coins, coins + owed);
    assert_eq!(ledger(&g), (new_level, new_level));
    assert_eq!(receipt(&g).level_coins, owed);
    let record = last_record(&g);
    assert_eq!((record.worker_id, record.level_grant_coins), (None, owed));
    assert!(
        battle(&g, b)
            .game()
            .last_result()
            .unwrap()
            .progression_rewards
            .is_empty()
    );
}

/// Design §7: a milestone item that cannot be stored is reported once as
/// `MilestoneItemLost`; its coins and the claim still apply, and the receipt
/// names no item.
#[test]
fn milestone_item_loss_keeps_the_coins_and_the_claim() {
    let _rewards = authored(design_milestones());
    let mut g = personal_run();
    g.campaign_mut().victories = 5;
    g.campaign_mut().next_item_id = u64::MAX;
    let coins = g.campaign().coins;
    withdraw(&mut g);
    assert_eq!(g.campaign().coins, coins + 500);
    assert_eq!(g.campaign().milestones_claimed, [5].into());
    assert_eq!(
        receipt(&g).milestones,
        [MilestoneReward {
            rank: 5,
            coins: 500,
            item_id: None,
            item_definition: None,
        }]
    );
    let issues = g.take_settlement_issues();
    assert!(
        matches!(
            issues.as_slice(),
            [SettlementIssue::MilestoneItemLost { rank: 5, .. }]
        ),
        "{issues:?}"
    );
    assert!(g.take_settlement_issues().is_empty(), "reported once");
}

/// Pin (W2-22 review): the record-less path (a legacy worker report whose
/// run never reached `pending_runs`) settles the victory it credits, but it
/// appends no run record, and a country operation that redeploys its next
/// stage in the same pass replaces that report. The receipt then has no
/// home: the coins and the ledger land once, and no report or record names
/// them. The frozen v7 schema has no other slot for it.
#[test]
fn record_less_operation_redeploy_pays_the_grant_without_a_receipt() {
    let _rewards = authored(Vec::new());
    let mut g = strategic_game();
    let next = g.campaign().player.next_level_xp();
    g.campaign_mut().player.xp = next - 1;
    let home = g.campaign().world.cleared[0].clone();
    let recruit = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &home))
        .unwrap();
    let country = world()
        .countries
        .iter()
        .find(|c| {
            g.campaign().world.can_enter(&c.id)
                && !g.campaign().world.cleared.contains(&c.id)
                && c.level_count() > 1
        })
        .unwrap()
        .id
        .clone();
    let op = g.deploy_country_operation(recruit, &country).unwrap();
    // The worker finishes its stage on its own, and its record is dropped,
    // as a legacy worker never queued one.
    let w = worker(&mut g, op);
    enemy(w, 5, 20.0, true);
    let a = w.attempt_mut().unwrap();
    a.boss_spawned = true;
    a.enemies[0].injury.unconscious = true;
    w.tick(Input::default());
    assert_eq!(w.last_result().unwrap().outcome, Outcome::Victory);
    w.pending_runs_mut().clear();
    let (level, paid) = ledger(&g);
    assert_eq!(paid, 0);
    let runs = g.campaign().run_history.len();
    let coins = g.campaign().coins;
    let earned = battle(&g, op).game().campaign().coins;

    advance(&mut g, 1);
    assert!(g.take_settlement_issues().is_empty());
    let b = battle(&g, op);
    assert!(!b.settled && b.stage == 1, "the operation redeployed");
    let (new_level, paid) = ledger(&g);
    assert!(
        new_level > level,
        "the record-less credit levelled the parent"
    );
    assert_eq!(paid, new_level, "the pass settled the ledger");
    let owed: u64 = (level.max(1) + 1..=new_level).map(level_grant).sum();
    assert!(owed > 0);
    assert_eq!(g.campaign().coins, coins + earned + owed);
    assert_eq!(g.campaign().run_history.len(), runs, "no record");
    assert!(
        b.game()
            .last_result()
            .is_none_or(|report| report.progression_rewards.is_empty()),
        "the redeploy replaced the report"
    );

    let coins = g.campaign().coins;
    advance(&mut g, 3);
    let mut g = reload(&g);
    advance(&mut g, 3);
    assert_eq!(g.campaign().coins, coins, "paid exactly once");
    assert_eq!(ledger(&g), (new_level, new_level));
}
