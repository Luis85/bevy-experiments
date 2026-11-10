//! Expedition settlement writers (W2-15, Ruling D10; design-campaign-defenders
//! 1.9; acceptance G3.8 and G3.9): `settle_parallel` creates, updates and
//! finalises each expedition's saved `ParallelBattle.summary`; pruning drops
//! legacy and acknowledged battles first and folds an unacknowledged outcome
//! into `StrategyState.folded_outcomes`; `Game::unacknowledged_outcomes`
//! lists one notice per settled expedition until `AcknowledgeOutcomes`; and
//! the shared `world::liberation_line` wording.
mod support;
use horde_core::{
    command::{Command, CommandOutcome},
    rules::Rng,
    sim::{ExpeditionSummary, Game, Outcome, ParallelBattle, SAVE_VERSION},
    strategy::RecruitStatus,
    world::{liberation_line, world},
};
use support::{advance, enemy, strategic_game};

const FIXTURE_V6: &str = include_str!("fixtures/save_v6.json");

/// A strategic game (four liberated countries) with `count` Ready recruits
/// from its starting country.
fn game_with_recruits(count: usize) -> (Game, String, Vec<u64>) {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let ids = (0..count)
        .map(|_| {
            g.with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
                .unwrap()
        })
        .collect();
    (g, country, ids)
}

fn battle(g: &Game, id: u64) -> &ParallelBattle {
    g.battles().iter().find(|b| b.id == id).unwrap()
}

/// Ends the battle's current stage in victory on the next tick.
fn win_stage(g: &mut Game, id: u64) {
    let b = g.battles_mut().iter_mut().find(|b| b.id == id).unwrap();
    enemy(b.game_mut(), 5, 20.0, true);
    let a = b.game_mut().attempt_mut().unwrap();
    a.boss_spawned = true;
    a.enemies[0].injury.unconscious = true;
    advance(g, 1);
}

fn reload(g: &Game) -> Game {
    Game::from_payload(serde_json::to_value(g).unwrap(), SAVE_VERSION)
        .unwrap_or_else(|e| panic!("save reloads: {e}"))
}

/// The ids of the battles `unacknowledged_outcomes` announces, in order.
fn announced(g: &Game) -> Vec<u64> {
    g.unacknowledged_outcomes()
        .notices
        .iter()
        .map(|n| n.battle)
        .collect()
}

fn acknowledge(g: &mut Game, battles: Vec<u64>, folded: u32) {
    assert_eq!(
        g.apply(&Command::AcknowledgeOutcomes { battles, folded }),
        Ok(CommandOutcome::Applied)
    );
}

/// A country the strategic game can enter, has not liberated, and that takes
/// more than one stage.
fn operation_country(g: &Game) -> &'static horde_core::world::Country {
    world()
        .countries
        .iter()
        .find(|c| {
            g.campaign().world.can_enter(&c.id)
                && !g.campaign().world.cleared.contains(&c.id)
                && c.level_count() > 1
        })
        .unwrap()
}

#[test]
fn liberation_line_names_the_country_and_counts_the_fronts() {
    assert_eq!(liberation_line("Belgium", 0), "Belgium liberated.");
    assert_eq!(
        liberation_line("Belgium", 1),
        "Belgium liberated · 1 new front open on the map."
    );
    assert_eq!(
        liberation_line("Belgium", 2),
        "Belgium liberated · 2 new fronts open on the map."
    );
}

