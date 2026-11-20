use super::*;
use crate::catalog::{exchange, inventory};

impl SandboxState {
    /// Profile installation works before living population bootstrap. Restoring
    /// an existing world replaces that provisional principal/profile together.
    pub fn set_character_profile(
        &mut self,
        player: u64,
        profile: CharacterProfile,
    ) -> Result<(), SandboxError> {
        profile.validate()?;
        if !self.players.contains_key(&player) {
            return Err(SandboxError::PlayerMissing);
        }
        if let Some(account) = self.living.accounts.get(&player) {
            let avatar = self
                .living
                .units
                .get_mut(&account.avatar_unit)
                .ok_or_else(|| invalid("profile avatar"))?;
            avatar.name = profile.name.clone();
        }
        self.transport.profiles.insert(player, profile);
        Ok(())
    }

    pub(crate) fn apply_transport(
        &mut self,
        player: u64,
        action: TransportCommand,
        catalog: &Catalog,
    ) -> Result<Outcome, SandboxError> {
        let placement_position = match &action {
            TransportCommand::ExitVehicle => Some(self.vehicle_exit_position(player, &Some)?),
            TransportCommand::BuyVehicle { model, facility_id }
            | TransportCommand::RentVehicle { model, facility_id } => Some(
                self.garage_vehicle_position(player, *model, *facility_id, &|point, _| {
                    Some(point)
                })?,
            ),
            _ => None,
        };
        self.apply_transport_transaction(player, action, catalog, placement_position)
    }

    pub(super) fn apply_transport_transaction(
        &mut self,
        player: u64,
        action: TransportCommand,
        catalog: &Catalog,
        placement_position: Option<GeoPosition>,
    ) -> Result<Outcome, SandboxError> {
        if let TransportCommand::SetCharacterProfile { profile } = action {
            self.set_character_profile(player, profile)?;
            return Ok(outcome("Character updated"));
        }
        if !self.transport.enabled || !self.living.accounts.contains_key(&player) {
            return Err(invalid(
                "transport services are not initialized for this driver",
            ));
        }
        // Transport commands are low-frequency transactions. Copy only their
        // bounded ledgers, never the terrain/source/resource working set.
        let capacity = self.living_unit_count();
        let mut candidate = SandboxState {
            players: self.players.clone(),
            living: self.living.clone(),
            transport: self.transport.clone(),
            dormant: self.dormant.clone(),
            dormant_players: self.dormant_players.clone(),
            ..SandboxState::default()
        };
        let result =
            candidate.transport_action(player, action, catalog, capacity, placement_position)?;
        candidate.living.validate(&candidate, Some(catalog))?;
        candidate.transport.validate(&candidate, Some(catalog))?;
        self.players = candidate.players;
        self.living = candidate.living;
        self.transport = candidate.transport;
        Ok(result)
    }

