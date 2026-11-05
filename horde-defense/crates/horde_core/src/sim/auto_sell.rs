//! Stash auto-sell (LE, W1-23): the pure [`Game::auto_sell_preview`] read
//! and the two commands behind the Equipment Auto-sell panel.
//! `SetAutoSell` stores the rule; `ApplyAutoSellToStash` sells what the
//! stored rule matches in the stash right now, for exactly the quote the
//! player confirmed. Every sale goes through `Game::plan_sale` and
//! `Campaign::execute_plan`, so the preview, the quote and the coins paid
//! are one plan. The settlement hooks (H1, W-a) arrive in wave 2.
use super::*;
use crate::campaign::{AutoSellRule, UpgradeJudge};
use crate::command::CommandError;
use crate::quality::Quality;

/// Which items a preview considers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoSellScope {
    /// Every owned item: the panel's "Would sell N now" and the stash apply.
    Stash,
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
    /// What `rule` would sell from `scope` now, ignoring `rule.enabled`.
    /// Candidates, in ascending id order: a quality (never quality-None shop
    /// stock or starters) strictly below `sell_below`, a selected slot, no
    /// player-origin sale guard, and, with `keep_upgrades`, no upgrade for
    /// any defender ([`UpgradeJudge::AnyDefender`]). Pure: no mutation, no
    /// RNG.
    pub fn auto_sell_preview(&self, rule: &AutoSellRule, scope: AutoSellScope) -> AutoSellPreview {
        let AutoSellScope::Stash = scope;
        let refs = self.strategy.sale_refs();
        let mut preview = AutoSellPreview::default();
        let mut candidates = Vec::new();
        for item in &self.campaign.items {
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
