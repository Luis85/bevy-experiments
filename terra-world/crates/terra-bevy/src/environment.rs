use crate::simulation::ResidentChunks;
use terra_core::{
    FeatureKind, GeoPosition, LocalFrame, TileKey, WeatherSample, WorldChunk, normalize_longitude,
};

/// A game-facing query, independent of any particular physics or AI library.
#[derive(Clone, Debug)]
pub struct EnvironmentSample {
    pub position: GeoPosition,
    pub terrain_height_m: f64,
    pub slope_degrees: f64,
    pub water: bool,
    pub weather: WeatherSample,
    pub sourced_terrain: bool,
}

pub trait WorldEnvironment {
    fn terrain_height(&self, position: GeoPosition) -> Option<f64>;
    fn sample_environment(
        &self,
        position: GeoPosition,
        weather: &WeatherSample,
    ) -> Option<EnvironmentSample>;
}

pub fn sample_chunk_height(chunk: &WorldChunk, position: GeoPosition) -> f64 {
    let [u, v] = chunk.key.bounds().uv(position);
    chunk.heights.sample(u, v) as f64
}

/// Polygon inclusion works in unwrapped longitude, including date-line chunks.
pub fn inside_polygon(position: GeoPosition, points: &[GeoPosition]) -> bool {
    if points.len() < 3 {
        return false;
    }
    // Anchor the polygon rather than the query. A query at Greenwich must not
    // turn a small date-line polygon into an almost whole-Earth polygon.
    let anchor = points[0].longitude;
    let unwrap = |x: f64| anchor + normalize_longitude(x - anchor);
    let lon = unwrap(position.longitude);
    let mut hit = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[j];
        if (a.latitude > position.latitude) != (b.latitude > position.latitude)
            && lon
                < (unwrap(b.longitude) - unwrap(a.longitude)) * (position.latitude - a.latitude)
                    / (b.latitude - a.latitude)
                    + unwrap(a.longitude)
        {
            hit = !hit;
        }
        j = i;
    }
    hit
}

