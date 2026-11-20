use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    time::{Duration, Instant},
};

use terra_core::{FeatureKind, GeoPosition, HeightField, TileKey, WorldChunk, WorldFeature};
use terra_net::{ClientMessage, NetClient, Server, ServerConfig, ServerMessage};
use terra_sandbox::{Catalog, Command, SandboxState};

fn server() -> Server {
    Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..ServerConfig::default()
    })
    .unwrap()
}

fn pump(host: &mut Server, client: &mut NetClient, count: usize) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    for index in 0.. {
        let report = host.update(0.025);
        messages.extend(client.poll().unwrap());
        if index + 1 >= count && !host.persistence_pending() && report.persistence_error.is_none() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "regional network fixture did not settle: {:?}",
            report.persistence_error
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    messages
}

fn identity(messages: &[ServerMessage]) -> (u64, String) {
    messages
        .iter()
        .find_map(|message| match message {
            ServerMessage::Welcome {
                actor_id,
                resume_token,
                ..
            } => Some((*actor_id, resume_token.clone())),
            _ => None,
        })
        .expect("successful handshake")
}

fn far_chunk() -> WorldChunk {
    let position = GeoPosition::new(40., -74., 0.).unwrap();
    WorldChunk {
        key: TileKey::at(position, 15).unwrap(),
        heights: HeightField::constant(0.),
        features: ["source:depleted-tree", "source:removed-tree"]
            .map(|id| WorldFeature {
                id: id.into(),
                kind: FeatureKind::Tree,
                points: vec![position],
                height_m: 6.,
                properties: BTreeMap::new(),
            })
            .to_vec(),
        sources: vec![],
        warnings: vec![],
    }
}

#[test]
fn prepared_source_batch_hydrates_in_background_without_simulating_dormant_records() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("world.db");
    let chunk = far_chunk();
    let mut host = server();
    host.try_register_chunk(&chunk).unwrap();
    host.open_region_store(&database).unwrap();
    host.try_evict_chunk(chunk.key).unwrap();
    host.checkpoint_regions().unwrap();
    assert!(host.sandbox().is_partial());
    let tick = host.tick();
    let original = host.sandbox().clone();
    assert!(!host.prepare_source_batch().unwrap());
    assert!(host.persistence_pending());
    assert_eq!(*host.sandbox(), original);
    assert!(
        host.try_register_chunk(&chunk).is_err(),
        "pending snapshot excludes host mutation"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while host.persistence_pending() {
        let report = host.update(0.025);
        assert!(
            report.persistence_error.is_none(),
            "{:?}",
            report.persistence_error
        );
        assert_eq!(host.tick(), tick);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!host.sandbox().is_partial());
    assert!(host.prepare_source_batch().unwrap());
    host.try_register_chunk(&chunk).unwrap();
    host.request_region_checkpoint().unwrap();
    host.update(0.025);
    assert!(host.persistence_pending());
    host.checkpoint_regions().unwrap();
    assert!(
        host.sandbox()
            .resource_nodes
            .contains_key("source:depleted-tree")
    );
}

