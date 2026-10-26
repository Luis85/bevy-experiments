use std::{io::Write, net::TcpStream, time::Duration};

use terra_core::{ActorKind, ActorState, GeoPosition};
use terra_net::{ClientMessage, NetClient, Server, ServerConfig, ServerMessage, encode_frame};
use terra_sandbox::{Command, FactionRole};

fn server() -> Server {
    Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..ServerConfig::default()
    })
    .unwrap()
}

fn pump(
    server: &mut Server,
    clients: &mut [&mut NetClient],
    count: usize,
) -> Vec<Vec<ServerMessage>> {
    let mut output: Vec<Vec<ServerMessage>> = clients.iter().map(|_| Vec::new()).collect();
    for _ in 0..count {
        server.update(0.025);
        for (index, client) in clients.iter_mut().enumerate() {
            output[index].extend(client.poll().unwrap());
        }
        // Allow platform TCP scheduling without requiring a background server.
        std::thread::sleep(Duration::from_millis(1));
    }
    output
}

fn welcome(messages: &[ServerMessage]) -> (u64, String) {
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

#[test]
fn two_clients_receive_one_authoritative_world_and_validated_movement() {
    let mut server = server();
    let address = server.local_addr().unwrap();
    let spawn = server.config().spawn;
    let mut alice = NetClient::connect(address, spawn).unwrap();
    let mut bob = NetClient::connect(address, spawn).unwrap();
    let received = pump(&mut server, &mut [&mut alice, &mut bob], 8);
    let (alice_id, _) = welcome(&received[0]);
    let (bob_id, _) = welcome(&received[1]);
    assert_ne!(alice_id, bob_id);
    assert_eq!(server.actors().len(), 2);
    for messages in &received {
        assert!(messages.iter().any(|message| matches!(message,
            ServerMessage::Snapshot { actors, .. } if actors.len() == 2)));
    }
    let start = server.actors()[&alice_id].position;
    alice
        .send(ClientMessage::Move {
            sequence: 1,
            velocity_enu_m_s: [5.0, 0.0, 0.0],
        })
        .unwrap();
    pump(&mut server, &mut [&mut alice, &mut bob], 12);
    let end = server.actors()[&alice_id].position;
    assert!(start.surface_distance_m(end) > 1.0);
    assert_eq!(server.actors()[&bob_id].velocity_enu_m_s, [0.0; 3]);
    alice
        .send(ClientMessage::Move {
            sequence: 2,
            velocity_enu_m_s: [100_000.0, 0.0, 0.0],
        })
        .unwrap();
    let received = pump(&mut server, &mut [&mut alice, &mut bob], 2);
    assert!(
        received[0]
            .iter()
            .any(|message| matches!(message, ServerMessage::Error { code, .. } if code == "speed"))
    );
    assert!(server.actors()[&alice_id].speed_m_s() <= 12.0);
    alice
        .send(ClientMessage::Move {
            sequence: 1,
            velocity_enu_m_s: [-5.0, 0.0, 0.0],
        })
        .unwrap();
    let received = pump(&mut server, &mut [&mut alice, &mut bob], 2);
    assert!(
        received[0].iter().any(
            |message| matches!(message, ServerMessage::Error { code, .. } if code == "sequence")
        )
    );
    assert!(server.actors()[&alice_id].velocity_enu_m_s[0] >= 0.0);
}

#[test]
fn snapshots_apply_interest_and_explicit_capacity() {
    let mut server = Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        max_snapshot_actors: 1,
        ..ServerConfig::default()
    })
    .unwrap();
    let address = server.local_addr().unwrap();
    let spawn = server.config().spawn;
    let mut client = NetClient::connect(address, spawn).unwrap();
    let received = pump(&mut server, &mut [&mut client], 4);
    let (owned, _) = welcome(&received[0]);
    server
        .insert_actor(ActorState {
            id: 1000,
            kind: ActorKind::Vehicle,
            position: spawn,
            velocity_enu_m_s: [0.0; 3],
            heading_deg: 0.0,
        })
        .unwrap();
    server
        .insert_actor(ActorState {
            id: 1001,
            kind: ActorKind::Airplane,
            position: GeoPosition::new(0.0, 0.0, 1_000.0).unwrap(),
            velocity_enu_m_s: [0.0; 3],
            heading_deg: 0.0,
        })
        .unwrap();
    let received = pump(&mut server, &mut [&mut client], 8);
    assert!(received[0].iter().any(|message| matches!(message,
        ServerMessage::Snapshot { actors, truncated: true, total_in_interest: 2, .. }
            if actors.len() == 1 && actors[0].id == owned)));
}

