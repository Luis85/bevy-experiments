//! Fixed-step needs, movement, production, dispatch and material transfer.
use super::autonomy::{AutonomousDriver, advance_vehicle, resolve_compatibility_envelope};
use super::*;

/// Walk around a stopped body with a stable side choice. Per-frame lateral
/// retries alone can oscillate across the same line forever at a vehicle nose.
/// This only suggests navigation: ordinary source/body sweeps admit each step.
fn insert_walking_detour(
    unit: &mut LivingUnit,
    target: GeoPosition,
    obstacle: u64,
    contacts: &LivingContacts,
    resolve: &impl Fn(ActorKind, GeoPosition, GeoPosition) -> Option<GeoPosition>,
) -> bool {
    match unit.order {
        UnitOrder::Move { .. }
            if unit.path.len() + 2 <= 129 && unit.path_index < unit.path.len() => {}
        UnitOrder::Patrol if unit.path.is_empty() => {}
        _ => return false,
    }
    for side in [1., -1.] {
        let Some(mut corners) =
            contacts.avoidance_corners(unit.id, obstacle, unit.position, target, side)
        else {
            continue;
        };
        if unit.position.surface_distance_m(corners[0]) < 2.
            || target.surface_distance_m(corners[0]) < 2.
        {
            continue;
        }
        let mut from = unit.position;
        let mut admitted = true;
        for corner in &mut corners {
            let Some(grounded) = resolve(unit.kind, from, *corner)
                .filter(|p| p.validate().is_ok() && p.surface_distance_m(*corner) < 0.05)
            else {
                admitted = false;
                break;
            };
            if contacts
                .resolve(
                    unit.id,
                    from,
                    unit.heading_deg,
                    grounded,
                    unit.heading_deg,
                    [0.; 3],
                )
                .map_or(true, |motion| motion.contact.is_some())
            {
                admitted = false;
                break;
            }
            *corner = grounded;
            from = grounded;
        }
        if !admitted {
            continue;
        }
        if matches!(unit.order, UnitOrder::Patrol) {
            unit.path.extend(corners);
        } else {
            unit.path.splice(unit.path_index..unit.path_index, corners);
        }
        return true;
    }
    false
}
use crate::GeoSpatialIndex;

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
        self.advance_living_with_environment_and_contacts(real_dt, sim_dt, catalog, true, resolve)
    }

    /// As above, with an explicit opt-out for a game's non-colliding local
    /// simulation. Multiplayer authority uses body contacts by default.
    pub fn advance_living_with_environment_and_contacts(
        &mut self,
        real_dt: f64,
        sim_dt: f64,
        catalog: &Catalog,
        collision: bool,
        resolve: impl Fn(ActorKind, GeoPosition, GeoPosition) -> Option<GeoPosition>,
    ) -> Result<(), SandboxError> {
        let sample = |p| {
            resolve(ActorKind::Vehicle, p, p).map(|p| terra_core::VehicleGroundSample {
                height_m: p.altitude_m,
                surface: terra_core::VehicleSurface::Paved,
            })
        };
        let envelope = |from: &ActorState, to: &ActorState, dimensions| {
            resolve_compatibility_envelope(from, to, dimensions, &resolve)
        };
        self.advance_living_with_vehicle_environment(
            real_dt,
            sim_dt,
            catalog,
            collision,
            &resolve,
            AutonomousVehicleEnvironment {
                sample: &sample,
                resolve: &envelope,
                weather_grip: 1.,
            },
        )
    }

    /// Native and network authorities supply the same supported material,
    /// weather and model-sized envelope used by controlled road vehicles.
    #[allow(clippy::too_many_arguments)]
    pub fn advance_living_with_vehicle_environment(
        &mut self,
        real_dt: f64,
        sim_dt: f64,
        catalog: &Catalog,
        collision: bool,
        resolve: &impl Fn(ActorKind, GeoPosition, GeoPosition) -> Option<GeoPosition>,
        vehicle_environment: AutonomousVehicleEnvironment<'_>,
    ) -> Result<(), SandboxError> {
        if !finite_range(real_dt, 0., 5.)
            || !finite_range(sim_dt, 0., 86_400.)
            || !finite_range(vehicle_environment.weather_grip, 0.05, 2.)
        {
            return Err(SandboxError::Invalid("living timestep".into()));
        }
        if !self.living.enabled {
            return Ok(());
        }
        self.prepare_company_drivers(real_dt)?;
        self.living.elapsed_seconds = (self.living.elapsed_seconds + sim_dt).min(1e12);
        let possessed: BTreeSet<_> = self
            .living
            .accounts
            .values()
            .filter(|a| a.possessed)
            .map(|a| a.active_unit)
            .collect();
        // The walking avatar stays parked when its owner enters a vehicle.
        // Driving still consumes that person's energy, even if an earlier RTS
        // order had left the parked avatar resting. Charge physical time only.
        let driving_avatars: BTreeSet<_> =
            self.living
                .accounts
                .values()
                .filter(|a| a.possessed)
                .filter(|a| {
                    self.transport
                        .vehicles
                        .get(&a.active_unit)
                        .is_some_and(|v| v.owner == a.player_id)
                        && self.living.units.get(&a.active_unit).is_some_and(|u| {
                            u.velocity_enu_m_s[0].hypot(u.velocity_enu_m_s[1]) > 0.1
                        })
                })
                .map(|a| a.avatar_unit)
                .collect();
        // Manual authorities account their own accepted motion. This path
        // accounts only unpossessed owned road vehicles, including RTS orders
        // and autopilot, so distance and fuel are never billed twice.
        let transport_starts: Vec<_> = self
            .transport
            .vehicles
            .values()
            .filter(|v| !possessed.contains(&v.unit_id) && !self.is_player_dormant(v.owner))
            .filter_map(|v| {
                self.living
                    .units
                    .get(&v.unit_id)
                    .map(|u| (v.unit_id, v.owner, u.position))
            })
            .collect();
        let transport_caps: BTreeMap<_, _> = self
            .transport
            .vehicles
            .values()
            .map(|v| {
                (
                    v.unit_id,
                    self.vehicle_speed_limit_m_s(v.unit_id).unwrap_or(0.),
                )
            })
            .collect();
        let retired_dynamics: Vec<_> = self
            .living
            .traffic
            .vehicle_dynamics
            .keys()
            .copied()
            .filter(|id| {
                self.transport.vehicles.contains_key(id)
                    || (!self.is_partial() && self.living_unit_reference(*id).is_none())
            })
            .collect();
        for id in retired_dynamics {
            self.living.traffic.vehicle_dynamics.remove(&id);
        }
        let transport_configs: BTreeMap<_, _> = self
            .transport
            .vehicles
            .values()
            .map(|v| {
                (
                    v.unit_id,
                    (
                        self.transport.vehicle_dynamics_config(v.unit_id),
                        v.model.dimensions_m(),
                    ),
                )
            })
            .collect();
        let roads = self.living.roads.clone();
        let mut autonomous_distance = BTreeMap::new();
        let traffic = self.prepare_traffic(real_dt, &possessed, collision)?;
        let mut contacts = collision.then(|| LivingContacts::new(self)).transpose()?;
        // Front-to-back yielding anticipates queues; the swept body checks
        // below also admit parked vehicles, crossing traffic and pedestrians.
        let vehicles: BTreeMap<_, _> = self
            .living
            .units
            .values()
            .filter(|u| {
                matches!(u.kind, ActorKind::Vehicle)
                    && (matches!(u.order, UnitOrder::Move { .. } | UnitOrder::Patrol)
                        || u.velocity_enu_m_s[0].hypot(u.velocity_enu_m_s[1]) > 0.1)
            })
            .map(|u| (u.id, (u.position, u.heading_deg)))
            .collect();
        let vehicle_index = GeoSpatialIndex::from_positions(
            16.,
            vehicles.iter().map(|(id, (position, _))| (*id, *position)),
        )?;
        for unit in self.living.units.values_mut() {
            if unit
                .owner
                .is_some_and(|owner| self.dormant_players.contains(&owner))
            {
                continue;
            }
            let moving = unit.velocity_enu_m_s[0].hypot(unit.velocity_enu_m_s[1]) > 0.1;
            if unit.is_person() {
                unit.needs.food = (unit.needs.food - sim_dt as f32 * 0.000_025).max(0.);
                unit.needs.water = (unit.needs.water - sim_dt as f32 * 0.000_04).max(0.);
                if driving_avatars.contains(&unit.id) {
                    unit.needs.energy = (unit.needs.energy - real_dt as f32 * 0.000_35).max(0.);
                } else if matches!(unit.order, UnitOrder::Rest) {
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
            if unit.kind == ActorKind::Vehicle {
                let query = vehicle_index.query(
                    unit.position,
                    (unit.velocity_enu_m_s[0]
                        .hypot(unit.velocity_enu_m_s[1])
                        .powi(2)
                        * 0.5
                        + 10.)
                        .clamp(8.5, 100.),
                )?;
                self.living.metrics.traffic_queries =
                    self.living.metrics.traffic_queries.saturating_add(1);
                self.living.metrics.traffic_candidate_tests = self
                    .living
                    .metrics
                    .traffic_candidate_tests
                    .saturating_add(query.candidates_tested as u64);
                let heading = f64::from(unit.heading_deg).to_radians();
                let queue_limit = query
                    .ids
                    .iter()
                    .filter(|id| **id != unit.id)
                    .filter_map(|id| {
                        let (p, other_heading) = vehicles[id];
                        let delta = LocalFrame::new(unit.position).to_local(p);
                        let ahead = delta[0] * heading.sin() - delta[2] * heading.cos();
                        let lateral = (delta[0] * heading.cos() + delta[2] * heading.sin()).abs();
                        let angle = (f64::from(other_heading - unit.heading_deg) + 540.)
                            .rem_euclid(360.)
                            - 180.;
                        (ahead > 0.1 && lateral < 2. && angle.abs() < 65.)
                            .then_some((ahead - 7.).max(0.))
                    })
                    .fold(f64::INFINITY, f64::min);
                let nominal = match unit.role {
                    UnitRole::Traffic => 8.,
                    UnitRole::Hauler => 6.,
                    _ => 10.,
                };
                let cap = unit.speed_limit_m_s().min(nominal).min(
                    transport_caps
                        .get(&unit.id)
                        .copied()
                        .unwrap_or(f64::INFINITY),
                );
                let (config, dimensions) =
                    transport_configs.get(&unit.id).copied().unwrap_or_else(|| {
                        let dimensions = if unit.role == UnitRole::Hauler {
                            [1.95, 1.8, 4.]
                        } else {
                            [2.2, 1.8, 3.9]
                        };
                        (
                            Some(terra_core::VehicleDynamicsConfig::default()),
                            dimensions,
                        )
                    });
                let Some(config) = config else {
                    unit.velocity_enu_m_s = [0.; 3];
                    continue;
                };
                let dynamics = if let Some(vehicle) = self.transport.vehicles.get_mut(&unit.id) {
                    &mut vehicle.dynamics
                } else {
                    self.living
                        .traffic
                        .vehicle_dynamics
                        .entry(unit.id)
                        .or_insert_with(|| {
                            let heading = f64::from(unit.heading_deg).to_radians();
                            terra_core::VehicleKinematics {
                                speed_m_s: unit.velocity_enu_m_s[0] * heading.sin()
                                    + unit.velocity_enu_m_s[1] * heading.cos(),
                                ..Default::default()
                            }
                        })
                };
                let distance = advance_vehicle(
                    unit,
                    dynamics,
                    &AutonomousDriver {
                        config,
                        dimensions,
                        cap,
                        traffic: &traffic,
                        environment: &vehicle_environment,
                        roads: &roads,
                        queue_limit,
                    },
                    real_dt,
                    contacts.as_mut(),
                )?;
                autonomous_distance.insert(unit.id, distance);
                continue;
            }
            let target = match unit.order {
                UnitOrder::Move { destination } => unit
                    .path
                    .get(unit.path_index)
                    .copied()
                    .or(Some(destination)),
                UnitOrder::Patrol => unit
                    .path
                    .first()
                    .copied()
                    .or_else(|| unit.patrol.get(unit.path_index).copied()),
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
                        if !unit.path.is_empty() {
                            // Walking detours are temporary; reaching one must
                            // not advance or reverse the original patrol.
                            unit.path.remove(0);
                        } else if unit.kind == ActorKind::Vehicle
                            && unit.patrol.len() > 2
                            && unit.patrol[0].surface_distance_m(*unit.patrol.last().unwrap()) < 2.
                        {
                            unit.patrol_reverse = false;
                            unit.path_index = if unit.path_index + 1 >= unit.patrol.len() {
                                1
                            } else {
                                unit.path_index + 1
                            };
                        } else if unit.patrol_reverse {
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
            let cap = unit.speed_limit_m_s().min(
                transport_caps
                    .get(&unit.id)
                    .copied()
                    .unwrap_or(f64::INFINITY),
            );
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
            let speed = cap.min(nominal).min(distance / real_dt.max(0.000_001));
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
                if let Some(contacts) = &mut contacts {
                    let mut admitted = contacts.resolve(
                        unit.id,
                        unit.position,
                        unit.heading_deg,
                        position,
                        actor.heading_deg,
                        velocity,
                    )?;
                    let mut accepted_velocity = velocity;
                    // People and wildlife can walk around a parked vehicle,
                    // including legacy patrols saved on the road centreline.
                    // Every sidestep has the same speed/time budget and passes
                    // terrain plus body admission; blocked bodies never warp.
                    if admitted.contact.is_some() && unit.is_person() && speed * real_dt > 0.01 {
                        if let Some(contact) = admitted.contact {
                            insert_walking_detour(
                                unit,
                                target,
                                contact.other_id,
                                contacts,
                                resolve,
                            );
                        }
                        for side in [1., -1.] {
                            let trial_velocity = [
                                -delta[2] / distance * speed * side,
                                -delta[0] / distance * speed * side,
                                0.,
                            ];
                            let mut trial = ActorState {
                                velocity_enu_m_s: trial_velocity,
                                ..actor.clone()
                            };
                            trial.position = unit.position;
                            trial.advance(real_dt)?;
                            if let Some(position) =
                                resolve(unit.kind, unit.position, trial.position)
                                    .filter(|p| p.validate().is_ok())
                            {
                                let alternative = contacts.resolve(
                                    unit.id,
                                    unit.position,
                                    unit.heading_deg,
                                    position,
                                    trial.heading_deg,
                                    trial_velocity,
                                )?;
                                if alternative.contact.is_none()
                                    && unit.position.surface_distance_m(alternative.position) > 0.01
                                {
                                    admitted = alternative;
                                    accepted_velocity = trial_velocity;
                                    break;
                                }
                            }
                        }
                    }
                    unit.position = admitted.position;
                    unit.heading_deg = admitted.heading_deg;
                    unit.velocity_enu_m_s = if admitted.contact.is_some() {
                        [0.; 3]
                    } else {
                        accepted_velocity
                    };
                    contacts.update(
                        unit.id,
                        unit.position,
                        unit.heading_deg,
                        unit.velocity_enu_m_s,
                    )?;
                } else {
                    unit.position = position;
                    unit.heading_deg = actor.heading_deg;
                    unit.velocity_enu_m_s = velocity;
                }
            } else {
                unit.velocity_enu_m_s = [0.; 3];
            }
        }
        for (id, owner, previous) in transport_starts {
            let unit = &self.living.units[&id];
            let distance = autonomous_distance
                .get(&id)
                .copied()
                .unwrap_or_else(|| previous.surface_distance_m(unit.position));
            let idle = matches!(unit.order, UnitOrder::Idle | UnitOrder::Rest);
            self.record_vehicle_motion(owner, id, distance, real_dt, 0.)?;
            let vehicle = self.transport.vehicles.get_mut(&id).unwrap();
            if vehicle.fuel_l <= 0. {
                vehicle.dynamics.engine_rpm = 0.;
            }
            if idle {
                vehicle.autopilot = false;
            }
        }
        self.advance_transport(real_dt)?;
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
        self.process_deliveries(real_dt)?;
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

    fn process_deliveries(&mut self, real_dt: f64) -> Result<(), SandboxError> {
        let mut remaining = Vec::with_capacity(self.living.deliveries.len());
        for mut delivery in std::mem::take(&mut self.living.deliveries) {
            if self
                .living_unit_reference(delivery.unit_id)
                .is_some_and(|(owner, _, _)| {
                    owner.is_some_and(|owner| self.is_player_dormant(owner))
                })
            {
                remaining.push(delivery);
                continue;
            }
            let Some(unit) = self.living.units.get(&delivery.unit_id) else {
                return Err(SandboxError::Invalid("delivery courier".into()));
            };
            let position = unit.position;
            let moved = delivery
                .last_position
                .is_some_and(|p| p.surface_distance_m(position) > 0.005);
            delivery.stalled_seconds = if moved {
                0.
            } else {
                (delivery.stalled_seconds + real_dt).min(86_400.)
            };
            delivery.last_position = Some(position);
            let source_position = self
                .living
                .producers
                .get(&delivery.from_site)
                .ok_or_else(|| SandboxError::Invalid("delivery source".into()))?
                .position;
            let destination = if matches!(
                delivery.phase,
                DeliveryPhase::ToPickup | DeliveryPhase::Returning
            ) {
                source_position
            } else {
                self.living
                    .producers
                    .get(&delivery.to_site)
                    .map(|s| s.position)
                    .or_else(|| self.living.shops.get(&delivery.to_site).map(|s| s.position))
                    .ok_or_else(|| SandboxError::Invalid("delivery destination".into()))?
            };
            if position
                .surface_distance_m(destination)
                .hypot(position.altitude_m - destination.altitude_m)
                > 2.5
            {
                if delivery.stalled_seconds >= 12. && delivery.route_retries < 3 {
                    delivery.route_retries += 1;
                    delivery.stalled_seconds = 0.;
                    self.replan_courier(&delivery, destination, true);
                } else if delivery.stalled_seconds >= 30.
                    && delivery.phase == DeliveryPhase::ToPickup
                {
                    // No cargo was removed yet: release the reservation safely.
                    self.clear_courier_order(delivery.unit_id);
                    continue;
                } else if delivery.stalled_seconds >= 30.
                    && delivery.phase == DeliveryPhase::ToDropoff
                {
                    delivery.phase = DeliveryPhase::Returning;
                    delivery.route_retries = 0;
                    delivery.stalled_seconds = 0.;
                    self.replan_courier(&delivery, source_position, false);
                }
                remaining.push(delivery);
                continue;
            }
            if delivery.phase == DeliveryPhase::ToPickup {
                let source = self.living.producers.get_mut(&delivery.from_site).unwrap();
                let amount = source
                    .stock
                    .get(&delivery.item)
                    .copied()
                    .unwrap_or(0)
                    .min(delivery.amount);
                if amount == 0 {
                    self.clear_courier_order(delivery.unit_id);
                    continue;
                }
                subtract(&mut source.stock, &delivery.item, amount);
                delivery.amount = amount;
                delivery.phase = DeliveryPhase::ToDropoff;
                delivery.route_retries = 0;
                delivery.stalled_seconds = 0.;
                let destination = self
                    .living
                    .producers
                    .get(&delivery.to_site)
                    .map(|s| s.position)
                    .or_else(|| self.living.shops.get(&delivery.to_site).map(|s| s.position))
                    .unwrap();
                self.living
                    .units
                    .get_mut(&delivery.unit_id)
                    .unwrap()
                    .supplies
                    .insert(delivery.item.clone(), amount);
                self.replan_courier(&delivery, destination, false);
                remaining.push(delivery);
            } else {
                let target = if delivery.phase == DeliveryPhase::Returning {
                    delivery.from_site
                } else {
                    delivery.to_site
                };
                let stock = self
                    .living
                    .producers
                    .get(&target)
                    .map(|s| &s.stock)
                    .or_else(|| self.living.shops.get(&target).map(|s| &s.stock))
                    .unwrap();
                let full = (self.living.producers.contains_key(&target)
                    && stock.values().map(|n| u64::from(*n)).sum::<u64>()
                        + u64::from(delivery.amount)
                        > 2000)
                    || stock
                        .get(&delivery.item)
                        .copied()
                        .unwrap_or(0)
                        .checked_add(delivery.amount)
                        .is_none_or(|n| n > crate::MAX_ITEM_COUNT);
                if full {
                    if delivery.stalled_seconds >= 30. && delivery.phase == DeliveryPhase::ToDropoff
                    {
                        delivery.phase = DeliveryPhase::Returning;
                        delivery.route_retries = 0;
                        delivery.stalled_seconds = 0.;
                        self.replan_courier(&delivery, source_position, false);
                    }
                    remaining.push(delivery);
                    continue;
                }
                let stock = if let Some(site) = self.living.producers.get_mut(&target) {
                    &mut site.stock
                } else {
                    &mut self.living.shops.get_mut(&target).unwrap().stock
                };
                *stock.entry(delivery.item.clone()).or_default() += delivery.amount;
                self.living
                    .units
                    .get_mut(&delivery.unit_id)
                    .unwrap()
                    .supplies
                    .remove(&delivery.item);
                self.clear_courier_order(delivery.unit_id);
                if delivery.phase == DeliveryPhase::Returning {
                    self.living.metrics.deliveries_returned =
                        self.living.metrics.deliveries_returned.saturating_add(1);
                } else {
                    self.living.metrics.deliveries_completed =
                        self.living.metrics.deliveries_completed.saturating_add(1);
                }
            }
        }
        self.living.deliveries = remaining;
        Ok(())
    }

    fn clear_courier_order(&mut self, id: u64) {
        let unit = self.living.units.get_mut(&id).unwrap();
        unit.order = UnitOrder::Idle;
        unit.path.clear();
        unit.path_index = 0;
        unit.velocity_enu_m_s = [0.; 3];
    }

    fn replan_courier(&mut self, delivery: &Delivery, destination: GeoPosition, detour: bool) {
        let position = self.living.units[&delivery.unit_id].position;
        let mut path = self.living.vehicle_route_between(position, destination);
        if detour && path.len() < 129 {
            let frame = LocalFrame::new(position);
            let target = frame.to_local(path.first().copied().unwrap_or(destination));
            let length = target[0].hypot(target[2]).max(0.01);
            let side = if delivery.route_retries % 2 == 1 {
                1.
            } else {
                -1.
            };
            let width = 3. + f64::from(delivery.route_retries) * 2.;
            if let Ok(point) = frame.to_geo([
                -target[2] / length * width * side,
                0.,
                target[0] / length * width * side,
            ]) {
                path.insert(0, point);
            }
            self.living.metrics.delivery_replans =
                self.living.metrics.delivery_replans.saturating_add(1);
        }
        let unit = self.living.units.get_mut(&delivery.unit_id).unwrap();
        unit.order = UnitOrder::Move { destination };
        unit.path = path;
        unit.path_index = 0;
    }

    fn dispatch_deliveries(&mut self) {
        let busy: BTreeSet<_> = self.living.deliveries.iter().map(|d| d.unit_id).collect();
        // Reserve both service forecourts together. Opposing loaded couriers
        // must not each hold one narrow bay while waiting for the other. The
        // reservations are derived from durable deliveries, so save/restore
        // cannot lose a booking or require a separate repair ledger.
        let mut service_bays: BTreeSet<_> = self
            .living
            .deliveries
            .iter()
            .flat_map(|delivery| [delivery.from_site, delivery.to_site])
            .collect();
        let couriers: Vec<_> = self
            .living
            .units
            .values()
            .filter(|u| {
                u.role == UnitRole::Hauler
                    && !busy.contains(&u.id)
                    && !u.owner.is_some_and(|owner| self.is_player_dormant(owner))
            })
            .map(|u| u.id)
            .collect();
        for courier in couriers {
            if self.living.deliveries.len() >= 16 {
                break;
            }
            let position = self.living.units[&courier].position;
            let mut requests = Vec::new();
            for destination in self.living.producers.values().filter(|s| s.enabled) {
                let room = 2000_u64.saturating_sub(
                    destination
                        .stock
                        .values()
                        .map(|n| u64::from(*n))
                        .sum::<u64>(),
                );
                for (item, per_cycle) in &destination.inputs {
                    let current = destination.stock.get(item).copied().unwrap_or(0);
                    let target = per_cycle.saturating_mul(8);
                    if current >= target
                        || self.living.deliveries.iter().any(|d| {
                            d.to_site == destination.id
                                && d.item == *item
                                && d.phase != DeliveryPhase::Returning
                        })
                    {
                        continue;
                    }
                    for source in self
                        .living
                        .producers
                        .values()
                        .filter(|s| s.id != destination.id && s.outputs.contains_key(item))
                    {
                        let reserved: u32 = self
                            .living
                            .deliveries
                            .iter()
                            .filter(|d| {
                                d.from_site == source.id
                                    && d.item == *item
                                    && d.phase == DeliveryPhase::ToPickup
                            })
                            .map(|d| d.amount)
                            .sum();
                        let available = source
                            .stock
                            .get(item)
                            .copied()
                            .unwrap_or(0)
                            .saturating_sub(reserved);
                        let amount = available.min(target - current).min(12).min(room as u32);
                        if amount == 0 {
                            continue;
                        }
                        let urgency = if current < *per_cycle { 0 } else { 1 };
                        let coverage = current / (*per_cycle).max(1);
                        let distance = position.surface_distance_m(source.position)
                            + source.position.surface_distance_m(destination.position);
                        requests.push((
                            urgency,
                            coverage,
                            distance,
                            source.id,
                            destination.id,
                            item.clone(),
                            amount,
                            source.position,
                        ));
                    }
                }
            }
            for shop in self.living.shops.values() {
                for item in shop.prices.keys() {
                    let current = shop.stock.get(item).copied().unwrap_or(0);
                    if current >= 60
                        || self.living.deliveries.iter().any(|d| {
                            d.to_site == shop.id
                                && d.item == *item
                                && d.phase != DeliveryPhase::Returning
                        })
                    {
                        continue;
                    }
                    for source in self
                        .living
                        .producers
                        .values()
                        .filter(|s| s.outputs.contains_key(item))
                    {
                        let reserved: u32 = self
                            .living
                            .deliveries
                            .iter()
                            .filter(|d| {
                                d.from_site == source.id
                                    && d.item == *item
                                    && d.phase == DeliveryPhase::ToPickup
                            })
                            .map(|d| d.amount)
                            .sum();
                        let available = source
                            .stock
                            .get(item)
                            .copied()
                            .unwrap_or(0)
                            .saturating_sub(reserved);
                        let amount = available.min(60 - current).min(12);
                        if amount == 0 {
                            continue;
                        }
                        let distance = position.surface_distance_m(source.position)
                            + source.position.surface_distance_m(shop.position);
                        requests.push((
                            2,
                            current / 4,
                            distance,
                            source.id,
                            shop.id,
                            item.clone(),
                            amount,
                            source.position,
                        ));
                    }
                }
            }
            requests.retain(|request| {
                !service_bays.contains(&request.3) && !service_bays.contains(&request.4)
            });
            requests.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then(a.1.cmp(&b.1))
                    .then(a.2.total_cmp(&b.2))
                    .then(a.3.cmp(&b.3))
                    .then(a.4.cmp(&b.4))
                    .then(a.5.cmp(&b.5))
            });
            let Some((_, _, _, from_site, to_site, item, amount, destination)) =
                requests.into_iter().next()
            else {
                break;
            };
            let Ok(id) = self.living.next() else {
                break;
            };
            service_bays.insert(from_site);
            service_bays.insert(to_site);
            let path = self.living.vehicle_route_between(position, destination);
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
                stalled_seconds: 0.,
                route_retries: 0,
                last_position: Some(position),
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
