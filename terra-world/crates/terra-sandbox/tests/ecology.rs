use std::collections::BTreeMap;
use terra_core::{
    ActorKind, FeatureKind, GeoPosition, HeightField, LocalFrame, TileKey, WeatherSample,
    WorldChunk, WorldFeature,
};
use terra_sandbox::*;

fn world() -> (SandboxState, Catalog, GeoPosition) {
    let catalog = Catalog::builtin();
    let mut world = SandboxState::new();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    world.add_player(1, origin).unwrap();
    world.seed_living_demo(origin, &[], &catalog).unwrap();
    (world, catalog, origin)
}

fn chunk(origin: GeoPosition) -> WorldChunk {
    let key = TileKey::at(origin, 15).unwrap();
    let bounds = key.bounds();
    let polygon = |u: f64, v: f64| {
        vec![
            bounds.point(u, v, 0.).unwrap(),
            bounds.point(u + 0.12, v, 0.).unwrap(),
            bounds.point(u + 0.12, v + 0.12, 0.).unwrap(),
            bounds.point(u, v + 0.12, 0.).unwrap(),
        ]
    };
    WorldChunk {
        key,
        heights: HeightField {
            width: 2,
            height: 2,
            samples_m: vec![100., 125., 115., 150.],
        },
        features: vec![
            WorldFeature {
                id: "ecology-test:forest".into(),
                kind: FeatureKind::Forest,
                points: polygon(0.25, 0.25),
                height_m: 12.,
                properties: BTreeMap::new(),
            },
            WorldFeature {
                id: "ecology-test:meadow".into(),
                kind: FeatureKind::Grass,
                points: polygon(0.6, 0.2),
                height_m: 0.,
                properties: BTreeMap::new(),
            },
            WorldFeature {
                id: "ecology-test:lake".into(),
                kind: FeatureKind::Water,
                points: polygon(0.6, 0.6),
                height_m: 0.,
                properties: BTreeMap::new(),
            },
        ],
        sources: vec![],
        warnings: vec![],
    }
}

#[test]
fn nearby_seed_creates_distinct_bounded_species_and_natural_nodes() {
    let (mut w, c, o) = world();
    w.seed_ecology_near(o, &c).unwrap();
    assert_eq!(w.ecology.habitats.len(), 5);
    for species in AnimalSpecies::ALL {
        assert!(w.ecology.animals.values().any(|a| a.species == species));
    }
    assert_eq!(w.ecology.animals.len(), 22);
    assert_eq!(w.ecology.resources.len(), 6);
    assert!(
        w.ecology
            .animals
            .keys()
            .all(|id| w.living.units[id].kind == ActorKind::Animal)
    );
    assert!(
        w.ecology
            .resources
            .keys()
            .all(|id| w.resource_nodes[id].source_feature.is_none())
    );
    let before = w.clone();
    w.seed_ecology_near(o, &c).unwrap();
    assert_eq!(w, before);
    w.validate_with_catalog(&c).unwrap();
}

#[test]
fn chunk_resource_elevation_is_sampled_and_reloads_do_not_refill() {
    let (mut w, c, o) = world();
    let chunk = chunk(o);
    w.register_chunk(&chunk);
    w.register_ecology_chunk(&chunk, &c).unwrap();
    assert!(!w.ecology.resources.is_empty());
    assert!(w.ecology.animals.is_empty());
    for id in w.ecology.resources.keys() {
        let n = &w.resource_nodes[id];
        assert!(
            (n.position.altitude_m - f64::from(chunk.height_at(n.position).unwrap()) - 0.06).abs()
                < 0.001
        );
    }
    let id = w
        .ecology
        .resources
        .values()
        .find(|r| r.kind == NaturalResourceKind::Berries)
        .unwrap()
        .node_id
        .clone();
    w.resource_nodes.get_mut(&id).unwrap().remaining = 0;
    let before = w.clone();
    w.register_ecology_chunk(&chunk, &c).unwrap();
    assert_eq!(w, before);
    let saved = serde_json::to_vec(&w).unwrap();
    let mut restored: SandboxState = serde_json::from_slice(&saved).unwrap();
    restored.register_ecology_chunk(&chunk, &c).unwrap();
    assert_eq!(restored.resource_nodes[&id].remaining, 0);
    restored.validate_with_catalog(&c).unwrap();
}

