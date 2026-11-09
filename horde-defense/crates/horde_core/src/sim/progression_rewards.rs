//! Progression settlement, W2-C (design-progression-tree §6-7, rulings E1
//! and E2): level-up coin grants and rank milestones are paid from saved
//! ledgers, once, by the root game, never at the level-up or rank change
//! itself and never in a worker.
use super::*;

impl Game {
    /// Pays what the progression ledgers owe and returns the receipt.
    ///
    /// - The level grants `level_grant(l)` for every `l` in
    ///   `(level_grant_paid.max(1) + 1)..=level` (saturating), after which
    ///   `level_grant_paid` is the level. A level-1 player writes nothing.
    /// - Every rank milestone at or below `progression_rank()` that is not in
    ///   `milestones_claimed`, in ascending rank order: its coins
    ///   (saturating) and its item ([`Campaign::milestone_item_definition`]
    ///   at `shop_item_level()`, quality the authored one or the
    ///   definition's base quality if higher). An item that cannot be stored
    ///   records [`SettlementIssue::MilestoneItemLost`]; its coins and the
    ///   claim still apply. A frozen v7 milestone rank
    ///   ([`V7_MILESTONE_RANKS`]) the catalogue authors no reward for is
    ///   claimed unpaid, as migration step (d) claims it, so authoring it
    ///   later pays no backlog (this replaces the wave-1
    ///   `Campaign::claim_neutral_milestones`).
    ///
    /// The receipt lists only coins paid and milestones authored. A worker
    /// pays nothing and returns the empty receipt: the parent settles its
    /// victories. Callers: `finish` after the H1 auto-sell, and the end of
    /// `settle_parallel` when no main attempt is live.
    pub(crate) fn settle_progression_rewards(&mut self) -> ProgressionRewards {
        let mut receipt = ProgressionRewards::default();
        if self.worker {
            return receipt;
        }
        let rewards = crate::progression::rewards();
        let player = &mut self.campaign.player;
        let paid = player.level_grant_paid.max(1);
        if player.level > paid {
            let coins = (paid + 1..=player.level)
                .map(|level| rewards.level_grant(level))
                .fold(0, u64::saturating_add);
            player.level_grant_paid = player.level;
            if coins > 0 {
                receipt.level_from = paid;
                receipt.level_to = player.level;
                receipt.level_coins = coins;
                self.campaign.coins = self.campaign.coins.saturating_add(coins);
            }
        }
        let rank = self.campaign.progression_rank();
        for milestone in rewards.milestones.iter().filter(|m| m.rank <= rank) {
            if !self.campaign.milestones_claimed.insert(milestone.rank) {
                continue;
            }
            self.campaign.coins = self.campaign.coins.saturating_add(milestone.coins);
            let (item_id, item_definition) = self.grant_milestone_item(milestone);
            receipt.milestones.push(MilestoneReward {
                rank: milestone.rank,
                coins: milestone.coins,
                item_id,
                item_definition,
            });
        }
        self.campaign
            .milestones_claimed
            .extend(V7_MILESTONE_RANKS.into_iter().filter(|r| *r <= rank));
        receipt
    }
    /// Stores `milestone`'s item. It is not a drop, so the bad-luck loot
    /// counter is left as it was. None when no definition qualifies or the
    /// item could not be stored (reported once as `MilestoneItemLost`).
    fn grant_milestone_item(
        &mut self,
        milestone: &crate::progression::MilestoneDefinition,
    ) -> (Option<u64>, Option<ItemDefId>) {
        let Some(index) = self
            .campaign
            .milestone_item_definition(self.seed, milestone.rank)
        else {
            return (None, None);
        };
        let definition = &content().items[index];
        let quality = definition.base_quality.max(milestone.item_quality);
        let level = self.campaign.shop_item_level();
        let kills_since_loot = self.campaign.kills_since_loot;
        let stored = self
            .campaign
            .add_item_with_quality(index, level, Some(quality));
        self.campaign.kills_since_loot = kills_since_loot;
        match stored {
            Ok(id) => (Some(id), Some(definition.id.clone())),
            Err(error) => {
                self.settlement_issues
                    .push(SettlementIssue::MilestoneItemLost {
                        rank: milestone.rank,
                        definition: definition.id.clone(),
                        error,
                    });
                (None, None)
            }
        }
    }
}