#[test]
fn sandbox_commands_are_owned_transactional_and_replay_protected() {
    let mut server = server();
    let spawn = server.config().spawn;
    let mut alice = NetClient::connect(server.local_addr().unwrap(), spawn).unwrap();
    let mut bob = NetClient::connect(server.local_addr().unwrap(), spawn).unwrap();
    let received = pump(&mut server, &mut [&mut alice, &mut bob], 4);
    let (alice_id, _) = welcome(&received[0]);
    alice
        .send(ClientMessage::Sandbox {
            sequence: 1,
            command: Command::ClaimPlot {
                center: spawn,
                radius_m: 10.0,
            },
        })
        .unwrap();
    pump(&mut server, &mut [&mut alice, &mut bob], 3);
    assert_eq!(server.sandbox().plots.len(), 1);
    assert_eq!(
        server.sandbox().plots.values().next().unwrap().owner,
        alice_id
    );
    alice
        .send(ClientMessage::Sandbox {
            sequence: 2,
            command: Command::Craft {
                recipe: "planks".into(),
                count: 3,
            },
        })
        .unwrap();
    pump(&mut server, &mut [&mut alice, &mut bob], 3);
    assert_eq!(server.sandbox().players[&alice_id].inventory["planks"], 12);
    alice
        .send(ClientMessage::Sandbox {
            sequence: 2,
            command: Command::Craft {
                recipe: "planks".into(),
                count: 3,
            },
        })
        .unwrap();
    bob.send(ClientMessage::Sandbox {
        sequence: 1,
        command: Command::Build {
            blueprint: "fence".into(),
            position: spawn,
        },
    })
    .unwrap();
    let received = pump(&mut server, &mut [&mut alice, &mut bob], 3);
    assert!(
        received[0]
            .iter()
            .any(|message| matches!(message, ServerMessage::SandboxRejected { sequence: 2, .. }))
    );
    assert!(
        received[1]
            .iter()
            .any(|message| matches!(message, ServerMessage::SandboxRejected { sequence: 1, .. }))
    );
    assert_eq!(server.sandbox().players[&alice_id].inventory["planks"], 12);
    assert!(server.sandbox().structures.is_empty());
}

