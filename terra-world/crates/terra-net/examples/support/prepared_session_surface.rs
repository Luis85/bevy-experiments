//! Memory-only prepared terrain for the real-session acceptance workload.
//! This uses the same ResidentSurface movement and support policy as the host.
use std::{error::Error, path::Path, sync::Arc};
use terra_core::{ActorKind, GeoPosition, TileKey, WorldChunk};
use terra_net::{ResidentSurface, Server, ServerConfig, WorldEnvironment};
use terra_stream::{ChunkProvider, PreparedHeightmapProvider};

#[derive(Clone)]
pub struct PreparedSessionWorld {
    pub revision: String,
    pub focus: GeoPosition,
    pub chunks: Vec<Arc<WorldChunk>>,
    surface: ResidentSurface,
}

impl PreparedSessionWorld {
    pub fn open(path: &Path) -> Result<Self, Box<dyn Error>> {
        let provider = PreparedHeightmapProvider::open(path, 65)?;
        let preferred = ServerConfig::default().spawn;
        let mut focus = if provider.covers(TileKey::at(preferred, 15)?) {
            preferred
        } else {
            provider
                .covered_focus(15)
                .ok_or("prepared session fixture has no complete tile")?
        };
        let mut surface = ResidentSurface::new(15);
        let chunks: Vec<_> = TileKey::at(focus, 15)?
            .neighbors(2)
            .into_iter()
            .filter(|key| provider.covers(*key))
            .map(|key| provider.load(key).map(|payload| Arc::new(payload.chunk)))
            .collect::<Result<_, _>>()?;
        if chunks.len() < 9 {
            return Err(
                "prepared session fixture requires at least nine covered nearby tiles".into(),
            );
        }
        for chunk in &chunks {
            surface.register_chunk(chunk);
        }
        focus.altitude_m = surface
            .sample(focus)
            .filter(|sample| !sample.water && sample.slope_degrees <= 32.)
            .ok_or("prepared session spawn requires dry supported ground")?
            .height_m
            + 0.025;
        Ok(Self {
            revision: provider.source_revision().to_owned(),
            focus,
            chunks,
            surface,
        })
    }

    pub fn configure(&self, config: &mut ServerConfig) {
        config.spawn = self.focus;
        config.manifest.chunk_level = 15;
        config.manifest.terrain_resolution = 65;
        config.manifest.terrain_provider = "heightmap".into();
        config.manifest.base_data_revision = self.revision.clone();
    }

    pub fn install(&self, server: &mut Server) -> Result<(), Box<dyn Error>> {
        server.set_environment(Box::new(self.clone()));
        for chunk in &self.chunks {
            server.try_register_chunk(chunk)?;
        }
        Ok(())
    }
}

impl WorldEnvironment for PreparedSessionWorld {
    fn ground_height_m(&self, position: GeoPosition) -> Option<f64> {
        self.surface.terrain_height_m(position)
    }
    fn can_enter(&self, position: GeoPosition) -> bool {
        self.surface.terrain_height_m(position).is_some()
    }
    fn water_height_m(&self, position: GeoPosition) -> Option<f64> {
        self.surface
            .sample(position)
            .filter(|sample| sample.water)
            .map(|sample| sample.height_m.max(0.) + 0.35)
    }
    fn register_chunk(&mut self, chunk: &WorldChunk) {
        self.surface.register_chunk(chunk);
    }
    fn evict_chunk(&mut self, key: TileKey) {
        self.surface.evict_chunk(key);
    }
    fn resolve_movement(
        &self,
        kind: ActorKind,
        from: GeoPosition,
        candidate: GeoPosition,
    ) -> Option<GeoPosition> {
        self.surface.resolve_movement(kind, from, candidate)
    }
    fn allows_placement(&self, position: GeoPosition, radius: f64, footprint: bool) -> bool {
        self.surface.allows_placement(position, radius, footprint)
    }
    fn vehicle_ground_sample(
        &self,
        position: GeoPosition,
    ) -> Option<terra_core::VehicleGroundSample> {
        self.surface.vehicle_ground_sample(position)
    }
    fn vehicle_support(&self, position: GeoPosition) -> Option<GeoPosition> {
        self.surface.vehicle_support(position)
    }
    fn vehicle_support_with_clearance(
        &self,
        position: GeoPosition,
        height: f64,
    ) -> Option<GeoPosition> {
        self.surface
            .vehicle_support_with_clearance(position, height)
    }
}
