//! Fixed-step needs, movement, production, dispatch and material transfer.
use super::*;

impl SandboxState {
    /// Host calls this once per fixed step. It never reads wall time or random
    /// state. Movement uses real seconds; needs/production use simulation time.
    pub fn advance_living(
        &mut self,
        real_dt: f64,
        sim_dt: f64,
        catalog: &Catalog,
    ) -> Result<(), SandboxError> {
        self.advance_living_with_environment(real_dt, sim_dt, catalog, |_, _, candidate| {
            Some(candidate)
        })
    }

    /// Resolve terrain and collision BEFORE movement commits and deliveries
    /// inspect arrival. Unknown/blocked destinations return None and stop the unit.
    pub fn advance_living_with_environment(
        &mut self,
        real_dt: f64,
        sim_dt: f64,
        catalog: &Catalog,
        resolve: impl Fn(ActorKind, GeoPosition, GeoPosition) -> Option<GeoPosition>,
    ) -> Result<(), SandboxError> {
        if !finite_range(real_dt, 0., 5.) || !finite_range(sim_dt, 0., 86_400.) {
            return Err(SandboxError::Invalid("living timestep".into()));
        }
        if !self.living.enabled {
            return Ok(());
        }
        self.living.elapsed_seconds = (self.living.elapsed_seconds + sim_dt).min(1e12);
        let possessed: BTreeSet<_> = self
            .living
            .accounts
            .values()
            .filter(|a| a.possessed)
            .map(|a| a.active_unit)
            .collect();
        // Front-to-back yielding prevents same-lane traffic from freely stacking.
        let vehicles: Vec<_> = self
            .living
            .units
            .values()
            .filter(|u| {
                matches!(u.kind, ActorKind::Vehicle)
                    && (matches!(u.order, UnitOrder::Move { .. } | UnitOrder::Patrol)
                        || u.velocity_enu_m_s[0].hypot(u.velocity_enu_m_s[1]) > 0.1)
            })
            .map(|u| (u.id, u.position, u.heading_deg))
            .collect();
        for unit in self.living.units.values_mut() {
            let moving = unit.velocity_enu_m_s[0].hypot(unit.velocity_enu_m_s[1]) > 0.1;
            if unit.is_person() {
                unit.needs.food = (unit.needs.food - sim_dt as f32 * 0.000_025).max(0.);
                unit.needs.water = (unit.needs.water - sim_dt as f32 * 0.000_04).max(0.);
                if matches!(unit.order, UnitOrder::Rest) {
                    unit.needs.energy = (unit.needs.energy + real_dt as f32 * 0.025).min(1.);
                } else if moving {
                    unit.needs.energy = (unit.needs.energy - real_dt as f32 * 0.000_7).max(0.);
                } else {
                    unit.needs.energy = (unit.needs.energy + real_dt as f32 * 0.001).min(1.);
                }
            }
            if possessed.contains(&unit.id) {
                continue;
            }
            if unit.owner.is_none() && unit.needs.energy < 0.08 && unit.is_person() {
                unit.order = UnitOrder::Rest;
            }
            if unit.owner.is_none()
                && matches!(unit.order, UnitOrder::Rest)
                && unit.needs.energy > 0.85
            {
                unit.order = if unit.patrol.is_empty() {
                    UnitOrder::Idle
                } else {
                    UnitOrder::Patrol
                };
            }
            let target = match unit.order {
                UnitOrder::Move { destination } => unit
                    .path
                    .get(unit.path_index)
                    .copied()
                    .or(Some(destination)),
                UnitOrder::Patrol => unit.patrol.get(unit.path_index).copied(),
                _ => None,
            };
            let Some(target) = target else {
                unit.velocity_enu_m_s = [0.; 3];
                continue;
            };
            let delta = LocalFrame::new(unit.position).to_local(target);
            let distance = delta[0].hypot(delta[2]);
            if distance < 1.5 {
                match unit.order {
                    UnitOrder::Move { .. } => {
                        unit.path_index += 1;
                        if unit.path_index >= unit.path.len() {
                            unit.order = UnitOrder::Idle;
                            unit.path.clear();
                            unit.path_index = 0;
                        }
                    }
                    UnitOrder::Patrol => {
                        if unit.patrol_reverse {
                            if unit.path_index == 0 {
                                unit.patrol_reverse = false;
                                unit.path_index = (unit.patrol.len() > 1) as usize;
                            } else {
                                unit.path_index -= 1;
                            }
                        } else if unit.path_index + 1 >= unit.patrol.len() {
                            unit.patrol_reverse = true;
                            unit.path_index = unit.path_index.saturating_sub(1);
                        } else {
                            unit.path_index += 1;
                        }
                    }
                    _ => {}
                }
                unit.velocity_enu_m_s = [0.; 3];
                continue;
            }
            let cap = unit.speed_limit_m_s();
            let nominal = match unit.role {
                UnitRole::Citizen => 1.4,
                UnitRole::Animal => 2.5,
                UnitRole::Traffic => 8.,
                UnitRole::Hauler => 6.,
                _ => match unit.kind {
                    ActorKind::Vehicle => 10.,
                    ActorKind::Helicopter | ActorKind::Airplane | ActorKind::Balloon => 15.,
                    _ => 4.5,
                },
            };
            let mut speed = cap.min(nominal).min(distance / real_dt.max(0.000_001));
            // A waypoint can reverse the route at a junction. Compare the
            // desired new leg with other headings, not this body's previous
            // heading, or its own following queue can deadlock the turn.
            let desired_heading = delta[0].atan2(-delta[2]).to_degrees() as f32;
            if matches!(unit.kind, ActorKind::Vehicle)
                && vehicles.iter().any(|(id, p, heading)| {
                    *id != unit.id && {
                        let d = LocalFrame::new(unit.position).to_local(*p);
                        let ahead = (d[0] * delta[0] + d[2] * delta[2]) / distance;
                        let lateral = (d[0] * delta[2] - d[2] * delta[0]).abs() / distance;
                        ahead > 0.1
                            && ahead < 7.
                            && lateral < 2.
                            && ((*heading - desired_heading + 540.).rem_euclid(360.) - 180.).abs()
                                < 65.
                    }
                })
            {
                speed = 0.;
            }
            let velocity = [
                delta[0] / distance * speed,
                -delta[2] / distance * speed,
                0.,
            ];
            let mut actor = ActorState {
                id: unit.id,
                kind: unit.kind,
                position: unit.position,
                velocity_enu_m_s: velocity,
                heading_deg: unit.heading_deg,
            };
            actor.advance(real_dt)?;
            if let Some(position) =
                resolve(unit.kind, unit.position, actor.position).filter(|p| p.validate().is_ok())
            {
                unit.position = position;
                unit.heading_deg = actor.heading_deg;
                unit.velocity_enu_m_s = velocity;
            } else {
                unit.velocity_enu_m_s = [0.; 3];
            }
        }
        for site in self.living.producers.values_mut() {
            if !site.enabled || site.resource_remaining == Some(0) {
                continue;
            }
            site.progress_seconds = (site.progress_seconds + sim_dt).min(site.cycle_seconds * 8.);
            // Bounded catch-up; unused time stays capped so a long pause cannot
            // produce an unbounded work burst or infinite items on resumption.
            for _ in 0..8 {
                if site.progress_seconds < site.cycle_seconds || site.resource_remaining == Some(0)
                {
                    break;
                }
                let Ok(next) = exchange(&site.stock, &site.inputs, &site.outputs, 1) else {
                    site.progress_seconds = site.progress_seconds.min(site.cycle_seconds);
                    break;
                };
                if next.values().map(|n| u64::from(*n)).sum::<u64>() > 2000 {
                    site.progress_seconds = site.cycle_seconds;
                    break;
                }
                site.stock = next;
                site.progress_seconds -= site.cycle_seconds;
                site.cycles_completed = site.cycles_completed.saturating_add(1);
                if let Some(left) = site.resource_remaining.as_mut() {
                    *left -= 1;
                }
                self.living.metrics.production_cycles =
                    self.living.metrics.production_cycles.saturating_add(1);
            }
        }
        self.process_deliveries()?;
        self.living.dispatch_accumulator += real_dt;
        if self.living.dispatch_accumulator >= 1. {
            self.living.dispatch_accumulator = self.living.dispatch_accumulator.rem_euclid(1.);
            self.dispatch_deliveries();
            self.npc_trade_and_consume(catalog);
        }
        for account in self.living.accounts.values() {
            if let (Some(unit), Some(player)) = (
                self.living.units.get(&account.active_unit),
                self.players.get_mut(&account.player_id),
            ) {
                player.position = unit.position;
            }
        }
        Ok(())
    }

