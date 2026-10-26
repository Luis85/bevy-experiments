use crate::{GeoPosition, WorldClock, WorldDataError};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeatherSample {
    pub temperature_c: f32,
    pub precipitation_mm_h: f32,
    /// Fraction [0,1], rather than the percentage used by some upstream APIs.
    pub cloud_cover: f32,
    pub wind_speed_m_s: f32,
    /// Meteorological direction **from**, clockwise from north.
    pub wind_direction_deg: f32,
    pub observed_unix_seconds: i64,
    pub synthetic: bool,
}

impl WeatherSample {
    pub fn validate(&self) -> Result<(), WorldDataError> {
        let values = [
            self.temperature_c,
            self.precipitation_mm_h,
            self.cloud_cover,
            self.wind_speed_m_s,
            self.wind_direction_deg,
        ];
        if values.iter().any(|x| !x.is_finite()) {
            return Err(WorldDataError::NonFinite);
        }
        if !(-150.0..=100.0).contains(&self.temperature_c)
            || !(0.0..=1_000.0).contains(&self.precipitation_mm_h)
            || !(0.0..=1.0).contains(&self.cloud_cover)
            || !(0.0..=500.0).contains(&self.wind_speed_m_s)
            || !(0.0..=360.0).contains(&self.wind_direction_deg)
        {
            return Err(WorldDataError::WeatherRange);
        }
        Ok(())
    }

    /// Horizontal wind velocity, east/north/up. Direction is converted from
    /// meteorological "from" to the actual direction the air moves toward.
    pub fn wind_enu_m_s(&self) -> [f64; 3] {
        let direction = (self.wind_direction_deg as f64).to_radians();
        [
            -self.wind_speed_m_s as f64 * direction.sin(),
            -self.wind_speed_m_s as f64 * direction.cos(),
            0.0,
        ]
    }
}

impl Default for WeatherSample {
    fn default() -> Self {
        Self {
            temperature_c: 18.0,
            precipitation_mm_h: 0.0,
            cloud_cover: 0.25,
            wind_speed_m_s: 3.0,
            wind_direction_deg: 270.0,
            observed_unix_seconds: 0,
            synthetic: true,
        }
    }
}

/// Seeded, continuous illustrative climate and weather. This is an offline
/// fallback, **not** an observation, historical reconstruction or forecast.
/// Same position, UTC time and seed reproduce the same sample.
pub fn synthetic_weather(position: &GeoPosition, clock: &WorldClock, seed: u64) -> WeatherSample {
    let latitude = position.latitude.clamp(-90.0, 90.0);
    let phase = ((mix64(seed) >> 11) as f64 / ((1_u64 << 53) as f64)) * TAU;
    let day = clock.unix_seconds / 86_400.0;
    let seasonal = ((clock.day_of_year() as f64 - 200.0) * TAU / clock.days_in_year() as f64).cos()
        * latitude.signum();
    let weather_phase =
        day * 0.73 + position.longitude.to_radians() * 3.0 + latitude.to_radians() * 2.0 + phase;
    let clouds = (0.46 + 0.27 * weather_phase.sin() + 0.18 * (weather_phase * 0.37 + phase).cos())
        .clamp(0.0, 1.0);
    let local_hour = (clock.utc_hour() + position.longitude / 15.0).rem_euclid(24.0);
    let daytime = ((local_hour - 14.0) / 24.0 * TAU).cos() * 3.0;
    let temperature = 29.0 - latitude.abs() * 0.65
        + latitude.abs().min(65.0) / 65.0 * 13.0 * seasonal
        - position.altitude_m.max(0.0) * 0.0065
        + daytime
        + 2.0 * weather_phase.sin();
    WeatherSample {
        temperature_c: temperature.clamp(-120.0, 65.0) as f32,
        precipitation_mm_h: ((clouds - 0.72).max(0.0) * 24.0) as f32,
        cloud_cover: clouds as f32,
        wind_speed_m_s: (5.0 + 3.5 * (weather_phase * 1.71).sin()).max(0.0) as f32,
        wind_direction_deg: (230.0 + 80.0 * (weather_phase * 0.33).sin()).rem_euclid(360.0) as f32,
        observed_unix_seconds: clock.unix_seconds as i64,
        synthetic: true,
    }
}

/// Coarse gameplay biome labels. Source land cover should override this
/// heuristic; temperature/rain alone cannot identify individual ecosystems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Biome {
    Ocean,
    TemperateForest,
    Grassland,
    Desert,
    TropicalForest,
    BorealForest,
    Tundra,
    Alpine,
    Ice,
}

/// Approximate classification from annual climate and altitude. Negative
/// altitude is treated as ocean only in the synthetic fallback; real inland
/// depressions require a sourced water mask instead.
pub fn classify_biome(
    position: &GeoPosition,
    mean_temperature_c: f32,
    annual_precipitation_mm: f32,
) -> Biome {
    if position.altitude_m < 0.0 {
        return Biome::Ocean;
    }
    if mean_temperature_c < -12.0 || (position.latitude.abs() > 75.0 && mean_temperature_c < 0.0) {
        return Biome::Ice;
    }
    if position.altitude_m > 3_000.0 {
        return Biome::Alpine;
    }
    if mean_temperature_c < 0.0 {
        return Biome::Tundra;
    }
    if annual_precipitation_mm < 250.0 {
        return Biome::Desert;
    }
    if mean_temperature_c > 21.0 && annual_precipitation_mm > 1_500.0 {
        return Biome::TropicalForest;
    }
    if mean_temperature_c < 8.0 && annual_precipitation_mm > 400.0 {
        return Biome::BorealForest;
    }
    if annual_precipitation_mm > 700.0 {
        return Biome::TemperateForest;
    }
    Biome::Grassland
}

/// Synthetic annual climate classification, stable across clock changes.
pub fn approximate_biome(position: &GeoPosition) -> Biome {
    let latitude = position.latitude.abs();
    let temperature = 29.0 - latitude * 0.65 - position.altitude_m.max(0.0) * 0.0065;
    let tropical_rain = 1_800.0 * (-(latitude / 18.0).powi(2)).exp();
    let temperate_rain = 850.0 * (-((latitude - 48.0) / 17.0).powi(2)).exp();
    classify_biome(
        position,
        temperature as f32,
        (180.0 + tropical_rain + temperate_rain) as f32,
    )
}

fn mix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}
