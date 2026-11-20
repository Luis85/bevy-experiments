//! Prepare bounded inventory/money changes before committing any ownership mutation.
use super::*;

impl SandboxState {
    pub fn check_settlement_land(
        &self,
        player: u64,
        position: GeoPosition,
        radius: f64,
    ) -> Result<(), SandboxError> {
        position.validate()?;
        if !finite_range(radius, 0., 1000.) {
            return Err(SandboxError::Invalid("territory footprint".into()));
        }
        if self.settlements.territories.values().any(|t| {
            t.center.surface_distance_m(position) < t.tier.radius_m() + radius
                && !self.may_build(player, t.faction_owner, t.founder)
        }) {
            return Err(SandboxError::NotOwner);
        }
        Ok(())
    }

    /// Extraction on unplotted territory is personal or available to current
    /// faction members. Existing private/work-plot rules remain an additional gate.
    pub fn check_settlement_interact(
        &self,
        player: u64,
        position: GeoPosition,
    ) -> Result<(), SandboxError> {
        position.validate()?;
        if self
            .settlements
            .territories
            .values()
            .any(|t| t.contains(position, 0.) && !self.may_use(player, t.faction_owner, t.founder))
        {
            return Err(SandboxError::NotOwner);
        }
        Ok(())
    }

    /// A transfer may not create new principals with rights inside another
    /// principal's territory. Personal territory must remain personally held.
    pub fn check_settlement_transfer(
        &self,
        player: u64,
        position: GeoPosition,
        radius: f64,
        faction: u64,
    ) -> Result<(), SandboxError> {
        self.check_settlement_land(player, position, radius)?;
        if self.settlements.territories.values().any(|t| {
            t.center.surface_distance_m(position) < t.tier.radius_m() + radius
                && t.faction_owner != Some(faction)
        }) {
            return Err(SandboxError::NotOwner);
        }
        Ok(())
    }

    /// A separate admission hook lets existing plot rules retain their bounds
    /// while settlement tiers provide a meaningful regional plot capacity.
    pub fn check_settlement_plot_capacity(
        &self,
        player: u64,
        position: GeoPosition,
        radius: f64,
    ) -> Result<(), SandboxError> {
        self.check_settlement_land(player, position, radius)?;
        for t in self
            .settlements
            .territories
            .values()
            .filter(|t| t.contains(position, radius))
        {
            if self
                .plots
                .values()
                .filter(|p| t.contains(p.center, p.radius_m))
                .count()
                >= t.tier.plot_capacity()
            {
                return Err(SandboxError::Capacity);
            }
        }
        Ok(())
    }

    /// Newly claimed plots inside a shared territory inherit its faction;
    /// this makes houses and crops follow the same revocable roles.
    pub fn settlement_plot_faction(
        &self,
        player: u64,
        position: GeoPosition,
        radius: f64,
    ) -> Result<Option<u64>, SandboxError> {
        self.check_settlement_plot_capacity(player, position, radius)?;
        for t in self.settlements.territories.values().filter(|t| {
            t.faction_owner.is_some()
                && t.center.surface_distance_m(position) < t.tier.radius_m() + radius
        }) {
            if !t.contains(position, radius) {
                return Err(SandboxError::Invalid(
                    "plot crosses a shared territory boundary; move inward or reduce its radius"
                        .into(),
                ));
            }
            return Ok(t.faction_owner);
        }
        Ok(None)
    }

    pub fn settlement_readiness(
        &self,
        player: u64,
        id: u64,
    ) -> Result<SettlementReadiness, SandboxError> {
        let t = self
            .settlements
            .territories
            .get(&id)
            .ok_or_else(|| SandboxError::Missing("settlement".into()))?;
        let houses = self
            .structures
            .values()
            .filter(|s| {
                s.housing
                    && t.contains(s.position, s.radius_m)
                    && self.may_build(player, s.faction_owner, s.owner)
            })
            .count();
        let farms = self
            .farms
            .values()
            .filter(|f| {
                t.contains(f.position, 0.5) && self.may_build(player, f.faction_owner, f.owner)
            })
            .count();
        let plots = self
            .plots
            .values()
            .filter(|p| t.contains(p.center, p.radius_m))
            .count();
        Ok(SettlementReadiness {
            next: t.tier.upgrade_cost(),
            houses,
            farms,
            plots,
            experience: self
                .settlements
                .progress
                .get(&player)
                .map_or(0, |p| p.experience),
        })
    }

