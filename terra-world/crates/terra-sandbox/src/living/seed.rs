//! Authored demo content, optionally placed using mapped semantic features.
use super::*;

impl SandboxState {
    /// Enable a small, authored living settlement. Real map tags place the
    /// facilities when suitable nearby features exist; economy and population
    /// always remain explicitly authored simulation content.
    pub fn seed_living_demo(
        &mut self,
        origin: GeoPosition,
        features: &[WorldFeature],
        catalog: &Catalog,
    ) -> Result<(), SandboxError> {
        if self.living.enabled {
            return Ok(());
        }
        origin.validate()?;
        if !finite_range(origin.altitude_m, -12_000., 100_000.) {
            return Err(SandboxError::Invalid("living origin altitude".into()));
        }
        if self.players.len().saturating_mul(STARTER_UNIT_COUNT) + 42 > MAX_LIVING_UNITS {
            return Err(SandboxError::Capacity);
        }
        if [
            "bread",
            "water",
            "flour",
            "wheat",
            "wheat_seed",
            "wood",
            "planks",
            "stone",
            "tool",
        ]
        .iter()
        .any(|id| !catalog.items.contains(*id))
        {
            return Err(SandboxError::Invalid("living demo requires bread, water, flour, wheat, wheat_seed, wood, planks, stone and tool in catalog".into()));
        }
        let mut world = LivingWorld {
            enabled: true,
            origin: Some(origin),
            ..LivingWorld::default()
        };
        let frame = LocalFrame::new(origin);
        let mut road_ids = BTreeSet::new();
        let mut road_features: Vec<_> = features
            .iter()
            .filter(|f| f.kind == FeatureKind::Road && valid_id(&f.id))
            .take(MAX_LIVING_ROADS * 8)
            .map(|feature| {
                let distance = feature
                    .points
                    .iter()
                    .take(64)
                    .filter(|p| p.validate().is_ok())
                    .map(|p| origin.surface_distance_m(*p))
                    .min_by(f64::total_cmp)
                    .unwrap_or(f64::INFINITY);
                (distance, feature)
            })
            .collect();
        // Streaming order must not select a distant corner of the neighborhood
        // while dropping the streets that serve the actual settlement.
        road_features.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
        for (_, feature) in road_features {
            let points: Vec<_> = feature
                .points
                .iter()
                .copied()
                .filter(|p| {
                    p.validate().is_ok()
                        && origin.surface_distance_m(*p) < 1500.
                        && finite_range(p.altitude_m, -12_000., 100_000.)
                })
                .take(64)
                .collect();
            if points.len() >= 2
                && points
                    .windows(2)
                    .any(|p| p[0].surface_distance_m(p[1]) > 2.)
            {
                // Reusable providers and historical packs may repeat a source
                // ID across chunks. Identical segments are one route; distinct
                // clipped segments retain stable identities independent of the
                // order in which the chunks reached this initializer.
                let id = road_segment_id(&feature.id, &points);
                if !road_ids.insert(id.clone()) {
                    continue;
                }
                world.roads.push(RoadRoute {
                    id,
                    points,
                    width_m: feature
                        .properties
                        .get("width")
                        .and_then(|x| x.parse().ok())
                        .filter(|x: &f32| x.is_finite() && *x > 0.)
                        .or_else(|| {
                            (feature.height_m.is_finite() && feature.height_m > 0.)
                                .then_some(feature.height_m)
                        })
                        .unwrap_or(6.)
                        .clamp(2., 30.),
                });
                if world.roads.len() == MAX_LIVING_ROADS {
                    break;
                }
            }
        }
        if world.roads.is_empty() {
            let coords = [
                [-120., 0., -90.],
                [0., 0., -90.],
                [120., 0., -90.],
                [120., 0., 0.],
                [120., 0., 90.],
                [0., 0., 90.],
                [-120., 0., 90.],
                [-120., 0., 0.],
                [-120., 0., -90.],
            ];
            world.roads.push(RoadRoute {
                id: "authored:settlement-loop".into(),
                points: coords
                    .into_iter()
                    .map(|p| frame.to_geo(p).unwrap())
                    .collect(),
                width_m: 6.,
            });
            world.roads.push(RoadRoute {
                id: "authored:market-lane".into(),
                points: vec![
                    frame.to_geo([-120., 0., 0.])?,
                    origin,
                    frame.to_geo([120., 0., 0.])?,
                ],
                width_m: 5.,
            });
        }
        let place = |predicate: &dyn Fn(&WorldFeature) -> bool, fallback: [f64; 3]| {
            features
                .iter()
                .filter(|f| {
                    !f.points.is_empty()
                        && predicate(f)
                        && finite_range(f.points[0].altitude_m, -12_000., 100_000.)
                })
                .filter(|f| origin.surface_distance_m(f.points[0]) < 500.)
                .min_by(|a, b| {
                    origin
                        .surface_distance_m(a.points[0])
                        .total_cmp(&origin.surface_distance_m(b.points[0]))
                })
                .map(|f| (f.points[0], Some(f.id.clone())))
                .unwrap_or((frame.to_geo(fallback).unwrap(), None))
        };
        let (market, source) = place(
            &|f| {
                f.properties
                    .get("shop")
                    .is_some_and(|s| s == "general" || s == "supermarket" || s == "convenience")
                    || f.properties
                        .get("amenity")
                        .is_some_and(|s| s == "marketplace")
            },
            [10., 0., -10.],
        );
        let prices = BTreeMap::from([
            (
                "bread".into(),
                TradePrice {
                    buy: 250,
                    sell: 140,
                },
            ),
            ("water".into(), TradePrice { buy: 80, sell: 30 }),
            ("wood".into(), TradePrice { buy: 120, sell: 70 }),
            (
                "planks".into(),
                TradePrice {
                    buy: 180,
                    sell: 100,
                },
            ),
            ("wheat".into(), TradePrice { buy: 100, sell: 60 }),
            (
                "flour".into(),
                TradePrice {
                    buy: 160,
                    sell: 100,
                },
            ),
            ("wheat_seed".into(), TradePrice { buy: 50, sell: 20 }),
            ("stone".into(), TradePrice { buy: 100, sell: 50 }),
            (
                "tool".into(),
                TradePrice {
                    buy: 1200,
                    sell: 600,
                },
            ),
        ]);
        let shop_id = world.next()?;
        world.shops.insert(
            shop_id,
            Shop {
                id: shop_id,
                name: "Village exchange".into(),
                position: market,
                stock: inventory(&[
                    ("bread", 40),
                    ("water", 80),
                    ("wood", 60),
                    ("planks", 40),
                    ("wheat", 30),
                    ("flour", 30),
                    ("wheat_seed", 80),
                    ("stone", 60),
                    ("tool", 8),
                ]),
                prices,
                money: 100_000,
                source_feature: source,
                authored: true,
            },
        );
        let specs = [
            (
                ProductionKind::Farm,
                "East field",
                [-75., 0., -55.],
                inventory(&[("water", 1), ("wheat_seed", 1)]),
                inventory(&[("wheat", 8)]),
                90.,
                inventory(&[("water", 40), ("wheat_seed", 40)]),
                None,
            ),
            (
                ProductionKind::Well,
                "Spring pump",
                [35., 0., -45.],
                Inventory::new(),
                inventory(&[("water", 4)]),
                30.,
                inventory(&[("water", 60)]),
                Some(2000),
            ),
            (
                ProductionKind::Woodlot,
                "Managed woodlot",
                [-105., 0., 45.],
                Inventory::new(),
                inventory(&[("wood", 4)]),
                60.,
                inventory(&[("wood", 40)]),
                Some(1000),
            ),
            (
                ProductionKind::Sawmill,
                "Riverside sawmill",
                [-60., 0., 45.],
                inventory(&[("wood", 2)]),
                inventory(&[("planks", 4)]),
                60.,
                inventory(&[("wood", 20)]),
                None,
            ),
            (
                ProductionKind::Mill,
                "Grain mill",
                [65., 0., -45.],
                inventory(&[("wheat", 2)]),
                inventory(&[("flour", 3)]),
                45.,
                inventory(&[("wheat", 20)]),
                None,
            ),
            (
                ProductionKind::Bakery,
                "Hearth bakery",
                [45., 0., 25.],
                inventory(&[("flour", 2), ("water", 1)]),
                inventory(&[("bread", 4)]),
                60.,
                inventory(&[("flour", 20), ("water", 20)]),
                None,
            ),
            (
                ProductionKind::SeedHouse,
                "Seed store",
                [-35., 0., -45.],
                inventory(&[("wheat", 1)]),
                inventory(&[("wheat_seed", 4)]),
                50.,
                inventory(&[("wheat", 10)]),
                None,
            ),
        ];
        for (kind, name, offset, inputs, outputs, cycle, stock, resource_remaining) in specs {
            let (position, source) = place(
                &|f| match kind {
                    ProductionKind::Farm => f.kind == FeatureKind::Farm,
                    ProductionKind::Well => f
                        .properties
                        .get("amenity")
                        .is_some_and(|s| s == "drinking_water"),
                    ProductionKind::Woodlot => f.kind == FeatureKind::Forest,
                    ProductionKind::Sawmill => f
                        .properties
                        .get("industrial")
                        .is_some_and(|s| s == "sawmill"),
                    ProductionKind::Mill => f.properties.get("craft").is_some_and(|s| s == "mill"),
                    ProductionKind::Bakery => {
                        f.properties.get("shop").is_some_and(|s| s == "bakery")
                    }
                    ProductionKind::SeedHouse => false,
                },
                offset,
            );
            let id = world.next()?;
            world.producers.insert(
                id,
                ProductionSite {
                    id,
                    name: name.into(),
                    kind,
                    position,
                    stock,
                    inputs,
                    outputs,
                    cycle_seconds: cycle,
                    progress_seconds: 0.,
                    enabled: true,
                    owner: self.players.keys().next().copied(),
                    source_feature: source,
                    authored: true,
                    resource_remaining,
                    cycles_completed: 0,
                },
            );
        }
        for n in 0..24 {
            let a = n as f64 * std::f64::consts::TAU / 24.;
            let position = frame.to_geo([a.cos() * 55., 0., a.sin() * 55.])?;
            let id = world.make_unit(
                format!("Resident {:02}", n + 1),
                ActorKind::Human,
                UnitRole::Citizen,
                None,
                position,
            )?;
            let destinations = [
                market,
                world.producers.values().nth(n % 7).unwrap().position,
                position,
            ];
            let mut route = vec![];
            let mut from = position;
            for target in destinations {
                route.extend(world.route_between(from, target));
                from = target;
            }
            let unit = world.units.get_mut(&id).unwrap();
            unit.needs.food = 0.55 + (n % 4) as f32 * 0.1;
            unit.needs.water = 0.45 + (n % 5) as f32 * 0.1;
            unit.patrol = route;
            unit.order = UnitOrder::Patrol;
        }
        let road_lengths: Vec<f64> = world
            .roads
            .iter()
            .map(|road| {
                road.points
                    .windows(2)
                    .map(|p| p[0].surface_distance_m(p[1]))
                    .sum()
            })
            .collect();
        let total_length: f64 = road_lengths.iter().sum();
        let traffic_count = ((total_length / 12.).floor() as usize).min(8);
        let mut traffic_positions: Vec<GeoPosition> = Vec::new();
        for n in 0..traffic_count {
            let mut along = total_length * (n as f64 + 0.5) / traffic_count as f64;
            let mut road_index = 0;
            while road_index + 1 < world.roads.len() && along > road_lengths[road_index] {
                along -= road_lengths[road_index];
                road_index += 1;
            }
            let road = &world.roads[road_index];
            let mut segment = 0;
            while segment + 2 < road.points.len() {
                let length = road.points[segment].surface_distance_m(road.points[segment + 1]);
                if along <= length {
                    break;
                }
                along -= length;
                segment += 1;
            }
            let a = road.points[segment];
            let b = road.points[segment + 1];
            let delta = LocalFrame::new(a).to_local(b);
            let length = a.surface_distance_m(b).max(0.001);
            let t = (along / length).clamp(0., 1.);
            let position = LocalFrame::new(a).to_geo([delta[0] * t, delta[1] * t, delta[2] * t])?;
            if traffic_positions
                .iter()
                .any(|p| p.surface_distance_m(position) < 8.)
            {
                continue;
            }
            traffic_positions.push(position);
            let route = road.points.clone();
            let id = world.make_unit(
                format!("Traffic {:02}", n + 1),
                ActorKind::Vehicle,
                UnitRole::Traffic,
                None,
                position,
            )?;
            let unit = world.units.get_mut(&id).unwrap();
            unit.patrol = route;
            unit.path_index = segment + 1;
            unit.heading_deg = delta[0].atan2(-delta[2]).to_degrees().rem_euclid(360.) as f32;
            unit.order = UnitOrder::Patrol;
        }
        for n in 0..6 {
            let position = frame.to_geo([15. + n as f64 * 4., 0., 16.])?;
            world.make_unit(
                format!("Courier {:02}", n + 1),
                ActorKind::Vehicle,
                UnitRole::Hauler,
                None,
                position,
            )?;
        }
        for n in 0..4 {
            let position = frame.to_geo([-80. + n as f64 * 8., 0., -60.])?;
            let id = world.make_unit(
                format!("Deer {}", n + 1),
                ActorKind::Animal,
                UnitRole::Animal,
                None,
                position,
            )?;
            let unit = world.units.get_mut(&id).unwrap();
            unit.patrol = vec![position, frame.to_geo([-60. + n as f64 * 7., 0., -85.])?];
            unit.order = UnitOrder::Patrol;
        }
        let mut prepared = SandboxState {
            players: self.players.clone(),
            living: world,
            ..SandboxState::default()
        };
        for id in self.players.keys().copied() {
            prepared.ensure_living_player(id)?;
        }
        prepared.living.validate(&prepared.players, Some(catalog))?;
        self.living = prepared.living;
        Ok(())
    }

