use super::*;

impl SandboxState {
    /// Pre-movement payroll. A missing account balance cannot buy one extra
    /// motion step. Waiting at a stop has no wage charge; dispatched travel does.
    pub fn prepare_company_drivers(&mut self, physical_seconds: f64) -> Result<(), SandboxError> {
        if !finite_range(physical_seconds, 0., 5.) {
            return Err(invalid("company physical timestep"));
        }
        if physical_seconds == 0. {
            return Ok(());
        }
        let owners: Vec<_> = self.transport.companies.keys().copied().collect();
        for owner in owners {
            if self.is_player_dormant(owner) {
                continue;
            }
            let drivers = self.transport.companies[&owner].drivers.clone();
            for driver in drivers {
                let Some(vehicle_id) = driver.vehicle_id else {
                    continue;
                };
                let Some(unit) = self.living.units.get(&vehicle_id) else {
                    continue;
                };
                let vehicle = &self.transport.vehicles[&vehicle_id];
                let route = self.transport.companies[&owner]
                    .routes
                    .iter()
                    .find(|route| Some(route.id) == driver.route_id)
                    .unwrap();
                let stop = driver
                    .trip
                    .as_ref()
                    .filter(|t| t.returning)
                    .map_or(route.stops[driver.next_stop], |t| t.pickup);
                let target = self.transport.facilities[&stop].position;
                let at_stop = near(unit.position, target) && body_speed(unit) <= 1.;
                let working = driver.running || driver.trip.is_some();
                let blocked =
                    if matches!(driver.status, FleetStatus::RouteBlocked | FleetStatus::Held) {
                        Some(driver.status)
                    } else if !working {
                        Some(FleetStatus::Paused)
                    } else if vehicle
                        .rental
                        .as_ref()
                        .is_some_and(|r| r.remaining_seconds <= 0.)
                    {
                        Some(FleetStatus::RentalExpired)
                    } else if vehicle.fuel_l <= 0. {
                        Some(FleetStatus::NeedsFuel)
                    } else if vehicle.condition <= 0.05 {
                        Some(FleetStatus::NeedsRepair)
                    } else {
                        None
                    };
                if let Some(status) = blocked {
                    self.park_fleet_vehicle(vehicle_id);
                    self.fleet_driver_mut(owner, driver.id).status = status;
                    continue;
                }
                if at_stop {
                    // Arrival processing performs checked load/delivery after
                    // all vehicles have had their ordinary physical step.
                    self.park_fleet_vehicle(vehicle_id);
                    continue;
                }
                let stalled =
                    if body_speed(unit) < 0.1 && matches!(unit.order, UnitOrder::Move { .. }) {
                        (driver.stalled_seconds + physical_seconds).min(15.)
                    } else {
                        0.
                    };
                if stalled >= 15. {
                    self.park_fleet_vehicle(vehicle_id);
                    let driver = self.fleet_driver_mut(owner, driver.id);
                    driver.stalled_seconds = stalled;
                    driver.status = FleetStatus::RouteBlocked;
                    continue;
                }
                let total = driver.wage_fraction + physical_seconds * COMPANY_WAGE_PER_SECOND;
                // Only absorb binary floating-point drift at an integer minor
                // credit; fractions otherwise survive every frame/assignment.
                let due = (total + 1e-9).floor() as u64;
                let available = self.transport.companies[&owner].cash;
                let earned = driver
                    .wages_earned
                    .checked_add(due)
                    .filter(|n| *n <= MAX_MONEY);
                if self.transport.companies[&owner]
                    .accounting
                    .as_ref()
                    .is_some_and(|accounting| {
                        accounting.monetary_entry_count >= MAX_COMPANY_ACCOUNTING_TOTAL
                            || accounting.total(CompanyCashCategory::Payroll)
                                >= MAX_COMPANY_ACCOUNTING_TOTAL
                    })
                {
                    // Do not accrue a fractional wage or authorize a motion
                    // step when its next payable credit cannot be recorded.
                    self.park_fleet_vehicle(vehicle_id);
                    self.fleet_driver_mut(owner, driver.id).status = FleetStatus::AccountingFull;
                    continue;
                }
                if available == 0 || available < due || earned.is_none() {
                    self.park_fleet_vehicle(vehicle_id);
                    self.fleet_driver_mut(owner, driver.id).status =
                        FleetStatus::WaitingCompanyFunds;
                    continue;
                }
                // Fraction-only ticks change the liability, not cash. Keep the
                // existing bounded journal instead of cloning it every frame.
                let accounting = if due == 0 {
                    None
                } else {
                    match self.transport.companies[&owner].prepare_accounting(
                        self.transport.elapsed_seconds,
                        &[(CompanyCashCategory::Payroll, due)],
                    ) {
                        Ok(accounting) => Some(accounting),
                        _ => {
                            self.park_fleet_vehicle(vehicle_id);
                            self.fleet_driver_mut(owner, driver.id).status =
                                FleetStatus::AccountingFull;
                            continue;
                        }
                    }
                };
                let company = self.transport.companies.get_mut(&owner).unwrap();
                company.cash -= due;
                if let Some(accounting) = accounting {
                    company.accounting = Some(accounting);
                }
                let driver_mut = company
                    .drivers
                    .iter_mut()
                    .find(|d| d.id == driver.id)
                    .unwrap();
                driver_mut.wages_earned = earned.unwrap();
                driver_mut.wage_fraction = (total - due as f64).max(0.);
                driver_mut.stalled_seconds = stalled;
                driver_mut.status = if driver.running {
                    FleetStatus::Driving
                } else {
                    FleetStatus::Delivering
                };
                self.set_fleet_destination(vehicle_id, target);
            }
        }
        Ok(())
    }

