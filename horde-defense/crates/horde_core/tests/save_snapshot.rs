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

// The conditional remedy (W1-50): the save clone's gate failed in the
// playtest profile, so the largest measured component at 1,000 items, the run
// history, is shared copy-on-write between the game and its snapshots. The
// save bytes must not move.

const FIXTURE_V6: &str = include_str!("fixtures/save_v6.json");
const FIXTURE_V7: &str = include_str!("fixtures/save_v7.json");
/// FNV-1a of the migrated v6 payload, cut before the remedy with the history
/// a plain `Vec`: the shared history serializes to the same bytes.
const V6_PAYLOAD_FNV: u64 = 11_732_925_792_674_115_074;

/// The payload text checked in inside `save_v7.json`'s envelope.
fn v7_fixture_payload() -> String {
    let envelope: serde_json::Value = serde_json::from_str(FIXTURE_V7).expect("v7 envelope");
    envelope["payload"]
        .as_str()
        .expect("the envelope's payload is text")
        .to_owned()
}

/// The validated save payload of `g`, and that payload loaded and saved again.
fn payload_round_trip(g: &Game) -> (String, String) {
    let payload = g.to_save_json().expect("saveable");
    let again: Game = serde_json::from_str(&payload).expect("payload loads");
    (payload, again.to_save_json().expect("still saveable"))
}

#[test]
fn consecutive_snapshots_share_unchanged_component() {
    let mut g = Game::decode_save_generation(FIXTURE_V6.as_bytes()).expect("v6 fixture");
    let history = |g: &Game| g.campaign().run_history.as_ptr();
    assert!(!g.campaign().run_history.is_empty());
    let first = g.save_snapshot();
    let second = g.save_snapshot();
    assert_eq!(
        history(&first),
        history(&g),
        "the snapshot shares the history"
    );
    assert_eq!(
        history(&second),
        history(&first),
        "and so does the next one"
    );

    // Recording a run copies on write: the earlier snapshot keeps its records.
    let records = g.campaign().run_history.len();
    if g.attempt().is_none() {
        g.deploy(0, Mode::Npc).unwrap();
    }
    g.withdraw();
    assert_eq!(g.campaign().run_history.len(), records + 1);
    assert_eq!(first.campaign().run_history.len(), records);
    let third = g.save_snapshot();
    assert_ne!(history(&third), history(&first));
    assert_eq!(history(&third), history(&g));
}

/// W1-50 review: with no snapshot in flight, recording a run appends to the
/// history in place instead of copying every earlier record.
#[test]
fn recording_a_run_without_a_snapshot_appends_in_place() {
    let mut g = Game::decode_save_generation(FIXTURE_V6.as_bytes()).expect("v6 fixture");
    let shared = |g: &Game| std::sync::Arc::as_ptr(&g.campaign().run_history) as *const u8;
    let before = shared(&g);
    let records = g.campaign().run_history.len();
    if g.attempt().is_none() {
        g.deploy(0, Mode::Npc).unwrap();
    }
    g.withdraw();
    assert_eq!(g.campaign().run_history.len(), records + 1);
    assert_eq!(shared(&g), before, "the unshared history was not copied");
    // A snapshot in flight still keeps its own records.
    let snapshot = g.save_snapshot();
    g.deploy(0, Mode::Npc).unwrap();
    g.withdraw();
    assert_eq!(snapshot.campaign().run_history.len(), records + 1);
    assert_eq!(g.campaign().run_history.len(), records + 2);
    assert_ne!(shared(&g), shared(&snapshot));
}

/// R58 closing: the 20,000-item stash (default auto-sell after about 150
/// runs) put `save_snapshot` at 2.7 ms p99, its stash copy 1.8 ms of that,
/// one definition-ID string allocation per item. A snapshot's items share
/// their definition IDs with the live stash instead.
#[test]
fn snapshot_items_share_their_definition_ids() {
    let g = Game::decode_save_generation(FIXTURE_V7.as_bytes()).expect("v7 fixture");
    let snapshot = g.save_snapshot();
    let ids = |g: &Game| -> Vec<*const u8> {
        g.campaign()
            .items
            .iter()
            .map(|item| item.definition.as_str().as_ptr())
            .collect()
    };
    assert!(!g.campaign().items.is_empty());
    assert_eq!(ids(&snapshot), ids(&g), "no item copies its definition ID");
}

#[test]
fn save_v6_and_v7_fixtures_round_trip_byte_identical() {
    // v6: the checked-in fixture, migrated, against its pinned FNV-1a. v7:
    // the checked-in save_v7.json (W1-63), against the payload its own
    // envelope holds. The synthetic stress fixture is in no digest.
    let v6 = Game::decode_save_generation(FIXTURE_V6.as_bytes()).expect("v6 fixture");
    let v7 = Game::decode_save_generation(FIXTURE_V7.as_bytes()).expect("v7 fixture");
    let v7_pinned = support::fnv1a(v7_fixture_payload().as_bytes());
    for (name, g, pinned) in [("v6", v6, V6_PAYLOAD_FNV), ("v7", v7, v7_pinned)] {
        let (payload, again) = payload_round_trip(&g);
        assert_eq!(payload, again, "{name} round-trips byte-identical");
        assert_eq!(
            serde_json::to_string(&g.save_snapshot()).unwrap(),
            payload,
            "{name} snapshot bytes"
        );
        assert_eq!(
            support::fnv1a(payload.as_bytes()),
            pinned,
            "{name} payload bytes moved"
        );
    }
}