    fn process_deliveries(&mut self) -> Result<(), SandboxError> {
        let mut remaining = Vec::with_capacity(self.living.deliveries.len());
        for mut delivery in std::mem::take(&mut self.living.deliveries) {
            let Some(unit) = self.living.units.get(&delivery.unit_id) else {
                remaining.push(delivery);
                continue;
            };
            let destination = match delivery.phase {
                DeliveryPhase::ToPickup => self
                    .living
                    .producers
                    .get(&delivery.from_site)
                    .map(|s| s.position),
                DeliveryPhase::ToDropoff => self
                    .living
                    .producers
                    .get(&delivery.to_site)
                    .map(|s| s.position)
                    .or_else(|| self.living.shops.get(&delivery.to_site).map(|s| s.position)),
            };
            let Some(destination) = destination else {
                return Err(SandboxError::Invalid("delivery destination".into()));
            };
            if unit.position.surface_distance_m(destination) > 2.5 {
                remaining.push(delivery);
                continue;
            }
            if delivery.phase == DeliveryPhase::ToPickup {
                let Some(source) = self.living.producers.get_mut(&delivery.from_site) else {
                    return Err(SandboxError::Invalid("delivery source".into()));
                };
                let amount = source
                    .stock
                    .get(&delivery.item)
                    .copied()
                    .unwrap_or(0)
                    .min(delivery.amount);
                if amount == 0 {
                    self.living.units.get_mut(&delivery.unit_id).unwrap().order = UnitOrder::Idle;
                    continue;
                }
                subtract(&mut source.stock, &delivery.item, amount);
                delivery.amount = amount;
                delivery.phase = DeliveryPhase::ToDropoff;
                let destination = self
                    .living
                    .producers
                    .get(&delivery.to_site)
                    .map(|s| s.position)
                    .or_else(|| self.living.shops.get(&delivery.to_site).map(|s| s.position))
                    .unwrap();
                let position = self.living.units[&delivery.unit_id].position;
                let path = self.living.route_between(position, destination);
                let unit = self.living.units.get_mut(&delivery.unit_id).unwrap();
                unit.supplies.insert(delivery.item.clone(), amount);
                unit.order = UnitOrder::Move { destination };
                unit.path = path;
                unit.path_index = 0;
                remaining.push(delivery);
            } else {
                if self
                    .living
                    .producers
                    .get(&delivery.to_site)
                    .is_some_and(|site| {
                        site.stock.values().map(|n| u64::from(*n)).sum::<u64>()
                            + u64::from(delivery.amount)
                            > 2000
                    })
                {
                    remaining.push(delivery);
                    continue;
                }
                let stock = if let Some(site) = self.living.producers.get_mut(&delivery.to_site) {
                    &mut site.stock
                } else {
                    &mut self.living.shops.get_mut(&delivery.to_site).unwrap().stock
                };
                let current = stock.get(&delivery.item).copied().unwrap_or(0);
                if current
                    .checked_add(delivery.amount)
                    .is_none_or(|n| n > crate::MAX_ITEM_COUNT)
                {
                    remaining.push(delivery);
                    continue;
                }
                stock.insert(delivery.item.clone(), current + delivery.amount);
                let unit = self.living.units.get_mut(&delivery.unit_id).unwrap();
                unit.supplies.remove(&delivery.item);
                unit.order = UnitOrder::Idle;
                unit.path.clear();
                unit.path_index = 0;
                self.living.metrics.deliveries_completed =
                    self.living.metrics.deliveries_completed.saturating_add(1);
            }
        }
        self.living.deliveries = remaining;
        Ok(())
    }

