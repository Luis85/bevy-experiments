//! Auto-sell (LE). Stash auto-sell (W1-23): the pure
//! [`Game::auto_sell_preview`] read and the two commands behind the
//! Equipment Auto-sell panel. `SetAutoSell` stores the rule;
//! `ApplyAutoSellToStash` sells what the stored rule matches in the stash
//! right now, for exactly the quote the player confirmed. Main-run
//! settlement, hook H1 (W2-14): [`Game::settle_auto_sell`] sells this run's
//! drops when the rule is enabled. Every sale goes through a `Campaign`
//! plan and `Campaign::execute_plan`, so the preview, the quote and the
//! coins paid are one plan. Expedition settlement, hook W-a (W2-16):
//! [`drain_expedition_sale`] sells an expedition's queued drops at its run
//! record, and once more as a final sweep before it is marked settled.
//! Results (W2-17): the pure [`Game::results_loot`] view of the reported
//! run's loot and the last-run preview scope.
use super::*;
use crate::campaign::{
    AutoSellRule, AutoSoldLine, AutoSoldReceipt, MAX_AUTO_SOLD_LINES, UpgradeJudge,
};
use crate::command::{Command, CommandError};
use crate::content::{ItemDefId, Slot};
use crate::quality::Quality;
use crate::strategy::{RecruitStatus, StrategyState};
use std::collections::BTreeSet;

/// Which items a preview considers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoSellScope {
    /// Every owned item: the panel's "Would sell N now" and the stash apply.
    Stash,
    /// The reported run's drops still owned (W2-17): the panel's "would
    /// have sold N" and its raise-the-threshold hint. Empty without a
    /// report, and in a worker, whose ids are its own.
    LastRun,
}

/// One of the reported run's drops still in the stash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeptDrop {
    pub item_id: u64,
    pub definition: ItemDefId,
    /// None only for an item without a rolled quality.
    pub quality: Option<Quality>,
    pub level: u32,
}

/// A kept drop that beats what the reported run's defender wears in its
/// slot by more than [`crate::campaign::UPGRADE_MARGIN`], and the command
/// that equips it on that defender.
#[derive(Clone, Debug, PartialEq)]
pub struct UpgradeOffer {
    pub item_id: u64,
    pub slot: Slot,
    /// The slot-score gain ([`Campaign::upgrade_gain`]).
    pub gain: f32,
    /// `Equip` for the player (Personal) or Mara (Npc), `EquipRecruit` for a
    /// recruit.
    pub command: Command,
}

/// The Results loot block's view of the reported run (loot-economy §8):
/// what dropped, what is still in the stash, what would upgrade the
/// defender who fought it and what settlement auto-sold.
#[derive(Clone, Debug, PartialEq)]
pub struct ResultsLoot {
    pub attempt_id: u64,
    /// Every drop the run banked (`Report::loot`).
    pub total: u32,
    pub by_quality: KindMap<Quality, u32>,
    /// The run's drops still owned, in ascending id order (drop order).
    /// Empty for a worker's report: its ids are worker-local, never the
    /// stash's.
    pub kept: Vec<KeptDrop>,
    /// At most one offer a slot (the largest gain, the earliest drop on a
    /// tie), largest gain first; the Results row shows the first two.
    /// Empty for a worker's report, and while the defender cannot change
    /// equipment (in battle, or a recruit away from camp).
    pub upgrades: Vec<UpgradeOffer>,
    pub auto_sold: AutoSoldReceipt,
}

/// What a rule would sell now. `ids` (ascending) are exactly the items a
/// stash apply sells and `coins` exactly what they pay.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoSellPreview {
    pub count: u32,
    pub coins: u64,
    pub by_quality: KindMap<Quality, u32>,
    /// Sold items per `Slot::index`.
    pub by_slot: [u32; 4],
    /// Matching, sellable items kept because they would upgrade a defender.
    pub kept_upgrades: u32,
    /// Matching items a sale guard keeps (in battle, reserved, locked,
    /// favourite, equipped, preset or recruit gear).
    pub blocked: u32,
    pub ids: Vec<u64>,
}