    pub(in crate::transport) fn advance_company_arrivals(&mut self) {
        let ready: Vec<_> = self
            .transport
            .companies
            .values()
            .filter(|company| !self.is_player_dormant(company.owner))
            .flat_map(|company| {
                company.drivers.iter().filter_map(|driver| {
                    let vehicle = driver.vehicle_id?;
                    let body = self.living.units.get(&vehicle)?;
                    let route = company
                        .routes
                        .iter()
                        .find(|route| Some(route.id) == driver.route_id)?;
                    let stop = driver
                        .trip
                        .as_ref()
                        .filter(|t| t.returning)
                        .map_or(route.stops[driver.next_stop], |t| t.pickup);
                    let active =
                        !matches!(driver.status, FleetStatus::RouteBlocked | FleetStatus::Held)
                            && (driver.running || driver.trip.is_some());
                    (active
                        && near(body.position, self.transport.facilities[&stop].position)
                        && body_speed(body) <= 1.)
                        .then_some((company.owner, driver.id, stop))
                })
            })
            .collect();
        for (owner, driver, stop) in ready {
            let snapshot = self.transport.companies[&owner]
                .drivers
                .iter()
                .find(|d| d.id == driver)
                .unwrap()
                .clone();
            if let Some(trip) = &snapshot.trip {
                if trip.returning {
                    if self.refund_company_trip(owner, driver).is_err() {
                        self.fleet_driver_mut(owner, driver).status = FleetStatus::DestinationFull;
                    }
                } else if stop == trip.destination {
                    let payment =
                        trip.reward_escrow + trip.goods_payment.as_ref().map_or(0, |p| p.total);
                    let company = &self.transport.companies[&owner];
                    let cash_fits = company
                        .cash
                        .checked_add(payment)
                        .is_some_and(|cash| cash <= MAX_MONEY);
                    if cash_fits && self.fleet_delivery_accounting(owner, trip).is_err() {
                        self.fleet_driver_mut(owner, driver).status = FleetStatus::AccountingFull;
                    } else if self.deliver_fleet_trip(owner, driver).is_err() {
                        self.fleet_driver_mut(owner, driver).status = FleetStatus::DestinationFull;
                    }
                } else {
                    // Freight route middle stops are ordered physical gates.
                    self.fleet_driver_mut(owner, driver).next_stop += 1;
                }
            } else if snapshot.running {
                let route = self.transport.companies[&owner]
                    .routes
                    .iter()
                    .find(|route| Some(route.id) == snapshot.route_id)
                    .unwrap()
                    .clone();
                if route.service.cargo_load().is_some() && snapshot.next_stop != 0 {
                    self.fleet_driver_mut(owner, driver).next_stop = 0;
                } else if let Err(status) = self.pickup_fleet_trip(owner, driver, &route) {
                    self.fleet_driver_mut(owner, driver).status = status;
                }
            }
        }
    }

