//! Exercise the actual reusable plugin without a window, renderer or GPU.
use bevy::{prelude::*, time::TimeUpdateStrategy};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use terra_bevy::{
    ResidentChunks, SandboxIntent, SandboxQueue, SandboxRuntime, TerraWorldPlugin, WorldConfig,
    WorldEnvironment, WorldRuntime,
};
use terra_core::{GeoPosition, TileKey};
use terra_sandbox::Command;

static NEXT_TEST: AtomicU64 = AtomicU64::new(1);

fn app_at(position: GeoPosition) -> App {
    let serial = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
    let directory =
        std::env::temp_dir().join(format!("terra-headless-{}-{serial}", std::process::id()));
    let config = WorldConfig {
        focus: position,
        chunk_radius: 0,
        max_resident_chunks: 4,
        max_queued: 4,
        max_in_flight: 1,
        terrain_resolution: 9,
        integrate_per_frame: 1,
        max_features_per_chunk: 16,
        cache_dir: directory.join("cache"),
        save_path: directory.join("world.json"),
        live_data: false,
        live_weather: false,
        time_scale: 1.0,
        fixed_hz: 20.0,
        ..Default::default()
    };
    config.validate().unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )))
        .add_plugins(TerraWorldPlugin::new(config));
    app
}

fn wait_for_focus_chunk(app: &mut App) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        app.update();
        let runtime = app.world().resource::<WorldRuntime>();
        let key = TileKey::at(runtime.focus, runtime.config.chunk_level).unwrap();
        if app
            .world()
            .resource::<ResidentChunks>()
            .chunks
            .contains_key(&key)
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "background procedural chunk did not complete: {:?}",
            runtime.last_error
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn headless_plugin_streams_a_chunk_and_advances_then_pauses_its_clock() {
    let mut app = app_at(GeoPosition::new(48.137, 11.575, 0.).unwrap());
    wait_for_focus_chunk(&mut app);
    let resident = app.world().resource::<ResidentChunks>();
    assert_eq!(resident.chunks.len(), 1);
    assert!(
        resident
            .chunks
            .values()
            .all(|chunk| chunk.is_synthetic() && chunk.validate().is_ok())
    );
    let before = app.world().resource::<WorldRuntime>().clock.unix_seconds;
    for _ in 0..20 {
        app.update();
    }
    let after = app.world().resource::<WorldRuntime>().clock.unix_seconds;
    assert!(
        (after - before - 1.0).abs() < 0.001,
        "fixed clock advanced by {}",
        after - before
    );
    app.world_mut().resource_mut::<WorldRuntime>().clock.paused = true;
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<WorldRuntime>().clock.unix_seconds,
        after
    );
}

#[test]
fn whole_world_focus_jumps_rebase_and_dateline_moves_stay_finite() {
    let mut app = app_at(GeoPosition::new(0., 0., 0.).unwrap());
    wait_for_focus_chunk(&mut app);
    app.world_mut().resource_mut::<WorldRuntime>().focus = GeoPosition::new(0., 180., 0.).unwrap();
    wait_for_focus_chunk(&mut app);
    let runtime = app.world().resource::<WorldRuntime>();
    assert!(runtime.rebases >= 1);
    assert!(runtime.frame.origin.surface_distance_m(runtime.focus) < 0.001);
    assert!(
        runtime
            .frame
            .to_local(runtime.focus)
            .iter()
            .all(|x| x.is_finite())
    );
    let destination = GeoPosition::new(0., -179.999, 0.).unwrap();
    app.world_mut().resource_mut::<WorldRuntime>().focus = destination;
    wait_for_focus_chunk(&mut app);
    let runtime = app.world().resource::<WorldRuntime>();
    let local = runtime.frame.to_local(destination);
    assert!(local.iter().all(|x| x.is_finite()));
    assert!(local[0].hypot(local[2]) < 200.);
    assert!(app.world().resource::<ResidentChunks>().chunks.len() <= 4);
}

