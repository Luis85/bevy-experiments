use super::*;

impl SandboxState {
    /// Registers authored resources and habitat definitions, never actor bodies.
    /// Every position is sampled from this chunk. No public map is interpreted
    /// as a wildlife census or geological survey.
    pub fn register_ecology_chunk(
        &mut self,
        chunk: &WorldChunk,
        catalog: &Catalog,
    ) -> Result<(), SandboxError> {
        self.ecology.config.validate()?;
        chunk.validate()?;
        if !self.ecology.config.enabled {
            return Ok(());
        }
        let region = format!(
            "ecology:{}:{}:{}",
            chunk.key.level, chunk.key.x, chunk.key.y
        );
        if self.ecology.seeded_regions.contains(&region)
            || self.ecology.seeded_regions.len() >= MAX_ECOLOGY_REGIONS
        {
            return Ok(());
        }
        let before = (self.ecology.habitats.len(), self.ecology.resources.len());
        let features = chunk.features.iter().take(4096);
        let woodland = features.clone().find(|f| f.kind == FeatureKind::Forest);
        let meadow = features
            .clone()
            .find(|f| matches!(f.kind, FeatureKind::Grass | FeatureKind::Farm));
        let water = features
            .clone()
            .find(|f| matches!(f.kind, FeatureKind::Water | FeatureKind::Waterway));
        let center = chunk.key.center();
        let center = surface(chunk, center, true);
        let relief = chunk
            .heights
            .min_max()
            .map(|(lo, hi)| hi - lo)
            .unwrap_or(0.);
        let woodland_point = woodland
            .and_then(|f| feature_center(&f.points))
            .and_then(|p| surface(chunk, p, true));
        let meadow_point = meadow
            .and_then(|f| feature_center(&f.points))
            .and_then(|p| surface(chunk, p, true))
            .or(center);
        let shore = water.and_then(|f| f.points.first()).and_then(|p| {
            // Find land at the water edge rather than putting ground bodies in
            // the lake. Gathering can still use the normal 20 m spatial reach.
            [
                (0., 0.),
                (8., 0.),
                (-8., 0.),
                (0., 8.),
                (0., -8.),
                (14., 14.),
                (-14., -14.),
            ]
            .into_iter()
            .find_map(|(e, n)| offset(*p, e, n).and_then(|p| surface(chunk, p, true)))
        });
        let habitat_limit = self.ecology.config.max_habitats.saturating_sub(5);
        let resource_limit = self.ecology.config.max_resources.saturating_sub(12);
        if let Some(p) = woodland_point {
            let species = if stable_hash(&region).is_multiple_of(3) {
                AnimalSpecies::Boar
            } else {
                AnimalSpecies::Deer
            };
            let id = format!("{region}:woodland");
            self.add_ecology_habitat(id.clone(), species, p, shore, habitat_limit);
            for (kind, east, north) in [
                (NaturalResourceKind::Berries, 6., 4.),
                (NaturalResourceKind::Timber, -8., 5.),
            ] {
                if let Some(p) = offset(p, east, north).and_then(|p| surface(chunk, p, true)) {
                    self.add_natural_resource(
                        format!("{region}:{}", kind.item()),
                        kind,
                        p,
                        Some(id.clone()),
                        resource_limit,
                        catalog,
                    );
                }
            }
        }
        if let Some(p) = meadow_point {
            let id = format!("{region}:meadow");
            self.add_ecology_habitat(id.clone(), AnimalSpecies::Hare, p, shore, habitat_limit);
            self.add_natural_resource(
                format!("{region}:fiber"),
                NaturalResourceKind::Fiber,
                p,
                Some(id),
                resource_limit,
                catalog,
            );
        }
        if let Some(p) = center {
            self.add_natural_resource(
                format!("{region}:stone"),
                NaturalResourceKind::Stone,
                p,
                None,
                resource_limit,
                catalog,
            );
            if relief > 18. {
                if let Some(ore) = offset(p, 12., 9.).and_then(|p| surface(chunk, p, true)) {
                    let id = format!("{region}:highland");
                    self.add_ecology_habitat(
                        id.clone(),
                        AnimalSpecies::Ibex,
                        ore,
                        None,
                        habitat_limit,
                    );
                    self.add_natural_resource(
                        format!("{region}:iron_ore"),
                        NaturalResourceKind::IronOre,
                        ore,
                        Some(id),
                        resource_limit,
                        catalog,
                    );
                }
            }
        }
        if let Some(p) = shore {
            self.add_natural_resource(
                format!("{region}:fish"),
                NaturalResourceKind::Fish,
                p,
                None,
                resource_limit,
                catalog,
            );
            if let Some(water) = offset(p, 3., 0.).and_then(|p| surface(chunk, p, true)) {
                self.add_natural_resource(
                    format!("{region}:water"),
                    NaturalResourceKind::Water,
                    water,
                    None,
                    resource_limit,
                    catalog,
                );
            }
        }
        if before != (self.ecology.habitats.len(), self.ecology.resources.len()) {
            self.ecology.seeded_regions.insert(region);
        }
        Ok(())
    }