    fn pickup_fleet_trip(
        &mut self,
        owner: u64,
        driver_id: u64,
        route: &CompanyRoute,
    ) -> Result<(), FleetStatus> {
        let driver = self.transport.companies[&owner]
            .drivers
            .iter()
            .find(|d| d.id == driver_id)
            .unwrap()
            .clone();
        let vehicle_id = driver.vehicle_id.unwrap();
        let vehicle = &self.transport.vehicles[&vehicle_id];
        if self.transport.companies[&owner].cash == 0 {
            return Err(FleetStatus::WaitingCompanyFunds);
        }
        if vehicle
            .rental
            .as_ref()
            .is_some_and(|r| r.remaining_seconds <= 0.)
        {
            return Err(FleetStatus::RentalExpired);
        }
        if vehicle.fuel_l <= 0. {
            return Err(FleetStatus::NeedsFuel);
        }
        if vehicle.condition <= 0.05 {
            return Err(FleetStatus::NeedsRepair);
        }
        let pickup = route.stops[driver.next_stop];
        let (destination, next_stop) = match route.service {
            FleetService::Passengers { .. } => {
                let next = (driver.next_stop + 1) % route.stops.len();
                (route.stops[next], next)
            }
            FleetService::Freight { .. }
            | FleetService::DepotFreight { .. }
            | FleetService::DepotTransfer { .. } => (*route.stops.last().unwrap(), 1),
        };
        let mut service = route.service.clone();
        let mut source_next = None;
        let mut vehicle_next = None;
        let mut goods_payment = None;
        match &mut service {
            FleetService::Passengers { count } => {
                if self.transport.facilities[&pickup].passengers_waiting < u32::from(*count) {
                    return Err(FleetStatus::WaitingPassengers);
                }
            }
            FleetService::Freight { item, amount } => {
                let source = self.transport.facilities[&pickup].linked_site.unwrap();
                let source_stock = self
                    .transport_stock(pickup)
                    .map_err(|_| FleetStatus::WaitingGoods)?;
                let destination_stock = self
                    .transport_stock(destination)
                    .map_err(|_| FleetStatus::WaitingDemand)?;
                let reserved = self
                    .living
                    .deliveries
                    .iter()
                    .filter(|d| {
                        d.from_site == source
                            && d.item == *item
                            && d.phase == DeliveryPhase::ToPickup
                    })
                    .map(|d| u64::from(d.amount))
                    .sum::<u64>();
                let incoming = self.fleet_incoming(destination, item);
                let target = u64::from(self.fleet_destination_target(destination, item));
                let demand = target.saturating_sub(
                    u64::from(destination_stock.get(item).copied().unwrap_or(0)) + incoming,
                );
                if demand == 0 {
                    return Err(FleetStatus::WaitingDemand);
                }
                let stock = u64::from(source_stock.get(item).copied().unwrap_or(0))
                    .saturating_sub(reserved);
                if stock == 0 {
                    return Err(FleetStatus::WaitingGoods);
                }
                *amount = (*amount).min(stock.min(demand) as u32);
                let goods = inventory(&[(item, *amount)]);
                source_next = Some(
                    exchange(source_stock, &goods, &Inventory::new(), 1)
                        .map_err(|_| FleetStatus::WaitingGoods)?,
                );
                vehicle_next = Some(
                    exchange(&vehicle.cargo, &Inventory::new(), &goods, 1)
                        .map_err(|_| FleetStatus::DestinationFull)?,
                );
            }
            FleetService::DepotFreight { item, amount }
            | FleetService::DepotTransfer { item, amount } => {
                let source_stock = self
                    .company_depot_stock(owner, pickup)
                    .map_err(|_| FleetStatus::WaitingGoods)?;
                let stock = source_stock.get(item).copied().unwrap_or(0);
                if stock == 0 {
                    return Err(FleetStatus::WaitingGoods);
                }
                let demand = if matches!(route.service, FleetService::DepotTransfer { .. }) {
                    let destination_stock = self
                        .company_depot_stock(owner, destination)
                        .map_err(|_| FleetStatus::DestinationFull)?;
                    let used = destination_stock
                        .values()
                        .map(|n| u64::from(*n))
                        .sum::<u64>()
                        + self.company_depot_reserved_units(owner, destination);
                    COMPANY_DEPOT_CAPACITY
                        .saturating_sub(used)
                        .min(u64::from(u32::MAX)) as u32
                } else {
                    let shop_id = self.transport.facilities[&destination]
                        .linked_site
                        .ok_or(FleetStatus::WaitingDemand)?;
                    let shop = self
                        .living
                        .shops
                        .get(&shop_id)
                        .ok_or(FleetStatus::WaitingDemand)?;
                    let price = shop
                        .prices
                        .get(item)
                        .ok_or(FleetStatus::WaitingDemand)?
                        .sell;
                    let target = u64::from(self.fleet_destination_target(destination, item));
                    let incoming = self.fleet_incoming(destination, item);
                    let demand = target.saturating_sub(
                        u64::from(shop.stock.get(item).copied().unwrap_or(0)) + incoming,
                    );
                    if demand == 0 {
                        return Err(FleetStatus::WaitingDemand);
                    }
                    if price == 0 || shop.money < price {
                        return Err(FleetStatus::WaitingBuyerFunds);
                    }
                    let amount = (*amount)
                        .min(stock)
                        .min(demand.min(shop.money / price) as u32);
                    goods_payment = Some(DepotGoodsPayment {
                        shop_id,
                        unit_price: price,
                        total: price * u64::from(amount),
                    });
                    amount
                };
                if demand == 0 {
                    return Err(FleetStatus::WaitingDemand);
                }
                *amount = (*amount).min(stock).min(demand);
                let goods = inventory(&[(item, *amount)]);
                source_next = Some(
                    exchange(source_stock, &goods, &Inventory::new(), 1)
                        .map_err(|_| FleetStatus::WaitingGoods)?,
                );
                vehicle_next = Some(
                    exchange(&vehicle.cargo, &Inventory::new(), &goods, 1)
                        .map_err(|_| FleetStatus::DestinationFull)?,
                );
            }
        }
        let reward = fleet_fare(
            &service,
            self.transport.facilities[&pickup].position,
            self.transport.facilities[&destination].position,
        );
        if self.transport.treasury < reward {
            return Err(FleetStatus::WaitingIssuerFunds);
        }
        let trip = FleetTrip {
            pickup,
            destination,
            service: service.clone(),
            reward_escrow: reward,
            pickup_odometer_m: vehicle.odometer_m,
            pickup_elapsed_seconds: self.transport.elapsed_seconds,
            returning: false,
            transferred_distance_m: 0.,
            goods_payment,
        };
        if let Some(stock) = source_next {
            if service.uses_company_stock() {
                *self.company_depot_stock_mut(owner, pickup).unwrap() = stock;
            } else {
                *self.transport_stock_mut(pickup).unwrap() = stock;
            }
        }
        if let Some(stock) = vehicle_next {
            self.transport.vehicles.get_mut(&vehicle_id).unwrap().cargo = stock;
        }
        if let FleetService::Passengers { count } = service {
            self.transport
                .facilities
                .get_mut(&pickup)
                .unwrap()
                .passengers_waiting -= u32::from(count);
        }
        self.transport.treasury -= reward;
        if let Some(payment) = &trip.goods_payment {
            self.living.shops.get_mut(&payment.shop_id).unwrap().money -= payment.total;
        }
        let driver = self.fleet_driver_mut(owner, driver_id);
        driver.trip = Some(trip);
        driver.next_stop = next_stop;
        driver.status = FleetStatus::Driving;
        Ok(())
    }

