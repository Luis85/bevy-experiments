use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use terra_core::{GeoPosition, LayerMask};
use terra_stream::StreamConfig;

/// Serializable game-owned policy. No coordinates are converted to f32 here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorldConfig {
    pub focus: GeoPosition,
    pub seed: u64,
    pub chunk_level: u8,
    pub chunk_radius: u32,
    pub terrain_resolution: usize,
    pub max_resident_chunks: usize,
    pub max_in_flight: usize,
    pub max_queued: usize,
    pub integrate_per_frame: usize,
    pub max_features_per_chunk: usize,
    pub cache_dir: PathBuf,
    pub live_data: bool,
    pub osm_features: bool,
    pub live_weather: bool,
    pub layers: LayerMask,
    /// Animate solar lighting; false uses a fixed daytime sun.
    pub day_night: bool,
    /// Apply seasonal vegetation tint. Calendar and sourced data remain available.
    pub seasons: bool,
    pub time_scale: f64,
    pub fixed_hz: f64,
    pub rebase_distance_m: f64,
    pub save_path: PathBuf,
    pub historical_pack: Option<PathBuf>,
    /// Frozen source-data epoch; the simulation may subsequently evolve from it.
    pub data_epoch_unix: Option<i64>,
    pub base_data_revision: String,
    pub politics_file: Option<PathBuf>,
    pub political_dataset_revision: Option<String>,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            focus: GeoPosition {
                latitude: 48.137,
                longitude: 11.575,
                altitude_m: 0.0,
            },
            seed: 42,
            chunk_level: 15,
            chunk_radius: 2,
            terrain_resolution: 33,
            max_resident_chunks: 64,
            max_in_flight: 2,
            max_queued: 64,
            integrate_per_frame: 2,
            max_features_per_chunk: 384,
            cache_dir: PathBuf::from(".terra-cache"),
            live_data: false,
            osm_features: true,
            live_weather: true,
            layers: LayerMask::all(),
            day_night: true,
            seasons: true,
            time_scale: 60.0,
            fixed_hz: 20.0,
            rebase_distance_m: 2_000.0,
            save_path: PathBuf::from("world-save.json"),
            historical_pack: None,
            data_epoch_unix: None,
            base_data_revision: "procedural-v1".into(),
            politics_file: None,
            political_dataset_revision: None,
        }
    }
}

impl WorldConfig {
    pub fn validate(&self) -> Result<(), String> {
        GeoPosition::new(
            self.focus.latitude,
            self.focus.longitude,
            self.focus.altitude_m,
        )
        .map_err(|e| e.to_string())?;
        if let Some(epoch) = self.data_epoch_unix {
            terra_history::parse_date(&epoch.to_string()).map_err(|e| e.to_string())?;
        }
        if self.historical_pack.is_some() && self.data_epoch_unix.is_none() {
            return Err("historical_pack requires an explicit data_epoch_unix".into());
        }
        if self.live_data && self.historical_pack.is_some() {
            return Err("historical packs cannot be combined with current live data".into());
        }
        if self.live_data && self.data_epoch_unix.is_some() {
            return Err(
                "current live APIs cannot satisfy an explicitly historical data epoch".into(),
            );
        }
        if !(10..=19).contains(&self.chunk_level) {
            return Err("chunk_level must be 10..=19 for the local renderer".into());
        }
        if self.chunk_radius > 5 {
            return Err("chunk_radius must be <= 5".into());
        }
        let needed = (2 * self.chunk_radius + 1).pow(2) as usize;
        if self.max_resident_chunks < needed || self.max_resident_chunks > 512 {
            return Err(format!(
                "max_resident_chunks must be {needed}..=512 for the selected radius"
            ));
        }
        if !(9..=129).contains(&self.terrain_resolution)
            || !(1..=4).contains(&self.max_in_flight)
            || !(1..=16).contains(&self.integrate_per_frame)
            || !(1..=2048).contains(&self.max_features_per_chunk)
            || self.max_queued < needed
            || self.max_queued > 1024
        {
            return Err("invalid terrain, request or integration budget".into());
        }
        if !self.time_scale.is_finite()
            || !(0.0..=3600.0).contains(&self.time_scale)
            || !self.fixed_hz.is_finite()
            || !(5.0..=120.0).contains(&self.fixed_hz)
            || !self.rebase_distance_m.is_finite()
            || !(100.0..=50_000.0).contains(&self.rebase_distance_m)
        {
            return Err("invalid simulation clock or rebase policy".into());
        }
        Ok(())
    }

    pub fn stream_config(&self) -> StreamConfig {
        StreamConfig {
            live: self.live_data,
            cache_dir: self.cache_dir.clone(),
            max_resident: self.max_resident_chunks,
            max_in_flight: self.max_in_flight,
            max_queued: self.max_queued,
            terrain_resolution: self.terrain_resolution,
            osm: self.osm_features,
            weather: self.live_weather,
            seed: self.seed,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_budgets_cover_the_requested_neighborhood() {
        let mut c = WorldConfig::default();
        assert!(c.validate().is_ok());
        c.max_resident_chunks = 4;
        assert!(c.validate().is_err());
        c = WorldConfig::default();
        c.time_scale = f64::NAN;
        assert!(c.validate().is_err());
    }
}