    pub fn seed_ecology_near(
        &mut self,
        origin: GeoPosition,
        catalog: &Catalog,
    ) -> Result<(), SandboxError> {
        self.seed_ecology_near_with_environment(origin, catalog, Some)
    }

    /// Seed a modest nearby authored ecosystem once. The host resolver must
    /// reject unloaded, water-covered or otherwise inadmissible ground.
    pub fn seed_ecology_near_with_environment(
        &mut self,
        origin: GeoPosition,
        catalog: &Catalog,
        resolve: impl Fn(GeoPosition) -> Option<GeoPosition>,
    ) -> Result<(), SandboxError> {
        self.ecology.config.validate()?;
        if !position_valid(origin) {
            return Err(SandboxError::Invalid("ecology origin".into()));
        }
        if !self.living.enabled
            || !self.ecology.config.enabled
            || self.ecology.seeded_regions.contains("ecology:starter:v1")
        {
            return Ok(());
        }
        // Prepare positions before mutating so an invalid resolver cannot leave
        // invalid partial seeds. Small radial alternatives avoid authored paths.
        let placements = [
            (AnimalSpecies::Deer, 78., 72.),
            (AnimalSpecies::Boar, -105., 66.),
            (AnimalSpecies::Hare, -62., -58.),
            (AnimalSpecies::Fox, 145., -58.),
            (AnimalSpecies::Ibex, -150., -125.),
        ];
        for (species, east, north) in placements {
            let Some(position) = safe_offset(origin, east, north, &resolve) else {
                continue;
            };
            let id = format!("ecology:starter:{}", species.label().to_ascii_lowercase());
            self.add_ecology_habitat(
                id,
                species,
                position,
                None,
                self.ecology.config.max_habitats,
            );
        }
        // Give old v0.2 animal bodies real behavior instead of leaving a second
        // decorative animal implementation running beside the ecology.
        let legacy: Vec<_> = self
            .living
            .units
            .values()
            .filter(|u| {
                u.kind == ActorKind::Animal
                    && u.owner.is_none()
                    && !self.ecology.animals.contains_key(&u.id)
            })
            .map(|u| u.id)
            .collect();
        if let Some(habitat) = self.ecology.habitats.get("ecology:starter:deer").cloned() {
            let already = self
                .ecology
                .animals
                .values()
                .filter(|a| a.habitat_id == habitat.id)
                .count();
            let capacity = usize::from(habitat.carrying_capacity).saturating_sub(already);
            for id in legacy.into_iter().take(capacity) {
                if self.ecology_animal_count() >= self.ecology.config.max_animals {
                    break;
                }
                self.ecology.animals.insert(
                    id,
                    WildlifeAnimal {
                        unit_id: id,
                        species: AnimalSpecies::Deer,
                        habitat_id: habitat.id.clone(),
                        behavior: WildlifeBehavior::Grazing,
                        age_seconds: 0.,
                        decision_remaining_seconds: 0.,
                    },
                );
                let u = self.living.units.get_mut(&id).expect("collected body");
                u.home = habitat.center;
                u.patrol.clear();
                u.path.clear();
                u.path_index = 0;
                u.order = UnitOrder::Idle;
            }
        }
        for (kind, east, north) in [
            (NaturalResourceKind::Timber, 28., -24.),
            (NaturalResourceKind::Stone, -31., -22.),
            (NaturalResourceKind::Berries, 30., 34.),
            (NaturalResourceKind::Fiber, -32., 28.),
            (NaturalResourceKind::IronOre, -83., -72.),
            (NaturalResourceKind::Water, 22., 54.),
        ] {
            if let Some(position) = safe_offset(origin, east, north, &resolve) {
                self.add_natural_resource(
                    format!("ecology:starter:{}", kind.item()),
                    kind,
                    position,
                    None,
                    self.ecology.config.max_resources,
                    catalog,
                );
            }
        }
        if let Some(water) = self
            .resource_nodes
            .get("ecology:starter:water")
            .map(|n| n.position)
        {
            for habitat in self.ecology.habitats.values_mut().filter(|h| {
                h.id.starts_with("ecology:starter:") && h.center.surface_distance_m(water) <= 500.
            }) {
                habitat.water_access = Some(water);
            }
        }
        let ids: Vec<_> = self
            .ecology
            .habitats
            .keys()
            .filter(|id| id.starts_with("ecology:starter:"))
            .cloned()
            .collect();
        for id in ids {
            let cap = self.ecology.habitats[&id].carrying_capacity;
            let present = self
                .ecology
                .animals
                .values()
                .filter(|a| a.habitat_id == id)
                .count();
            for _ in present..usize::from(cap) {
                if !self.spawn_wildlife(&id, &resolve, false) {
                    break;
                }
            }
            let present = self
                .ecology
                .animals
                .values()
                .filter(|a| a.habitat_id == id)
                .count();
            if let Some(h) = self.ecology.habitats.get_mut(&id) {
                h.initial_population_seeded = present >= usize::from(cap);
            }
        }
        let seeded_any = self
            .ecology
            .habitats
            .keys()
            .chain(self.ecology.resources.keys())
            .any(|id| id.starts_with("ecology:starter:"));
        if seeded_any && self.ecology.seeded_regions.len() < MAX_ECOLOGY_REGIONS {
            self.ecology
                .seeded_regions
                .insert("ecology:starter:v1".into());
        }
        Ok(())
    }

