//! Trusted operator API stays private, durable and failure-atomic.
use std::time::{Duration, Instant};
use terra_net::{ClientMessage, NetClient, OperatorAction, Server, ServerConfig, ServerMessage};

fn host() -> Server {
    Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    })
    .unwrap()
}
fn poll_until(
    server: &mut Server,
    client: &mut NetClient,
    predicate: impl Fn(&ServerMessage) -> bool,
) -> Vec<ServerMessage> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut messages = Vec::new();
    loop {
        let report = server.update(0.);
        assert!(report.persistence_error.is_none(), "{report:?}");
        match client.poll() {
            Ok(more) => messages.extend(more),
            Err(_error) if messages.iter().any(&predicate) => return messages,
            Err(error) => panic!("unexpected network error: {error}"),
        }
        if messages.iter().any(&predicate) {
            return messages;
        }
        assert!(Instant::now() < deadline, "network fixture timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn hello(server: &mut Server, client: &mut NetClient) -> (u64, String) {
    poll_until(server, client, |m| {
        matches!(m, ServerMessage::Welcome { .. })
    })
    .iter()
    .find_map(|m| match m {
        ServerMessage::Welcome {
            actor_id,
            resume_token,
            ..
        } => Some((*actor_id, resume_token.clone())),
        _ => None,
    })
    .unwrap()
}
fn settle(server: &mut Server) {
    let deadline = Instant::now() + Duration::from_secs(10);
    for _ in 0.. {
        let report = server.update(0.);
        assert!(report.persistence_error.is_none(), "{report:?}");
        if !server.persistence_pending() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn resume_only_host_provisions_a_durable_dormant_principal_without_guest_registration() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("provisioned.db");
    let config = ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        allow_new_players: false,
        ..Default::default()
    };
    let mut server = Server::bind(config.clone()).unwrap();
    server.enable_living_demo(&[]).unwrap();
    server.open_region_store(&database).unwrap();
    let (receipt, identity) = server.provision_principal().unwrap();
    assert_eq!(receipt.player_id, 1);
    assert!(receipt.committed_generation.is_some());
    assert_eq!(receipt.disconnected_peers, 0);
    let status = server.principal_status(receipt.player_id).unwrap();
    assert!(status.dormant && !status.connected && !status.suspended);
    assert!(!server.config().allow_new_players);
    let before = server.sandbox().players[&receipt.player_id].clone();
    let money = server.sandbox().living.accounts[&receipt.player_id].money;
    let token = identity.resume_token.clone();
    identity
        .save(directory.path().join("new-account.json"))
        .unwrap();
    let public = serde_json::to_string(&server.operator_audit().collect::<Vec<_>>()).unwrap();
    assert!(!public.contains(&token));
    assert_eq!(
        server.operator_audit().last().unwrap().action,
        OperatorAction::Provision
    );
    drop(server);

    let mut server = Server::bind(config).unwrap();
    server.open_region_store(&database).unwrap();
    settle(&mut server);
    assert!(server.principal_status(receipt.player_id).unwrap().dormant);
    let mut guest =
        NetClient::connect(server.local_addr().unwrap(), server.config().spawn).unwrap();
    let messages = poll_until(
        &mut server,
        &mut guest,
        |message| matches!(message, ServerMessage::Error { code, .. } if code == "registration_closed"),
    );
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, ServerMessage::Welcome { .. }))
    );
    drop(guest);
    let mut client = NetClient::connect_with_token(
        server.local_addr().unwrap(),
        server.config().spawn,
        Some(token.clone()),
    )
    .unwrap();
    assert_eq!(hello(&mut server, &mut client), (receipt.player_id, token));
    assert_eq!(server.sandbox().players.len(), 1);
    assert_eq!(server.sandbox().players[&receipt.player_id], before);
    assert_eq!(
        server.sandbox().living.accounts[&receipt.player_id].money,
        money
    );
    assert!(
        server
            .principal_status(receipt.player_id)
            .unwrap()
            .connected
    );
}