#[test]
fn each_settled_expedition_yields_exactly_one_notice_across_save_reload() {
    let (mut g, country, ids) = game_with_recruits(2);
    let won = g.deploy_survivor(ids[0], &country, 0).unwrap();
    let recalled = g.deploy_survivor(ids[1], &country, 0).unwrap();
    // A live expedition is never announced.
    advance(&mut g, 3);
    assert_eq!(announced(&g), Vec::<u64>::new());
    assert_eq!(g.unacknowledged_outcomes().folded, 0);

    win_stage(&mut g, won);
    g.recall_survivor(recalled).unwrap();
    assert!(g.take_settlement_issues().is_empty());
    assert_eq!(announced(&g), [won, recalled]);

    let notices = g.unacknowledged_outcomes();
    let country_name = &world().country(&country).unwrap().name;
    for (notice, id, outcome) in [
        (&notices.notices[0], ids[0], Outcome::Victory),
        (&notices.notices[1], ids[1], Outcome::Withdrawal),
    ] {
        let b = battle(&g, notice.battle);
        assert_eq!(notice.recruit_id, id);
        assert_eq!(notice.recruit_name, b.game().campaign().npc.name);
        assert_eq!(notice.country_id, country);
        assert_eq!(&notice.country_name, country_name);
        let summary = &notice.summary;
        assert_eq!(summary.outcome, Some(outcome));
        assert_eq!(
            summary.stages_cleared,
            u32::from(outcome == Outcome::Victory)
        );
        // An already liberated country opens no front.
        assert!(!summary.liberated);
        assert!(summary.fronts.is_empty());
        assert_eq!(summary.coins, b.game().campaign().coins);
        assert!(!summary.acknowledged);
        assert_eq!(b.summary.as_ref(), Some(summary));
    }

    // Time, another settlement pass and a save/load announce nothing new.
    advance(&mut g, 10);
    let mut g = reload(&g);
    advance(&mut g, 10);
    assert_eq!(g.unacknowledged_outcomes(), notices);

    acknowledge(&mut g, vec![won], 0);
    assert_eq!(announced(&g), [recalled]);
    assert!(battle(&g, won).summary.as_ref().unwrap().acknowledged);
    let mut g = reload(&g);
    assert_eq!(announced(&g), [recalled]);

    // Idempotent: unknown, repeated and already acknowledged ids change nothing.
    let before = serde_json::to_value(&g).unwrap();
    acknowledge(&mut g, vec![won, 9_999], 0);
    assert_eq!(serde_json::to_value(&g).unwrap(), before);

    acknowledge(&mut g, vec![recalled, recalled], 0);
    assert_eq!(announced(&g), Vec::<u64>::new());
    assert_eq!(announced(&reload(&g)), Vec::<u64>::new());
}

#[test]
fn acknowledge_outcomes_ignores_live_expeditions() {
    let (mut g, country, ids) = game_with_recruits(1);
    let live = g.deploy_survivor(ids[0], &country, 0).unwrap();
    advance(&mut g, 2);
    let before = serde_json::to_value(&g).unwrap();
    acknowledge(&mut g, vec![live], 0);
    assert_eq!(serde_json::to_value(&g).unwrap(), before);
    g.recall_survivor(live).unwrap();
    assert_eq!(announced(&g), [live]);
}

#[test]
fn country_operation_notice_appears_once_after_final_stage_with_liberation_and_fronts() {
    let (mut g, _, ids) = game_with_recruits(1);
    let country = operation_country(&g);
    let stages = country.level_count();
    let operation = g.deploy_country_operation(ids[0], &country.id).unwrap();
    // Stage n drops n + 1 items in the worker game, so the summary's
    // loot_count is checked against what settlement really stored.
    let mut dropped = 0u32;
    let parent_items = g.campaign().items.len();
    let natural = |g: &Game| -> u32 {
        g.campaign()
            .run_history
            .iter()
            .filter(|r| r.recruit_id == Some(ids[0]))
            .map(|r| r.loot)
            .sum()
    };
    for stage in 0..stages {
        // What the earlier stages stored (this stage's drops settle on its
        // victory tick).
        let stored = dropped + natural(&g);
        let b = g
            .battles_mut()
            .iter_mut()
            .find(|b| b.id == operation)
            .unwrap();
        for _ in 0..=stage {
            b.game_mut().campaign_mut().add_item(4).unwrap();
            dropped += 1;
        }
        let b = battle(&g, operation);
        assert_eq!((b.stage, b.settled), (stage, false));
        assert_eq!(announced(&g), Vec::<u64>::new(), "stage {stage}");
        // The running count is saved: a reload mid-operation keeps it.
        g = reload(&g);
        let (cleared, loot) = battle(&g, operation)
            .summary
            .as_ref()
            .map_or((0, 0), |s| (s.stages_cleared, s.loot_count));
        assert_eq!(cleared, stage as u32, "stage {stage}");
        assert_eq!(loot, stored, "stage {stage}: loot_count");
        win_stage(&mut g, operation);
        assert!(g.take_settlement_issues().is_empty(), "stage {stage}");
    }
    assert!(battle(&g, operation).settled);
    assert!(g.campaign().world.cleared.contains(&country.id));

    let fronts: Vec<String> = g
        .campaign()
        .world
        .fronts_opened_by(&country.id)
        .iter()
        .map(|c| c.id.clone())
        .collect();
    assert!(!fronts.is_empty(), "{} opens a front", country.id);
    // Each stage's own drops (its run record's `loot`) on top of the
    // injected ones.
    let records = g
        .campaign()
        .run_history
        .iter()
        .filter(|r| r.recruit_id == Some(ids[0]))
        .count();
    assert_eq!(records, stages);
    let loot = dropped + natural(&g);
    // Auto-sale is off: every stored drop is still in the parent's armory
    // (beside what the operation hands back when it settles).
    assert!(g.campaign().items.len() - parent_items >= loot as usize);
    let notices = g.unacknowledged_outcomes();
    assert_eq!(notices.notices.len(), 1);
    let notice = &notices.notices[0];
    assert_eq!(notice.battle, operation);
    assert_eq!(notice.country_id, country.id);
    assert_eq!(
        notice.summary,
        ExpeditionSummary {
            outcome: Some(Outcome::Victory),
            stages_cleared: stages as u32,
            liberated: true,
            fronts: fronts.clone(),
            coins: battle(&g, operation).game().campaign().coins,
            loot_count: loot,
            acknowledged: false,
        }
    );
    assert_eq!(
        liberation_line(&notice.country_name, notice.summary.fronts.len()),
        format!(
            "{} liberated · {} new front{} open on the map.",
            country.name,
            fronts.len(),
            if fronts.len() == 1 { "" } else { "s" }
        )
    );

    advance(&mut g, 5);
    let mut g = reload(&g);
    assert_eq!(g.unacknowledged_outcomes(), notices);
    acknowledge(&mut g, vec![operation], 0);
    assert_eq!(announced(&reload(&g)), Vec::<u64>::new());
}

