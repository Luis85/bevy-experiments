//! The loot and coin cues (A6-14): the runner's LootDrop, the Results
//! LootReveal, and one CoinBatch per auto-sell or sale batch (R9).

use horde_audio::{
    bank::{BankId, UiFamily},
    cue::Cue,
    loot::{
        COIN_BATCH_DELAY_MS, ExpeditionCoins, LOOT_REVEAL_DELAY_MS, LootTracker, ResultsAudioGuard,
        ResultsLoot, UiCue, quality_speed, reveal_gain_db, sale_cue,
    },
    mix::{Bus, db_to_linear, duck, mix},
    pan::{Places, lane_position, positional},
    select::CueGate,
    voices::{Family, family},
};
use horde_core::{
    command::{Command, CommandOutcome},
    content::Mode,
    quality::{LootEvent, Quality},
    sim::{BattleId, Game},
};

fn results(attempt_id: u64, best: Option<Quality>, auto_sold: u32) -> ResultsLoot {
    ResultsLoot {
        attempt_id,
        best,
        auto_sold,
    }
}
/// Every cue the guard emits over `frames` frames, 16 ms apart, on the
/// Results screen of `loot`.
fn shown(
    guard: &mut ResultsAudioGuard,
    loot: ResultsLoot,
    from_ms: u64,
    frames: u64,
) -> Vec<UiCue> {
    (0..frames)
        .flat_map(|frame| guard.observe(Some(loot), from_ms + frame * 16))
        .collect()
}
fn count(cues: &[UiCue], cue: Cue) -> usize {
    cues.iter().filter(|c| c.cue == cue).count()
}

#[test]
fn ui_cue_rows_follow_the_design() {
    assert_eq!(mix(Cue::LootDrop).bus, Bus::Effects);
    assert_eq!(mix(Cue::LootReveal).bus, Bus::Ui);
    assert_eq!(mix(Cue::CoinBatch).bus, Bus::Ui);
    // -6 dB and -8 dB.
    assert!((mix(Cue::LootDrop).gain - db_to_linear(-6.0)).abs() < 0.01);
    assert!((mix(Cue::LootReveal).gain - db_to_linear(-6.0)).abs() < 0.01);
    assert!((mix(Cue::CoinBatch).gain - db_to_linear(-8.0)).abs() < 0.01);
    assert_eq!(mix(Cue::CoinBatch).cooldown_ms, 600);
    assert!(positional(Cue::LootDrop));
    assert!(!positional(Cue::LootReveal) && !positional(Cue::CoinBatch));
    assert_eq!(family(Cue::LootDrop), Some(Family::LootDrop));
    assert_eq!(Family::LootDrop.cap(), 1);
    assert_eq!(Cue::LootReveal.bank(), BankId::Ui(UiFamily::LootReveal));
    assert_eq!(Cue::CoinBatch.bank(), BankId::Ui(UiFamily::CoinBatch));
    for cue in [Cue::LootReveal, Cue::CoinBatch] {
        let duck = duck(cue).expect("ducks the ambience");
        assert_eq!(
            (duck.atmosphere_db, duck.music_db, duck.release_ms),
            (-3.0, 0.0, 400)
        );
    }
    assert!(duck(Cue::LootDrop).is_none());
    // Reduced effects softens the drop, never removes it.
    let drop = mix(Cue::LootDrop);
    assert!(drop.level(true) > 0.0 && drop.level(true) < drop.level(false));
    assert!(drop.cooldown(true) > drop.cooldown(false));
}

#[test]
fn fifty_auto_sold_items_one_coin_batch_on_results_none_on_rerender() {
    let mut guard = ResultsAudioGuard::default();
    let loot = results(7, Some(Quality::Rare), 50);
    // Nothing on the first frame: the reveal waits 150 ms, the coins 450 ms.
    assert_eq!(guard.observe(Some(loot), 1000), vec![]);
    let cues = shown(&mut guard, loot, 1016, 120);
    assert_eq!(count(&cues, Cue::CoinBatch), 1);
    assert_eq!(count(&cues, Cue::LootReveal), 1);
    assert_eq!(cues[0].cue, Cue::LootReveal, "the reveal comes first");
    // Leaving for History and coming back, a later frame, a re-render: none.
    assert_eq!(guard.observe(None, 4000), vec![]);
    assert_eq!(shown(&mut guard, loot, 4016, 120), vec![]);
    // The delays themselves.
    let mut guard = ResultsAudioGuard::default();
    guard.observe(Some(loot), 0);
    assert_eq!(guard.observe(Some(loot), LOOT_REVEAL_DELAY_MS - 1), vec![]);
    let at_reveal = guard.observe(Some(loot), LOOT_REVEAL_DELAY_MS);
    assert_eq!(at_reveal.len(), 1);
    assert_eq!(at_reveal[0].cue, Cue::LootReveal);
    assert_eq!(guard.observe(Some(loot), COIN_BATCH_DELAY_MS - 1), vec![]);
    let at_coins = guard.observe(Some(loot), COIN_BATCH_DELAY_MS);
    assert_eq!(at_coins.len(), 1);
    assert_eq!(at_coins[0].cue, Cue::CoinBatch);
    // The next run's report sounds again.
    let cues = shown(&mut guard, results(8, None, 3), 9000, 120);
    assert_eq!(
        (count(&cues, Cue::CoinBatch), count(&cues, Cue::LootReveal)),
        (1, 0)
    );
}

