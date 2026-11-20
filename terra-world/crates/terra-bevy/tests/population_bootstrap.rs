//! Startup admission uses the actual streamed neighborhood, including polar
//! clipping. These are headless scheduler/terrain fixtures, not native visuals.
use bevy::{prelude::*, time::TimeUpdateStrategy};
use std::{
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
use terra_bevy::{
    LivingDiagnostics, ResidentChunks, SandboxRuntime, TerraLivingPlugin, TerraWorldPlugin,
    WorldConfig, WorldRuntime, living::startup_neighborhood,
};
use terra_core::{GeoPosition, HeightField, TileKey, WorldChunk};
use terra_stream::{ChunkPayload, ChunkProvider, StreamError};

#[derive(Default)]
struct LoadGate {
    open: Mutex<bool>,
    changed: Condvar,
}

struct ReleaseGate(Arc<LoadGate>);
impl ReleaseGate {
    fn open(&self) {
        *self.0.open.lock().unwrap() = true;
        self.0.changed.notify_all();
    }
}
impl Drop for ReleaseGate {
    fn drop(&mut self) {
        self.open();
    }
}

struct FlatProvider {
    height: f32,
    held: Option<TileKey>,
    gate: Arc<LoadGate>,
}
fn chunk(key: TileKey, height: f32) -> WorldChunk {
    WorldChunk {
        key,
        heights: HeightField::constant(height),
        features: vec![],
        sources: vec![],
        warnings: vec![],
    }
}
impl ChunkProvider for FlatProvider {
    fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
        if self.held == Some(key) {
            let _guard = self
                .gate
                .changed
                .wait_while(self.gate.open.lock().unwrap(), |open| !*open)
                .unwrap();
        }
        Ok(ChunkPayload {
            chunk: chunk(key, self.height),
            cache_hit: false,
        })
    }
}

fn game(
    focus: GeoPosition,
    level: u8,
    radius: u32,
    height: f32,
    held: Option<TileKey>,
) -> (App, ReleaseGate) {
    let gate = Arc::new(LoadGate::default());
    let config = WorldConfig {
        focus,
        chunk_level: level,
        chunk_radius: radius,
        terrain_resolution: 9,
        live_weather: false,
        time_scale: 0.,
        base_data_revision: "authored-bootstrap-flat-v1".into(),
        ..Default::default()
    };
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )))
        .add_plugins((
            TerraWorldPlugin::with_provider(
                config,
                Arc::new(FlatProvider {
                    height,
                    held,
                    gate: Arc::clone(&gate),
                }),
            ),
            TerraLivingPlugin,
        ));
    app.world_mut().resource_mut::<WorldRuntime>().clock.paused = true;
    (app, ReleaseGate(gate))
}

fn update_until(app: &mut App, ready: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.update();
        if ready(app) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "bootstrap fixture timed out: {:?}; {:?}",
            app.world().resource::<WorldRuntime>().stream_stats,
            app.world().resource::<LivingDiagnostics>(),
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn shared_startup_neighborhood_keeps_full_scheduler_extent_and_rejects_invalid_input() {
    let focus = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut config = WorldConfig {
        focus,
        chunk_level: 15,
        chunk_radius: 3,
        ..Default::default()
    };
    let expected = TileKey::at(focus, 15).unwrap().neighbors(3);
    assert_eq!(expected.len(), 49);
    assert_eq!(startup_neighborhood(&config, focus), expected);
    assert!(
        startup_neighborhood(
            &config,
            GeoPosition {
                latitude: f64::NAN,
                ..focus
            }
        )
        .is_empty()
    );
    config.chunk_level = u8::MAX;
    assert!(startup_neighborhood(&config, focus).is_empty());
}

#[test]
fn polar_clipped_and_single_tile_neighborhoods_bootstrap_the_integrated_world() {
    for (latitude, level, radius, expected_count) in
        [(89.9, 10, 1, 6), (-89.9, 10, 1, 6), (48.137, 15, 0, 1)]
    {
        let center = TileKey::at(GeoPosition::new(latitude, 11.575, 100.).unwrap(), level).unwrap();
        let focus = GeoPosition {
            altitude_m: 100.,
            ..center.center()
        };
        let requested = center.neighbors(radius);
        assert_eq!(requested.len(), expected_count);
        let (mut app, _release) = game(focus, level, radius, 100., None);
        update_until(&mut app, |app| {
            app.world().resource::<WorldRuntime>().stream_stats.resident == expected_count
        });
        let resident = app.world().resource::<ResidentChunks>();
        assert_eq!(
            resident.chunks.keys().copied().collect::<Vec<_>>(),
            requested
        );
        let sandbox = app.world().resource::<SandboxRuntime>();
        assert!(
            sandbox.state.living.enabled,
            "{latitude}: {:?}",
            app.world().resource::<LivingDiagnostics>()
        );
        assert!(sandbox.state.transport.enabled);
        assert!(sandbox.state.transport.drivers.contains_key(&1));
        assert!(!sandbox.state.ecology.resources.is_empty());
        let active = sandbox.state.living.accounts[&1].active_unit;
        assert!((sandbox.state.living.units[&active].position.altitude_m - 100.025).abs() < 0.001);
        sandbox
            .state
            .validate_with_catalog(&sandbox.catalog)
            .unwrap();
    }
}

#[test]
fn startup_waits_for_relevant_neighbors_even_when_unrelated_pages_are_resident() {
    let focus = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let center = TileKey::at(focus, 15).unwrap();
    let requested = center.neighbors(1);
    assert_eq!(requested.len(), 9);
    let held = requested
        .iter()
        .copied()
        .find(|key| *key != center)
        .unwrap();
    let (mut app, release) = game(focus, 15, 1, 100., Some(held));
    update_until(&mut app, |app| {
        app.world().resource::<WorldRuntime>().stream_stats.resident == 8
    });
    assert!(
        !app.world()
            .resource::<SandboxRuntime>()
            .state
            .living
            .enabled
    );
    let unrelated = TileKey::new(15, center.x + 20, center.y).unwrap();
    app.world_mut()
        .resource_mut::<ResidentChunks>()
        .chunks
        .insert(unrelated, Arc::new(chunk(unrelated, 100.)));
    app.update();
    assert_eq!(app.world().resource::<ResidentChunks>().chunks.len(), 9);
    assert!(
        !app.world()
            .resource::<SandboxRuntime>()
            .state
            .living
            .enabled
    );
    release.open();
    update_until(&mut app, |app| {
        app.world()
            .resource::<SandboxRuntime>()
            .state
            .living
            .enabled
    });
    assert!(
        app.world()
            .resource::<SandboxRuntime>()
            .state
            .transport
            .enabled
    );
}

#[test]
fn complete_neighbor_coverage_does_not_admit_unsupported_water_ground() {
    let focus = GeoPosition::new(48.137, 11.575, -10.).unwrap();
    let (mut app, _release) = game(focus, 15, 1, -10., None);
    update_until(&mut app, |app| {
        app.world().resource::<WorldRuntime>().stream_stats.resident == 9
    });
    assert!(
        !app.world()
            .resource::<SandboxRuntime>()
            .state
            .living
            .enabled
    );
    assert!(
        !app.world()
            .resource::<SandboxRuntime>()
            .state
            .transport
            .enabled
    );
    assert!(
        app.world()
            .resource::<LivingDiagnostics>()
            .last_error
            .as_deref()
            .unwrap()
            .contains("walkable land")
    );
}
