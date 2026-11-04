//! Coin and XP transactions: rewards, purchases, sales and run bookkeeping.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaleQuote {
    pub coins: u64,
    pub affected_presets: Vec<String>,
}

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
        if def.tier > self.unlocked {
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
    pub fn sale_preview(&self, id: u64) -> Result<SaleQuote, String> {
        self.preparation()?;
        let item = self.item(id).ok_or("Item is no longer owned")?;
        if self.reserved_items.contains(&id) {
            return Err("This item is deployed or assigned to a country guard".into());
        }
        if item.protected || item.locked || item.favorite {
            return Err("Remove item protection before selling".into());
        }
        Ok(SaleQuote {
            coins: self.item_sale_value(item.id)?,
            affected_presets: self
                .presets
                .iter()
                .filter(|p| p.loadout.contains(&id))
                .map(|p| p.name.clone())
                .collect(),
        })
    }
    pub fn sell(&mut self, id: u64) -> Result<u64, String> {
        let quote = self.sale_preview(id)?;
        let wallet = self
            .coins
            .checked_add(quote.coins)
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
        Ok(quote.coins)
    }
    pub fn sell_quote(&self) -> u64 {
        self.items
            .iter()
            .filter(|x| {
                !x.protected
                    && !x.locked
                    && !x.favorite
                    && !self.reserved_items.contains(&x.id)
                    && !self.player.loadout.contains(&x.id)
                    && !self.npc.loadout.contains(&x.id)
                    && !self.presets.iter().any(|p| p.loadout.contains(&x.id))
            })
            .map(|x| self.item_sale_value(x.id).expect("known owned item"))
            .sum()
    }
    pub fn sell_unprotected(&mut self) -> Result<u64, String> {
        self.preparation()?;
        self.coins
            .checked_add(self.sell_quote())
            .ok_or("Batch sale would exceed the wallet limit")?;
        let ids: Vec<_> = self
            .items
            .iter()
            .filter(|x| {
                !x.protected
                    && !x.locked
                    && !x.favorite
                    && !self.reserved_items.contains(&x.id)
                    && !self.player.loadout.contains(&x.id)
                    && !self.npc.loadout.contains(&x.id)
                    && !self.presets.iter().any(|p| p.loadout.contains(&x.id))
            })
            .map(|x| x.id)
            .collect();
        let mut total = 0;
        for id in ids {
            total += self.sell(id)?;
        }
        Ok(total)
    }
    pub fn finish_attempt(&mut self) {
        if self.reserved == Some(Mode::Npc) {
            self.npc.attempts += 1;
        }
        self.reserved = None;
        self.attempts += 1;
    }
}