#[test]
fn continuous_driving_focus_and_ping_advance_without_per_input_storage_barriers() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("driving.db");
    let mut host = server();
    host.enable_living_demo(&[]).unwrap();
    let cold = far_chunk();
    host.try_register_chunk(&cold).unwrap();
    host.try_evict_chunk(cold.key).unwrap();
    host.open_region_store(&database).unwrap();
    host.checkpoint_regions().unwrap();
    let mut client = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let (player, _) = identity(&pump(&mut host, &mut client, 8));
    let vehicle = host
        .sandbox()
        .transport
        .vehicles
        .values()
        .find(|vehicle| vehicle.owner == player)
        .unwrap()
        .unit_id;
    client
        .send(ClientMessage::Sandbox {
            sequence: 1,
            command: Command::Transport {
                action: terra_sandbox::TransportCommand::EnterVehicle {
                    vehicle_id: vehicle,
                },
            },
        })
        .unwrap();
    let messages = pump(&mut host, &mut client, 8);
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, ServerMessage::SandboxResult { sequence: 1, .. }))
    );
    let generation = host.region_store_status().unwrap().generation;
    let tick = host.tick();
    let fuel = host.sandbox().transport.vehicles[&vehicle].fuel_l;
    assert!(host.sandbox().is_partial());
    for sequence in 1..=20 {
        client
            .send(ClientMessage::Drive {
                sequence,
                input: terra_core::DriveInput {
                    throttle: 0.7,
                    ..Default::default()
                },
            })
            .unwrap();
        client
            .send(ClientMessage::Focus {
                position: host.config().spawn,
                radius_m: 1000.,
            })
            .unwrap();
        client
            .send(ClientMessage::Ping { nonce: sequence })
            .unwrap();
        let report = host.update(0.1);
        assert!(
            !report.persistence_pending,
            "movement/read-only input must not hydrate or checkpoint"
        );
        assert!(report.persistence_error.is_none());
        assert_eq!(report.domain_errors, 0);
        assert_eq!(host.region_store_status().unwrap().generation, generation);
        assert!(
            host.sandbox().is_partial(),
            "cold records stay asleep during driving"
        );
        client.poll().unwrap();
    }
    assert!(host.tick() >= tick + 40);
    assert!(host.sandbox().transport.vehicles[&vehicle].fuel_l < fuel);
    host.request_region_checkpoint().unwrap();
    host.update(0.);
    assert!(
        host.persistence_pending(),
        "explicit/timed checkpoint remains durable"
    );
    host.checkpoint_regions().unwrap();
    assert!(host.region_store_status().unwrap().generation > generation);
}

#[test]
fn server_json_migration_validates_host_metadata_and_never_overwrites() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("migrated.db");
    let json = directory.path().join("server.json");
    let mut original = server();
    original
        .seed_resource(
            "authored:migration".into(),
            original.config().spawn,
            "stone",
            19,
            0.,
        )
        .unwrap();
    original.export_world_new(&json).unwrap();
    assert!(original.export_world_new(&json).is_err());
    original.import_region_store_new(&database).unwrap();
    assert!(original.import_region_store_new(&database).is_err());
    let mut restored = server();
    restored.open_region_store(&database).unwrap();
    assert_eq!(*restored.sandbox(), *original.sandbox());
    let exported = directory.path().join("roundtrip.json");
    restored.export_world_new(&exported).unwrap();
    let mut from_json = server();
    from_json.load_world(exported).unwrap();
    assert_eq!(*from_json.sandbox(), *original.sandbox());
}

#[test]
fn regional_restart_preserves_principal_wallet_inventory_clock_and_command_sequence() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("world.sqlite");
    let mut initial = server();
    initial.enable_living_demo(&[]).unwrap();
    initial.open_region_store(&database).unwrap();
    let spawn = initial.config().spawn;
    let mut client = NetClient::connect(initial.local_addr().unwrap(), spawn).unwrap();
    let messages = pump(&mut initial, &mut client, 8);
    let (player, token) = identity(&messages);
    client
        .send(ClientMessage::Sandbox {
            sequence: 1,
            command: Command::Craft {
                recipe: "planks".into(),
                count: 1,
            },
        })
        .unwrap();
    pump(&mut initial, &mut client, 8);
    let money = initial.sandbox().living.accounts[&player].money;
    let inventory = initial.sandbox().players[&player].inventory.clone();
    let clock = initial.clock().unix_seconds;
    let tick = initial.tick();
    let generation = initial.checkpoint_regions().unwrap().generation;
    assert!(generation > 0);
    drop(client);
    drop(initial);

    let mut restored = server();
    assert_eq!(
        restored.open_region_store(&database).unwrap().generation,
        generation
    );
    assert_eq!(restored.tick(), tick);
    assert_eq!(restored.clock().unix_seconds, clock);
    assert_eq!(restored.sandbox().players[&player].inventory, inventory);
    assert_eq!(restored.sandbox().living.accounts[&player].money, money);
    let mut client =
        NetClient::connect_with_token(restored.local_addr().unwrap(), spawn, Some(token)).unwrap();
    let messages = pump(&mut restored, &mut client, 8);
    assert_eq!(identity(&messages).0, player);
    assert!(messages.iter().any(|message| matches!(
        message,
        ServerMessage::Welcome {
            next_sandbox_sequence: 2,
            ..
        }
    )));
    client
        .send(ClientMessage::Sandbox {
            sequence: 1,
            command: Command::Craft {
                recipe: "planks".into(),
                count: 1,
            },
        })
        .unwrap();
    let messages = pump(&mut restored, &mut client, 8);
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, ServerMessage::SandboxRejected { sequence: 1, .. }))
    );
    assert_eq!(restored.sandbox().players[&player].inventory, inventory);
}