#[test]
fn gathered_regrowing_and_finite_nodes_keep_distinct_lifecycle() {
    let (mut w, c, o) = world();
    w.seed_ecology_near(o, &c).unwrap();
    for id in ["ecology:starter:berries", "ecology:starter:stone"] {
        let n = w.resource_nodes[id].clone();
        w.set_player_position(1, n.position).unwrap();
        while w.resource_nodes[id].remaining > 0 {
            let amount = w.resource_nodes[id].remaining.min(100);
            w.apply(
                1,
                Command::Gather {
                    node_id: id.into(),
                    amount,
                },
                &c,
            )
            .unwrap();
        }
        assert_eq!(w.resource_nodes[id].remaining, 0);
    }
    w.advance(180., &WeatherSample::default()).unwrap();
    assert_eq!(w.resource_nodes["ecology:starter:berries"].remaining, 2);
    assert_eq!(w.resource_nodes["ecology:starter:stone"].remaining, 0);
    w.seed_ecology_near(o, &c).unwrap();
    assert_eq!(w.resource_nodes["ecology:starter:stone"].remaining, 0);
}

#[test]
fn tool_admission_is_atomic_and_improved_tool_is_accepted() {
    let (mut w, c, o) = world();
    w.seed_ecology_near(o, &c).unwrap();
    let id = "ecology:starter:iron_ore";
    w.set_player_position(1, w.resource_nodes[id].position)
        .unwrap();
    let before = w.clone();
    assert!(
        w.apply(
            1,
            Command::Gather {
                node_id: id.into(),
                amount: 3
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
    w.players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("iron_tool".into(), 1);
    w.apply(
        1,
        Command::Gather {
            node_id: id.into(),
            amount: 3,
        },
        &c,
    )
    .unwrap();
    assert_eq!(w.players[&1].inventory["iron_ore"], 3);
}

#[test]
fn people_trigger_flee_and_undisturbed_wildlife_resume_normal_behavior() {
    let (mut w, c, o) = world();
    w.seed_ecology_near(o, &c).unwrap();
    let id = w
        .ecology
        .animals
        .values()
        .find(|a| a.species == AnimalSpecies::Ibex)
        .unwrap()
        .unit_id;
    let far = LocalFrame::new(o).to_geo([600., 0., 0.]).unwrap();
    for u in w
        .living
        .units
        .values_mut()
        .filter(|u| u.kind != ActorKind::Animal)
    {
        u.position = far;
    }
    w.set_player_position(1, far).unwrap();
    let p = w.living.units[&id].position;
    w.set_player_position(1, LocalFrame::new(p).to_geo([3., 0., 0.]).unwrap())
        .unwrap();
    w.advance_ecology(1., 60., 12.).unwrap();
    assert_eq!(w.ecology.animals[&id].behavior, WildlifeBehavior::Fleeing);
    assert!(matches!(w.living.units[&id].order, UnitOrder::Move { .. }));
    assert!(w.ecology.metrics.flee_events > 0);
    // Every human/vehicle body moves away from the chosen herd so only the
    // ecological decision, not a second nearby NPC, determines recovery.
    let far = LocalFrame::new(p).to_geo([600., 0., 0.]).unwrap();
    for u in w
        .living
        .units
        .values_mut()
        .filter(|u| u.kind != ActorKind::Animal)
    {
        u.position = far;
    }
    w.set_player_position(1, far).unwrap();
    for _ in 0..8 {
        w.advance_ecology(1., 60., 12.).unwrap();
    }
    assert_ne!(w.ecology.animals[&id].behavior, WildlifeBehavior::Fleeing);
}

#[test]
fn inactive_hours_rest_and_grazing_replenishes_needs() {
    let (mut w, c, o) = world();
    w.seed_ecology_near(o, &c).unwrap();
    let far = LocalFrame::new(o).to_geo([600., 0., 0.]).unwrap();
    for u in w
        .living
        .units
        .values_mut()
        .filter(|u| u.kind != ActorKind::Animal)
    {
        u.position = far;
    }
    w.set_player_position(1, far).unwrap();
    let id = w
        .ecology
        .animals
        .values()
        .find(|a| a.species == AnimalSpecies::Ibex)
        .unwrap()
        .unit_id;
    w.advance_ecology(1., 60., 1.).unwrap();
    assert_eq!(w.ecology.animals[&id].behavior, WildlifeBehavior::Resting);
    w.living.units.get_mut(&id).unwrap().needs.food = 0.2;
    w.ecology
        .animals
        .get_mut(&id)
        .unwrap()
        .decision_remaining_seconds = 0.;
    w.advance_ecology(1., 60., 12.).unwrap();
    assert_eq!(w.ecology.animals[&id].behavior, WildlifeBehavior::Grazing);
    w.advance_ecology(1., 60., 12.).unwrap();
    assert!(w.living.units[&id].needs.food > 0.2);
}

#[test]
fn recovery_is_delayed_and_never_exceeds_carrying_capacity() {
    let (mut w, c, o) = world();
    w.seed_ecology_near(o, &c).unwrap();
    let h = "ecology:starter:hare";
    let id = w
        .ecology
        .animals
        .values()
        .find(|a| a.habitat_id == h)
        .unwrap()
        .unit_id;
    let original = w.ecology.animals.len();
    w.living.units.remove(&id);
    w.advance_ecology(1., 1., 12.).unwrap();
    assert_eq!(w.ecology.animals.len(), original - 1);
    // One large elapsed-calendar update can recover at most one body per
    // habitat; it cannot simulate thousands of births in a blocking loop.
    w.advance_ecology(1., 86_400., 12.).unwrap();
    assert_eq!(w.ecology.animals.len(), original);
    assert_eq!(w.ecology.metrics.population_recoveries, 1);
    for _ in 0..30 {
        w.advance_ecology(1., 86_400., 12.).unwrap();
    }
    assert_eq!(w.ecology.animals.len(), original);
    w.validate_with_catalog(&c).unwrap();
}

#[test]
fn thirsty_animals_seek_accessible_water_before_hydrating() {
    let (mut w, c, o) = world();
    w.seed_ecology_near(o, &c).unwrap();
    let id = w
        .ecology
        .animals
        .values()
        .find(|a| a.species == AnimalSpecies::Deer)
        .unwrap()
        .unit_id;
    let far = LocalFrame::new(o).to_geo([600., 0., 0.]).unwrap();
    for u in w
        .living
        .units
        .values_mut()
        .filter(|u| u.kind != ActorKind::Animal)
    {
        u.position = far;
    }
    w.set_player_position(1, far).unwrap();
    let water = w.ecology.habitats[&w.ecology.animals[&id].habitat_id]
        .water_access
        .unwrap();
    w.living.units.get_mut(&id).unwrap().needs.water = 0.2;
    w.advance_ecology(1., 60., 12.).unwrap();
    assert_eq!(w.ecology.animals[&id].behavior, WildlifeBehavior::Drinking);
    let en_route = w.living.units[&id].needs.water;
    w.advance_ecology(1., 60., 12.).unwrap();
    assert_eq!(w.living.units[&id].needs.water, en_route);
    w.living.units.get_mut(&id).unwrap().position = water;
    w.advance_ecology(1., 60., 12.).unwrap();
    assert!(w.living.units[&id].needs.water > en_route);
}

#[test]
fn absent_terrain_blocks_new_bodies_and_does_not_commit_empty_starter_seed() {
    let (mut w, c, o) = world();
    let before = w.clone();
    w.seed_ecology_near_with_environment(o, &c, |_| None)
        .unwrap();
    assert_eq!(w, before);
    w.seed_ecology_near_with_environment(o, &c, Some).unwrap();
    assert!(!w.ecology.animals.is_empty());
    let id = *w.ecology.animals.keys().next().unwrap();
    w.living.units.remove(&id);
    w.advance_ecology_with_environment(1., 86_400., 12., |_| None)
        .unwrap();
    assert!(!w.ecology.animals.contains_key(&id));
    assert_eq!(w.ecology.metrics.population_recoveries, 0);
}

#[test]
fn malformed_metadata_and_configuration_reject_without_mutation() {
    let (mut w, c, o) = world();
    w.seed_ecology_near(o, &c).unwrap();
    let before = w.clone();
    assert!(w.advance_ecology(f64::NAN, 1., 12.).is_err());
    assert_eq!(w, before);
    let id = *w.ecology.animals.keys().next().unwrap();
    w.ecology.animals.get_mut(&id).unwrap().habitat_id = "missing".into();
    assert!(w.validate().is_err());
    let (mut w, c, o) = world();
    w.ecology.config.max_animals = MAX_ECOLOGY_ANIMALS + 1;
    let before = w.clone();
    assert!(w.seed_ecology_near(o, &c).is_err());
    assert_eq!(w, before);
}

#[test]
fn custom_catalog_skips_unknown_and_unharvestable_resource_types() {
    let (mut w, mut c, o) = world();
    // This policy is inspected at seeding; the existing demo economy is left
    // alone because it was intentionally created with the built-in catalog.
    for item in ["berries", "fiber", "fishing_rod", "tool", "iron_tool"] {
        c.items.remove(item);
    }
    w.seed_ecology_near(o, &c).unwrap();
    assert!(!w.ecology.resources.values().any(|r| matches!(
        r.kind,
        NaturalResourceKind::Berries
            | NaturalResourceKind::Fiber
            | NaturalResourceKind::IronOre
            | NaturalResourceKind::Fish
    )));
    assert!(
        w.ecology
            .resources
            .values()
            .any(|r| r.kind == NaturalResourceKind::Stone)
    );
}

#[test]
fn shoreline_and_ore_nodes_require_available_catalog_tools_and_enforce_gathering() {
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let source = chunk(origin);
    let builtin = Catalog::builtin();
    let mut complete = SandboxState::new();
    complete
        .add_player_with_inventory(1, origin, BTreeMap::new(), &builtin)
        .unwrap();
    complete.register_ecology_chunk(&source, &builtin).unwrap();
    let fish = complete
        .ecology
        .resources
        .values()
        .find(|r| r.kind == NaturalResourceKind::Fish)
        .expect("the water fixture must actually provide a usable shoreline")
        .node_id
        .clone();
    complete
        .set_player_position(1, complete.resource_nodes[&fish].position)
        .unwrap();
    let before = complete.clone();
    let gather = Command::Gather {
        node_id: fish.clone(),
        amount: 1,
    };
    assert!(complete.apply(1, gather.clone(), &builtin).is_err());
    assert_eq!(
        complete, before,
        "missing rod cannot consume a fish or mutate inventory"
    );
    complete
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("fishing_rod".into(), 1);
    complete.apply(1, gather, &builtin).unwrap();
    assert_eq!(complete.players[&1].inventory["fish"], 1);

    let mut iron_only = builtin.clone();
    for item in ["fishing_rod", "tool"] {
        iron_only.items.remove(item);
        iron_only.recipes.remove(item);
    }
    iron_only.validate().unwrap();
    let mut custom = SandboxState::new();
    custom
        .add_player_with_inventory(
            1,
            origin,
            BTreeMap::from([("iron_tool".into(), 1)]),
            &iron_only,
        )
        .unwrap();
    custom.register_ecology_chunk(&source, &iron_only).unwrap();
    assert!(iron_only.items.contains("fish"));
    assert!(
        !custom
            .ecology
            .resources
            .values()
            .any(|r| r.kind == NaturalResourceKind::Fish)
    );
    let ore = custom
        .ecology
        .resources
        .values()
        .find(|r| r.kind == NaturalResourceKind::IronOre)
        .expect("iron tools alone must permit a harvestable deposit")
        .node_id
        .clone();
    custom
        .set_player_position(1, custom.resource_nodes[&ore].position)
        .unwrap();
    custom
        .apply(
            1,
            Command::Gather {
                node_id: ore,
                amount: 3,
            },
            &iron_only,
        )
        .unwrap();
    assert_eq!(custom.players[&1].inventory["iron_ore"], 3);
    custom.validate_with_catalog(&iron_only).unwrap();

    iron_only.items.remove("iron_tool");
    iron_only.recipes.remove("iron_tool");
    iron_only.validate().unwrap();
    let mut tool_less = SandboxState::new();
    tool_less
        .register_ecology_chunk(&source, &iron_only)
        .unwrap();
    assert!(iron_only.items.contains("iron_ore"));
    assert!(!tool_less.ecology.resources.values().any(|r| matches!(
        r.kind,
        NaturalResourceKind::Fish | NaturalResourceKind::IronOre
    )));
    tool_less.validate_with_catalog(&iron_only).unwrap();
}

#[test]
fn views_only_carry_visible_metadata_and_survive_budget_trimming() {
    let (mut w, c, o) = world();
    w.seed_ecology_near(o, &c).unwrap();
    let living = w.living.snapshot(1, o, 500.);
    let nodes: Vec<_> = w.resource_nodes.values().take(4).cloned().collect();
    let mut view = w.ecology.snapshot(&living, &nodes);
    view.validate().unwrap();
    assert!(
        view.animals
            .iter()
            .all(|a| living.units.iter().any(|u| u.id == a.unit_id))
    );
    assert!(
        view.resources
            .iter()
            .all(|r| nodes.iter().any(|n| n.id == r.node_id))
    );
    let mut reduced = living;
    reduced.units.clear();
    view.retain_visible(&reduced, &[]);
    view.validate().unwrap();
    assert!(view.truncated);
    assert!(view.animals.is_empty());
    assert!(view.resources.is_empty());
    assert!(view.habitats.is_empty());
}

#[test]
fn v2_save_migrates_empty_ecology_and_updated_catalog_example_matches() {
    let (w, c, _) = world();
    let mut json = serde_json::to_value(&w).unwrap();
    json.as_object_mut().unwrap().remove("ecology");
    let migrated: SandboxState = serde_json::from_value(json).unwrap();
    assert!(migrated.ecology.animals.is_empty());
    migrated.validate_with_catalog(&c).unwrap();
    let example = Catalog::from_json(include_str!("../catalog.example.json")).unwrap();
    assert_eq!(
        serde_json::to_value(example).unwrap(),
        serde_json::to_value(c).unwrap()
    );
}
