//! `Game::save_snapshot` (W1-50): the copy the runtime hands to its storage
//! worker. It leaves out the transient, never-saved state (the typed event
//! outbox, the attempt's queued records and settlement issues, in the root
//! game and in every expedition's worker game) and serializes to exactly the
//! bytes of the game it was taken from.
mod support;
use horde_core::content::{EnemyKind, Mode};
use horde_core::sim::{Game, Input};
use support::{enemy, strategic_game};

/// Transient records still waiting in `g`, without draining `g` itself.
fn pending_events(g: &Game) -> usize {
    g.clone().drain_events().count()
}
fn pending_issues(g: &Game) -> usize {
    g.clone().take_settlement_issues().len()
}

/// A game with a deployed main field and one expedition, ticked without
/// draining, so the root outbox, the root attempt's queue, the worker
/// game's outbox and the settlement issues all hold something.
fn game_with_transients() -> Game {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let recruit = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    g.deploy_survivor(recruit, &country, 0).unwrap();
    g.deploy(0, Mode::Npc).unwrap();
    g.set_paused(false);
    enemy(&mut g, 5, 12.0, false);
    for _ in 0..30 {
        g.tick(Input::default());
    }
    // An exhausted item identity space makes the next settlement fail loudly.
    g.campaign_mut().next_item_id = u64::MAX;
    g.battles_mut()[0]
        .game_mut()
        .campaign_mut()
        .add_item_with_quality(4, 9, None)
        .unwrap();
    g.tick(Input::default());
    // A worker tick outside the root forward leaves the worker's own outbox full.
    let worker = g.battles_mut()[0].game_mut();
    let mut a = worker.attempt_slot_mut().take().unwrap();
    worker.spawn_enemy(&mut a, EnemyKind::ALL[0], 3, false);
    *worker.attempt_slot_mut() = Some(a);
    for _ in 0..30 {
        worker.tick(Input::default());
    }
    g
}

#[test]
fn save_snapshot_skips_transient_outbox_and_equals_serialized_state() {
    let g = game_with_transients();
    assert!(pending_events(&g) > 0, "the root outbox holds records");
    assert!(pending_issues(&g) > 0, "a settlement issue is pending");
    assert!(
        pending_events(g.battles()[0].game()) > 0,
        "the worker outbox holds records"
    );

    let snapshot = g.save_snapshot();
    assert_eq!(pending_events(&snapshot), 0, "no root records");
    assert_eq!(pending_issues(&snapshot), 0, "no settlement issues");
    assert_eq!(snapshot.battles().len(), g.battles().len());
    for battle in snapshot.battles() {
        assert_eq!(pending_events(battle.game()), 0, "no worker records");
        assert_eq!(pending_issues(battle.game()), 0, "no worker issues");
    }
    assert_eq!(
        serde_json::to_vec(&snapshot).unwrap(),
        serde_json::to_vec(&g).unwrap(),
        "the snapshot serializes to the game's bytes"
    );
    assert_eq!(snapshot.state_digest(), g.state_digest());
    assert_eq!(
        snapshot.to_save_json().unwrap(),
        g.to_save_json().unwrap(),
        "the validated save payload is unchanged"
    );
    // The original keeps its transients for the frontend.
    assert!(pending_events(&g) > 0);
    assert!(pending_issues(&g) > 0);
}
