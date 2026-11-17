//! The loot and coin cues: what a drop, a Results screen and a sale sound
//! like, decided here and never in a widget.
//!
//! - **The drop.** [`LootTracker`] reads the attempt's `loot_events` after
//!   every tick. A new drop of Rare or better plays `LootDrop` on the
//!   Effects bus, panned to where it fell, at the speed of its tier
//!   ([`quality_speed`]). Several in one frame are one cue, the best one's.
//! - **The reveal.** [`ResultsAudioGuard`] sounds a report once, the first
//!   time its Results screen shows: `LootReveal` after
//!   [`LOOT_REVEAL_DELAY_MS`] when the run dropped anything, at the speed
//!   and level of its best drop, then one `CoinBatch` after
//!   [`COIN_BATCH_DELAY_MS`] when settlement auto-sold anything or the
//!   battle left an expedition batch owed. The reveal reads the drops only:
//!   the auto-sell rule does not change it.
//! - **The coins.** One `CoinBatch` per batch, however many items it sold
//!   (ruling R9): a successful sale command ([`sale_cue`]), a report's
//!   auto-sold receipt, and each settled expedition whose receipt sold
//!   anything ([`ExpeditionCoins`]). None of them sounds during a battle;
//!   an expedition batch owed from one sounds with its Results screen's
//!   coins, as one `CoinBatch` clear of the stinger.
//!
//! Nothing hovers: no cue here or anywhere reads the pointer.
//!
//! All of it is presentation. It reads reports, receipts and command
//! outcomes, changes nothing and is not saved.

use crate::{
    cue::Cue,
    events::push_cue,
    pan::{self, Places},
    voicing::Voicing,
};
use horde_core::{
    command::{Command, CommandOutcome},
    quality::Quality,
    sim::{Attempt, BattleId, Game, Report},
};
use std::collections::{BTreeMap, BTreeSet};

/// The reveal follows the first Results frame by this long, clear of the
/// stinger's attack.
pub const LOOT_REVEAL_DELAY_MS: u64 = 150;
/// The coins follow the first Results frame by this long, after the reveal.
pub const COIN_BATCH_DELAY_MS: u64 = 450;

/// The playback speed of a loot cue of tier `quality`: a better drop rings
/// a little higher. Below Rare it is the clip's own.
pub const fn quality_speed(quality: Quality) -> f32 {
    match quality {
        Quality::Poor | Quality::Common | Quality::Uncommon | Quality::Rare => 1.00,
        Quality::Epic => 1.03,
        Quality::Legendary => 1.06,
    }
}

/// The reveal is 1 dB louder per tier above Rare.
pub const fn reveal_gain_db(quality: Quality) -> f32 {
    match quality {
        Quality::Poor | Quality::Common | Quality::Uncommon | Quality::Rare => 0.0,
        Quality::Epic => 1.0,
        Quality::Legendary => 2.0,
    }
}

/// Whether a drop of `quality` is heard in battle: Rare and better.
pub const fn drop_is_heard(quality: Quality) -> bool {
    matches!(quality, Quality::Rare | Quality::Epic | Quality::Legendary)
}

/// What a battle drop of `quality` plays.
pub const fn drop_voicing(quality: Quality) -> Voicing {
    Voicing::tuned(Cue::LootDrop.bank(), quality_speed(quality), 0.0)
}

/// What the reveal of a report whose best drop is `best` plays.
pub const fn reveal_voicing(best: Quality) -> Voicing {
    Voicing::tuned(
        Cue::LootReveal.bank(),
        quality_speed(best),
        reveal_gain_db(best),
    )
}

/// The newest drop the tracker saw of one battle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SeenLoot {
    attempt_id: u64,
    /// `(tick, item_id)` of the newest drop; both only grow in an attempt.
    newest: Option<(u64, u64)>,
}

