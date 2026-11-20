//! Atomic owner actions and finite-stock exchanges.
use super::*;

struct PreparedPersonalTrade {
    quote: PersonalTradeQuote,
    player_stock: Inventory,
    shop_stock: Inventory,
    wallet: u64,
    till: u64,
}

impl SandboxState {
    /// Called only after host terrain/obstacle validation. No client command can
    /// submit an authoritative position. Manual input takes over a queued order.
    /// Dormant owners must resume before movement or stationary repositioning;
    /// trusted host placement uses the separate `set_player_position` API.
    pub fn set_controlled_motion(
        &mut self,
        player_id: u64,
        position: GeoPosition,
        velocity: [f64; 3],
        heading: f32,
    ) -> Result<(), SandboxError> {
        if self.is_player_dormant(player_id) {
            return Err(SandboxError::Invalid(
                "resume the owner's session before controlled motion".into(),
            ));
        }
        validate_living_position(position)?;
        if velocity.iter().any(|v| !v.is_finite()) || !heading.is_finite() {
            return Err(SandboxError::Invalid("controlled motion".into()));
        }
        let active = self
            .living
            .accounts
            .get(&player_id)
            .ok_or(SandboxError::PlayerMissing)?
            .active_unit;
        let unit = self
            .living
            .units
            .get_mut(&active)
            .ok_or_else(|| SandboxError::Missing("controlled body".into()))?;
        if unit.owner != Some(player_id) {
            return Err(SandboxError::NotOwner);
        }
        if velocity.iter().map(|v| v * v).sum::<f64>().sqrt() > unit.speed_limit_m_s() + 1e-5 {
            return Err(SandboxError::Invalid(
                "controlled speed exceeds current capability".into(),
            ));
        }
        unit.position = position;
        unit.velocity_enu_m_s = velocity;
        unit.heading_deg = heading;
        if velocity.iter().any(|v| v.abs() > 0.001) {
            unit.order = UnitOrder::Idle;
            unit.path.clear();
            unit.path_index = 0;
        }
        self.players
            .get_mut(&player_id)
            .ok_or(SandboxError::PlayerMissing)?
            .position = position;
        Ok(())
    }

