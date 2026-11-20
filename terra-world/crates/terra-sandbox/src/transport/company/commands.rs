use super::*;

impl SandboxState {
    /// A company assignment reserves the body against manual/RTS/job takeover.
    pub fn company_driver_for_vehicle(&self, vehicle: u64) -> Option<&CompanyDriver> {
        self.transport
            .companies
            .values()
            .flat_map(|company| company.drivers.iter())
            .find(|driver| driver.vehicle_id == Some(vehicle))
    }

    pub fn is_company_vehicle(&self, vehicle: u64) -> bool {
        self.company_driver_for_vehicle(vehicle).is_some()
    }

    pub(in crate::transport) fn company_action(
        &mut self,
        player: u64,
        action: CompanyCommand,
        catalog: &Catalog,
    ) -> Result<Outcome, SandboxError> {
        if self.is_player_dormant(player) {
            return Err(invalid(
                "resume the company owner before changing its business",
            ));
        }
        if let CompanyCommand::Create { name } = action {
            if !valid_name(&name) {
                return Err(invalid(
                    "company name must contain 1–32 printable characters",
                ));
            }
            if self.transport.companies.contains_key(&player) {
                return Err(invalid("you already own a transport company"));
            }
            self.require_company_garage(player)?;
            self.transport_charge(player, COMPANY_REGISTRATION_PRICE)?;
            let mut company = TransportCompany {
                owner: player,
                name,
                cash: 0,
                accounting: Some(CompanyAccounting::since_registration(
                    self.transport.elapsed_seconds,
                )),
                drivers: Vec::new(),
                routes: Vec::new(),
                revenue: 0,
                next_id: 1,
                depots: Vec::new(),
                departed_wages: 0,
                departed_spending: 0,
                household_consumed: Inventory::new(),
                household_elapsed: 0.,
            };
            company.accounting = Some(company.prepare_accounting(
                self.transport.elapsed_seconds,
                &[(
                    CompanyCashCategory::OwnerPaidRegistration,
                    COMPANY_REGISTRATION_PRICE,
                )],
            )?);
            self.transport.companies.insert(player, company);
            return Ok(outcome(
                "Company registered. Deposit funds, hire a driver and create a route",
            ));
        }
        let company = self
            .transport
            .companies
            .get(&player)
            .ok_or_else(|| invalid("register a transport company first"))?
            .clone();
        match action {
            CompanyCommand::Create { .. } => unreachable!(),
            CompanyCommand::DismissDriver { driver_id } => {
                self.dismiss_company_driver(player, driver_id)
            }
            CompanyCommand::ReturnLoad { driver_id } => self.return_company_load(player, driver_id),
            CompanyCommand::HoldVehicle { driver_id } => {
                self.hold_company_vehicle(player, driver_id)
            }
            CompanyCommand::HandoverLoad {
                driver_id,
                relief_driver_id,
                vehicle_id,
            } => self.handover_company_load(player, driver_id, relief_driver_id, vehicle_id),
            CompanyCommand::OpenDepot { facility_id } => {
                self.open_company_depot(player, facility_id)
            }
            CompanyCommand::CloseDepot { facility_id } => {
                self.close_company_depot(player, facility_id)
            }
            CompanyCommand::TransferDepotStock {
                facility_id,
                item,
                amount,
                deposit,
            } => self.transfer_company_depot_stock(
                player,
                facility_id,
                &item,
                amount,
                deposit,
                catalog,
            ),
            CompanyCommand::TradeDepotStock {
                facility_id,
                shop_id,
                item,
                amount,
                buy,
                quoted_total,
            } => self.trade_company_depot_stock(
                player,
                facility_id,
                shop_id,
                &item,
                amount,
                buy,
                quoted_total,
                catalog,
            ),
            CompanyCommand::RoadsideService {
                vehicle_id,
                facility_id,
                litres,
                repair,
                quoted_total,
            } => self.service_company_vehicle(
                player,
                vehicle_id,
                facility_id,
                litres,
                repair,
                quoted_total,
            ),
            CompanyCommand::Deposit { amount } | CompanyCommand::Withdraw { amount } => {
                if amount == 0 || amount > MAX_MONEY {
                    return Err(invalid(
                        "transfer a positive amount within the account limit",
                    ));
                }
                let deposit = matches!(action, CompanyCommand::Deposit { .. });
                let wallet = self.living.accounts[&player].money;
                let (cash, wallet) = if deposit {
                    (
                        company
                            .cash
                            .checked_add(amount)
                            .filter(|n| *n <= MAX_MONEY)
                            .ok_or(SandboxError::Capacity)?,
                        wallet
                            .checked_sub(amount)
                            .ok_or_else(|| SandboxError::Insufficient("personal credits".into()))?,
                    )
                } else {
                    (
                        company
                            .cash
                            .checked_sub(amount)
                            .ok_or_else(|| SandboxError::Insufficient("company credits".into()))?,
                        wallet
                            .checked_add(amount)
                            .filter(|n| *n <= MAX_MONEY)
                            .ok_or(SandboxError::Capacity)?,
                    )
                };
                let accounting = company.prepare_accounting(
                    self.transport.elapsed_seconds,
                    &[(
                        if deposit {
                            CompanyCashCategory::OwnerDeposit
                        } else {
                            CompanyCashCategory::OwnerWithdrawal
                        },
                        amount,
                    )],
                )?;
                let company = self.transport.companies.get_mut(&player).unwrap();
                company.cash = cash;
                company.accounting = Some(accounting);
                self.living.accounts.get_mut(&player).unwrap().money = wallet;
                Ok(outcome(if deposit {
                    "Funds transferred to company"
                } else {
                    "Company funds withdrawn"
                }))
            }
            CompanyCommand::FundPublicServices { amount } => {
                if amount == 0 || amount > MAX_MONEY {
                    return Err(invalid("positive service-fund contribution required"));
                }
                let cash = company.cash.checked_sub(amount).ok_or_else(|| {
                    SandboxError::Insufficient("company service-fund contribution".into())
                })?;
                let treasury = self
                    .transport
                    .treasury
                    .checked_add(amount)
                    .filter(|n| *n <= MAX_MONEY)
                    .ok_or(SandboxError::Capacity)?;
                let accounting = company.prepare_accounting(
                    self.transport.elapsed_seconds,
                    &[(CompanyCashCategory::PublicServiceContribution, amount)],
                )?;
                let company = self.transport.companies.get_mut(&player).unwrap();
                company.cash = cash;
                company.accounting = Some(accounting);
                self.transport.treasury = treasury;
                Ok(outcome(
                    "Contribution transferred to the shared service issuer. It can fund public transport work and is not refundable",
                ))
            }
            CompanyCommand::HireDriver { name } => {
                self.require_company_garage(player)?;
                if !valid_name(&name)
                    || company
                        .drivers
                        .iter()
                        .any(|d| d.name.eq_ignore_ascii_case(&name))
                {
                    return Err(invalid(
                        "choose a unique driver name of 1–32 printable characters",
                    ));
                }
                if company.drivers.len() >= MAX_COMPANY_DRIVERS {
                    return Err(SandboxError::Capacity);
                }
                let cash = company
                    .cash
                    .checked_sub(COMPANY_HIRE_PRICE)
                    .ok_or_else(|| {
                        SandboxError::Insufficient("company recruitment funds".into())
                    })?;
                let treasury = self
                    .transport
                    .treasury
                    .checked_add(COMPANY_HIRE_PRICE)
                    .filter(|n| *n <= MAX_MONEY)
                    .ok_or(SandboxError::Capacity)?;
                let id = company.next_id;
                let next = id
                    .checked_add(1)
                    .filter(|n| *n != u64::MAX)
                    .ok_or(SandboxError::Capacity)?;
                let accounting = company.prepare_accounting(
                    self.transport.elapsed_seconds,
                    &[(CompanyCashCategory::Recruitment, COMPANY_HIRE_PRICE)],
                )?;
                let company = self.transport.companies.get_mut(&player).unwrap();
                company.cash = cash;
                company.accounting = Some(accounting);
                company.next_id = next;
                company.drivers.push(CompanyDriver {
                    id,
                    name,
                    vehicle_id: None,
                    route_id: None,
                    next_stop: 0,
                    running: false,
                    status: FleetStatus::Unassigned,
                    trip: None,
                    wages_earned: 0,
                    wages_spent: 0,
                    wage_fraction: 0.,
                    stalled_seconds: 0.,
                    deliveries_completed: 0,
                });
                self.transport.treasury = treasury;
                Ok(Outcome { message: "Qualified road, passenger and heavy-goods driver hired; pay starts when dispatched".into(), created_id: Some(id) })
            }
            CompanyCommand::CreateRoute {
                name,
                stops,
                service,
            } => {
                if company.routes.len() >= MAX_COMPANY_ROUTES {
                    return Err(SandboxError::Capacity);
                }
                if company
                    .routes
                    .iter()
                    .any(|route| route.name.eq_ignore_ascii_case(&name))
                {
                    return Err(invalid("choose a unique route name"));
                }
                let route = CompanyRoute {
                    id: company.next_id,
                    name,
                    stops,
                    service,
                };
                route.validate(&self.transport.facilities, Some(catalog))?;
                if let FleetService::Freight { item, .. } = &route.service {
                    self.transport_stock(route.stops[0])?;
                    self.transport_stock(*route.stops.last().unwrap())?;
                    if self.fleet_destination_target(*route.stops.last().unwrap(), item) == 0 {
                        return Err(invalid(
                            "freight destination must consume or sell this item",
                        ));
                    }
                }
                if let FleetService::DepotFreight { item, .. }
                | FleetService::DepotTransfer { item, .. } = &route.service
                {
                    self.validate_company_depot_route(player, &route, item)?;
                }
                let id = route.id;
                let next = id
                    .checked_add(1)
                    .filter(|n| *n != u64::MAX)
                    .ok_or(SandboxError::Capacity)?;
                let company = self.transport.companies.get_mut(&player).unwrap();
                company.next_id = next;
                company.routes.push(route);
                Ok(Outcome {
                    message: "Commercial route created".into(),
                    created_id: Some(id),
                })
            }
            CompanyCommand::DeleteRoute { route_id } => {
                if company.drivers.iter().any(|d| d.route_id == Some(route_id)) {
                    return Err(invalid("release assigned vehicles before deleting a route"));
                }
                if !company.routes.iter().any(|route| route.id == route_id) {
                    return Err(invalid("company route"));
                }
                self.transport
                    .companies
                    .get_mut(&player)
                    .unwrap()
                    .routes
                    .retain(|route| route.id != route_id);
                Ok(outcome("Unused route deleted"))
            }
            CompanyCommand::Assign {
                driver_id,
                vehicle_id,
                route_id,
            } => {
                let driver = company
                    .drivers
                    .iter()
                    .find(|driver| driver.id == driver_id)
                    .ok_or_else(|| invalid("company driver"))?;
                let route = company
                    .routes
                    .iter()
                    .find(|route| route.id == route_id)
                    .ok_or_else(|| invalid("company route"))?;
                let vehicle = self.owned_vehicle(player, vehicle_id)?;
                let body = self
                    .living
                    .units
                    .get(&vehicle_id)
                    .ok_or_else(|| invalid("vehicle is outside resident regions"))?;
                if driver.vehicle_id.is_some()
                    || self.is_company_vehicle(vehicle_id)
                    || self.vehicle_has_job(vehicle_id)
                    || self
                        .transport
                        .races
                        .values()
                        .any(|r| r.vehicle_id == vehicle_id)
                    || !vehicle.cargo.is_empty()
                    || body_speed(body) > 0.5
                {
                    return Err(invalid(
                        "assign a stopped, empty vehicle and an unassigned driver",
                    ));
                }
                if self.living.accounts[&player].active_unit == vehicle_id {
                    return Err(invalid(
                        "exit and select your avatar before handing the vehicle to a hired driver",
                    ));
                }
                if vehicle.model.spec().required_license == DriverLicense::Emergency {
                    return Err(invalid(
                        "company employees do not hold emergency-service qualifications",
                    ));
                }
                self.validate_fleet_capacity(vehicle_id, &route.service)?;
                let mut accounting =
                    company.prepare_accounting(self.transport.elapsed_seconds, &[])?;
                accounting.personal_vehicle_used = true;
                self.transport
                    .companies
                    .get_mut(&player)
                    .unwrap()
                    .accounting = Some(accounting);
                let driver = self
                    .transport
                    .companies
                    .get_mut(&player)
                    .unwrap()
                    .drivers
                    .iter_mut()
                    .find(|driver| driver.id == driver_id)
                    .unwrap();
                driver.vehicle_id = Some(vehicle_id);
                driver.route_id = Some(route_id);
                driver.next_stop = 0;
                driver.running = false;
                driver.stalled_seconds = 0.;
                driver.status = FleetStatus::Paused;
                self.park_fleet_vehicle(vehicle_id);
                Ok(outcome(
                    "Driver and vehicle assigned. Start the route when funded",
                ))
            }
            CompanyCommand::SetDriverRunning { driver_id, running } => {
                let driver = company
                    .drivers
                    .iter()
                    .find(|driver| driver.id == driver_id)
                    .ok_or_else(|| invalid("company driver"))?;
                let vehicle = driver
                    .vehicle_id
                    .ok_or_else(|| invalid("assign a vehicle and route first"))?;
                if !self.living.units.contains_key(&vehicle) {
                    return Err(invalid("vehicle is outside resident regions"));
                }
                if running && company.cash == 0 {
                    return Err(SandboxError::Insufficient("company payroll funds".into()));
                }
                let loaded = driver.trip.is_some();
                let returning = driver.trip.as_ref().is_some_and(|trip| trip.returning);
                let driver = self
                    .transport
                    .companies
                    .get_mut(&player)
                    .unwrap()
                    .drivers
                    .iter_mut()
                    .find(|driver| driver.id == driver_id)
                    .unwrap();
                driver.running = running && !returning;
                driver.stalled_seconds = 0.;
                driver.status = if running {
                    FleetStatus::Driving
                } else if loaded {
                    FleetStatus::Delivering
                } else {
                    FleetStatus::Paused
                };
                if !running && !loaded {
                    self.park_fleet_vehicle(vehicle);
                }
                Ok(outcome(if running {
                    "Route dispatched; payroll uses physical seconds"
                } else if loaded {
                    "Driver will deliver the current load, then park"
                } else {
                    "Empty vehicle parked; payroll paused"
                }))
            }
            CompanyCommand::ReleaseVehicle { driver_id } => {
                let driver = company
                    .drivers
                    .iter()
                    .find(|driver| driver.id == driver_id)
                    .ok_or_else(|| invalid("company driver"))?;
                let vehicle = driver
                    .vehicle_id
                    .ok_or_else(|| invalid("driver has no vehicle"))?;
                let unit = self
                    .living
                    .units
                    .get(&vehicle)
                    .ok_or_else(|| invalid("vehicle is outside resident regions"))?;
                if driver.running || driver.trip.is_some() || body_speed(unit) > 0.5 {
                    return Err(invalid(
                        "pause the route and finish its current load before releasing the vehicle",
                    ));
                }
                self.park_fleet_vehicle(vehicle);
                let driver = self
                    .transport
                    .companies
                    .get_mut(&player)
                    .unwrap()
                    .drivers
                    .iter_mut()
                    .find(|driver| driver.id == driver_id)
                    .unwrap();
                driver.vehicle_id = None;
                driver.route_id = None;
                driver.next_stop = 0;
                driver.status = FleetStatus::Unassigned;
                Ok(outcome(
                    "Vehicle released; the employee's earned wages remain in their account",
                ))
            }
        }
    }