impl Game {
    /// What `rule` would sell from `scope` now, ignoring `rule.enabled`; the
    /// last-run scope only narrows which owned items are considered.
    /// Candidates, in ascending id order: a quality (never quality-None shop
    /// stock or starters) strictly below `sell_below`, a selected slot, no
    /// player-origin sale guard, and, with `keep_upgrades`, no upgrade for
    /// any defender ([`UpgradeJudge::AnyDefender`]). Pure: no mutation, no
    /// RNG.
    pub fn auto_sell_preview(&self, rule: &AutoSellRule, scope: AutoSellScope) -> AutoSellPreview {
        let last_run: Option<BTreeSet<u64>> = match scope {
            AutoSellScope::Stash => None,
            AutoSellScope::LastRun => Some(self.reported_drops().iter().copied().collect()),
        };
        let refs = self.strategy.sale_refs();
        let mut preview = AutoSellPreview::default();
        let mut candidates = Vec::new();
        for item in &self.campaign.items {
            if last_run.as_ref().is_some_and(|ids| !ids.contains(&item.id)) {
                continue;
            }
            let Some(quality) = item.quality else {
                continue;
            };
            if quality >= rule.sell_below || !rule.slots[item.definition().slot.index()] {
                continue;
            }
            if self
                .campaign
                .sale_block(item.id, &refs, SaleOrigin::Player)
                .is_some()
            {
                preview.blocked += 1;
            } else if rule.keep_upgrades
                && self
                    .campaign
                    .is_upgrade_for(item.id, &UpgradeJudge::AnyDefender, &self.strategy)
            {
                preview.kept_upgrades += 1;
            } else {
                candidates.push(item.id);
            }
        }
        let plan = self.plan_sale(SaleOrigin::Player, candidates);
        for &id in &plan.ids {
            let item = self.campaign.item(id).expect("planned items are owned");
            preview.by_quality[item.quality.expect("candidates have a quality")] += 1;
            preview.by_slot[item.definition().slot.index()] += 1;
        }
        preview.count = u32::try_from(plan.ids.len()).unwrap_or(u32::MAX);
        preview.coins = plan.coins;
        preview.ids = plan.ids;
        preview
    }
    /// The reported run's drop ids when they are this game's stash ids: none
    /// without a report, and none in a worker, whose ids are worker-local.
    fn reported_drops(&self) -> &[u64] {
        match &self.last_result {
            Some(report) if !self.worker => &report.drop_ids,
            _ => &[],
        }
    }
    /// The Results loot view of the reported run (loot-economy §8; W2-17),
    /// or None without a report. Totals and the receipt come from the
    /// saved report; `kept` lists its drops still owned; `upgrades` judges
    /// them for the defender who fought the run ([`Report::defender`]) by
    /// [`Campaign::defender_upgrade_gain`], keeping the best offer a slot.
    /// Offers need the defender free to change equipment (no defense in
    /// progress, a recruit Ready in camp) and an unreserved item. A worker's
    /// report has neither kept drops nor offers. Pure: no mutation, no RNG.
    pub fn results_loot(&self) -> Option<ResultsLoot> {
        let report = self.last_result.as_ref()?;
        // One pass over the stash, not a lookup per drop.
        let drops: BTreeSet<u64> = self.reported_drops().iter().copied().collect();
        let mut kept: Vec<KeptDrop> = self
            .campaign
            .items
            .iter()
            .filter(|item| drops.contains(&item.id))
            .map(|item| KeptDrop {
                item_id: item.id,
                definition: item.definition.clone(),
                quality: item.quality,
                level: item.level,
            })
            .collect();
        kept.sort_unstable_by_key(|drop| drop.item_id);
        let defender = report.defender();
        let mut best: [Option<UpgradeOffer>; 4] = Default::default();
        if self.can_change_equipment(defender) {
            for drop in &kept {
                if self.campaign.item_reserved(drop.item_id) {
                    continue;
                }
                let Some(gain) =
                    self.campaign
                        .defender_upgrade_gain(drop.item_id, defender, &self.strategy)
                else {
                    continue;
                };
                let Some(slot) = drop.definition.definition().map(|d| d.slot) else {
                    continue;
                };
                let held = &mut best[slot.index()];
                if held.as_ref().is_none_or(|offer| gain > offer.gain) {
                    *held = Some(UpgradeOffer {
                        item_id: drop.item_id,
                        slot,
                        gain,
                        command: equip_command(defender, drop.item_id),
                    });
                }
            }
        }
        let mut upgrades: Vec<UpgradeOffer> = best.into_iter().flatten().collect();
        upgrades.sort_by(|a, b| {
            b.gain
                .total_cmp(&a.gain)
                .then(a.slot.index().cmp(&b.slot.index()))
        });
        Some(ResultsLoot {
            attempt_id: report.attempt_id,
            total: report.loot,
            by_quality: report.loot_by_quality.clone(),
            kept,
            upgrades,
            auto_sold: report.auto_sold.clone(),
        })
    }
    /// Whether `defender`'s equip command can apply now: no defense in
    /// progress, and a recruit on the roster and Ready in camp.
    fn can_change_equipment(&self, defender: DefenderId) -> bool {
        self.campaign.reserved.is_none()
            && match defender {
                DefenderId::Player | DefenderId::Mara => true,
                DefenderId::Recruit(id) => self
                    .strategy
                    .roster
                    .iter()
                    .any(|r| r.id == id && r.status == RecruitStatus::Ready),
            }
    }
    /// Hook H1 (loot-economy §7; rulings D13, E2, E5, E15): the main run's
    /// settlement sale of this run's `drops`, called by `finish` after the
    /// attempt ends and the takeover release, before the run record and the
    /// report. Nothing happens in a worker or with the rule off. The sale
    /// ([`settlement_sale`]) judges `keep_upgrades` against `defender`, the
    /// defender who ran the battle; every roster loadout stays guarded, the
    /// defending recruit's included. No RNG and no event.
    pub(super) fn settle_auto_sell(
        &mut self,
        drops: &[u64],
        defender: DefenderId,
    ) -> AutoSoldReceipt {
        if self.worker || !self.campaign.auto_sell.enabled {
            return AutoSoldReceipt::default();
        }
        settlement_sale(
            &mut self.campaign,
            &self.strategy,
            drops,
            &UpgradeJudge::Defender(defender),
        )
    }
    /// `Command::SetAutoSell`: stores a valid rule, in or out of battle.
    pub(crate) fn set_auto_sell(&mut self, rule: AutoSellRule) -> Result<(), CommandError> {
        rule.validate()
            .map_err(|_| CommandError::AutoSellRuleInvalid(rule.sell_below))?;
        self.campaign.auto_sell = rule;
        Ok(())
    }
    /// `Command::ApplyAutoSellToStash`: recomputes the stored rule's stash
    /// preview and sells it as one batch when it still equals the confirmed
    /// quote. Checked in order: `SaleBlocked(InBattle)`, `NothingToSell`,
    /// `SaleQuoteChanged` (with the current quote), `WalletLimit`. Every
    /// error leaves the game unchanged: the checks only read, and
    /// `execute_plan` checks the wallet before it removes anything, which
    /// gives the clone-then-commit guarantee without cloning the game.
    pub(crate) fn apply_auto_sell_to_stash(
        &mut self,
        expect_count: u32,
        expect_coins: u64,
    ) -> Result<(u32, u64), CommandError> {
        if self.campaign.reserved.is_some() {
            return Err(CommandError::SaleBlocked(SaleBlock::InBattle));
        }
        let preview = self.auto_sell_preview(&self.campaign.auto_sell, AutoSellScope::Stash);
        if preview.count == 0 {
            return Err(CommandError::NothingToSell);
        }
        if (preview.count, preview.coins) != (expect_count, expect_coins) {
            return Err(CommandError::SaleQuoteChanged {
                count: preview.count,
                coins: preview.coins,
            });
        }
        let plan = SalePlan {
            ids: preview.ids,
            coins: preview.coins,
        };
        let coins = self.campaign.execute_plan(&plan)?;
        Ok((preview.count, coins))
    }
}

