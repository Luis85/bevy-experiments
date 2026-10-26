use serde::{Deserialize, Serialize};
use thiserror::Error;

/// WGS84 defining ellipsoid parameters, in metres.
pub const WGS84_A: f64 = 6_378_137.0;
pub const WGS84_F: f64 = 1.0 / 298.257_223_563;
pub const WGS84_B: f64 = WGS84_A * (1.0 - WGS84_F);
pub const WGS84_E2: f64 = WGS84_F * (2.0 - WGS84_F);
pub const MEAN_EARTH_RADIUS_M: f64 = (2.0 * WGS84_A + WGS84_B) / 3.0;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum GeoError {
    #[error("coordinate or vector contains a non-finite number")]
    NonFinite,
    #[error("latitude must be in [-90, 90] degrees, got {0}")]
    Latitude(f64),
    #[error("ECEF origin has no unique geodetic coordinate")]
    EarthCenter,
    #[error("tile level {0} exceeds the supported level")]
    TileLevel(u8),
    #[error("tile coordinates do not fit the selected level")]
    TileIndex,
    #[error("time step must be finite and non-negative")]
    TimeStep,
}

/// WGS84 geodetic position. Altitude is **ellipsoidal**, not mean-sea-level.
///
/// Public fields ease integration. Call `validate` after deserializing or
/// constructing with a struct literal; constructors normalize longitude.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoPosition {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_m: f64,
}

impl GeoPosition {
    pub fn new(latitude: f64, longitude: f64, altitude_m: f64) -> Result<Self, GeoError> {
        let value = Self {
            latitude,
            longitude,
            altitude_m,
        };
        value.validate()?;
        Ok(Self {
            longitude: normalize_longitude(longitude),
            ..value
        })
    }

    pub fn validate(&self) -> Result<(), GeoError> {
        if !self.latitude.is_finite() || !self.longitude.is_finite() || !self.altitude_m.is_finite()
        {
            return Err(GeoError::NonFinite);
        }
        if !(-90.0..=90.0).contains(&self.latitude) {
            return Err(GeoError::Latitude(self.latitude));
        }
        Ok(())
    }

    /// Earth-centered, Earth-fixed Cartesian metres.
    pub fn to_ecef(self) -> [f64; 3] {
        let (sin_lat, cos_lat) = self.latitude.to_radians().sin_cos();
        let (sin_lon, cos_lon) = self.longitude.to_radians().sin_cos();
        let n = WGS84_A / (1.0 - WGS84_E2 * sin_lat * sin_lat).sqrt();
        [
            (n + self.altitude_m) * cos_lat * cos_lon,
            (n + self.altitude_m) * cos_lat * sin_lon,
            (n * (1.0 - WGS84_E2) + self.altitude_m) * sin_lat,
        ]
    }

    /// Convert ECEF to WGS84. Stable at the poles and below sea level.
    pub fn from_ecef([x, y, z]: [f64; 3]) -> Result<Self, GeoError> {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return Err(GeoError::NonFinite);
        }
        let p = x.hypot(y);
        if p.hypot(z) < 1e-6 {
            return Err(GeoError::EarthCenter);
        }
        if p < 1e-8 {
            return Self::new(90.0 * z.signum(), 0.0, z.abs() - WGS84_B);
        }
        let mut latitude = z.atan2(p * (1.0 - WGS84_E2));
        for _ in 0..12 {
            let sin_lat = latitude.sin();
            let n = WGS84_A / (1.0 - WGS84_E2 * sin_lat * sin_lat).sqrt();
            let next = (z + WGS84_E2 * n * sin_lat).atan2(p);
            if (next - latitude).abs() < 1e-14 {
                latitude = next;
                break;
            }
            latitude = next;
        }
        let (sin_lat, cos_lat) = latitude.sin_cos();
        let n = WGS84_A / (1.0 - WGS84_E2 * sin_lat * sin_lat).sqrt();
        let altitude_m = if cos_lat.abs() > 0.5 {
            p / cos_lat - n
        } else {
            z / sin_lat - n * (1.0 - WGS84_E2)
        };
        Self::new(latitude.to_degrees(), y.atan2(x).to_degrees(), altitude_m)
    }

    /// Great-circle surface distance on the mean-radius sphere.
    ///
    /// Suitable for streaming/interest budgets; not a survey-grade geodesic.
    pub fn surface_distance_m(self, other: Self) -> f64 {
        let dlat = (other.latitude - self.latitude).to_radians();
        let dlon = normalize_longitude(other.longitude - self.longitude).to_radians();
        let a = (dlat * 0.5).sin().powi(2)
            + self.latitude.to_radians().cos()
                * other.latitude.to_radians().cos()
                * (dlon * 0.5).sin().powi(2);
        2.0 * MEAN_EARTH_RADIUS_M * a.clamp(0.0, 1.0).sqrt().asin()
    }
}

pub fn normalize_longitude(longitude: f64) -> f64 {
    // Reducing first avoids losing the original low bits when adding 180 to
    // very large angles, and maps both forms of the antimeridian to -180.
    let reduced = longitude.rem_euclid(360.0);
    if reduced >= 180.0 {
        reduced - 360.0
    } else {
        reduced
    }
}

/// Tangent reference frame for a Bevy-compatible Y-up renderer.
///
/// Points are projected through ECEF, so Earth curvature is preserved. Rebase
/// all rendered objects with a new frame when the camera travels far away.
#[derive(Debug, Clone, Copy)]
pub struct LocalFrame {
    pub origin: GeoPosition,
    ecef_origin: [f64; 3],
    east: [f64; 3],
    north: [f64; 3],
    up: [f64; 3],
}

impl LocalFrame {
    pub fn new(origin: GeoPosition) -> Self {
        let (sin_lat, cos_lat) = origin.latitude.to_radians().sin_cos();
        let (sin_lon, cos_lon) = origin.longitude.to_radians().sin_cos();
        Self {
            origin,
            ecef_origin: origin.to_ecef(),
            east: [-sin_lon, cos_lon, 0.0],
            north: [-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat],
            up: [cos_lat * cos_lon, cos_lat * sin_lon, sin_lat],
        }
    }

    pub fn to_local(&self, position: GeoPosition) -> [f64; 3] {
        let ecef = position.to_ecef();
        let d = std::array::from_fn(|i| ecef[i] - self.ecef_origin[i]);
        [dot(d, self.east), dot(d, self.up), -dot(d, self.north)]
    }

    pub fn to_geo(&self, [east, up, south]: [f64; 3]) -> Result<GeoPosition, GeoError> {
        if !east.is_finite() || !up.is_finite() || !south.is_finite() {
            return Err(GeoError::NonFinite);
        }
        GeoPosition::from_ecef(std::array::from_fn(|i| {
            self.ecef_origin[i] + self.east[i] * east + self.up[i] * up - self.north[i] * south
        }))
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
