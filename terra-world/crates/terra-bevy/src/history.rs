//! Bridge frozen era packs into the same bounded streaming machinery.
use crate::WorldConfig;
use std::sync::Arc;
use terra_core::TileKey;
use terra_stream::{ChunkPayload, ChunkProvider, ChunkStreamer, StreamError};

struct EraProvider {
    pack: HistoricalPack,
    epoch: i64,
}
impl ChunkProvider for EraProvider {
    fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
        self.pack
            .chunk(key, self.epoch)
            .map(|chunk| ChunkPayload {
                chunk,
                cache_hit: true,
            })
            .map_err(|e| StreamError::Payload(e.to_string()))
    }
}
pub(crate) fn build_streamer(config: &mut WorldConfig) -> Result<ChunkStreamer, StreamError> {
    if let Some(path) = &config.historical_pack {
        let epoch = config
            .data_epoch_unix
            .ok_or_else(|| StreamError::Configuration("historical epoch missing".into()))?;
        let pack =
            HistoricalPack::load(path).map_err(|e| StreamError::Configuration(e.to_string()))?;
        if !pack.contains_time(epoch) {
            return Err(StreamError::Configuration(
                "historical pack does not cover the requested data epoch".into(),
            ));
        }
        let configured_keys: Vec<_> = pack
            .keys()
            .into_iter()
            .filter(|key| key.level == config.chunk_level)
            .collect();
        if configured_keys.is_empty() {
            return Err(StreamError::Configuration(
                "historical pack has no chunks at configured detail level".into(),
            ));
        }
        // The declared grid participates in save and network identity. Never
        // label a loaded pack with a different grid merely because its tile
        // level matches. Sparse coverage and other levels remain supported.
        for key in configured_keys {
            let chunk = pack
                .chunk_ref(key, epoch)
                .map_err(|e| StreamError::Configuration(e.to_string()))?;
            if chunk.heights.width != config.terrain_resolution
                || chunk.heights.height != config.terrain_resolution
            {
                return Err(StreamError::Configuration(format!(
                    "historical pack grid {}x{} differs from configured terrain_resolution {}",
                    chunk.heights.width, chunk.heights.height, config.terrain_resolution
                )));
            }
        }
        config.base_data_revision = pack.manifest.id.clone();
        ChunkStreamer::with_provider(
            config.stream_config(),
            Arc::new(EraProvider { pack, epoch }),
        )
    } else {
        if config.live_data {
            config.base_data_revision = "live-providers-v1".into();
        }
        ChunkStreamer::new(config.stream_config())
    }
}
pub use terra_history::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_bridge_rejects_a_misdeclared_grid_before_streaming() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../terra-history/data/synthetic-era-a/manifest.json");
        let mut config = WorldConfig {
            historical_pack: Some(path),
            data_epoch_unix: Some(-631_152_000),
            terrain_resolution: 17,
            ..Default::default()
        };
        // A finite regional pack need not cover the entire world or neighbor
        // radius to initialize; missing requested tiles still fail explicitly.
        assert!(build_streamer(&mut config).is_ok());
        assert_ne!(config.base_data_revision, "procedural-v1");
        config.terrain_resolution = 33;
        let error = build_streamer(&mut config)
            .err()
            .expect("wrong grid must fail");
        assert!(error.to_string().contains("17x17"));
        assert!(error.to_string().contains("terrain_resolution 33"));
    }
}