#[test]
fn save_restart_and_resume_preserve_inventory_and_command_sequences() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.json");
    let mut initial = server();
    let spawn = initial.config().spawn;
    let mut client = NetClient::connect(initial.local_addr().unwrap(), spawn).unwrap();
    let received = pump(&mut initial, &mut [&mut client], 4);
    let (player_id, token) = welcome(&received[0]);
    client
        .send(ClientMessage::Sandbox {
            sequence: 1,
            command: Command::Craft {
                recipe: "planks".into(),
                count: 1,
            },
        })
        .unwrap();
    pump(&mut initial, &mut [&mut client], 4);
    initial.save_world(&path).unwrap();
    drop(client);
    drop(initial);
    let mut restored = server();
    restored.load_world(&path).unwrap();
    let mut client =
        NetClient::connect_with_token(restored.local_addr().unwrap(), spawn, Some(token)).unwrap();
    let received = pump(&mut restored, &mut [&mut client], 4);
    assert_eq!(welcome(&received[0]).0, player_id);
    assert_eq!(
        restored.sandbox().players[&player_id].inventory["planks"],
        4
    );
    assert!(received[0].iter().any(|message| matches!(
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
    let received = pump(&mut restored, &mut [&mut client], 3);
    assert!(
        received[0]
            .iter()
            .any(|message| matches!(message, ServerMessage::SandboxRejected { .. }))
    );
    assert_eq!(
        restored.sandbox().players[&player_id].inventory["planks"],
        4
    );
}

#[test]
fn malformed_network_frame_is_dropped_without_killing_server() {
    let mut server = server();
    let mut hostile = TcpStream::connect(server.local_addr().unwrap()).unwrap();
    hostile.write_all(&u32::MAX.to_be_bytes()).unwrap();
    let report = server.update(0.05);
    assert_eq!(report.disconnected, 1);
    let mut good = NetClient::connect(server.local_addr().unwrap(), server.config().spawn).unwrap();
    let received = pump(&mut server, &mut [&mut good], 4);
    assert!(
        received[0]
            .iter()
            .any(|message| matches!(message, ServerMessage::Welcome { .. }))
    );
}

#[test]
fn movement_before_handshake_does_not_create_or_mutate_actor() {
    let mut server = server();
    let mut raw = TcpStream::connect(server.local_addr().unwrap()).unwrap();
    raw.write_all(
        &encode_frame(&ClientMessage::Move {
            sequence: 1,
            velocity_enu_m_s: [5.0, 0.0, 0.0],
        })
        .unwrap(),
    )
    .unwrap();
    server.update(0.05);
    assert!(server.actors().is_empty());
    assert!(server.sandbox().players.is_empty());
}

#[test]
fn malformed_save_is_rejected_without_replacing_live_world() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.json");
    let mut server = server();
    let actor = ActorState {
        id: 1000,
        kind: ActorKind::Animal,
        position: server.config().spawn,
        velocity_enu_m_s: [0.0; 3],
        heading_deg: 0.0,
    };
    server.insert_actor(actor.clone()).unwrap();
    server.save_world(&path).unwrap();
    let valid: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for mutate in [
        |value: &mut serde_json::Value| value["schema_version"] = 999.into(),
        |value: &mut serde_json::Value| value["sessions"] = serde_json::json!({"bad":1}),
        |value: &mut serde_json::Value| value["clock"]["time_scale"] = (-10).into(),
        |value: &mut serde_json::Value| value["manifest"]["seed"] = 123.into(),
    ] {
        let mut invalid = valid.clone();
        mutate(&mut invalid);
        std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(server.load_world(&path).is_err());
        assert_eq!(server.actors()[&1000], actor);
        assert!(server.sandbox().players.is_empty());
    }
}

#[test]
fn sandbox_target_requires_authoritative_terrain_support() {
    let mut server = server();
    let spawn = server.config().spawn;
    let mut client = NetClient::connect(server.local_addr().unwrap(), spawn).unwrap();
    pump(&mut server, &mut [&mut client], 4);
    let mut floating = spawn;
    floating.altitude_m += 10.0;
    client
        .send(ClientMessage::Sandbox {
            sequence: 1,
            command: Command::ClaimPlot {
                center: floating,
                radius_m: 10.0,
            },
        })
        .unwrap();
    let received = pump(&mut server, &mut [&mut client], 4);
    assert!(received[0].iter().any(|message| matches!(message,
        ServerMessage::SandboxRejected { message, .. } if message.contains("terrain surface"))));
    assert!(server.sandbox().plots.is_empty());
}

#[test]
fn shared_faction_permissions_and_demotion_survive_player_resume() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("factions.json");
    let mut initial = server();
    let spawn = initial.config().spawn;
    let mut alice = NetClient::connect(initial.local_addr().unwrap(), spawn).unwrap();
    let mut bob = NetClient::connect(initial.local_addr().unwrap(), spawn).unwrap();
    let received = pump(&mut initial, &mut [&mut alice, &mut bob], 4);
    let (alice_id, _) = welcome(&received[0]);
    let (bob_id, bob_token) = welcome(&received[1]);

    alice
        .send(ClientMessage::Sandbox {
            sequence: 1,
            command: Command::CreateFaction {
                name: "Surveyors".into(),
            },
        })
        .unwrap();
    pump(&mut initial, &mut [&mut alice, &mut bob], 3);
    let faction_id = *initial.sandbox().factions.keys().next().unwrap();
    alice
        .send(ClientMessage::Sandbox {
            sequence: 2,
            command: Command::ClaimPlot {
                center: spawn,
                radius_m: 10.0,
            },
        })
        .unwrap();
    pump(&mut initial, &mut [&mut alice, &mut bob], 3);
    let plot_id = *initial.sandbox().plots.keys().next().unwrap();
    alice
        .send(ClientMessage::Sandbox {
            sequence: 3,
            command: Command::TransferPlotToFaction {
                plot_id,
                faction_id,
            },
        })
        .unwrap();
    alice
        .send(ClientMessage::Sandbox {
            sequence: 4,
            command: Command::InviteToFaction {
                faction_id,
                player_id: bob_id,
            },
        })
        .unwrap();
    pump(&mut initial, &mut [&mut alice, &mut bob], 3);
    bob.send(ClientMessage::Sandbox {
        sequence: 1,
        command: Command::AcceptFactionInvite { faction_id },
    })
    .unwrap();
    bob.send(ClientMessage::Sandbox {
        sequence: 2,
        command: Command::Craft {
            recipe: "planks".into(),
            count: 1,
        },
    })
    .unwrap();
    bob.send(ClientMessage::Sandbox {
        sequence: 3,
        command: Command::Build {
            blueprint: "fence".into(),
            position: spawn,
        },
    })
    .unwrap();
    let received = pump(&mut initial, &mut [&mut alice, &mut bob], 3);
    assert!(
        received[1]
            .iter()
            .any(|message| matches!(message, ServerMessage::SandboxRejected { sequence: 3, .. }))
    );
    assert!(initial.sandbox().structures.is_empty());

    // A member cannot promote themselves. The trusted sender is Bob's socket,
    // regardless of the target player ID carried by this command.
    bob.send(ClientMessage::Sandbox {
        sequence: 4,
        command: Command::SetFactionRole {
            faction_id,
            player_id: bob_id,
            role: FactionRole::Builder,
        },
    })
    .unwrap();
    let received = pump(&mut initial, &mut [&mut alice, &mut bob], 3);
    assert!(
        received[1]
            .iter()
            .any(|message| matches!(message, ServerMessage::SandboxRejected { sequence: 4, .. }))
    );
    alice
        .send(ClientMessage::Sandbox {
            sequence: 5,
            command: Command::SetFactionRole {
                faction_id,
                player_id: bob_id,
                role: FactionRole::Builder,
            },
        })
        .unwrap();
    pump(&mut initial, &mut [&mut alice, &mut bob], 3);
    bob.send(ClientMessage::Sandbox {
        sequence: 5,
        command: Command::Build {
            blueprint: "fence".into(),
            position: spawn,
        },
    })
    .unwrap();
    pump(&mut initial, &mut [&mut alice, &mut bob], 3);
    let structure_id = *initial.sandbox().structures.keys().next().unwrap();
    let structure = &initial.sandbox().structures[&structure_id];
    assert_eq!(structure.owner, bob_id);
    assert_eq!(structure.faction_owner, Some(faction_id));
    assert_eq!(initial.sandbox().factions[&faction_id].leader, alice_id);
    let hitpoints = structure.hitpoints;

    alice
        .send(ClientMessage::Sandbox {
            sequence: 6,
            command: Command::SetFactionRole {
                faction_id,
                player_id: bob_id,
                role: FactionRole::Member,
            },
        })
        .unwrap();
    pump(&mut initial, &mut [&mut alice, &mut bob], 3);
    bob.send(ClientMessage::Sandbox {
        sequence: 6,
        command: Command::Damage {
            structure_id,
            amount: 25,
        },
    })
    .unwrap();
    let received = pump(&mut initial, &mut [&mut alice, &mut bob], 3);
    assert!(
        received[1]
            .iter()
            .any(|message| matches!(message, ServerMessage::SandboxRejected { sequence: 6, .. }))
    );
    assert!(received[1].iter().any(|message| matches!(message,
        ServerMessage::SandboxSnapshot { view, .. } if view.player.id == bob_id
            && view.factions.iter().any(|faction| faction.id == faction_id && faction.members.get(&bob_id) == Some(&FactionRole::Member)))));
    assert_eq!(
        initial.sandbox().structures[&structure_id].hitpoints,
        hitpoints
    );
    initial.save_world(&path).unwrap();
    drop(alice);
    drop(bob);
    drop(initial);

    let mut restored = server();
    restored.load_world(path).unwrap();
    let mut bob =
        NetClient::connect_with_token(restored.local_addr().unwrap(), spawn, Some(bob_token))
            .unwrap();
    let received = pump(&mut restored, &mut [&mut bob], 4);
    assert_eq!(welcome(&received[0]).0, bob_id);
    assert!(received[0].iter().any(|message| matches!(
        message,
        ServerMessage::Welcome {
            next_sandbox_sequence: 7,
            ..
        }
    )));
    assert_eq!(
        restored.sandbox().structures[&structure_id].faction_owner,
        Some(faction_id)
    );
    bob.send(ClientMessage::Sandbox {
        sequence: 7,
        command: Command::Damage {
            structure_id,
            amount: 25,
        },
    })
    .unwrap();
    let received = pump(&mut restored, &mut [&mut bob], 3);
    assert!(
        received[0]
            .iter()
            .any(|message| matches!(message, ServerMessage::SandboxRejected { sequence: 7, .. }))
    );
    assert_eq!(
        restored.sandbox().structures[&structure_id].hitpoints,
        hitpoints
    );
}

