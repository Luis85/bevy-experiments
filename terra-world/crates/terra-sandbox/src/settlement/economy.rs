//! Bounded economy maintenance: funded work, stable price anchors, and earned NPC pay.
use super::*;

impl SandboxState {
    /// Call on the authoritative host with calendar seconds, after living
    /// production. A large step performs one bounded maintenance batch.
    pub fn advance_settlements(
        &mut self,
        sim_dt: f64,
        catalog: &Catalog,
    ) -> Result<(), SandboxError> {
        if !finite_range(sim_dt, 0., 86400.)
            || !finite_range(self.settlements.elapsed_seconds + sim_dt, 0., 1e12)
        {
            return Err(SandboxError::Invalid("settlement timestep".into()));
        }
        if !self.living.enabled {
            return Ok(());
        }
        self.settlements.elapsed_seconds += sim_dt;
        self.settlements.update_accumulator =
            (self.settlements.update_accumulator + sim_dt).min(86460.);
        self.settlements.wage_accumulator =
            (self.settlements.wage_accumulator + sim_dt).min(90000.);
        if self.settlements.update_accumulator >= 60. {
            self.settlements.update_accumulator =
                self.settlements.update_accumulator.rem_euclid(60.);
            self.maintain_public_supply();
            self.ensure_resource_markets(catalog);
            self.refresh_market_prices();
            self.post_supply_contracts(catalog);
        }
        if self.settlements.wage_accumulator >= 3600. {
            self.settlements.wage_accumulator = self.settlements.wage_accumulator.rem_euclid(3600.);
            self.pay_productive_residents();
        }
        Ok(())
    }

    fn maintain_public_supply(&mut self) {
        // Global contract IDs order the bounded batch, including competing
        // refunds into one nearly-full issuer. An accepted, unexpired promise
        // is never rewritten merely because stock or production needs changed.
        let ids: Vec<_> = self
            .settlements
            .contracts
            .keys()
            .copied()
            .take(MAX_SUPPLY_CONTRACTS)
            .collect();
        for id in ids {
            let contract = self.settlements.contracts.get_mut(&id).unwrap();
            let expired = contract.expires_at <= self.settlements.elapsed_seconds;
            if contract.claimed_by.is_some() {
                if !expired {
                    continue;
                }
                // Ordinary expiry releases the claim before public policy is
                // evaluated. No goods or progression are changed by expiry.
                contract.claimed_by = None;
            }
            let contract = &self.settlements.contracts[&id];
            match self.supply_demand(
                contract.target_site,
                contract.target_position,
                &contract.item,
            ) {
                super::demand::SupplyDemand::Obsolete => {
                    let issuer = contract.issuer_shop;
                    let Some(refunded) = self
                        .living
                        .shops
                        .get(&issuer)
                        .and_then(|shop| shop.money.checked_add(contract.reward))
                        .filter(|money| *money <= MAX_MONEY)
                    else {
                        // The exact escrow stays in the offer until its original
                        // issuer can receive it. Never delete value, split the
                        // refund, redirect funds or reset the deadline to hide it.
                        continue;
                    };
                    self.living.shops.get_mut(&issuer).unwrap().money = refunded;
                    self.settlements.contracts.remove(&id);
                }
                super::demand::SupplyDemand::Needed { .. } if expired => {
                    self.settlements.contracts.get_mut(&id).unwrap().expires_at =
                        (self.settlements.elapsed_seconds
                            + self.settlements.contract_duration_seconds)
                            .min(1e12);
                }
                // Unknown receiving data does not authorize retirement or a
                // renewed promise. Existing terms and escrow remain available
                // for the next authoritative maintenance observation.
                _ => {}
            }
        }
    }

    fn ensure_resource_markets(&mut self, catalog: &Catalog) {
        for shop in self
            .living
            .shops
            .values_mut()
            .filter(|s| s.authored && s.name == "Village exchange")
        {
            for (item, buy, sell) in [
                ("berries", 90, 45),
                ("fiber", 80, 40),
                ("fish", 180, 95),
                ("cooked_fish", 320, 170),
                ("iron_ore", 220, 120),
                ("iron_ingot", 480, 260),
                ("fishing_rod", 900, 450),
                ("iron_tool", 1800, 900),
                ("stone_blocks", 240, 120),
            ] {
                if catalog.items.contains(item) && shop.prices.len() < crate::MAX_INVENTORY_ITEMS {
                    shop.prices
                        .entry(item.into())
                        .or_insert(TradePrice { buy, sell });
                }
            }
        }
    }