    fn fleet_delivery_accounting(
        &self,
        owner: u64,
        trip: &FleetTrip,
    ) -> Result<CompanyAccounting, SandboxError> {
        self.transport.companies[&owner].prepare_accounting(
            self.transport.elapsed_seconds,
            &[
                (CompanyCashCategory::PublicFares, trip.reward_escrow),
                (
                    CompanyCashCategory::GoodsSales,
                    trip.goods_payment.as_ref().map_or(0, |p| p.total),
                ),
            ],
        )
    }

    fn deliver_fleet_trip(&mut self, owner: u64, driver_id: u64) -> Result<(), SandboxError> {
        let driver = self.transport.companies[&owner]
            .drivers
            .iter()
            .find(|d| d.id == driver_id)
            .unwrap()
            .clone();
        let trip = driver.trip.as_ref().unwrap();
        let vehicle_id = driver.vehicle_id.unwrap();
        let vehicle = &self.transport.vehicles[&vehicle_id];
        let distance = self.transport.facilities[&trip.pickup]
            .position
            .surface_distance_m(self.transport.facilities[&trip.destination].position);
        let required = (distance - 2. * INTERACTION_REACH_M).max(1.);
        if vehicle.odometer_m - trip.pickup_odometer_m + trip.transferred_distance_m + 1e-6
            < required
            || self.transport.elapsed_seconds - trip.pickup_elapsed_seconds + 1e-6 < required / 40.
        {
            return Err(invalid("fleet delivery needs physical travel"));
        }
        let company = &self.transport.companies[&owner];
        let cash = company
            .cash
            .checked_add(trip.reward_escrow)
            .and_then(|n| n.checked_add(trip.goods_payment.as_ref().map_or(0, |p| p.total)))
            .filter(|n| *n <= MAX_MONEY)
            .ok_or(SandboxError::Capacity)?;
        let accounting = self.fleet_delivery_accounting(owner, trip)?;
        let revenue = company
            .revenue
            .checked_add(trip.reward_escrow)
            .and_then(|n| n.checked_add(trip.goods_payment.as_ref().map_or(0, |p| p.total)))
            .filter(|n| *n <= MAX_COMPANY_ACCOUNTING_TOTAL)
            .ok_or(SandboxError::Capacity)?;
        let mut destination_next = None;
        let mut vehicle_next = None;
        let mut passengers_next = None;
        match &trip.service {
            FleetService::Freight { item, amount } => {
                let goods = inventory(&[(item, *amount)]);
                vehicle_next = Some(exchange(&vehicle.cargo, &goods, &Inventory::new(), 1)?);
                destination_next = Some(self.transport_delivery_stock(trip.destination, &goods)?);
            }
            FleetService::DepotFreight { item, amount } => {
                let goods = inventory(&[(item, *amount)]);
                vehicle_next = Some(exchange(&vehicle.cargo, &goods, &Inventory::new(), 1)?);
                destination_next = Some(self.transport_delivery_stock(trip.destination, &goods)?);
            }
            FleetService::DepotTransfer { item, amount } => {
                let goods = inventory(&[(item, *amount)]);
                vehicle_next = Some(exchange(&vehicle.cargo, &goods, &Inventory::new(), 1)?);
                destination_next = Some(self.company_freight_delivery_stock(
                    owner,
                    trip.destination,
                    &goods,
                    Some(driver_id),
                )?);
            }
            FleetService::Passengers { count } => {
                passengers_next = Some(
                    self.transport.facilities[&trip.destination]
                        .passengers_waiting
                        .checked_add(u32::from(*count))
                        .filter(|n| *n <= 1_000_000)
                        .ok_or(SandboxError::Capacity)?,
                );
            }
        }
        if let Some(stock) = destination_next {
            if matches!(trip.service, FleetService::DepotTransfer { .. }) {
                self.set_company_freight_stock(owner, trip.destination, stock)?;
            } else {
                *self.transport_stock_mut(trip.destination)? = stock;
            }
        }
        if let Some(stock) = vehicle_next {
            self.transport.vehicles.get_mut(&vehicle_id).unwrap().cargo = stock;
        }
        if let Some(waiting) = passengers_next {
            self.transport
                .facilities
                .get_mut(&trip.destination)
                .unwrap()
                .passengers_waiting = waiting;
        }
        if let Some(payment) = &trip.goods_payment {
            self.living.metrics.units_traded = self
                .living
                .metrics
                .units_traded
                .saturating_add(u64::from(trip.service.cargo_load().unwrap().1));
            self.living.metrics.credits_traded = self
                .living
                .metrics
                .credits_traded
                .saturating_add(payment.total);
        }
        let company = self.transport.companies.get_mut(&owner).unwrap();
        company.cash = cash;
        company.accounting = Some(accounting);
        company.revenue = revenue;
        let driver = company
            .drivers
            .iter_mut()
            .find(|d| d.id == driver_id)
            .unwrap();
        if trip.service.cargo_load().is_some() {
            driver.next_stop = 0;
        }
        driver.trip = None;
        driver.deliveries_completed = driver
            .deliveries_completed
            .saturating_add(1)
            .min(1_000_000_000);
        driver.status = if driver.running {
            FleetStatus::Driving
        } else {
            FleetStatus::Paused
        };
        self.park_fleet_vehicle(vehicle_id);
        Ok(())
    }