#[test]
fn queued_sandbox_commands_commit_success_and_leave_rejections_atomic() {
    let position = GeoPosition::new(48.137, 11.575, 0.).unwrap();
    let mut app = app_at(position);
    wait_for_focus_chunk(&mut app);
    let position = GeoPosition {
        altitude_m: app
            .world()
            .resource::<ResidentChunks>()
            .terrain_height(position)
            .unwrap(),
        ..position
    };
    app.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .set_player_position(1, position)
        .unwrap();
    app.world_mut().resource_mut::<WorldRuntime>().clock.paused = true;
    app.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Command(Command::ClaimPlot {
            center: position,
            radius_m: 30.,
        }));
    app.update();
    assert_eq!(
        app.world().resource::<SandboxRuntime>().state.plots.len(),
        1
    );
    let before = app.world().resource::<SandboxRuntime>().state.clone();
    app.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Command(Command::Craft {
            recipe: "planks".into(),
            count: 20,
        }));
    app.update();
    let sandbox = app.world().resource::<SandboxRuntime>();
    assert_eq!(sandbox.state, before);
    assert!(sandbox.last_message.starts_with("Action rejected:"));
    app.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Command(Command::Craft {
            recipe: "planks".into(),
            count: 1,
        }));
    app.update();
    let sandbox = app.world().resource::<SandboxRuntime>();
    assert_eq!(sandbox.state.players[&1].inventory["planks"], 4);
    assert_eq!(sandbox.state.players[&1].inventory["wood"], 28);
    sandbox.state.validate().unwrap();
}

#[test]
fn disabling_weather_uses_calm_conditions_without_destroying_the_source_sample() {
    let mut app = app_at(GeoPosition::new(48.137, 11.575, 0.).unwrap());
    let mut runtime = app.world_mut().resource_mut::<WorldRuntime>();
    runtime.weather.wind_speed_m_s = 20.;
    runtime.weather.precipitation_mm_h = 12.;
    let cached = runtime.weather;
    runtime.config.layers.set(terra_core::Layer::Weather, false);
    let effective = runtime.effective_weather();
    assert_eq!(effective.wind_speed_m_s, 0.);
    assert_eq!(effective.precipitation_mm_h, 0.);
    assert_eq!(effective.cloud_cover, 0.);
    assert!(effective.synthetic);
    assert_eq!(runtime.weather, cached);
    runtime.config.layers.set(terra_core::Layer::Weather, true);
    assert_eq!(runtime.effective_weather(), cached);
}

#[test]
fn a_paused_focus_jump_refreshes_synthetic_weather_for_the_new_region() {
    let mut app = app_at(GeoPosition::new(48.137, 11.575, 0.).unwrap());
    app.update();
    let (previous, expected, paused_time) = {
        let mut runtime = app.world_mut().resource_mut::<WorldRuntime>();
        runtime.clock.paused = true;
        let previous = runtime.weather;
        runtime.focus = GeoPosition::new(-33.86, 151.21, 0.).unwrap();
        let expected =
            terra_core::synthetic_weather(&runtime.focus, &runtime.clock, runtime.config.seed);
        (previous, expected, runtime.clock.unix_seconds)
    };
    assert_ne!(
        previous, expected,
        "fixture must span different regional conditions"
    );
    app.update();
    let runtime = app.world().resource::<WorldRuntime>();
    assert_eq!(runtime.clock.unix_seconds, paused_time);
    assert_eq!(runtime.weather, expected);
    assert_eq!(runtime.effective_weather(), expected);
}

#[test]
fn custom_catalog_streaming_preserves_metadata_without_seeding_unsupported_items() {
    use std::collections::{BTreeMap, BTreeSet};
    use terra_sandbox::{Catalog, SandboxState};

    let position = GeoPosition::new(48.137, 11.575, 0.).unwrap();
    let mut app = app_at(position);
    let catalog = Catalog {
        items: BTreeSet::from(["energy".into()]),
        recipes: BTreeMap::new(),
        blueprints: BTreeMap::new(),
        crops: BTreeMap::new(),
    };
    let mut state = SandboxState::default();
    state
        .add_player_with_inventory(
            1,
            position,
            BTreeMap::from([("energy".into(), 7)]),
            &catalog,
        )
        .unwrap();
    {
        let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
        sandbox.catalog = catalog;
        sandbox.state = state;
    }
    wait_for_focus_chunk(&mut app);
    {
        let sandbox = app.world().resource::<SandboxRuntime>();
        assert!(
            sandbox
                .state
                .world_features
                .values()
                .any(|feature| feature.kind == terra_core::FeatureKind::Tree)
        );
        assert!(sandbox.state.resource_nodes.is_empty());
        assert_eq!(sandbox.state.players[&1].inventory["energy"], 7);
        sandbox
            .state
            .validate_with_catalog(&sandbox.catalog)
            .unwrap();
    }
    app.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Save);
    app.update();
    let sandbox = app.world().resource::<SandboxRuntime>();
    assert!(
        sandbox.last_message.starts_with("Saved world"),
        "{}",
        sandbox.last_message
    );
    let config = &app.world().resource::<WorldRuntime>().config;
    let restored =
        terra_bevy::sandbox::LocalWorldSave::read(&config.save_path, config, &sandbox.catalog)
            .unwrap();
    assert_eq!(restored.state, sandbox.state);
}
