use super::*;
use crate::catalog::validate_inventory;

impl TransportWorld {
    pub(crate) fn validate(
        &self,
        world: &SandboxState,
        catalog: Option<&Catalog>,
    ) -> Result<(), SandboxError> {
        if self.profiles.len() > MAX_PLAYERS
            || self.drivers.len() > MAX_PLAYERS
            || self.vehicles.len() > MAX_TRANSPORT_VEHICLES
            || self.facilities.len() > MAX_TRANSPORT_FACILITIES
            || self.jobs.len() > MAX_TRANSPORT_JOBS
            || self.trailers.len() > MAX_TRANSPORT_TRAILERS
            || self.courses.len() > MAX_RACE_COURSES
            || self.races.len() > MAX_PLAYERS
            || self.personal_bests.len() > MAX_PLAYERS * MAX_RACE_COURSES
        {
            return Err(SandboxError::Capacity);
        }
        if !self.visible_vehicles.is_empty() {
            return Err(invalid(
                "client vehicle appearance cache cannot be saved as authoritative state",
            ));
        }
        if self.next_id == 0
            || self.next_id == u64::MAX
            || self.treasury > MAX_MONEY
            || !finite_range(self.elapsed_seconds, 0., 1e12)
        {
            return Err(invalid("transport clock, treasury or sequence"));
        }
        for (&player, profile) in &self.profiles {
            profile.validate()?;
            if !world.players.contains_key(&player) {
                return Err(invalid("character principal"));
            }
        }
        if !self.enabled {
            if !self.drivers.is_empty()
                || !self.vehicles.is_empty()
                || !self.facilities.is_empty()
                || !self.jobs.is_empty()
                || !self.trailers.is_empty()
                || !self.courses.is_empty()
                || !self.races.is_empty()
                || !self.personal_bests.is_empty()
                || !self.companies.is_empty()
                || self.treasury != 0
                || self.elapsed_seconds != 0.
            {
                return Err(invalid("disabled transport world has live ledgers"));
            }
            return Ok(());
        }
        if !world.living.enabled {
            return Err(invalid("transport requires a living world"));
        }
        for (&player, driver) in &self.drivers {
            driver.validate()?;
            if player != driver.player_id
                || !world.players.contains_key(&player)
                || !world.living.accounts.contains_key(&player)
            {
                return Err(invalid("driver principal"));
            }
        }
        let mut owned_counts = BTreeMap::<u64, usize>::new();
        for (&id, vehicle) in &self.vehicles {
            vehicle.validate()?;
            if id != vehicle.unit_id
                || !self.drivers.contains_key(&vehicle.owner)
                || world
                    .living_unit_reference(id)
                    .is_none_or(|(owner, kind, _)| {
                        owner != Some(vehicle.owner) || kind != ActorKind::Vehicle
                    })
            {
                return Err(invalid("vehicle ownership/body reference"));
            }
            if vehicle.rental.as_ref().is_some_and(|r| {
                self.facilities
                    .get(&r.facility_id)
                    .is_none_or(|f| f.kind != TransportFacilityKind::Garage)
            }) {
                return Err(invalid("rental garage reference"));
            }
            *owned_counts.entry(vehicle.owner).or_default() += 1;
            let capacity = vehicle.model.spec().cargo_capacity
                + vehicle
                    .trailer
                    .and_then(|id| self.trailers.get(&id))
                    .map_or(0, |t| t.capacity);
            if vehicle.cargo.values().map(|n| u64::from(*n)).sum::<u64>() > u64::from(capacity) {
                return Err(invalid("vehicle cargo capacity"));
            }
            if catalog.is_some_and(|c| vehicle.cargo.keys().any(|i| !c.items.contains(i))) {
                return Err(invalid("vehicle cargo catalog reference"));
            }
            if let Some(trailer) = vehicle.trailer {
                if !vehicle.model.can_tow()
                    || self
                        .trailers
                        .get(&trailer)
                        .is_none_or(|t| t.attached_to != Some(id) || t.owner != vehicle.owner)
                {
                    return Err(invalid("vehicle trailer reference"));
                }
            }
        }
        if owned_counts.values().any(|n| *n > MAX_PLAYER_VEHICLES) {
            return Err(SandboxError::Capacity);
        }
        let mut ids = BTreeSet::new();
        for (&id, facility) in &self.facilities {
            facility.validate()?;
            if id != facility.id || id >= self.next_id || !ids.insert(id) {
                return Err(invalid("transport facility identity"));
            }
            if let Some(site) = facility.linked_site {
                if !world.living.producers.contains_key(&site)
                    && !world.living.shops.contains_key(&site)
                {
                    return Err(invalid("transport facility stock reference"));
                }
            }
        }
        let mut assigned_vehicles = BTreeSet::new();
        let mut assigned_players = BTreeMap::<u64, usize>::new();
        for (&id, job) in &self.jobs {
            job.validate_definition(&self.facilities, catalog)?;
            if id != job.id || id >= self.next_id || !ids.insert(id) {
                return Err(invalid("transport job identity"));
            }
            if let Some(assignment) = &job.assignment {
                assignment.validate(job, self.elapsed_seconds)?;
                let vehicle = self
                    .vehicles
                    .get(&assignment.vehicle_id)
                    .ok_or_else(|| invalid("assigned vehicle"))?;
                if vehicle.owner != assignment.owner
                    || !self.drivers.contains_key(&assignment.owner)
                    || !job.kind.accepts(vehicle.model)
                    || !assigned_vehicles.insert(assignment.vehicle_id)
                {
                    return Err(invalid(
                        "transport assignment ownership or duplicate vehicle",
                    ));
                }
                if job.passengers > vehicle.model.spec().passenger_seats {
                    return Err(invalid("passenger capacity"));
                }
                let capacity = vehicle.model.spec().cargo_capacity
                    + vehicle
                        .trailer
                        .and_then(|id| self.trailers.get(&id))
                        .map_or(0, |t| t.capacity);
                if job.amount > capacity
                    || !self.drivers[&assignment.owner]
                        .licenses
                        .contains(&vehicle.model.spec().required_license)
                {
                    return Err(invalid(
                        "assignment cargo capacity or required driver license",
                    ));
                }
                if assignment.phase != TransportJobPhase::ToPickup {
                    if let Some(item) = &job.item {
                        if vehicle.cargo.get(item).copied().unwrap_or(0) < job.amount {
                            return Err(invalid("assigned goods missing from vehicle"));
                        }
                    }
                }
                *assigned_players.entry(assignment.owner).or_default() += 1;
            }
        }
        if assigned_players.values().any(|n| *n > MAX_PLAYER_JOBS) {
            return Err(SandboxError::Capacity);
        }
        for (&id, trailer) in &self.trailers {
            trailer.validate()?;
            if id != trailer.id
                || id >= self.next_id
                || !ids.insert(id)
                || !self.drivers.contains_key(&trailer.owner)
            {
                return Err(invalid("trailer identity or owner"));
            }
            if let Some(vehicle) = trailer.attached_to {
                if self
                    .vehicles
                    .get(&vehicle)
                    .is_none_or(|v| v.owner != trailer.owner || v.trailer != Some(id))
                {
                    return Err(invalid("trailer attachment reference"));
                }
            }
        }
        for (&id, course) in &self.courses {
            course.validate()?;
            if id != course.id || id >= self.next_id || !ids.insert(id) {
                return Err(invalid("race course identity"));
            }
        }
        for (&player, run) in &self.races {
            let course = self
                .courses
                .get(&run.course_id)
                .ok_or_else(|| invalid("race course reference"))?;
            let vehicle = self
                .vehicles
                .get(&run.vehicle_id)
                .ok_or_else(|| invalid("race vehicle reference"))?;
            run.validate(course, self.elapsed_seconds)?;
            if player != run.player_id
                || vehicle.owner != player
                || assigned_vehicles.contains(&run.vehicle_id)
                || run.last_gate_odometer_m > vehicle.odometer_m + 1e-6
            {
                return Err(invalid("race driver or mileage reference"));
            }
        }
        let mut bests = BTreeSet::new();
        for best in &self.personal_bests {
            if !self.drivers.contains_key(&best.player_id)
                || !self.courses.contains_key(&best.course_id)
                || !bests.insert((best.player_id, best.course_id))
                || !finite_range(best.seconds, 0.000_001, 1e12)
            {
                return Err(invalid("personal race result"));
            }
        }
        self.validate_companies(world, catalog)?;
        Ok(())
    }
}