#[test]
fn failed_provisioning_does_not_consume_ids_capabilities_or_change_cold_residency() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("provision-lock.db");
    let mut server = host();
    server.enable_living_demo(&[]).unwrap();
    let far = terra_core::GeoPosition::new(40., -74., 0.).unwrap();
    let chunk = terra_core::WorldChunk {
        key: terra_core::TileKey::at(far, 15).unwrap(),
        heights: terra_core::HeightField::constant(0.),
        features: vec![terra_core::WorldFeature {
            id: "provision-cold-tree".into(),
            kind: terra_core::FeatureKind::Tree,
            points: vec![far],
            height_m: 6.,
            properties: Default::default(),
        }],
        sources: vec![],
        warnings: vec![],
    };
    server.try_register_chunk(&chunk).unwrap();
    server.open_region_store(&database).unwrap();
    server.try_evict_chunk(chunk.key).unwrap();
    server.checkpoint_regions().unwrap();
    let (first, first_identity) = server.provision_principal().unwrap();
    assert!(
        server.sandbox().is_partial(),
        "the fixture retains a cold environmental page"
    );
    let before = server.sandbox().clone();
    let store = server.region_store_status();
    let audit = server.operator_audit().cloned().collect::<Vec<_>>();
    let lock = rusqlite::Connection::open(&database).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(server.provision_principal().is_err());
    assert_eq!(*server.sandbox(), before);
    assert_eq!(server.region_store_status(), store);
    assert_eq!(server.operator_audit().cloned().collect::<Vec<_>>(), audit);
    lock.execute_batch("ROLLBACK").unwrap();
    let (next, identity) = server.provision_principal().unwrap();
    assert_eq!(next.player_id, first.player_id + 1);
    assert_ne!(identity.resume_token, first_identity.resume_token);
    assert!(next.committed_generation > first.committed_generation);
    let mut client = NetClient::connect_with_token(
        server.local_addr().unwrap(),
        server.config().spawn,
        Some(first_identity.resume_token.clone()),
    )
    .unwrap();
    assert_eq!(
        hello(&mut server, &mut client),
        (first.player_id, first_identity.resume_token)
    );
}

#[test]
fn provisioning_enforces_capacity_and_endpoint_before_any_state_change() {
    let mut server = host();
    for address in ["0.0.0.0:7777", "127.0.0.1:0", "224.0.0.1:7777", "[::]:7777"] {
        assert!(
            server
                .provision_principal_for(address.parse().unwrap())
                .is_err()
        );
        assert!(server.sandbox().players.is_empty());
        assert_eq!(server.operator_audit().count(), 0);
    }
    let advertised = "127.0.0.1:8888".parse().unwrap();
    for id in 1..=terra_sandbox::MAX_PLAYERS as u64 {
        let (receipt, identity) = server.provision_principal_for(advertised).unwrap();
        assert_eq!(receipt.player_id, id);
        assert!(receipt.committed_generation.is_none());
        assert_eq!(identity.server, advertised);
        assert!(server.principal_status(id).unwrap().dormant);
    }
    let before = server.sandbox().clone();
    assert!(server.provision_principal_for(advertised).is_err());
    assert_eq!(*server.sandbox(), before);
    assert_eq!(server.operator_audit().count(), terra_sandbox::MAX_PLAYERS);
}

#[test]
fn suspension_and_rotation_are_durable_private_and_preserve_the_same_principal() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("operator.db");
    let mut server = host();
    server.enable_living_demo(&[]).unwrap();
    server.open_region_store(&database).unwrap();
    let mut client =
        NetClient::connect(server.local_addr().unwrap(), server.config().spawn).unwrap();
    let (player, old_token) = hello(&mut server, &mut client);
    settle(&mut server);
    let inventory = server.sandbox().players[&player].inventory.clone();
    let money = server.sandbox().living.accounts[&player].money;
    let receipt = server.set_principal_suspended(player, true).unwrap();
    assert!(receipt.committed_generation.is_some());
    assert_eq!(receipt.disconnected_peers, 1);
    assert!(server.principal_status(player).unwrap().suspended);
    assert!(server.principal_status(player).unwrap().dormant);
    let generation = receipt.committed_generation.unwrap();
    drop(client);
    let mut rejected = NetClient::connect_with_token(
        server.local_addr().unwrap(),
        server.config().spawn,
        Some(old_token.clone()),
    )
    .unwrap();
    let messages = poll_until(
        &mut server,
        &mut rejected,
        |m| matches!(m, ServerMessage::Error { code, .. } if code == "access"),
    );
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, ServerMessage::Welcome { .. }))
    );
    assert_eq!(
        server.region_store_status().unwrap().generation,
        generation,
        "suspended Hello is rejected before hydration or checkpoint"
    );
    drop(rejected);
    let (rotation, identity) = server.rotate_resume_capability(player).unwrap();
    assert!(rotation.committed_generation.unwrap() > generation);
    assert_ne!(identity.resume_token, old_token);
    let new_token = identity.resume_token;
    assert!(server.principal_status(player).unwrap().suspended);
    drop(server);

    let mut server = host();
    server.open_region_store(&database).unwrap();
    settle(&mut server);
    assert!(server.principal_status(player).unwrap().suspended);
    server.set_principal_suspended(player, false).unwrap();
    assert!(
        server.principal_status(player).unwrap().dormant,
        "allow does not awaken bodies without an authenticated connection"
    );
    let mut old = NetClient::connect_with_token(
        server.local_addr().unwrap(),
        server.config().spawn,
        Some(old_token),
    )
    .unwrap();
    poll_until(
        &mut server,
        &mut old,
        |m| matches!(m, ServerMessage::Error { code, .. } if code == "resume"),
    );
    drop(old);
    let mut resumed = NetClient::connect_with_token(
        server.local_addr().unwrap(),
        server.config().spawn,
        Some(new_token.clone()),
    )
    .unwrap();
    assert_eq!(hello(&mut server, &mut resumed), (player, new_token));
    assert_eq!(server.sandbox().players.len(), 1);
    assert_eq!(server.sandbox().players[&player].inventory, inventory);
    assert_eq!(server.sandbox().living.accounts[&player].money, money);
    assert_eq!(
        server
            .operator_audit()
            .map(|entry| entry.action)
            .collect::<Vec<_>>(),
        vec![
            OperatorAction::Suspend,
            OperatorAction::RotateCapability,
            OperatorAction::Allow
        ]
    );
}

