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

/// Width used by both source-road rendering and surface movement. OSM widths
/// are metres when supplied without a unit; lane count is a bounded fallback.
pub fn road_width_m(feature: &terra_core::WorldFeature) -> f64 {
    let explicit = feature
        .properties
        .get("width")
        .and_then(|v| terra_stream::providers::parse_osm_length(v))
        .map(f64::from)
        .filter(|v| v.is_finite() && *v > 0.);
    let lanes = feature
        .properties
        .get("lanes")
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.);
    explicit
        .or_else(|| {
            (feature.height_m.is_finite() && feature.height_m > 0.)
                .then_some(feature.height_m as f64)
        })
        .or_else(|| lanes.map(|n| n * 3.2 + 0.8))
        .unwrap_or_else(
            || match feature.properties.get("highway").map(String::as_str) {
                Some("footway" | "path" | "steps" | "cycleway") => 2.,
                Some("motorway" | "trunk") => 13.,
                Some("primary" | "secondary") => 8.,
                Some("service" | "track") => 4.,
                _ => 6.,
            },
        )
        .clamp(1.2, 30.)
}

/// Reference bridge deck. Geometry and locomotion share this exact height;
/// source tags determine an illustrative clearance, not a surveyed bridge model.
pub fn road_deck_height(chunk: &WorldChunk, feature: &terra_core::WorldFeature) -> Option<f64> {
    if feature.kind != FeatureKind::Road
        || !feature
            .properties
            .get("bridge")
            .is_some_and(|v| v != "no" && v != "false" && v != "0")
    {
        return None;
    }
    let first = *feature.points.first()?;
    let last = *feature.points.last()?;
    let layer = feature
        .properties
        .get("layer")
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(1.)
        .clamp(1., 10.);
    Some(sample_chunk_height(chunk, first).max(sample_chunk_height(chunk, last)) + layer * 4.)
}

/// Returns the bridge deck only inside its drivable ribbon. This reference uses
/// the top deck as support; detailed stacked bridge/tunnel navigation is a host concern.
pub fn road_support_height(
    chunk: &WorldChunk,
    feature: &terra_core::WorldFeature,
    position: GeoPosition,
) -> Option<f64> {
    let deck = road_deck_height(chunk, feature)?;
    let frame = LocalFrame::new(position);
    let half_width = road_width_m(feature) * 0.5;
    feature
        .points
        .windows(2)
        .any(|pair| {
            let a = frame.to_local(pair[0]);
            let b = frame.to_local(pair[1]);
            segment_distance_squared([a[0], a[2]], [b[0], b[2]]) <= half_width * half_width
        })
        .then_some(deck)
}

fn segment_distance_squared(a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length = dx * dx + dy * dy;
    let t = if length > 1e-12 {
        (-(a[0] * dx + a[1] * dy) / length).clamp(0., 1.)
    } else {
        0.
    };
    (a[0] + t * dx).powi(2) + (a[1] + t * dy).powi(2)
}

/// Width-aware inclusion for linear rivers/streams. A waterway is not a polygon,
/// and must not accidentally fill the land between its first and last points.
pub fn inside_ribbon(position: GeoPosition, points: &[GeoPosition], width_m: f64) -> bool {
    if !width_m.is_finite() || width_m <= 0. {
        return false;
    }
    let frame = LocalFrame::new(position);
    let radius_squared = (width_m * 0.5).powi(2);
    points.windows(2).any(|pair| {
        let a = frame.to_local(pair[0]);
        let b = frame.to_local(pair[1]);
        segment_distance_squared([a[0], a[2]], [b[0], b[2]]) <= radius_squared
    })
}

/// Terrain queries augmented with a source bridge deck. A river mask below a
/// bridge must not forbid a person or vehicle from using that deck.
pub fn physical_surface(
    resident: &ResidentChunks,
    position: GeoPosition,
    weather: &WeatherSample,
) -> Option<EnvironmentSample> {
    let mut sample = resident.sample_environment(position, weather)?;
    let key = TileKey::at(position, resident.level).ok()?;
    let chunk = resident.chunks.get(&key)?;
    if let Some(deck) = chunk
        .features
        .iter()
        .filter_map(|feature| road_support_height(chunk, feature, position))
        .max_by(f64::total_cmp)
    {
        sample.terrain_height_m = sample.terrain_height_m.max(deck);
        sample.water = false;
        sample.slope_degrees = 0.;
    }
    Some(sample)
}

