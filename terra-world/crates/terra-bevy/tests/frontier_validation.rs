//! Exercise the native network adapter through real framed loopback messages.
//! Full saves and bounded replicas have distinct validation requirements.
use bevy::{prelude::*, time::TimeUpdateStrategy};
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use terra_bevy::{
    NetworkStatus, SandboxRuntime, TerraLivingPlugin, TerraNetworkPlugin, TerraWorldPlugin,
    WorldConfig, WorldRuntime,
};
use terra_core::GeoPosition;
use terra_net::{BaseWorldManifest, PROTOCOL_VERSION, SandboxUpdate, ServerMessage, encode_frame};
use terra_sandbox::{Catalog, PlayerProgress, SandboxState, SandboxView};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct Replica {
    app: App,
    peer: TcpStream,
    directory: PathBuf,
}

impl Replica {
    fn new(origin: GeoPosition) -> Self {
        let serial = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "terra-frontier-replica-{}-{serial}",
            std::process::id()
        ));
        let config = WorldConfig {
            focus: origin,
            chunk_radius: 0,
            max_resident_chunks: 4,
            max_queued: 4,
            max_in_flight: 1,
            terrain_resolution: 9,
            live_data: false,
            live_weather: false,
            cache_dir: directory.join("cache"),
            save_path: directory.join("unused.json"),
            ..Default::default()
        };
        let manifest = BaseWorldManifest {
            seed: config.seed,
            base_data_revision: config.base_data_revision.clone(),
            terrain_provider: "procedural".into(),
            chunk_level: config.chunk_level,
            terrain_resolution: config.terrain_resolution,
            data_epoch_unix: config.data_epoch_unix,
            political_dataset_revision: config.political_dataset_revision.clone(),
            ..Default::default()
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .add_plugins((TerraWorldPlugin::new(config), TerraLivingPlugin))
            .add_plugins(TerraNetworkPlugin {
                address: listener.local_addr().unwrap(),
                interest: origin,
                resume_token: None,
            });
        let deadline = Instant::now() + Duration::from_secs(5);
        let peer = loop {
            app.update();
            match listener.accept() {
                Ok((peer, _)) => break peer,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("loopback accept failed: {error}"),
            }
            assert!(Instant::now() < deadline, "native client did not connect");
            std::thread::sleep(Duration::from_millis(1));
        };
        peer.set_nonblocking(false).unwrap();
        peer.set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut fixture = Self {
            app,
            peer,
            directory,
        };
        fixture.send(ServerMessage::Welcome {
            protocol: PROTOCOL_VERSION,
            client_id: 1,
            actor_id: 1,
            manifest,
            tick: 1,
            unix_seconds: 1_760_000_000.,
            time_scale: 60.,
            paused: false,
            tick_hz: 20,
            spawn: origin,
            resume_token: "a".repeat(64),
            next_sandbox_sequence: 1,
        });
        fixture.until(|app| app.world().resource::<NetworkStatus>().connected);
        fixture
    }

    fn send(&mut self, message: ServerMessage) {
        self.peer
            .write_all(&encode_frame(&message).unwrap())
            .unwrap();
    }

    fn send_sandbox(&mut self, revision: u64, view: SandboxView) {
        // Protocol 10 carries revisioned updates. SandboxSnapshot is the SDK's
        // normalized output event, never a peer shortcut around validation.
        self.send(ServerMessage::SandboxUpdate {
            update: SandboxUpdate {
                revision,
                base_revision: None,
                tick: revision,
                full: Some(view),
                delta: None,
            },
        });
    }

    fn until(&mut self, complete: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.app.update();
            if complete(&self.app) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "native adapter timed out: {}",
                self.app.world().resource::<NetworkStatus>().message
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Drop for Replica {
    fn drop(&mut self) {
        // Stop the asynchronous source worker before removing its small cache.
        self.app = App::new();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn view() -> SandboxView {
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let catalog = Catalog::builtin();
    let mut state = SandboxState::new();
    state.add_player(1, origin).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    state.seed_ecology_near(origin, &catalog).unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    state.snapshot_for(1, 5000.).unwrap()
}

#[test]
fn owned_wildlife_is_accepted_and_native_replica_never_runs_host_ecology() {
    let mut view = view();
    let animal = view.ecology.animals[0].unit_id;
    view.living
        .units
        .iter_mut()
        .find(|u| u.id == animal)
        .unwrap()
        .owner = Some(1);
    view.living.validate_for(1).unwrap();
    let expected_animals = view.ecology.animals.len();
    let expected_resources = view.resource_nodes.len();
    let mut replica = Replica::new(view.player.position);
    replica.send_sandbox(1, view);
    replica.until(|app| {
        app.world()
            .resource::<NetworkStatus>()
            .replication
            .keyframes
            == 1
            || !app.world().resource::<NetworkStatus>().connected
    });
    assert!(
        replica.app.world().resource::<NetworkStatus>().connected,
        "{}",
        replica.app.world().resource::<NetworkStatus>().message
    );
    assert!(
        replica
            .app
            .world()
            .resource::<WorldRuntime>()
            .network_authority
    );
    let before = replica
        .app
        .world()
        .resource::<SandboxRuntime>()
        .state
        .clone();
    assert_eq!(before.living.units[&animal].owner, Some(1));
    for _ in 0..40 {
        replica.app.update();
    }
    assert_eq!(
        replica
            .app
            .world()
            .resource::<terra_bevy::EcologyRuntime>()
            .scheduler
            .stats()
            .passes,
        0
    );
    assert_eq!(
        replica
            .app
            .world()
            .resource::<terra_bevy::WorldDiagnostics>()
            .stage(terra_bevy::WorldStage::Ecology)
            .total_samples,
        0
    );
    let after = &replica.app.world().resource::<SandboxRuntime>().state;
    assert_eq!(after.ecology.animals.len(), expected_animals);
    assert_eq!(after.resource_nodes.len(), expected_resources);
    assert_eq!(
        after.ecology, before.ecology,
        "client frame time must not advance population, AI or recovery"
    );
    assert_eq!(
        after.living, before.living,
        "client frame time must not advance economy or actor needs"
    );
    assert_eq!(
        after.settlements, before.settlements,
        "client must not create host work contracts"
    );
}

#[test]
fn malformed_ecology_references_and_another_players_progress_are_rejected_atomically() {
    let baseline = view();
    let mut missing_body = baseline.clone();
    let id = missing_body.ecology.animals[0].unit_id;
    missing_body.living.units.retain(|u| u.id != id);
    let mut wrong_item = baseline.clone();
    let node_id = wrong_item.ecology.resources[0].node_id.clone();
    let node = wrong_item
        .resource_nodes
        .iter_mut()
        .find(|n| n.id == node_id)
        .unwrap();
    node.item = if node.item == "stone" {
        "wood"
    } else {
        "stone"
    }
    .into();
    let mut wrong_progress = baseline.clone();
    wrong_progress.settlements.progress = Some(PlayerProgress {
        player_id: 2,
        ..Default::default()
    });
    for (name, invalid, expected_rejection) in [
        (
            "missing animal body",
            missing_body,
            "incoherent ecology references in replication",
        ),
        (
            "resource metadata mismatch",
            wrong_item,
            "incoherent ecology references in replication",
        ),
        (
            "another player's progress",
            wrong_progress,
            "private progress view",
        ),
    ] {
        let mut replica = Replica::new(baseline.player.position);
        // Install a legitimate baseline first. A rejected second keyframe must
        // preserve actual replicated state, not merely an empty local fixture.
        replica.send_sandbox(1, baseline.clone());
        replica.until(|app| {
            app.world()
                .resource::<NetworkStatus>()
                .replication
                .keyframes
                == 1
                || !app.world().resource::<NetworkStatus>().connected
        });
        assert!(replica.app.world().resource::<NetworkStatus>().connected);
        let before = replica
            .app
            .world()
            .resource::<SandboxRuntime>()
            .state
            .clone();
        replica.send_sandbox(2, invalid);
        replica.until(|app| !app.world().resource::<NetworkStatus>().connected);
        let status = replica.app.world().resource::<NetworkStatus>();
        assert!(
            status.message.contains(expected_rejection),
            "{name}: {}",
            status.message
        );
        assert_eq!(
            replica.app.world().resource::<SandboxRuntime>().state,
            before,
            "{name}: no partial replica replacement"
        );
    }
}
