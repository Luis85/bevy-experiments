//! Host integration regressions for additive frontier state, game-owned catalog
//! policy and terrain admission. These exercise public APIs and persisted files.
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};
use terra_core::{GeoPosition, LocalFrame};
use terra_net::{
    ClientMessage, FrameDecoder, PROTOCOL_VERSION, Server, ServerConfig, ServerMessage,
    WorldEnvironment, encode_frame,
};
use terra_sandbox::{AnimalSpecies, Catalog, Recipe, SandboxState};

fn server() -> Server {
    Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..ServerConfig::default()
    })
    .unwrap()
}

// A literal archived policy prevents future built-in changes from silently
// changing which catalogs this compatibility regression considers legacy.
fn original_catalog() -> Catalog {
    Catalog::from_json(
        r#"{
          "items":["wood","stone","planks","stone_blocks","tool","wheat_seed","wheat"],
          "recipes":{
            "planks":{"inputs":{"wood":2},"outputs":{"planks":4},"requires_workbench":false},
            "stone_blocks":{"inputs":{"stone":2},"outputs":{"stone_blocks":1},"requires_workbench":false},
            "tool":{"inputs":{"planks":2,"stone":2},"outputs":{"tool":1},"requires_workbench":true},
            "wheat_seed":{"inputs":{"wheat":1},"outputs":{"wheat_seed":3},"requires_workbench":false}
          },
          "blueprints":{
            "house":{"costs":{"planks":12,"stone_blocks":4},"hitpoints":200,"radius_m":4.0,"housing":true,"workbench":false},
            "workbench":{"costs":{"planks":4,"stone":2},"hitpoints":80,"radius_m":1.0,"housing":false,"workbench":true},
            "fence":{"costs":{"planks":2},"hitpoints":40,"radius_m":0.7,"housing":false,"workbench":false}
          },
          "crops":{"wheat":{"seed_item":"wheat_seed","harvest_item":"wheat","harvest_amount":4,"growth_seconds":180.0}}
        }"#,
    )
    .unwrap()
}

