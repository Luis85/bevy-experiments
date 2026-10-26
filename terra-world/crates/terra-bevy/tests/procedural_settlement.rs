//! The reference procedural map must support an actual living settlement, not
//! only isolated rules and a flat terrain fixture. This gate uses the same
//! stream provider, physical surfaces, source footprints and schedules as play.
use bevy::{prelude::*, time::TimeUpdateStrategy};
use std::time::{Duration, Instant};
use terra_bevy::{
    LivingDiagnostics, ResidentChunks, SandboxRuntime, TerraLivingPlugin, TerraWorldPlugin,
    WorldConfig, WorldRuntime,
};
use terra_core::{FeatureKind, GeoPosition, LocalFrame};
use terra_sandbox::UnitRole;

#[test]
fn default_munich_procedural_settlement_produces_and_physically_delivers() {
    let directory = std::env::temp_dir().join(format!(
        "terra-procedural-settlement-{}",
        std::process::id()
    ));
    let config = WorldConfig {
        focus: GeoPosition::new(48.137, 11.575, 0.).unwrap(),
        seed: 42,
        chunk_radius: 3,
        terrain_resolution: 65,
        max_resident_chunks: 64,
        max_queued: 64,
        max_in_flight: 2,
        live_data: false,
        live_weather: false,
        time_scale: 60.,
        cache_dir: directory.join("cache"),
        save_path: directory.join("world.json"),
        ..Default::default()
    };
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )))
        .add_plugins((TerraWorldPlugin::new(config), TerraLivingPlugin));
    // Waiting for worker threads must not accidentally age or move the
    // population according to machine-dependent loading time.
    app.world_mut().resource_mut::<WorldRuntime>().clock.paused = true;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        app.update();
        if app.world().resource::<ResidentChunks>().chunks.len() == 49
            && app
                .world()
                .resource::<SandboxRuntime>()
                .state
                .living
                .enabled
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "reference scene did not initialize: {} chunks; {:?}; {:?}",
            app.world().resource::<ResidentChunks>().chunks.len(),
            app.world().resource::<LivingDiagnostics>(),
            app.world().resource::<WorldRuntime>().last_error
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let resident = app.world().resource::<ResidentChunks>();
    for kind in [
        FeatureKind::Road,
        FeatureKind::Building,
        FeatureKind::Farm,
        FeatureKind::Forest,
    ] {
        assert!(
            resident
                .chunks
                .values()
                .flat_map(|chunk| &chunk.features)
                .any(|feature| feature.kind == kind),
            "actual procedural source must exercise {kind:?} semantics"
        );
    }
    app.world_mut().resource_mut::<WorldRuntime>().clock.paused = false;
    for _ in 0..3600 {
        app.update();
    }
    let sandbox = app.world().resource::<SandboxRuntime>();
    let living = &sandbox.state.living;
    let frame = LocalFrame::new(living.origin.unwrap());
    eprintln!(
        "Procedural settlement after180s: {} bodies, {} shops, {} sites, {:?}",
        living.units.len(),
        living.shops.len(),
        living.producers.len(),
        living.metrics
    );
    assert!(living.units.len() >= 39);
    assert!(living.metrics.production_cycles > 0);
    assert!(
        living.metrics.deliveries_completed > 0,
        "source-world couriers failed: {:?}; {:?}; {:?}",
        living.deliveries,
        living
            .units
            .values()
            .filter(|unit| unit.role == UnitRole::Hauler)
            .map(|unit| (
                unit.id,
                frame.to_local(unit.position),
                unit.velocity_enu_m_s,
                unit.path_index,
                unit.path.get(unit.path_index).map(|p| frame.to_local(*p)),
                unit.order.clone()
            ))
            .collect::<Vec<_>>(),
        app.world().resource::<LivingDiagnostics>()
    );
    sandbox
        .state
        .validate_with_catalog(&sandbox.catalog)
        .unwrap();
    let _ = std::fs::remove_dir_all(directory);
}