#[test]
fn dormant_source_depletion_and_destruction_survive_export_revisit_and_restart() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = directory.path().join("legacy.json");
    let database = directory.path().join("world.sqlite");
    let exported = directory.path().join("export.json");
    let chunk = far_chunk();
    let mut host = server();
    host.try_register_chunk(&chunk).unwrap();
    host.save_world(&legacy).unwrap();
    let mut saved: serde_json::Value = serde_json::from_slice(&fs::read(&legacy).unwrap()).unwrap();
    saved["sandbox"]["resource_nodes"]["source:depleted-tree"]["remaining"] = 3.into();
    saved["sandbox"]["resource_nodes"]
        .as_object_mut()
        .unwrap()
        .remove("source:removed-tree");
    saved["sandbox"]["feature_edits"] = serde_json::json!(["source:removed-tree"]);
    fs::write(&legacy, serde_json::to_vec(&saved).unwrap()).unwrap();
    host.load_world(&legacy).unwrap();
    host.open_region_store(&database).unwrap();
    host.evict_chunk(chunk.key);
    let status = host.checkpoint_regions().unwrap();
    assert!(status.dormant_records >= 4);
    assert!(
        !host
            .sandbox()
            .resource_nodes
            .contains_key("source:depleted-tree")
    );
    host.save_world(&exported).unwrap();
    let exported: serde_json::Value =
        serde_json::from_slice(&fs::read(&exported).unwrap()).unwrap();
    assert_eq!(
        exported["sandbox"]["resource_nodes"]["source:depleted-tree"]["remaining"],
        3
    );
    assert_eq!(
        exported["sandbox"]["feature_edits"],
        serde_json::json!(["source:removed-tree"])
    );
    host.try_register_chunk(&chunk).unwrap();
    assert_eq!(
        host.sandbox().resource_nodes["source:depleted-tree"].remaining,
        3
    );
    assert!(
        !host
            .sandbox()
            .resource_nodes
            .contains_key("source:removed-tree")
    );
    assert!(host.sandbox().feature_edits.contains("source:removed-tree"));
    host.evict_chunk(chunk.key);
    host.checkpoint_regions().unwrap();
    drop(host);

    let mut restored = server();
    restored.open_region_store(&database).unwrap();
    restored.try_register_chunk(&chunk).unwrap();
    assert_eq!(
        restored.sandbox().resource_nodes["source:depleted-tree"].remaining,
        3
    );
    assert!(
        !restored
            .sandbox()
            .resource_nodes
            .contains_key("source:removed-tree")
    );
    assert!(
        restored
            .sandbox()
            .feature_edits
            .contains("source:removed-tree")
    );
}

#[test]
fn stale_host_checkpoint_cannot_overwrite_committed_regions_or_mutate_live_state() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("world.sqlite");
    let mut first = server();
    first.open_region_store(&database).unwrap();
    first.checkpoint_regions().unwrap();
    let mut stale = server();
    stale.open_region_store(&database).unwrap();
    let before = stale.sandbox().clone();
    first
        .seed_resource(
            "authored:committed".into(),
            first.config().spawn,
            "wood",
            19,
            0.,
        )
        .unwrap();
    first.checkpoint_regions().unwrap();
    assert!(stale.checkpoint_regions().is_err());
    assert_eq!(*stale.sandbox(), before);
    drop(first);
    drop(stale);
    let mut restored = server();
    restored.open_region_store(database).unwrap();
    assert_eq!(
        restored.sandbox().resource_nodes["authored:committed"].remaining,
        19
    );
}

