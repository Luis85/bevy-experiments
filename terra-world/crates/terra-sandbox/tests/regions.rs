use std::collections::{BTreeMap, BTreeSet};
use terra_core::{FeatureKind, GeoPosition, LocalFrame, WeatherSample};
use terra_sandbox::*;

fn origin() -> GeoPosition {
    GeoPosition::new(48.137, 11.575, 100.).unwrap()
}

fn world() -> (SandboxState, Catalog) {
    let catalog = Catalog::builtin();
    let mut state = SandboxState::default();
    state.add_player(1, origin()).unwrap();
    state.seed_living_demo(origin(), &[], &catalog).unwrap();
    (state, catalog)
}

fn distant() -> GeoPosition {
    LocalFrame::new(origin()).to_geo([20_000., 0., 0.]).unwrap()
}

fn key(position: GeoPosition) -> RegionKey {
    RegionKey::at(position, 15).unwrap()
}

fn requested(position: GeoPosition) -> BTreeSet<RegionKey> {
    BTreeSet::from([key(position)])
}

fn add_construction(state: &mut SandboxState, catalog: &Catalog) {
    state.plots.insert(
        1,
        LandPlot {
            id: 1,
            owner: 1,
            faction_owner: None,
            center: origin(),
            radius_m: 50.,
        },
    );
    for (id, name, offset) in [(2, "house", 0.), (3, "workbench", 20.)] {
        let blueprint = &catalog.blueprints[name];
        state.structures.insert(
            id,
            Structure {
                id,
                owner: 1,
                faction_owner: None,
                blueprint: name.into(),
                position: LocalFrame::new(origin()).to_geo([offset, 0., 0.]).unwrap(),
                hitpoints: blueprint.hitpoints,
                max_hitpoints: blueprint.hitpoints,
                radius_m: blueprint.radius_m,
                housing: blueprint.housing,
                workbench: blueprint.workbench,
            },
        );
    }
    state.players.get_mut(&1).unwrap().home = Some(2);
    let crop = &catalog.crops["wheat"];
    state.farms.insert(
        4,
        FarmPlot {
            id: 4,
            owner: 1,
            faction_owner: None,
            plot_id: 1,
            position: origin(),
            crop: "wheat".into(),
            growth_seconds: 41.,
            required_growth_seconds: crop.growth_seconds,
            moisture: 0.5,
            harvest_item: crop.harvest_item.clone(),
            harvest_amount: crop.harvest_amount,
        },
    );
    state.next_id = 5;
}

fn move_non_home_construction_away(state: &mut SandboxState) {
    state.plots.insert(
        5,
        LandPlot {
            id: 5,
            owner: 1,
            faction_owner: None,
            center: distant(),
            radius_m: 50.,
        },
    );
    state.structures.get_mut(&3).unwrap().position = distant();
    let farm = state.farms.get_mut(&4).unwrap();
    farm.position = distant();
    farm.plot_id = 5;
    state.next_id = 6;
}

fn total_money(state: &SandboxState) -> u64 {
    state.living.accounts.values().map(|a| a.money).sum::<u64>()
        + state.living.shops.values().map(|s| s.money).sum::<u64>()
        + state
            .living
            .units
            .values()
            .map(|u| u.pocket_money)
            .sum::<u64>()
        + state
            .settlements
            .territories
            .values()
            .map(|t| t.treasury)
            .sum::<u64>()
        + state
            .settlements
            .contracts
            .values()
            .map(|c| c.reward)
            .sum::<u64>()
}

#[test]
fn geographic_partition_round_trips_every_domain_without_refilling() {
    let (mut state, catalog) = world();
    state.seed_ecology_near(origin(), &catalog).unwrap();
    add_construction(&mut state, &catalog);
    state.resource_nodes.values_mut().next().unwrap().remaining = 0;
    state.advance_settlements(60., &catalog).unwrap();
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    assert!(!partition.pages.is_empty());
    assert!(partition.global.resource_nodes.is_empty());
    assert!(partition.global.farms.is_empty());
    assert!(partition.global.ecology.habitats.is_empty());
    assert_eq!(partition.assemble_all(&catalog).unwrap(), state);
    let loaded = partition
        .working_set(&partition.pages.keys().copied().collect(), &catalog)
        .unwrap();
    assert_eq!(loaded.state, state);
    assert_eq!(
        loaded.record_count(),
        partition.global_record_count() + partition.record_count()
    );
    assert_eq!(total_money(&loaded.state), total_money(&state));
}

