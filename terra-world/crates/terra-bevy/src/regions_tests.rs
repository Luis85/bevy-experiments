use super::*;
use crate::{SandboxIntent, SandboxQueue, TerraWorldPlugin, WorldConfig};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use terra_core::{
    FeatureKind, GeoPosition, HeightField, TileKey, WeatherSample, WorldChunk, WorldFeature,
};
use terra_sandbox::Command;
use terra_stream::{ChunkPayload, ChunkProvider, StreamError};

struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "terra-native-activation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn config(path: &Path) -> WorldConfig {
    WorldConfig {
        focus: GeoPosition::new(48.137, 11.575, 100.).unwrap(),
        chunk_radius: 0,
        max_resident_chunks: 1,
        max_queued: 1,
        max_in_flight: 1,
        integrate_per_frame: 1,
        terrain_resolution: 9,
        time_scale: 0.,
        live_weather: false,
        region_checkpoint_seconds: 3600.,
        region_store: Some(path.join("world.db")),
        save_path: path.join("manual.json"),
        cache_dir: path.join("cache"),
        ..Default::default()
    }
}

struct Fixture {
    persistence: RegionalPersistence,
    sandbox: SandboxRuntime,
    runtime: WorldRuntime,
    politics: PoliticalWorld,
    resident: ResidentChunks,
    directory: TestDirectory,
}
impl Fixture {
    fn new(initialized: bool, dormant: bool) -> Self {
        let directory = TestDirectory::new();
        let config = config(&directory.0);
        let mut sandbox = SandboxRuntime::new(config.focus);
        sandbox.state.ecology.config.enabled = false;
        sandbox
            .state
            .seed_resource("near".into(), config.focus, "wood", 20, 0.)
            .unwrap();
        if dormant {
            sandbox
                .state
                .seed_resource(
                    "cold".into(),
                    GeoPosition::new(49., 12., 100.).unwrap(),
                    "stone",
                    17,
                    0.,
                )
                .unwrap();
        }
        let clock = WorldClock {
            paused: true,
            time_scale: 0.,
            ..Default::default()
        };
        let mut store = RegionStore::open(
            config.region_store.as_ref().unwrap(),
            local_store_identity(&config),
            &sandbox.catalog,
            config.chunk_level,
        )
        .unwrap();
        if initialized {
            let metadata = serde_json::to_value(LocalRegionMetadata {
                schema_version: 1,
                clock,
                player_id: sandbox.player_id,
                politics: None,
            })
            .unwrap();
            store
                .checkpoint(
                    &mut sandbox.state,
                    metadata,
                    &BTreeSet::from([RegionKey::at(config.focus, config.chunk_level).unwrap()]),
                )
                .unwrap();
        }
        let persistence = RegionalPersistence {
            enabled: true,
            status: Some(store.status()),
            store: Some(RegionWorker::new(store).unwrap()),
            ..Default::default()
        };
        let runtime = WorldRuntime {
            focus: config.focus,
            frame: LocalFrame::new(config.focus),
            config,
            clock,
            origin_revision: 0,
            weather: WeatherSample::default(),
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            rebases: 0,
            network_authority: false,
        };
        Self {
            persistence,
            sandbox,
            runtime,
            politics: PoliticalWorld::load(None).unwrap(),
            resident: ResidentChunks::default(),
            directory,
        }
    }

    fn checkpoint(&mut self) {
        self.persistence
            .checkpoint_sync(
                &mut self.sandbox,
                &mut self.runtime,
                &self.politics,
                &self.resident,
                "Test retirement",
            )
            .unwrap();
    }

    fn into_app(self) -> (App, TestDirectory) {
        let mut app = App::new();
        app.insert_resource(self.persistence)
            .insert_resource(self.sandbox)
            .insert_resource(self.runtime)
            .insert_resource(self.politics)
            .insert_resource(self.resident)
            .init_resource::<SandboxQueue>()
            .init_resource::<Time<bevy::time::Real>>()
            .init_resource::<crate::WorldDiagnostics>()
            .add_systems(
                Update,
                (
                    poll_regions,
                    crate::sandbox::apply_sandbox_commands,
                    checkpoint_regions,
                )
                    .chain(),
            );
        (app, self.directory)
    }
}