    /// Player intent requires an active session even when an SDK caller uses
    /// this direct entry point instead of `apply`.
    pub fn apply_settlement(
        &mut self,
        player: u64,
        command: SettlementCommand,
        catalog: &Catalog,
    ) -> Result<Outcome, SandboxError> {
        if self.is_player_dormant(player) {
            return Err(SandboxError::Invalid(
                "resume the owner's session before issuing settlement commands".into(),
            ));
        }
        let position = self
            .players
            .get(&player)
            .ok_or(SandboxError::PlayerMissing)?
            .position;
        if !self.living.enabled || !self.living.accounts.contains_key(&player) {
            return Err(SandboxError::Invalid(
                "settlement economy requires an active living account".into(),
            ));
        }
        match command {
            SettlementCommand::Found {
                name,
                center,
                faction_owner,
            } => {
                require_reach(position, center)?;
                if !valid_name(&name) {
                    return Err(SandboxError::Invalid(
                        "settlement name must be 1..48 printable characters, trimmed".into(),
                    ));
                }
                if self.settlements.territories.len() >= MAX_SETTLEMENTS
                    || self
                        .settlements
                        .territories
                        .values()
                        .filter(|t| t.founder == player)
                        .count()
                        >= 4
                {
                    return Err(SandboxError::Capacity);
                }
                if !self.may_build(player, faction_owner, player) {
                    return Err(SandboxError::NotOwner);
                }
                let tier = SettlementTier::Neighborhood;
                self.check_new_territory(player, faction_owner, center, tier.radius_m(), None)?;
                let costs = tier.material_cost();
                check_items(&costs, catalog)?;
                let inventory = exchange(
                    &self.players[&player].inventory,
                    &costs,
                    &Inventory::new(),
                    1,
                )?;
                let money = self.living.accounts[&player]
                    .money
                    .checked_sub(tier.credit_cost())
                    .ok_or_else(|| {
                        SandboxError::Insufficient("credits for the founding treasury".into())
                    })?;
                let (id, next) = next_id(self.settlements.next_id)?;
                self.players.get_mut(&player).unwrap().inventory = inventory;
                self.living.accounts.get_mut(&player).unwrap().money = money;
                self.settlements.territories.insert(
                    id,
                    Settlement {
                        id,
                        name: name.clone(),
                        founder: player,
                        faction_owner,
                        center,
                        tier,
                        stock: Inventory::new(),
                        treasury: tier.credit_cost(),
                    },
                );
                self.settlements.next_id = next;
                let p = self
                    .settlements
                    .progress
                    .entry(player)
                    .or_insert(PlayerProgress {
                        player_id: player,
                        ..Default::default()
                    });
                p.settlements_founded = p.settlements_founded.saturating_add(1);
                p.experience = p.experience.saturating_add(10).min(MAX_EXPERIENCE);
                Ok(Outcome {
                    message: format!(
                        "Founded {name}: 100 m neighborhood, 50.00 credits in its treasury"
                    ),
                    created_id: Some(id),
                })
            }
            SettlementCommand::Contribute {
                settlement_id,
                item,
                amount,
            } => {
                if !valid_id(&item)
                    || !catalog.items.contains(&item)
                    || amount == 0
                    || amount > 1000
                {
                    return Err(SandboxError::Invalid(
                        "contribution item or quantity".into(),
                    ));
                }
                let t = self.operable_settlement(player, settlement_id, position)?;
                let required = t.tier.upgrade_cost().ok_or_else(|| {
                    SandboxError::Invalid("city public works are complete".into())
                })?;
                if !required.materials.contains_key(&item) {
                    return Err(SandboxError::Invalid(
                        "this item is not required by the next public-works project".into(),
                    ));
                }
                let old = t.stock.get(&item).copied().unwrap_or(0);
                let value = old.checked_add(amount).ok_or(SandboxError::Capacity)?;
                if value > required.materials[&item] {
                    return Err(SandboxError::Invalid(
                        "contribution exceeds the remaining project requirement".into(),
                    ));
                }
                if t.stock.values().map(|n| u64::from(*n)).sum::<u64>() + u64::from(amount)
                    > MAX_SETTLEMENT_STOCK
                {
                    return Err(SandboxError::Capacity);
                }
                let next = exchange(
                    &self.players[&player].inventory,
                    &inventory(&[(&item, amount)]),
                    &Inventory::new(),
                    1,
                )?;
                self.players.get_mut(&player).unwrap().inventory = next;
                self.settlements
                    .territories
                    .get_mut(&settlement_id)
                    .unwrap()
                    .stock
                    .insert(item.clone(), value);
                Ok(outcome(format!(
                    "Contributed {amount} {item} to public works"
                )))
            }
            SettlementCommand::Fund {
                settlement_id,
                amount,
            } => {
                if amount == 0 || amount > MAX_MONEY {
                    return Err(SandboxError::Invalid("treasury funding amount".into()));
                }
                let t = self.operable_settlement(player, settlement_id, position)?;
                let treasury = t
                    .treasury
                    .checked_add(amount)
                    .filter(|n| *n <= MAX_MONEY)
                    .ok_or(SandboxError::Capacity)?;
                let money = self.living.accounts[&player]
                    .money
                    .checked_sub(amount)
                    .ok_or_else(|| SandboxError::Insufficient("credits".into()))?;
                self.living.accounts.get_mut(&player).unwrap().money = money;
                self.settlements
                    .territories
                    .get_mut(&settlement_id)
                    .unwrap()
                    .treasury = treasury;
                Ok(outcome(format!(
                    "Funded treasury with {:.2} credits",
                    amount as f64 / 100.
                )))
            }
            SettlementCommand::Upgrade { settlement_id } => {
                let t = self.operable_settlement(player, settlement_id, position)?;
                let ready = self.settlement_readiness(player, settlement_id)?;
                let cost = ready
                    .next
                    .ok_or_else(|| SandboxError::Invalid("settlement is already a city".into()))?;
                if ready.experience < cost.experience {
                    return Err(SandboxError::Insufficient(format!(
                        "community experience: {} required",
                        cost.experience
                    )));
                }
                if ready.houses < cost.houses || ready.farms < cost.farms {
                    return Err(SandboxError::Insufficient(format!(
                        "infrastructure: {} houses and {} farms required inside the current territory",
                        cost.houses, cost.farms
                    )));
                }
                self.check_new_territory(
                    t.founder,
                    t.faction_owner,
                    t.center,
                    cost.tier.radius_m(),
                    Some(t.id),
                )?;
                check_items(&cost.materials, catalog)?;
                let stock = exchange(&t.stock, &cost.materials, &Inventory::new(), 1)?;
                let treasury = t
                    .treasury
                    .checked_sub(cost.credits)
                    .ok_or_else(|| SandboxError::Insufficient("settlement treasury".into()))?;
                let shop = self
                    .living
                    .shops
                    .values()
                    .filter(|s| {
                        s.money
                            .checked_add(cost.credits)
                            .is_some_and(|m| m <= MAX_MONEY)
                    })
                    .min_by(|a, b| {
                        t.center
                            .surface_distance_m(a.position)
                            .total_cmp(&t.center.surface_distance_m(b.position))
                    })
                    .map(|s| s.id)
                    .ok_or_else(|| {
                        SandboxError::Invalid("no market can receive the charter payment".into())
                    })?;
                self.living.shops.get_mut(&shop).unwrap().money += cost.credits;
                let t = self
                    .settlements
                    .territories
                    .get_mut(&settlement_id)
                    .unwrap();
                t.stock = stock;
                t.treasury = treasury;
                t.tier = cost.tier;
                Ok(outcome(format!(
                    "{} is now a {}: {:.0} m territory, {} plot capacity",
                    t.name,
                    t.tier.label(),
                    t.tier.radius_m(),
                    t.tier.plot_capacity()
                )))
            }
            SettlementCommand::AcceptContract { contract_id } => {
                self.prepare_supply_acceptance(player, contract_id)
                    .map_err(SupplyContractBlocker::into_sandbox_error)?;
                let c = self.settlements.contracts.get_mut(&contract_id).unwrap();
                c.claimed_by = Some(player);
                c.expires_at = (self.settlements.elapsed_seconds
                    + self.settlements.contract_duration_seconds)
                    .min(1e12);
                self.settlements
                    .progress
                    .entry(player)
                    .or_insert(PlayerProgress {
                        player_id: player,
                        ..Default::default()
                    });
                Ok(outcome(
                    "Contract accepted. Deliver the requested goods to its destination.",
                ))
            }
            SettlementCommand::CancelContract { contract_id } => {
                self.prepare_supply_return(player, contract_id)
                    .map_err(SupplyContractBlocker::into_sandbox_error)?;
                self.settlements
                    .contracts
                    .get_mut(&contract_id)
                    .unwrap()
                    .claimed_by = None;
                Ok(outcome(
                    "Request returned. Your goods remain yours; the noticeboard will review demand and retain or refund the issuer's reserved reward.",
                ))
            }
            SettlementCommand::FulfillContract { contract_id } => {
                let prepared = self
                    .prepare_supply_delivery(player, contract_id, catalog)
                    .map_err(SupplyContractBlocker::into_sandbox_error)?;
                let c = prepared.contract;
                self.players.get_mut(&player).unwrap().inventory = prepared.player_stock;
                if let Some(site) = self.living.producers.get_mut(&c.target_site) {
                    site.stock = prepared.receiver_stock;
                } else {
                    self.living.shops.get_mut(&c.target_site).unwrap().stock =
                        prepared.receiver_stock;
                }
                self.living.accounts.get_mut(&player).unwrap().money = prepared.wallet;
                let p = self
                    .settlements
                    .progress
                    .entry(player)
                    .or_insert(PlayerProgress {
                        player_id: player,
                        ..Default::default()
                    });
                p.experience = p
                    .experience
                    .saturating_add(c.experience)
                    .min(MAX_EXPERIENCE);
                p.contracts_completed = p.contracts_completed.saturating_add(1);
                self.settlements.contracts.remove(&contract_id);
                Ok(outcome(format!(
                    "Delivered {} {} · earned {:.2} credits and {} experience",
                    c.amount,
                    c.item,
                    c.reward as f64 / 100.,
                    c.experience
                )))
            }
        }
    }

