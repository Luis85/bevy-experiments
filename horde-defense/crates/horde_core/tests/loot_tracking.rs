//! Drop tracking (W2-06, loot-economy L1; schema v7 §3.3 and §3.4, rulings
//! E5 and E11). Every attempt deployed from wave 2 on freezes
//! `drop_tier = Some(max(level + 1, unlocked).clamp(1, max_drop_tier()))`
//! at deploy, so None marks an attempt deployed before v7 (the legacy drop
//! path, pinned by `V6_DROP_TRACE_FNV` in `save_migration.rs`). A root
//! attempt on the Some path lists every item it dropped in `drop_ids`, past
//! the 128 capped receipts, and its report carries `drop_ids` and
//! `loot_by_quality`. Expedition attempts never record drops.
mod support;
use horde_core::{
    command::Command,
    content::{Mode, content},
    loot::max_drop_tier,
    quality::{MAX_LOOT_EVENTS, Quality},
    sim::{Game, Input, SAVE_VERSION},
};
use support::{legacy_game, strategic_game};

/// The saved game loads (`validate_game` accepts it).
fn reloads(g: &Game, what: &str) {
    if let Err(error) = Game::from_payload(serde_json::to_value(g).unwrap(), SAVE_VERSION) {
        panic!("{what} does not reload: {error:?}");
    }
}

/// Fail-first: a run's `drop_ids` hold every drop, ascending, after the
/// receipts in `loot_events` hit their 128 cap; the report keeps the full
/// list and the quality counts.
#[test]
fn drop_ids_complete_beyond_128_receipts() {
    let mut g = support::game(Mode::Personal);
    let drops = MAX_LOOT_EVENTS + 12;
    let first_new_id = g.campaign().next_item_id;
    for _ in 0..drops {
        support::enemy(&mut g, 1, 10.0, false);
        let a = g.attempt_mut().unwrap();
        // The staged hostile is already down, and its kill is the
        // twentieth: today's every-20th-kill rule makes it drop.
        a.enemies.last_mut().unwrap().hp = 0;
        a.kills += 19 - a.kills % 20;
        g.tick(Input::default());
    }
    let a = g.attempt().expect("the defender survives the staged kills");
    assert_eq!(a.loot as usize, drops, "every staged kill dropped");
    assert_eq!(a.loot_events.len(), MAX_LOOT_EVENTS, "receipts are capped");
    let all: Vec<u64> = (first_new_id..g.campaign().next_item_id).collect();
    assert_eq!(all.len(), drops);
    assert_eq!(a.drop_ids, all, "drop_ids list every drop, ascending");
    assert!(
        a.loot_events
            .iter()
            .all(|e| a.drop_ids.contains(&e.item_id)),
        "every receipt's item is listed"
    );
    reloads(&g, "the live attempt");

    let counts = a.loot_quality_counts.clone();
    g.apply(&Command::Withdraw).unwrap();
    let report = g.last_result().unwrap();
    assert_eq!(report.drop_ids, all, "the report keeps every drop");
    assert_eq!(report.loot_by_quality, counts);
    assert_eq!(
        Quality::ALL
            .iter()
            .map(|q| report.loot_by_quality[*q] as usize)
            .sum::<usize>(),
        drops
    );
    reloads(&g, "the report");
}

/// Fail-first: every deploy path (a local stage in either mode, a country
/// stage and an expedition's worker game) stores a drop tier in
/// `1..=max_drop_tier()`; only the root attempt records drops.
#[test]
fn new_attempts_always_store_some_drop_tier() {
    let tier = |g: &Game| g.attempt().unwrap().drop_tier;
    let valid = |t: Option<u8>| t.is_some_and(|t| (1..=max_drop_tier()).contains(&t));
    assert_eq!(max_drop_tier(), 4);
    assert_eq!(max_drop_tier(), content().max_drop_tier());

    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = legacy_game();
        g.campaign_mut().attempts = 1;
        g.deploy(0, mode).unwrap();
        assert_eq!(tier(&g), Some(1), "{mode:?} local stage 1");
        g.apply(&Command::Withdraw).unwrap();
        g.retry().unwrap();
        assert_eq!(tier(&g), Some(1), "{mode:?} retry");
    }

    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let recruit = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    let battle = g.deploy_survivor(recruit, &country, 0).unwrap();
    let worker = g.active_battle(battle).unwrap().game().attempt().unwrap();
    assert!(valid(worker.drop_tier), "the expedition's attempt");
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    assert!(valid(tier(&g)), "a country stage");
    g.set_paused(false);

    // Ticked with expedition drops: the worker never records one. Its
    // first tick defeats a staged hostile whose kill is the twentieth, so
    // the worker banks at least that drop.
    let w = g
        .battles_mut()
        .iter_mut()
        .find(|b| b.id == battle)
        .unwrap()
        .game_mut();
    support::enemy(w, 1, 10.0, false);
    let a = w.attempt_mut().unwrap();
    let staged = a.enemies.last_mut().unwrap();
    staged.hp = 0;
    staged.injury.unconscious = true;
    a.kills += 19 - a.kills % 20;
    let mut worker_drops = 0;
    for _ in 0..3600 {
        g.tick(Input::default());
        let worker = g.battles().iter().find(|b| b.id == battle).unwrap().game();
        if let Some(a) = worker.attempt() {
            worker_drops = worker_drops.max(a.loot);
            assert!(a.drop_ids.is_empty(), "an expedition never records drops");
        }
        if g.attempt().is_none() {
            break;
        }
    }
    assert!(
        worker_drops > 0,
        "the worker banked a drop it did not record"
    );
    reloads(&g, "the strategic game");
}

/// Fail-first: a late country stage wraps to an early encounter template
/// (`stage % 5`), but its drop tier follows the campaign's unlocked stages,
/// clamped to the highest droppable tier.
#[test]
fn late_country_stage_sets_unlocked_drop_tier() {
    let mut g = legacy_game();
    g.campaign_mut().choose_start_country("USA").unwrap();
    g.campaign_mut().world.record_clear(0).unwrap();
    g.campaign_mut().first_clear(0, Mode::Personal);
    assert_eq!(g.campaign().unlocked(), 2);
    g.deploy_country_stage(1, Mode::Personal).unwrap();
    assert_eq!(g.attempt().unwrap().level, 1);
    assert_eq!(g.attempt().unwrap().drop_tier, Some(2), "level 1 + 1");
    g.apply(&Command::Withdraw).unwrap();
    g.deploy(0, Mode::Personal).unwrap();
    assert_eq!(
        g.attempt().unwrap().drop_tier,
        Some(2),
        "two stages unlocked"
    );
    g.apply(&Command::Withdraw).unwrap();

    for stage in 1..6 {
        g.campaign_mut().world.record_clear(stage).unwrap();
        g.campaign_mut().first_clear(stage % 5, Mode::Personal);
    }
    assert_eq!(g.campaign().unlocked(), 5);
    g.deploy_country_stage(6, Mode::Personal).unwrap();
    let a = g.attempt().unwrap();
    assert_eq!((a.stage, a.level), (6, 1), "stage 7 replays template 2");
    assert_eq!(
        a.drop_tier,
        Some(max_drop_tier()),
        "unlocked 5, clamped to 4"
    );
    reloads(&g, "the late stage");
}
