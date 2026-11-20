use std::time::{Duration, Instant};
use terra_core::GeoPosition;
use terra_net::{NetClient, Server, ServerConfig, ServerMessage, WorldEnvironment};

fn host() -> Server {
    Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn successive_principals_receive_complete_unoccupied_crews() {
    let mut host = host();
    host.enable_living_demo(&[]).unwrap();
    for _ in 0..8 {
        let (receipt, _) = host.provision_principal().unwrap();
        let units: Vec<_> = host
            .sandbox()
            .living
            .units
            .values()
            .filter(|u| u.owner == Some(receipt.player_id))
            .collect();
        assert_eq!(units.len(), terra_sandbox::STARTER_UNIT_COUNT);
        for unit in units {
            assert_eq!(
                host.sandbox().spawn_obstruction(unit).unwrap(),
                None,
                "{} overlaps",
                unit.name
            );
        }
    }
}

struct TinyIsland {
    center: GeoPosition,
}
impl WorldEnvironment for TinyIsland {
    fn ground_height_m(&self, _: GeoPosition) -> Option<f64> {
        Some(self.center.altitude_m)
    }
    fn can_enter(&self, p: GeoPosition) -> bool {
        self.center.surface_distance_m(p) < 1.
    }
}
#[test]
fn an_avatar_only_island_cannot_create_a_partial_or_unsupported_crew() {
    let mut host = host();
    host.enable_living_demo(&[]).unwrap();
    host.set_environment(Box::new(TinyIsland {
        center: host.config().spawn,
    }));
    let before = serde_json::to_value(host.sandbox()).unwrap();
    let error = host
        .provision_principal()
        .err()
        .expect("unsupported crew must fail")
        .to_string();
    assert!(error.contains("crew placement"), "{error}");
    assert_eq!(serde_json::to_value(host.sandbox()).unwrap(), before);
}

struct ChangedSurface {
    height: f64,
}
impl WorldEnvironment for ChangedSurface {
    fn ground_height_m(&self, _: GeoPosition) -> Option<f64> {
        Some(self.height)
    }
}
#[test]
fn reconnect_keeps_saved_body_positions_even_when_the_surface_is_unavailable() {
    let mut host = host();
    host.enable_living_demo(&[]).unwrap();
    let (receipt, identity) = host.provision_principal().unwrap();
    // Changing the provider must not relocate a returning saved crew.
    host.set_environment(Box::new(ChangedSurface { height: 500. }));
    let positions: Vec<_> = host
        .sandbox()
        .living
        .units
        .values()
        .filter(|u| u.owner == Some(receipt.player_id))
        .map(|u| (u.id, u.position))
        .collect();
    let mut client = NetClient::connect_with_token(
        host.local_addr().unwrap(),
        host.config().spawn,
        Some(identity.resume_token),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        host.update(0.);
        if client
            .poll()
            .unwrap()
            .iter()
            .any(|m| matches!(m, ServerMessage::Welcome { .. }))
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    for (id, p) in positions {
        assert_eq!(host.sandbox().living.units[&id].position, p);
    }
}

#[test]
fn regional_operator_provision_checks_cold_bodies_before_committing_a_new_crew() {
    let directory = tempfile::tempdir().unwrap();
    let snapshot = directory.path().join("initial.json");
    let exported = directory.path().join("complete.json");
    let database = directory.path().join("world.db");
    let mut initial = host();
    initial.enable_living_demo(&[]).unwrap();
    let (first, _) = initial.provision_principal().unwrap();
    initial.save_world(&snapshot).unwrap();
    // Establish where the next crew would be admitted without the cold body.
    let (probe, _) = initial.provision_principal().unwrap();
    let next_rover_position = initial
        .sandbox()
        .living
        .units
        .values()
        .find(|u| u.owner == Some(probe.player_id) && u.kind == terra_core::ActorKind::Vehicle)
        .unwrap()
        .position;
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&snapshot).unwrap()).unwrap();
    let mut complete: terra_sandbox::SandboxState =
        serde_json::from_value(document["sandbox"].clone()).unwrap();
    let old_rover = complete
        .living
        .units
        .values()
        .find(|u| u.owner == Some(first.player_id) && u.kind == terra_core::ActorKind::Vehicle)
        .unwrap()
        .id;
    complete.living.units.get_mut(&old_rover).unwrap().position = next_rover_position;
    complete.living.units.get_mut(&old_rover).unwrap().home = next_rover_position;
    let far = terra_core::LocalFrame::new(initial.config().spawn)
        .to_geo([30_000., 0., 0.])
        .unwrap();
    // The global couriers normally pin nearby pages. Move only these idle test
    // couriers away so the arrival area's dormant bodies really leave memory.
    for unit in complete
        .living
        .units
        .values_mut()
        .filter(|u| u.role == terra_sandbox::UnitRole::Hauler && u.owner.is_none())
    {
        unit.position = far;
        unit.home = far;
        unit.order = terra_sandbox::UnitOrder::Idle;
        unit.path.clear();
        unit.path_index = 0;
    }
    complete
        .validate_with_catalog(&terra_sandbox::Catalog::builtin())
        .unwrap();
    document["sandbox"] = serde_json::to_value(complete).unwrap();
    std::fs::write(&snapshot, serde_json::to_vec(&document).unwrap()).unwrap();
    drop(initial);
    let mut server = host();
    server.load_world(&snapshot).unwrap();
    server.open_region_store(&database).unwrap();
    server.checkpoint_regions().unwrap();
    assert!(server.sandbox().is_partial());
    assert!(!server.sandbox().living.units.contains_key(&old_rover));
    let (created, _) = server.provision_principal().unwrap();
    server.save_world(&exported).unwrap();
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(exported).unwrap()).unwrap();
    let complete: terra_sandbox::SandboxState =
        serde_json::from_value(document["sandbox"].clone()).unwrap();
    assert_eq!(
        complete.living.units[&old_rover].position,
        next_rover_position
    );
    for unit in complete
        .living
        .units
        .values()
        .filter(|u| u.owner == Some(created.player_id))
    {
        assert_eq!(
            complete.spawn_obstruction(unit).unwrap(),
            None,
            "new crew overlaps a cold body"
        );
    }
}