    fn refresh_market_prices(&mut self) {
        for shop in self.living.shops.values_mut() {
            let anchors = self
                .settlements
                .reference_prices
                .entry(shop.id)
                .or_insert_with(|| shop.prices.clone());
            // New catalog items can be added by a host without compounding
            // prior prices. Existing anchors remain pinned in the save.
            for (item, price) in &shop.prices {
                anchors.entry(item.clone()).or_insert(*price);
            }
            for (item, anchor) in anchors.iter() {
                if !shop.prices.contains_key(item) {
                    continue;
                }
                let stock = shop.stock.get(item).copied().unwrap_or(0).min(120);
                let buy_factor = if stock < 40 {
                    100 + (40 - stock) * 50 / 40
                } else {
                    100 - ((stock - 40) * 20 / 80)
                };
                let sell_factor = if stock > 40 {
                    100 - ((stock - 40) * 30 / 80)
                } else {
                    100
                };
                // Across all stock levels, sell is below the lowest possible
                // buy quote. Repeated buy/sell repricing cannot mint profit.
                let buy =
                    (anchor.buy.saturating_mul(u64::from(buy_factor)) / 100).clamp(1, MAX_MONEY);
                let sell = (anchor
                    .sell
                    .min(anchor.buy.saturating_mul(65) / 100)
                    .saturating_mul(u64::from(sell_factor))
                    / 100)
                    .clamp(1, buy);
                shop.prices.insert(item.clone(), TradePrice { buy, sell });
            }
        }
    }

    fn post_supply_contracts(&mut self, catalog: &Catalog) {
        if self.settlements.contracts.len() >= MAX_SUPPLY_CONTRACTS {
            return;
        }
        let Some(issuer) = self.living.shops.keys().next().copied() else {
            return;
        };
        let prices = self.living.shops[&issuer].prices.clone();
        // Rank actual input shortages before retail replenishment. At most
        // four offers enter the board per minute of world time.
        let mut requests = Vec::new();
        for (target_site, target_position, item) in self
            .living
            .producers
            .values()
            .flat_map(|site| {
                site.inputs
                    .keys()
                    .map(move |item| (site.id, site.position, item))
            })
            .chain(self.living.shops.values().flat_map(|shop| {
                shop.prices
                    .keys()
                    .map(move |item| (shop.id, shop.position, item))
            }))
        {
            if !catalog.items.contains(item) || !prices.contains_key(item) {
                continue;
            }
            if let Some((priority, amount)) = self
                .supply_demand(target_site, target_position, item)
                .posting_amount(item)
            {
                requests.push((
                    priority,
                    target_site,
                    target_position,
                    item.clone(),
                    amount,
                    priority < 2,
                ));
            }
        }
        requests.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.3.cmp(&b.3)));
        let mut added = 0;
        for (_, target_site, target_position, item, amount, production) in requests {
            if added == 4 || self.settlements.contracts.len() >= MAX_SUPPLY_CONTRACTS {
                break;
            }
            if self
                .settlements
                .contracts
                .values()
                .any(|c| c.target_site == target_site && c.item == item)
            {
                continue;
            }
            // Retailers may have different assortments. Only the central
            // issuer's known prices can value its funded public requests;
            // an unsupported item consumes no offer slot, ID or escrow.
            let Some(price) = prices.get(&item) else {
                continue;
            };
            let reward = if production {
                let distance = self.living.shops[&issuer]
                    .position
                    .surface_distance_m(target_position);
                price
                    .buy
                    .saturating_mul(u64::from(amount))
                    .saturating_mul(115)
                    / 100
                    + (distance / 2.).clamp(0., 1000.) as u64
            } else {
                price
                    .sell
                    .saturating_mul(u64::from(amount))
                    .saturating_mul(110)
                    / 100
            }
            .max(1);
            if reward > MAX_MONEY || self.living.shops[&issuer].money < reward {
                continue;
            }
            let Ok((id, next)) = super::commands::next_id(self.settlements.next_id) else {
                break;
            };
            self.living.shops.get_mut(&issuer).unwrap().money -= reward;
            self.settlements.contracts.insert(
                id,
                SupplyContract {
                    id,
                    issuer_shop: issuer,
                    target_site,
                    target_position,
                    item,
                    amount,
                    reward,
                    experience: if production { 40 } else { 25 },
                    claimed_by: None,
                    expires_at: (self.settlements.elapsed_seconds
                        + self.settlements.contract_duration_seconds)
                        .min(1e12),
                },
            );
            self.settlements.next_id = next;
            added += 1;
        }
    }

    fn pay_productive_residents(&mut self) {
        let cycles = self.living.metrics.production_cycles;
        if cycles <= self.settlements.last_paid_cycles {
            return;
        }
        self.settlements.last_paid_cycles = cycles;
        let Some(issuer) = self.living.shops.keys().next().copied() else {
            return;
        };
        let candidates: Vec<_> = self
            .living
            .units
            .values()
            .filter(|u| u.owner.is_none() && u.role == UnitRole::Citizen && u.pocket_money < 1000)
            .map(|u| u.id)
            .collect();
        for id in candidates {
            let amount = 150_u64.min(self.living.shops[&issuer].money);
            if amount == 0 {
                break;
            }
            self.living.shops.get_mut(&issuer).unwrap().money -= amount;
            self.living.units.get_mut(&id).unwrap().pocket_money += amount;
            self.settlements.wages_paid = self.settlements.wages_paid.saturating_add(amount);
        }
    }
}