    fn transport_action(
        &mut self,
        player: u64,
        action: TransportCommand,
        catalog: &Catalog,
        aggregate_units: usize,
        placement_position: Option<GeoPosition>,
    ) -> Result<Outcome, SandboxError> {
        let reserved_vehicle = match &action {
            TransportCommand::ReturnRental { vehicle_id, .. }
            | TransportCommand::SellVehicle { vehicle_id, .. }
            | TransportCommand::EnterVehicle { vehicle_id }
            | TransportCommand::SetSignals { vehicle_id, .. }
            | TransportCommand::SetAutopilot { vehicle_id, .. }
            | TransportCommand::AcceptJob { vehicle_id, .. }
            | TransportCommand::HitchTrailer { vehicle_id, .. }
            | TransportCommand::UnhitchTrailer { vehicle_id }
            | TransportCommand::StartRace { vehicle_id, .. } => Some(*vehicle_id),
            _ => None,
        };
        if reserved_vehicle.is_some_and(|id| self.is_company_vehicle(id)) {
            return Err(invalid(
                "pause the company route and release its vehicle before taking control or assigning other work",
            ));
        }
        match action {
            TransportCommand::Company { action } => self.company_action(player, action, catalog),
            TransportCommand::SetCharacterProfile { .. } => unreachable!(),
            TransportCommand::RentVehicle { model, facility_id } => self.rent_vehicle(
                player,
                model,
                facility_id,
                aggregate_units,
                placement_position.ok_or_else(|| invalid("validated garage parking"))?,
            ),
            TransportCommand::RenewRental { vehicle_id } => {
                self.renew_vehicle_rental(player, vehicle_id)
            }
            TransportCommand::ReturnRental {
                vehicle_id,
                facility_id,
                quoted_refund,
            } => self.return_vehicle_rental(player, vehicle_id, facility_id, quoted_refund),
            TransportCommand::BuyVehicle { model, facility_id } => {
                self.require_garage(player, facility_id)?;
                let spec = model.spec();
                self.require_license(player, spec.required_license)?;
                if aggregate_units >= MAX_LIVING_UNITS
                    || self.transport.vehicles.len() >= MAX_TRANSPORT_VEHICLES
                    || self
                        .transport
                        .vehicles
                        .values()
                        .filter(|v| v.owner == player)
                        .count()
                        >= MAX_PLAYER_VEHICLES
                {
                    return Err(SandboxError::Capacity);
                }
                self.transport_charge(player, spec.price)?;
                let position =
                    placement_position.ok_or_else(|| invalid("validated garage parking"))?;
                let fuel_l = spec.fuel_capacity_l * 0.65;
                if self.transport.facilities[&facility_id].fuel_l < fuel_l {
                    return Err(SandboxError::Insufficient("dealer delivery fuel".into()));
                }
                self.transport
                    .facilities
                    .get_mut(&facility_id)
                    .unwrap()
                    .fuel_l -= fuel_l;
                let id = self.living.make_unit(
                    spec.name.into(),
                    ActorKind::Vehicle,
                    UnitRole::Worker,
                    Some(player),
                    position,
                )?;
                self.transport
                    .vehicles
                    .insert(id, VehicleState::new(id, player, model));
                Ok(Outcome {
                    message: format!("{} purchased. It is parked at the garage", spec.name),
                    created_id: Some(id),
                })
            }
            TransportCommand::SellVehicle {
                vehicle_id,
                facility_id,
            } => {
                self.require_vehicle_at_garage(player, vehicle_id, facility_id)?;
                let vehicle = self.owned_vehicle(player, vehicle_id)?;
                if vehicle.rental.is_some()
                    || vehicle.trailer.is_some()
                    || !vehicle.cargo.is_empty()
                    || self.vehicle_has_job(vehicle_id)
                    || self
                        .transport
                        .races
                        .values()
                        .any(|r| r.vehicle_id == vehicle_id)
                    || self.living.accounts[&player].active_unit == vehicle_id
                {
                    return Err(invalid(
                        "exit the empty, unassigned vehicle and detach its trailer before selling",
                    ));
                }
                let value =
                    (vehicle.model.spec().price as f64 * 0.65 * f64::from(vehicle.condition))
                        .round() as u64;
                let returned_fuel = self.transport.facilities[&facility_id].fuel_l + vehicle.fuel_l;
                if returned_fuel > 1_000_000. {
                    return Err(SandboxError::Capacity);
                }
                self.transport_pay_from_treasury(player, value)?;
                self.transport
                    .facilities
                    .get_mut(&facility_id)
                    .unwrap()
                    .fuel_l = returned_fuel;
                self.transport.vehicles.remove(&vehicle_id);
                self.living.units.remove(&vehicle_id);
                Ok(outcome(format!(
                    "Vehicle sold for {:.2} credits",
                    value as f64 / 100.
                )))
            }
            TransportCommand::EnterVehicle { vehicle_id } => {
                let vehicle = self.owned_vehicle(player, vehicle_id)?;
                self.require_license(player, vehicle.model.spec().required_license)?;
                let unit = &self.living.units[&vehicle_id];
                if !near(self.players[&player].position, unit.position) {
                    return Err(SandboxError::OutOfReach);
                }
                if speed(unit) > 0.5 {
                    return Err(invalid("stop the vehicle before entering"));
                }
                self.apply_living(
                    player,
                    LivingCommand::Possess {
                        unit_id: vehicle_id,
                    },
                    catalog,
                )?;
                let vehicle = self.transport.vehicles.get_mut(&vehicle_id).unwrap();
                vehicle.autopilot = false;
                vehicle.parking_brake = false;
                Ok(outcome("Driving. W/S accelerate and reverse; A/D steer"))
            }
            TransportCommand::ExitVehicle => {
                let active = self.living.accounts[&player].active_unit;
                self.owned_vehicle(player, active)?;
                if speed(&self.living.units[&active]) > 0.5 {
                    return Err(invalid("stop the vehicle before exiting"));
                }
                let position =
                    placement_position.ok_or_else(|| invalid("validated vehicle exit"))?;
                let heading = self.living.units[&active].heading_deg;
                let avatar = self.living.accounts[&player].avatar_unit;
                let avatar_unit = self
                    .living
                    .units
                    .get_mut(&avatar)
                    .ok_or_else(|| invalid("driver avatar"))?;
                avatar_unit.position = position;
                avatar_unit.heading_deg = heading;
                self.apply_living(player, LivingCommand::Possess { unit_id: avatar }, catalog)?;
                self.living.units.get_mut(&active).unwrap().velocity_enu_m_s = [0.; 3];
                let vehicle = self.transport.vehicles.get_mut(&active).unwrap();
                vehicle.autopilot = false;
                vehicle.parking_brake = true;
                vehicle.dynamics.stop();
                vehicle.dynamics.engine_rpm = 0.;
                Ok(outcome("On foot. Vehicle parked"))
            }
            TransportCommand::Refuel {
                vehicle_id,
                facility_id,
                litres,
            } => {
                self.require_vehicle_at_garage(player, vehicle_id, facility_id)?;
                if litres == 0 || litres > 500 {
                    return Err(invalid("refuel quantity must be 1..500 litres"));
                }
                let amount = f64::from(litres);
                let vehicle = self.owned_vehicle(player, vehicle_id)?;
                if vehicle.fuel_l + amount > vehicle.model.spec().fuel_capacity_l + 1e-6 {
                    return Err(invalid("requested fuel exceeds remaining tank capacity"));
                }
                if self.transport.facilities[&facility_id].fuel_l + 1e-6 < amount {
                    return Err(SandboxError::Insufficient("station fuel".into()));
                }
                let cost = u64::from(litres) * 150;
                self.charge_owner_paid_company_vehicle(
                    player,
                    vehicle_id,
                    CompanyCashCategory::OwnerPaidFuel,
                    cost,
                )?;
                self.transport
                    .facilities
                    .get_mut(&facility_id)
                    .unwrap()
                    .fuel_l -= amount;
                self.transport.vehicles.get_mut(&vehicle_id).unwrap().fuel_l += amount;
                Ok(outcome(format!("Added {litres} litres")))
            }
            TransportCommand::Repair {
                vehicle_id,
                facility_id,
            } => {
                self.require_vehicle_at_garage(player, vehicle_id, facility_id)?;
                let vehicle = self.owned_vehicle(player, vehicle_id)?;
                if vehicle.condition >= 0.9999 {
                    return Err(invalid("vehicle does not need repair"));
                }
                let cost = repair_price(vehicle);
                self.charge_owner_paid_company_vehicle(
                    player,
                    vehicle_id,
                    CompanyCashCategory::OwnerPaidRepair,
                    cost,
                )?;
                self.transport
                    .vehicles
                    .get_mut(&vehicle_id)
                    .unwrap()
                    .condition = 1.;
                Ok(outcome(format!(
                    "Vehicle repaired for {:.2} credits",
                    cost as f64 / 100.
                )))
            }
            TransportCommand::Paint {
                vehicle_id,
                facility_id,
                paint,
            } => {
                self.require_vehicle_at_garage(player, vehicle_id, facility_id)?;
                if paint > 7 {
                    return Err(invalid("vehicle paint"));
                }
                if self.transport.vehicles[&vehicle_id].paint == paint {
                    return Err(invalid("this paint is already installed"));
                }
                self.charge_owner_paid_company_vehicle(
                    player,
                    vehicle_id,
                    CompanyCashCategory::OwnerPaidPaint,
                    500,
                )?;
                self.transport.vehicles.get_mut(&vehicle_id).unwrap().paint = paint;
                Ok(outcome("Vehicle paint applied"))
            }
            TransportCommand::Upgrade {
                vehicle_id,
                facility_id,
                engine,
            } => {
                self.require_vehicle_at_garage(player, vehicle_id, facility_id)?;
                let vehicle = self.owned_vehicle(player, vehicle_id)?;
                if vehicle.rental.is_some() {
                    return Err(invalid("rental mechanical upgrades are not offered"));
                }
                let level = if engine {
                    vehicle.engine_level
                } else {
                    vehicle.brakes_level
                };
                if level >= 3 {
                    return Err(invalid("this component is already at its maximum level"));
                }
                let cost = upgrade_price(level);
                self.charge_owner_paid_company_vehicle(
                    player,
                    vehicle_id,
                    CompanyCashCategory::OwnerPaidUpgrade,
                    cost,
                )?;
                let vehicle = self.transport.vehicles.get_mut(&vehicle_id).unwrap();
                if engine {
                    vehicle.engine_level += 1;
                } else {
                    vehicle.brakes_level += 1;
                }
                Ok(outcome(if engine {
                    "Engine upgraded"
                } else {
                    "Brakes upgraded"
                }))
            }
            TransportCommand::BuyLicense {
                license,
                facility_id,
            } => {
                self.require_garage(player, facility_id)?;
                let driver = self
                    .transport
                    .drivers
                    .get(&player)
                    .ok_or_else(|| invalid("driver progress"))?;
                if driver.licenses.contains(&license) {
                    return Err(invalid("license already held"));
                }
                if driver.jobs_completed < license.required_jobs() {
                    return Err(invalid("complete more transport jobs before this license"));
                }
                self.transport_charge(player, license.cost())?;
                self.transport
                    .drivers
                    .get_mut(&player)
                    .unwrap()
                    .licenses
                    .insert(license);
                Ok(outcome(format!("{} license issued", license.name())))
            }
            TransportCommand::SetSignals {
                vehicle_id,
                lights,
                indicator,
                parking_brake,
            } => {
                self.require_active_vehicle(player, vehicle_id)?;
                let vehicle = self.transport.vehicles.get_mut(&vehicle_id).unwrap();
                vehicle.lights = lights;
                vehicle.indicator = indicator;
                vehicle.parking_brake = parking_brake;
                Ok(outcome("Vehicle controls updated"))
            }
            TransportCommand::SetAutopilot {
                vehicle_id,
                enabled,
                destination,
            } => {
                let vehicle = self.owned_vehicle(player, vehicle_id)?;
                if enabled {
                    self.require_license(player, vehicle.model.spec().required_license)?;
                    let destination =
                        destination.ok_or_else(|| invalid("choose a route destination"))?;
                    destination.validate()?;
                    let position = self.living.units[&vehicle_id].position;
                    if position.surface_distance_m(destination) > 2000. {
                        return Err(invalid("autopilot route is limited to 2 km"));
                    }
                    if self.transport.vehicles[&vehicle_id].fuel_l <= 0. {
                        return Err(SandboxError::Insufficient("fuel".into()));
                    }
                    let path = self.living.vehicle_route_between(position, destination);
                    let unit = self.living.units.get_mut(&vehicle_id).unwrap();
                    unit.order = UnitOrder::Move { destination };
                    unit.path = path;
                    unit.path_index = 0;
                    if self.living.accounts[&player].active_unit == vehicle_id {
                        self.living.accounts.get_mut(&player).unwrap().possessed = false;
                    }
                } else {
                    let unit = self.living.units.get_mut(&vehicle_id).unwrap();
                    unit.order = UnitOrder::Idle;
                    unit.path.clear();
                    unit.path_index = 0;
                    unit.velocity_enu_m_s = [0.; 3];
                }
                let vehicle = self.transport.vehicles.get_mut(&vehicle_id).unwrap();
                vehicle.autopilot = enabled;
                vehicle.parking_brake = false;
                Ok(outcome(if enabled {
                    "Road autopilot engaged; loaded terrain is required"
                } else {
                    "Autopilot stopped"
                }))
            }
            TransportCommand::AcceptJob { job_id, vehicle_id } => {
                self.accept_transport_job(player, job_id, vehicle_id)
            }
            TransportCommand::PickupJob { job_id } => self.pickup_transport_job(player, job_id),
            TransportCommand::CompleteJob { job_id } => {
                self.finish_transport_job(player, job_id, false)
            }
            TransportCommand::CancelJob { job_id } => {
                let job = self
                    .transport
                    .jobs
                    .get(&job_id)
                    .ok_or_else(|| invalid("transport job"))?
                    .clone();
                let assignment = job
                    .assignment
                    .as_ref()
                    .ok_or_else(|| invalid("job has not been accepted"))?;
                if assignment.owner != player {
                    return Err(SandboxError::NotOwner);
                }
                if assignment.phase == TransportJobPhase::ToPickup {
                    self.refund_transport_job(job_id)?;
                    Ok(outcome("Job cancelled; reserved goods and reward returned"))
                } else if near(
                    self.living.units[&assignment.vehicle_id].position,
                    self.transport.facilities[&job.pickup_facility].position,
                ) {
                    self.finish_transport_job(player, job_id, true)
                } else {
                    self.transport
                        .jobs
                        .get_mut(&job_id)
                        .unwrap()
                        .assignment
                        .as_mut()
                        .unwrap()
                        .phase = TransportJobPhase::Returning;
                    Ok(outcome(
                        "Return passengers and cargo to the pickup point to cancel",
                    ))
                }
            }
            TransportCommand::BuyTrailer { facility_id } => {
                self.require_garage(player, facility_id)?;
                if self.transport.trailers.len() >= MAX_TRANSPORT_TRAILERS {
                    return Err(SandboxError::Capacity);
                }
                self.transport_charge(player, 15_000)?;
                let id = self.transport.next()?;
                self.transport.trailers.insert(
                    id,
                    TransportTrailer {
                        id,
                        owner: player,
                        position: self.transport.facilities[&facility_id].position,
                        capacity: 160,
                        attached_to: None,
                    },
                );
                Ok(Outcome {
                    message: "Cargo trailer purchased".into(),
                    created_id: Some(id),
                })
            }
            TransportCommand::HitchTrailer {
                vehicle_id,
                trailer_id,
            } => {
                let vehicle = self.owned_vehicle(player, vehicle_id)?;
                if !vehicle.model.can_tow()
                    || vehicle.trailer.is_some()
                    || self.vehicle_has_job(vehicle_id)
                {
                    return Err(invalid(
                        "this vehicle cannot attach another trailer while assigned",
                    ));
                }
                let trailer = self
                    .transport
                    .trailers
                    .get(&trailer_id)
                    .ok_or_else(|| invalid("trailer"))?;
                if trailer.owner != player {
                    return Err(SandboxError::NotOwner);
                }
                if trailer.attached_to.is_some() {
                    return Err(invalid("trailer is already attached"));
                }
                if speed(&self.living.units[&vehicle_id]) > 0.5
                    || !near(self.living.units[&vehicle_id].position, trailer.position)
                    || !near(self.players[&player].position, trailer.position)
                {
                    return Err(SandboxError::OutOfReach);
                }
                self.transport
                    .vehicles
                    .get_mut(&vehicle_id)
                    .unwrap()
                    .trailer = Some(trailer_id);
                self.transport
                    .trailers
                    .get_mut(&trailer_id)
                    .unwrap()
                    .attached_to = Some(vehicle_id);
                Ok(outcome("Trailer attached; cargo capacity increased"))
            }
            TransportCommand::UnhitchTrailer { vehicle_id } => {
                let vehicle = self.owned_vehicle(player, vehicle_id)?;
                let trailer_id = vehicle
                    .trailer
                    .ok_or_else(|| invalid("no trailer attached"))?;
                if !vehicle.cargo.is_empty()
                    || self.vehicle_has_job(vehicle_id)
                    || speed(&self.living.units[&vehicle_id]) > 0.5
                    || !near(
                        self.players[&player].position,
                        self.living.units[&vehicle_id].position,
                    )
                {
                    return Err(invalid(
                        "stop the empty, unassigned vehicle before detaching",
                    ));
                }
                self.transport
                    .vehicles
                    .get_mut(&vehicle_id)
                    .unwrap()
                    .trailer = None;
                let trailer = self.transport.trailers.get_mut(&trailer_id).unwrap();
                trailer.attached_to = None;
                trailer.position = self.living.units[&vehicle_id].position;
                Ok(outcome("Trailer detached"))
            }
            TransportCommand::StartRace {
                course_id,
                vehicle_id,
            } => {
                self.require_active_vehicle(player, vehicle_id)?;
                if self.vehicle_has_job(vehicle_id) || self.transport.races.contains_key(&player) {
                    return Err(invalid("finish the current job or race first"));
                }
                let course = self
                    .transport
                    .courses
                    .get(&course_id)
                    .ok_or_else(|| invalid("race course"))?;
                if !near(
                    self.living.units[&vehicle_id].position,
                    course.checkpoints[0],
                ) {
                    return Err(SandboxError::OutOfReach);
                }
                self.transport.races.insert(
                    player,
                    RaceRun {
                        player_id: player,
                        vehicle_id,
                        course_id,
                        next_checkpoint: 1,
                        started_at: self.transport.elapsed_seconds,
                        last_gate_odometer_m: self.transport.vehicles[&vehicle_id].odometer_m,
                    },
                );
                Ok(outcome(
                    "Time trial started. Visit each checkpoint in order",
                ))
            }
            TransportCommand::RaceCheckpoint => self.transport_race_gate(player),
            TransportCommand::AbandonRace => {
                if self.transport.races.remove(&player).is_none() {
                    return Err(invalid("no active race"));
                }
                Ok(outcome("Time trial abandoned"))
            }
        }
    }

