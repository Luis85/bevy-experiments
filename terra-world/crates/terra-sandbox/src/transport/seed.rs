use super::*;

impl SandboxState {
    /// Install the authored transport scenario once, then register newly joined
    /// drivers and their existing utility vehicles without resetting progress.
    /// Existing catalog items and producer/shop inventories are reused.
    pub fn ensure_transport_demo(&mut self, catalog: &Catalog) -> Result<(), SandboxError> {
        if !self.living.enabled {
            return Ok(());
        }
        let mut transport = self.transport.clone();
        if !transport.enabled {
            let origin = self
                .living
                .origin
                .ok_or_else(|| invalid("transport origin"))?;
            let frame = LocalFrame::new(origin);
            let garage_position = self
                .living
                .units
                .values()
                .find(|u| u.owner.is_some() && u.kind == ActorKind::Vehicle)
                .map(|u| u.position)
                .unwrap_or(origin);
            transport.enabled = true;
            transport.treasury = 1_000_000;
            let garage = transport.add_facility(
                "Roadside garage",
                TransportFacilityKind::Garage,
                garage_position,
                None,
                0,
                5_000.,
            )?;
            let market = self.living.shops.values().next();
            let town_position = market.map_or(origin, |s| s.position);
            let town = transport.add_facility(
                "Town interchange",
                TransportFacilityKind::Terminal,
                town_position,
                market.map(|s| s.id),
                80,
                0.,
            )?;
            let remote_position = self
                .living
                .roads
                .iter()
                .flat_map(|r| r.points.iter())
                .copied()
                .filter(|p| {
                    p.surface_distance_m(origin) >= 100. && p.surface_distance_m(origin) <= 600.
                })
                .max_by(|a, b| {
                    a.surface_distance_m(origin)
                        .total_cmp(&b.surface_distance_m(origin))
                })
                .unwrap_or(frame.to_geo([120., 0., 90.])?);
            let remote = transport.add_facility(
                "Hillside interchange",
                TransportFacilityKind::Terminal,
                remote_position,
                None,
                80,
                0.,
            )?;
            let hospital = transport.add_facility(
                "Community clinic",
                TransportFacilityKind::Hospital,
                town_position,
                None,
                0,
                0.,
            )?;
            let fire = transport.add_facility(
                "Community fire station",
                TransportFacilityKind::FireStation,
                town_position,
                market.map(|s| s.id),
                0,
                0.,
            )?;
            let mut production = BTreeMap::new();
            for site in self.living.producers.values() {
                let id = transport.add_facility(
                    &site.name,
                    TransportFacilityKind::Depot,
                    site.position,
                    Some(site.id),
                    0,
                    0.,
                )?;
                production.insert(site.id, id);
            }
            transport.add_job(
                "Town to hillside passenger",
                TransportJobKind::Hitchhiker,
                town,
                remote,
                None,
                0,
                1,
                2_500,
            )?;
            transport.add_job(
                "Hillside to town passenger",
                TransportJobKind::Hitchhiker,
                remote,
                town,
                None,
                0,
                1,
                2_500,
            )?;
            transport.add_job(
                "Town taxi fare",
                TransportJobKind::Taxi,
                town,
                remote,
                None,
                0,
                2,
                4_000,
            )?;
            transport.add_job(
                "Hillside taxi fare",
                TransportJobKind::Taxi,
                remote,
                town,
                None,
                0,
                2,
                4_000,
            )?;
            transport.add_job(
                "Outbound bus service",
                TransportJobKind::Bus,
                town,
                remote,
                None,
                0,
                8,
                6_500,
            )?;
            transport.add_job(
                "Inbound bus service",
                TransportJobKind::Bus,
                remote,
                town,
                None,
                0,
                8,
                6_500,
            )?;
            let facility_of = |kind| {
                self.living
                    .producers
                    .values()
                    .find(|s| s.kind == kind)
                    .and_then(|s| production.get(&s.id))
                    .copied()
            };
            for (name, item, amount, reward, from, to) in [
                (
                    "Timber for the sawmill",
                    "wood",
                    8,
                    4_000,
                    facility_of(ProductionKind::Woodlot),
                    facility_of(ProductionKind::Sawmill),
                ),
                (
                    "Irrigation water",
                    "water",
                    12,
                    3_500,
                    facility_of(ProductionKind::Well),
                    facility_of(ProductionKind::Farm),
                ),
                (
                    "Flour for the bakery",
                    "flour",
                    6,
                    3_000,
                    market.map(|_| town),
                    facility_of(ProductionKind::Bakery),
                ),
                (
                    "Bulk water supply",
                    "water",
                    60,
                    7_500,
                    facility_of(ProductionKind::Well),
                    market.map(|_| town),
                ),
            ] {
                if catalog.items.contains(item) {
                    if let (Some(from), Some(to)) = (from, to) {
                        transport.add_job(
                            name,
                            TransportJobKind::Freight,
                            from,
                            to,
                            Some(item.into()),
                            amount,
                            0,
                            reward,
                        )?;
                    }
                }
            }
            transport.add_job(
                "Roadside recovery dispatch",
                TransportJobKind::Tow,
                remote,
                garage,
                None,
                0,
                0,
                5_000,
            )?;
            transport.add_job(
                "Patient transport",
                TransportJobKind::Ambulance,
                remote,
                hospital,
                None,
                0,
                1,
                5_500,
            )?;
            if catalog.items.contains("water") {
                if let (Some(well), Some(_)) = (facility_of(ProductionKind::Well), market) {
                    transport.add_job(
                        "Fire-station water supply",
                        TransportJobKind::FireSupply,
                        well,
                        fire,
                        Some("water".into()),
                        30,
                        0,
                        6_000,
                    )?;
                }
            }
            let mut checkpoints = vec![garage_position];
            for p in self
                .living
                .roads
                .iter()
                .flat_map(|r| r.points.iter())
                .copied()
            {
                if p.surface_distance_m(origin) < 600.
                    && p.surface_distance_m(garage_position) > 40.
                    && p.surface_distance_m(*checkpoints.last().unwrap()) > 60.
                {
                    checkpoints.push(p);
                    if checkpoints.len() == 5 {
                        break;
                    }
                }
            }
            if checkpoints.len() >= 3 {
                checkpoints.push(garage_position);
                let id = transport.next()?;
                transport.courses.insert(
                    id,
                    RaceCourse {
                        id,
                        name: "Settlement road trial".into(),
                        checkpoints,
                        radius_m: 12.,
                    },
                );
            }
        }
        for (&player, account) in &self.living.accounts {
            transport
                .drivers
                .entry(player)
                .or_insert_with(|| DriverProgress::new(player));
            for unit in self
                .living
                .units
                .values()
                .filter(|u| u.owner == Some(player) && u.kind == ActorKind::Vehicle)
            {
                if !transport.vehicles.contains_key(&unit.id) {
                    if transport.vehicles.len() >= MAX_TRANSPORT_VEHICLES {
                        return Err(SandboxError::Capacity);
                    }
                    transport.vehicles.insert(
                        unit.id,
                        VehicleState::new(unit.id, player, VehicleModel::Pickup),
                    );
                }
            }
            if let Some(profile) = transport.profiles.get(&player) {
                profile.validate()?;
                if !self.living.units.contains_key(&account.avatar_unit) {
                    return Err(invalid("driver avatar"));
                }
            }
        }
        transport.validate(self, Some(catalog))?;
        self.transport = transport;
        for (&player, profile) in &self.transport.profiles {
            if let Some(account) = self.living.accounts.get(&player) {
                if let Some(avatar) = self.living.units.get_mut(&account.avatar_unit) {
                    avatar.name = profile.name.clone();
                }
            }
        }
        Ok(())
    }
}

