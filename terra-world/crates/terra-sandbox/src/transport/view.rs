use super::*;

impl TransportWorld {
    pub fn snapshot(&self, player: u64, living: &LivingView) -> TransportView {
        let visible: BTreeSet<_> = living.units.iter().map(|u| u.id).collect();
        let owners: BTreeSet<_> = living.units.iter().filter_map(|u| u.owner).collect();
        let mut view = TransportView {
            enabled: self.enabled,
            profile: self.profiles.get(&player).cloned(),
            driver: self.drivers.get(&player).cloned(),
            vehicles: self
                .vehicles
                .values()
                .filter(|v| v.owner == player && visible.contains(&v.unit_id))
                .cloned()
                .collect(),
            visible_vehicles: self
                .vehicles
                .values()
                .filter(|vehicle| visible.contains(&vehicle.unit_id))
                .map(VehicleAppearance::from)
                .collect(),
            facilities: self.facilities.values().cloned().collect(),
            jobs: self
                .jobs
                .values()
                .filter(|j| {
                    j.assignment
                        .as_ref()
                        .is_none_or(|a| a.owner == player && visible.contains(&a.vehicle_id))
                })
                .cloned()
                .collect(),
            trailers: self
                .trailers
                .values()
                .filter(|t| {
                    t.owner == player || t.attached_to.is_some_and(|id| visible.contains(&id))
                })
                .cloned()
                .collect(),
            courses: self.courses.values().cloned().collect(),
            race: self.races.get(&player).cloned(),
            personal_bests: self
                .personal_bests
                .iter()
                .filter(|b| b.player_id == player)
                .cloned()
                .collect(),
            company: self.companies.get(&player).cloned(),
            visible_profiles: self
                .profiles
                .iter()
                .filter(|(id, _)| owners.contains(id))
                .map(|(&id, p)| (id, p.clone()))
                .collect(),
            elapsed_seconds: self.elapsed_seconds,
            truncated: false,
        };
        view.retain_visible(living);
        view
    }

    /// Client-only reconstruction. The private issuer treasury and other
    /// principals' progress, cargo and job assignments never cross the wire.
    pub fn from_view(view: TransportView, player: u64) -> Self {
        let mut profiles = view.visible_profiles;
        if let Some(profile) = view.profile {
            profiles.insert(player, profile);
        }
        let mut world = Self {
            enabled: view.enabled,
            profiles,
            elapsed_seconds: view.elapsed_seconds,
            vehicles: view.vehicles.into_iter().map(|v| (v.unit_id, v)).collect(),
            visible_vehicles: view
                .visible_vehicles
                .into_iter()
                .map(|v| (v.unit_id, v))
                .collect(),
            facilities: view.facilities.into_iter().map(|v| (v.id, v)).collect(),
            jobs: view.jobs.into_iter().map(|v| (v.id, v)).collect(),
            trailers: view.trailers.into_iter().map(|v| (v.id, v)).collect(),
            courses: view.courses.into_iter().map(|v| (v.id, v)).collect(),
            personal_bests: view.personal_bests,
            ..Self::default()
        };
        if let Some(driver) = view.driver {
            world.drivers.insert(player, driver);
        }
        if let Some(race) = view.race {
            world.races.insert(player, race);
        }
        if let Some(company) = view.company {
            world.companies.insert(player, company);
        }
        world
    }

    pub fn appearance(&self, unit: u64) -> Option<VehicleAppearance> {
        self.vehicles
            .get(&unit)
            .map(VehicleAppearance::from)
            .or_else(|| self.visible_vehicles.get(&unit).cloned())
    }
}