    fn operable_settlement(
        &self,
        player: u64,
        id: u64,
        position: GeoPosition,
    ) -> Result<&Settlement, SandboxError> {
        let t = self
            .settlements
            .territories
            .get(&id)
            .ok_or_else(|| SandboxError::Missing("settlement".into()))?;
        if !self.may_build(player, t.faction_owner, t.founder) {
            return Err(SandboxError::NotOwner);
        }
        require_reach(position, t.center)?;
        Ok(t)
    }
    fn check_new_territory(
        &self,
        founder: u64,
        faction: Option<u64>,
        center: GeoPosition,
        radius: f64,
        except: Option<u64>,
    ) -> Result<(), SandboxError> {
        if self.settlements.territories.values().any(|t| {
            Some(t.id) != except && t.center.surface_distance_m(center) < t.tier.radius_m() + radius
        }) {
            return Err(SandboxError::Overlap);
        }
        // A large new charter cannot confiscate prior personal/faction plots.
        if self.plots.values().any(|p| {
            center.surface_distance_m(p.center) < radius + p.radius_m
                && !(p.faction_owner == faction && (faction.is_some() || p.owner == founder))
        }) {
            return Err(SandboxError::Invalid(
                "territory would cover another owner's existing property".into(),
            ));
        }
        Ok(())
    }
}

