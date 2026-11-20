use super::*;

impl SandboxState {
    /// Authority driving cap: license, vehicle mechanics and the present
    /// driver's needs. Unpossessed RTS/autopilot vehicles retain their mechanical
    /// cap; a parked avatar is not an implicit driver for every owned vehicle.
    pub fn vehicle_speed_limit_m_s(&self, vehicle: u64) -> Option<f64> {
        self.transport.vehicles.get(&vehicle).map(|vehicle| {
            if self.is_player_dormant(vehicle.owner) {
                return 0.;
            }
            if let Some(driver) = self.company_driver_for_vehicle(vehicle.unit_id) {
                return if (driver.running || driver.trip.is_some())
                    && matches!(
                        driver.status,
                        FleetStatus::Driving | FleetStatus::Delivering
                    ) {
                    vehicle.speed_limit_m_s()
                } else {
                    0.
                };
            }
            if self
                .transport
                .drivers
                .get(&vehicle.owner)
                .is_some_and(|d| d.licenses.contains(&vehicle.model.spec().required_license))
            {
                let driver_factor = self
                    .living
                    .accounts
                    .get(&vehicle.owner)
                    .filter(|a| a.possessed && a.active_unit == vehicle.unit_id)
                    .and_then(|a| self.living.units.get(&a.avatar_unit))
                    .map_or(1., |avatar| avatar.needs.speed_factor());
                vehicle.speed_limit_m_s() * driver_factor
            } else {
                0.
            }
        })
    }

    /// Trusted authority accounting after terrain/collision resolution. Clients
    /// never submit position, mileage, damage or raw kinematic state as commands.
    pub fn record_vehicle_motion(
        &mut self,
        player: u64,
        vehicle_id: u64,
        distance_m: f64,
        physical_seconds: f64,
        collision_speed_m_s: f64,
    ) -> Result<(), SandboxError> {
        if !finite_range(physical_seconds, 0., 5.)
            || !finite_range(distance_m, 0., 201.)
            || distance_m > physical_seconds * 40. + 0.5
            || !finite_range(collision_speed_m_s, 0., 100.)
        {
            return Err(invalid("vehicle motion accounting"));
        }
        self.owned_vehicle(player, vehicle_id)?;
        let unit = self
            .living
            .units
            .get(&vehicle_id)
            .ok_or_else(|| invalid("vehicle body"))?;
        if unit.owner != Some(player) || unit.kind != ActorKind::Vehicle {
            return Err(SandboxError::NotOwner);
        }
        let position = unit.position;
        let heading = f64::from(unit.heading_deg).to_radians();
        let trailer_position = if self.transport.vehicles[&vehicle_id].trailer.is_some() {
            Some(LocalFrame::new(position).to_geo([-heading.sin() * 6., 0., heading.cos() * 6.])?)
        } else {
            None
        };
        let active = self
            .living
            .accounts
            .get(&player)
            .is_some_and(|a| a.active_unit == vehicle_id && a.possessed);
        let load = 1.
            + self
                .transport
                .vehicle_load(vehicle_id)
                .ok_or_else(|| invalid("vehicle load dependencies"))?
                .total_kg()
                / 10_000.;
        let vehicle = self.transport.vehicles.get_mut(&vehicle_id).unwrap();
        let running = !vehicle.parking_brake && (distance_m > 0.001 || vehicle.autopilot || active);
        let fuel = distance_m / 100_000.
            * vehicle.model.spec().fuel_l_per_100km
            * load
            * (1. + f64::from(vehicle.engine_level) * 0.05)
            + if running {
                physical_seconds * 0.00015
            } else {
                0.
            };
        vehicle.fuel_l = (vehicle.fuel_l - fuel).max(0.);
        vehicle.condition = (f64::from(vehicle.condition)
            - distance_m * 0.000_000_5
            - collision_speed_m_s * collision_speed_m_s * 0.0008)
            .clamp(0., 1.) as f32;
        vehicle.odometer_m = (vehicle.odometer_m + distance_m).min(1e12);
        if let Some(driver) = self.transport.drivers.get_mut(&player) {
            driver.distance_m = (driver.distance_m + distance_m).min(1e12);
        }
        if let Some(id) = vehicle.trailer {
            if let Some(trailer) = self.transport.trailers.get_mut(&id) {
                trailer.position = trailer_position.unwrap();
            }
        }
        Ok(())
    }

