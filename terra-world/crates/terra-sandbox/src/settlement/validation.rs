//! Full-state and deliberately partial-view invariants are checked separately.
use super::*;

fn validate_territory(t: &Settlement) -> Result<(), SandboxError> {
    if t.id < 2_000_000
        || t.founder == 0
        || t.faction_owner == Some(0)
        || !super::commands::valid_name(&t.name)
        || t.treasury > MAX_MONEY
    {
        return Err(SandboxError::Invalid(
            "settlement identity, name or treasury".into(),
        ));
    }
    t.center.validate()?;
    if !finite_range(t.center.altitude_m, -12000., 100000.) {
        return Err(SandboxError::Invalid("settlement altitude".into()));
    }
    validate_inventory(&t.stock)?;
    if t.stock.values().map(|n| u64::from(*n)).sum::<u64>() > MAX_SETTLEMENT_STOCK {
        return Err(SandboxError::Capacity);
    }
    let requirements = t.tier.upgrade_cost();
    if t.stock.iter().any(|(item, n)| {
        requirements
            .as_ref()
            .is_none_or(|r| r.materials.get(item).is_none_or(|max| n > max))
    }) {
        return Err(SandboxError::Invalid("settlement project stock".into()));
    }
    Ok(())
}
fn validate_contract(c: &SupplyContract) -> Result<(), SandboxError> {
    if c.id < 2_000_000
        || c.issuer_shop == 0
        || c.target_site == 0
        || !valid_id(&c.item)
        || c.amount == 0
        || c.amount > 100
        || c.reward == 0
        || c.reward > MAX_MONEY
        || c.experience == 0
        || c.experience > 1000
        || c.claimed_by == Some(0)
        || !finite_range(c.expires_at, 0., 1e12)
    {
        return Err(SandboxError::Invalid("supply contract".into()));
    }
    c.target_position.validate()?;
    if !finite_range(c.target_position.altitude_m, -12000., 100000.) {
        return Err(SandboxError::Invalid("contract altitude".into()));
    }
    Ok(())
}
fn validate_progress(p: &PlayerProgress) -> Result<(), SandboxError> {
    if p.player_id == 0 || p.experience > MAX_EXPERIENCE {
        Err(SandboxError::Invalid("player progress".into()))
    } else {
        Ok(())
    }
}

