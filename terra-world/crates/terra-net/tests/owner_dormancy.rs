//! Session disconnect and real TCP resume across cold pages and host restart.
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, Instant},
};
use terra_core::{ActorKind, LocalFrame};
use terra_net::{NetClient, Server, ServerConfig, ServerMessage};
use terra_sandbox::{Catalog, LandPlot, SandboxState, Structure};

fn host() -> Server {
    Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    })
    .unwrap()
}
fn hello(server: &mut Server, client: &mut NetClient) -> (u64, String) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        // Admission/persistence work is polled without advancing the newly
        // awakened actor, so exact restoration is distinct from its next tick.
        let report = server.update(0.);
        assert_eq!(report.domain_errors, 0, "{report:?}");
        assert!(report.persistence_error.is_none(), "{report:?}");
        for message in client.poll().unwrap() {
            if let ServerMessage::Welcome {
                actor_id,
                resume_token,
                ..
            } = message
            {
                return (actor_id, resume_token);
            }
        }
        assert!(Instant::now() < deadline, "session admission stalled");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn disconnect(server: &mut Server, player: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = server.update(0.025);
        assert_eq!(report.domain_errors, 0, "{report:?}");
        assert!(report.persistence_error.is_none(), "{report:?}");
        if server.sandbox().is_player_dormant(player) && !server.persistence_pending() {
            break;
        }
        assert!(Instant::now() < deadline, "disconnect checkpoint stalled");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn read_sandbox(path: &std::path::Path) -> SandboxState {
    let document: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    serde_json::from_value(document["sandbox"].clone()).unwrap()
}

#[test]
fn disconnected_owner_retires_bodies_home_and_resumes_exact_identity_after_host_restart() {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("private-world.json");
    let database = dir.path().join("world.db");
    let export = dir.path().join("after-offline.json");
    let catalog = Catalog::builtin();
    let mut initial = host();
    initial.enable_living_demo(&[]).unwrap();
    let mut client =
        NetClient::connect(initial.local_addr().unwrap(), initial.config().spawn).unwrap();
    let (player, token) = hello(&mut initial, &mut client);
    drop(client);
    disconnect(&mut initial, player);
    initial.save_world(&json).unwrap();
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&json).unwrap()).unwrap();
    let mut fixture: SandboxState = serde_json::from_value(document["sandbox"].clone()).unwrap();
    let mut remote = LocalFrame::new(initial.config().spawn)
        .to_geo([30_000., 0., 0.])
        .unwrap();
    // The reference host uses a flat authoritative surface. A long tangent
    // displacement must be grounded before saving this synthetic settlement.
    remote.altitude_m = initial.config().spawn.altitude_m;
    for body in fixture
        .living
        .units
        .values_mut()
        .filter(|u| u.owner == Some(player))
    {
        body.position = remote;
        if matches!(
            body.kind,
            ActorKind::Helicopter | ActorKind::Airplane | ActorKind::Balloon
        ) {
            body.position.altitude_m += 18.;
        }
        body.home = remote;
        body.heading_deg = 137.;
        body.needs.food = 0.61;
        body.needs.water = 0.49;
        body.needs.energy = 0.37;
    }
    fixture.players.get_mut(&player).unwrap().position = remote;
    let plot = fixture.next_id;
    let house = plot + 1;
    fixture.next_id += 2;
    fixture.plots.insert(
        plot,
        LandPlot {
            id: plot,
            owner: player,
            faction_owner: None,
            center: remote,
            radius_m: 50.,
        },
    );
    let blueprint = &catalog.blueprints["house"];
    fixture.structures.insert(
        house,
        Structure {
            id: house,
            owner: player,
            faction_owner: None,
            blueprint: "house".into(),
            position: remote,
            hitpoints: blueprint.hitpoints,
            max_hitpoints: blueprint.hitpoints,
            radius_m: blueprint.radius_m,
            housing: blueprint.housing,
            workbench: blueprint.workbench,
        },
    );
    fixture.players.get_mut(&player).unwrap().home = Some(house);
    fixture.validate_with_catalog(&catalog).unwrap();
    let bodies: BTreeMap<_, _> = fixture
        .living
        .units
        .iter()
        .filter(|(_, u)| u.owner == Some(player))
        .map(|(&id, unit)| (id, unit.clone()))
        .collect();
    let principal = fixture.players[&player].clone();
    let account = fixture.living.accounts[&player].clone();
    let vehicles: BTreeMap<_, _> = fixture
        .transport
        .vehicles
        .iter()
        .filter(|(_, v)| v.owner == player)
        .map(|(&id, v)| (id, v.clone()))
        .collect();
    document["sandbox"] = serde_json::to_value(&fixture).unwrap();
    fs::write(&json, serde_json::to_vec(&document).unwrap()).unwrap();
    drop(initial);

    let mut stored = host();
    stored.load_world(&json).unwrap();
    stored.open_region_store(&database).unwrap();
    stored.checkpoint_regions().unwrap();
    assert!(stored.sandbox().is_player_dormant(player));
    assert!(
        bodies
            .keys()
            .all(|id| !stored.sandbox().living.units.contains_key(id))
    );
    assert!(!stored.sandbox().structures.contains_key(&house));
    let dormant = stored.region_store_status().unwrap();
    assert!(dormant.dormant_records >= bodies.len() + 1);
    for _ in 0..120 {
        let report = stored.update(0.25);
        assert_eq!(report.domain_errors, 0, "{report:?}");
        assert!(report.persistence_error.is_none(), "{report:?}");
    }
    stored.save_world(&export).unwrap();
    let offline = read_sandbox(&export);
    assert_eq!(offline.players[&player], principal);
    assert_eq!(offline.living.accounts[&player], account);
    for (&id, body) in &bodies {
        assert_eq!(&offline.living.units[&id], body);
    }
    for (&id, vehicle) in &vehicles {
        assert_eq!(&offline.transport.vehicles[&id], vehicle);
    }
    stored.checkpoint_regions().unwrap();
    let previous_generation = stored.region_store_status().unwrap().generation;
    drop(stored);

    let mut restarted = host();
    restarted.open_region_store(&database).unwrap();
    restarted.checkpoint_regions().unwrap();
    assert!(restarted.region_store_status().unwrap().generation > previous_generation);
    assert!(
        bodies
            .keys()
            .all(|id| !restarted.sandbox().living.units.contains_key(id))
    );
    let mut resumed =
        NetClient::connect_with_token(restarted.local_addr().unwrap(), remote, Some(token.clone()))
            .unwrap();
    let (same_player, same_token) = hello(&mut restarted, &mut resumed);
    assert_eq!(same_player, player);
    assert_eq!(same_token, token);
    assert!(!restarted.sandbox().is_player_dormant(player));
    assert_eq!(restarted.sandbox().players[&player], principal);
    assert_eq!(restarted.sandbox().living.accounts[&player], account);
    assert_eq!(
        restarted.actors()[&player].heading_deg,
        bodies[&account.active_unit].heading_deg
    );
    assert!(restarted.sandbox().structures.contains_key(&house));
    for (&id, body) in &bodies {
        assert_eq!(&restarted.sandbox().living.units[&id], body);
    }
    for (&id, vehicle) in &vehicles {
        assert_eq!(&restarted.sandbox().transport.vehicles[&id], vehicle);
    }
    drop(resumed);
    disconnect(&mut restarted, player);
    assert!(
        bodies
            .keys()
            .all(|id| !restarted.sandbox().living.units.contains_key(id))
    );
}