#[test]
fn small_pack_grounds_spawn_and_blocks_movement_outside_coverage() {
    struct SmallPack {
        center: GeoPosition,
    }
    impl terra_net::WorldEnvironment for SmallPack {
        fn ground_height_m(&self, position: GeoPosition) -> Option<f64> {
            self.can_enter(position).then_some(17.0)
        }
        fn can_enter(&self, position: GeoPosition) -> bool {
            position.surface_distance_m(self.center) < 1.0
        }
    }
    let mut server = server();
    let spawn = server.config().spawn;
    server.set_environment(Box::new(SmallPack { center: spawn }));
    let mut alice = NetClient::connect(server.local_addr().unwrap(), spawn).unwrap();
    let mut bob = NetClient::connect(server.local_addr().unwrap(), spawn).unwrap();
    let received = pump(&mut server, &mut [&mut alice, &mut bob], 4);
    let (bob_id, _) = welcome(&received[1]);
    for messages in &received {
        assert!(messages.iter().any(|message| matches!(message, ServerMessage::Welcome { spawn, .. } if spawn.altitude_m == 17.0)));
    }
    bob.send(ClientMessage::Move {
        sequence: 1,
        velocity_enu_m_s: [10.0, 0.0, 0.0],
    })
    .unwrap();
    pump(&mut server, &mut [&mut alice, &mut bob], 12);
    assert!(server.actors()[&bob_id].position.surface_distance_m(spawn) < 1.0);
    assert_eq!(server.actors()[&bob_id].velocity_enu_m_s, [0.0; 3]);
}

