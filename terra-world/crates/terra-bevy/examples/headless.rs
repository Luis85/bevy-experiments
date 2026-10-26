//! Run without a window/GPU: cargo run -p terra-bevy --no-default-features --example headless
use bevy::{prelude::*, time::TimeUpdateStrategy};
use std::time::Duration;
use terra_bevy::{ResidentChunks, TerraWorldPlugin, WorldConfig, WorldEnvironment, WorldRuntime};
fn main() {
    let config = WorldConfig {
        chunk_radius: 0,
        live_weather: false,
        time_scale: 1.,
        ..Default::default()
    };
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )))
        .add_plugins(TerraWorldPlugin::new(config));
    for _ in 0..100 {
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
    let runtime = app.world().resource::<WorldRuntime>();
    let resident = app.world().resource::<ResidentChunks>();
    println!(
        "{}",
        serde_json::json!({"clock":runtime.clock.label(),"chunks":resident.chunks.len(),"ground_m":resident.terrain_height(runtime.focus),"source":"synthetic example"})
    );
    assert!(
        !resident.chunks.is_empty(),
        "background provider failed to produce a chunk"
    );
}