fn saved_value(host: &Server, path: &std::path::Path) -> serde_json::Value {
    host.save_world(path).unwrap();
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn legacy_protocols_reject_before_allocating_saved_player_or_capability() {
    assert!(
        PROTOCOL_VERSION > 2,
        "frontier wire variants require a new protocol"
    );
    for legacy_protocol in [1, 2, 7, 8, 9] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rejected-protocol.json");
        let mut host = server();
        let before = saved_value(&host, &path);
        let mut legacy = TcpStream::connect(host.local_addr().unwrap()).unwrap();
        legacy
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        legacy
            .write_all(
                &encode_frame(&ClientMessage::Hello {
                    protocol: legacy_protocol,
                    interest: host.config().spawn,
                    resume_token: None,
                    expected_manifest: Some(host.config().manifest.clone()),
                })
                .unwrap(),
            )
            .unwrap();

        let mut disconnected = false;
        for _ in 0..20 {
            let report = host.update(0.025);
            assert_eq!(report.domain_errors, 0);
            if report.disconnected > 0 {
                disconnected = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(disconnected, "the legacy peer must be rejected and closed");
        let mut bytes = Vec::new();
        legacy.read_to_end(&mut bytes).unwrap();
        let mut decoder = FrameDecoder::default();
        decoder.push(&bytes).unwrap();
        assert!(matches!(
            decoder.decode::<ServerMessage>().unwrap(),
            Some(ServerMessage::Error { code, .. }) if code == "version"
        ));
        assert!(
            decoder.decode::<ServerMessage>().unwrap().is_none(),
            "a rejected peer must receive no Welcome or private snapshots"
        );
        assert_eq!(decoder.buffered_bytes(), 0);
        assert!(host.actors().is_empty());
        assert!(host.sandbox().players.is_empty());
        let after = saved_value(&host, &path);
        for field in ["sessions", "nonplayer_actors", "sandbox_sequences"] {
            assert_eq!(
                after.get(field).unwrap(),
                before.get(field).unwrap(),
                "rejection changed {field}"
            );
        }
        assert!(after["sessions"].as_object().unwrap().is_empty());
        assert!(after["sandbox"]["players"].as_object().unwrap().is_empty());
    }
}

fn prior_living_catalog() -> Catalog {
    let mut policy = original_catalog();
    policy
        .items
        .extend(["bread", "flour", "water"].map(str::to_owned));
    policy.recipes.insert(
        "flour".into(),
        Recipe {
            inputs: BTreeMap::from([("wheat".into(), 2)]),
            outputs: BTreeMap::from([("flour".into(), 3)]),
            requires_workbench: true,
        },
    );
    policy.recipes.insert(
        "bread".into(),
        Recipe {
            inputs: BTreeMap::from([("flour".into(), 2), ("water".into(), 1)]),
            outputs: BTreeMap::from([("bread".into(), 4)]),
            requires_workbench: true,
        },
    );
    policy.validate().unwrap();
    policy
}

fn assert_shop_migration_preserves_assets(
    before: &SandboxState,
    after: &SandboxState,
    catalog: &Catalog,
) {
    assert_eq!(
        after.living.shops.keys().collect::<Vec<_>>(),
        before.living.shops.keys().collect::<Vec<_>>()
    );
    for (id, previous) in &before.living.shops {
        let shop = &after.living.shops[id];
        assert_eq!(shop.id, previous.id);
        assert_eq!(shop.name, previous.name);
        assert_eq!(shop.position, previous.position);
        assert_eq!(shop.source_feature, previous.source_feature);
        assert_eq!(shop.authored, previous.authored);
        assert_eq!(
            shop.stock, previous.stock,
            "migration must not refill shop stock"
        );
        let escrow = |state: &SandboxState| -> u64 {
            state
                .settlements
                .contracts
                .values()
                .filter(|contract| contract.issuer_shop == *id)
                .map(|contract| contract.reward)
                .sum()
        };
        assert_eq!(
            shop.money.checked_add(escrow(after)).unwrap(),
            previous.money.checked_add(escrow(before)).unwrap(),
            "initial funded contracts must conserve the issuer's till plus escrow"
        );
        let anchors = &after.settlements.reference_prices[id];
        for (item, original_quote) in &previous.prices {
            assert_eq!(
                anchors.get(item),
                Some(original_quote),
                "existing price policy must remain anchored: {item}"
            );
        }
        for (item, quote) in &shop.prices {
            assert!(catalog.items.contains(item));
            assert!(anchors.contains_key(item));
            assert!(
                quote.sell > 0 && quote.sell <= quote.buy && quote.buy <= terra_sandbox::MAX_MONEY
            );
        }
    }
    after.validate_with_catalog(catalog).unwrap();
}

#[test]
fn exact_prior_living_catalog_upgrades_when_a_saved_living_world_is_resumed() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("prior-reference.json");
    let mut host = server();
    let catalog = prior_living_catalog();
    let mut earlier = SandboxState::new();
    earlier
        .seed_living_demo(host.config().spawn, &[], &catalog)
        .unwrap();
    let mut saved = saved_value(&host, &path);
    saved["sandbox"] = serde_json::to_value(&earlier).unwrap();
    saved["sandbox"].as_object_mut().unwrap().remove("ecology");
    saved["sandbox"]
        .as_object_mut()
        .unwrap()
        .remove("settlements");
    saved["catalog"] = serde_json::to_value(catalog).unwrap();
    saved["clock"]["paused"] = true.into();
    fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    host.load_world(&path).unwrap();
    assert_eq!(host.update(0.05).domain_errors, 0);
    assert_shop_migration_preserves_assets(&earlier, host.sandbox(), &Catalog::builtin());
    assert_eq!(host.sandbox().living.producers, earlier.living.producers);
    assert!(
        host.sandbox()
            .resource_nodes
            .values()
            .any(|n| n.item == "berries")
    );
    assert!(
        host.sandbox()
            .resource_nodes
            .values()
            .any(|n| n.item == "iron_ore")
    );
    let upgraded = saved_value(&host, &path);
    assert_eq!(
        upgraded["catalog"],
        serde_json::to_value(Catalog::builtin()).unwrap()
    );
}

#[test]
fn only_the_exact_archived_builtin_is_extended_by_living_opt_in() {
    let directory = tempfile::tempdir().unwrap();
    let mut exact = server();
    exact.set_catalog(original_catalog()).unwrap();
    exact.enable_living_demo(&[]).unwrap();
    assert!(exact.sandbox().living.enabled);
    assert!(!exact.sandbox().ecology.resources.is_empty());
    let saved = saved_value(&exact, &directory.path().join("exact.json"));
    assert_eq!(
        saved["catalog"],
        serde_json::to_value(Catalog::builtin()).unwrap()
    );

    let mut custom_catalog = original_catalog();
    custom_catalog
        .crops
        .get_mut("wheat")
        .unwrap()
        .growth_seconds = 420.;
    let custom_policy = serde_json::to_value(&custom_catalog).unwrap();
    let mut custom = server();
    custom.set_catalog(custom_catalog).unwrap();
    assert!(custom.enable_living_demo(&[]).is_err());
    assert!(!custom.sandbox().living.enabled);
    let saved = saved_value(&custom, &directory.path().join("custom.json"));
    assert_eq!(saved["catalog"], custom_policy);
    assert!(custom.sandbox().ecology.animals.is_empty());
}

#[test]
fn earlier_living_save_gains_ecology_without_resetting_existing_state_or_policy() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("earlier-living-world.json");
    let mut initial = server();
    let origin = initial.config().spawn;
    // v0.2 included the settlement economy, but not the optional natural items.
    let mut policy = prior_living_catalog();
    policy.crops.get_mut("wheat").unwrap().growth_seconds = 420.;
    policy.validate().unwrap();
    let mut earlier = SandboxState::new();
    earlier.add_player(1, origin).unwrap();
    earlier.seed_living_demo(origin, &[], &policy).unwrap();
    let moved = LocalFrame::new(origin).to_geo([310., 0., 85.]).unwrap();
    earlier
        .set_controlled_motion(1, moved, [0.; 3], 75.)
        .unwrap();
    earlier
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("wood".into(), 17);
    earlier.living.accounts.get_mut(&1).unwrap().money = 12_345;
    earlier.validate_with_catalog(&policy).unwrap();

    let mut saved = saved_value(&initial, &path);
    saved["sandbox"] = serde_json::to_value(&earlier).unwrap();
    let state = saved["sandbox"].as_object_mut().unwrap();
    state.remove("ecology");
    state.remove("settlements");
    saved["catalog"] = serde_json::to_value(&policy).unwrap();
    saved["sessions"] = serde_json::to_value(BTreeMap::from([("a".repeat(64), 1_u64)])).unwrap();
    saved["clock"]["paused"] = true.into();
    fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    initial.load_world(&path).unwrap();
    assert!(initial.sandbox().ecology.animals.is_empty());
    let report = initial.update(0.05);
    assert_eq!(report.domain_errors, 0);
    let migrated = initial.sandbox();
    assert!(!migrated.ecology.animals.is_empty());
    assert!(
        migrated
            .ecology
            .seeded_regions
            .contains("ecology:starter:v1")
    );
    assert_eq!(migrated.players, earlier.players);
    assert_eq!(migrated.living.accounts, earlier.living.accounts);
    assert_shop_migration_preserves_assets(&earlier, migrated, &policy);
    assert_eq!(migrated.living.producers, earlier.living.producers);
    assert!(
        migrated
            .ecology
            .resources
            .values()
            .all(|r| policy.items.contains(r.kind.item()))
    );
    let ecology = migrated.ecology.clone();
    let nodes = migrated.resource_nodes.clone();
    for _ in 0..8 {
        assert_eq!(initial.update(0.05).domain_errors, 0);
    }
    assert_eq!(initial.sandbox().ecology, ecology);
    assert_eq!(initial.sandbox().resource_nodes, nodes);
    let resaved = saved_value(&initial, &path);
    assert_eq!(resaved["catalog"], serde_json::to_value(&policy).unwrap());
}

