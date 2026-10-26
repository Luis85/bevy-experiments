use std::collections::BTreeMap;
use terra_core::*;
use terra_sandbox::*;

fn position() -> GeoPosition {
    GeoPosition::new(48.137, 11.575, 520.0).unwrap()
}
fn offset(east: f64, north: f64) -> GeoPosition {
    let mut p = LocalFrame::new(position())
        .to_geo([east, 0.0, -north])
        .unwrap();
    p.altitude_m = position().altitude_m;
    p
}
fn setup() -> (SandboxState, Catalog) {
    let mut world = SandboxState::default();
    world.add_player(1, position()).unwrap();
    (world, Catalog::builtin())
}
fn claim(world: &mut SandboxState, catalog: &Catalog) -> u64 {
    world
        .apply(
            1,
            Command::ClaimPlot {
                center: position(),
                radius_m: 30.0,
            },
            catalog,
        )
        .unwrap()
        .created_id
        .unwrap()
}
fn tree_chunk() -> WorldChunk {
    WorldChunk {
        key: TileKey::at(position(), 15).unwrap(),
        heights: HeightField::constant(520.0),
        features: vec![WorldFeature {
            id: "osm:node:10".into(),
            kind: FeatureKind::Tree,
            points: vec![position()],
            height_m: 8.0,
            properties: BTreeMap::new(),
        }],
        sources: vec![],
        warnings: vec![],
    }
}

#[test]
fn builtin_catalog_roundtrips_and_rejects_unknown_materials() {
    let catalog = Catalog::builtin();
    catalog.validate().unwrap();
    Catalog::from_json(&serde_json::to_string(&catalog).unwrap()).unwrap();
    let mut bad = catalog;
    bad.recipes
        .get_mut("planks")
        .unwrap()
        .inputs
        .insert("magic".into(), 1);
    assert!(bad.validate().is_err());
}

#[test]
fn custom_catalog_registration_checks_equipment_before_registering_player() {
    let catalog = Catalog {
        items: ["fiber".to_string()].into_iter().collect(),
        recipes: BTreeMap::new(),
        blueprints: BTreeMap::new(),
        crops: BTreeMap::new(),
    };
    let mut state = SandboxState::new();
    let before = state.clone();
    assert!(
        state
            .add_player_with_inventory(
                1,
                position(),
                BTreeMap::from([("wood".into(), 5)]),
                &catalog
            )
            .is_err()
    );
    assert_eq!(state, before);
    state
        .add_player_with_inventory(
            1,
            position(),
            BTreeMap::from([("fiber".into(), 5)]),
            &catalog,
        )
        .unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    assert_eq!(state.players[&1].inventory.len(), 1);
    assert_eq!(state.players[&1].inventory["fiber"], 5);
}