    fn add_ecology_habitat(
        &mut self,
        id: String,
        species: AnimalSpecies,
        center: GeoPosition,
        water_access: Option<GeoPosition>,
        limit: usize,
    ) {
        if self.ecology.habitats.contains_key(&id)
            || self.ecology_habitat_count() >= limit
            || !position_valid(center)
        {
            return;
        }
        let Some(definition) = self.ecology.config.species.get(&species) else {
            return;
        };
        let water_access =
            water_access.filter(|p| position_valid(*p) && center.surface_distance_m(*p) <= 5000.);
        self.ecology.habitats.insert(
            id.clone(),
            WildlifeHabitat {
                id,
                kind: definition.habitat,
                species,
                center,
                radius_m: definition.roaming_radius_m,
                carrying_capacity: definition.group_size,
                water_access,
                recovery_accumulator_seconds: 0.,
                initial_population_seeded: false,
                authored: true,
            },
        );
    }

    fn add_natural_resource(
        &mut self,
        id: String,
        kind: NaturalResourceKind,
        position: GeoPosition,
        habitat_id: Option<String>,
        limit: usize,
        catalog: &Catalog,
    ) {
        if self.ecology.resources.contains_key(&id)
            || self.resource_nodes.contains_key(&id)
            || self.ecology_resource_count() >= limit
            || self.resource_node_count() >= MAX_ENTITIES
            || !catalog.items.contains(kind.item())
            || !position_valid(position)
        {
            return;
        }
        // Do not create a deposit that its catalog can never harvest.
        if kind.required_tool().is_some_and(|tool| {
            !catalog.items.contains(tool)
                && !(tool == "tool" && catalog.items.contains("iron_tool"))
        }) {
            return;
        }
        let Some(definition) = self.ecology.config.resources.get(&kind) else {
            return;
        };
        let habitat_id = habitat_id.filter(|id| self.ecology.habitats.contains_key(id));
        self.resource_nodes.insert(
            id.clone(),
            ResourceNode {
                id: id.clone(),
                position,
                item: kind.item().into(),
                remaining: definition.capacity,
                capacity: definition.capacity,
                regrow_seconds: definition.regrow_seconds,
                regrow_accumulator: 0.,
                source_feature: None,
            },
        );
        self.ecology.resources.insert(
            id.clone(),
            NaturalResource {
                node_id: id,
                kind,
                habitat_id,
                authored: true,
            },
        );
    }