#[test]
fn store_identity_and_catalog_guards_reject_without_replacing_current_world() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("world.sqlite");
    let mut initial = server();
    initial.open_region_store(&database).unwrap();
    initial.checkpoint_regions().unwrap();
    let before = initial.sandbox().clone();
    let mut changed = Catalog::builtin();
    changed.items.insert("custom_item".into());
    assert!(initial.set_catalog(changed).is_err());
    assert_eq!(*initial.sandbox(), before);
    drop(initial);
    let mut config = ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    config.manifest.seed += 1;
    let mut incompatible = Server::bind(config).unwrap();
    let before = incompatible.sandbox().clone();
    assert!(incompatible.open_region_store(&database).is_err());
    assert_eq!(*incompatible.sandbox(), before);
    assert!(incompatible.region_store_status().is_none());
}

#[test]
fn complete_export_cannot_overwrite_database_companions_or_file_aliases() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("world.sqlite");
    let mut host = server();
    host.open_region_store(&database).unwrap();
    host.seed_resource(
        "authored:protected".into(),
        host.config().spawn,
        "stone",
        23,
        0.,
    )
    .unwrap();
    host.checkpoint_regions().unwrap();
    let mut protected = vec![
        database.clone(),
        directory.path().join(".").join("world.sqlite"),
    ];
    for suffix in ["-wal", "-shm", "-journal"] {
        protected.push(directory.path().join(format!("world.sqlite{suffix}")));
    }
    #[cfg(unix)]
    {
        let link = directory.path().join("world-link.json");
        std::os::unix::fs::symlink(&database, &link).unwrap();
        protected.push(link);
        let hard_link = directory.path().join("world-hard-link.json");
        fs::hard_link(&database, &hard_link).unwrap();
        protected.push(hard_link);
    }
    let before: Vec<_> = protected.iter().map(|path| fs::read(path).ok()).collect();
    let generation = host.region_store_status().unwrap().generation;
    for (path, previous) in protected.iter().zip(before.iter()) {
        assert!(
            host.save_world(path).is_err(),
            "must reject protected output {}",
            path.display()
        );
        assert_eq!(
            &fs::read(path).ok(),
            previous,
            "must not change protected output {}",
            path.display()
        );
    }
    assert_eq!(host.region_store_status().unwrap().generation, generation);
    let safe_export = directory.path().join("world.sqlite.json");
    host.save_world(&safe_export).unwrap();
    let exported: serde_json::Value =
        serde_json::from_slice(&fs::read(safe_export).unwrap()).unwrap();
    assert_eq!(
        exported["sandbox"]["resource_nodes"]["authored:protected"]["remaining"],
        23
    );
    drop(host);
    let mut restored = server();
    restored.open_region_store(database).unwrap();
    assert_eq!(
        restored.sandbox().resource_nodes["authored:protected"].remaining,
        23
    );
}

#[test]
fn source_feature_tombstone_stays_active_until_all_contributing_tiles_evict() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = directory.path().join("legacy.json");
    let database = directory.path().join("world.sqlite");
    let mut source = far_chunk();
    let outside = source.key.bounds().point(1.5, 0.5, 0.).unwrap();
    assert_ne!(TileKey::at(outside, 15).unwrap(), source.key);
    source.features[1].points = vec![outside];
    let other = WorldChunk {
        key: TileKey::new(15, source.key.x, source.key.y + 1).unwrap(),
        features: vec![source.features[1].clone()],
        ..source.clone()
    };
    let mut host = server();
    host.try_register_chunk(&source).unwrap();
    host.try_register_chunk(&other).unwrap();
    host.save_world(&legacy).unwrap();
    let mut saved: serde_json::Value = serde_json::from_slice(&fs::read(&legacy).unwrap()).unwrap();
    saved["sandbox"]["resource_nodes"]
        .as_object_mut()
        .unwrap()
        .remove("source:removed-tree");
    saved["sandbox"]["feature_edits"] = serde_json::json!(["source:removed-tree"]);
    fs::write(&legacy, serde_json::to_vec(&saved).unwrap()).unwrap();
    host.load_world(&legacy).unwrap();
    host.open_region_store(&database).unwrap();
    host.checkpoint_regions().unwrap();
    assert!(host.sandbox().feature_edits.contains("source:removed-tree"));
    host.evict_chunk(source.key);
    host.checkpoint_regions().unwrap();
    assert!(
        host.sandbox().feature_edits.contains("source:removed-tree"),
        "a shared feature anchor must remain active for the other resident tile"
    );
    host.evict_chunk(other.key);
    host.checkpoint_regions().unwrap();
    assert!(!host.sandbox().feature_edits.contains("source:removed-tree"));
    host.try_register_chunk(&source).unwrap();
    host.checkpoint_regions().unwrap();
    assert!(host.sandbox().feature_edits.contains("source:removed-tree"));
    assert!(
        !host
            .sandbox()
            .resource_nodes
            .contains_key("source:removed-tree")
    );
}