#[test]
fn historical_epoch_and_political_revision_are_pinned_in_saves() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("historical.json");
    let mut config = ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    config.manifest.terrain_provider = "historical-pack".into();
    config.manifest.base_data_revision = "fixture-era-b:sha256:abc123".into();
    config.manifest.data_epoch_unix = Some(946684800);
    config.manifest.political_dataset_revision = Some("fixture-politics-v1".into());
    config.initial_unix_seconds = Some(946684800.0);
    let initial = Server::bind(config.clone()).unwrap();
    initial.save_world(&path).unwrap();
    assert_eq!(initial.clock().unix_seconds, 946684800.0);
    config.manifest.data_epoch_unix = Some(978307200);
    assert!(
        Server::bind(config.clone())
            .unwrap()
            .load_world(&path)
            .is_err()
    );
    config.manifest.data_epoch_unix = Some(946684800);
    config.manifest.political_dataset_revision = Some("different-politics".into());
    assert!(Server::bind(config).unwrap().load_world(&path).is_err());
}

#[test]
fn custom_game_catalog_and_new_player_equipment_survive_restart() {
    use std::collections::{BTreeMap, BTreeSet};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("custom-game.json");
    let mut initial = server();
    let catalog = terra_sandbox::Catalog {
        items: BTreeSet::from(["energy".to_owned()]),
        recipes: BTreeMap::new(),
        blueprints: BTreeMap::new(),
        crops: BTreeMap::new(),
    };
    assert!(initial.set_catalog(catalog.clone()).is_err());
    initial
        .set_catalog_and_starting_inventory(catalog, BTreeMap::from([("energy".to_owned(), 7)]))
        .unwrap();
    assert!(
        initial
            .seed_resource("bad-node".into(), initial.config().spawn, "wood", 10, 0.0)
            .is_err()
    );
    initial.save_world(&path).unwrap();
    drop(initial);
    let mut restored = server();
    restored.load_world(path).unwrap();
    let mut client =
        NetClient::connect(restored.local_addr().unwrap(), restored.config().spawn).unwrap();
    let received = pump(&mut restored, &mut [&mut client], 4);
    let (player_id, _) = welcome(&received[0]);
    assert_eq!(
        restored.sandbox().players[&player_id].inventory,
        BTreeMap::from([("energy".to_owned(), 7)])
    );
    restored
        .save_world(directory.path().join("after-join.json"))
        .unwrap();
}

#[test]
fn incompatible_data_is_rejected_before_allocating_a_saved_identity() {
    let mut server = server();
    let mut expected = server.config().manifest.clone();
    expected.data_epoch_unix = Some(946684800);
    let mut client = NetClient::connect_with_manifest(
        server.local_addr().unwrap(),
        server.config().spawn,
        None,
        expected,
    )
    .unwrap();
    server.update(0.05);
    let messages = client.poll().unwrap();
    assert!(messages.iter().any(
        |message| matches!(message, ServerMessage::Error { code, .. } if code == "world_mismatch")
    ));
    assert!(server.sandbox().players.is_empty());
    assert!(server.actors().is_empty());
}