#[test]
fn rule_off_or_zero_sold_no_coin_batch() {
    let mut guard = ResultsAudioGuard::default();
    let cues = shown(&mut guard, results(1, Some(Quality::Epic), 0), 0, 120);
    assert_eq!(count(&cues, Cue::CoinBatch), 0);
    assert_eq!(count(&cues, Cue::LootReveal), 1);
    let cues = shown(&mut guard, results(2, None, 0), 5000, 120);
    assert_eq!(cues, vec![]);
    // A command that sold nothing.
    assert_eq!(
        sale_cue(&Command::SellUnprotected, &CommandOutcome::Amount(0), false),
        None
    );
    let apply = Command::ApplyAutoSellToStash {
        expect_count: 0,
        expect_coins: 0,
    };
    let sold_none = CommandOutcome::Sold { count: 0, coins: 0 };
    assert_eq!(sale_cue(&apply, &sold_none, false), None);
    // A command that is no sale.
    assert_eq!(
        sale_cue(&Command::BuyNpcPoint, &CommandOutcome::Amount(9), false),
        None
    );
}

#[test]
fn sell_extras_of_50_plays_one_coin_batch() {
    // One command, one cue, however many items it sold.
    assert_eq!(
        sale_cue(
            &Command::SellUnprotected,
            &CommandOutcome::Amount(1250),
            false
        ),
        Some(Cue::CoinBatch)
    );
    let apply = Command::ApplyAutoSellToStash {
        expect_count: 50,
        expect_coins: 1250,
    };
    let sold = CommandOutcome::Sold {
        count: 50,
        coins: 1250,
    };
    assert_eq!(sale_cue(&apply, &sold, false), Some(Cue::CoinBatch));
    assert_eq!(
        sale_cue(&Command::SellItem(3), &CommandOutcome::Amount(25), false),
        Some(Cue::CoinBatch)
    );
    // Two sales inside the gate are one cue.
    let mut gate = CueGate::default();
    assert!(gate.accept(Cue::CoinBatch, 0, false));
    assert!(!gate.accept(Cue::CoinBatch, 599, false));
    assert!(gate.accept(Cue::CoinBatch, 600, false));
}

#[test]
fn no_sale_cue_in_battle() {
    assert_eq!(
        sale_cue(
            &Command::SellUnprotected,
            &CommandOutcome::Amount(1250),
            true
        ),
        None
    );
    assert_eq!(
        sale_cue(&Command::SellItem(3), &CommandOutcome::Amount(25), true),
        None
    );
    // An expedition's batch waits for the battle to end.
    let mut coins = ExpeditionCoins::default();
    assert_eq!(coins.observe(&[], false), 0);
    assert_eq!(coins.observe(&[(4, 12)], true), 0);
    assert_eq!(coins.observe(&[(4, 12)], false), 1);
    assert_eq!(coins.observe(&[(4, 12)], false), 0);
}

/// An expedition that settles during a battle is toasted and acknowledged
/// while the battle is paused (W3-25): its batch still plays once the
/// battle ends, and only once.
#[test]
fn expedition_batch_acknowledged_in_battle_plays_after_it() {
    let mut coins = ExpeditionCoins::default();
    assert_eq!(coins.observe(&[], false), 0);
    // Settles in the battle, then its notice is acknowledged in the pause.
    assert_eq!(coins.observe(&[(4, 12), (5, 0)], true), 0);
    assert_eq!(coins.observe(&[], true), 0);
    assert_eq!(coins.observe(&[], false), 1);
    assert_eq!(coins.observe(&[], false), 0);
    // Two owed batches still request one each.
    assert_eq!(coins.observe(&[(6, 3)], true), 0);
    assert_eq!(coins.observe(&[(6, 3), (7, 9)], true), 0);
    assert_eq!(coins.observe(&[(6, 3)], false), 2);
    assert_eq!(coins.observe(&[(6, 3)], false), 0);
}