/// Fills the battle list to its 32-battle cap with settled copies of a
/// recalled expedition (each an unacknowledged Withdrawal), oldest first,
/// and returns their ids. The recruits stay Ready.
fn fill_with_settled(g: &mut Game, country: &str, recruit: u64) -> Vec<u64> {
    let template = g.deploy_survivor(recruit, country, 0).unwrap();
    g.recall_survivor(template).unwrap();
    let mut settled = battle(g, template).clone();
    assert!(settled.settled && settled.summary.is_some());
    let battles = g.battles_mut();
    battles.clear();
    for n in 0..32 {
        settled.id = 1_000 + n;
        battles.push(settled.clone());
    }
    battles.iter().map(|b| b.id).collect()
}

#[test]
fn pruning_folds_unacknowledged_outcomes_and_conserves_the_count() {
    let (mut g, country, ids) = game_with_recruits(3);
    let settled = fill_with_settled(&mut g, &country, ids[0]);
    let total = |g: &Game| {
        let notices = g.unacknowledged_outcomes();
        notices.notices.len() + notices.folded as usize
    };
    assert_eq!(announced(&g), settled);
    assert_eq!(total(&g), 32);

    for (n, recruit) in ids.iter().enumerate() {
        g.deploy_survivor(*recruit, &country, 0).unwrap();
        assert_eq!(g.battles().len(), 32);
        // Oldest first: each deploy folds the oldest unacknowledged outcome.
        assert_eq!(announced(&g), settled[n + 1..]);
        assert_eq!(g.strategy().folded_outcomes, n as u32 + 1);
        assert_eq!(total(&g), 32, "deploy {n}");
        g = reload(&g);
        assert_eq!(total(&g), 32, "deploy {n} reloaded");
    }

    // The folded count is acknowledged by number and saturates.
    acknowledge(&mut g, Vec::new(), 2);
    assert_eq!(g.unacknowledged_outcomes().folded, 1);
    acknowledge(&mut g, settled.clone(), 5);
    let notices = reload(&g).unacknowledged_outcomes();
    assert_eq!((notices.notices.len(), notices.folded), (0, 0));
}