struct ElevatedSurface {
    origin: GeoPosition,
    excluded: GeoPosition,
}
impl WorldEnvironment for ElevatedSurface {
    fn ground_height_m(&self, position: GeoPosition) -> Option<f64> {
        // A stale spawn altitude is common when a streamed height map arrives.
        self.can_enter(position).then_some(250.)
    }
    fn can_enter(&self, position: GeoPosition) -> bool {
        self.origin.surface_distance_m(position) < 700.
    }
    fn allows_placement(&self, position: GeoPosition, radius: f64, _footprint: bool) -> bool {
        self.can_enter(position)
            && (position.altitude_m - 250.).abs() <= 3.
            && self.excluded.surface_distance_m(position) >= 30. + radius
    }
}

#[test]
fn host_projects_ecology_onto_resident_heights_before_checking_source_footprints() {
    let mut host = server();
    let origin = host.config().spawn;
    let excluded = LocalFrame::new(origin).to_geo([-105., 0., -66.]).unwrap();
    host.set_environment(Box::new(ElevatedSurface { origin, excluded }));
    host.enable_living_demo(&[]).unwrap();
    let state = host.sandbox();
    assert!(
        !state.ecology.habitats.is_empty(),
        "loaded elevated land must seed despite a stale spawn altitude"
    );
    assert!(!state.ecology.resources.is_empty());
    assert!(
        !state.ecology.habitats.contains_key("ecology:starter:boar"),
        "all nearby alternatives lie inside the source footprint"
    );
    for habitat in state.ecology.habitats.values() {
        assert!((habitat.center.altitude_m - 250.).abs() < 0.001);
        assert!(excluded.surface_distance_m(habitat.center) >= 30.4);
    }
    for resource in state.ecology.resources.values() {
        let position = state.resource_nodes[&resource.node_id].position;
        assert!((position.altitude_m - 250.).abs() < 0.001);
        assert!(excluded.surface_distance_m(position) >= 30.4);
    }
    // Existing earlier deer may be adopted, while every newly seeded species
    // must pass the strict endpoint test used for births and recovery.
    let newborns: Vec<_> = state
        .ecology
        .animals
        .values()
        .filter(|a| a.species != AnimalSpecies::Deer)
        .collect();
    assert!(!newborns.is_empty());
    for animal in newborns {
        let position = state.living.units[&animal.unit_id].position;
        assert!((position.altitude_m - 250.).abs() < 0.001);
        assert!(excluded.surface_distance_m(position) >= 30.4);
        assert!(origin.surface_distance_m(position) < 700.);
    }
    state.validate_with_catalog(&Catalog::builtin()).unwrap();
}