/// Hears the drops of the battles it is shown. It primes on an attempt it
/// has not seen (a deploy, a load), so a loaded battle's earlier drops are
/// not played again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LootTracker {
    battles: BTreeMap<BattleId, SeenLoot>,
}
impl LootTracker {
    /// Queues `LootDrop` for the drops of `attempt` that are new since the
    /// last call for `battle`: once, at the place and tier of the best.
    pub fn observe(
        &mut self,
        battle: BattleId,
        attempt: &Attempt,
        cues: &mut Vec<Cue>,
        places: &mut Places,
    ) {
        let key = |event: &horde_core::quality::LootEvent| (event.tick, event.item_id);
        let newest = attempt.loot_events.iter().map(key).max();
        let seen = self.battles.get(&battle).copied();
        self.battles.insert(
            battle,
            SeenLoot {
                attempt_id: attempt.id,
                newest,
            },
        );
        let Some(seen) = seen.filter(|seen| seen.attempt_id == attempt.id) else {
            return;
        };
        let best = attempt
            .loot_events
            .iter()
            .filter(|event| seen.newest.is_none_or(|newest| key(event) > newest))
            .filter(|event| drop_is_heard(event.quality))
            .max_by_key(|event| (event.quality, key(event)));
        if let Some(event) = best {
            push_cue(cues, Cue::LootDrop);
            places.place(
                Cue::LootDrop,
                pan::lane_position(event.at[0], attempt.width_yards),
            );
            places.voice(Cue::LootDrop, drop_voicing(event.quality));
        }
    }
    /// Drops every battle but those in `keep`.
    pub fn retain(&mut self, keep: impl Fn(BattleId) -> bool) {
        self.battles.retain(|battle, _| keep(*battle));
    }
}

/// What a report's Results screen sounds like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResultsLoot {
    pub attempt_id: u64,
    /// The best tier among the run's drops; `None` when it dropped nothing.
    pub best: Option<Quality>,
    /// How many items settlement auto-sold.
    pub auto_sold: u32,
    /// The expedition batches the battle left owed
    /// ([`ExpeditionCoins::take_for_results`]): they sound with the run's
    /// own, as one `CoinBatch`.
    pub expedition_batches: usize,
}
impl ResultsLoot {
    /// The loot of `report`, with no expedition batch owed. The best tier
    /// counts every drop, sold or kept.
    pub fn of(report: &Report) -> Self {
        Self {
            attempt_id: report.attempt_id,
            best: report
                .loot_by_quality
                .entries()
                .filter(|(_, count)| **count > 0)
                .map(|(quality, _)| quality)
                .max(),
            auto_sold: report.auto_sold.count,
            expedition_batches: 0,
        }
    }
}

/// One interface cue and what it plays; `None` is the cue's own bank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiCue {
    pub cue: Cue,
    pub voicing: Option<Voicing>,
}

/// Sounds each report's Results screen once. Presentation-local: after a
/// load the report on screen sounds once more, as its stinger does not.
/// Its one `CoinBatch` also carries the expedition batches the battle left
/// owed, so none of them plays under the stinger or inside the coins' gate.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResultsAudioGuard {
    last_attempt: Option<u64>,
    /// The reveal still to play, and when.
    reveal: Option<(u64, Voicing)>,
    /// When the coins still to play are due.
    coins: Option<u64>,
}
impl ResultsAudioGuard {
    /// One frame at real time `now_ms`. `results` is the report on the
    /// Results screen, `None` on any other screen (what was still pending
    /// is dropped). Returns the cues due this frame.
    pub fn observe(&mut self, results: Option<ResultsLoot>, now_ms: u64) -> Vec<UiCue> {
        let Some(results) = results else {
            self.reveal = None;
            self.coins = None;
            return Vec::new();
        };
        if self.last_attempt != Some(results.attempt_id) {
            self.last_attempt = Some(results.attempt_id);
            self.reveal = results
                .best
                .map(|best| (now_ms + LOOT_REVEAL_DELAY_MS, reveal_voicing(best)));
            let sold = results.auto_sold > 0 || results.expedition_batches > 0;
            self.coins = sold.then_some(now_ms + COIN_BATCH_DELAY_MS);
        } else if results.expedition_batches > 0 && self.coins.is_none() {
            // Owed batches that reach a report already sounded still play,
            // on the coins' own timing.
            self.coins = Some(now_ms + COIN_BATCH_DELAY_MS);
        }
        let mut due = Vec::new();
        if let Some((_, voicing)) = self.reveal.take_if(|(at, _)| now_ms >= *at) {
            due.push(UiCue {
                cue: Cue::LootReveal,
                voicing: Some(voicing),
            });
        }
        if self.coins.take_if(|at| now_ms >= *at).is_some() {
            due.push(UiCue {
                cue: Cue::CoinBatch,
                voicing: None,
            });
        }
        due
    }
}