    pub(super) fn require_company_garage(&self, player: u64) -> Result<(), SandboxError> {
        if self.transport.facilities.values().any(|facility| {
            facility.kind == TransportFacilityKind::Garage
                && near(self.players[&player].position, facility.position)
        }) {
            Ok(())
        } else {
            Err(SandboxError::OutOfReach)
        }
    }

    pub(super) fn validate_fleet_capacity(
        &self,
        vehicle: u64,
        service: &FleetService,
    ) -> Result<(), SandboxError> {
        let model = self
            .transport
            .vehicles
            .get(&vehicle)
            .ok_or_else(|| invalid("fleet vehicle"))?
            .model;
        match service {
            FleetService::Passengers { count } if *count <= model.spec().passenger_seats => Ok(()),
            FleetService::Freight { amount, .. }
            | FleetService::DepotFreight { amount, .. }
            | FleetService::DepotTransfer { amount, .. }
                if *amount <= self.vehicle_cargo_capacity(vehicle) =>
            {
                Ok(())
            }
            _ => Err(invalid("vehicle lacks capacity for the route load")),
        }
    }

    pub(super) fn park_fleet_vehicle(&mut self, vehicle: u64) {
        if let Some(unit) = self.living.units.get_mut(&vehicle) {
            unit.order = UnitOrder::Idle;
            unit.path.clear();
            unit.path_index = 0;
            unit.velocity_enu_m_s = [0.; 3];
        }
        if let Some(vehicle) = self.transport.vehicles.get_mut(&vehicle) {
            vehicle.autopilot = false;
            vehicle.parking_brake = true;
            vehicle.dynamics.stop();
        }
    }

    /// Bounded restocking policy, shared with pickup and route authoring.
    pub fn fleet_destination_target(&self, facility: u64, item: &str) -> u32 {
        let Some(site) = self
            .transport
            .facilities
            .get(&facility)
            .and_then(|f| f.linked_site)
        else {
            return 0;
        };
        if let Some(producer) = self.living.producers.get(&site) {
            return producer
                .inputs
                .get(item)
                .copied()
                .unwrap_or(0)
                .saturating_mul(16)
                .min(256);
        }
        self.living
            .shops
            .get(&site)
            .filter(|shop| shop.prices.contains_key(item))
            .map_or(0, |_| 64)
    }
}
