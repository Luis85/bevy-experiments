//! Structural save invariants and untrusted partial-view validation.
use super::*;

impl LivingWorld {
    pub(crate) fn validate(
        &self,
        players: &BTreeMap<u64, crate::PlayerState>,
        catalog: Option<&Catalog>,
    ) -> Result<(), SandboxError> {
        if self.units.len() > MAX_LIVING_UNITS
            || self.accounts.len() > crate::MAX_PLAYERS
            || self.shops.len() > 8
            || self.producers.len() > MAX_LIVING_SITES
            || self.deliveries.len() > 16
            || self.roads.len() > MAX_LIVING_ROADS
        {
            return Err(SandboxError::Capacity);
        }
        if !finite_range(self.elapsed_seconds, 0., 1e12)
            || !finite_range(self.dispatch_accumulator, 0., 1.)
            || self.next_id < 1_000_000
            || self.next_id == u64::MAX
        {
            return Err(SandboxError::Invalid("living clock or sequence".into()));
        }
        if self.enabled != self.origin.is_some() {
            return Err(SandboxError::Invalid("living origin".into()));
        }
        if let Some(origin) = self.origin {
            validate_living_position(origin)?;
        }
        if !self.enabled
            && (!self.units.is_empty()
                || !self.accounts.is_empty()
                || !self.producers.is_empty()
                || !self.shops.is_empty()
                || !self.deliveries.is_empty())
        {
            return Err(SandboxError::Invalid(
                "disabled living state contains entities".into(),
            ));
        }
        let mut ids = BTreeSet::new();
        for (&id, unit) in &self.units {
            if id != unit.id
                || id >= self.next_id
                || !ids.insert(id)
                || unit.name.is_empty()
                || unit.name.len() > 64
                || unit.owner.is_some_and(|p| !players.contains_key(&p))
                || !unit.needs.validate()
                || unit.path.len() > 129
                || unit.patrol.len() > 512
                || unit.path_index > unit.path.len().max(unit.patrol.len())
                || unit.pocket_money > MAX_MONEY
                || !unit.heading_deg.is_finite()
                || unit.velocity_enu_m_s.iter().any(|v| !v.is_finite())
            {
                return Err(SandboxError::Invalid("living unit".into()));
            }
            validate_living_position(unit.position)?;
            validate_living_position(unit.home)?;
            for p in unit.path.iter().chain(&unit.patrol) {
                validate_living_position(*p)?;
            }
            if let UnitOrder::Move { destination } = unit.order {
                validate_living_position(destination)?;
            }
            validate_inventory(&unit.supplies)?;
        }
        for (&id, a) in &self.accounts {
            if id != a.player_id
                || !players.contains_key(&id)
                || a.money > MAX_MONEY
                || [a.avatar_unit, a.active_unit]
                    .iter()
                    .any(|u| self.units.get(u).is_none_or(|u| u.owner != Some(id)))
            {
                return Err(SandboxError::Invalid(
                    "living account or control ownership".into(),
                ));
            }
            if players[&id].position != self.units[&a.active_unit].position {
                return Err(SandboxError::Invalid(
                    "active body position differs from player context".into(),
                ));
            }
        }
        for (&id, shop) in &self.shops {
            if id != shop.id
                || id >= self.next_id
                || !ids.insert(id)
                || shop.name.is_empty()
                || shop.name.len() > 64
                || shop.money > MAX_MONEY
                || shop.prices.len() > crate::MAX_INVENTORY_ITEMS
                || shop.prices.iter().any(|(item, p)| {
                    !valid_id(item)
                        || p.buy == 0
                        || p.sell == 0
                        || p.buy > MAX_MONEY
                        || p.sell > p.buy
                })
                || shop.source_feature.as_ref().is_some_and(|id| !valid_id(id))
            {
                return Err(SandboxError::Invalid("shop".into()));
            }
            validate_living_position(shop.position)?;
            validate_inventory(&shop.stock)?;
        }
        for (&id, site) in &self.producers {
            if id != site.id
                || id >= self.next_id
                || !ids.insert(id)
                || site.name.is_empty()
                || site.name.len() > 64
                || site.owner.is_some_and(|p| !players.contains_key(&p))
                || !finite_range(site.cycle_seconds, 1., 86_400.)
                || !finite_range(site.progress_seconds, 0., site.cycle_seconds * 8.)
                || site.outputs.is_empty()
                || site.source_feature.as_ref().is_some_and(|id| !valid_id(id))
            {
                return Err(SandboxError::Invalid("production site".into()));
            }
            validate_living_position(site.position)?;
            for stock in [&site.stock, &site.inputs, &site.outputs] {
                validate_inventory(stock)?;
            }
        }
        let mut courier_ids = BTreeSet::new();
        for delivery in &self.deliveries {
            if delivery.id >= self.next_id
                || !ids.insert(delivery.id)
                || !courier_ids.insert(delivery.unit_id)
                || self
                    .units
                    .get(&delivery.unit_id)
                    .is_none_or(|u| u.role != UnitRole::Hauler)
                || !self.producers.contains_key(&delivery.from_site)
                || (!self.producers.contains_key(&delivery.to_site)
                    && !self.shops.contains_key(&delivery.to_site))
                || !valid_id(&delivery.item)
                || delivery.amount == 0
                || delivery.amount > 100
                || (delivery.phase == DeliveryPhase::ToPickup
                    && !self.units[&delivery.unit_id].supplies.is_empty())
                || (delivery.phase == DeliveryPhase::ToDropoff
                    && (self.units[&delivery.unit_id].supplies.len() != 1
                        || self.units[&delivery.unit_id].supplies.get(&delivery.item)
                            != Some(&delivery.amount)))
            {
                return Err(SandboxError::Invalid("delivery or reserved cargo".into()));
            }
        }
        if self.units.values().any(|unit| {
            unit.role == UnitRole::Hauler
                && !courier_ids.contains(&unit.id)
                && !unit.supplies.is_empty()
        }) {
            return Err(SandboxError::Invalid("unassigned courier cargo".into()));
        }
        let mut road_ids = BTreeSet::new();
        for road in &self.roads {
            if !valid_id(&road.id)
                || !road_ids.insert(&road.id)
                || road.points.len() < 2
                || road.points.len() > 64
                || !finite_range(road.width_m as f64, 2., 30.)
            {
                return Err(SandboxError::Invalid("road route".into()));
            }
            for p in &road.points {
                validate_living_position(*p)?;
            }
        }
        if let Some(catalog) = catalog {
            let inventories = self
                .units
                .values()
                .map(|u| &u.supplies)
                .chain(self.shops.values().map(|s| &s.stock))
                .chain(
                    self.producers
                        .values()
                        .flat_map(|s| [&s.stock, &s.inputs, &s.outputs]),
                );
            if inventories
                .into_iter()
                .any(|i| i.keys().any(|id| !catalog.items.contains(id)))
                || self
                    .shops
                    .values()
                    .any(|s| s.prices.keys().any(|id| !catalog.items.contains(id)))
            {
                return Err(SandboxError::Invalid("living catalog reference".into()));
            }
        }
        Ok(())
    }
}