    fn fleet_incoming(&self, destination: u64, item: &str) -> u64 {
        let site = self.transport.facilities[&destination].linked_site;
        let company = self
            .transport
            .companies
            .values()
            .flat_map(|company| company.drivers.iter())
            .filter_map(|driver| driver.trip.as_ref())
            .filter(|trip| {
                !trip.returning && self.transport.facilities[&trip.destination].linked_site == site
            })
            .filter_map(|trip| match &trip.service {
                FleetService::Freight { item: i, amount }
                | FleetService::DepotFreight { item: i, amount }
                    if i == item =>
                {
                    Some(u64::from(*amount))
                }
                _ => None,
            })
            .sum::<u64>();
        let jobs = self
            .transport
            .jobs
            .values()
            .filter(|job| {
                job.assignment.is_some()
                    && self.transport.facilities[&job.destination_facility].linked_site == site
                    && job.item.as_deref() == Some(item)
            })
            .map(|job| u64::from(job.amount))
            .sum::<u64>();
        company
            + jobs
            + self
                .living
                .deliveries
                .iter()
                .filter(|d| {
                    Some(d.to_site) == site && d.item == item && d.phase != DeliveryPhase::Returning
                })
                .map(|d| u64::from(d.amount))
                .sum::<u64>()
    }