impl SettlementWorld {
    pub fn validate(
        &self,
        world: &SandboxState,
        catalog: Option<&Catalog>,
    ) -> Result<(), SandboxError> {
        if self.territories.len() > MAX_SETTLEMENTS
            || self.contracts.len() > MAX_SUPPLY_CONTRACTS
            || self.progress.len() > crate::MAX_PLAYERS
            || self.reference_prices.len() > 8
        {
            return Err(SandboxError::Capacity);
        }
        if !(2_000_000..u64::MAX).contains(&self.next_id)
            || !finite_range(self.elapsed_seconds, 0., 1e12)
            || !finite_range(self.update_accumulator, 0., 60.)
            || !finite_range(self.wage_accumulator, 0., 3600.)
            || !finite_range(self.contract_duration_seconds, 86_400., 31_536_000.)
            || self.last_paid_cycles > world.living.metrics.production_cycles
        {
            return Err(SandboxError::Invalid("settlement clock or sequence".into()));
        }
        let mut ids = BTreeSet::new();
        for (&id, t) in &self.territories {
            validate_territory(t)?;
            if id != t.id
                || id >= self.next_id
                || !ids.insert(id)
                || !world.players.contains_key(&t.founder)
                || t.faction_owner
                    .is_some_and(|id| !world.factions.contains_key(&id))
            {
                return Err(SandboxError::Invalid(
                    "settlement ownership reference".into(),
                ));
            }
            if let Some(catalog) = catalog {
                if t.stock.keys().any(|i| !catalog.items.contains(i)) {
                    return Err(SandboxError::Invalid("settlement catalog reference".into()));
                }
            }
        }
        let regions: Vec<_> = self.territories.values().collect();
        for (i, a) in regions.iter().enumerate() {
            for b in regions.iter().skip(i + 1) {
                if a.center.surface_distance_m(b.center) < a.tier.radius_m() + b.tier.radius_m() {
                    return Err(SandboxError::Overlap);
                }
            }
        }
        let mut requests = BTreeSet::new();
        let mut accepted = BTreeMap::<u64, usize>::new();
        for (&id, c) in &self.contracts {
            validate_contract(c)?;
            if id != c.id
                || id >= self.next_id
                || !ids.insert(id)
                || !world.living.shops.contains_key(&c.issuer_shop)
                || !requests.insert((c.target_site, &c.item))
                || c.claimed_by
                    .is_some_and(|id| !world.players.contains_key(&id))
            {
                return Err(SandboxError::Invalid(
                    "contract ownership or issuer reference".into(),
                ));
            }
            let target = world
                .living
                .producers
                .get(&c.target_site)
                .map(|s| s.position)
                .or_else(|| world.living.shops.get(&c.target_site).map(|s| s.position));
            if target != Some(c.target_position) {
                return Err(SandboxError::Invalid(
                    "contract target position reference".into(),
                ));
            }
            if let Some(catalog) = catalog {
                if !catalog.items.contains(&c.item) {
                    return Err(SandboxError::Invalid("contract catalog reference".into()));
                }
            }
            if let Some(player) = c.claimed_by {
                *accepted.entry(player).or_default() += 1;
            }
        }
        if accepted.values().any(|n| *n > MAX_ACTIVE_CONTRACTS) {
            return Err(SandboxError::Capacity);
        }
        for (&id, p) in &self.progress {
            validate_progress(p)?;
            if id != p.player_id || !world.players.contains_key(&id) {
                return Err(SandboxError::Invalid("progress ownership reference".into()));
            }
        }
        for (shop, prices) in &self.reference_prices {
            if !world.living.shops.contains_key(shop)
                || prices.len() > crate::MAX_INVENTORY_ITEMS
                || prices.iter().any(|(item, p)| {
                    !valid_id(item)
                        || p.buy == 0
                        || p.buy > MAX_MONEY
                        || p.sell == 0
                        || p.sell > p.buy
                })
            {
                return Err(SandboxError::Invalid("market reference prices".into()));
            }
            if let Some(catalog) = catalog {
                if prices.keys().any(|i| !catalog.items.contains(i)) {
                    return Err(SandboxError::Invalid(
                        "market price catalog reference".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}
impl SettlementView {
    pub fn validate_for(&self, player: u64) -> Result<(), SandboxError> {
        if self.territories.len() > MAX_SETTLEMENTS || self.contracts.len() > MAX_SUPPLY_CONTRACTS {
            return Err(SandboxError::Capacity);
        }
        if !finite_range(self.elapsed_seconds, 0., 1e12) {
            return Err(SandboxError::Invalid("settlement view clock".into()));
        }
        let mut ids = BTreeSet::new();
        for t in &self.territories {
            validate_territory(t)?;
            if !ids.insert(t.id) {
                return Err(SandboxError::Invalid("duplicate territory".into()));
            }
        }
        let mut requests = BTreeSet::new();
        let mut accepted = 0;
        for c in &self.contracts {
            validate_contract(c)?;
            if !ids.insert(c.id)
                || !requests.insert((c.target_site, &c.item))
                || c.claimed_by.is_some_and(|p| p != player)
            {
                return Err(SandboxError::Invalid(
                    "private or duplicate contract view".into(),
                ));
            }
            if c.claimed_by == Some(player) {
                accepted += 1;
            }
        }
        if accepted > MAX_ACTIVE_CONTRACTS {
            return Err(SandboxError::Capacity);
        }
        if let Some(p) = &self.progress {
            validate_progress(p)?;
            if p.player_id != player {
                return Err(SandboxError::Invalid("private progress view".into()));
            }
        }
        Ok(())
    }
}