fn update_until(app: &mut App, condition: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.update();
        if condition(app) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native activation did not complete: {}",
            app.world().resource::<SandboxRuntime>().last_message
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn complete_activation_preserves_live_edits_and_reuses_one_admission_until_retirement() {
    let mut fixture = Fixture::new(true, false);
    assert!(!fixture.sandbox.state.is_partial());
    fixture
        .sandbox
        .state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("wood".into(), 71);
    let before = serde_json::to_value(&fixture.sandbox.state).unwrap();
    let generation = fixture.persistence.status.as_ref().unwrap().generation;
    assert!(!fixture.persistence.hydrate(&mut fixture.sandbox).unwrap());
    let request = fixture.persistence.pending.as_ref().unwrap().id;
    for _ in 0..8 {
        assert!(!fixture.persistence.hydrate(&mut fixture.sandbox).unwrap());
        assert_eq!(fixture.persistence.pending.as_ref().unwrap().id, request);
    }
    fixture
        .persistence
        .finish_pending(&mut fixture.sandbox, &mut fixture.runtime)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&fixture.sandbox.state).unwrap(),
        before
    );
    for _ in 0..8 {
        assert!(fixture.persistence.hydrate(&mut fixture.sandbox).unwrap());
        assert!(!fixture.persistence.pending());
    }
    assert_eq!(
        fixture.persistence.status.as_ref().unwrap().generation,
        generation
    );
    fixture.checkpoint();
    assert!(!fixture.persistence.activation_open);
    assert!(!fixture.persistence.hydrate(&mut fixture.sandbox).unwrap());
    assert!(fixture.persistence.pending.as_ref().unwrap().id > request);
    fixture
        .persistence
        .finish_pending(&mut fixture.sandbox, &mut fixture.runtime)
        .unwrap();
}

#[test]
fn empty_partial_and_explicitly_restored_worlds_each_finish_their_activation() {
    for (initialized, dormant) in [(false, false), (true, true)] {
        let mut fixture = Fixture::new(initialized, dormant);
        assert_eq!(fixture.sandbox.state.is_partial(), dormant);
        assert!(!fixture.persistence.hydrate(&mut fixture.sandbox).unwrap());
        fixture
            .persistence
            .finish_pending(&mut fixture.sandbox, &mut fixture.runtime)
            .unwrap();
        assert!(!fixture.sandbox.state.is_partial());
        if dormant {
            assert_eq!(fixture.sandbox.state.resource_nodes["cold"].remaining, 17);
        }
        assert!(fixture.persistence.hydrate(&mut fixture.sandbox).unwrap());
        fixture.checkpoint();
        assert!(!fixture.persistence.authority_paused());
        fixture
            .persistence
            .restore(
                &mut fixture.sandbox,
                &mut fixture.runtime,
                &mut fixture.politics,
            )
            .unwrap();
        let revision = fixture.sandbox.revision;
        assert!(fixture.persistence.hydrate(&mut fixture.sandbox).unwrap());
        assert!(!fixture.persistence.pending());
        assert_eq!(
            fixture.sandbox.revision, revision,
            "restore already admitted the complete state"
        );
        fixture.checkpoint();
        assert!(!fixture.persistence.authority_paused());
    }
}

#[test]
fn complete_native_activation_rejects_corrupt_storage_before_queued_commands() {
    for sql in [
        "UPDATE world SET digest='broken' WHERE singleton=1",
        "CREATE TRIGGER foreign_schema AFTER UPDATE ON world BEGIN DELETE FROM regions; END",
    ] {
        let fixture = Fixture::new(true, false);
        let database = fixture.runtime.config.region_store.clone().unwrap();
        let generation = fixture.persistence.status.as_ref().unwrap().generation;
        let command = Command::Gather {
            node_id: "near".into(),
            amount: 1,
        };
        let mut admissible = fixture.sandbox.state.clone();
        admissible
            .apply(1, command.clone(), &fixture.sandbox.catalog)
            .unwrap();
        let before = serde_json::to_value(&fixture.sandbox.state).unwrap();
        let (mut app, _directory) = fixture.into_app();
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection.execute_batch(sql).unwrap();
        drop(connection);
        app.world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .push_back(SandboxIntent::Command(command));
        app.update();
        assert!(app.world().resource::<RegionalPersistence>().pending());
        assert_eq!(app.world().resource::<SandboxQueue>().0.len(), 1);
        assert_eq!(
            serde_json::to_value(&app.world().resource::<SandboxRuntime>().state).unwrap(),
            before
        );
        update_until(&mut app, |app| {
            app.world().resource::<RegionalPersistence>().blocked()
        });
        assert_eq!(
            serde_json::to_value(&app.world().resource::<SandboxRuntime>().state).unwrap(),
            before
        );
        assert_eq!(
            app.world()
                .resource::<RegionalPersistence>()
                .status
                .as_ref()
                .unwrap()
                .generation,
            generation
        );
        assert!(
            app.world()
                .resource::<RegionalPersistence>()
                .last_error
                .is_some()
        );
        drop(app);
    }
}