    fn set_fleet_destination(&mut self, vehicle: u64, destination: GeoPosition) {
        let unit = &self.living.units[&vehicle];
        if !matches!(unit.order, UnitOrder::Move { destination: current } if current == destination)
            || unit.path.is_empty()
        {
            let path = self
                .living
                .vehicle_route_between(unit.position, destination);
            let unit = self.living.units.get_mut(&vehicle).unwrap();
            unit.order = UnitOrder::Move { destination };
            unit.path = path;
            unit.path_index = 0;
        }
        let vehicle = self.transport.vehicles.get_mut(&vehicle).unwrap();
        vehicle.autopilot = true;
        vehicle.parking_brake = false;
    }

    pub(super) fn fleet_driver_mut(&mut self, owner: u64, driver: u64) -> &mut CompanyDriver {
        self.transport
            .companies
            .get_mut(&owner)
            .unwrap()
            .drivers
            .iter_mut()
            .find(|d| d.id == driver)
            .unwrap()
    }
}

/// The issuer, not the player, sets fares. There is no user-provided reward.
pub fn fleet_fare(service: &FleetService, pickup: GeoPosition, destination: GeoPosition) -> u64 {
    if service.uses_company_stock() {
        return 0;
    }
    let distance = pickup.surface_distance_m(destination).clamp(0., 16_000.);
    let load = match service {
        FleetService::Passengers { count } => u64::from(*count) * 200,
        FleetService::Freight { amount, .. } => u64::from(*amount) * 30,
        FleetService::DepotFreight { .. } | FleetService::DepotTransfer { .. } => 0,
    };
    500 + load + distance.round() as u64
}