    fn accept_transport_job(
        &mut self,
        player: u64,
        job_id: u64,
        vehicle_id: u64,
    ) -> Result<Outcome, SandboxError> {
        let vehicle = self.owned_vehicle(player, vehicle_id)?.clone();
        self.require_license(player, vehicle.model.spec().required_license)?;
        let job = self
            .transport
            .jobs
            .get(&job_id)
            .ok_or_else(|| invalid("transport job"))?
            .clone();
        if job.assignment.is_some()
            || self.vehicle_has_job(vehicle_id)
            || !vehicle.cargo.is_empty()
            || self
                .transport
                .races
                .values()
                .any(|r| r.vehicle_id == vehicle_id)
            || self
                .transport
                .jobs
                .values()
                .filter(|j| j.assignment.as_ref().is_some_and(|a| a.owner == player))
                .count()
                >= MAX_PLAYER_JOBS
        {
            return Err(invalid(
                "job or vehicle already assigned, or driver job limit reached",
            ));
        }
        if !job.kind.accepts(vehicle.model) {
            return Err(invalid("vehicle is not suitable for this job"));
        }
        if job.passengers > vehicle.model.spec().passenger_seats
            || job.amount > self.vehicle_cargo_capacity(vehicle_id)
        {
            return Err(invalid(
                "vehicle has insufficient passenger or cargo capacity",
            ));
        }
        if self.transport.treasury < job.reward {
            return Err(SandboxError::Insufficient("issuer funds".into()));
        }
        let reserved_cargo = if let Some(item) = &job.item {
            let source = self.transport.facilities[&job.pickup_facility]
                .linked_site
                .unwrap();
            let committed = self
                .living
                .deliveries
                .iter()
                .filter(|d| {
                    d.from_site == source && d.item == *item && d.phase == DeliveryPhase::ToPickup
                })
                .map(|d| u64::from(d.amount))
                .sum::<u64>();
            let stock = u64::from(
                self.transport_stock(job.pickup_facility)?
                    .get(item)
                    .copied()
                    .unwrap_or(0),
            );
            if stock.saturating_sub(committed) < u64::from(job.amount) {
                return Err(SandboxError::Insufficient("unreserved source goods".into()));
            }
            let amount = inventory(&[(item, job.amount)]);
            let remaining = exchange(
                self.transport_stock(job.pickup_facility)?,
                &amount,
                &Inventory::new(),
                1,
            )?;
            *self.transport_stock_mut(job.pickup_facility)? = remaining;
            amount
        } else {
            Inventory::new()
        };
        if self.transport.facilities[&job.pickup_facility].passengers_waiting
            < u32::from(job.passengers)
        {
            return Err(SandboxError::Insufficient("waiting passengers".into()));
        }
        self.transport
            .facilities
            .get_mut(&job.pickup_facility)
            .unwrap()
            .passengers_waiting -= u32::from(job.passengers);
        self.transport.treasury -= job.reward;
        self.transport.jobs.get_mut(&job_id).unwrap().assignment = Some(TransportAssignment {
            owner: player,
            vehicle_id,
            phase: TransportJobPhase::ToPickup,
            reward_escrow: job.reward,
            reserved_cargo,
            passengers_onboard: 0,
            accepted_at: self.transport.elapsed_seconds,
            deadline: self.transport.elapsed_seconds + job.time_limit_seconds,
        });
        Ok(outcome(format!(
            "{} accepted. Drive to the pickup point",
            job.name
        )))
    }

