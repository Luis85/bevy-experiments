use super::seed::safe_offset;
use super::*;
use crate::GeoSpatialIndex;

impl SandboxState {
    pub fn advance_ecology(
        &mut self,
        real_dt: f64,
        sim_dt: f64,
        hour_of_day: f64,
    ) -> Result<(), SandboxError> {
        self.advance_ecology_with_environment(real_dt, sim_dt, hour_of_day, Some)
    }

    /// Decide behavior before the shared living movement update. Position
    /// changes still go through that update's authoritative physical resolver.
    /// The supplied resolver is additionally required for targets and births.
    pub fn advance_ecology_with_environment(
        &mut self,
        real_dt: f64,
        sim_dt: f64,
        hour_of_day: f64,
        resolve: impl Fn(GeoPosition) -> Option<GeoPosition>,
    ) -> Result<(), SandboxError> {
        self.advance_ecology_inner(real_dt, sim_dt, hour_of_day, None, &[], resolve)
    }

    /// Budget only behavior decisions. Needs, recovery clocks, movement and
    /// economic transfers retain their normal fixed-step updates. Observers are
    /// authoritative owned-body positions, never an untrusted/free camera.
    pub fn advance_ecology_scheduled_with_environment(
        &mut self,
        real_dt: f64,
        sim_dt: f64,
        hour_of_day: f64,
        scheduler: &mut EcologyScheduler,
        observers: &[GeoPosition],
        resolve: impl Fn(GeoPosition) -> Option<GeoPosition>,
    ) -> Result<(), SandboxError> {
        if observers.len() > MAX_LIVING_UNITS || observers.iter().any(|p| p.validate().is_err()) {
            return Err(SandboxError::Invalid("ecology observers".into()));
        }
        self.advance_ecology_inner(
            real_dt,
            sim_dt,
            hour_of_day,
            Some(scheduler),
            observers,
            resolve,
        )
    }