#[test]
fn catalog_aware_restore_rejects_changed_blueprints_and_crop_yields() {
    let (mut world, catalog) = setup();
    let plot_id = claim(&mut world, &catalog);
    world
        .apply(
            1,
            Command::Craft {
                recipe: "planks".into(),
                count: 1,
            },
            &catalog,
        )
        .unwrap();
    let structure_id = world
        .apply(
            1,
            Command::Build {
                blueprint: "fence".into(),
                position: offset(5.0, 0.0),
            },
            &catalog,
        )
        .unwrap()
        .created_id
        .unwrap();
    let farm_id = world
        .apply(
            1,
            Command::Plant {
                plot_id,
                position: position(),
                crop: "wheat".into(),
            },
            &catalog,
        )
        .unwrap()
        .created_id
        .unwrap();
    world.validate_with_catalog(&catalog).unwrap();
    let path = std::env::temp_dir().join(format!(
        "terra-catalog-save-test-{}.json",
        std::process::id()
    ));
    world.save(&path).unwrap();
    assert_eq!(
        SandboxState::load_with_catalog(&path, &catalog).unwrap(),
        world
    );

    let mut changed_catalog = catalog.clone();
    changed_catalog.blueprints.get_mut("fence").unwrap().housing = true;
    assert!(SandboxState::load_with_catalog(&path, &changed_catalog).is_err());
    let original = world.clone();
    world
        .structures
        .get_mut(&structure_id)
        .unwrap()
        .max_hitpoints += 1;
    assert!(world.validate_with_catalog(&catalog).is_err());
    world = original.clone();
    world.farms.get_mut(&farm_id).unwrap().harvest_amount += 1;
    assert!(world.validate_with_catalog(&catalog).is_err());
    world = original;
    world.farms.get_mut(&farm_id).unwrap().harvest_item = "stone".into();
    assert!(world.validate_with_catalog(&catalog).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn crafting_failure_is_atomic_and_checked_against_overflow() {
    let (mut world, catalog) = setup();
    let before = world.clone();
    assert!(
        world
            .apply(
                1,
                Command::Craft {
                    recipe: "planks".into(),
                    count: 20
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(world, before);
    assert!(
        world
            .apply(
                1,
                Command::Craft {
                    recipe: "planks".into(),
                    count: u32::MAX
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(world, before);
    world
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("planks".into(), MAX_ITEM_COUNT);
    let before = world.clone();
    assert!(
        world
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
    assert_eq!(world, before);
}

#[test]
fn house_build_home_and_demolition_have_no_refund_duplication() {
    let (mut world, catalog) = setup();
    claim(&mut world, &catalog);
    world
        .apply(
            1,
            Command::Craft {
                recipe: "planks".into(),
                count: 3,
            },
            &catalog,
        )
        .unwrap();
    world
        .apply(
            1,
            Command::Craft {
                recipe: "stone_blocks".into(),
                count: 4,
            },
            &catalog,
        )
        .unwrap();
    let house = world
        .apply(
            1,
            Command::Build {
                blueprint: "house".into(),
                position: offset(8.0, 0.0),
            },
            &catalog,
        )
        .unwrap()
        .created_id
        .unwrap();
    world
        .apply(
            1,
            Command::SetHome {
                structure_id: house,
            },
            &catalog,
        )
        .unwrap();
    assert_eq!(world.players[&1].home, Some(house));
    let inventory = world.players[&1].inventory.clone();
    for _ in 0..8 {
        world
            .apply(
                1,
                Command::Damage {
                    structure_id: house,
                    amount: 25,
                },
                &catalog,
            )
            .unwrap();
    }
    assert!(!world.structures.contains_key(&house));
    assert_eq!(world.players[&1].home, None);
    assert_eq!(world.players[&1].inventory, inventory);
    let before = world.clone();
    assert!(
        world
            .apply(
                1,
                Command::Damage {
                    structure_id: house,
                    amount: 25
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(world, before);
    world.validate().unwrap();
}

#[test]
fn ownership_and_three_dimensional_reach_prevent_remote_actions() {
    let (mut world, catalog) = setup();
    world.add_player(2, position()).unwrap();
    claim(&mut world, &catalog);
    let before = world.clone();
    assert!(matches!(
        world.apply(
            2,
            Command::Build {
                blueprint: "workbench".into(),
                position: position()
            },
            &catalog
        ),
        Err(SandboxError::NotOwner)
    ));
    assert_eq!(world, before);
    let sky = GeoPosition {
        altitude_m: 1_000.0,
        ..position()
    };
    assert!(matches!(
        world.apply(
            1,
            Command::Build {
                blueprint: "house".into(),
                position: sky
            },
            &catalog
        ),
        Err(SandboxError::OutOfReach)
    ));
    assert_eq!(world, before);
    assert!(
        world
            .apply(
                1,
                Command::ClaimPlot {
                    center: position(),
                    radius_m: f64::NAN
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn tool_requires_owned_nearby_workbench() {
    let (mut world, catalog) = setup();
    claim(&mut world, &catalog);
    world
        .apply(
            1,
            Command::Craft {
                recipe: "planks".into(),
                count: 2,
            },
            &catalog,
        )
        .unwrap();
    let before = world.clone();
    assert!(matches!(
        world.apply(
            1,
            Command::Craft {
                recipe: "tool".into(),
                count: 1
            },
            &catalog
        ),
        Err(SandboxError::WorkbenchRequired)
    ));
    assert_eq!(world, before);
    world
        .apply(
            1,
            Command::Build {
                blueprint: "workbench".into(),
                position: offset(4.0, 0.0),
            },
            &catalog,
        )
        .unwrap();
    world
        .apply(
            1,
            Command::Craft {
                recipe: "tool".into(),
                count: 1,
            },
            &catalog,
        )
        .unwrap();
    assert_eq!(world.players[&1].inventory["tool"], 1);
}

#[test]
fn resource_counts_survive_reloading_and_regrow_without_tick_loops() {
    let (mut world, catalog) = setup();
    let chunk = tree_chunk();
    world.register_chunk(&chunk);
    world
        .apply(
            1,
            Command::Gather {
                node_id: "osm:node:10".into(),
                amount: 15,
            },
            &catalog,
        )
        .unwrap();
    world.register_chunk(&chunk);
    assert_eq!(world.resource_nodes["osm:node:10"].remaining, 5);
    world.advance(60.0, &WeatherSample::default()).unwrap();
    assert_eq!(world.resource_nodes["osm:node:10"].remaining, 7);
    world.advance(86_400.0, &WeatherSample::default()).unwrap();
    assert_eq!(world.resource_nodes["osm:node:10"].remaining, 20);
    let before = world.clone();
    assert!(
        world
            .apply(
                1,
                Command::Gather {
                    node_id: "osm:node:10".into(),
                    amount: 21
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(world, before);
    assert!(
        world
            .advance(f64::INFINITY, &WeatherSample::default())
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn source_removal_is_overlay_only_and_does_not_resurrect() {
    let (mut world, catalog) = setup();
    claim(&mut world, &catalog);
    let chunk = tree_chunk();
    world.register_chunk(&chunk);
    world
        .apply(
            1,
            Command::RemoveFeature {
                feature_id: "osm:node:10".into(),
            },
            &catalog,
        )
        .unwrap();
    world.register_chunk(&chunk);
    assert_eq!(chunk.features.len(), 1);
    assert!(world.feature_edits.contains("osm:node:10"));
    assert!(!world.resource_nodes.contains_key("osm:node:10"));
    world.validate().unwrap();
}

#[test]
fn owning_a_clipped_source_way_does_not_allow_global_geometry_removal() {
    let (mut world, catalog) = setup();
    claim(&mut world, &catalog);
    let mut chunk = tree_chunk();
    chunk.features[0].id = "osm:way:20".into();
    chunk.features[0].kind = FeatureKind::Building;
    chunk.features[0].points = vec![position(), offset(2.0, 0.0), offset(1.0, 2.0)];
    world.register_chunk(&chunk);
    let before = world.clone();
    assert!(
        world
            .apply(
                1,
                Command::RemoveFeature {
                    feature_id: "osm:way:20".into()
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(world, before);
    world.feature_edits.insert("osm:way:20".into());
    assert!(world.validate().is_err());
}

#[test]
fn farming_requires_ownership_readiness_and_single_harvest() {
    let (mut world, catalog) = setup();
    let plot_id = claim(&mut world, &catalog);
    let farm_id = world
        .apply(
            1,
            Command::Plant {
                plot_id,
                position: position(),
                crop: "wheat".into(),
            },
            &catalog,
        )
        .unwrap()
        .created_id
        .unwrap();
    assert_eq!(world.players[&1].inventory["wheat_seed"], 7);
    let before = world.clone();
    assert!(matches!(
        world.apply(1, Command::Harvest { farm_id }, &catalog),
        Err(SandboxError::NotReady)
    ));
    assert_eq!(world, before);
    world
        .advance(
            1_000.0,
            &WeatherSample {
                temperature_c: 20.0,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(world.farms[&farm_id].ready());
    world
        .apply(1, Command::Harvest { farm_id }, &catalog)
        .unwrap();
    assert_eq!(world.players[&1].inventory["wheat"], 4);
    let before = world.clone();
    assert!(
        world
            .apply(1, Command::Harvest { farm_id }, &catalog)
            .is_err()
    );
    assert_eq!(world, before);
    world
        .apply(
            1,
            Command::Craft {
                recipe: "wheat_seed".into(),
                count: 1,
            },
            &catalog,
        )
        .unwrap();
    assert_eq!(world.players[&1].inventory["wheat_seed"], 10);
    world.validate().unwrap();
}

#[test]
fn persistence_roundtrip_rejects_version_and_broken_ownership() {
    let (mut world, catalog) = setup();
    claim(&mut world, &catalog);
    let path = std::env::temp_dir().join(format!("terra-save-test-{}.json", std::process::id()));
    world.save(&path).unwrap();
    assert_eq!(SandboxState::load(&path).unwrap(), world);
    world.version += 1;
    assert!(world.save(&path).is_err());
    assert_eq!(SandboxState::load(&path).unwrap().version, SAVE_VERSION);
    world.version = SAVE_VERSION;
    world.plots.values_mut().next().unwrap().owner = 999;
    assert!(world.validate().is_err());
    std::fs::write(&path, serde_json::to_string(&world).unwrap()).unwrap();
    assert!(SandboxState::load(&path).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn snapshots_are_interest_filtered_and_hard_bounded() {
    let (mut world, _) = setup();
    for i in 0..130 {
        world
            .seed_resource(format!("node:{i}"), position(), "stone", 20, 0.0)
            .unwrap();
    }
    world
        .seed_resource("far".into(), offset(10_000.0, 0.0), "stone", 20, 0.0)
        .unwrap();
    let view = world.snapshot_for(1, 50.0).unwrap();
    assert_eq!(view.resource_nodes.len(), 64);
    assert!(view.truncated);
    assert!(view.resource_nodes.iter().all(|n| n.id != "far"));
    assert!(serde_json::to_vec(&view).unwrap().len() < 256 * 1024);
}