/// A session that opens in a battle: what is pending then is old news, and
/// stays old once the battle ends.
#[test]
fn notices_pending_when_a_battle_session_opens_stay_old() {
    let mut coins = ExpeditionCoins::default();
    assert_eq!(coins.observe(&[(1, 30)], true), 0);
    assert_eq!(coins.observe(&[(1, 30), (2, 4)], true), 0);
    assert_eq!(coins.observe(&[(1, 30), (2, 4)], false), 1);
}

#[test]
fn two_worker_batches_at_most_one_each() {
    let mut coins = ExpeditionCoins::default();
    // Notices pending when the tracker first looks (a load) are not news.
    assert_eq!(coins.observe(&[(1, 30)], false), 0);
    // Three expeditions settle; one sold nothing.
    let pending = [(1, 30), (2, 50), (3, 0), (4, 7)];
    assert_eq!(coins.observe(&pending, false), 2);
    assert_eq!(coins.observe(&pending, false), 0);
    assert_eq!(coins.observe(&[(2, 50), (4, 7), (5, 1)], false), 1);
}

fn drop(item_id: u64, tick: u64, quality: Quality, x: f32) -> LootEvent {
    LootEvent {
        at: [x, 1.5],
        item_id,
        definition: horde_core::content::content().items[0].id.clone(),
        level: 1,
        quality,
        name: "Test".into(),
        tick,
    }
}

#[test]
fn rare_drop_cue_comes_from_runner_not_widget() {
    let mut game = Game::default();
    game.set_seed(1);
    game.deploy(0, Mode::Personal).unwrap();
    let mut tracker = LootTracker::default();
    let (mut cues, mut places) = (Vec::new(), Places::default());
    let push = |game: &mut Game, event: LootEvent| {
        game.staging().attempt().unwrap().loot_events.push(event);
    };
    // A loaded battle's old drops are not news.
    push(&mut game, drop(1, 5, Quality::Legendary, 6.0));
    tracker.observe(
        BattleId::Root,
        game.attempt().unwrap(),
        &mut cues,
        &mut places,
    );
    assert_eq!(cues, vec![]);
    // A Common drop is silent.
    push(&mut game, drop(2, 9, Quality::Common, 6.0));
    tracker.observe(
        BattleId::Root,
        game.attempt().unwrap(),
        &mut cues,
        &mut places,
    );
    assert_eq!(cues, vec![]);
    // A Rare and an Epic drop in one tick: one cue, the Epic's.
    let width = game.attempt().unwrap().width_yards;
    push(&mut game, drop(3, 12, Quality::Rare, 4.0));
    push(&mut game, drop(4, 12, Quality::Epic, 9.0));
    tracker.observe(
        BattleId::Root,
        game.attempt().unwrap(),
        &mut cues,
        &mut places,
    );
    assert_eq!(cues, vec![Cue::LootDrop]);
    assert_eq!(places.of(Cue::LootDrop), Some(lane_position(9.0, width)));
    assert_eq!(places.voicing(Cue::LootDrop).speed, 1.03);
    // The same attempt again: nothing new.
    let (mut cues, mut places) = (Vec::new(), Places::default());
    tracker.observe(
        BattleId::Root,
        game.attempt().unwrap(),
        &mut cues,
        &mut places,
    );
    assert_eq!(cues, vec![]);
}

#[test]
fn loot_reveal_follows_quality_speed_and_ignores_auto_sell_rule() {
    assert_eq!(
        [Quality::Rare, Quality::Epic, Quality::Legendary].map(quality_speed),
        [1.00, 1.03, 1.06]
    );
    assert_eq!(
        [Quality::Poor, Quality::Common, Quality::Uncommon].map(quality_speed),
        [1.0; 3]
    );
    assert_eq!(
        [
            Quality::Common,
            Quality::Rare,
            Quality::Epic,
            Quality::Legendary
        ]
        .map(reveal_gain_db),
        [0.0, 0.0, 1.0, 2.0]
    );
    for (best, speed, gain_db) in [
        (Quality::Common, 1.0, 0.0),
        (Quality::Epic, 1.03, 1.0),
        (Quality::Legendary, 1.06, 2.0),
    ] {
        // Whatever the rule sold, the reveal is the same.
        for auto_sold in [0, 50] {
            let mut guard = ResultsAudioGuard::default();
            let cues = shown(&mut guard, results(1, Some(best), auto_sold), 0, 120);
            let reveal = cues.iter().find(|c| c.cue == Cue::LootReveal).unwrap();
            let voicing = reveal.voicing.expect("the reveal is voiced");
            assert_eq!(voicing.bank, BankId::Ui(UiFamily::LootReveal));
            assert_eq!((voicing.speed, voicing.gain_db), (speed, gain_db));
        }
    }
}