#[test]
fn complete_native_queue_progresses_after_one_hydration_and_one_checkpoint() {
    let fixture = Fixture::new(true, false);
    let generation = fixture.persistence.status.as_ref().unwrap().generation;
    let mut expected = fixture.sandbox.state.clone();
    let command = Command::Gather {
        node_id: "near".into(),
        amount: 1,
    };
    for _ in 0..16 {
        expected
            .apply(1, command.clone(), &fixture.sandbox.catalog)
            .unwrap();
    }
    let (mut app, _directory) = fixture.into_app();
    app.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .extend((0..16).map(|_| SandboxIntent::Command(command.clone())));
    app.update();
    assert_eq!(app.world().resource::<SandboxQueue>().0.len(), 16);
    assert!(app.world().resource::<RegionalPersistence>().pending());
    update_until(&mut app, |app| {
        app.world().resource::<SandboxQueue>().0.is_empty()
            && !app
                .world()
                .resource::<RegionalPersistence>()
                .authority_paused()
    });
    assert_eq!(
        serde_json::to_value(&app.world().resource::<SandboxRuntime>().state).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(
        app.world()
            .resource::<RegionalPersistence>()
            .status
            .as_ref()
            .unwrap()
            .generation,
        generation + 1
    );
    assert_eq!(
        app.world().resource::<SandboxRuntime>().revision,
        18,
        "one hydrate, sixteen commands, one checkpoint"
    );
    for _ in 0..4 {
        app.update();
    }
    assert!(!app.world().resource::<RegionalPersistence>().pending());
    drop(app);
}

struct TreeSource;
impl ChunkProvider for TreeSource {
    fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
        let mut position = key.center();
        position.altitude_m = 100.;
        Ok(ChunkPayload {
            cache_hit: false,
            chunk: WorldChunk {
                key,
                heights: HeightField::constant(100.),
                features: vec![WorldFeature {
                    id: format!("tree:{key}"),
                    kind: FeatureKind::Tree,
                    points: vec![position],
                    height_m: 8.,
                    properties: BTreeMap::new(),
                }],
                sources: vec![],
                warnings: vec![],
            },
        })
    }
}

#[test]
fn complete_native_activation_rejects_corrupt_storage_before_source_registration() {
    let directory = TestDirectory::new();
    let config = config(&directory.0);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(TerraWorldPlugin::with_provider(
            config.clone(),
            Arc::new(TreeSource),
        ));
    app.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .ecology
        .config
        .enabled = false;
    app.world_mut().resource_mut::<WorldRuntime>().clock.paused = true;
    update_until(&mut app, |app| {
        !app.world().resource::<ResidentChunks>().chunks.is_empty()
            && !app
                .world()
                .resource::<RegionalPersistence>()
                .authority_paused()
    });
    assert!(!app.world().resource::<SandboxRuntime>().state.is_partial());
    let before = serde_json::to_value(&app.world().resource::<SandboxRuntime>().state).unwrap();
    let connection = rusqlite::Connection::open(config.region_store.as_ref().unwrap()).unwrap();
    connection
        .execute("UPDATE world SET digest='broken' WHERE singleton=1", [])
        .unwrap();
    drop(connection);
    // A source chunk can finish while registration is deferred. Exercise the
    // actual native replay path without an eviction checkpoint masking the
    // activation failure before that source is registered.
    {
        let mut resident = app.world_mut().resource_mut::<ResidentChunks>();
        let chunk = Arc::make_mut(resident.chunks.values_mut().next().unwrap());
        chunk.features.push(WorldFeature {
            id: "new-source-tree".into(),
            kind: FeatureKind::Tree,
            points: vec![config.focus],
            height_m: 8.,
            properties: BTreeMap::new(),
        });
    }
    app.world_mut()
        .resource_mut::<RegionalPersistence>()
        .source_replay_pending = true;
    update_until(&mut app, |app| {
        app.world().resource::<RegionalPersistence>().blocked()
    });
    assert_eq!(
        serde_json::to_value(&app.world().resource::<SandboxRuntime>().state).unwrap(),
        before
    );
    assert!(
        app.world()
            .resource::<RegionalPersistence>()
            .source_replay_pending
    );
    drop(app);
}

#[test]
fn required_existing_checkpoint_is_validated_before_startup_can_initialize_storage() {
    for kind in ["missing", "empty", "corrupt", "zero_generation"] {
        let Fixture {
            persistence,
            sandbox,
            runtime,
            politics,
            resident,
            directory: _directory,
        } = Fixture::new(false, false);
        drop(persistence);
        let database = runtime.config.region_store.clone().unwrap();
        match kind {
            "missing" => fs::remove_file(&database).unwrap(),
            "empty" => fs::write(&database, []).unwrap(),
            "corrupt" => fs::write(&database, b"foreign data that must survive").unwrap(),
            _ => (),
        }
        let original = fs::read(&database).ok();
        let state = serde_json::to_value(&sandbox.state).unwrap();
        let mut admission = RegionalPersistence::default();
        admission.require_existing_checkpoint().unwrap();
        let mut app = App::new();
        app.insert_resource(admission)
            .insert_resource(sandbox)
            .insert_resource(runtime)
            .insert_resource(politics)
            .insert_resource(resident)
            .add_message::<AppExit>()
            .add_systems(Startup, initialize_regions);
        app.update();
        assert!(app.should_exit().unwrap().is_error(), "{kind}");
        assert!(
            app.world()
                .resource::<RegionalPersistence>()
                .last_error
                .as_deref()
                .unwrap()
                .contains("no fresh world was initialized")
        );
        assert_eq!(fs::read(&database).ok(), original, "{kind}");
        assert_eq!(
            serde_json::to_value(&app.world().resource::<SandboxRuntime>().state).unwrap(),
            state
        );
        assert!(
            app.world_mut()
                .resource_mut::<RegionalPersistence>()
                .require_existing_checkpoint()
                .is_err(),
            "admission cannot be changed after startup"
        );
        drop(app);
    }
}