#[test]
fn live_connections_prevent_store_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = server();
    let mut client = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let messages = pump(&mut host, &mut client, 8);
    let player = identity(&messages).0;
    assert!(
        host.open_region_store(directory.path().join("world.sqlite"))
            .is_err()
    );
    assert!(host.region_store_status().is_none());
    assert!(host.sandbox().players.contains_key(&player));
}

#[test]
fn invalid_private_host_metadata_cannot_replace_a_running_world() {
    let directory = tempfile::tempdir().unwrap();
    let export = directory.path().join("source.json");
    let database = directory.path().join("world.sqlite");
    let mut source = server();
    let mut client =
        NetClient::connect(source.local_addr().unwrap(), source.config().spawn).unwrap();
    let messages = pump(&mut source, &mut client, 8);
    let player = identity(&messages).0;
    source.save_world(&export).unwrap();
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(export).unwrap()).unwrap();
    let mut state: SandboxState =
        serde_json::from_value(metadata.as_object_mut().unwrap().remove("sandbox").unwrap())
            .unwrap();
    let catalog: Catalog = serde_json::from_value(metadata["catalog"].clone()).unwrap();
    metadata["sessions"] = serde_json::json!({"bad": player});
    let identity = serde_json::json!({
        "kind": "terra-server", "schema_version": 1,
        "manifest": source.config().manifest,
    });
    let mut store =
        terra_persistence::RegionStore::open(&database, identity, &catalog, 15).unwrap();
    store
        .checkpoint(&mut state, metadata, &BTreeSet::new())
        .unwrap();
    drop(store);
    let mut destination = server();
    destination
        .seed_resource(
            "authored:unchanged".into(),
            destination.config().spawn,
            "stone",
            13,
            0.,
        )
        .unwrap();
    let before = destination.sandbox().clone();
    assert!(destination.open_region_store(database).is_err());
    assert_eq!(*destination.sandbox(), before);
    assert!(destination.region_store_status().is_none());
}

#[test]
fn failed_commit_holds_success_responses_and_blocks_new_input_until_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("world.sqlite");
    let mut host = server();
    host.open_region_store(&database).unwrap();
    host.checkpoint_regions().unwrap();
    let blocker = rusqlite::Connection::open(&database).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut client = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let mut blocked = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let report = host.update(0.025);
        if report.persistence_error.is_some() {
            blocked = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        blocked,
        "real SQLite write lock must block the admission commit"
    );
    assert!(client.poll().unwrap().iter().all(|message| !matches!(
        message,
        ServerMessage::Welcome { .. } | ServerMessage::SandboxResult { .. }
    )));
    let (player, before) = host
        .sandbox()
        .players
        .iter()
        .next()
        .map(|(id, player)| (*id, player.inventory.clone()))
        .unwrap();
    let tick = host.tick();
    client
        .send(ClientMessage::Sandbox {
            sequence: 1,
            command: Command::Craft {
                recipe: "planks".into(),
                count: 1,
            },
        })
        .unwrap();
    let report = host.update(0.1);
    assert!(report.persistence_error.is_some());
    assert_eq!(
        report.messages, 0,
        "blocked host must not consume a second command batch"
    );
    assert_eq!(host.tick(), tick);
    assert_eq!(host.sandbox().players[&player].inventory, before);
    assert!(client.poll().unwrap().iter().all(|message| !matches!(
        message,
        ServerMessage::Welcome { .. } | ServerMessage::SandboxResult { .. }
    )));

    blocker.execute_batch("ROLLBACK").unwrap();
    let messages = pump(&mut host, &mut client, 8);
    let (resumed_player, token) = identity(&messages);
    assert_eq!(resumed_player, player);
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, ServerMessage::SandboxResult { sequence: 1, .. }))
    );
    assert_eq!(host.sandbox().players[&player].inventory["planks"], 4);
    drop(client);
    drop(host);
    let mut restored = server();
    restored.open_region_store(&database).unwrap();
    assert_eq!(restored.sandbox().players[&player].inventory["planks"], 4);
    let mut client = NetClient::connect_with_token(
        restored.local_addr().unwrap(),
        restored.config().spawn,
        Some(token),
    )
    .unwrap();
    let messages = pump(&mut restored, &mut client, 8);
    assert_eq!(identity(&messages).0, player);
    assert!(messages.iter().any(|message| matches!(
        message,
        ServerMessage::Welcome {
            next_sandbox_sequence: 2,
            ..
        }
    )));
}