pub(super) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.len() <= 96
        && name.chars().count() <= 48
        && name.chars().all(|c| !c.is_control())
}
pub(super) fn require_reach(a: GeoPosition, b: GeoPosition) -> Result<(), SandboxError> {
    b.validate()?;
    if !finite_range(b.altitude_m, -12000., 100000.) {
        return Err(SandboxError::Invalid("settlement position altitude".into()));
    }
    if a.surface_distance_m(b).hypot(a.altitude_m - b.altitude_m) > crate::INTERACTION_REACH_M {
        Err(SandboxError::OutOfReach)
    } else {
        Ok(())
    }
}
pub(super) fn next_id(id: u64) -> Result<(u64, u64), SandboxError> {
    if !(2_000_000..u64::MAX - 1).contains(&id) {
        return Err(SandboxError::Capacity);
    }
    Ok((id, id + 1))
}
fn check_items(items: &Inventory, catalog: &Catalog) -> Result<(), SandboxError> {
    if items.keys().any(|i| !catalog.items.contains(i)) {
        Err(SandboxError::Invalid(
            "settlement project requires items absent from this catalog".into(),
        ))
    } else {
        Ok(())
    }
}
fn outcome(message: impl Into<String>) -> Outcome {
    Outcome {
        message: message.into(),
        created_id: None,
    }
}
