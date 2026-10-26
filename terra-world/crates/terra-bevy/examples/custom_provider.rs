//! Replace this synthetic adapter with a game-owned archive, database or tile CDN.
//! cargo run -p terra-bevy --no-default-features --example custom_provider
use bevy::prelude::*;
use std::{sync::Arc, time::Duration};
use terra_bevy::{ResidentChunks, TerraWorldPlugin, WorldConfig};
use terra_core::TileKey;
use terra_stream::{ChunkPayload, ChunkProvider, StreamError};
struct ScenarioProvider;
impl ChunkProvider for ScenarioProvider {
    fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
        let chunk = terra_stream::procedural_chunk(key, 17, 123);
        // Keep source.synthetic=true: this example is authored, not observed geography.
        Ok(ChunkPayload {
            chunk,
            cache_hit: false,
        })
    }
}
fn main() {
    let config = WorldConfig {
        base_data_revision: "example-scenario-v1".into(),
        terrain_resolution: 17,
        seed: 123,
        chunk_radius: 0,
        ..Default::default()
    };
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(TerraWorldPlugin::with_provider(
            config,
            Arc::new(ScenarioProvider),
        ));
    for _ in 0..100 {
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
    let chunks = app.world().resource::<ResidentChunks>();
    assert!(!chunks.chunks.is_empty());
    println!("Custom provider loaded {} chunk(s)", chunks.chunks.len());
}
