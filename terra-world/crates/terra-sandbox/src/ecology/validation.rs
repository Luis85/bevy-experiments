use super::*;

impl EcologyConfig {
    pub fn validate(&self) -> Result<(), SandboxError> {
        if self.max_habitats > MAX_ECOLOGY_HABITATS
            || self.max_animals > MAX_ECOLOGY_ANIMALS
            || self.max_resources > MAX_ECOLOGY_RESOURCES
            || self.species.len() > AnimalSpecies::ALL.len()
            || self.resources.len() > 7
        {
            return Err(SandboxError::Capacity);
        }
        if !finite_range(self.active_radius_m, 25., 5000.)
            || !finite_range(self.update_interval_seconds, 0.25, 30.)
            || [
                self.grazing_food_per_sim_second,
                self.grazing_water_per_sim_second,
                self.drinking_water_per_sim_second,
                self.rest_energy_per_real_second,
            ]
            .iter()
            .any(|r| !finite_range(*r, 0., 1.))
        {
            return Err(SandboxError::Invalid(
                "ecology rates or working radius".into(),
            ));
        }
        for (species, d) in &self.species {
            if species != &d.species
                || d.group_size == 0
                || d.group_size > 16
                || !finite_range(d.flee_distance_m, 1., 100.)
                || !finite_range(d.roaming_radius_m, 10., 300.)
                || !finite_range(d.decision_seconds, 0.25, 120.)
                || !finite_range(d.recovery_seconds, 60., 31_536_000.)
                || !finite_range(d.active_start_hour, 0., 24.)
                || !finite_range(d.active_end_hour, 0., 24.)
            {
                return Err(SandboxError::Invalid("wildlife species definition".into()));
            }
        }
        for (kind, d) in &self.resources {
            if kind != &d.kind
                || d.capacity == 0
                || d.capacity > 10_000
                || !finite_range(d.regrow_seconds, 0., 31_536_000.)
            {
                return Err(SandboxError::Invalid("natural resource definition".into()));
            }
        }
        Ok(())
    }
}

impl EcologyWorld {
    pub fn validate(
        &self,
        living: &LivingWorld,
        nodes: &BTreeMap<String, ResourceNode>,
    ) -> Result<(), SandboxError> {
        self.config.validate()?;
        if self.animals.len() > self.config.max_animals
            || self.habitats.len() > self.config.max_habitats
            || self.resources.len() > self.config.max_resources
            || self.seeded_regions.len() > MAX_ECOLOGY_REGIONS
        {
            return Err(SandboxError::Capacity);
        }
        if !finite_range(self.elapsed_seconds, 0., 1e12)
            || !finite_range(
                self.update_accumulator_seconds,
                0.,
                self.config.update_interval_seconds,
            )
            || self.seeded_regions.iter().any(|id| !valid_id(id))
        {
            return Err(SandboxError::Invalid("ecology clock or seed ledger".into()));
        }
        for (id, h) in &self.habitats {
            valid_habitat(h)?;
            if id != &h.id {
                return Err(SandboxError::Invalid("habitat key".into()));
            }
            let definition = self
                .config
                .species
                .get(&h.species)
                .ok_or_else(|| SandboxError::Invalid("habitat species".into()))?;
            if h.kind != definition.habitat
                || h.carrying_capacity != definition.group_size
                || h.radius_m != definition.roaming_radius_m
                || h.recovery_accumulator_seconds > definition.recovery_seconds
            {
                return Err(SandboxError::Invalid(
                    "saved habitat differs from ecology policy; migrate explicitly".into(),
                ));
            }
            if self
                .animals
                .values()
                .filter(|a| a.habitat_id == *id)
                .count()
                > usize::from(h.carrying_capacity)
            {
                return Err(SandboxError::Capacity);
            }
        }
        for (id, a) in &self.animals {
            valid_animal(a)?;
            let habitat = self
                .habitats
                .get(&a.habitat_id)
                .ok_or_else(|| SandboxError::Invalid("animal habitat reference".into()))?;
            if *id != a.unit_id
                || habitat.species != a.species
                || living
                    .units
                    .get(id)
                    .is_none_or(|u| u.kind != ActorKind::Animal || u.role != UnitRole::Animal)
            {
                return Err(SandboxError::Invalid("animal body reference".into()));
            }
        }
        for (id, r) in &self.resources {
            valid_resource(r)?;
            let node = nodes
                .get(id)
                .ok_or_else(|| SandboxError::Invalid("ecology resource node reference".into()))?;
            let definition = self
                .config
                .resources
                .get(&r.kind)
                .ok_or_else(|| SandboxError::Invalid("resource definition reference".into()))?;
            if id != &r.node_id
                || node.item != r.kind.item()
                || node.capacity != definition.capacity
                || node.regrow_seconds != definition.regrow_seconds
                || r.habitat_id
                    .as_ref()
                    .is_some_and(|id| !self.habitats.contains_key(id))
            {
                return Err(SandboxError::Invalid(
                    "saved natural resource definition or habitat".into(),
                ));
            }
        }
        Ok(())
    }
}

