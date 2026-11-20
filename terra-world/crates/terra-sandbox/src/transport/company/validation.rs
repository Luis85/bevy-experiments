use super::*;

impl CompanyRoute {
    pub(super) fn validate(
        &self,
        facilities: &BTreeMap<u64, TransportFacility>,
        catalog: Option<&Catalog>,
    ) -> Result<(), SandboxError> {
        if self.id == 0
            || !valid_name(&self.name)
            || !(2..=MAX_COMPANY_ROUTE_STOPS).contains(&self.stops.len())
        {
            return Err(invalid("company route identity, name or stop count"));
        }
        let mut seen = BTreeSet::new();
        for (index, &stop) in self.stops.iter().enumerate() {
            let facility = facilities
                .get(&stop)
                .ok_or_else(|| invalid("company route facility"))?;
            if !seen.insert(stop) {
                return Err(invalid("company route repeats a stop"));
            }
            let next = facilities
                .get(&self.stops[(index + 1) % self.stops.len()])
                .ok_or_else(|| invalid("company route facility"))?;
            let distance = facility.position.surface_distance_m(next.position);
            if !finite_range(distance, 50., 2000.) {
                return Err(invalid(
                    "consecutive route stops must be 50 m–2 km apart, including the return leg",
                ));
            }
        }
        match &self.service {
            FleetService::Passengers { count } if (1..=30).contains(count) => {}
            FleetService::Freight { item, amount }
                if (1..=240).contains(amount)
                    && !item.is_empty()
                    && item.len() <= 64
                    && !item.chars().any(char::is_control)
                    && catalog.is_none_or(|c| c.items.contains(item)) =>
            {
                let source = facilities[&self.stops[0]].linked_site;
                let destination = facilities[self.stops.last().unwrap()].linked_site;
                if source.is_none() || destination.is_none() || source == destination {
                    return Err(invalid(
                        "freight route needs different source and destination inventories",
                    ));
                }
            }
            FleetService::DepotFreight { item, amount }
            | FleetService::DepotTransfer { item, amount }
                if (1..=240).contains(amount)
                    && !item.is_empty()
                    && item.len() <= 64
                    && !item.chars().any(char::is_control)
                    && catalog.is_none_or(|c| c.items.contains(item)) =>
            {
                if !matches!(
                    facilities[&self.stops[0]].kind,
                    TransportFacilityKind::Garage | TransportFacilityKind::Depot
                ) || (matches!(self.service, FleetService::DepotTransfer { .. })
                    && !matches!(
                        facilities[self.stops.last().unwrap()].kind,
                        TransportFacilityKind::Garage | TransportFacilityKind::Depot
                    ))
                    || (matches!(self.service, FleetService::DepotFreight { .. })
                        && facilities[self.stops.last().unwrap()].linked_site.is_none())
                {
                    return Err(invalid("warehouse route source, buyer or destination"));
                }
            }
            _ => return Err(invalid("company service item or load")),
        }
        Ok(())
    }
}