impl TransportView {
    pub fn retain_visible(&mut self, living: &LivingView) {
        let ids: BTreeSet<_> = living.units.iter().map(|u| u.id).collect();
        let owners: BTreeSet<_> = living.units.iter().filter_map(|u| u.owner).collect();
        let old = (self.vehicles.len(), self.jobs.len(), self.trailers.len());
        self.vehicles.retain(|v| ids.contains(&v.unit_id));
        let appearance_count = self.visible_vehicles.len();
        self.visible_vehicles.retain(|v| ids.contains(&v.unit_id));
        self.truncated |= appearance_count != self.visible_vehicles.len();
        let vehicles: BTreeSet<_> = self.vehicles.iter().map(|v| v.unit_id).collect();
        self.jobs.retain(|j| {
            j.assignment
                .as_ref()
                .is_none_or(|a| vehicles.contains(&a.vehicle_id))
        });
        self.trailers
            .retain(|t| t.attached_to.is_none_or(|id| ids.contains(&id)));
        self.visible_profiles.retain(|id, _| owners.contains(id));
        if self
            .race
            .as_ref()
            .is_some_and(|r| !vehicles.contains(&r.vehicle_id))
        {
            self.race = None;
            self.truncated = true;
        }
        self.truncated |= old != (self.vehicles.len(), self.jobs.len(), self.trailers.len());
    }