    fn pickup_transport_job(&mut self, player: u64, job_id: u64) -> Result<Outcome, SandboxError> {
        let job = self
            .transport
            .jobs
            .get(&job_id)
            .ok_or_else(|| invalid("transport job"))?
            .clone();
        let assignment = job
            .assignment
            .as_ref()
            .ok_or_else(|| invalid("job has not been accepted"))?;
        if assignment.owner != player {
            return Err(SandboxError::NotOwner);
        }
        self.require_active_vehicle(player, assignment.vehicle_id)?;
        if assignment.phase != TransportJobPhase::ToPickup {
            return Err(invalid("job pickup is already complete"));
        }
        if self.transport.elapsed_seconds > assignment.deadline {
            return Err(invalid("job expired; cancel to return the reservation"));
        }
        if !near(
            self.living.units[&assignment.vehicle_id].position,
            self.transport.facilities[&job.pickup_facility].position,
        ) {
            return Err(SandboxError::OutOfReach);
        }
        if speed(&self.living.units[&assignment.vehicle_id]) > 1. {
            return Err(invalid("stop at the pickup point"));
        }
        let vehicle = self
            .transport
            .vehicles
            .get_mut(&assignment.vehicle_id)
            .unwrap();
        vehicle.cargo = exchange(
            &vehicle.cargo,
            &Inventory::new(),
            &assignment.reserved_cargo,
            1,
        )?;
        let assignment = self
            .transport
            .jobs
            .get_mut(&job_id)
            .unwrap()
            .assignment
            .as_mut()
            .unwrap();
        assignment.reserved_cargo.clear();
        assignment.passengers_onboard = job.passengers;
        assignment.phase = TransportJobPhase::ToDestination;
        Ok(outcome(
            "Loaded. Drive to the destination and stop to unload",
        ))
    }