#[test]
fn unauthenticated_commands_and_obsolete_hello_do_not_activate_cold_pages() {
    use std::{io::Write, net::TcpStream};
    let directory = tempfile::tempdir().unwrap();
    let mut host = server();
    let chunk = far_chunk();
    host.try_register_chunk(&chunk).unwrap();
    host.try_evict_chunk(chunk.key).unwrap();
    host.open_region_store(directory.path().join("world.db"))
        .unwrap();
    host.checkpoint_regions().unwrap();
    assert!(host.sandbox().is_partial());
    let generation = host.region_store_status().unwrap().generation;
    let mut socket = TcpStream::connect(host.local_addr().unwrap()).unwrap();
    for message in [
        ClientMessage::Sandbox {
            sequence: 1,
            command: Command::Craft {
                recipe: "planks".into(),
                count: 1,
            },
        },
        ClientMessage::Hello {
            protocol: 4,
            interest: host.config().spawn,
            resume_token: None,
            expected_manifest: None,
        },
    ] {
        socket
            .write_all(&terra_net::encode_frame(&message).unwrap())
            .unwrap();
    }
    for _ in 0..10 {
        let report = host.update(0.025);
        assert!(
            !report.persistence_pending,
            "unadmitted input must not activate the world"
        );
        assert!(report.persistence_error.is_none());
        assert!(host.sandbox().is_partial());
        assert_eq!(host.region_store_status().unwrap().generation, generation);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(host.sandbox().players.is_empty());
}

#[test]
fn already_connected_resume_is_rejected_without_any_hydration_or_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = server();
    let chunk = far_chunk();
    host.try_register_chunk(&chunk).unwrap();
    host.try_evict_chunk(chunk.key).unwrap();
    host.open_region_store(directory.path().join("world.db"))
        .unwrap();
    host.checkpoint_regions().unwrap();
    let mut owner = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let (player, token) = identity(&pump(&mut host, &mut owner, 5));
    assert!(host.sandbox().is_partial());
    let generation = host.region_store_status().unwrap().generation;
    let mut duplicate =
        NetClient::connect_with_token(host.local_addr().unwrap(), host.config().spawn, Some(token))
            .unwrap();
    let mut rejected = false;
    for _ in 0..10 {
        let report = host.update(0.025);
        assert!(!report.persistence_pending);
        assert!(report.persistence_error.is_none());
        assert!(
            host.sandbox().is_partial(),
            "duplicate Hello must not synchronously hydrate either"
        );
        assert_eq!(host.region_store_status().unwrap().generation, generation);
        rejected |= duplicate.poll().unwrap().iter().any(
            |message| matches!(message, ServerMessage::Error { code, .. } if code == "connected"),
        );
        owner.poll().unwrap();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(rejected);
    assert_eq!(host.sandbox().players.len(), 1);
    assert!(!host.sandbox().is_player_dormant(player));
}