    pub fn validate_replica(&self, player: u64, living: &LivingView) -> Result<(), SandboxError> {
        for vehicle in &self.vehicles {
            if vehicle.rental.as_ref().is_some_and(|r| {
                self.facilities
                    .iter()
                    .all(|f| f.id != r.facility_id || f.kind != TransportFacilityKind::Garage)
            }) {
                return Err(invalid("rental garage view reference"));
            }
        }
        self.validate_company_view(player)?;
        if self.vehicles.len() > MAX_PLAYER_VEHICLES
            || self.visible_vehicles.len() > 64
            || self.facilities.len() > MAX_TRANSPORT_FACILITIES
            || self.jobs.len() > MAX_TRANSPORT_JOBS
            || self.trailers.len() > MAX_TRANSPORT_TRAILERS
            || self.courses.len() > MAX_RACE_COURSES
            || self.personal_bests.len() > MAX_RACE_COURSES
            || self.visible_profiles.len() > 64
            || !finite_range(self.elapsed_seconds, 0., 1e12)
        {
            return Err(SandboxError::Capacity);
        }
        if let Some(profile) = &self.profile {
            profile.validate()?;
            if self
                .visible_profiles
                .get(&player)
                .is_some_and(|p| p != profile)
            {
                return Err(invalid("conflicting public character profile"));
            }
        }
        let owners: BTreeSet<_> = living.units.iter().filter_map(|u| u.owner).collect();
        for (&id, profile) in &self.visible_profiles {
            profile.validate()?;
            if !owners.contains(&id) {
                return Err(invalid("unseen profile in transport view"));
            }
        }
        if !self.enabled {
            if self.driver.is_some()
                || !self.vehicles.is_empty()
                || !self.visible_vehicles.is_empty()
                || !self.facilities.is_empty()
                || !self.jobs.is_empty()
                || !self.trailers.is_empty()
                || !self.courses.is_empty()
                || self.race.is_some()
                || !self.personal_bests.is_empty()
            {
                return Err(invalid("disabled transport view contains live state"));
            }
            return Ok(());
        }
        if !living.enabled {
            return Err(invalid("transport view requires living state"));
        }
        if let Some(driver) = &self.driver {
            driver.validate()?;
            if driver.player_id != player {
                return Err(invalid("another driver's private progress"));
            }
        } else {
            return Err(invalid("transport driver missing"));
        }
        let mut vehicles = BTreeMap::new();
        let mut appearance_ids = BTreeSet::new();
        for appearance in &self.visible_vehicles {
            appearance.validate()?;
            if !appearance_ids.insert(appearance.unit_id)
                || !living.units.iter().any(|u| {
                    u.id == appearance.unit_id
                        && u.kind == ActorKind::Vehicle
                        && u.owner == Some(appearance.owner)
                })
            {
                return Err(invalid("vehicle appearance owner or visible body"));
            }
        }
        for vehicle in &self.vehicles {
            vehicle.validate()?;
            if vehicle.owner != player
                || vehicles.insert(vehicle.unit_id, vehicle).is_some()
                || !living.units.iter().any(|u| {
                    u.id == vehicle.unit_id
                        && u.kind == ActorKind::Vehicle
                        && u.owner == Some(player)
                })
            {
                return Err(invalid("another driver's vehicle or missing visible body"));
            }
            if self
                .visible_vehicles
                .iter()
                .find(|v| v.unit_id == vehicle.unit_id)
                .is_none_or(|v| *v != VehicleAppearance::from(vehicle))
            {
                return Err(invalid("private vehicle and public appearance disagree"));
            }
        }
        let mut facilities = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for facility in &self.facilities {
            facility.validate()?;
            if !ids.insert(facility.id) {
                return Err(invalid("duplicate transport facility"));
            }
            facilities.insert(facility.id, facility.clone());
        }
        let mut assigned = BTreeSet::new();
        let mut own_jobs = 0;
        for job in &self.jobs {
            job.validate_definition(&facilities, None)?;
            if !ids.insert(job.id) {
                return Err(invalid("duplicate transport job"));
            }
            if let Some(assignment) = &job.assignment {
                assignment.validate(job, self.elapsed_seconds)?;
                if assignment.owner != player
                    || !vehicles.contains_key(&assignment.vehicle_id)
                    || !assigned.insert(assignment.vehicle_id)
                {
                    return Err(invalid(
                        "another driver's job or duplicate vehicle assignment",
                    ));
                }
                if !job.kind.accepts(vehicles[&assignment.vehicle_id].model) {
                    return Err(invalid("incompatible assigned vehicle"));
                }
                own_jobs += 1;
            }
        }
        if own_jobs > MAX_PLAYER_JOBS {
            return Err(SandboxError::Capacity);
        }
        let mut trailers = BTreeMap::new();
        for trailer in &self.trailers {
            trailer.validate()?;
            if !ids.insert(trailer.id) {
                return Err(invalid("duplicate trailer"));
            }
            if let Some(vehicle) = trailer.attached_to {
                if self
                    .visible_vehicles
                    .iter()
                    .find(|v| v.unit_id == vehicle)
                    .is_none_or(|v| v.owner != trailer.owner || v.trailer != Some(trailer.id))
                {
                    return Err(invalid("view trailer attachment"));
                }
            } else if trailer.owner != player {
                return Err(invalid(
                    "unseen detached trailer belonging to another driver",
                ));
            }
            trailers.insert(trailer.id, trailer);
        }
        for appearance in &self.visible_vehicles {
            if let Some(id) = appearance.trailer {
                if !appearance.model.can_tow()
                    || trailers.get(&id).is_none_or(|t| {
                        t.owner != appearance.owner || t.attached_to != Some(appearance.unit_id)
                    })
                {
                    return Err(invalid("public vehicle trailer reference"));
                }
            }
        }
        for vehicle in &self.vehicles {
            if vehicle.trailer.is_some_and(|id| {
                trailers
                    .get(&id)
                    .is_none_or(|t| t.attached_to != Some(vehicle.unit_id))
            }) {
                return Err(invalid("missing vehicle trailer"));
            }
            let capacity = vehicle.model.spec().cargo_capacity
                + vehicle
                    .trailer
                    .and_then(|id| trailers.get(&id))
                    .map_or(0, |t| t.capacity);
            if vehicle.cargo.values().map(|n| u64::from(*n)).sum::<u64>() > u64::from(capacity) {
                return Err(invalid("view vehicle cargo capacity"));
            }
            if vehicle.trailer.is_some() && !vehicle.model.can_tow() {
                return Err(invalid("view vehicle cannot tow"));
            }
            for job in self.jobs.iter().filter(|j| {
                j.assignment
                    .as_ref()
                    .is_some_and(|a| a.vehicle_id == vehicle.unit_id)
            }) {
                if job.amount > capacity || job.passengers > vehicle.model.spec().passenger_seats {
                    return Err(invalid("view job vehicle capacity"));
                }
                if let (Some(item), Some(assignment)) = (&job.item, &job.assignment) {
                    if assignment.phase != TransportJobPhase::ToPickup
                        && vehicle.cargo.get(item).copied().unwrap_or(0) < job.amount
                    {
                        return Err(invalid("view job cargo missing"));
                    }
                }
            }
        }
        let mut courses = BTreeMap::new();
        for course in &self.courses {
            course.validate()?;
            if !ids.insert(course.id) {
                return Err(invalid("duplicate race course"));
            }
            courses.insert(course.id, course);
        }
        if let Some(race) = &self.race {
            let course = courses
                .get(&race.course_id)
                .ok_or_else(|| invalid("view race course"))?;
            race.validate(course, self.elapsed_seconds)?;
            if race.player_id != player || !vehicles.contains_key(&race.vehicle_id) {
                return Err(invalid("another driver's race"));
            }
            if assigned.contains(&race.vehicle_id)
                || race.last_gate_odometer_m > vehicles[&race.vehicle_id].odometer_m + 1e-6
            {
                return Err(invalid(
                    "view race vehicle is assigned or mileage regressed",
                ));
            }
        }
        let mut bests = BTreeSet::new();
        for best in &self.personal_bests {
            if best.player_id != player
                || !courses.contains_key(&best.course_id)
                || !bests.insert(best.course_id)
                || !finite_range(best.seconds, 0.000_001, 1e12)
            {
                return Err(invalid("another driver's race result"));
            }
        }
        Ok(())
    }
}