    fn finish_transport_job(
        &mut self,
        player: u64,
        job_id: u64,
        returning: bool,
    ) -> Result<Outcome, SandboxError> {
        let job = self
            .transport
            .jobs
            .get(&job_id)
            .ok_or_else(|| invalid("transport job"))?
            .clone();
        let assignment = job
            .assignment
            .as_ref()
            .ok_or_else(|| invalid("job has not been accepted"))?;
        if assignment.owner != player {
            return Err(SandboxError::NotOwner);
        }
        self.require_active_vehicle(player, assignment.vehicle_id)?;
        if assignment.phase == TransportJobPhase::ToPickup {
            return Err(invalid("pick up the job first"));
        }
        if !returning
            && (assignment.phase == TransportJobPhase::Returning
                || self.transport.elapsed_seconds > assignment.deadline)
        {
            return Err(invalid("job must return to its pickup point"));
        }
        let destination = if returning {
            job.pickup_facility
        } else {
            job.destination_facility
        };
        let unit = &self.living.units[&assignment.vehicle_id];
        if !near(
            unit.position,
            self.transport.facilities[&destination].position,
        ) {
            return Err(SandboxError::OutOfReach);
        }
        if speed(unit) > 1. {
            return Err(invalid("stop at the service point"));
        }
        if let Some(item) = &job.item {
            let goods = inventory(&[(item, job.amount)]);
            let vehicle_stock = exchange(
                &self.transport.vehicles[&assignment.vehicle_id].cargo,
                &goods,
                &Inventory::new(),
                1,
            )?;
            let destination_stock = self.transport_delivery_stock(destination, &goods)?;
            self.transport
                .vehicles
                .get_mut(&assignment.vehicle_id)
                .unwrap()
                .cargo = vehicle_stock;
            *self.transport_stock_mut(destination)? = destination_stock;
        }
        let waiting = self.transport.facilities[&destination]
            .passengers_waiting
            .checked_add(u32::from(job.passengers))
            .ok_or(SandboxError::Capacity)?;
        if waiting > 1_000_000 {
            return Err(SandboxError::Capacity);
        }
        self.transport
            .facilities
            .get_mut(&destination)
            .unwrap()
            .passengers_waiting = waiting;
        if returning {
            self.transport.treasury = self
                .transport
                .treasury
                .checked_add(assignment.reward_escrow)
                .filter(|n| *n <= MAX_MONEY)
                .ok_or(SandboxError::Capacity)?;
        } else {
            let account = self.living.accounts.get_mut(&player).unwrap();
            account.money = account
                .money
                .checked_add(assignment.reward_escrow)
                .filter(|n| *n <= MAX_MONEY)
                .ok_or(SandboxError::Capacity)?;
            let driver = self.transport.drivers.get_mut(&player).unwrap();
            driver.jobs_completed = driver.jobs_completed.saturating_add(1);
            driver.xp = driver.xp.saturating_add(150);
            driver.earnings = driver.earnings.saturating_add(assignment.reward_escrow);
            self.transport.jobs.get_mut(&job_id).unwrap().completions =
                job.completions.saturating_add(1);
        }
        self.transport.jobs.get_mut(&job_id).unwrap().assignment = None;
        Ok(outcome(if returning {
            "Cargo and passengers returned; job cancelled".into()
        } else {
            format!(
                "Delivery complete: {:.2} credits and 150 driver XP",
                assignment.reward_escrow as f64 / 100.
            )
        }))
    }