/// Static source-building footprint check for the reference controller. An
/// elevated building part with explicit min_height does not block its underpass.
pub fn source_building_at(resident: &ResidentChunks, position: GeoPosition, radius_m: f64) -> bool {
    let Some(key) = TileKey::at(position, resident.level).ok() else {
        return true;
    };
    let frame = LocalFrame::new(position);
    let radius_sq = radius_m.max(0.).powi(2);
    key.neighbors(1)
        .into_iter()
        .filter_map(|k| resident.chunks.get(&k))
        .flat_map(|chunk| &chunk.features)
        .filter(|feature| feature.kind == FeatureKind::Building)
        .any(|feature| {
            if feature
                .properties
                .get("min_height")
                .and_then(|s| terra_stream::providers::parse_osm_length(s))
                .is_some_and(|h| h > 2.5)
            {
                return false;
            }
            if inside_polygon(position, &feature.points) {
                return true;
            }
            feature
                .points
                .iter()
                .zip(feature.points.iter().cycle().skip(1))
                .take(feature.points.len())
                .any(|(a, b)| {
                    let a = frame.to_local(*a);
                    let b = frame.to_local(*b);
                    segment_distance_squared([a[0], a[2]], [b[0], b[2]]) < radius_sq
                })
        })
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
        let water = chunk.features.iter().any(|f| match f.kind {
            FeatureKind::Water => inside_polygon(position, &f.points),
            FeatureKind::Waterway => {
                inside_ribbon(position, &f.points, (f.height_m as f64).clamp(0.5, 500.))
            }
            _ => false,
        }) || (!sourced_terrain && h < 0.0 && chunk.is_synthetic());
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
    fn bridge_support_and_width_follow_the_same_source_geometry() {
        let origin = GeoPosition::new(48.137, 11.575, 10.).unwrap();
        let frame = LocalFrame::new(origin);
        let key = TileKey::at(origin, 15).unwrap();
        let data = chunk(key, 10.);
        let road = WorldFeature {
            id: "bridge".into(),
            kind: FeatureKind::Road,
            height_m: 6.,
            points: vec![
                frame.to_geo([-20., 0., 0.]).unwrap(),
                frame.to_geo([20., 0., 0.]).unwrap(),
            ],
            properties: BTreeMap::from([
                ("bridge".into(), "yes".into()),
                ("width".into(), "10 ft".into()),
                ("layer".into(), "2".into()),
            ]),
        };
        assert!((road_width_m(&road) - 3.048).abs() < 1e-5);
        assert_eq!(road_deck_height(&data, &road), Some(18.));
        assert_eq!(road_support_height(&data, &road, origin), Some(18.));
        assert_eq!(
            road_support_height(&data, &road, frame.to_geo([0., 0., 3.]).unwrap()),
            None
        );
        let river = WorldFeature {
            id: "river".into(),
            kind: FeatureKind::Waterway,
            height_m: 10.,
            points: vec![
                frame.to_geo([0., 0., -30.]).unwrap(),
                frame.to_geo([0., 0., 30.]).unwrap(),
            ],
            properties: BTreeMap::new(),
        };
        let mut crossing = data.clone();
        crossing.features = vec![river, road.clone()];
        let resident = ResidentChunks {
            level: 15,
            chunks: BTreeMap::from([(key, Arc::new(crossing))]),
        };
        assert!(
            resident
                .sample_environment(origin, &WeatherSample::default())
                .unwrap()
                .water
        );
        let supported = physical_surface(&resident, origin, &WeatherSample::default()).unwrap();
        assert!(!supported.water);
        assert_eq!(supported.terrain_height_m, 18.);
        assert!(
            physical_surface(
                &resident,
                frame.to_geo([0., 0., 5.]).unwrap(),
                &WeatherSample::default()
            )
            .unwrap()
            .water
        );
        assert!(
            !physical_surface(
                &resident,
                frame.to_geo([10., 0., 5.]).unwrap(),
                &WeatherSample::default()
            )
            .unwrap()
            .water
        );
        let mut normal = road;
        normal.properties.insert("bridge".into(), "no".into());
        assert_eq!(road_deck_height(&data, &normal), None);
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