impl DriverProgress {
    pub(super) fn validate(&self) -> Result<(), SandboxError> {
        if self.player_id == 0
            || !self.licenses.contains(&DriverLicense::Basic)
            || self.licenses.len() > 4
            || !finite_range(self.distance_m, 0., 1e12)
        {
            return Err(invalid("driver progress"));
        }
        Ok(())
    }
}
impl VehicleState {
    pub fn validate(&self) -> Result<(), SandboxError> {
        if self.unit_id == 0
            || self.owner == 0
            || !finite_range(self.fuel_l, 0., self.model.spec().fuel_capacity_l + 1e-6)
            || !finite_range(f64::from(self.condition), 0., 1.)
            || !finite_range(self.odometer_m, 0., 1e12)
            || self.paint > 7
            || self.engine_level > 3
            || self.brakes_level > 3
        {
            return Err(invalid("road vehicle"));
        }
        if self.rental.as_ref().is_some_and(|r| {
            r.facility_id == 0
                || r.bond_escrow != self.model.spec().price / 4
                || !finite_range(r.remaining_seconds, 0., 3600.)
        }) {
            return Err(invalid("vehicle rental bond or prepaid time"));
        }
        self.dynamics
            .validate()
            .map_err(|e| SandboxError::Invalid(e.to_string()))?;
        validate_inventory(&self.cargo)
    }
}
impl VehicleAppearance {
    pub fn validate(&self) -> Result<(), SandboxError> {
        if self.unit_id == 0
            || self.owner == 0
            || self.paint > 7
            || !finite_range(self.steering_rad, -0.75, 0.75)
            || self.trailer == Some(0)
        {
            return Err(invalid("public vehicle appearance"));
        }
        Ok(())
    }
}
impl TransportFacility {
    pub(super) fn validate(&self) -> Result<(), SandboxError> {
        self.position.validate()?;
        if self.id == 0
            || !valid_name(&self.name)
            || self.passengers_waiting > 1_000_000
            || !finite_range(self.fuel_l, 0., 1_000_000.)
            || self.linked_site == Some(0)
        {
            return Err(invalid("transport facility"));
        }
        Ok(())
    }
}
impl TransportJob {
    pub(super) fn validate_definition(
        &self,
        facilities: &BTreeMap<u64, TransportFacility>,
        catalog: Option<&Catalog>,
    ) -> Result<(), SandboxError> {
        if self.id == 0
            || !valid_name(&self.name)
            || self.pickup_facility == self.destination_facility
            || !facilities.contains_key(&self.pickup_facility)
            || !facilities.contains_key(&self.destination_facility)
            || self.reward == 0
            || self.reward > MAX_MONEY
            || !finite_range(self.time_limit_seconds, 30., 86_400.)
            || self.passengers > 30
            || self.amount > 1000
        {
            return Err(invalid("transport job definition"));
        }
        if let Some(item) = &self.item {
            if !valid_id(item)
                || self.amount == 0
                || self.passengers != 0
                || catalog.is_some_and(|c| !c.items.contains(item))
                || facilities[&self.pickup_facility].linked_site.is_none()
                || facilities[&self.destination_facility].linked_site.is_none()
            {
                return Err(invalid("freight job goods reference"));
            }
        } else if self.amount != 0 {
            return Err(invalid("job amount without goods"));
        }
        if matches!(
            self.kind,
            TransportJobKind::Hitchhiker
                | TransportJobKind::Taxi
                | TransportJobKind::Bus
                | TransportJobKind::Ambulance
        ) && (self.passengers == 0 || self.item.is_some())
        {
            return Err(invalid("passenger job definition"));
        }
        if matches!(
            self.kind,
            TransportJobKind::Freight | TransportJobKind::FireSupply
        ) && self.item.is_none()
        {
            return Err(invalid("cargo job without goods"));
        }
        Ok(())
    }
}
impl TransportAssignment {
    pub(super) fn validate(&self, job: &TransportJob, elapsed: f64) -> Result<(), SandboxError> {
        validate_inventory(&self.reserved_cargo)?;
        let expected: Inventory = job
            .item
            .as_ref()
            .map(|i| BTreeMap::from([(i.clone(), job.amount)]))
            .unwrap_or_default();
        if self.owner == 0
            || self.vehicle_id == 0
            || self.reward_escrow != job.reward
            || !finite_range(self.accepted_at, 0., elapsed + 1e-6)
            || !finite_range(self.deadline, self.accepted_at, 1e12 + 86_400.)
            || (self.deadline - self.accepted_at - job.time_limit_seconds).abs() > 1e-3
        {
            return Err(invalid("transport assignment"));
        }
        if self.phase == TransportJobPhase::ToPickup {
            if self.reserved_cargo != expected || self.passengers_onboard != 0 {
                return Err(invalid("job pickup reservation"));
            }
        } else if !self.reserved_cargo.is_empty() || self.passengers_onboard != job.passengers {
            return Err(invalid("loaded transport job"));
        }
        Ok(())
    }
}
impl TransportTrailer {
    pub(super) fn validate(&self) -> Result<(), SandboxError> {
        self.position.validate()?;
        if self.id == 0
            || self.owner == 0
            || self.capacity == 0
            || self.capacity > 1000
            || self.attached_to == Some(0)
        {
            return Err(invalid("transport trailer"));
        }
        Ok(())
    }
}
impl RaceCourse {
    pub(super) fn validate(&self) -> Result<(), SandboxError> {
        if self.id == 0
            || !valid_name(&self.name)
            || !(3..=32).contains(&self.checkpoints.len())
            || !finite_range(self.radius_m, 4., 20.)
        {
            return Err(invalid("race course"));
        }
        for point in &self.checkpoints {
            point.validate()?;
        }
        if self
            .checkpoints
            .windows(2)
            .any(|p| p[0].surface_distance_m(p[1]) <= self.radius_m * 2. + 1.)
            || self.checkpoints[0].surface_distance_m(*self.checkpoints.last().unwrap()) > 1.
        {
            return Err(invalid("overlapping gates or open race course"));
        }
        Ok(())
    }
}
impl RaceRun {
    pub(super) fn validate(&self, course: &RaceCourse, elapsed: f64) -> Result<(), SandboxError> {
        if self.player_id == 0
            || self.vehicle_id == 0
            || self.next_checkpoint == 0
            || self.next_checkpoint >= course.checkpoints.len()
            || !finite_range(self.started_at, 0., elapsed + 1e-6)
            || !finite_range(self.last_gate_odometer_m, 0., 1e12)
        {
            return Err(invalid("race run"));
        }
        Ok(())
    }
}
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && !name.chars().any(char::is_control)
}