    fn advance_ecology_inner(
        &mut self,
        real_dt: f64,
        sim_dt: f64,
        hour_of_day: f64,
        mut scheduler: Option<&mut EcologyScheduler>,
        observers: &[GeoPosition],
        resolve: impl Fn(GeoPosition) -> Option<GeoPosition>,
    ) -> Result<(), SandboxError> {
        if !finite_range(real_dt, 0., 5.)
            || !finite_range(sim_dt, 0., 86_400.)
            || !finite_range(hour_of_day, 0., 24.)
        {
            return Err(SandboxError::Invalid("ecology timestep or hour".into()));
        }
        self.ecology.config.validate()?;
        if !self.ecology.config.enabled || !self.living.enabled {
            return Ok(());
        }
        self.ecology.elapsed_seconds = (self.ecology.elapsed_seconds + sim_dt).min(1e12);
        if let Some(scheduler) = scheduler.as_deref_mut() {
            scheduler.advance_clock(real_dt, self.ecology.elapsed_seconds);
        }
        // Games may remove a wildlife body through an authoritative lifecycle
        // policy. A removed body is recovered slowly, never by reloading a tile.
        self.ecology.animals.retain(|id, _| {
            self.living
                .units
                .get(id)
                .is_some_and(|u| u.kind == ActorKind::Animal)
        });
        for animal in self.ecology.animals.values_mut() {
            if self
                .living
                .units
                .get(&animal.unit_id)
                .and_then(|unit| unit.owner)
                .is_some_and(|owner| self.dormant_players.contains(&owner))
            {
                continue;
            }
            animal.age_seconds = (animal.age_seconds + sim_dt).min(1e12);
            animal.decision_remaining_seconds =
                (animal.decision_remaining_seconds - real_dt).max(0.);
            let Some(unit) = self.living.units.get_mut(&animal.unit_id) else {
                continue;
            };
            if unit.owner.is_some() {
                continue;
            }
            match animal.behavior {
                WildlifeBehavior::Grazing => {
                    // Grazing/foraging is illustrative habitat nutrition, not
                    // a transfer from player harvest stock.
                    unit.needs.food = (unit.needs.food
                        + (sim_dt * self.ecology.config.grazing_food_per_sim_second) as f32)
                        .min(1.);
                    unit.needs.water = (unit.needs.water
                        + (sim_dt * self.ecology.config.grazing_water_per_sim_second) as f32)
                        .min(1.);
                }
                WildlifeBehavior::Drinking => {
                    if self
                        .ecology
                        .habitats
                        .get(&animal.habitat_id)
                        .and_then(|h| h.water_access)
                        .is_some_and(|p| unit.position.surface_distance_m(p) < 6.)
                    {
                        unit.needs.water = (unit.needs.water
                            + (sim_dt * self.ecology.config.drinking_water_per_sim_second) as f32)
                            .min(1.);
                    }
                }
                WildlifeBehavior::Resting => {
                    unit.needs.energy = (unit.needs.energy
                        + (real_dt * self.ecology.config.rest_energy_per_real_second) as f32)
                        .min(1.);
                }
                _ => {}
            }
        }
        for habitat in self.ecology.habitats.values_mut() {
            if let Some(definition) = self.ecology.config.species.get(&habitat.species) {
                habitat.recovery_accumulator_seconds = (habitat.recovery_accumulator_seconds
                    + sim_dt)
                    .min(definition.recovery_seconds);
            }
        }
        self.ecology.update_accumulator_seconds += real_dt;
        let interval = self.ecology.config.update_interval_seconds;
        if self.ecology.update_accumulator_seconds < interval {
            return Ok(());
        }
        self.ecology.update_accumulator_seconds =
            self.ecology.update_accumulator_seconds.rem_euclid(interval);
        let centers: Vec<_> = self
            .players
            .values()
            .filter(|p| !self.is_player_dormant(p.id))
            .map(|p| p.position)
            .chain(observers.iter().copied())
            .collect();
        let active_radius = self.ecology.config.active_radius_m;
        let active = |position: GeoPosition| {
            centers
                .iter()
                .any(|p| p.surface_distance_m(position) <= active_radius)
        };
        let near_radius = scheduler.as_ref().map_or(0., |s| s.config().near_radius_m);
        // Explicit observers own near-priority admission. Persisted players
        // still define the existing regional activity scope, but a disconnected
        // account is not implicitly a near observer for the scheduled path.
        let near_centers = if scheduler.is_some() {
            observers
        } else {
            &centers
        };
        let observer_index = GeoSpatialIndex::from_positions(
            256.,
            near_centers
                .iter()
                .enumerate()
                .map(|(id, p)| (id as u64, *p)),
        )?;
        // Rebuild once per behavior pass, not per animal. Geographic cells
        // avoid an all-pairs threat scan without missing date-line neighbors.
        let threats: BTreeMap<_, _> = self
            .living
            .units
            .values()
            .filter(|u| {
                !matches!(
                    u.kind,
                    ActorKind::Animal
                        | ActorKind::Ship
                        | ActorKind::Balloon
                        | ActorKind::Helicopter
                        | ActorKind::Airplane
                ) || self
                    .ecology
                    .animals
                    .get(&u.id)
                    .is_some_and(|a| a.species == AnimalSpecies::Fox)
            })
            .map(|u| (u.id, (u.position, u.kind)))
            .collect();
        let threat_index =
            GeoSpatialIndex::from_positions(64., threats.iter().map(|(id, (p, _))| (*id, *p)))?;
        let mut contexts = BTreeMap::new();
        let mut candidates = Vec::new();
        let mut threat_candidates = 0;
        let mut observer_candidates = 0;
        for animal in self.ecology.animals.values() {
            let Some(unit) = self.living.units.get(&animal.unit_id) else {
                continue;
            };
            if unit.owner.is_some() || !active(unit.position) {
                continue;
            }
            let Some(definition) = self.ecology.config.species.get(&animal.species) else {
                continue;
            };
            let position = unit.position;
            let query =
                threat_index.query(position, (definition.flee_distance_m * 1.35).max(16.))?;
            threat_candidates += query.candidates_tested;
            let threat = query
                .ids
                .into_iter()
                .filter_map(|id| {
                    let &(p, kind) = threats.get(&id)?;
                    let distance = position.surface_distance_m(p);
                    let admitted = if kind == ActorKind::Animal {
                        animal.species == AnimalSpecies::Hare && distance < 16.
                    } else {
                        let radius = definition.flee_distance_m
                            * if kind == ActorKind::Vehicle { 1.35 } else { 1. };
                        distance < radius && (p.altitude_m - position.altitude_m).abs() < 12.
                    };
                    admitted.then_some((distance, p))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, p)| p);
            let near = if scheduler.is_some() {
                let query = observer_index.query(position, near_radius)?;
                observer_candidates += query.candidates_tested;
                !query.ids.is_empty() || threat.is_some()
            } else {
                true
            };
            candidates.push((animal.unit_id, near));
            contexts.insert(animal.unit_id, threat);
        }
        let admitted = scheduler
            .as_deref_mut()
            .map(|s| s.select(&candidates, threat_candidates, observer_candidates));
        for (id, threat) in contexts {
            if admitted.as_ref().is_some_and(|ids| !ids.contains(&id)) {
                continue;
            }
            let Some(animal) = self.ecology.animals.get(&id).cloned() else {
                continue;
            };
            let Some(unit) = self.living.units.get(&id) else {
                continue;
            };
            let Some(habitat) = self.ecology.habitats.get(&animal.habitat_id) else {
                continue;
            };
            let Some(definition) = self.ecology.config.species.get(&animal.species) else {
                continue;
            };
            let position = unit.position;
            if animal.decision_remaining_seconds > 0. && threat.is_none() {
                continue;
            }
            let mut behavior;
            let mut target = None;
            if let Some(threat) = threat {
                behavior = WildlifeBehavior::Fleeing;
                let d = LocalFrame::new(threat).to_local(position);
                let length = d[0].hypot(d[2]);
                let (east, north) = if length > 0.01 {
                    (d[0] / length * 28., -d[2] / length * 28.)
                } else {
                    (21., 21.)
                };
                target = safe_offset(position, east, north, &resolve);
                // A herd stays bounded to a recoverable home area. This is a
                // local avoidance controller, not an unbounded chase model.
                if target
                    .is_some_and(|p| p.surface_distance_m(habitat.center) > habitat.radius_m * 2.5)
                {
                    target = safe_offset(habitat.center, east * 0.5, north * 0.5, &resolve);
                }
                if animal.behavior != WildlifeBehavior::Fleeing {
                    self.ecology.metrics.flee_events =
                        self.ecology.metrics.flee_events.saturating_add(1);
                }
            } else if unit.needs.water < 0.55 && habitat.water_access.is_some() {
                behavior = WildlifeBehavior::Drinking;
                target = habitat
                    .water_access
                    .and_then(&resolve)
                    .filter(|p| position_valid(*p));
                if target.is_none() {
                    behavior = WildlifeBehavior::Grazing;
                }
            } else if unit.needs.energy < 0.15
                || (!active_hour(
                    hour_of_day,
                    definition.active_start_hour,
                    definition.active_end_hour,
                ) && unit.needs.food > 0.25)
            {
                behavior = WildlifeBehavior::Resting;
            } else if unit.needs.food < 0.55 {
                behavior = WildlifeBehavior::Grazing;
            } else {
                let seed = stable_hash(&animal.habitat_id)
                    .wrapping_add(id.wrapping_mul(17))
                    .wrapping_add((self.ecology.elapsed_seconds / 90.).floor() as u64);
                if seed % 5 < 2 {
                    behavior = WildlifeBehavior::Grazing;
                } else {
                    behavior = WildlifeBehavior::Roaming;
                    let angle = (seed % 6283) as f64 / 1000.;
                    let radius = habitat.radius_m * (0.2 + (seed % 61) as f64 / 100.);
                    target = safe_offset(
                        habitat.center,
                        angle.cos() * radius,
                        angle.sin() * radius,
                        &resolve,
                    );
                    if target.is_none() {
                        behavior = WildlifeBehavior::Grazing;
                    }
                }
            }
            let wait = if behavior == WildlifeBehavior::Fleeing {
                1.
            } else {
                definition.decision_seconds
            };
            let u = self.living.units.get_mut(&id).expect("existing wildlife");
            u.patrol.clear();
            u.path.clear();
            u.path_index = 0;
            u.order = if let Some(destination) =
                target.filter(|p| position.surface_distance_m(*p) > 2.)
            {
                u.path.push(destination);
                UnitOrder::Move { destination }
            } else if behavior == WildlifeBehavior::Resting {
                UnitOrder::Rest
            } else {
                UnitOrder::Idle
            };
            let a = self
                .ecology
                .animals
                .get_mut(&id)
                .expect("existing metadata");
            a.behavior = behavior;
            a.decision_remaining_seconds = wait;
            self.ecology.metrics.behavior_updates =
                self.ecology.metrics.behavior_updates.saturating_add(1);
        }
        // Spawn nearest eligible habitats first. Four attempts per pass bound
        // streaming arrival and population-recovery work even for large dt.
        let mut habitats: Vec<_> = self
            .ecology
            .habitats
            .values()
            .filter(|h| active(h.center))
            .map(|h| h.id.clone())
            .collect();
        habitats.sort_by(|a, b| {
            let dist = |id: &String| {
                centers
                    .iter()
                    .map(|p| p.surface_distance_m(self.ecology.habitats[id].center))
                    .fold(f64::INFINITY, f64::min)
            };
            dist(a).total_cmp(&dist(b)).then(a.cmp(b))
        });
        let mut budget = 4;
        for id in habitats {
            if budget == 0 || self.ecology_animal_count() >= self.ecology.config.max_animals {
                break;
            }
            let h = &self.ecology.habitats[&id];
            let Some(definition) = self.ecology.config.species.get(&h.species) else {
                continue;
            };
            let present = self
                .ecology
                .animals
                .values()
                .filter(|a| a.habitat_id == id)
                .count();
            if present >= usize::from(h.carrying_capacity) {
                self.ecology
                    .habitats
                    .get_mut(&id)
                    .expect("habitat")
                    .initial_population_seeded = true;
                self.ecology
                    .habitats
                    .get_mut(&id)
                    .expect("habitat")
                    .recovery_accumulator_seconds = 0.;
                continue;
            }
            let recovered = h.initial_population_seeded;
            if recovered && h.recovery_accumulator_seconds < definition.recovery_seconds {
                continue;
            }
            budget -= 1;
            if self.spawn_wildlife(&id, &resolve, recovered) {
                let habitat = self.ecology.habitats.get_mut(&id).expect("habitat");
                if recovered {
                    habitat.recovery_accumulator_seconds = 0.;
                }
            }
        }
        Ok(())
    }
}

fn active_hour(hour: f64, start: f64, end: f64) -> bool {
    if start < end {
        (start..end).contains(&hour)
    } else {
        hour >= start || hour < end
    }
}
