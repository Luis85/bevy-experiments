//! Failure recovery preserves the physical shipment and its funded commitment.
use super::*;

impl SandboxState {
    pub(super) fn hold_company_vehicle(
        &mut self,
        owner: u64,
        id: u64,
    ) -> Result<Outcome, SandboxError> {
        let driver = self.transport.companies[&owner]
            .drivers
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| invalid("company driver"))?;
        let vehicle = driver
            .vehicle_id
            .ok_or_else(|| invalid("driver has no vehicle"))?;
        if !self.living.units.contains_key(&vehicle) {
            return Err(invalid("vehicle is outside resident regions"));
        }
        self.park_fleet_vehicle(vehicle);
        let driver = self.fleet_driver_mut(owner, id);
        driver.running = false;
        driver.status = FleetStatus::Held;
        driver.stalled_seconds = 0.;
        Ok(outcome(
            "Vehicle held in place; payroll stopped and the load remains aboard",
        ))
    }

    pub(super) fn return_company_load(
        &mut self,
        owner: u64,
        id: u64,
    ) -> Result<Outcome, SandboxError> {
        let driver = self.transport.companies[&owner]
            .drivers
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| invalid("company driver"))?;
        let vehicle = driver
            .vehicle_id
            .ok_or_else(|| invalid("driver has no vehicle"))?;
        if driver.trip.is_none() {
            return Err(invalid("there is no loaded trip to return"));
        }
        if !self.living.units.contains_key(&vehicle) {
            return Err(invalid("vehicle is outside resident regions"));
        }
        if driver.trip.as_ref().unwrap().returning {
            return Err(invalid("this shipment is already returning"));
        }
        self.park_fleet_vehicle(vehicle);
        let driver = self.fleet_driver_mut(owner, id);
        driver.trip.as_mut().unwrap().returning = true;
        driver.running = false;
        driver.status = FleetStatus::Delivering;
        driver.stalled_seconds = 0.;
        Ok(outcome(
            "Returning to the original pickup. Cargo and payment stay reserved until physically unloaded; no delivery reward is paid",
        ))
    }

    pub(super) fn refund_company_trip(&mut self, owner: u64, id: u64) -> Result<(), SandboxError> {
        let driver = self.transport.companies[&owner]
            .drivers
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| invalid("company driver"))?
            .clone();
        let trip = driver
            .trip
            .as_ref()
            .ok_or_else(|| invalid("loaded company trip"))?;
        let vehicle = driver
            .vehicle_id
            .ok_or_else(|| invalid("company vehicle"))?;
        let body = self
            .living
            .units
            .get(&vehicle)
            .ok_or_else(|| invalid("resident company vehicle"))?;
        if !trip.returning
            || !near(
                body.position,
                self.transport.facilities[&trip.pickup].position,
            )
            || body_speed(body) > 1.
        {
            return Err(SandboxError::OutOfReach);
        }
        let treasury = self
            .transport
            .treasury
            .checked_add(trip.reward_escrow)
            .filter(|n| *n <= MAX_MONEY)
            .ok_or(SandboxError::Capacity)?;
        let mut stock = None;
        let mut cargo = None;
        let mut waiting = None;
        let shop_refund = trip
            .goods_payment
            .as_ref()
            .map(|payment| {
                let shop = self
                    .living
                    .shops
                    .get(&payment.shop_id)
                    .ok_or_else(|| invalid("reserved warehouse buyer"))?;
                shop.money
                    .checked_add(payment.total)
                    .filter(|n| *n <= MAX_MONEY)
                    .ok_or(SandboxError::Capacity)
                    .map(|money| (payment.shop_id, money))
            })
            .transpose()?;
        match &trip.service {
            FleetService::Freight { item, amount } => {
                let goods = inventory(&[(item, *amount)]);
                stock = Some(self.transport_delivery_stock(trip.pickup, &goods)?);
                cargo = Some(exchange(
                    &self.transport.vehicles[&vehicle].cargo,
                    &goods,
                    &Inventory::new(),
                    1,
                )?);
            }
            FleetService::DepotFreight { item, amount }
            | FleetService::DepotTransfer { item, amount } => {
                let goods = inventory(&[(item, *amount)]);
                stock = Some(self.company_freight_delivery_stock(
                    owner,
                    trip.pickup,
                    &goods,
                    Some(id),
                )?);
                cargo = Some(exchange(
                    &self.transport.vehicles[&vehicle].cargo,
                    &goods,
                    &Inventory::new(),
                    1,
                )?);
            }
            FleetService::Passengers { count } => {
                waiting = Some(
                    self.transport.facilities[&trip.pickup]
                        .passengers_waiting
                        .checked_add(u32::from(*count))
                        .filter(|n| *n <= 1_000_000)
                        .ok_or(SandboxError::Capacity)?,
                );
            }
        }
        if let Some(stock) = stock {
            if trip.service.uses_company_stock() {
                self.set_company_freight_stock(owner, trip.pickup, stock)?;
            } else {
                *self.transport_stock_mut(trip.pickup)? = stock;
            }
        }
        if let Some(cargo) = cargo {
            self.transport.vehicles.get_mut(&vehicle).unwrap().cargo = cargo;
        }
        if let Some(waiting) = waiting {
            self.transport
                .facilities
                .get_mut(&trip.pickup)
                .unwrap()
                .passengers_waiting = waiting;
        }
        self.transport.treasury = treasury;
        if let Some((shop, money)) = shop_refund {
            self.living.shops.get_mut(&shop).unwrap().money = money;
        }
        self.park_fleet_vehicle(vehicle);
        let route = self.transport.companies[&owner]
            .routes
            .iter()
            .find(|r| Some(r.id) == driver.route_id)
            .unwrap();
        let cursor = route
            .stops
            .iter()
            .position(|stop| *stop == trip.pickup)
            .unwrap();
        let driver = self.fleet_driver_mut(owner, id);
        driver.trip = None;
        driver.next_stop = cursor;
        driver.running = false;
        driver.status = FleetStatus::Paused;
        driver.stalled_seconds = 0.;
        Ok(())
    }

    pub(super) fn handover_company_load(
        &mut self,
        owner: u64,
        id: u64,
        relief: u64,
        replacement: u64,
    ) -> Result<Outcome, SandboxError> {
        let company = &self.transport.companies[&owner];
        let driver = company
            .drivers
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| invalid("company driver"))?
            .clone();
        let next = company
            .drivers
            .iter()
            .find(|d| d.id == relief)
            .ok_or_else(|| invalid("relief driver"))?;
        let original = driver
            .vehicle_id
            .ok_or_else(|| invalid("driver has no vehicle"))?;
        let mut trip = driver
            .trip
            .clone()
            .ok_or_else(|| invalid("there is no shipment to hand over"))?;
        if id == relief
            || next.vehicle_id.is_some()
            || self.is_company_vehicle(replacement)
            || self.vehicle_has_job(replacement)
            || self
                .transport
                .races
                .values()
                .any(|r| r.vehicle_id == replacement)
            || self.living.accounts[&owner].active_unit == replacement
        {
            return Err(invalid(
                "choose an unassigned relief driver and a free vehicle; exit it first",
            ));
        }
        let source = self
            .living
            .units
            .get(&original)
            .ok_or_else(|| invalid("resident loaded vehicle"))?;
        let target = self
            .living
            .units
            .get(&replacement)
            .ok_or_else(|| invalid("resident replacement vehicle"))?;
        let vehicle = self.owned_vehicle(owner, replacement)?;
        if body_speed(source) > 0.5
            || body_speed(target) > 0.5
            || !near(source.position, target.position)
            || !near(self.players[&owner].position, source.position)
            || !near(self.players[&owner].position, target.position)
        {
            return Err(invalid(
                "hold both vehicles within 20 m of each other and approach them on foot",
            ));
        }
        if self.living.accounts[&owner].active_unit != self.living.accounts[&owner].avatar_unit
            || !vehicle.cargo.is_empty()
            || vehicle.model.spec().required_license == DriverLicense::Emergency
        {
            return Err(invalid(
                "exit to your walking avatar and use an empty qualified replacement vehicle",
            ));
        }
        let route = company
            .routes
            .iter()
            .find(|r| Some(r.id) == driver.route_id)
            .unwrap();
        self.validate_fleet_capacity(replacement, &route.service)?;
        let distance = trip.transferred_distance_m
            + (self.transport.vehicles[&original].odometer_m - trip.pickup_odometer_m).max(0.);
        if !finite_range(distance, 0., 1e12) {
            return Err(SandboxError::Capacity);
        }
        trip.transferred_distance_m = distance;
        trip.pickup_odometer_m = vehicle.odometer_m;
        let cargo = self.transport.vehicles[&original].cargo.clone();
        self.park_fleet_vehicle(original);
        self.park_fleet_vehicle(replacement);
        self.transport
            .vehicles
            .get_mut(&original)
            .unwrap()
            .cargo
            .clear();
        self.transport.vehicles.get_mut(&replacement).unwrap().cargo = cargo;
        let old = self.fleet_driver_mut(owner, id);
        old.vehicle_id = None;
        old.route_id = None;
        old.next_stop = 0;
        old.running = false;
        old.trip = None;
        old.status = FleetStatus::Unassigned;
        old.stalled_seconds = 0.;
        let next = self.fleet_driver_mut(owner, relief);
        next.vehicle_id = Some(replacement);
        next.route_id = driver.route_id;
        next.next_stop = driver.next_stop;
        next.running = false;
        next.trip = Some(trip);
        next.status = FleetStatus::Held;
        next.stalled_seconds = 0.;
        Ok(outcome(
            "Load, fare and travel proof handed over nearby. Original vehicle released; resume the held relief driver when ready",
        ))
    }

    pub(super) fn dismiss_company_driver(
        &mut self,
        owner: u64,
        id: u64,
    ) -> Result<Outcome, SandboxError> {
        self.require_company_garage(owner)?;
        let company = &self.transport.companies[&owner];
        let driver = company
            .drivers
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| invalid("company driver"))?;
        if driver.vehicle_id.is_some() || driver.trip.is_some() {
            return Err(invalid("release the employee's vehicle before dismissal"));
        }
        let final_pay = u64::from(driver.wage_fraction > 1e-9);
        let cash = company
            .cash
            .checked_sub(final_pay)
            .ok_or_else(|| SandboxError::Insufficient("final payroll credit".into()))?;
        let departed = company
            .departed_wages
            .checked_add(driver.wages_earned)
            .and_then(|n| n.checked_add(final_pay))
            .filter(|n| *n <= 1_000_000_000_000)
            .ok_or(SandboxError::Capacity)?;
        let spent = company
            .departed_spending
            .checked_add(driver.wages_spent)
            .filter(|n| *n <= 1_000_000_000_000)
            .ok_or(SandboxError::Capacity)?;
        let accounting = company.prepare_accounting(
            self.transport.elapsed_seconds,
            &[(CompanyCashCategory::Payroll, final_pay)],
        )?;
        let company = self.transport.companies.get_mut(&owner).unwrap();
        company.cash = cash;
        company.accounting = Some(accounting);
        company.departed_wages = departed;
        company.departed_spending = spent;
        company.drivers.retain(|d| d.id != id);
        Ok(outcome(
            "Employee dismissed. Earned wages remain theirs; any fractional final credit was settled. Recruitment is not refunded",
        ))
    }
}