#[test]
fn pruning_prefers_acknowledged_and_legacy_battles() {
    let (mut g, country, ids) = game_with_recruits(3);
    let settled = fill_with_settled(&mut g, &country, ids[0]);
    let (legacy, seen) = (settled[5], settled[10]);
    g.battles_mut()[5].summary = None;
    acknowledge(&mut g, vec![seen], 0);
    let unacknowledged: Vec<u64> = settled
        .iter()
        .copied()
        .filter(|id| ![legacy, seen].contains(id))
        .collect();
    assert_eq!(announced(&g), unacknowledged);

    // The legacy battle goes first even though older outcomes are pending.
    g.deploy_survivor(ids[0], &country, 0).unwrap();
    assert!(g.battles().iter().all(|b| b.id != legacy));
    assert_eq!(announced(&g), unacknowledged);
    assert_eq!(g.strategy().folded_outcomes, 0);
    // Then the acknowledged one.
    g.deploy_survivor(ids[1], &country, 0).unwrap();
    assert!(g.battles().iter().all(|b| b.id != seen));
    assert_eq!(announced(&g), unacknowledged);
    assert_eq!(g.strategy().folded_outcomes, 0);
    // Only then is the oldest unacknowledged outcome folded.
    g.deploy_survivor(ids[2], &country, 0).unwrap();
    assert_eq!(announced(&g), unacknowledged[1..]);
    assert_eq!(g.strategy().folded_outcomes, 1);
    // Live expeditions are never pruned.
    assert_eq!(g.battles().iter().filter(|b| !b.settled).count(), 3);
}

/// Every parent-side ledger a settlement pass can credit, plus the battles.
fn settlement_ledgers(g: &Game) -> serde_json::Value {
    let c = g.campaign();
    serde_json::json!({
        "coins": c.coins,
        "victories": c.victories,
        "rank": c.progression_rank(),
        "runs": serde_json::to_value(&*c.run_history).unwrap(),
        "items": serde_json::to_value(&c.items).unwrap(),
        "world": serde_json::to_value(&c.world).unwrap(),
        "reserved": serde_json::to_value(&c.reserved_items).unwrap(),
        "roster": serde_json::to_value(&g.strategy().roster).unwrap(),
        "folded": g.strategy().folded_outcomes,
        "battles": serde_json::to_value(g.battles()).unwrap(),
    })
}

#[test]
fn settle_parallel_twice_is_a_no_op() {
    let (mut g, country, ids) = game_with_recruits(1);
    let b = g.deploy_survivor(ids[0], &country, 0).unwrap();
    win_stage(&mut g, b);
    assert!(battle(&g, b).settled);
    let settled = settlement_ledgers(&g);
    // Later passes (ticks, a recall of the settled battle, a reload) skip it.
    advance(&mut g, 5);
    assert!(g.recall_survivor(b).is_err());
    let mut g = reload(&g);
    advance(&mut g, 5);
    assert_eq!(settlement_ledgers(&g), settled);
    assert!(g.take_settlement_issues().is_empty());

    // The record-less path: an expedition whose finished victory is already
    // in the parent's history (committed with its credit by an earlier pass)
    // but which is not yet marked settled, a valid save, is not credited again.
    let victories = g.campaign().victories;
    let runs = g.campaign().run_history.len();
    let loadout = battle(&g, b).loadout;
    let entry = g.battles_mut().iter_mut().find(|x| x.id == b).unwrap();
    entry.settled = false;
    if let Some(summary) = entry.summary.as_mut() {
        summary.outcome = None;
    }
    let recruit = g
        .strategy_mut()
        .roster
        .iter_mut()
        .find(|r| r.id == ids[0])
        .unwrap();
    recruit.status = RecruitStatus::Deployed;
    g.campaign_mut().reserve_external(loadout).unwrap();
    let mut g = reload(&g);
    advance(&mut g, 1);
    assert!(g.take_settlement_issues().is_empty());
    assert!(battle(&g, b).settled);
    assert_eq!(g.campaign().victories, victories);
    assert_eq!(g.campaign().run_history.len(), runs);
    assert_eq!(settlement_ledgers(&g), settled);
}

/// Acceptance G3.8 (design-campaign-defenders 1.9 "Worker credit"). Pin: a
/// settled worker victory writes exactly one RunRecord naming the recruit,
/// and the parent's victory and threat credit lands once, across repeated
/// settlement passes and a save reload.
#[test]
fn worker_victory_credits_recruit_and_threat_once() {
    let (mut g, country, ids) = game_with_recruits(1);
    let (victories, rank) = (g.campaign().victories, g.campaign().progression_rank());
    let b = g.deploy_survivor(ids[0], &country, 0).unwrap();
    win_stage(&mut g, b);
    let check = |g: &Game| {
        let records: Vec<_> = g
            .campaign()
            .run_history
            .iter()
            .filter(|r| r.worker_id == Some(b))
            .collect();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].recruit_id, Some(ids[0]));
        assert_eq!(records[0].outcome, Outcome::Victory);
        assert_eq!(g.campaign().victories, victories + 1);
        assert!(g.campaign().progression_rank() >= rank);
    };
    check(&g);
    let credited_rank = g.campaign().progression_rank();
    advance(&mut g, 3);
    check(&g);
    let mut g = reload(&g);
    advance(&mut g, 3);
    check(&g);
    assert_eq!(g.campaign().progression_rank(), credited_rank);
}