#[test]
fn failed_operator_commit_preserves_connected_peer_policy_money_and_capability() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("locked.db");
    let mut server = host();
    server.enable_living_demo(&[]).unwrap();
    server.open_region_store(&database).unwrap();
    let mut client =
        NetClient::connect(server.local_addr().unwrap(), server.config().spawn).unwrap();
    let (player, token) = hello(&mut server, &mut client);
    settle(&mut server);
    let before = server.sandbox().clone();
    let status = server.region_store_status();
    let lock = rusqlite::Connection::open(&database).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(server.set_principal_suspended(player, true).is_err());
    assert!(server.rotate_resume_capability(player).is_err());
    assert_eq!(*server.sandbox(), before);
    assert_eq!(server.region_store_status(), status);
    assert!(server.principal_status(player).unwrap().connected);
    assert!(!server.principal_status(player).unwrap().suspended);
    assert_eq!(server.operator_audit().count(), 0);
    lock.execute_batch("ROLLBACK").unwrap();
    client.send(ClientMessage::Ping { nonce: 42 }).unwrap();
    poll_until(&mut server, &mut client, |m| {
        matches!(m, ServerMessage::Pong { nonce: 42, .. })
    });
    drop(client);
    settle(&mut server);
    let mut client = NetClient::connect_with_token(
        server.local_addr().unwrap(),
        server.config().spawn,
        Some(token.clone()),
    )
    .unwrap();
    assert_eq!(hello(&mut server, &mut client), (player, token));
}

#[test]
fn operator_target_is_exact_and_audit_is_bounded_without_exposing_capabilities() {
    let mut server = host();
    let mut client =
        NetClient::connect(server.local_addr().unwrap(), server.config().spawn).unwrap();
    let (player, token) = hello(&mut server, &mut client);
    assert!(server.set_principal_suspended(player + 100, true).is_err());
    assert!(server.rotate_resume_capability(player + 100).is_err());
    assert!(server.principal_status(player).unwrap().connected);
    assert_eq!(server.operator_audit().count(), 0);
    for index in 0..140 {
        server
            .set_principal_suspended(player, index % 2 == 0)
            .unwrap();
    }
    assert_eq!(server.operator_audit().count(), 128);
    assert_eq!(server.operator_audit().last().unwrap().revision, 140);
    let public = serde_json::to_string(&server.operator_audit().collect::<Vec<_>>()).unwrap();
    assert!(!public.contains(&token));
    assert!(!public.contains("resume_token"));
}

#[test]
fn recovery_identity_requires_an_explicit_usable_client_endpoint_before_mutation() {
    let mut server = host();
    let mut client =
        NetClient::connect(server.local_addr().unwrap(), server.config().spawn).unwrap();
    let (player, _) = hello(&mut server, &mut client);
    for endpoint in ["0.0.0.0:7777", "127.0.0.1:0", "224.0.0.1:7777", "[::]:7777"] {
        assert!(
            server
                .rotate_resume_capability_for(player, endpoint.parse().unwrap())
                .is_err()
        );
        assert!(server.principal_status(player).unwrap().connected);
        assert_eq!(server.operator_audit().count(), 0);
    }
    let endpoint = "127.0.0.1:7777".parse().unwrap();
    let (receipt, identity) = server
        .rotate_resume_capability_for(player, endpoint)
        .unwrap();
    assert_eq!(receipt.disconnected_peers, 1);
    assert_eq!(identity.server, endpoint);
    let mut resumed = NetClient::connect_with_token(
        server.local_addr().unwrap(),
        server.config().spawn,
        Some(identity.resume_token.clone()),
    )
    .unwrap();
    assert_eq!(hello(&mut server, &mut resumed).0, player);
}