impl LivingView {
    /// Validate an untrusted, deliberately incomplete replication view. Full
    /// ownership/source references remain an authoritative save concern.
    pub fn validate_for(&self, player_id: u64) -> Result<(), SandboxError> {
        if self.units.len() > 64
            || self.shops.len() > 8
            || self.producers.len() > 16
            || self.deliveries.len() > 16
            || self.roads.len() > 8
        {
            return Err(SandboxError::Capacity);
        }
        if !finite_range(self.elapsed_seconds, 0., 1e12) || self.enabled != self.origin.is_some() {
            return Err(SandboxError::Invalid("living view clock or origin".into()));
        }
        if !self.enabled {
            if self.account.is_some()
                || !self.units.is_empty()
                || !self.shops.is_empty()
                || !self.producers.is_empty()
                || !self.deliveries.is_empty()
                || !self.roads.is_empty()
            {
                return Err(SandboxError::Invalid("disabled living view".into()));
            }
            return Ok(());
        }
        validate_living_position(self.origin.unwrap())?;
        let account = self.account.as_ref().ok_or(SandboxError::PlayerMissing)?;
        if account.player_id != player_id || account.money > MAX_MONEY {
            return Err(SandboxError::Invalid("living view account".into()));
        }
        let mut ids = BTreeSet::new();
        for unit in &self.units {
            if unit.id == 0
                || !ids.insert(unit.id)
                || unit.name.is_empty()
                || unit.name.len() > 64
                || unit.name.chars().any(char::is_control)
                || unit.owner == Some(0)
                || !unit.needs.validate()
                || !unit.heading_deg.is_finite()
                || unit.velocity_enu_m_s.iter().any(|v| !v.is_finite())
                || unit
                    .velocity_enu_m_s
                    .iter()
                    .map(|v| v * v)
                    .sum::<f64>()
                    .sqrt()
                    > unit.kind.speed_limit_m_s()
                || !unit.path.is_empty()
                || unit.path_index != 0
                || !unit.patrol.is_empty()
                || !unit.supplies.is_empty()
                || unit.pocket_money != 0
            {
                return Err(SandboxError::Invalid("living view unit".into()));
            }
            validate_living_position(unit.position)?;
            validate_living_position(unit.home)?;
            if let UnitOrder::Move { destination } = unit.order {
                validate_living_position(destination)?;
            }
        }
        if [account.avatar_unit, account.active_unit].iter().any(|id| {
            !self
                .units
                .iter()
                .any(|u| u.id == *id && u.owner == Some(player_id))
        }) {
            return Err(SandboxError::Invalid("living view active body".into()));
        }
        for shop in &self.shops {
            if shop.id == 0
                || !ids.insert(shop.id)
                || shop.name.is_empty()
                || shop.name.len() > 64
                || shop.name.chars().any(char::is_control)
                || shop.money > MAX_MONEY
                || shop.prices.len() > crate::MAX_INVENTORY_ITEMS
                || shop.prices.iter().any(|(id, p)| {
                    !valid_id(id)
                        || p.buy == 0
                        || p.sell == 0
                        || p.sell > p.buy
                        || p.buy > MAX_MONEY
                })
                || shop.source_feature.as_ref().is_some_and(|id| !valid_id(id))
            {
                return Err(SandboxError::Invalid("living view shop".into()));
            }
            validate_living_position(shop.position)?;
            validate_inventory(&shop.stock)?;
        }
        for site in &self.producers {
            if site.id == 0
                || !ids.insert(site.id)
                || site.name.is_empty()
                || site.name.len() > 64
                || site.name.chars().any(char::is_control)
                || site.owner == Some(0)
                || !finite_range(site.cycle_seconds, 1., 86_400.)
                || !finite_range(site.progress_seconds, 0., site.cycle_seconds * 8.)
                || site.outputs.is_empty()
                || site.source_feature.as_ref().is_some_and(|id| !valid_id(id))
            {
                return Err(SandboxError::Invalid("living view production".into()));
            }
            validate_living_position(site.position)?;
            for stock in [&site.stock, &site.inputs, &site.outputs] {
                validate_inventory(stock)?;
            }
        }
        let mut couriers = BTreeSet::new();
        for delivery in &self.deliveries {
            if delivery.id == 0
                || !ids.insert(delivery.id)
                || delivery.unit_id == 0
                || !couriers.insert(delivery.unit_id)
                || delivery.from_site == 0
                || delivery.to_site == 0
                || delivery.from_site == delivery.to_site
                || !valid_id(&delivery.item)
                || delivery.amount == 0
                || delivery.amount > 100
            {
                return Err(SandboxError::Invalid("living view delivery".into()));
            }
        }
        let mut roads = BTreeSet::new();
        for road in &self.roads {
            if !valid_id(&road.id)
                || !roads.insert(&road.id)
                || road.points.len() < 2
                || road.points.len() > 32
                || !finite_range(road.width_m as f64, 2., 30.)
            {
                return Err(SandboxError::Invalid("living view road".into()));
            }
            for position in &road.points {
                validate_living_position(*position)?;
            }
        }
        Ok(())
    }
}