    /// Physical-time clock for driving jobs and time trials. This is called once
    /// by advance_living, independent of the accelerated calendar clock.
    pub fn advance_transport(&mut self, real_dt: f64) -> Result<(), SandboxError> {
        if !finite_range(real_dt, 0., 5.) {
            return Err(invalid("transport timestep"));
        }
        if !self.transport.enabled || real_dt == 0. {
            return Ok(());
        }
        let dormant: BTreeSet<_> = self
            .players
            .keys()
            .copied()
            .filter(|id| self.is_player_dormant(*id))
            .collect();
        let previous = self.transport.elapsed_seconds;
        self.transport.elapsed_seconds = (previous + real_dt).min(1e12);
        let elapsed = self.transport.elapsed_seconds - previous;
        for vehicle in self.transport.vehicles.values_mut() {
            if !dormant.contains(&vehicle.owner) {
                if let Some(rental) = &mut vehicle.rental {
                    rental.remaining_seconds = (rental.remaining_seconds - elapsed).max(0.);
                    if rental.remaining_seconds == 0. {
                        vehicle.dynamics.stop();
                        if let Some(body) = self.living.units.get_mut(&vehicle.unit_id) {
                            body.velocity_enu_m_s = [0.; 3];
                        }
                    }
                }
            }
        }
        for job in self.transport.jobs.values_mut() {
            if let Some(assignment) = &mut job.assignment {
                if dormant.contains(&assignment.owner) {
                    assignment.accepted_at =
                        (assignment.accepted_at + elapsed).min(self.transport.elapsed_seconds);
                    assignment.deadline = assignment.accepted_at + job.time_limit_seconds;
                    continue;
                }
                if self.transport.elapsed_seconds > assignment.deadline
                    && assignment.phase == TransportJobPhase::ToDestination
                {
                    assignment.phase = TransportJobPhase::Returning;
                }
            }
        }
        for (&owner, run) in &mut self.transport.races {
            if dormant.contains(&owner) {
                run.started_at = (run.started_at + elapsed).min(self.transport.elapsed_seconds);
            }
        }
        for (&owner, company) in &mut self.transport.companies {
            if dormant.contains(&owner) {
                for trip in company
                    .drivers
                    .iter_mut()
                    .filter_map(|driver| driver.trip.as_mut())
                {
                    trip.pickup_elapsed_seconds =
                        (trip.pickup_elapsed_seconds + elapsed).min(self.transport.elapsed_seconds);
                }
            }
        }
        // Uncollected reservations return to their original accounts when the
        // offer expires. A temporarily full destination/treasury keeps its
        // escrow intact and retries on a later step; no goods are discarded.
        let expired: Vec<_> = self
            .transport
            .jobs
            .values()
            .filter(|job| {
                job.assignment.as_ref().is_some_and(|a| {
                    !dormant.contains(&a.owner)
                        && a.phase == TransportJobPhase::ToPickup
                        && self.transport.elapsed_seconds > a.deadline
                })
            })
            .map(|job| job.id)
            .collect();
        for job in expired {
            let _ = self.refund_transport_job(job);
        }
        // One gate at most per authority step; the gate itself also checks
        // ordered proximity and physical mileage since the previous gate.
        let ready: Vec<_> = self
            .transport
            .races
            .iter()
            .filter_map(|(&player, run)| {
                if dormant.contains(&player) {
                    return None;
                }
                let course = self.transport.courses.get(&run.course_id)?;
                let gate = course.checkpoints.get(run.next_checkpoint)?;
                let unit = self.living.units.get(&run.vehicle_id)?;
                (unit
                    .position
                    .surface_distance_m(*gate)
                    .hypot(unit.position.altitude_m - gate.altitude_m)
                    <= course.radius_m)
                    .then_some(player)
            })
            .collect();
        for player in ready {
            let _ = self.transport_race_gate(player);
        }
        self.advance_company_arrivals();
        self.advance_company_households(elapsed);
        Ok(())
    }

