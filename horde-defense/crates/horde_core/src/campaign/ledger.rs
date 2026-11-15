//! Coin and XP transactions: rewards, purchases, sales and run bookkeeping.

use super::*;
use crate::command::CommandError;

impl Campaign {
    pub(super) fn spend(&mut self, price: u64) -> Result<(), String> {
        if self.coins < price {
            return Err(crate::coins::need_more(price - self.coins));
        }
        self.coins -= price;
        Ok(())
    }
    pub(crate) fn append_run(&mut self, mut record: crate::stats::RunRecord) {
        record.id = self.next_run_id;
        self.next_run_id = self
            .next_run_id
            .checked_add(1)
            .expect("Run history sequence exhausted");
        // Copy on write: a snapshot still holding the history keeps it, and
        // only then are the earlier records copied.
        std::sync::Arc::make_mut(&mut self.run_history).push(record);
    }
    pub fn reward(&mut self, coins: u64, xp: u64, mode: Mode) -> u64 {
        self.coins = self.coins.saturating_add(coins);
        if mode == Mode::Personal {
            self.award_player_xp(xp)
        } else {
            0
        }
    }
    pub fn buy_item(&mut self, definition: usize) -> Result<u64, String> {
        self.preparation()?;
        let def = content().items.get(definition).ok_or("Unknown equipment")?;
        if def.tier == 0 {
            return Err("Protected starter equipment is already owned".into());
        }
        if !def.purchasable() {
            return Err(crate::content::LOOT_ONLY_REASON.into());
        }
        if def.tier > self.unlocked() {
            return Err("Clear more stages to unlock this equipment tier".into());
        }
        let price = self.buy_price(definition)?;
        if self.coins < price {
            return Err(crate::coins::need_more(price - self.coins));
        }
        let id = self.add_item_at_level(definition, self.shop_item_level())?;
        self.coins -= price;
        Ok(id)
    }
    /// `Command::SellItem`, strict since ruleset 2: one item sells only when
    /// [`Self::sale_block`] allows it, otherwise the typed block is returned
    /// and nothing changes. No loadout, preset or recruit slot is rewritten.
    pub(crate) fn sell(&mut self, id: u64, refs: &SaleRefs) -> Result<u64, CommandError> {
        let quote = self
            .sale_eligibility_for(id, SaleOrigin::Player, refs)
            .map_err(CommandError::SaleBlocked)?;
        self.execute_plan(&SalePlan {
            ids: vec![id],
            coins: quote.coins,
        })
    }
    /// What selling every eligible item pays; 0 while a defender is deployed.
    pub(crate) fn sell_quote(&self, refs: &SaleRefs) -> u64 {
        self.plan_batch(refs, SaleOrigin::Player, self.owned_item_ids())
            .coins
    }
    /// Sells every eligible item, or nothing when the wallet would overflow.
    /// While a defender is deployed it answers `SaleBlocked(InBattle)`
    /// (`StrategyState::sell_extras` keeps its `Ok(0)` in front of this).
    pub(crate) fn sell_unprotected(&mut self, refs: &SaleRefs) -> Result<u64, CommandError> {
        if self.reserved.is_some() {
            return Err(CommandError::SaleBlocked(SaleBlock::InBattle));
        }
        let plan = self.plan_batch(refs, SaleOrigin::Player, self.owned_item_ids());
        self.execute_plan(&plan)
    }
    /// Ends the main run's reservation and counts the attempt. The npc's own
    /// attempt count belongs to whoever fought (Mara or a recruit), so
    /// `Game::finish` bumps it through the attempt's seat (ruling D8).
    pub fn finish_attempt(&mut self) {
        self.reserved = None;
        self.attempts += 1;
    }
}