struct UnloadedSurface;
impl WorldEnvironment for UnloadedSurface {
    fn ground_height_m(&self, _: GeoPosition) -> Option<f64> {
        None
    }
}

#[test]
fn unloaded_terrain_defers_ecology_and_retries_after_residency_without_duplicates() {
    let mut host = server();
    host.set_environment(Box::new(UnloadedSurface));
    host.enable_living_demo(&[]).unwrap();
    assert!(host.sandbox().ecology.animals.is_empty());
    assert!(host.sandbox().ecology.resources.is_empty());
    assert!(
        !host
            .sandbox()
            .ecology
            .seeded_regions
            .contains("ecology:starter:v1")
    );
    for _ in 0..5 {
        assert_eq!(host.update(0.05).domain_errors, 0);
    }
    assert!(host.sandbox().ecology.animals.is_empty());
    let origin = host.config().spawn;
    let excluded = LocalFrame::new(origin).to_geo([650., 0., 0.]).unwrap();
    host.set_environment(Box::new(ElevatedSurface { origin, excluded }));
    assert_eq!(host.update(0.05).domain_errors, 0);
    let ids: Vec<_> = host.sandbox().ecology.animals.keys().copied().collect();
    assert!(!ids.is_empty());
    for _ in 0..5 {
        assert_eq!(host.update(0.05).domain_errors, 0);
    }
    assert_eq!(
        host.sandbox()
            .ecology
            .animals
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        ids
    );
}