/// Pin (acceptance G3.9): battles settled before v7 carry no summary and are
/// never announced; the v6 in-flight expedition is announced once, with its
/// counts starting at load.
#[test]
fn v6_settled_battles_produce_no_notice_burst() {
    let g = Game::decode_save_generation(FIXTURE_V6.as_bytes()).expect("v6 fixture");
    let settled: Vec<u64> = g
        .battles()
        .iter()
        .filter(|b| b.settled)
        .map(|b| b.id)
        .collect();
    let live: Vec<u64> = g
        .battles()
        .iter()
        .filter(|b| !b.settled)
        .map(|b| b.id)
        .collect();
    assert!(!settled.is_empty() && !live.is_empty());
    assert!(
        g.battles()
            .iter()
            .all(|b| b.summary.is_none() && b.settled != live.contains(&b.id))
    );
    assert_eq!(g.unacknowledged_outcomes().notices, Vec::new());
    let mut g = reload(&g);
    assert_eq!(announced(&g), Vec::<u64>::new());
    for id in &live {
        g.recall_survivor(*id).unwrap();
    }
    assert_eq!(announced(&g), live);
    assert!(settled.iter().all(|id| battle(&g, *id).summary.is_none()));
}

/// 200 random interleavings of ticks, stage wins, recalls (live and already
/// settled) and save reloads over three expeditions: every expedition
/// settles once, its notice matches its records and the parent's credit, and
/// no outcome is announced twice.
#[test]
fn random_tick_recall_interleavings_settle_each_battle_once() {
    for case in 0..200u64 {
        let mut rng = Rng::new(0x5E77_1E00 + case);
        let (mut g, country, ids) = game_with_recruits(3);
        let operation = operation_country(&g).id.clone();
        let victories = g.campaign().victories;
        let mut battles = Vec::new();
        for (n, id) in ids.iter().enumerate() {
            let b = if n == 0 && rng.below(2) == 0 {
                g.deploy_country_operation(*id, &operation).unwrap()
            } else {
                g.deploy_survivor(*id, &country, 0).unwrap()
            };
            battles.push(b);
        }
        for _ in 0..12 {
            let b = battles[rng.below(3) as usize];
            match rng.below(5) {
                0 => advance(&mut g, 1 + rng.below(4) as u32),
                1 if !battle(&g, b).settled => win_stage(&mut g, b),
                2 => {
                    let live = !battle(&g, b).settled;
                    assert_eq!(g.recall_survivor(b).is_ok(), live, "case {case}");
                }
                3 => g = reload(&g),
                _ => advance(&mut g, 1),
            }
            assert_eq!(g.take_settlement_issues(), Vec::new(), "case {case}");
            let notices = announced(&g);
            let settled: Vec<u64> = battles
                .iter()
                .copied()
                .filter(|b| battle(&g, *b).settled)
                .collect();
            assert_eq!(notices, settled, "case {case}");
        }
        for b in &battles {
            if !battle(&g, *b).settled {
                g.recall_survivor(*b).unwrap();
            }
        }
        advance(&mut g, 2);
        assert_eq!(g.take_settlement_issues(), Vec::new(), "case {case}");
        let g = reload(&g);
        assert_eq!(announced(&g), battles, "case {case}");
        let mut credited = 0;
        for notice in g.unacknowledged_outcomes().notices {
            let wins = g
                .campaign()
                .run_history
                .iter()
                .filter(|r| r.worker_id == Some(notice.battle) && r.outcome == Outcome::Victory)
                .count() as u32;
            assert_eq!(notice.summary.stages_cleared, wins, "case {case}");
            let b = battle(&g, notice.battle);
            assert_eq!(
                notice.summary.coins,
                b.game().campaign().coins,
                "case {case}"
            );
            assert_eq!(
                notice.summary.outcome,
                b.game().last_result().map(|r| r.outcome),
                "case {case}"
            );
            credited += wins;
        }
        assert_eq!(
            g.campaign().victories,
            victories + u64::from(credited),
            "case {case}"
        );
    }
}