impl TransportCompany {
    pub(super) fn validate_shape(
        &self,
        facilities: &BTreeMap<u64, TransportFacility>,
        elapsed: f64,
        catalog: Option<&Catalog>,
    ) -> Result<(), SandboxError> {
        if self.owner == 0
            || !valid_name(&self.name)
            || self.cash > MAX_MONEY
            || self.revenue > 1_000_000_000_000
            || self.next_id == 0
            || self.next_id == u64::MAX
            || self.drivers.len() > MAX_COMPANY_DRIVERS
            || self.routes.len() > MAX_COMPANY_ROUTES
            || self.depots.len() > MAX_COMPANY_DEPOTS
            || self.departed_wages > 1_000_000_000_000
            || self.departed_spending > 1_000_000_000_000
            || !finite_range(self.household_elapsed, 0., 60.)
            || self.household_elapsed >= 60.
            || self.household_consumed.len() > 2
            || catalog.is_some_and(|c| {
                self.household_consumed
                    .keys()
                    .any(|item| !c.items.contains(item))
            })
            || self.household_consumed.iter().any(|(item, count)| {
                !matches!(item.as_str(), "bread" | "water") || *count == 0 || *count > 1_000_000_000
            })
        {
            return Err(invalid("company identity, ledger or bounds"));
        }
        let mut depots = BTreeSet::new();
        for depot in &self.depots {
            if !depots.insert(depot.facility_id)
                || facilities.get(&depot.facility_id).is_none_or(|f| {
                    !matches!(
                        f.kind,
                        TransportFacilityKind::Garage | TransportFacilityKind::Depot
                    )
                })
                || depot.stock.values().map(|n| u64::from(*n)).sum::<u64>() > COMPANY_DEPOT_CAPACITY
            {
                return Err(invalid("company depot facility or capacity"));
            }
            crate::catalog::validate_inventory(&depot.stock)?;
            if catalog.is_some_and(|c| depot.stock.keys().any(|item| !c.items.contains(item))) {
                return Err(invalid("company depot catalog reference"));
            }
        }
        let mut ids = BTreeSet::new();
        let mut route_names = BTreeSet::new();
        for route in &self.routes {
            route.validate(facilities, catalog)?;
            if route.service.uses_company_stock()
                && (!depots.contains(&route.stops[0])
                    || (matches!(route.service, FleetService::DepotTransfer { .. })
                        && !depots.contains(route.stops.last().unwrap())))
            {
                return Err(invalid("warehouse route refers to an unowned depot"));
            }
            if route.id >= self.next_id
                || !ids.insert(route.id)
                || !route_names.insert(route.name.to_lowercase())
            {
                return Err(invalid("company route sequence or duplicate name"));
            }
        }
        let mut vehicles = BTreeSet::new();
        let mut names = BTreeSet::new();
        for driver in &self.drivers {
            if driver.id == 0
                || driver.id >= self.next_id
                || !ids.insert(driver.id)
                || !valid_name(&driver.name)
                || !names.insert(driver.name.to_lowercase())
                || driver.wages_earned > MAX_MONEY
                || driver.wages_spent > 1_000_000_000_000
                || !finite_range(driver.wage_fraction, 0., 1.)
                || driver.wage_fraction >= 1.
                || driver.deliveries_completed > 1_000_000_000
                || !finite_range(driver.stalled_seconds, 0., 15.)
            {
                return Err(invalid("company driver identity or payroll"));
            }
            let Some(vehicle) = driver.vehicle_id else {
                if driver.route_id.is_some()
                    || driver.running
                    || driver.trip.is_some()
                    || driver.next_stop != 0
                    || driver.status != FleetStatus::Unassigned
                {
                    return Err(invalid("unassigned company driver has active service"));
                }
                continue;
            };
            if vehicle == 0 || !vehicles.insert(vehicle) || driver.status == FleetStatus::Unassigned
            {
                return Err(invalid("duplicate or invalid company vehicle assignment"));
            }
            let route = self
                .routes
                .iter()
                .find(|route| Some(route.id) == driver.route_id)
                .ok_or_else(|| invalid("company driver route"))?;
            if driver.next_stop >= route.stops.len() {
                return Err(invalid("company route cursor"));
            }
            if let Some(trip) = &driver.trip {
                let pickup = facilities
                    .get(&trip.pickup)
                    .ok_or_else(|| invalid("fleet pickup"))?;
                let destination = facilities
                    .get(&trip.destination)
                    .ok_or_else(|| invalid("fleet destination"))?;
                if !finite_range(trip.pickup_odometer_m, 0., 1e12)
                    || !finite_range(trip.pickup_elapsed_seconds, 0., elapsed)
                    || !finite_range(trip.transferred_distance_m, 0., 1e12)
                    || (trip.returning && driver.running)
                    || trip.reward_escrow
                        != fleet_fare(&trip.service, pickup.position, destination.position)
                    || trip.reward_escrow > MAX_MONEY
                {
                    return Err(invalid("fleet escrow or trip accounting"));
                }
                match (&trip.service, &trip.goods_payment) {
                    (FleetService::DepotFreight { amount, .. }, Some(payment))
                        if Some(payment.shop_id) == destination.linked_site
                            && payment.unit_price > 0
                            && payment.unit_price <= MAX_MONEY
                            && payment.unit_price.checked_mul(u64::from(*amount))
                                == Some(payment.total)
                            && payment.total <= MAX_MONEY => {}
                    (FleetService::DepotFreight { .. }, _) => {
                        return Err(invalid("warehouse buyer escrow"));
                    }
                    (_, None) => {}
                    (_, Some(_)) => return Err(invalid("unexpected warehouse buyer escrow")),
                }
                match (&route.service, &trip.service) {
                    (
                        FleetService::Passengers { count: a },
                        FleetService::Passengers { count: b },
                    ) if a == b => {
                        let previous =
                            (driver.next_stop + route.stops.len() - 1) % route.stops.len();
                        if route.stops[previous] != trip.pickup
                            || route.stops[driver.next_stop] != trip.destination
                        {
                            return Err(invalid("passenger route stage"));
                        }
                    }
                    (
                        FleetService::Freight {
                            item: a,
                            amount: limit,
                        },
                        FleetService::Freight { item: b, amount },
                    ) if a == b && *amount > 0 && amount <= limit => {
                        if driver.next_stop == 0
                            || route.stops[0] != trip.pickup
                            || *route.stops.last().unwrap() != trip.destination
                        {
                            return Err(invalid("freight route stage"));
                        }
                    }
                    (
                        FleetService::DepotFreight {
                            item: a,
                            amount: limit,
                        },
                        FleetService::DepotFreight { item: b, amount },
                    )
                    | (
                        FleetService::DepotTransfer {
                            item: a,
                            amount: limit,
                        },
                        FleetService::DepotTransfer { item: b, amount },
                    ) if a == b && *amount > 0 && amount <= limit => {
                        if driver.next_stop == 0
                            || route.stops[0] != trip.pickup
                            || *route.stops.last().unwrap() != trip.destination
                        {
                            return Err(invalid("warehouse freight route stage"));
                        }
                    }
                    _ => return Err(invalid("fleet trip does not match its route")),
                }
            } else if route.service.cargo_load().is_some() && driver.next_stop != 0 {
                return Err(invalid("empty freight vehicle must return to pickup"));
            }
        }
        for depot in &self.depots {
            let reserved: u64 = self
                .drivers
                .iter()
                .filter_map(|d| d.trip.as_ref())
                .filter_map(|t| match &t.service {
                    FleetService::DepotFreight { amount, .. } if t.pickup == depot.facility_id => {
                        Some(u64::from(*amount))
                    }
                    FleetService::DepotTransfer { amount, .. }
                        if t.pickup == depot.facility_id
                            || (!t.returning && t.destination == depot.facility_id) =>
                    {
                        Some(u64::from(*amount))
                    }
                    _ => None,
                })
                .sum();
            if depot.stock.values().map(|n| u64::from(*n)).sum::<u64>() + reserved
                > COMPANY_DEPOT_CAPACITY
            {
                return Err(invalid(
                    "warehouse stock exceeds loaded arrival/refund reservations",
                ));
            }
        }
        if let Some(accounting) = &self.accounting {
            accounting.validate(self, elapsed)?;
        }
        Ok(())
    }
}