    pub(crate) fn apply_living(
        &mut self,
        player: u64,
        command: LivingCommand,
        catalog: &Catalog,
    ) -> Result<Outcome, SandboxError> {
        if !self.living.enabled {
            return Err(SandboxError::Invalid("living world is not enabled".into()));
        }
        let account = self
            .living
            .accounts
            .get(&player)
            .ok_or(SandboxError::PlayerMissing)?
            .clone();
        let manual = matches!(&command, LivingCommand::Possess { .. });
        let reserved = match &command {
            LivingCommand::Possess { unit_id } | LivingCommand::SelectActive { unit_id } => {
                self.is_company_vehicle(*unit_id)
            }
            LivingCommand::MoveUnits { unit_ids, .. }
            | LivingCommand::StopUnits { unit_ids }
            | LivingCommand::Rest { unit_ids } => {
                unit_ids.iter().any(|id| self.is_company_vehicle(*id))
            }
            _ => false,
        };
        if reserved {
            return Err(SandboxError::Invalid(
                "pause the company route and release its vehicle before manual or RTS control"
                    .into(),
            ));
        }
        match command {
            LivingCommand::Possess { unit_id } | LivingCommand::SelectActive { unit_id } => {
                let unit = self
                    .living
                    .units
                    .get(&unit_id)
                    .ok_or_else(|| SandboxError::Missing("unit".into()))?;
                if unit.owner != Some(player) {
                    return Err(SandboxError::NotOwner);
                }
                if manual {
                    if let Some(vehicle) = self.transport.vehicles.get(&unit_id) {
                        if self.transport.drivers.get(&player).is_none_or(|driver| {
                            !driver
                                .licenses
                                .contains(&vehicle.model.spec().required_license)
                        }) {
                            return Err(SandboxError::Invalid(
                                "driver does not hold the required vehicle license".into(),
                            ));
                        }
                    }
                }
                let position = unit.position;
                let name = unit.name.clone();
                let a = self.living.accounts.get_mut(&player).unwrap();
                a.active_unit = unit_id;
                a.possessed = manual;
                if manual {
                    let unit = self.living.units.get_mut(&unit_id).unwrap();
                    unit.order = UnitOrder::Idle;
                    unit.path.clear();
                    unit.path_index = 0;
                    unit.velocity_enu_m_s = [0.; 3];
                    if let Some(vehicle) = self.transport.vehicles.get_mut(&unit_id) {
                        vehicle.autopilot = false;
                        vehicle.dynamics.stop();
                    }
                }
                self.players.get_mut(&player).unwrap().position = position;
                Ok(living_outcome(format!(
                    "{} {name}",
                    if manual { "Controlling" } else { "Selected" }
                )))
            }
            LivingCommand::Release => {
                let position = self
                    .living
                    .units
                    .get(&account.active_unit)
                    .ok_or_else(|| SandboxError::Missing("active body".into()))?
                    .position;
                if let Some(u) = self.living.units.get_mut(&account.active_unit) {
                    u.velocity_enu_m_s = [0.; 3];
                }
                if let Some(vehicle) = self.transport.vehicles.get_mut(&account.active_unit) {
                    vehicle.dynamics.stop();
                }
                let a = self.living.accounts.get_mut(&player).unwrap();
                a.possessed = false;
                self.players.get_mut(&player).unwrap().position = position;
                Ok(living_outcome("Command mode"))
            }
            LivingCommand::MoveUnits {
                unit_ids,
                destination,
            } => {
                validate_living_position(destination)?;
                validate_group(&self.living, player, &unit_ids)?;
                if unit_ids
                    .iter()
                    .filter_map(|id| self.transport.vehicles.get(id))
                    .any(|vehicle| {
                        self.transport.drivers.get(&player).is_none_or(|driver| {
                            !driver
                                .licenses
                                .contains(&vehicle.model.spec().required_license)
                        })
                    })
                {
                    return Err(SandboxError::Invalid(
                        "driver does not hold the required vehicle license".into(),
                    ));
                }
                if unit_ids.iter().any(|id| {
                    self.living.units[id]
                        .position
                        .surface_distance_m(destination)
                        > 2000.
                }) {
                    return Err(SandboxError::Invalid("orders are limited to 2 km".into()));
                }
                let cols = (unit_ids.len() as f64).sqrt().ceil() as usize;
                let frame = LocalFrame::new(destination);
                let routes = unit_ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        let target = frame
                            .to_geo([(i % cols) as f64 * 3., 0., (i / cols) as f64 * 3.])
                            .unwrap();
                        (
                            *id,
                            target,
                            if matches!(
                                self.living.units[id].kind,
                                ActorKind::Helicopter | ActorKind::Balloon | ActorKind::Airplane
                            ) {
                                vec![target]
                            } else if self.living.units[id].kind == ActorKind::Vehicle {
                                self.living
                                    .vehicle_route_between(self.living.units[id].position, target)
                            } else if matches!(
                                self.living.units[id].kind,
                                ActorKind::Human | ActorKind::Creature
                            ) {
                                self.living.pedestrian_route_between(
                                    self.living.units[id].position,
                                    target,
                                )
                            } else {
                                self.living
                                    .route_between(self.living.units[id].position, target)
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                for (id, target, path) in routes {
                    let unit = self.living.units.get_mut(&id).unwrap();
                    unit.order = UnitOrder::Move {
                        destination: target,
                    };
                    unit.path = path;
                    unit.path_index = 0;
                    // A new, admitted RTS move is explicit driving intent,
                    // just like road autopilot. Keep release/exit parking in
                    // place until this commit; rejected groups change no brake.
                    if let Some(vehicle) = self.transport.vehicles.get_mut(&id) {
                        vehicle.parking_brake = false;
                    }
                }
                if unit_ids.contains(&account.active_unit) {
                    self.living.accounts.get_mut(&player).unwrap().possessed = false;
                }
                Ok(living_outcome(format!(
                    "Move ordered for {} units",
                    unit_ids.len()
                )))
            }
            LivingCommand::StopUnits { unit_ids } => {
                validate_group(&self.living, player, &unit_ids)?;
                for id in &unit_ids {
                    let unit = self.living.units.get_mut(id).unwrap();
                    unit.order = UnitOrder::Idle;
                    unit.velocity_enu_m_s = [0.; 3];
                    unit.path.clear();
                    unit.path_index = 0;
                    if let Some(vehicle) = self.transport.vehicles.get_mut(id) {
                        vehicle.autopilot = false;
                        vehicle.dynamics.stop();
                    }
                }
                Ok(living_outcome(format!("Stopped {} units", unit_ids.len())))
            }
            LivingCommand::Rest { unit_ids } => {
                validate_group(&self.living, player, &unit_ids)?;
                for id in &unit_ids {
                    let unit = self.living.units.get_mut(id).unwrap();
                    unit.order = UnitOrder::Rest;
                    unit.velocity_enu_m_s = [0.; 3];
                    unit.path.clear();
                    unit.path_index = 0;
                    if let Some(vehicle) = self.transport.vehicles.get_mut(id) {
                        vehicle.autopilot = false;
                        vehicle.dynamics.stop();
                    }
                }
                Ok(living_outcome(format!(
                    "Rest ordered for {} units",
                    unit_ids.len()
                )))
            }
            LivingCommand::Buy {
                shop_id,
                item,
                amount,
            } => self.trade(player, shop_id, item, amount, (true, None), catalog),
            LivingCommand::Sell {
                shop_id,
                item,
                amount,
            } => self.trade(player, shop_id, item, amount, (false, None), catalog),
            LivingCommand::TradeQuoted {
                shop_id,
                item,
                amount,
                buy,
                quoted_total,
            } => self.trade(
                player,
                shop_id,
                item,
                amount,
                (buy, Some(quoted_total)),
                catalog,
            ),
            LivingCommand::Consume { item } => {
                if !matches!(item.as_str(), "bread" | "water" | "berries" | "cooked_fish") {
                    return Err(SandboxError::Invalid(
                        "consume bread, berries, cooked_fish or water".into(),
                    ));
                }
                let p = self.players.get(&player).unwrap();
                let next = exchange(
                    &p.inventory,
                    &inventory(&[(&item, 1)]),
                    &Inventory::new(),
                    1,
                )?;
                let unit = self
                    .living
                    .units
                    .get(&account.active_unit)
                    .ok_or_else(|| SandboxError::Missing("body".into()))?;
                if !unit.is_person() {
                    return Err(SandboxError::Invalid(
                        "this body cannot eat or drink".into(),
                    ));
                }
                if (item != "water" && unit.needs.food >= 0.98)
                    || (item == "water" && unit.needs.water >= 0.98)
                {
                    return Err(SandboxError::Invalid(
                        "this need is already satisfied".into(),
                    ));
                }
                let unit = self.living.units.get_mut(&account.active_unit).unwrap();
                let message = match item.as_str() {
                    "water" => {
                        unit.needs.water = (unit.needs.water + 0.6).min(1.);
                        "Drank water"
                    }
                    "berries" => {
                        unit.needs.food = (unit.needs.food + 0.20).min(1.);
                        unit.needs.water = (unit.needs.water + 0.06).min(1.);
                        "Ate berries"
                    }
                    "cooked_fish" => {
                        unit.needs.food = (unit.needs.food + 0.55).min(1.);
                        "Ate cooked fish"
                    }
                    _ => {
                        unit.needs.food = (unit.needs.food + 0.5).min(1.);
                        "Ate bread"
                    }
                };
                self.players.get_mut(&player).unwrap().inventory = next;
                Ok(living_outcome(message))
            }
            LivingCommand::SetProduction { site_id, enabled } => {
                let site = self
                    .living
                    .producers
                    .get(&site_id)
                    .ok_or_else(|| SandboxError::Missing("production site".into()))?;
                if site.owner != Some(player) {
                    return Err(SandboxError::NotOwner);
                }
                // Owned facilities are operated at their service point.
                require_living_reach(self.players[&player].position, site.position)?;
                self.living.producers.get_mut(&site_id).unwrap().enabled = enabled;
                Ok(living_outcome(if enabled {
                    "Production enabled"
                } else {
                    "Production paused"
                }))
            }
        }
    }

    /// Inspect exactly the same bounded exchange accepted by personal trading.
    /// No stock, money, metrics or quote reservation is changed by inspection.
    pub fn personal_trade_quote(
        &self,
        player: u64,
        shop_id: u64,
        item: &str,
        amount: u32,
        buy: bool,
        catalog: &Catalog,
    ) -> Result<PersonalTradeQuote, SandboxError> {
        self.prepare_personal_trade(player, shop_id, item, amount, buy, catalog)
            .map(|prepared| prepared.quote)
    }

    fn prepare_personal_trade(
        &self,
        player: u64,
        shop_id: u64,
        item: &str,
        amount: u32,
        buy: bool,
        catalog: &Catalog,
    ) -> Result<PreparedPersonalTrade, SandboxError> {
        if self.is_player_dormant(player) {
            return Err(SandboxError::Invalid(
                "resume the owner's session before trading".into(),
            ));
        }
        if !self.living.enabled {
            return Err(SandboxError::Invalid("living world is not enabled".into()));
        }
        if !valid_id(item) || !catalog.items.contains(item) || amount == 0 || amount > 100 {
            return Err(SandboxError::Invalid(
                "trade item or quantity (1..100)".into(),
            ));
        }
        let p = self
            .players
            .get(&player)
            .ok_or(SandboxError::PlayerMissing)?;
        let shop = self
            .living
            .shops
            .get(&shop_id)
            .ok_or_else(|| SandboxError::Missing("shop".into()))?;
        require_living_reach(p.position, shop.position)?;
        let price = shop
            .prices
            .get(item)
            .ok_or_else(|| SandboxError::Missing("shop price".into()))?;
        let price = if buy { price.buy } else { price.sell };
        let total = price
            .checked_mul(amount as u64)
            .ok_or(SandboxError::Capacity)?;
        let wallet = self
            .living
            .accounts
            .get(&player)
            .ok_or(SandboxError::PlayerMissing)?
            .money;
        let one = inventory(&[(item, amount)]);
        let (player_stock, shop_stock, new_wallet, new_till) = if buy {
            if wallet < total {
                return Err(SandboxError::Insufficient("credits".into()));
            }
            (
                exchange(&p.inventory, &Inventory::new(), &one, 1)?,
                exchange(&shop.stock, &one, &Inventory::new(), 1)?,
                wallet - total,
                shop.money
                    .checked_add(total)
                    .filter(|v| *v <= MAX_MONEY)
                    .ok_or(SandboxError::Capacity)?,
            )
        } else {
            if shop.money < total {
                return Err(SandboxError::Insufficient("shop credits".into()));
            }
            (
                exchange(&p.inventory, &one, &Inventory::new(), 1)?,
                exchange(&shop.stock, &Inventory::new(), &one, 1)?,
                wallet
                    .checked_add(total)
                    .filter(|v| *v <= MAX_MONEY)
                    .ok_or(SandboxError::Capacity)?,
                shop.money - total,
            )
        };
        Ok(PreparedPersonalTrade {
            quote: PersonalTradeQuote {
                shop_id,
                item: item.into(),
                amount,
                buy,
                total_cost: total,
            },
            player_stock,
            shop_stock,
            wallet: new_wallet,
            till: new_till,
        })
    }

    fn trade(
        &mut self,
        player: u64,
        shop_id: u64,
        item: String,
        amount: u32,
        (buy, expected_total): (bool, Option<u64>),
        catalog: &Catalog,
    ) -> Result<Outcome, SandboxError> {
        if expected_total.is_some_and(|quoted| quoted > MAX_MONEY) {
            return Err(SandboxError::Capacity);
        }
        let prepared = self.prepare_personal_trade(player, shop_id, &item, amount, buy, catalog)?;
        let total = prepared.quote.total_cost;
        if expected_total.is_some_and(|quoted| quoted != total) {
            return Err(SandboxError::Invalid(
                "market quote changed; review the current price and try again".into(),
            ));
        }
        self.players.get_mut(&player).unwrap().inventory = prepared.player_stock;
        self.living.accounts.get_mut(&player).unwrap().money = prepared.wallet;
        let shop = self.living.shops.get_mut(&shop_id).unwrap();
        shop.stock = prepared.shop_stock;
        shop.money = prepared.till;
        self.living.metrics.units_traded = self
            .living
            .metrics
            .units_traded
            .saturating_add(amount as u64);
        self.living.metrics.credits_traded =
            self.living.metrics.credits_traded.saturating_add(total);
        Ok(living_outcome(format!(
            "{} {amount} {item} for {}.{:02} credits",
            if buy { "Bought" } else { "Sold" },
            total / 100,
            total % 100
        )))
    }
}

fn require_living_reach(a: GeoPosition, b: GeoPosition) -> Result<(), SandboxError> {
    if a.surface_distance_m(b).hypot(a.altitude_m - b.altitude_m) > LIVING_INTERACTION_REACH_M {
        Err(SandboxError::OutOfReach)
    } else {
        Ok(())
    }
}
fn validate_group(world: &LivingWorld, player: u64, ids: &[u64]) -> Result<(), SandboxError> {
    if ids.is_empty()
        || ids.len() > MAX_GROUP_ORDER
        || ids.iter().copied().collect::<BTreeSet<_>>().len() != ids.len()
    {
        return Err(SandboxError::Invalid("select 1..32 unique units".into()));
    }
    if ids
        .iter()
        .any(|id| world.units.get(id).is_none_or(|u| u.owner != Some(player)))
    {
        return Err(SandboxError::NotOwner);
    }
    Ok(())
}
fn living_outcome(message: impl Into<String>) -> Outcome {
    Outcome {
        message: message.into(),
        created_id: None,
    }
}
