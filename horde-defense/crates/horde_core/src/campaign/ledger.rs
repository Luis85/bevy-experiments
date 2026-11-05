//! Coin and XP transactions: rewards, purchases, sales and run bookkeeping.

use super::*;

impl Campaign {
    pub(super) fn spend(&mut self, price: u64) -> Result<(), String> {
        if self.coins < price {
            return Err(format!("Need {} more coins", price - self.coins));
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
        self.run_history.push(record);
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
        if def.tier > self.unlocked() {
            return Err("Clear more stages to unlock this equipment tier".into());
        }
        let price = self.buy_price(definition)?;
        if self.coins < price {
            return Err(format!("Need {} more coins", price - self.coins));
        }
        let id = self.add_item_at_level(definition, self.shop_item_level())?;
        self.coins -= price;
        Ok(id)
    }
    /// `Command::SellItem` before W1-20: one item through [`Self::sale_block`],
    /// overriding only the blocks [`SaleBlock::sell_item_overrides`] lists;
    /// the loadouts that held it fall back to protected starter equipment.
    pub(crate) fn sell(&mut self, id: u64, refs: &SaleRefs) -> Result<u64, String> {
        if let Some(block) = self.sale_block(id, refs, SaleOrigin::Player)
            && !block.sell_item_overrides()
        {
            return Err(block.label());
        }
        let coins = self.item_sale_value(id)?;
        let wallet = self
            .coins
            .checked_add(coins)
            .ok_or("Sale would exceed the wallet limit")?;
        self.items.retain(|x| x.id != id);
        for slot in 0..4 {
            if self.player.loadout[slot] == id {
                self.player.loadout[slot] = self.personal_starter_id(slot);
            }
            if self.npc.loadout[slot] == id {
                self.npc.loadout[slot] = slot as u64 + 1;
            }
        }
        self.coins = wallet;
        Ok(coins)
    }
    /// What selling every eligible item pays; 0 while a defender is deployed.
    pub(crate) fn sell_quote(&self, refs: &SaleRefs) -> u64 {
        self.plan_batch(refs, SaleOrigin::Player, self.owned_item_ids())
            .coins
    }
    /// Sells every eligible item, or nothing when the wallet would overflow.
    pub(crate) fn sell_unprotected(&mut self, refs: &SaleRefs) -> Result<u64, String> {
        self.preparation()?;
        let plan = self.plan_batch(refs, SaleOrigin::Player, self.owned_item_ids());
        self.execute_plan(&plan)
    }
    pub fn finish_attempt(&mut self) {
        if self.reserved == Some(Mode::Npc) {
            self.npc.attempts += 1;
        }
        self.reserved = None;
        self.attempts += 1;
    }
}