    fn dispatch_deliveries(&mut self) {
        let busy: BTreeSet<_> = self.living.deliveries.iter().map(|d| d.unit_id).collect();
        let couriers: Vec<_> = self
            .living
            .units
            .values()
            .filter(|u| u.role == UnitRole::Hauler && !busy.contains(&u.id))
            .map(|u| u.id)
            .collect();
        for courier in couriers {
            let mut request = None;
            // Replenish production inputs before stocking retail shelves.
            'outer: for destination in self.living.producers.values() {
                for (item, amount) in &destination.inputs {
                    if destination.stock.get(item).copied().unwrap_or(0) >= amount * 8
                        || self
                            .living
                            .deliveries
                            .iter()
                            .any(|d| d.to_site == destination.id && d.item == *item)
                    {
                        continue;
                    }
                    if let Some(source) = self.living.producers.values().find(|s| {
                        s.id != destination.id
                            && s.outputs.contains_key(item)
                            && s.stock.get(item).copied().unwrap_or(0) >= 4
                    }) {
                        request = Some((
                            source.id,
                            destination.id,
                            item.clone(),
                            source.stock[item].min(12),
                            source.position,
                        ));
                        break 'outer;
                    }
                }
            }
            if request.is_none() {
                'shops: for shop in self.living.shops.values() {
                    for item in shop.prices.keys() {
                        if shop.stock.get(item).copied().unwrap_or(0) >= 60
                            || self
                                .living
                                .deliveries
                                .iter()
                                .any(|d| d.to_site == shop.id && d.item == *item)
                        {
                            continue;
                        }
                        if let Some(source) = self.living.producers.values().find(|s| {
                            s.outputs.contains_key(item)
                                && s.stock.get(item).copied().unwrap_or(0) >= 8
                        }) {
                            request = Some((
                                source.id,
                                shop.id,
                                item.clone(),
                                source.stock[item].min(12),
                                source.position,
                            ));
                            break 'shops;
                        }
                    }
                }
            }
            let Some((from_site, to_site, item, amount, destination)) = request else {
                break;
            };
            let Ok(id) = self.living.next() else { break };
            let path = self
                .living
                .route_between(self.living.units[&courier].position, destination);
            let unit = self.living.units.get_mut(&courier).unwrap();
            unit.order = UnitOrder::Move { destination };
            unit.path = path;
            unit.path_index = 0;
            self.living.deliveries.push(Delivery {
                id,
                unit_id: courier,
                from_site,
                to_site,
                item,
                amount,
                phase: DeliveryPhase::ToPickup,
            });
        }
    }

    fn npc_trade_and_consume(&mut self, catalog: &Catalog) {
        for unit in self
            .living
            .units
            .values_mut()
            .filter(|u| u.owner.is_none() && u.kind == ActorKind::Human)
        {
            let item = if unit.needs.water < 0.65 {
                Some("water")
            } else if unit.needs.food < 0.7 {
                Some("bread")
            } else {
                None
            };
            let Some(item) = item.filter(|i| catalog.items.contains(*i)) else {
                continue;
            };
            let Some(shop) = self
                .living
                .shops
                .values_mut()
                .filter(|s| unit.position.surface_distance_m(s.position) < 12.)
                .min_by_key(|s| s.id)
            else {
                continue;
            };
            let Some(price) = shop.prices.get(item) else {
                continue;
            };
            if unit.pocket_money < price.buy
                || shop.stock.get(item).copied().unwrap_or(0) == 0
                || shop
                    .money
                    .checked_add(price.buy)
                    .is_none_or(|n| n > MAX_MONEY)
            {
                continue;
            }
            unit.pocket_money -= price.buy;
            shop.money += price.buy;
            subtract(&mut shop.stock, item, 1);
            if item == "water" {
                unit.needs.water = (unit.needs.water + 0.6).min(1.);
            } else {
                unit.needs.food = (unit.needs.food + 0.5).min(1.);
            }
            self.living.metrics.npc_purchases = self.living.metrics.npc_purchases.saturating_add(1);
        }
    }
}

fn subtract(stock: &mut Inventory, item: &str, amount: u32) {
    let value = stock.get(item).copied().unwrap_or(0) - amount;
    if value == 0 {
        stock.remove(item);
    } else {
        stock.insert(item.into(), value);
    }
}