#[test]
fn homes_ownership_accounts_and_physical_cargo_remain_valid_without_environment() {
    let (mut state, catalog) = world();
    add_construction(&mut state, &catalog);
    move_non_home_construction_away(&mut state);
    state.advance_settlements(60., &catalog).unwrap();
    assert!(!state.settlements.contracts.is_empty());
    let courier = state
        .living
        .units
        .values()
        .find(|u| u.role == UnitRole::Hauler)
        .unwrap()
        .id;
    let from = *state.living.producers.keys().next().unwrap();
    let to = *state.living.shops.keys().next().unwrap();
    state.living.units.get_mut(&courier).unwrap().supplies = BTreeMap::from([("wood".into(), 3)]);
    let id = state.living.next_id;
    state.living.next_id += 1;
    state.living.deliveries.push(Delivery {
        id,
        unit_id: courier,
        from_site: from,
        to_site: to,
        item: "wood".into(),
        amount: 3,
        phase: DeliveryPhase::ToDropoff,
        stalled_seconds: 2.,
        route_retries: 1,
        last_position: Some(origin()),
    });
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let empty = partition.working_set(&BTreeSet::new(), &catalog).unwrap();
    assert!(empty.state.farms.is_empty());
    assert!(!empty.state.structures.contains_key(&3));
    assert!(empty.state.structures.contains_key(&2));
    assert_eq!(empty.state.plots, state.plots);
    assert_eq!(empty.state.players, state.players);
    assert_eq!(empty.state.living.accounts, state.living.accounts);
    assert_eq!(empty.state.living.deliveries, state.living.deliveries);
    assert_eq!(empty.state.living.units[&courier].supplies["wood"], 3);
    assert_eq!(
        empty.state.settlements.contracts,
        state.settlements.contracts
    );
    empty.state.validate_with_catalog(&catalog).unwrap();
    let merged = partition
        .merge_active(&empty.state, &empty.active_regions, &catalog)
        .unwrap();
    assert_eq!(merged.assemble_all(&catalog).unwrap(), state);
}