    pub(super) fn transport_race_gate(&mut self, player: u64) -> Result<Outcome, SandboxError> {
        let run = self
            .transport
            .races
            .get(&player)
            .ok_or_else(|| invalid("no active time trial"))?
            .clone();
        let course = self
            .transport
            .courses
            .get(&run.course_id)
            .ok_or_else(|| invalid("race course"))?;
        let gate = *course
            .checkpoints
            .get(run.next_checkpoint)
            .ok_or_else(|| invalid("race gate"))?;
        let unit = self
            .living
            .units
            .get(&run.vehicle_id)
            .ok_or_else(|| invalid("race vehicle"))?;
        if self
            .living
            .accounts
            .get(&player)
            .is_none_or(|a| a.active_unit != run.vehicle_id)
        {
            return Err(invalid("return to the race vehicle"));
        }
        if unit
            .position
            .surface_distance_m(gate)
            .hypot(unit.position.altitude_m - gate.altitude_m)
            > course.radius_m
        {
            return Err(SandboxError::OutOfReach);
        }
        let vehicle = self
            .transport
            .vehicles
            .get(&run.vehicle_id)
            .ok_or_else(|| invalid("race vehicle"))?;
        let leg = course.checkpoints[run.next_checkpoint - 1].surface_distance_m(gate);
        if vehicle.odometer_m - run.last_gate_odometer_m < (leg - 2. * course.radius_m).max(1.) {
            return Err(invalid(
                "drive the route before claiming its next checkpoint",
            ));
        }
        let elapsed = self.transport.elapsed_seconds - run.started_at;
        if elapsed <= 0. {
            return Err(invalid("race clock has not advanced"));
        }
        if run.next_checkpoint + 1 == course.checkpoints.len() {
            if let Some(best) = self
                .transport
                .personal_bests
                .iter_mut()
                .find(|b| b.player_id == player && b.course_id == run.course_id)
            {
                best.seconds = best.seconds.min(elapsed);
            } else {
                self.transport.personal_bests.push(RaceBest {
                    player_id: player,
                    course_id: run.course_id,
                    seconds: elapsed,
                });
            }
            self.transport.races.remove(&player);
            Ok(outcome(format!(
                "Time trial finished in {elapsed:.2} seconds"
            )))
        } else {
            let race = self.transport.races.get_mut(&player).unwrap();
            race.next_checkpoint += 1;
            race.last_gate_odometer_m = vehicle.odometer_m;
            Ok(outcome(format!(
                "Checkpoint {} / {}",
                run.next_checkpoint,
                course.checkpoints.len() - 1
            )))
        }
    }
}

impl TransportWorld {
    pub fn job_target(&self, job_id: u64) -> Option<GeoPosition> {
        let job = self.jobs.get(&job_id)?;
        let destination = match job.assignment.as_ref().map(|a| a.phase) {
            Some(TransportJobPhase::ToDestination) => job.destination_facility,
            _ => job.pickup_facility,
        };
        self.facilities.get(&destination).map(|f| f.position)
    }
    pub fn race_target(&self, player: u64) -> Option<GeoPosition> {
        let run = self.races.get(&player)?;
        self.courses
            .get(&run.course_id)?
            .checkpoints
            .get(run.next_checkpoint)
            .copied()
    }
    pub fn active_job_for_vehicle(&self, vehicle: u64) -> Option<&TransportJob> {
        self.jobs.values().find(|job| {
            job.assignment
                .as_ref()
                .is_some_and(|a| a.vehicle_id == vehicle)
        })
    }
}