    pub(super) fn refund_transport_job(&mut self, job_id: u64) -> Result<(), SandboxError> {
        let job = self
            .transport
            .jobs
            .get(&job_id)
            .ok_or_else(|| invalid("transport job"))?
            .clone();
        let assignment = job
            .assignment
            .as_ref()
            .ok_or_else(|| invalid("transport assignment"))?;
        if assignment.phase != TransportJobPhase::ToPickup {
            return Err(invalid("loaded jobs require physical return"));
        }
        let next_stock = if !assignment.reserved_cargo.is_empty() {
            Some(self.transport_delivery_stock(job.pickup_facility, &assignment.reserved_cargo)?)
        } else {
            None
        };
        let waiting = self.transport.facilities[&job.pickup_facility]
            .passengers_waiting
            .checked_add(u32::from(job.passengers))
            .filter(|n| *n <= 1_000_000)
            .ok_or(SandboxError::Capacity)?;
        let treasury = self
            .transport
            .treasury
            .checked_add(assignment.reward_escrow)
            .filter(|n| *n <= MAX_MONEY)
            .ok_or(SandboxError::Capacity)?;
        if let Some(stock) = next_stock {
            *self.transport_stock_mut(job.pickup_facility)? = stock;
        }
        self.transport
            .facilities
            .get_mut(&job.pickup_facility)
            .unwrap()
            .passengers_waiting = waiting;
        self.transport.treasury = treasury;
        self.transport.jobs.get_mut(&job_id).unwrap().assignment = None;
        Ok(())
    }