impl EcologyView {
    /// Deliberately checks local fields only. The containing snapshot has its
    /// own living/resource bounds; absence outside an AOI is not corruption.
    pub fn validate(&self) -> Result<(), SandboxError> {
        if self.animals.len() > MAX_ECOLOGY_ANIMALS
            || self.habitats.len() > MAX_ECOLOGY_HABITATS
            || self.resources.len() > 64
        {
            return Err(SandboxError::Capacity);
        }
        let mut animals = BTreeSet::new();
        let mut habitats = BTreeSet::new();
        let mut resources = BTreeSet::new();
        for a in &self.animals {
            valid_animal(a)?;
            if !animals.insert(a.unit_id) {
                return Err(SandboxError::Invalid("duplicate wildlife metadata".into()));
            }
        }
        for h in &self.habitats {
            valid_habitat(h)?;
            if !habitats.insert(&h.id) {
                return Err(SandboxError::Invalid("duplicate habitat".into()));
            }
        }
        for r in &self.resources {
            valid_resource(r)?;
            if !resources.insert(&r.node_id) {
                return Err(SandboxError::Invalid(
                    "duplicate natural resource metadata".into(),
                ));
            }
        }
        Ok(())
    }

    /// Keep metadata coherent after the enclosing view's wire-budget reductions.
    pub fn retain_visible(&mut self, living: &LivingView, nodes: &[ResourceNode]) {
        let before = (
            self.animals.len(),
            self.resources.len(),
            self.habitats.len(),
        );
        let bodies: BTreeSet<_> = living.units.iter().map(|u| u.id).collect();
        let nodes: BTreeSet<_> = nodes.iter().map(|n| n.id.as_str()).collect();
        self.animals.retain(|a| bodies.contains(&a.unit_id));
        self.resources
            .retain(|r| nodes.contains(r.node_id.as_str()));
        let habitats: BTreeSet<_> = self
            .animals
            .iter()
            .map(|a| a.habitat_id.as_str())
            .chain(
                self.resources
                    .iter()
                    .filter_map(|r| r.habitat_id.as_deref()),
            )
            .collect();
        self.habitats.retain(|h| habitats.contains(h.id.as_str()));
        self.truncated |= before
            != (
                self.animals.len(),
                self.resources.len(),
                self.habitats.len(),
            );
    }
}

fn valid_habitat(h: &WildlifeHabitat) -> Result<(), SandboxError> {
    if !valid_id(&h.id)
        || !position_valid(h.center)
        || !finite_range(h.radius_m, 10., 300.)
        || h.carrying_capacity == 0
        || h.carrying_capacity > 16
        || h.water_access
            .is_some_and(|p| !position_valid(p) || p.surface_distance_m(h.center) > 5000.)
        || !finite_range(h.recovery_accumulator_seconds, 0., 31_536_000.)
    {
        return Err(SandboxError::Invalid("wildlife habitat".into()));
    }
    Ok(())
}
fn valid_animal(a: &WildlifeAnimal) -> Result<(), SandboxError> {
    if a.unit_id < 1_000_000
        || a.unit_id == u64::MAX
        || !valid_id(&a.habitat_id)
        || !finite_range(a.age_seconds, 0., 1e12)
        || !finite_range(a.decision_remaining_seconds, 0., 120.)
    {
        return Err(SandboxError::Invalid("wildlife animal".into()));
    }
    Ok(())
}
fn valid_resource(r: &NaturalResource) -> Result<(), SandboxError> {
    if !valid_id(&r.node_id) || r.habitat_id.as_ref().is_some_and(|id| !valid_id(id)) {
        return Err(SandboxError::Invalid("natural resource metadata".into()));
    }
    Ok(())
}