    pub fn ensure_living_player(&mut self, player_id: u64) -> Result<(), SandboxError> {
        if !self.living.enabled || self.living.accounts.contains_key(&player_id) {
            return Ok(());
        }
        let position = self
            .players
            .get(&player_id)
            .ok_or(SandboxError::PlayerMissing)?
            .position;
        validate_living_position(position)?;
        if self.living.units.len().saturating_add(STARTER_UNIT_COUNT) > MAX_LIVING_UNITS
            || self
                .living
                .next_id
                .checked_add(STARTER_UNIT_COUNT as u64)
                .is_none_or(|n| n == u64::MAX)
        {
            return Err(SandboxError::Capacity);
        }
        // Prepare the complete crew before mutating the durable world. Invalid
        // coordinates or exhausted IDs must never leave a partial account.
        let frame = LocalFrame::new(position);
        let crew = [
            (
                format!("Explorer {player_id}"),
                ActorKind::Human,
                UnitRole::Player,
                position,
            ),
            (
                "Crew 1".into(),
                ActorKind::Human,
                UnitRole::Worker,
                frame.to_geo([4., 0., 3.])?,
            ),
            (
                "Crew 2".into(),
                ActorKind::Human,
                UnitRole::Worker,
                frame.to_geo([7., 0., 3.])?,
            ),
            (
                "Utility rover".into(),
                ActorKind::Vehicle,
                UnitRole::Worker,
                frame.to_geo([-6., 0., 5.])?,
            ),
            (
                "Survey helicopter".into(),
                ActorKind::Helicopter,
                UnitRole::Worker,
                frame.to_geo([10., 18., 12.])?,
            ),
        ];
        debug_assert_eq!(crew.len(), STARTER_UNIT_COUNT);
        let mut prepared = LivingWorld {
            next_id: self.living.next_id,
            ..LivingWorld::default()
        };
        for (name, kind, role, spawn) in crew {
            validate_living_position(spawn)?;
            if self.living.units.contains_key(&prepared.next_id) {
                return Err(SandboxError::Invalid(
                    "living unit ID is already allocated".into(),
                ));
            }
            prepared.make_unit(name, kind, role, Some(player_id), spawn)?;
        }
        let avatar = self.living.next_id;
        self.living.next_id = prepared.next_id;
        self.living.units.append(&mut prepared.units);
        if self.living.accounts.is_empty() {
            for site in self.living.producers.values_mut() {
                if site.owner.is_none() {
                    site.owner = Some(player_id);
                }
            }
        }
        self.living.accounts.insert(
            player_id,
            LivingAccount {
                player_id,
                money: 25_000,
                avatar_unit: avatar,
                active_unit: avatar,
                possessed: false,
            },
        );
        Ok(())
    }
}

fn road_segment_id(source: &str, points: &[GeoPosition]) -> String {
    // Fixed FNV-1a fingerprint, not DefaultHasher: route IDs must survive
    // process restarts and Rust implementation changes. Source IDs are ASCII.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in source.bytes().chain(points.iter().flat_map(|point| {
        [point.latitude, point.longitude, point.altitude_m]
            .into_iter()
            .flat_map(|value| value.to_bits().to_le_bytes())
    })) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{}:route:{hash:016x}", &source[..source.len().min(105)])
}
