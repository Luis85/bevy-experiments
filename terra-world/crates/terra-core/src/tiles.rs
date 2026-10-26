use crate::{GeoError, GeoPosition, normalize_longitude};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_TILE_LEVEL: u8 = 20;

/// Geographic quadtree bounds, in degrees. Never cross the antimeridian.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoBounds {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

impl GeoBounds {
    /// North/west are inclusive; south/east edges are accepted for sampling.
    pub fn contains(&self, position: GeoPosition) -> bool {
        let lon = normalize_longitude(position.longitude);
        position.latitude >= self.south
            && position.latitude <= self.north
            && ((lon >= self.west && lon <= self.east) || (self.east == 180.0 && lon == -180.0))
    }

    /// Normalize position into west-to-east / north-to-south sample space.
    pub fn uv(&self, position: GeoPosition) -> [f64; 2] {
        let mut lon = normalize_longitude(position.longitude);
        if self.east == 180.0 && lon == -180.0 {
            lon = 180.0;
        }
        [
            (lon - self.west) / (self.east - self.west),
            (self.north - position.latitude) / (self.north - self.south),
        ]
    }

    pub fn point(&self, u: f64, v: f64, altitude_m: f64) -> Result<GeoPosition, GeoError> {
        GeoPosition::new(
            self.north + (self.south - self.north) * v,
            self.west + (self.east - self.west) * u,
            altitude_m,
        )
    }
}

/// Planet-wide equirectangular quadtree, **not** a Web Mercator/slippy tile.
///
/// Level L has 2^L rows and columns, including both poles. X runs west to east,
/// Y north to south. Providers must explicitly reproject their source grids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TileKey {
    pub level: u8,
    pub x: u32,
    pub y: u32,
}

impl TileKey {
    pub fn new(level: u8, x: u32, y: u32) -> Result<Self, GeoError> {
        let key = Self { level, x, y };
        key.validate()?;
        Ok(key)
    }

    pub fn validate(self) -> Result<(), GeoError> {
        if self.level > MAX_TILE_LEVEL {
            return Err(GeoError::TileLevel(self.level));
        }
        let side = 1_u32 << self.level;
        if self.x >= side || self.y >= side {
            return Err(GeoError::TileIndex);
        }
        Ok(())
    }

    pub fn at(position: GeoPosition, level: u8) -> Result<Self, GeoError> {
        position.validate()?;
        if level > MAX_TILE_LEVEL {
            return Err(GeoError::TileLevel(level));
        }
        let side = 1_u32 << level;
        let x = (((normalize_longitude(position.longitude) + 180.0) / 360.0 * side as f64).floor()
            as u32)
            .min(side - 1);
        let y = (((90.0 - position.latitude) / 180.0 * side as f64).floor() as u32).min(side - 1);
        Ok(Self { level, x, y })
    }

    /// Bounds of a validated key. Invalid keys return a harmless global box;
    /// untrusted keys must be rejected with `validate` at ingress.
    pub fn bounds(self) -> GeoBounds {
        if self.validate().is_err() {
            return GeoBounds {
                south: -90.0,
                west: -180.0,
                north: 90.0,
                east: 180.0,
            };
        }
        let side = (1_u32 << self.level) as f64;
        GeoBounds {
            west: -180.0 + 360.0 * self.x as f64 / side,
            east: -180.0 + 360.0 * (self.x + 1) as f64 / side,
            north: 90.0 - 180.0 * self.y as f64 / side,
            south: 90.0 - 180.0 * (self.y + 1) as f64 / side,
        }
    }

    pub fn center(self) -> GeoPosition {
        let b = self.bounds();
        GeoPosition {
            latitude: (b.south + b.north) * 0.5,
            longitude: (b.west + b.east) * 0.5,
            altitude_m: 0.0,
        }
    }

    pub fn parent(self) -> Option<Self> {
        if self.level == 0 || self.validate().is_err() {
            return None;
        }
        Some(Self {
            level: self.level - 1,
            x: self.x / 2,
            y: self.y / 2,
        })
    }

    pub fn children(self) -> Result<[Self; 4], GeoError> {
        self.validate()?;
        if self.level == MAX_TILE_LEVEL {
            return Err(GeoError::TileLevel(self.level + 1));
        }
        Ok(std::array::from_fn(|i| Self {
            level: self.level + 1,
            x: self.x * 2 + i as u32 % 2,
            y: self.y * 2 + i as u32 / 2,
        }))
    }

    /// The center and neighboring tiles in a square window. Longitude wraps;
    /// latitude clips at the poles. Radius is capped at 64 to bound work even
    /// when input comes from a remote client. Larger interests should use LOD.
    pub fn neighbors(self, radius: u32) -> Vec<Self> {
        if self.validate().is_err() {
            return Vec::new();
        }
        let side = 1_i64 << self.level;
        let radius = radius.min(64) as i64;
        let mut keys = BTreeSet::new();
        for dy in -radius..=radius {
            let y = self.y as i64 + dy;
            if !(0..side).contains(&y) {
                continue;
            }
            for dx in -radius..=radius {
                keys.insert(Self {
                    level: self.level,
                    x: (self.x as i64 + dx).rem_euclid(side) as u32,
                    y: y as u32,
                });
            }
        }
        keys.into_iter().collect()
    }
}

impl std::fmt::Display for TileKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.level, self.x, self.y)
    }
}