impl TransportWorld {
    pub(in crate::transport) fn validate_companies(
        &self,
        world: &SandboxState,
        catalog: Option<&Catalog>,
    ) -> Result<(), SandboxError> {
        if self.companies.len() > MAX_PLAYERS {
            return Err(SandboxError::Capacity);
        }
        let mut vehicles = BTreeSet::new();
        for (&owner, company) in &self.companies {
            company.validate_shape(&self.facilities, self.elapsed_seconds, catalog)?;
            if owner != company.owner
                || !self.drivers.contains_key(&owner)
                || !world.living.accounts.contains_key(&owner)
            {
                return Err(invalid("company owner principal"));
            }
            for route in &company.routes {
                if route.service.uses_company_stock() {
                    world.validate_company_depot_route(
                        owner,
                        route,
                        route.service.cargo_load().unwrap().0,
                    )?;
                }
            }
            for driver in &company.drivers {
                let Some(id) = driver.vehicle_id else {
                    continue;
                };
                let vehicle = self
                    .vehicles
                    .get(&id)
                    .ok_or_else(|| invalid("company vehicle"))?;
                if vehicle.owner != owner
                    || !vehicles.insert(id)
                    || vehicle.model.spec().required_license == DriverLicense::Emergency
                    || self
                        .jobs
                        .values()
                        .any(|job| job.assignment.as_ref().is_some_and(|a| a.vehicle_id == id))
                    || self.races.values().any(|run| run.vehicle_id == id)
                    || world.living.accounts[&owner].active_unit == id
                {
                    return Err(invalid(
                        "company vehicle ownership or conflicting assignment",
                    ));
                }
                let route = company
                    .routes
                    .iter()
                    .find(|route| Some(route.id) == driver.route_id)
                    .unwrap();
                world.validate_fleet_capacity(id, &route.service)?;
                let expected_cargo = match driver.trip.as_ref().map(|trip| &trip.service) {
                    Some(
                        FleetService::Freight { item, amount }
                        | FleetService::DepotFreight { item, amount }
                        | FleetService::DepotTransfer { item, amount },
                    ) => inventory(&[(item, *amount)]),
                    _ => Inventory::new(),
                };
                if vehicle.cargo != expected_cargo {
                    return Err(invalid("fleet load ledger differs from vehicle cargo"));
                }
                if driver
                    .trip
                    .as_ref()
                    .is_some_and(|trip| trip.pickup_odometer_m > vehicle.odometer_m + 1e-6)
                {
                    return Err(invalid("fleet mileage regressed"));
                }
            }
        }
        Ok(())
    }
}

impl TransportView {
    pub(in crate::transport) fn validate_company_view(
        &self,
        player: u64,
    ) -> Result<(), SandboxError> {
        let Some(company) = &self.company else {
            return Ok(());
        };
        if company.owner != player || !self.enabled {
            return Err(invalid("another owner's company ledger"));
        }
        let facilities = self.facilities.iter().map(|f| (f.id, f.clone())).collect();
        company.validate_shape(&facilities, self.elapsed_seconds, None)?;
        for driver in &company.drivers {
            if let Some(vehicle) = driver
                .vehicle_id
                .and_then(|id| self.vehicles.iter().find(|v| v.unit_id == id))
            {
                if vehicle.owner != player
                    || vehicle.model.spec().required_license == DriverLicense::Emergency
                {
                    return Err(invalid("company vehicle view ownership or qualification"));
                }
                let cargo = match driver.trip.as_ref().map(|trip| &trip.service) {
                    Some(
                        FleetService::Freight { item, amount }
                        | FleetService::DepotFreight { item, amount }
                        | FleetService::DepotTransfer { item, amount },
                    ) => inventory(&[(item, *amount)]),
                    _ => Inventory::new(),
                };
                if vehicle.cargo != cargo {
                    return Err(invalid("company vehicle view load"));
                }
            }
        }
        Ok(())
    }
}