    pub fn vehicle_cargo_capacity(&self, vehicle_id: u64) -> u32 {
        self.transport
            .vehicles
            .get(&vehicle_id)
            .map(|v| {
                v.model.spec().cargo_capacity
                    + v.trailer
                        .and_then(|id| self.transport.trailers.get(&id))
                        .map_or(0, |t| t.capacity)
            })
            .unwrap_or(0)
    }

    pub(super) fn owned_vehicle(
        &self,
        player: u64,
        id: u64,
    ) -> Result<&VehicleState, SandboxError> {
        let vehicle = self
            .transport
            .vehicles
            .get(&id)
            .ok_or_else(|| invalid("owned road vehicle"))?;
        if vehicle.owner != player {
            return Err(SandboxError::NotOwner);
        }
        Ok(vehicle)
    }
    fn require_active_vehicle(&self, player: u64, id: u64) -> Result<(), SandboxError> {
        self.owned_vehicle(player, id)?;
        if self.living.accounts[&player].active_unit != id {
            return Err(invalid("take control of the assigned vehicle first"));
        }
        Ok(())
    }
    pub(super) fn require_garage(&self, player: u64, id: u64) -> Result<(), SandboxError> {
        let facility = self
            .transport
            .facilities
            .get(&id)
            .ok_or_else(|| invalid("garage"))?;
        if facility.kind != TransportFacilityKind::Garage {
            return Err(invalid("this service requires a garage"));
        }
        if !near(self.players[&player].position, facility.position) {
            return Err(SandboxError::OutOfReach);
        }
        Ok(())
    }
    pub(super) fn require_vehicle_at_garage(
        &self,
        player: u64,
        vehicle: u64,
        facility: u64,
    ) -> Result<(), SandboxError> {
        self.owned_vehicle(player, vehicle)?;
        self.require_garage(player, facility)?;
        let unit = &self.living.units[&vehicle];
        if !near(unit.position, self.transport.facilities[&facility].position) {
            return Err(SandboxError::OutOfReach);
        }
        if speed(unit) > 0.5 {
            return Err(invalid("stop the vehicle before garage service"));
        }
        Ok(())
    }
    pub(super) fn require_license(
        &self,
        player: u64,
        license: DriverLicense,
    ) -> Result<(), SandboxError> {
        if self
            .transport
            .drivers
            .get(&player)
            .is_none_or(|d| !d.licenses.contains(&license))
        {
            return Err(invalid("driver does not hold the required vehicle license"));
        }
        Ok(())
    }
    pub(super) fn vehicle_has_job(&self, vehicle: u64) -> bool {
        self.transport.jobs.values().any(|j| {
            j.assignment
                .as_ref()
                .is_some_and(|a| a.vehicle_id == vehicle)
        })
    }
    pub(super) fn transport_charge(
        &mut self,
        player: u64,
        amount: u64,
    ) -> Result<(), SandboxError> {
        let money = self.living.accounts[&player]
            .money
            .checked_sub(amount)
            .ok_or_else(|| SandboxError::Insufficient("credits".into()))?;
        let treasury = self
            .transport
            .treasury
            .checked_add(amount)
            .filter(|n| *n <= MAX_MONEY)
            .ok_or(SandboxError::Capacity)?;
        self.living.accounts.get_mut(&player).unwrap().money = money;
        self.transport.treasury = treasury;
        Ok(())
    }
    fn transport_pay_from_treasury(
        &mut self,
        player: u64,
        amount: u64,
    ) -> Result<(), SandboxError> {
        let treasury = self
            .transport
            .treasury
            .checked_sub(amount)
            .ok_or_else(|| SandboxError::Insufficient("dealer funds".into()))?;
        let money = self.living.accounts[&player]
            .money
            .checked_add(amount)
            .filter(|n| *n <= MAX_MONEY)
            .ok_or(SandboxError::Capacity)?;
        self.living.accounts.get_mut(&player).unwrap().money = money;
        self.transport.treasury = treasury;
        Ok(())
    }
    pub(super) fn transport_stock(&self, facility: u64) -> Result<&Inventory, SandboxError> {
        let site = self
            .transport
            .facilities
            .get(&facility)
            .and_then(|f| f.linked_site)
            .ok_or_else(|| invalid("facility has no goods stock"))?;
        if let Some(producer) = self.living.producers.get(&site) {
            return Ok(&producer.stock);
        }
        self.living
            .shops
            .get(&site)
            .map(|s| &s.stock)
            .ok_or_else(|| invalid("linked goods stock"))
    }
    /// Shared admission for manual delivery, reservation refunds and fleet
    /// unloading. Producer aggregate storage is the same 2,000-unit budget used
    /// by production and NPC couriers, including mixtures of different items.
    pub(super) fn transport_delivery_stock(
        &self,
        facility: u64,
        goods: &Inventory,
    ) -> Result<Inventory, SandboxError> {
        let next = exchange(self.transport_stock(facility)?, &Inventory::new(), goods, 1)?;
        if self
            .transport
            .facilities
            .get(&facility)
            .and_then(|f| f.linked_site)
            .is_some_and(|site| self.living.producers.contains_key(&site))
            && next.values().map(|amount| u64::from(*amount)).sum::<u64>() > 2_000
        {
            return Err(invalid(
                "producer storage is full; free space before unloading or returning the reservation",
            ));
        }
        Ok(next)
    }
    pub(super) fn transport_stock_mut(
        &mut self,
        facility: u64,
    ) -> Result<&mut Inventory, SandboxError> {
        let site = self
            .transport
            .facilities
            .get(&facility)
            .and_then(|f| f.linked_site)
            .ok_or_else(|| invalid("facility has no goods stock"))?;
        if let Some(producer) = self.living.producers.get_mut(&site) {
            return Ok(&mut producer.stock);
        }
        self.living
            .shops
            .get_mut(&site)
            .map(|s| &mut s.stock)
            .ok_or_else(|| invalid("linked goods stock"))
    }
}

pub fn repair_price(vehicle: &VehicleState) -> u64 {
    ((vehicle.model.spec().price as f64 * (1. - f64::from(vehicle.condition)) * 0.45).ceil() as u64)
        .max(200)
}
pub fn upgrade_price(current_level: u8) -> u64 {
    2_000 * (u64::from(current_level) + 1)
}
fn speed(unit: &LivingUnit) -> f64 {
    unit.velocity_enu_m_s[0].hypot(unit.velocity_enu_m_s[1])
}