impl WorldEnvironment for ResidentChunks {
    fn terrain_height(&self, position: GeoPosition) -> Option<f64> {
        let key = TileKey::at(position, self.level).ok()?;
        self.chunks
            .get(&key)
            .map(|c| sample_chunk_height(c, position))
    }
    fn sample_environment(
        &self,
        position: GeoPosition,
        weather: &WeatherSample,
    ) -> Option<EnvironmentSample> {
        let key = TileKey::at(position, self.level).ok()?;
        let chunk = self.chunks.get(&key)?;
        let h = sample_chunk_height(chunk, position);
        let frame = LocalFrame::new(position);
        // Sample in metres through the local frame so the offsets stay stable
        // near the poles. Read neighboring tiles instead of clamping to the
        // current tile edge. A one-sided derivative works at a load boundary.
        let derivative = |axis: [f64; 3]| {
            let positive = frame
                .to_geo(axis)
                .ok()
                .and_then(|p| self.terrain_height(p).map(|height| (p, height)));
            let negative = frame
                .to_geo(axis.map(|x| -x))
                .ok()
                .and_then(|p| self.terrain_height(p).map(|height| (p, height)));
            match (positive, negative) {
                (Some((a, ha)), Some((b, hb))) => (ha - hb) / a.surface_distance_m(b).max(0.001),
                (Some((a, ha)), None) => (ha - h) / position.surface_distance_m(a).max(0.001),
                (None, Some((b, hb))) => (h - hb) / position.surface_distance_m(b).max(0.001),
                (None, None) => 0.0,
            }
        };
        let slope = derivative([5.0, 0.0, 0.0])
            .hypot(derivative([0.0, 0.0, -5.0]))
            .atan()
            .to_degrees();
        let sourced_terrain = chunk.sources.iter().any(|source| {
            let name = source.provider.to_ascii_lowercase();
            !source.synthetic
                && ["terrain", "terrarium", "elevation", "mapzen"]
                    .iter()
                    .any(|term| name.contains(term))
        });
        // Below-sea-level terrain may be dry land (polders, inland depressions).
        // A real DEM does not imply a water mask; prefer explicit water polygons.
        let water = chunk
            .features
            .iter()
            .any(|f| f.kind == FeatureKind::Water && inside_polygon(position, &f.points))
            || (!sourced_terrain && h < 0.0 && chunk.is_synthetic());
        Some(EnvironmentSample {
            position,
            terrain_height_m: h,
            slope_degrees: slope,
            water,
            weather: *weather,
            sourced_terrain,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, sync::Arc};
    use terra_core::{HeightField, SourceRecord, WorldFeature};

    fn geo(lat: f64, lon: f64) -> GeoPosition {
        GeoPosition::new(lat, lon, 0.0).unwrap()
    }
    fn chunk(key: TileKey, height: f32) -> WorldChunk {
        WorldChunk {
            key,
            heights: HeightField::constant(height),
            features: vec![],
            warnings: vec![],
            sources: vec![SourceRecord {
                provider: "Mapzen Terrarium".into(),
                license: "mixed".into(),
                attribution: "test fixture".into(),
                url: String::new(),
                retrieved_unix_seconds: 0,
                synthetic: false,
            }],
        }
    }

    #[test]
    fn date_line_polygon_does_not_cover_the_other_side_of_earth() {
        let polygon = [
            geo(-1., 179.9),
            geo(-1., -179.9),
            geo(1., -179.9),
            geo(1., 179.9),
        ];
        assert!(inside_polygon(geo(0., 180.), &polygon));
        assert!(!inside_polygon(geo(0., 0.), &polygon));
        assert!(!inside_polygon(geo(2., 180.), &polygon));
    }

    #[test]
    fn real_negative_elevation_needs_a_water_mask() {
        let position = geo(52., 5.);
        let key = TileKey::at(position, 15).unwrap();
        let mut data = chunk(key, -5.);
        let mut resident = ResidentChunks {
            level: 15,
            chunks: BTreeMap::from([(key, Arc::new(data.clone()))]),
        };
        let sample = resident
            .sample_environment(position, &WeatherSample::default())
            .unwrap();
        assert!(!sample.water);
        assert!(sample.sourced_terrain);
        let bounds = key.bounds();
        data.features.push(WorldFeature {
            id: "water".into(),
            kind: FeatureKind::Water,
            height_m: 0.,
            properties: BTreeMap::new(),
            points: vec![
                geo(bounds.south, bounds.west),
                geo(bounds.south, bounds.east),
                geo(bounds.north, bounds.east),
                geo(bounds.north, bounds.west),
            ],
        });
        resident.chunks.insert(key, Arc::new(data));
        assert!(
            resident
                .sample_environment(position, &WeatherSample::default())
                .unwrap()
                .water
        );
    }

    #[test]
    fn slope_at_a_chunk_edge_uses_neighbor_or_inward_samples() {
        let key = TileKey::at(geo(48.137, 11.575), 15).unwrap();
        let bounds = key.bounds();
        let position = geo((bounds.north + bounds.south) * 0.5, bounds.east - 1e-10);
        let frame = LocalFrame::new(position);
        let make = |key: TileKey| {
            let b = key.bounds();
            let mut data = chunk(key, 0.);
            data.heights.samples_m = vec![
                geo(b.north, b.west),
                geo(b.north, b.east),
                geo(b.south, b.west),
                geo(b.south, b.east),
            ]
            .into_iter()
            .map(|p| frame.to_local(p)[0] as f32)
            .collect();
            data
        };
        let mut resident = ResidentChunks {
            level: 15,
            chunks: BTreeMap::from([(key, Arc::new(make(key)))]),
        };
        let slope = resident
            .sample_environment(position, &WeatherSample::default())
            .unwrap()
            .slope_degrees;
        assert!((slope - 45.).abs() < 0.3, "inward slope={slope}");
        let neighbor = TileKey::new(15, key.x + 1, key.y).unwrap();
        resident.chunks.insert(neighbor, Arc::new(make(neighbor)));
        let slope = resident
            .sample_environment(position, &WeatherSample::default())
            .unwrap()
            .slope_degrees;
        assert!((slope - 45.).abs() < 0.3, "cross-tile slope={slope}");
    }
}