impl TransportWorld {
    pub(super) fn next(&mut self) -> Result<u64, SandboxError> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .filter(|n| *n != u64::MAX)
            .ok_or(SandboxError::Capacity)?;
        Ok(id)
    }
    fn add_facility(
        &mut self,
        name: &str,
        kind: TransportFacilityKind,
        position: GeoPosition,
        linked_site: Option<u64>,
        passengers_waiting: u32,
        fuel_l: f64,
    ) -> Result<u64, SandboxError> {
        position.validate()?;
        if self.facilities.len() >= MAX_TRANSPORT_FACILITIES {
            return Err(SandboxError::Capacity);
        }
        let id = self.next()?;
        self.facilities.insert(
            id,
            TransportFacility {
                id,
                name: name.into(),
                kind,
                position,
                linked_site,
                passengers_waiting,
                fuel_l,
            },
        );
        Ok(id)
    }
    #[allow(clippy::too_many_arguments)]
    fn add_job(
        &mut self,
        name: &str,
        kind: TransportJobKind,
        pickup: u64,
        destination: u64,
        item: Option<String>,
        amount: u32,
        passengers: u16,
        reward: u64,
    ) -> Result<u64, SandboxError> {
        if self.jobs.len() >= MAX_TRANSPORT_JOBS {
            return Err(SandboxError::Capacity);
        }
        let id = self.next()?;
        self.jobs.insert(
            id,
            TransportJob {
                id,
                name: name.into(),
                kind,
                pickup_facility: pickup,
                destination_facility: destination,
                item,
                amount,
                passengers,
                reward,
                time_limit_seconds: 600.,
                assignment: None,
                completions: 0,
            },
        );
        Ok(id)
    }
}