// Internally tagged network envelopes buffer JSON map keys as strings. Accept
// both the ordinary JSON adapter and the buffered representation, while keeping
// profile allocation bounded and rejecting duplicate/noncanonical identities.
pub(super) fn deserialize_profiles<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<u64, CharacterProfile>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, MapAccess, Visitor};
    struct Key(u64);
    impl<'de> Deserialize<'de> for Key {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct KeyVisitor;
            impl Visitor<'_> for KeyVisitor {
                type Value = Key;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a positive character owner ID")
                }
                fn visit_u64<E: Error>(self, id: u64) -> Result<Key, E> {
                    if id == 0 {
                        Err(E::custom("character ID must be positive"))
                    } else {
                        Ok(Key(id))
                    }
                }
                fn visit_i64<E: Error>(self, id: i64) -> Result<Key, E> {
                    self.visit_u64(u64::try_from(id).map_err(E::custom)?)
                }
                fn visit_str<E: Error>(self, id: &str) -> Result<Key, E> {
                    if id.is_empty()
                        || id.len() > 20
                        || !id.bytes().all(|b| b.is_ascii_digit())
                        || id.starts_with('0')
                    {
                        return Err(E::custom("invalid character owner ID"));
                    }
                    self.visit_u64(id.parse::<u64>().map_err(E::custom)?)
                }
            }
            deserializer.deserialize_any(KeyVisitor)
        }
    }
    struct ProfilesVisitor;
    impl<'de> Visitor<'de> for ProfilesVisitor {
        type Value = BTreeMap<u64, CharacterProfile>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 64 visible cosmetic profiles")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            if map.size_hint().is_some_and(|n| n > 64) {
                return Err(M::Error::custom("too many visible profiles"));
            }
            let mut result = BTreeMap::new();
            while let Some((Key(id), profile)) = map.next_entry::<Key, CharacterProfile>()? {
                if result.len() >= 64 || result.insert(id, profile).is_some() {
                    return Err(M::Error::custom("duplicate or excess visible profile"));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(ProfilesVisitor)
}