#[test]
fn cross_region_resource_references_load_as_one_dependency_closed_set() {
    let (mut state, catalog) = (SandboxState::default(), Catalog::builtin());
    state
        .seed_resource("remote-tree".into(), distant(), "wood", 20, 30.)
        .unwrap();
    state
        .resource_nodes
        .get_mut("remote-tree")
        .unwrap()
        .source_feature = Some("tree".into());
    state.world_features.insert(
        "tree".into(),
        FeatureReference {
            position: origin(),
            radius_m: 1.,
            kind: FeatureKind::Tree,
        },
    );
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let loaded = partition
        .working_set(&requested(distant()), &catalog)
        .unwrap();
    assert!(loaded.dependency_regions.contains(&key(origin())));
    assert!(loaded.state.world_features.contains_key("tree"));
    assert!(loaded.state.resource_nodes.contains_key("remote-tree"));
    let reverse = partition
        .working_set(&requested(origin()), &catalog)
        .unwrap();
    assert!(reverse.active_regions.contains(&key(distant())));
    reverse.state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn wildlife_crossing_region_boundary_retains_habitat_and_resource_dependencies() {
    let (mut state, catalog) = world();
    state.seed_ecology_near(origin(), &catalog).unwrap();
    let animal = *state.ecology.animals.keys().next().unwrap();
    state.living.units.get_mut(&animal).unwrap().position = distant();
    let habitat = state.ecology.animals[&animal].habitat_id.clone();
    let habitat_key = key(state.ecology.habitats[&habitat].center);
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let loaded = partition
        .working_set(&requested(distant()), &catalog)
        .unwrap();
    assert!(loaded.active_regions.contains(&habitat_key));
    assert!(loaded.state.ecology.habitats.contains_key(&habitat));
    assert!(loaded.state.ecology.animals.contains_key(&animal));
    loaded.state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn owned_wildlife_pins_its_habitat_and_closes_dependencies() {
    let (mut state, catalog) = world();
    state.seed_ecology_near(origin(), &catalog).unwrap();
    let animal = *state.ecology.animals.keys().next().unwrap();
    state.living.units.get_mut(&animal).unwrap().owner = Some(1);
    let habitat = state.ecology.animals[&animal].habitat_id.clone();
    let definition = state.ecology.habitats.get_mut(&habitat).unwrap();
    definition.center = distant();
    definition.water_access = None;
    let habitat_key = key(state.ecology.habitats[&habitat].center);
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let loaded = partition.working_set(&BTreeSet::new(), &catalog).unwrap();
    assert!(partition.global.living.units.contains_key(&animal));
    assert!(loaded.pinned_regions.contains(&habitat_key));
    assert!(loaded.state.ecology.habitats.contains_key(&habitat));
    loaded.state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn empty_requested_regions_are_owned_for_later_seed_and_deletion_persists() {
    let (state, catalog) = world();
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut loaded = partition
        .working_set(&requested(distant()), &catalog)
        .unwrap();
    assert!(loaded.active_regions.contains(&key(distant())));
    loaded
        .state
        .seed_resource("new-ore".into(), distant(), "iron_ore", 72, 0.)
        .unwrap();
    let seeded = partition
        .merge_active(&loaded.state, &loaded.active_regions, &catalog)
        .unwrap();
    let mut loaded = seeded.working_set(&requested(distant()), &catalog).unwrap();
    loaded.state.resource_nodes.remove("new-ore");
    let removed = seeded
        .merge_active(&loaded.state, &loaded.active_regions, &catalog)
        .unwrap();
    assert!(
        !removed
            .assemble_all(&catalog)
            .unwrap()
            .resource_nodes
            .contains_key("new-ore")
    );
}

#[test]
fn migrating_actor_merges_with_existing_cold_region_without_duplication() {
    let (mut state, catalog) = world();
    state
        .living
        .units
        .retain(|_, u| u.owner.is_some() || u.role == UnitRole::Hauler);
    let mut prototype = state
        .living
        .units
        .values()
        .find(|u| u.role == UnitRole::Player)
        .unwrap()
        .clone();
    prototype.owner = None;
    prototype.role = UnitRole::Citizen;
    prototype.pocket_money = 1234;
    let moving = state.living.next_id;
    for (offset, position) in [(0, origin()), (1, distant())] {
        let mut unit = prototype.clone();
        unit.id = moving + offset;
        unit.position = position;
        unit.home = position;
        state.living.units.insert(unit.id, unit);
    }
    state.living.next_id += 2;
    let before_count = state.living.units.len();
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut loaded = partition
        .working_set(&requested(origin()), &catalog)
        .unwrap();
    assert!(!loaded.state.living.units.contains_key(&(moving + 1)));
    loaded.state.living.units.get_mut(&moving).unwrap().position = distant();
    let moved = partition
        .merge_active(&loaded.state, &loaded.active_regions, &catalog)
        .unwrap();
    let complete = moved.assemble_all(&catalog).unwrap();
    assert_eq!(complete.living.units.len(), before_count);
    assert_eq!(total_money(&complete), total_money(&state));
    assert_eq!(complete.living.units[&moving].position, distant());
    assert_eq!(
        complete.living.units[&(moving + 1)],
        state.living.units[&(moving + 1)]
    );
    assert!(moved.pages[&key(distant())].units.contains_key(&moving));
    assert!(
        moved.pages[&key(distant())]
            .units
            .contains_key(&(moving + 1))
    );
    assert!(
        moved
            .pages
            .get(&key(origin()))
            .is_none_or(|p| !p.units.contains_key(&moving))
    );
}

#[test]
fn dormant_environment_freezes_while_global_clocks_and_owned_needs_advance() {
    let (mut state, catalog) = world();
    state.seed_ecology_near(origin(), &catalog).unwrap();
    add_construction(&mut state, &catalog);
    move_non_home_construction_away(&mut state);
    let shift = |p| LocalFrame::new(p).to_geo([20_000., 0., 0.]).unwrap();
    for node in state.resource_nodes.values_mut() {
        node.position = shift(node.position);
    }
    for habitat in state.ecology.habitats.values_mut() {
        habitat.center = shift(habitat.center);
        habitat.water_access = habitat.water_access.map(shift);
    }
    for unit in state
        .living
        .units
        .values_mut()
        .filter(|u| u.owner.is_none() && u.role != UnitRole::Hauler)
    {
        unit.position = shift(unit.position);
        unit.home = shift(unit.home);
        unit.path = unit.path.iter().copied().map(shift).collect();
        unit.patrol = unit.patrol.iter().copied().map(shift).collect();
        if let UnitOrder::Move { destination } = &mut unit.order {
            *destination = shift(*destination);
        }
    }
    for node in state.resource_nodes.values_mut() {
        node.remaining = 0;
    }
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut loaded = partition.working_set(&BTreeSet::new(), &catalog).unwrap();
    let avatar = state.living.accounts[&1].avatar_unit;
    for _ in 0..4 {
        loaded
            .state
            .advance(86_400., &WeatherSample::default())
            .unwrap();
        loaded.state.advance_living(0., 86_400., &catalog).unwrap();
        loaded.state.advance_ecology(0., 86_400., 12.).unwrap();
        loaded.state.advance_settlements(86_400., &catalog).unwrap();
    }
    let result = partition
        .merge_active(&loaded.state, &loaded.active_regions, &catalog)
        .unwrap();
    let complete = result.assemble_all(&catalog).unwrap();
    assert_eq!(complete.resource_nodes, state.resource_nodes);
    assert_eq!(complete.farms, state.farms);
    assert_eq!(complete.ecology.habitats, state.ecology.habitats);
    assert_eq!(complete.ecology.animals, state.ecology.animals);
    for id in state.ecology.animals.keys() {
        assert_eq!(complete.living.units[id], state.living.units[id]);
    }
    assert!(complete.tick_seconds > state.tick_seconds);
    assert!(complete.living.units[&avatar].needs.food < state.living.units[&avatar].needs.food);
    assert_eq!(result.pages, partition.pages);
}

#[test]
fn forged_dependency_graph_and_misplaced_or_duplicate_records_are_rejected() {
    let (mut state, catalog) = (SandboxState::default(), Catalog::builtin());
    state
        .seed_resource("node".into(), distant(), "wood", 20, 30.)
        .unwrap();
    state.resource_nodes.get_mut("node").unwrap().source_feature = Some("tree".into());
    state.world_features.insert(
        "tree".into(),
        FeatureReference {
            position: origin(),
            radius_m: 1.,
            kind: FeatureKind::Tree,
        },
    );
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut forged = partition.clone();
    forged
        .pages
        .get_mut(&key(distant()))
        .unwrap()
        .dependencies
        .clear();
    assert!(forged.assemble_all(&catalog).is_err());
    assert!(forged.working_set(&requested(distant()), &catalog).is_err());
    let mut misplaced = partition.clone();
    let node = misplaced
        .pages
        .get_mut(&key(distant()))
        .unwrap()
        .resource_nodes
        .remove("node")
        .unwrap();
    misplaced
        .pages
        .get_mut(&key(origin()))
        .unwrap()
        .resource_nodes
        .insert("node".into(), node);
    assert!(misplaced.assemble_all(&catalog).is_err());
    let mut duplicate = partition.clone();
    duplicate
        .global
        .resource_nodes
        .insert("node".into(), state.resource_nodes["node"].clone());
    assert!(duplicate.assemble_all(&catalog).is_err());
}

#[test]
fn cold_record_conflict_fails_atomically_and_invalid_interest_is_rejected() {
    let (mut state, catalog) = world();
    state
        .seed_resource("ore".into(), distant(), "iron_ore", 72, 0.)
        .unwrap();
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut loaded = partition
        .working_set(&requested(origin()), &catalog)
        .unwrap();
    loaded
        .state
        .seed_resource("ore".into(), origin(), "iron_ore", 72, 0.)
        .unwrap();
    let before = partition.clone();
    assert!(
        partition
            .merge_active(&loaded.state, &loaded.active_regions, &catalog)
            .is_err()
    );
    assert_eq!(partition, before);
    assert!(
        partition
            .working_set(
                &BTreeSet::from([RegionKey {
                    level: 15,
                    x: u32::MAX,
                    y: 0
                }]),
                &catalog
            )
            .is_err()
    );
    assert!(
        partition
            .working_set(
                &BTreeSet::from([RegionKey::at(origin(), 14).unwrap()]),
                &catalog
            )
            .is_err()
    );
}

#[test]
fn poles_and_date_line_round_trip_without_region_aliasing() {
    let catalog = Catalog::builtin();
    let mut state = SandboxState::default();
    for (id, latitude, longitude) in [
        ("east", 0., 179.999),
        ("west", 0., -179.999),
        ("north", 90., 0.),
        ("south", -90., 0.),
    ] {
        state
            .seed_resource(
                id.into(),
                GeoPosition::new(latitude, longitude, 0.).unwrap(),
                "stone",
                10,
                0.,
            )
            .unwrap();
    }
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    assert_eq!(partition.pages.len(), 4);
    assert_eq!(partition.assemble_all(&catalog).unwrap(), state);
    for (id, node) in &state.resource_nodes {
        let loaded = partition
            .working_set(&requested(node.position), &catalog)
            .unwrap();
        assert_eq!(loaded.state.resource_nodes.len(), 1);
        assert_eq!(&loaded.state.resource_nodes[id], node);
    }
}

#[test]
fn partial_save_is_rejected_and_serialization_preserves_the_marker() {
    let (mut state, catalog) = world();
    state
        .seed_resource("cold".into(), distant(), "stone", 40, 0.)
        .unwrap();
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let loaded = partition.working_set(&BTreeSet::new(), &catalog).unwrap();
    assert!(loaded.state.is_partial());
    assert_eq!(loaded.state.dormant_reservations().resource_nodes, 1);
    let restored: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&loaded.state).unwrap()).unwrap();
    assert!(restored.is_partial());
    assert!(restored.require_complete().is_err());
    assert!(RegionPartition::from_state(&restored, 15, &catalog).is_err());
    let path = std::env::temp_dir().join(format!("terra-partial-save-{}.json", std::process::id()));
    assert!(restored.save(&path).is_err());
    assert!(!path.exists());
    let complete = partition
        .merge_active(&restored, &loaded.active_regions, &catalog)
        .unwrap()
        .assemble_all(&catalog)
        .unwrap();
    assert!(!complete.is_partial());
    assert_eq!(complete, state);
}

#[test]
fn cold_resources_reserve_total_capacity_before_new_seed_mutates_state() {
    let catalog = Catalog::builtin();
    let mut state = SandboxState::default();
    for i in 0..MAX_ENTITIES {
        state
            .seed_resource(format!("cold:{i}"), distant(), "stone", 10, 0.)
            .unwrap();
    }
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut loaded = partition
        .working_set(&requested(origin()), &catalog)
        .unwrap();
    assert!(loaded.state.resource_nodes.is_empty());
    assert_eq!(loaded.state.resource_node_count(), MAX_ENTITIES);
    let before = loaded.state.clone();
    assert!(matches!(
        loaded
            .state
            .seed_resource("new".into(), origin(), "stone", 10, 0.),
        Err(SandboxError::Capacity)
    ));
    assert_eq!(loaded.state, before);
    assert_eq!(
        partition
            .merge_active(&loaded.state, &loaded.active_regions, &catalog)
            .unwrap()
            .assemble_all(&catalog)
            .unwrap(),
        state
    );
}

#[test]
fn cold_animal_population_reserves_capacity_during_passive_recovery() {
    let (mut state, catalog) = world();
    state.ecology.config.max_animals = 2;
    let definition = &state.ecology.config.species[&AnimalSpecies::Deer];
    for (name, center) in [("near", origin()), ("far", distant())] {
        state.ecology.habitats.insert(
            name.into(),
            WildlifeHabitat {
                id: name.into(),
                kind: definition.habitat,
                species: AnimalSpecies::Deer,
                center,
                radius_m: definition.roaming_radius_m,
                carrying_capacity: definition.group_size,
                water_access: None,
                recovery_accumulator_seconds: 0.,
                initial_population_seeded: false,
                authored: true,
            },
        );
    }
    let prototype = state.living.units.values().next().unwrap().clone();
    for _ in 0..2 {
        let id = state.living.next_id;
        state.living.next_id += 1;
        let mut body = prototype.clone();
        body.id = id;
        body.owner = None;
        body.role = UnitRole::Animal;
        body.kind = terra_core::ActorKind::Animal;
        body.position = distant();
        body.home = distant();
        body.order = UnitOrder::Idle;
        state.living.units.insert(id, body);
        state.ecology.animals.insert(
            id,
            WildlifeAnimal {
                unit_id: id,
                species: AnimalSpecies::Deer,
                habitat_id: "far".into(),
                behavior: WildlifeBehavior::Grazing,
                age_seconds: 0.,
                decision_remaining_seconds: 0.,
            },
        );
    }
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut loaded = partition
        .working_set(&requested(origin()), &catalog)
        .unwrap();
    assert_eq!(loaded.state.ecology.animals.len(), 0);
    assert_eq!(loaded.state.ecology_animal_count(), 2);
    loaded.state.advance_ecology(1., 1., 12.).unwrap();
    assert_eq!(loaded.state.ecology.animals.len(), 0);
    let complete = partition
        .merge_active(&loaded.state, &loaded.active_regions, &catalog)
        .unwrap()
        .assemble_all(&catalog)
        .unwrap();
    assert_eq!(complete.ecology.animals.len(), 2);
}

#[test]
fn retained_bodies_pin_nearby_environment_when_camera_interest_moves_elsewhere() {
    let (mut state, catalog) = world();
    let neighbor = key(origin())
        .neighbors(1)
        .into_iter()
        .find(|k| *k != key(origin()))
        .unwrap();
    state
        .seed_resource("near".into(), neighbor.center(), "stone", 10, 0.)
        .unwrap();
    state
        .seed_resource("cold".into(), distant(), "stone", 10, 0.)
        .unwrap();
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let camera = LocalFrame::new(origin())
        .to_geo([-20_000., 0., 0.])
        .unwrap();
    let loaded = partition.working_set(&requested(camera), &catalog).unwrap();
    assert!(loaded.pinned_regions.contains(&neighbor));
    assert!(loaded.state.resource_nodes.contains_key("near"));
    assert!(!loaded.state.resource_nodes.contains_key("cold"));
    assert_eq!(loaded.state.living.accounts, state.living.accounts);
}

#[test]
fn polar_metric_pins_keep_physically_overlapping_buildings_at_different_longitudes() {
    let catalog = Catalog::builtin();
    let mut state = SandboxState::new();
    let player = GeoPosition::new(90., 0., 100.).unwrap();
    let other_longitude = GeoPosition::new(90., 100., 100.).unwrap();
    state.add_player(1, player).unwrap();
    state.plots.insert(
        1,
        LandPlot {
            id: 1,
            owner: 1,
            faction_owner: None,
            center: other_longitude,
            radius_m: 50.,
        },
    );
    let definition = &catalog.blueprints["house"];
    state.structures.insert(
        2,
        Structure {
            id: 2,
            owner: 1,
            faction_owner: None,
            blueprint: "house".into(),
            position: other_longitude,
            hitpoints: definition.hitpoints,
            max_hitpoints: definition.hitpoints,
            radius_m: definition.radius_m,
            housing: definition.housing,
            workbench: definition.workbench,
        },
    );
    state.next_id = 3;
    state
        .seed_resource("polar-water".into(), other_longitude, "water", 20, 30.)
        .unwrap();
    let far = GeoPosition::new(80., 100., 100.).unwrap();
    state
        .seed_resource("cold".into(), far, "stone", 20, 0.)
        .unwrap();
    let player_key = RegionKey::at(player, 20).unwrap();
    let object_key = RegionKey::at(other_longitude, 20).unwrap();
    assert!(!player_key.neighbors(1).contains(&object_key));
    assert!(player.surface_distance_m(other_longitude) < 0.001);
    let partition = RegionPartition::from_state(&state, 20, &catalog).unwrap();
    let loaded = partition.working_set(&BTreeSet::new(), &catalog).unwrap();
    assert!(loaded.pinned_regions.contains(&object_key));
    assert!(loaded.state.structures.contains_key(&2));
    assert!(loaded.state.resource_nodes.contains_key("polar-water"));
    assert!(!loaded.state.resource_nodes.contains_key("cold"));
    loaded.state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn metric_pins_cover_nearby_records_beyond_one_ring_at_high_latitude_and_dateline() {
    for (latitude, longitude, object_longitude) in [(89.999, 0., 20.), (0., 179.9999, -179.9993)] {
        let catalog = Catalog::builtin();
        let mut state = SandboxState::new();
        let player = GeoPosition::new(latitude, longitude, 100.).unwrap();
        let object = GeoPosition::new(latitude, object_longitude, 100.).unwrap();
        state.add_player(1, player).unwrap();
        state
            .seed_resource("near".into(), object, "stone", 20, 0.)
            .unwrap();
        let player_key = RegionKey::at(player, 20).unwrap();
        let object_key = RegionKey::at(object, 20).unwrap();
        assert!(!player_key.neighbors(1).contains(&object_key));
        assert!(player.surface_distance_m(object) < REGION_SAFETY_RADIUS_M);
        let partition = RegionPartition::from_state(&state, 20, &catalog).unwrap();
        let loaded = partition.working_set(&BTreeSet::new(), &catalog).unwrap();
        assert!(loaded.pinned_regions.contains(&object_key));
        assert!(loaded.state.resource_nodes.contains_key("near"));
    }
}

fn move_owner_and_home_to_remote(state: &mut SandboxState) {
    let target = distant();
    for unit in state
        .living
        .units
        .values_mut()
        .filter(|u| u.owner == Some(1))
    {
        unit.position = target;
        unit.home = target;
    }
    state.players.get_mut(&1).unwrap().position = target;
    let old = origin();
    for structure in state.structures.values_mut() {
        let offset = LocalFrame::new(old).to_local(structure.position);
        structure.position = LocalFrame::new(target).to_geo(offset).unwrap();
    }
    for plot in state.plots.values_mut() {
        plot.center = target;
    }
    for farm in state.farms.values_mut() {
        farm.position = target;
    }
}

#[test]
fn dormant_owner_bodies_and_home_retire_without_losing_private_references() {
    let (mut state, catalog) = world();
    state.ensure_transport_demo(&catalog).unwrap();
    add_construction(&mut state, &catalog);
    move_owner_and_home_to_remote(&mut state);
    let unit_ids: Vec<_> = state
        .living
        .units
        .values()
        .filter(|u| u.owner == Some(1))
        .map(|u| u.id)
        .collect();
    state.set_player_dormant(1, true).unwrap();
    let original = state.clone();
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut active = partition.working_set(&BTreeSet::new(), &catalog).unwrap();
    assert!(active.state.is_player_dormant(1));
    assert!(!active.pinned_regions.contains(&key(distant())));
    assert!(!active.state.structures.contains_key(&2));
    assert!(
        unit_ids
            .iter()
            .all(|id| !active.state.living.units.contains_key(id))
    );
    assert_eq!(active.state.living.accounts, original.living.accounts);
    assert_eq!(active.state.transport.vehicles, original.transport.vehicles);
    assert_eq!(active.state.players, original.players);
    assert_eq!(
        active.state.living_unit_count(),
        original.living_unit_count()
    );
    active.state.validate_with_catalog(&catalog).unwrap();
    let before = active.state.clone();
    assert!(active.state.set_player_dormant(1, false).is_err());
    assert_eq!(active.state, before, "failed activation must be atomic");
    assert!(
        active
            .state
            .apply(
                1,
                Command::Craft {
                    recipe: "planks".into(),
                    count: 1
                },
                &catalog
            )
            .is_err()
    );
    let complete = partition
        .merge_active(&active.state, &active.active_regions, &catalog)
        .unwrap()
        .assemble_all(&catalog)
        .unwrap();
    assert_eq!(complete, original);
    let mut resumed = complete;
    resumed.set_player_dormant(1, false).unwrap();
    assert!(!resumed.is_player_dormant(1));
    resumed.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn offline_owned_bodies_remain_frozen_when_a_region_is_hot() {
    let (mut state, catalog) = world();
    state.ensure_transport_demo(&catalog).unwrap();
    let vehicle = state.transport.vehicles.values().next().unwrap().unit_id;
    let avatar = state.living.accounts[&1].avatar_unit;
    let destination = LocalFrame::new(origin()).to_geo([150., 0., 0.]).unwrap();
    for unit in state
        .living
        .units
        .values_mut()
        .filter(|u| u.owner == Some(1))
    {
        unit.order = UnitOrder::Move { destination };
        unit.path = vec![destination];
        unit.path_index = 0;
        unit.needs.food = 0.43;
        unit.needs.water = 0.38;
        unit.needs.energy = 0.27;
    }
    state.set_player_dormant(1, true).unwrap();
    let units: BTreeMap<_, _> = state
        .living
        .units
        .iter()
        .filter(|(_, u)| u.owner == Some(1))
        .map(|(&id, u)| (id, u.clone()))
        .collect();
    let before_vehicle = state.transport.vehicles[&vehicle].clone();
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut active = partition
        .working_set(&requested(origin()), &catalog)
        .unwrap()
        .state;
    assert!(active.living.units.contains_key(&avatar));
    for _ in 0..20 {
        active.advance_living(1., 100., &catalog).unwrap();
    }
    for (&id, unit) in &units {
        assert_eq!(&active.living.units[&id], unit);
    }
    assert_eq!(active.transport.vehicles[&vehicle], before_vehicle);
    active.validate_with_catalog(&catalog).unwrap();
    active.set_player_dormant(1, false).unwrap();
    active.advance_living(1., 1., &catalog).unwrap();
    assert!(active.living.units[&avatar].needs.food < units[&avatar].needs.food);
    assert!(
        active.living.units[&avatar].needs.food > 0.42,
        "no offline catchup"
    );
}

#[test]
fn private_cold_descriptors_cannot_bypass_complete_save_or_owner_validation() {
    let (mut state, catalog) = world();
    state.ensure_transport_demo(&catalog).unwrap();
    add_construction(&mut state, &catalog);
    move_owner_and_home_to_remote(&mut state);
    state.set_player_dormant(1, true).unwrap();
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let active = partition.working_set(&BTreeSet::new(), &catalog).unwrap();
    assert!(active.state.require_complete().is_err());
    let mut raw = serde_json::to_value(&active.state).unwrap();
    raw["dormant_players"] = serde_json::json!([]);
    let forged: SandboxState = serde_json::from_value(raw).unwrap();
    assert!(forged.validate_with_catalog(&catalog).is_err());
    let mut raw = serde_json::to_value(&active.state).unwrap();
    raw["dormant"]["homes"]["2"]["owner"] = serde_json::json!(999);
    let forged: SandboxState = serde_json::from_value(raw).unwrap();
    assert!(forged.validate_with_catalog(&catalog).is_err());
}

#[test]
fn immutable_partition_proof_matches_checked_paths_and_rejects_forged_dependencies() {
    let (mut state, catalog) = world();
    add_construction(&mut state, &catalog);
    move_non_home_construction_away(&mut state);
    let plain = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let certified = ValidatedRegionPartition::from_partition(plain.clone(), &catalog).unwrap();
    assert_eq!(certified.assemble().unwrap(), state);
    let requested = requested(origin());
    let mut working = certified.select(&requested).unwrap();
    assert_eq!(
        working.state,
        plain.working_set(&requested, &catalog).unwrap().state
    );
    working.state.advance_living(0.05, 0.5, &catalog).unwrap();
    let merged = certified
        .merge(&working.state, &working.active_regions)
        .unwrap();
    assert_eq!(
        merged.assemble().unwrap(),
        plain
            .merge_active(&working.state, &working.active_regions, &catalog)
            .unwrap()
            .assemble_all(&catalog)
            .unwrap()
    );
    let mut forged = plain;
    let keys: Vec<_> = forged.pages.keys().copied().collect();
    assert!(keys.len() > 1);
    forged
        .pages
        .get_mut(&keys[0])
        .unwrap()
        .dependencies
        .insert(keys[1]);
    assert!(ValidatedRegionPartition::from_partition(forged, &catalog).is_err());
}

#[test]
fn owned_wildlife_age_and_behavior_freeze_even_when_an_observer_loads_its_habitat() {
    let (mut state, catalog) = world();
    state.seed_ecology_near(origin(), &catalog).unwrap();
    let id = *state.ecology.animals.keys().next().unwrap();
    state.living.units.get_mut(&id).unwrap().owner = Some(1);
    state.set_player_dormant(1, true).unwrap();
    let body = state.living.units[&id].clone();
    let animal = state.ecology.animals[&id].clone();
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut active = partition
        .working_set(&requested(body.position), &catalog)
        .unwrap()
        .state;
    assert!(active.ecology.animals.contains_key(&id));
    for _ in 0..10 {
        active.advance_ecology(1., 100., 12.).unwrap();
        active.advance_living(1., 100., &catalog).unwrap();
    }
    assert_eq!(active.living.units[&id], body);
    assert_eq!(active.ecology.animals[&id], animal);
    active.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn certified_environment_merge_matches_complete_validation_across_mutation_and_residency() {
    let (mut state, catalog) = world();
    add_construction(&mut state, &catalog);
    move_non_home_construction_away(&mut state);
    for index in 0..400 {
        state
            .seed_resource(format!("certified-{index}"), distant(), "stone", 80, 0.)
            .unwrap();
    }
    // Mixed pages: the environmental proof must not accidentally certify this
    // resident's ownership, money, needs or numeric IDs as static environment.
    let citizen = state
        .living
        .units
        .values()
        .find(|unit| unit.owner.is_none() && unit.role == UnitRole::Citizen)
        .unwrap()
        .id;
    state.living.units.get_mut(&citizen).unwrap().position = distant();
    state.living.units.get_mut(&citizen).unwrap().home = distant();
    let mut proof = ValidatedRegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut selected = proof.select(&requested(origin())).unwrap();
    let mut saw_certified = false;
    for step in 0..36 {
        selected.state.tick_seconds += 0.5;
        selected.state.living.accounts.get_mut(&1).unwrap().money -= 1;
        if let Some(node) = selected.state.resource_nodes.values_mut().next() {
            node.remaining = node.remaining.saturating_sub(1);
        }
        if step % 5 == 0 {
            if let Some(unit) = selected.state.living.units.get_mut(&citizen) {
                unit.position = if step % 2 == 0 { origin() } else { distant() };
            }
        }
        if step % 7 == 0 {
            selected
                .state
                .seed_resource(format!("new-{step}"), origin(), "stone", 23, 0.)
                .unwrap();
        }
        if step % 11 == 0 {
            selected.state.resource_nodes.remove("new-0");
        }
        let complete_path = proof
            .merge_active(&selected.state, &selected.active_regions, &catalog)
            .unwrap();
        let next = proof
            .merge(&selected.state, &selected.active_regions)
            .unwrap();
        saw_certified |= next.validation_work().certified_environment_records >= 400;
        assert_eq!(
            *next, complete_path,
            "canonical partition after step {step}"
        );
        assert_eq!(
            next.assemble().unwrap(),
            complete_path.assemble_all(&catalog).unwrap()
        );
        let focus = if (step / 6) % 2 == 0 {
            origin()
        } else {
            distant()
        };
        selected = next.select(&requested(focus)).unwrap();
        proof = next;
    }
    assert!(
        saw_certified,
        "the comparison must actually exercise cold certificates"
    );
}

#[test]
fn certified_environment_rejects_duplicate_ids_without_mutating_the_proof() {
    let (mut state, catalog) = world();
    state
        .seed_resource("cold-node".into(), distant(), "stone", 31, 0.)
        .unwrap();
    let proof = ValidatedRegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut selected = proof.select(&requested(origin())).unwrap();
    assert!(!selected.state.resource_nodes.contains_key("cold-node"));
    selected
        .state
        .seed_resource("cold-node".into(), origin(), "stone", 8, 0.)
        .unwrap();
    selected.state.validate_with_catalog(&catalog).unwrap();
    assert!(
        proof
            .merge(&selected.state, &selected.active_regions)
            .is_err()
    );
    assert_eq!(proof.assemble().unwrap(), state);
}

#[test]
fn certified_environment_keeps_cold_construction_dependent_on_current_land_rights() {
    let (mut state, catalog) = world();
    add_construction(&mut state, &catalog);
    move_non_home_construction_away(&mut state);
    state
        .seed_resource("cold-node".into(), distant(), "stone", 31, 0.)
        .unwrap();
    let proof = ValidatedRegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut selected = proof.select(&requested(origin())).unwrap();
    assert!(!selected.state.structures.contains_key(&3));
    selected.state.plots.remove(&5);
    // Active-only validation cannot see the now-orphaned cold workbench/farm.
    selected.state.validate_with_catalog(&catalog).unwrap();
    assert!(
        proof
            .merge(&selected.state, &selected.active_regions)
            .is_err()
    );
    assert_eq!(proof.assemble().unwrap(), state);
}

#[test]
fn certified_environment_preserves_cross_page_source_links_and_excludes_ecology_policy() {
    let mut state = SandboxState::default();
    let catalog = Catalog::builtin();
    state
        .seed_resource("remote-tree".into(), distant(), "wood", 20, 30.)
        .unwrap();
    state
        .resource_nodes
        .get_mut("remote-tree")
        .unwrap()
        .source_feature = Some("tree".into());
    state.world_features.insert(
        "tree".into(),
        FeatureReference {
            position: origin(),
            radius_m: 1.,
            kind: FeatureKind::Tree,
        },
    );
    let proof = ValidatedRegionPartition::from_state(&state, 15, &catalog).unwrap();
    let selected = proof.select(&BTreeSet::new()).unwrap();
    let next = proof
        .merge(&selected.state, &selected.active_regions)
        .unwrap();
    assert_eq!(next.validation_work().certified_environment_records, 2);
    assert_eq!(next.validation_work().certified_environment_groups, 1);
    assert_eq!(next.assemble_all(&catalog).unwrap(), state);

    let (mut state, catalog) = world();
    state.seed_ecology_near(distant(), &catalog).unwrap();
    for id in state.ecology.animals.keys() {
        let unit = state.living.units.get_mut(id).unwrap();
        unit.position = distant();
        unit.home = distant();
    }
    let proof = ValidatedRegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut selected = proof.select(&requested(origin())).unwrap();
    assert!(selected.state.ecology.resources.is_empty());
    selected
        .state
        .ecology
        .config
        .resources
        .values_mut()
        .for_each(|definition| definition.capacity += 1);
    selected.state.validate_with_catalog(&catalog).unwrap();
    assert!(
        proof
            .merge(&selected.state, &selected.active_regions)
            .is_err(),
        "cold natural-resource policy is checked against actual records"
    );
}