#[test]
fn resume_only_rejects_guests_invalid_tokens_and_suspended_owners_before_hydration() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("closed-registration.db");
    let mut server = host();
    server.enable_living_demo(&[]).unwrap();
    let remote = terra_core::LocalFrame::new(server.config().spawn)
        .to_geo([20_000., 0., 0.])
        .unwrap();
    server
        .seed_resource("policy:cold".into(), remote, "stone", 17, 0.)
        .unwrap();
    server.open_region_store(&database).unwrap();
    let mut client =
        NetClient::connect(server.local_addr().unwrap(), server.config().spawn).unwrap();
    let (player, token) = hello(&mut server, &mut client);
    drop(client);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !server.sandbox().is_player_dormant(player) || server.persistence_pending() {
        server.update(0.);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    server.checkpoint_regions().unwrap();
    drop(server);
    let mut server = Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        allow_new_players: false,
        ..Default::default()
    })
    .unwrap();
    server.open_region_store(&database).unwrap();
    settle(&mut server);
    assert!(server.sandbox().is_partial());
    let generation = server.region_store_status().unwrap().generation;
    for (capability, expected_code) in [
        (None, "registration_closed"),
        (Some("ab".repeat(32)), "resume"),
    ] {
        let mut rejected = NetClient::connect_with_token(
            server.local_addr().unwrap(),
            server.config().spawn,
            capability,
        )
        .unwrap();
        poll_until(
            &mut server,
            &mut rejected,
            |message| matches!(message, ServerMessage::Error { code, .. } if code==expected_code),
        );
        assert_eq!(server.sandbox().players.len(), 1);
        assert!(server.sandbox().is_partial());
        assert!(!server.persistence_pending());
        assert_eq!(server.region_store_status().unwrap().generation, generation);
    }
    let mut resumed = NetClient::connect_with_token(
        server.local_addr().unwrap(),
        server.config().spawn,
        Some(token.clone()),
    )
    .unwrap();
    assert_eq!(hello(&mut server, &mut resumed).0, player);
    settle(&mut server);
    server.set_principal_suspended(player, true).unwrap();
    let mut suspended = NetClient::connect_with_token(
        server.local_addr().unwrap(),
        server.config().spawn,
        Some(token),
    )
    .unwrap();
    poll_until(
        &mut server,
        &mut suspended,
        |message| matches!(message,ServerMessage::Error{code,..}if code=="access"),
    );
    assert_eq!(server.sandbox().players.len(), 1);
    assert!(server.principal_status(player).unwrap().suspended);
}

#[test]
fn provisioned_dormant_crews_still_count_against_living_capacity() {
    let mut server = host();
    server.enable_living_demo(&[]).unwrap();
    loop {
        let before = server.sandbox().clone();
        let audit_revision = server.operator_audit().last().map(|entry| entry.revision);
        match server.provision_principal() {
            Ok((receipt, _identity)) => {
                assert!(server.principal_status(receipt.player_id).unwrap().dormant);
                assert!(
                    !server
                        .principal_status(receipt.player_id)
                        .unwrap()
                        .connected
                );
                assert!(server.sandbox().living_unit_count() <= terra_sandbox::MAX_LIVING_UNITS);
            }
            Err(_) => {
                assert_eq!(*server.sandbox(), before);
                assert_eq!(
                    server.operator_audit().last().map(|entry| entry.revision),
                    audit_revision
                );
                assert!(server.sandbox().players.len() < terra_sandbox::MAX_PLAYERS);
                assert!(
                    server.sandbox().living_unit_count() + terra_sandbox::STARTER_UNIT_COUNT
                        > terra_sandbox::MAX_LIVING_UNITS
                );
                break;
            }
        }
    }
}

#[test]
fn provisioning_requires_traversable_resident_spawn_before_consuming_identity() {
    struct Unavailable;
    impl terra_net::WorldEnvironment for Unavailable {
        fn ground_height_m(&self, _: terra_core::GeoPosition) -> Option<f64> {
            None
        }
    }
    let mut server = host();
    server.set_environment(Box::new(Unavailable));
    let before = server.sandbox().clone();
    assert!(server.provision_principal().is_err());
    assert_eq!(*server.sandbox(), before);
    assert_eq!(server.operator_audit().count(), 0);
    server.set_environment(Box::new(terra_net::FlatEnvironment {
        ground_height_m: 0.,
        water_height_m: 0.,
    }));
    assert_eq!(server.provision_principal().unwrap().0.player_id, 1);
}