/// The command that equips item `id` on `defender`.
fn equip_command(defender: DefenderId, id: u64) -> Command {
    match defender {
        DefenderId::Player => Command::Equip {
            mode: Mode::Personal,
            id,
        },
        DefenderId::Mara => Command::Equip {
            mode: Mode::Npc,
            id,
        },
        DefenderId::Recruit(recruit) => Command::EquipRecruit {
            id: recruit,
            item: id,
        },
    }
}

/// Hook W-a (loot-economy §7; rulings D13, E5): drains `battle`'s queue of
/// parent-side drops exactly once. With the rule enabled and
/// `apply_to_npc_runs` on, the queue is sold like a main run's drops,
/// judged against the expedition's own loadout and NPC; otherwise it is
/// dropped, so a later rule change never sells it. `settle_parallel` calls
/// it at a pass's last run record and, as the final sweep, right before the
/// battle is marked settled. No RNG and no event.
pub(super) fn drain_expedition_sale(
    campaign: &mut Campaign,
    strategy: &StrategyState,
    battle: &mut ParallelBattle,
) -> AutoSoldReceipt {
    let ids = std::mem::take(&mut battle.pending_auto_sell);
    let rule = campaign.auto_sell;
    if ids.is_empty() || !rule.enabled || !rule.apply_to_npc_runs {
        return AutoSoldReceipt::default();
    }
    let judge = UpgradeJudge::Loadout {
        mode: Mode::Npc,
        loadout: battle.loadout,
        npc: Box::new(battle.game.campaign.npc.clone()),
    };
    settlement_sale(campaign, strategy, &ids, &judge)
}