/// The cue of a command that succeeded with `outcome`: one `CoinBatch` for
/// a sale that sold at least one item, whatever the batch's size. Nothing
/// during a battle.
pub fn sale_cue(command: &Command, outcome: &CommandOutcome, in_battle: bool) -> Option<Cue> {
    if in_battle {
        return None;
    }
    let sold = match (command, outcome) {
        // A strict sale sells its one item or fails.
        (Command::SellItem(_), _) => true,
        (Command::SellUnprotected, CommandOutcome::Amount(coins)) => *coins > 0,
        (Command::ApplyAutoSellToStash { .. }, CommandOutcome::Sold { count, .. }) => *count > 0,
        _ => false,
    };
    sold.then_some(Cue::CoinBatch)
}

/// Hears each settled expedition's auto-sold batch once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExpeditionCoins {
    /// `None` until the first look: what is pending then (a load) is old.
    heard: Option<BTreeSet<u64>>,
    /// Batches that arrived during a battle and wait for it to end. Their
    /// notice may be acknowledged meanwhile (toasted in the pause, W3-25).
    owed: usize,
    /// Whether the last look was in a battle.
    in_battle: bool,
    /// Batches the battle left owed when it ended, for its Results screen
    /// ([`Self::take_for_results`]).
    for_results: usize,
}
impl ExpeditionCoins {
    /// `pending` is every unacknowledged expedition notice as `(battle id,
    /// items its receipt sold)`. Returns how many `CoinBatch` requests are
    /// due: one per notice not heard before that sold at least one item.
    /// During a battle nothing is due: a new batch is owed, even if its
    /// notice is gone by the time the battle ends. The look that ends the
    /// battle (its last tick, a withdrawal) plays no coins under the
    /// stinger: what is owed then is kept for the report's Results screen
    /// ([`Self::take_for_results`]), and due on a later look if no Results
    /// screen took it.
    pub fn observe(&mut self, pending: &[(u64, u32)], in_battle: bool) -> usize {
        let ids = || pending.iter().map(|(battle, _)| *battle);
        let ended = std::mem::replace(&mut self.in_battle, in_battle) && !in_battle;
        let Some(heard) = &mut self.heard else {
            self.heard = Some(ids().collect());
            return 0;
        };
        let new = pending
            .iter()
            .filter(|(battle, sold)| !heard.contains(battle) && *sold > 0)
            .count();
        // An acknowledged notice is forgotten; its id is never reused.
        *heard = ids().collect();
        if in_battle {
            self.owed += new;
            return 0;
        }
        if ended {
            self.for_results += std::mem::take(&mut self.owed) + new;
            return 0;
        }
        std::mem::take(&mut self.owed) + std::mem::take(&mut self.for_results) + new
    }
    /// The batches the battle that just ended left owed, taken once by the
    /// first frame of its Results screen, which plays them with the run's
    /// own as one `CoinBatch` ([`ResultsLoot::expedition_batches`]).
    pub fn take_for_results(&mut self) -> usize {
        std::mem::take(&mut self.for_results)
    }
    /// [`Self::observe`] of `game`'s unacknowledged expedition notices.
    pub fn observe_game(&mut self, game: &Game) -> usize {
        let pending: Vec<(u64, u32)> = game
            .battles()
            .iter()
            .filter(|battle| {
                battle.settled
                    && battle
                        .summary
                        .as_ref()
                        .is_some_and(|summary| !summary.acknowledged)
            })
            .map(|battle| {
                let sold = battle
                    .game()
                    .last_result()
                    .map_or(0, |report| report.auto_sold.count);
                (battle.id, sold)
            })
            .collect();
        self.observe(&pending, game.attempt().is_some())
    }
}
