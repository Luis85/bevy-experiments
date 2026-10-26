use crate::{GeoError, GeoPosition, TileKey};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    Terrain,
    Water,
    Vegetation,
    Buildings,
    Roads,
    Weather,
    Actors,
}

impl Layer {
    pub const ALL: [Self; 7] = [
        Self::Terrain,
        Self::Water,
        Self::Vegetation,
        Self::Buildings,
        Self::Roads,
        Self::Weather,
        Self::Actors,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerMask {
    pub enabled: BTreeSet<Layer>,
}

impl Default for LayerMask {
    fn default() -> Self {
        Self::all()
    }
}

impl LayerMask {
    pub fn all() -> Self {
        Self {
            enabled: Layer::ALL.into_iter().collect(),
        }
    }
    pub fn contains(&self, layer: Layer) -> bool {
        self.enabled.contains(&layer)
    }
    pub fn set(&mut self, layer: Layer, enabled: bool) {
        if enabled {
            self.enabled.insert(layer);
        } else {
            self.enabled.remove(&layer);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum WorldDataError {
    #[error("height field has invalid dimensions or sample count")]
    HeightDimensions,
    #[error("world data contains non-finite values")]
    NonFinite,
    #[error("feature {0} has too few geometry points")]
    Geometry(String),
    #[error("invalid weather value or unit range")]
    WeatherRange,
    #[error(transparent)]
    Geo(#[from] GeoError),
}

/// Row-major height field, north to south and west to east. Samples include
/// the tile boundary. Heights use the provider's recorded vertical datum;
/// conversion to WGS84 ellipsoidal altitude must happen in the provider layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeightField {
    pub width: usize,
    pub height: usize,
    pub samples_m: Vec<f32>,
}

impl HeightField {
    pub fn constant(value: f32) -> Self {
        Self {
            width: 2,
            height: 2,
            samples_m: vec![value; 4],
        }
    }

    pub fn validate(&self) -> Result<(), WorldDataError> {
        self.check_dimensions()?;
        if self.samples_m.iter().any(|x| !x.is_finite()) {
            return Err(WorldDataError::NonFinite);
        }
        Ok(())
    }

    fn check_dimensions(&self) -> Result<(), WorldDataError> {
        if self.width == 0
            || self.height == 0
            || self.width.checked_mul(self.height) != Some(self.samples_m.len())
        {
            return Err(WorldDataError::HeightDimensions);
        }
        Ok(())
    }

    /// Bilinear interpolation with edge-clamped UV. Invalid fields return zero;
    /// providers should validate on ingestion or use `try_sample` for errors.
    pub fn sample(&self, u: f64, v: f64) -> f32 {
        self.try_sample(u, v).unwrap_or(0.0)
    }

    /// Constant-time checked interpolation, without revalidating every sample.
    pub fn try_sample(&self, u: f64, v: f64) -> Result<f32, WorldDataError> {
        self.check_dimensions()?;
        if !u.is_finite() || !v.is_finite() {
            return Err(WorldDataError::NonFinite);
        }
        let x = u.clamp(0.0, 1.0) * (self.width - 1) as f64;
        let y = v.clamp(0.0, 1.0) * (self.height - 1) as f64;
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let a = self.samples_m[y0 * self.width + x0];
        let b = self.samples_m[y0 * self.width + x1];
        let c = self.samples_m[y1 * self.width + x0];
        let d = self.samples_m[y1 * self.width + x1];
        if [a, b, c, d].iter().any(|x| !x.is_finite()) {
            return Err(WorldDataError::NonFinite);
        }
        let tx = x - x0 as f64;
        let ty = y - y0 as f64;
        // Perform arithmetic in f64 to avoid intermediate f32 overflow.
        Ok(((1.0 - ty) * ((1.0 - tx) * a as f64 + tx * b as f64)
            + ty * ((1.0 - tx) * c as f64 + tx * d as f64)) as f32)
    }

    pub fn min_max(&self) -> Option<(f32, f32)> {
        self.samples_m
            .iter()
            .copied()
            .filter(|x| x.is_finite())
            .fold(None, |range, x| {
                Some(match range {
                    Some((lo, hi)) => (f32::min(lo, x), f32::max(hi, x)),
                    None => (x, x),
                })
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureKind {
    Building,
    Road,
    Water,
    Forest,
    Farm,
    Grass,
    Tree,
    /// Semantic facility or landmark node, retaining its original tags.
    PointOfInterest,
    /// Linear river, stream, canal or ditch; never treated as a closed lake.
    Waterway,
}

impl FeatureKind {
    pub fn layer(self) -> Layer {
        match self {
            Self::Building | Self::PointOfInterest => Layer::Buildings,
            Self::Road => Layer::Roads,
            Self::Water | Self::Waterway => Layer::Water,
            _ => Layer::Vegetation,
        }
    }
}

/// Normalized semantic geometry. Roads/waterways are polylines; trees and POIs are points; other
/// kinds are polygon exteriors. Polygon holes/multipolygons require splitting
/// or a richer provider-specific representation; they are not inferred here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldFeature {
    pub id: String,
    pub kind: FeatureKind,
    pub points: Vec<GeoPosition>,
    pub height_m: f32,
    pub properties: BTreeMap<String, String>,
}

impl WorldFeature {
    pub fn validate(&self) -> Result<(), WorldDataError> {
        let minimum = match self.kind {
            FeatureKind::Tree | FeatureKind::PointOfInterest => 1,
            FeatureKind::Road | FeatureKind::Waterway => 2,
            _ => 3,
        };
        if self.points.len() < minimum {
            return Err(WorldDataError::Geometry(self.id.clone()));
        }
        if !self.height_m.is_finite() {
            return Err(WorldDataError::NonFinite);
        }
        for position in &self.points {
            position.validate()?;
        }
        Ok(())
    }
}

/// Retain provenance with every data chunk, including synthetic fallbacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRecord {
    pub provider: String,
    pub license: String,
    pub attribution: String,
    pub url: String,
    pub retrieved_unix_seconds: i64,
    pub synthetic: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldChunk {
    pub key: TileKey,
    pub heights: HeightField,
    pub features: Vec<WorldFeature>,
    pub sources: Vec<SourceRecord>,
    pub warnings: Vec<String>,
}

impl WorldChunk {
    pub fn validate(&self) -> Result<(), WorldDataError> {
        self.key.validate()?;
        self.heights.validate()?;
        for feature in &self.features {
            feature.validate()?;
        }
        Ok(())
    }

    pub fn height_at(&self, position: GeoPosition) -> Option<f32> {
        let bounds = self.key.bounds();
        if !bounds.contains(position) {
            return None;
        }
        let [u, v] = bounds.uv(position);
        self.heights.try_sample(u, v).ok()
    }

    pub fn is_synthetic(&self) -> bool {
        self.sources.is_empty() || self.sources.iter().all(|source| source.synthetic)
    }

    /// Approximate resident payload bytes; useful for cache budgets. Allocator
    /// overhead and `BTreeMap` node overhead are intentionally excluded.
    pub fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.heights.samples_m.len() * std::mem::size_of::<f32>()
            + self
                .features
                .iter()
                .map(|feature| {
                    std::mem::size_of::<WorldFeature>()
                        + feature.id.len()
                        + feature.points.len() * std::mem::size_of::<GeoPosition>()
                        + feature
                            .properties
                            .iter()
                            .map(|(k, v)| k.len() + v.len())
                            .sum::<usize>()
                })
                .sum::<usize>()
            + self
                .sources
                .iter()
                .map(|s| {
                    std::mem::size_of::<SourceRecord>()
                        + s.provider.len()
                        + s.license.len()
                        + s.attribution.len()
                        + s.url.len()
                })
                .sum::<usize>()
            + self.warnings.iter().map(String::len).sum::<usize>()
    }
}