/// The settlement sale H1 and W-a share, under the campaign's rule (each
/// caller checks its own gate). Candidates, in ascending id order: still
/// owned, a quality (never quality None) strictly below `sell_below`, a
/// selected slot, no settlement-origin sale guard (every roster loadout
/// stays guarded) and, with `keep_upgrades`, no upgrade for `judge`. An
/// item that would overflow the wallet is kept and counted. Returns the
/// receipt: exact totals and the [`MAX_AUTO_SOLD_LINES`] sales with the
/// highest coins (ties to the lower id). No RNG.
fn settlement_sale(
    campaign: &mut Campaign,
    strategy: &StrategyState,
    drops: &[u64],
    judge: &UpgradeJudge,
) -> AutoSoldReceipt {
    let rule = campaign.auto_sell;
    let mut ids = drops.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let candidates: Vec<u64> = ids
        .into_iter()
        .filter(|&id| {
            let Some(item) = campaign.item(id) else {
                return false;
            };
            item.quality
                .is_some_and(|quality| quality < rule.sell_below)
                && rule.slots[item.definition().slot.index()]
                && !(rule.keep_upgrades && campaign.is_upgrade_for(id, judge, strategy))
        })
        .collect();
    let (plan, kept_on_overflow) = campaign.plan_settlement(&strategy.sale_refs(), candidates);
    let mut receipt = AutoSoldReceipt {
        kept_on_overflow,
        ..AutoSoldReceipt::default()
    };
    let mut lines = Vec::with_capacity(plan.ids.len());
    for &id in &plan.ids {
        let item = campaign.item(id).expect("planned items are owned");
        let quality = item.quality.expect("candidates have a quality");
        let coins = campaign
            .item_sale_value(id)
            .expect("planned items are owned");
        receipt.by_quality[quality] += 1;
        lines.push((
            id,
            AutoSoldLine {
                definition: item.definition.clone(),
                level: item.level,
                quality,
                coins,
            },
        ));
    }
    lines.sort_by(|(a_id, a), (b_id, b)| b.coins.cmp(&a.coins).then(a_id.cmp(b_id)));
    receipt.lines = lines
        .into_iter()
        .take(MAX_AUTO_SOLD_LINES)
        .map(|(_, line)| line)
        .collect();
    receipt.count = u32::try_from(plan.ids.len()).unwrap_or(u32::MAX);
    receipt.coins = campaign
        .execute_plan(&plan)
        .expect("a settlement plan fits the wallet");
    receipt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, CommandError, CommandOutcome};
    use crate::quality::Quality;

    /// The stored rule the stash tests use: Poor and Common in every slot,
    /// upgrades not kept, left disabled (the stash apply ignores `enabled`).
    fn rule() -> AutoSellRule {
        AutoSellRule {
            sell_below: Quality::Uncommon,
            keep_upgrades: false,
            ..AutoSellRule::default()
        }
    }
    fn game_with_rule() -> Game {
        let mut g = Game::default();
        g.campaign.auto_sell = rule();
        g
    }
    /// A level-1 drop of `definition` at `quality`.
    fn found(g: &mut Game, definition: usize, quality: Quality) -> u64 {
        g.campaign
            .add_item_with_quality(definition, 1, Some(quality))
            .unwrap()
    }
    fn stash_apply(
        g: &mut Game,
        expect_count: u32,
        expect_coins: u64,
    ) -> Result<CommandOutcome, CommandError> {
        g.apply(&Command::ApplyAutoSellToStash {
            expect_count,
            expect_coins,
        })
    }
    fn stash_preview(g: &Game) -> AutoSellPreview {
        g.auto_sell_preview(&g.campaign.auto_sell, AutoSellScope::Stash)
    }

    #[test]
    fn set_auto_sell_rejects_poor_threshold_with_auto_sell_rule_invalid() {
        let mut g = Game::default();
        let before = g.state_digest();
        let poor = AutoSellRule {
            enabled: true,
            sell_below: Quality::Poor,
            ..AutoSellRule::default()
        };
        assert_eq!(
            g.apply(&Command::SetAutoSell(poor)),
            Err(CommandError::AutoSellRuleInvalid(Quality::Poor))
        );
        assert_eq!(g.state_digest(), before, "a rejected rule changes nothing");
        for quality in Quality::ALL.into_iter().skip(1) {
            let rule = AutoSellRule {
                enabled: true,
                sell_below: quality,
                ..AutoSellRule::default()
            };
            assert_eq!(
                g.apply(&Command::SetAutoSell(rule)),
                Ok(CommandOutcome::Applied)
            );
            assert_eq!(g.campaign.auto_sell, rule);
        }
        // Allowed mid-battle: the rule acts at the next settlement.
        g.campaign.reserve(Mode::Personal).unwrap();
        assert_eq!(
            g.apply(&Command::SetAutoSell(AutoSellRule::default())),
            Ok(CommandOutcome::Applied)
        );
        assert!(g.campaign.auto_sell.is_default());
    }

    #[test]
    fn apply_auto_sell_to_stash_error_order_is_in_battle_then_nothing_then_quote_then_wallet() {
        let mut g = game_with_rule();
        // An empty stash in battle: InBattle comes before NothingToSell.
        g.campaign.reserve(Mode::Personal).unwrap();
        assert_eq!(
            stash_apply(&mut g, 0, 0),
            Err(CommandError::SaleBlocked(SaleBlock::InBattle))
        );
        g.campaign.reserved = None;
        assert_eq!(stash_apply(&mut g, 0, 0), Err(CommandError::NothingToSell));
        // NothingToSell comes before the quote check: a non-zero quote on an
        // empty stash is not reported as a changed quote.
        assert_eq!(stash_apply(&mut g, 1, 7), Err(CommandError::NothingToSell));
        let id = found(&mut g, 5, Quality::Poor);
        let coins = g.campaign.item_sale_value(id).unwrap();
        // In battle the matching drop is still refused with InBattle, even
        // for the right quote.
        g.campaign.reserve(Mode::Personal).unwrap();
        assert_eq!(
            stash_apply(&mut g, 1, coins),
            Err(CommandError::SaleBlocked(SaleBlock::InBattle))
        );
        g.campaign.reserved = None;
        assert_eq!(
            stash_apply(&mut g, 0, 0),
            Err(CommandError::SaleQuoteChanged { count: 1, coins })
        );
        // A full wallet and a stale quote: the quote is reported first.
        g.campaign.coins = u64::MAX;
        assert_eq!(
            stash_apply(&mut g, 2, coins),
            Err(CommandError::SaleQuoteChanged { count: 1, coins })
        );
        let before = g.state_digest();
        assert_eq!(
            stash_apply(&mut g, 1, coins),
            Err(CommandError::WalletLimit)
        );
        assert_eq!(g.state_digest(), before, "the wallet limit sells nothing");
        g.campaign.coins = 0;
        assert_eq!(
            stash_apply(&mut g, 1, coins),
            Ok(CommandOutcome::Sold { count: 1, coins })
        );
        assert!(g.campaign.item(id).is_none());
        assert_eq!(g.campaign.coins, coins);
        // The realistic stale case: the player quotes one drop, then locks
        // it, so nothing matches. The old quote gets NothingToSell, not
        // SaleQuoteChanged { count: 0, coins: 0 }.
        let late = found(&mut g, 5, Quality::Poor);
        let quote = stash_preview(&g);
        assert_eq!(quote.ids, vec![late]);
        g.campaign.set_lock(late, true).unwrap();
        let before = g.state_digest();
        assert_eq!(
            stash_apply(&mut g, quote.count, quote.coins),
            Err(CommandError::NothingToSell)
        );
        assert_eq!(g.state_digest(), before, "NothingToSell changes nothing");
    }

    #[test]
    fn apply_auto_sell_to_stash_rejects_a_stale_quote_without_mutation() {
        let mut g = game_with_rule();
        let first = found(&mut g, 5, Quality::Poor);
        let quote = stash_preview(&g);
        assert_eq!(quote.ids, vec![first]);
        // The stash changes after the confirmation opened: a new drop of the
        // same count but other coins, and the quoted drop gets locked.
        let second = found(&mut g, 6, Quality::Common);
        g.campaign.set_lock(first, true).unwrap();
        let fresh = stash_preview(&g);
        assert_eq!(fresh.ids, vec![second]);
        assert_eq!(fresh.count, quote.count);
        assert_ne!(fresh.coins, quote.coins);
        let before = g.state_digest();
        assert_eq!(
            stash_apply(&mut g, quote.count, quote.coins),
            Err(CommandError::SaleQuoteChanged {
                count: fresh.count,
                coins: fresh.coins
            })
        );
        assert_eq!(g.state_digest(), before, "a stale quote changes nothing");
        // A count change is stale too.
        found(&mut g, 7, Quality::Poor);
        let before = g.state_digest();
        assert!(matches!(
            stash_apply(&mut g, fresh.count, fresh.coins),
            Err(CommandError::SaleQuoteChanged { count: 2, .. })
        ));
        assert_eq!(g.state_digest(), before);
    }

    #[test]
    fn apply_to_stash_ignores_enabled_and_never_sells_shop_items() {
        let mut g = game_with_rule();
        assert!(!g.campaign.auto_sell.enabled);
        g.campaign.coins = 1_000;
        let CommandOutcome::Id(bought) = g
            .apply(&Command::BuyItem(crate::content::ItemDefId::at(5)))
            .unwrap()
        else {
            panic!("a purchase returns its item id");
        };
        assert_eq!(g.campaign.item(bought).unwrap().quality, None);
        let drop = found(&mut g, 5, Quality::Poor);
        let owned_before = g.campaign.owned_item_ids();
        // Even the widest threshold leaves quality-None items (shop stock,
        // starters, pre-quality saves) alone.
        let widest = AutoSellRule {
            sell_below: Quality::Legendary,
            ..rule()
        };
        let preview = g.auto_sell_preview(&widest, AutoSellScope::Stash);
        assert_eq!(preview.ids, vec![drop]);
        assert_eq!(preview.blocked, 0, "quality-None items are not candidates");
        let preview = stash_preview(&g);
        assert_eq!(preview.ids, vec![drop]);
        let wallet = g.campaign.coins;
        assert_eq!(
            stash_apply(&mut g, preview.count, preview.coins),
            Ok(CommandOutcome::Sold {
                count: 1,
                coins: preview.coins
            })
        );
        assert!(
            !g.campaign.auto_sell.enabled,
            "applying never enables the rule"
        );
        assert_eq!(g.campaign.coins, wallet + preview.coins);
        let expected: Vec<u64> = owned_before.into_iter().filter(|id| *id != drop).collect();
        assert_eq!(g.campaign.owned_item_ids(), expected);
        assert!(g.campaign.item(bought).is_some());
    }

    #[test]
    fn stash_preview_counts_guards_upgrades_threshold_and_slots() {
        let mut g = game_with_rule();
        let sold = found(&mut g, 5, Quality::Common);
        let locked = found(&mut g, 5, Quality::Poor);
        g.campaign.set_lock(locked, true).unwrap();
        let favorite = found(&mut g, 6, Quality::Poor);
        g.campaign.set_favorite(favorite, true).unwrap();
        let rare = found(&mut g, 6, Quality::Uncommon);
        let accessory = found(&mut g, 7, Quality::Poor);
        let rule = AutoSellRule {
            slots: [true, true, true, false],
            ..rule()
        };
        let preview = g.auto_sell_preview(&rule, AutoSellScope::Stash);
        assert_eq!(preview.ids, vec![sold]);
        assert_eq!(preview.count, 1);
        assert_eq!(preview.coins, g.campaign.item_sale_value(sold).unwrap());
        assert_eq!(preview.blocked, 2, "locked and favourite");
        assert_eq!(preview.kept_upgrades, 0);
        assert_eq!(preview.by_slot, [0, 1, 0, 0]);
        assert_eq!(preview.by_quality[Quality::Common], 1);
        assert!(![rare, accessory].iter().any(|id| preview.ids.contains(id)));
        // keep_upgrades consults every defender: the fire axe beats the
        // player's crowbar and tier-3 riot armor the starter work jacket, so
        // both are kept and each is counted once.
        let armor = found(&mut g, 14, Quality::Common);
        assert!(g.campaign.is_upgrade_for(
            armor,
            &crate::campaign::UpgradeJudge::AnyDefender,
            &g.strategy
        ));
        let keep = AutoSellRule {
            keep_upgrades: true,
            ..rule
        };
        let preview = g.auto_sell_preview(&keep, AutoSellScope::Stash);
        assert_eq!(preview.ids, Vec::<u64>::new());
        assert_eq!(preview.kept_upgrades, 2, "the axe and the armor");
        assert_eq!(preview.blocked, 2);
        let preview = g.auto_sell_preview(&rule, AutoSellScope::Stash);
        assert_eq!(preview.ids, vec![sold, armor]);
        // Not only the player: a marksman rifle is no upgrade for the
        // untrained player, but it is one for Mara once she is available.
        let rifle = found(&mut g, 4, Quality::Common);
        let strategy = &g.strategy;
        let player = UpgradeJudge::Defender(DefenderId::Player);
        let mara = UpgradeJudge::Defender(DefenderId::Mara);
        assert!(!g.campaign.is_upgrade_for(rifle, &player, strategy));
        assert!(g.campaign.is_upgrade_for(rifle, &mara, strategy));
        // Without the companion Mara is not judged, so the rifle is sold.
        assert!(!g.campaign.npc_available());
        let preview = g.auto_sell_preview(&keep, AutoSellScope::Stash);
        assert_eq!(preview.ids, vec![rifle]);
        assert_eq!(preview.kept_upgrades, 2);
        g.campaign.legacy_features = true;
        g.campaign.attempts = 1;
        assert!(g.campaign.npc_available());
        let preview = g.auto_sell_preview(&keep, AutoSellScope::Stash);
        assert_eq!(preview.ids, Vec::<u64>::new());
        assert_eq!(
            preview.kept_upgrades, 3,
            "the axe, the armor and Mara's rifle"
        );
        assert_eq!(preview.blocked, 2);
        // In battle the player origin blocks every candidate.
        g.campaign.reserve(Mode::Personal).unwrap();
        let preview = g.auto_sell_preview(&rule, AutoSellScope::Stash);
        assert_eq!(preview.count, 0);
        assert_eq!(preview.blocked, 5);
    }

    #[test]
    fn one_stash_apply_of_50_items_returns_one_sold_outcome() {
        let mut g = game_with_rule();
        let ids: Vec<u64> = (0..50)
            .map(|n| {
                let quality = if n % 2 == 0 {
                    Quality::Poor
                } else {
                    Quality::Common
                };
                found(&mut g, 4 + n % 4, quality)
            })
            .collect();
        let preview = stash_preview(&g);
        assert_eq!(preview.ids, ids);
        assert_eq!(preview.count, 50);
        assert_eq!(preview.by_slot, [13, 13, 12, 12]);
        assert_eq!(preview.by_quality[Quality::Poor], 25);
        assert_eq!(preview.by_quality[Quality::Common], 25);
        let wallet = g.campaign.coins;
        // One command, one batch outcome (R9: one cue for the batch).
        assert_eq!(
            stash_apply(&mut g, 50, preview.coins),
            Ok(CommandOutcome::Sold {
                count: 50,
                coins: preview.coins
            })
        );
        assert_eq!(g.campaign.coins, wallet + preview.coins);
        assert!(ids.iter().all(|id| g.campaign.item(*id).is_none()));
        assert_eq!(stash_apply(&mut g, 0, 0), Err(CommandError::NothingToSell));
    }
}