    pub(super) fn spawn_wildlife(
        &mut self,
        habitat_id: &str,
        resolve: &impl Fn(GeoPosition) -> Option<GeoPosition>,
        recovery: bool,
    ) -> bool {
        // Keep headroom for eight new five-body player accounts. The host may
        // configure a smaller ecological cap independently of total actor cap.
        if self.ecology_animal_count() >= self.ecology.config.max_animals
            || self.living_unit_count() >= MAX_LIVING_UNITS.saturating_sub(40)
            || self.living.next_id >= u64::MAX - 1
        {
            return false;
        }
        let Some(habitat) = self.ecology.habitats.get(habitat_id).cloned() else {
            return false;
        };
        let present = self
            .ecology
            .animals
            .values()
            .filter(|a| a.habitat_id == habitat_id)
            .count();
        if present >= usize::from(habitat.carrying_capacity) {
            return false;
        }
        let seed = stable_hash(habitat_id).wrapping_add(self.living.next_id.wrapping_mul(37));
        let angle = (seed % 6283) as f64 / 1000.;
        let radius = 5. + (present as f64 * 2.5).min(habitat.radius_m * 0.3);
        let Some(position) = safe_offset(
            habitat.center,
            angle.cos() * radius,
            angle.sin() * radius,
            resolve,
        ) else {
            return false;
        };
        let id = self.living.next_id;
        if self.living.units.contains_key(&id)
            || self.living.producers.contains_key(&id)
            || self.living.shops.contains_key(&id)
        {
            return false;
        }
        let unit = LivingUnit {
            id,
            name: format!("{} {}", habitat.species.label(), present + 1),
            kind: ActorKind::Animal,
            owner: None,
            role: UnitRole::Animal,
            position,
            heading_deg: (seed % 360) as f32,
            velocity_enu_m_s: [0.; 3],
            needs: Needs::default(),
            order: UnitOrder::Idle,
            home: habitat.center,
            path: vec![],
            path_index: 0,
            patrol: vec![],
            patrol_reverse: false,
            pocket_money: 0,
            supplies: Inventory::new(),
        };
        self.living.units.insert(id, unit);
        self.living.next_id += 1;
        self.ecology.animals.insert(
            id,
            WildlifeAnimal {
                unit_id: id,
                species: habitat.species,
                habitat_id: habitat_id.into(),
                behavior: WildlifeBehavior::Grazing,
                age_seconds: 0.,
                decision_remaining_seconds: 0.,
            },
        );
        self.ecology.metrics.animals_spawned =
            self.ecology.metrics.animals_spawned.saturating_add(1);
        if recovery {
            self.ecology.metrics.population_recoveries =
                self.ecology.metrics.population_recoveries.saturating_add(1);
        }
        true
    }
}

fn feature_center(points: &[GeoPosition]) -> Option<GeoPosition> {
    let origin = *points.first()?;
    let frame = LocalFrame::new(origin);
    let mut sum = [0.; 3];
    let mut n = 0.;
    for p in points.iter().take(128) {
        let v = frame.to_local(*p);
        for i in 0..3 {
            sum[i] += v[i];
        }
        n += 1.;
    }
    frame.to_geo(sum.map(|v| v / n)).ok()
}

fn surface(chunk: &WorldChunk, mut point: GeoPosition, land: bool) -> Option<GeoPosition> {
    point.altitude_m = f64::from(chunk.height_at(point)?) + 0.06;
    if !position_valid(point) {
        return None;
    }
    if chunk.features.iter().take(4096).any(|feature| {
        (feature.kind == FeatureKind::Building || (land && feature.kind == FeatureKind::Water))
            && inside_polygon(point, &feature.points)
    }) {
        return None;
    }
    Some(point)
}

fn inside_polygon(point: GeoPosition, points: &[GeoPosition]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let frame = LocalFrame::new(point);
    let mut inside = false;
    let end = points.len().min(512);
    let mut j = end - 1;
    for i in 0..end {
        let a = frame.to_local(points[i]);
        let b = frame.to_local(points[j]);
        if (a[2] > 0.) != (b[2] > 0.) && 0. < (b[0] - a[0]) * (-a[2]) / (b[2] - a[2]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

pub(super) fn safe_offset(
    origin: GeoPosition,
    east: f64,
    north: f64,
    resolve: &impl Fn(GeoPosition) -> Option<GeoPosition>,
) -> Option<GeoPosition> {
    for (e, n) in [
        (east, north),
        (east + 6., north),
        (east - 6., north),
        (east, north + 6.),
        (east, north - 6.),
    ] {
        if let Some(point) = offset(origin, e, n)
            .and_then(resolve)
            .filter(|p| position_valid(*p))
        {
            return Some(point);
        }
    }
    None
}
