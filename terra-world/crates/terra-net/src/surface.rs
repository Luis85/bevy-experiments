//! Resident source geometry for the reference authoritative surface controller.
//! Queries are memory-only; unknown tiles reject motion instead of guessing.
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
use terra_core::{
    ActorKind, FeatureKind, GeoPosition, LocalFrame, TileKey, TravelMode, WorldChunk, WorldFeature,
    normalize_longitude,
};

#[derive(Clone, Copy, Debug)]
pub struct SurfaceSample {
    pub height_m: f64,
    pub slope_degrees: f64,
    pub water: bool,
}

/// Bounded resident geometry. Hosts feed the same normalized chunks used by
/// rendering; hiding a map layer never removes its physical geometry.
#[derive(Clone, Debug)]
pub struct ResidentSurface {
    level: u8,
    chunks: BTreeMap<TileKey, Arc<WorldChunk>>,
    insertion_order: VecDeque<TileKey>,
}
impl ResidentSurface {
    pub fn new(level: u8) -> Self {
        Self {
            level,
            chunks: BTreeMap::new(),
            insertion_order: VecDeque::new(),
        }
    }
    pub fn register_chunk(&mut self, chunk: &WorldChunk) {
        if chunk.key.level != self.level {
            return;
        }
        self.insertion_order.retain(|key| *key != chunk.key);
        self.insertion_order.push_back(chunk.key);
        self.chunks.insert(chunk.key, Arc::new(chunk.clone()));
        while self.chunks.len() > 512 {
            if let Some(key) = self.insertion_order.pop_front() {
                self.chunks.remove(&key);
            }
        }
    }
    pub fn evict_chunk(&mut self, key: TileKey) {
        self.chunks.remove(&key);
        self.insertion_order.retain(|old| *old != key);
    }
    pub fn terrain_height_m(&self, position: GeoPosition) -> Option<f64> {
        let key = TileKey::at(position, self.level).ok()?;
        let chunk = self.chunks.get(&key)?;
        let [u, v] = key.bounds().uv(position);
        Some(chunk.heights.sample(u, v) as f64)
    }
    pub fn sample(&self, position: GeoPosition) -> Option<SurfaceSample> {
        let key = TileKey::at(position, self.level).ok()?;
        let chunk = self.chunks.get(&key)?;
        let h = self.terrain_height_m(position)?;
        let frame = LocalFrame::new(position);
        let derivative = |axis: [f64; 3]| {
            let a = frame
                .to_geo(axis)
                .ok()
                .and_then(|p| self.terrain_height_m(p).map(|h| (p, h)));
            let b = frame
                .to_geo(axis.map(|x| -x))
                .ok()
                .and_then(|p| self.terrain_height_m(p).map(|h| (p, h)));
            match (a, b) {
                (Some((a, ha)), Some((b, hb))) => (ha - hb) / a.surface_distance_m(b).max(0.001),
                (Some((a, ha)), None) => (ha - h) / position.surface_distance_m(a).max(0.001),
                (None, Some((b, hb))) => (h - hb) / position.surface_distance_m(b).max(0.001),
                _ => 0.,
            }
        };
        let mut sample = SurfaceSample {
            height_m: h,
            slope_degrees: derivative([5., 0., 0.])
                .hypot(derivative([0., 0., -5.]))
                .atan()
                .to_degrees(),
            water: chunk.features.iter().any(|f| match f.kind {
                FeatureKind::Water => inside_polygon(position, &f.points),
                FeatureKind::Waterway => {
                    inside_ribbon(position, &f.points, (f.height_m as f64).clamp(0.5, 500.))
                }
                _ => false,
            }) || (chunk.is_synthetic() && h < 0.),
        };
        for feature in &chunk.features {
            if feature.kind != FeatureKind::Road
                || !feature
                    .properties
                    .get("bridge")
                    .is_some_and(|v| !matches!(v.as_str(), "no" | "false" | "0"))
                || !inside_ribbon(position, &feature.points, road_width(feature))
            {
                continue;
            }
            let Some((a, b)) = feature.points.first().zip(feature.points.last()) else {
                continue;
            };
            let endpoint_height = |p: GeoPosition| {
                let [u, v] = key.bounds().uv(p);
                chunk.heights.sample(u, v) as f64
            };
            let layer = feature
                .properties
                .get("layer")
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(1.)
                .clamp(1., 10.);
            sample.height_m = sample
                .height_m
                .max(endpoint_height(*a).max(endpoint_height(*b)) + 4. * layer);
            sample.slope_degrees = 0.;
            sample.water = false;
        }
        Some(sample)
    }
    pub fn building_at(&self, position: GeoPosition, radius_m: f64) -> bool {
        let Ok(key) = TileKey::at(position, self.level) else {
            return true;
        };
        let frame = LocalFrame::new(position);
        key.neighbors(1)
            .into_iter()
            .filter_map(|k| self.chunks.get(&k))
            .flat_map(|c| &c.features)
            .filter(|f| f.kind == FeatureKind::Building)
            .any(|feature| {
                if feature
                    .properties
                    .get("min_height")
                    .and_then(|v| parse_length(v))
                    .is_some_and(|h| h > 2.5)
                {
                    return false;
                }
                inside_polygon(position, &feature.points)
                    || feature
                        .points
                        .iter()
                        .zip(feature.points.iter().cycle().skip(1))
                        .take(feature.points.len())
                        .any(|(a, b)| {
                            let a = frame.to_local(*a);
                            let b = frame.to_local(*b);
                            segment_distance_sq([a[0], a[2]], [b[0], b[2]])
                                < radius_m.max(0.).powi(2)
                        })
            })
    }
    pub fn allows_placement(&self, position: GeoPosition, radius_m: f64, footprint: bool) -> bool {
        self.sample(position).is_some_and(|s| {
            !s.water && s.slope_degrees <= 32. && (position.altitude_m - s.height_m).abs() <= 3.
        }) && (!footprint || !self.building_at(position, radius_m))
    }
    pub fn resolve_movement(
        &self,
        kind: ActorKind,
        from: GeoPosition,
        mut candidate: GeoPosition,
    ) -> Option<GeoPosition> {
        let sample = self.sample(candidate)?;
        match kind.travel_mode() {
            TravelMode::Ground => {
                let radius = if kind == ActorKind::Vehicle {
                    1.15
                } else {
                    0.32
                };
                let slope = if kind == ActorKind::Vehicle { 32. } else { 44. };
                let frame = LocalFrame::new(from);
                let delta = frame.to_local(candidate);
                // Reference intent caps keep one tick below 18 m. Bounded swept
                // samples prevent a fast body crossing narrow map buildings.
                let samples = (from.surface_distance_m(candidate) / 0.3)
                    .ceil()
                    .clamp(1., 64.) as usize;
                for i in 1..=samples {
                    let p = frame
                        .to_geo(delta.map(|v| v * i as f64 / samples as f64))
                        .ok()?;
                    let s = self.sample(p)?;
                    if s.water || s.slope_degrees > slope || self.building_at(p, radius) {
                        return None;
                    }
                }
                candidate.altitude_m = sample.height_m + 0.025;
            }
            TravelMode::Water => {
                if !sample.water {
                    return None;
                }
                candidate.altitude_m = sample.height_m.max(0.) + 0.35;
            }
            TravelMode::Air => {
                candidate.altitude_m = candidate.altitude_m.max(sample.height_m + 3.);
            }
        }
        Some(candidate)
    }
}

fn parse_length(value: &str) -> Option<f64> {
    // Keep source-unit interpretation aligned with the normalized OSM adapter,
    // without making terra-stream a required dependency of the net library.
    let value = value.trim();
    let (raw, factor) = if let Some(raw) = value.strip_suffix(" ft") {
        (raw, 0.3048)
    } else if let Some(raw) = value.strip_suffix(" cm") {
        (raw, 0.01)
    } else if let Some(raw) = value.strip_suffix('m') {
        (raw, 1.)
    } else {
        (value, 1.)
    };
    raw.trim()
        .parse::<f64>()
        .ok()
        .map(|n| n * factor)
        .filter(|n| n.is_finite())
}

fn road_width(f: &WorldFeature) -> f64 {
    f.properties
        .get("width")
        .and_then(|v| parse_length(v))
        .filter(|n| *n > 0.)
        .or_else(|| (f.height_m.is_finite() && f.height_m > 0.).then_some(f.height_m as f64))
        .or_else(|| {
            f.properties
                .get("lanes")
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v > 0.)
                .map(|n| n * 3.2 + 0.8)
        })
        .unwrap_or_else(|| match f.properties.get("highway").map(String::as_str) {
            Some("footway" | "path" | "steps" | "cycleway") => 2.,
            Some("motorway" | "trunk") => 13.,
            Some("primary" | "secondary") => 8.,
            Some("service" | "track") => 4.,
            _ => 6.,
        })
        .clamp(1.2, 30.)
}
fn inside_polygon(position: GeoPosition, points: &[GeoPosition]) -> bool {
    if points.len() < 3 {
        return false;
    }
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
fn segment_distance_sq(a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    let t = if length > 1e-12 {
        (-(a[0] * d[0] + a[1] * d[1]) / length).clamp(0., 1.)
    } else {
        0.
    };
    (a[0] + t * d[0]).powi(2) + (a[1] + t * d[1]).powi(2)
}
fn inside_ribbon(position: GeoPosition, points: &[GeoPosition], width: f64) -> bool {
    let frame = LocalFrame::new(position);
    points.windows(2).any(|p| {
        let a = frame.to_local(p[0]);
        let b = frame.to_local(p[1]);
        segment_distance_sq([a[0], a[2]], [b[0], b[2]]) <= (width * 0.5).powi(2)
    })
}

impl crate::WorldEnvironment for ResidentSurface {
    fn ground_height_m(&self, position: GeoPosition) -> Option<f64> {
        self.terrain_height_m(position)
    }
    fn can_enter(&self, position: GeoPosition) -> bool {
        self.terrain_height_m(position).is_some()
    }
    fn water_height_m(&self, position: GeoPosition) -> Option<f64> {
        self.sample(position)
            .filter(|s| s.water)
            .map(|s| s.height_m.max(0.) + 0.35)
    }
    fn register_chunk(&mut self, chunk: &WorldChunk) {
        ResidentSurface::register_chunk(self, chunk);
    }
    fn evict_chunk(&mut self, key: TileKey) {
        ResidentSurface::evict_chunk(self, key);
    }
    fn resolve_movement(
        &self,
        kind: ActorKind,
        from: GeoPosition,
        candidate: GeoPosition,
    ) -> Option<GeoPosition> {
        ResidentSurface::resolve_movement(self, kind, from, candidate)
    }
    fn allows_placement(&self, position: GeoPosition, radius_m: f64, footprint: bool) -> bool {
        ResidentSurface::allows_placement(self, position, radius_m, footprint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terra_core::HeightField;

    fn scene() -> (WorldChunk, GeoPosition, ResidentSurface) {
        let key = TileKey::at(GeoPosition::new(48.137, 11.575, 10.).unwrap(), 15).unwrap();
        let mut origin = key.center();
        origin.altitude_m = 10.;
        let chunk = WorldChunk {
            key,
            heights: HeightField::constant(10.),
            features: vec![],
            sources: vec![],
            warnings: vec![],
        };
        (chunk, origin, ResidentSurface::new(15))
    }
    fn rectangle(
        origin: GeoPosition,
        id: &str,
        kind: FeatureKind,
        east: f64,
        north: f64,
        r: f64,
    ) -> WorldFeature {
        let frame = LocalFrame::new(origin);
        WorldFeature {
            id: id.into(),
            kind,
            points: [[-r, -r], [r, -r], [r, r], [-r, r]]
                .map(|v| frame.to_geo([east + v[0], 0., -(north + v[1])]).unwrap())
                .to_vec(),
            height_m: 8.,
            properties: BTreeMap::new(),
        }
    }
    #[test]
    fn source_buildings_block_swept_motion_and_placement() {
        let (mut chunk, origin, mut surface) = scene();
        let frame = LocalFrame::new(origin);
        chunk.features.push(rectangle(
            origin,
            "house",
            FeatureKind::Building,
            5.,
            0.,
            0.5,
        ));
        surface.register_chunk(&chunk);
        let through = frame.to_geo([10., 0., 0.]).unwrap();
        assert!(
            surface
                .resolve_movement(ActorKind::Human, origin, through)
                .is_none()
        );
        assert!(!surface.allows_placement(frame.to_geo([5., 0., 0.]).unwrap(), 2., true));
        assert!(surface.allows_placement(frame.to_geo([-5., 0., 0.]).unwrap(), 2., true));
        chunk.features[0]
            .properties
            .insert("min_height".into(), "400 cm".into());
        surface.register_chunk(&chunk);
        assert!(
            surface
                .resolve_movement(ActorKind::Human, origin, through)
                .is_some()
        );
        surface.evict_chunk(chunk.key);
        assert!(
            surface
                .resolve_movement(ActorKind::Human, origin, through)
                .is_none()
        );
    }
    #[test]
    fn water_ship_and_bridge_support_follow_resident_source_geometry() {
        let (mut chunk, origin, mut surface) = scene();
        let frame = LocalFrame::new(origin);
        chunk
            .features
            .push(rectangle(origin, "pond", FeatureKind::Water, 0., 0., 12.));
        surface.register_chunk(&chunk);
        assert!(
            surface
                .resolve_movement(ActorKind::Human, origin, origin)
                .is_none()
        );
        assert!(
            surface
                .resolve_movement(ActorKind::Ship, origin, origin)
                .is_some()
        );
        assert!(!surface.allows_placement(origin, 1., true));
        chunk.features.push(WorldFeature {
            id: "bridge".into(),
            kind: FeatureKind::Road,
            points: vec![
                frame.to_geo([-20., 0., 0.]).unwrap(),
                frame.to_geo([20., 0., 0.]).unwrap(),
            ],
            height_m: 4.,
            properties: BTreeMap::from([
                ("bridge".into(), "yes".into()),
                ("width".into(), "10 ft".into()),
            ]),
        });
        surface.register_chunk(&chunk);
        let grounded = surface
            .resolve_movement(ActorKind::Human, origin, origin)
            .unwrap();
        assert!((grounded.altitude_m - 14.025).abs() < 1e-6);
        assert!(
            surface
                .resolve_movement(ActorKind::Ship, origin, origin)
                .is_none()
        );
        assert!(
            surface
                .resolve_movement(
                    ActorKind::Human,
                    origin,
                    frame.to_geo([0., 0., 4.]).unwrap()
                )
                .is_none()
        );
    }
    #[test]
    fn steep_resident_heightmap_rejects_ground_motion_and_building() {
        let (mut chunk, origin, mut surface) = scene();
        let bounds = chunk.key.bounds();
        let west = GeoPosition::new(origin.latitude, bounds.west, 10.).unwrap();
        let east = GeoPosition::new(origin.latitude, bounds.east, 10.).unwrap();
        let rise = west.surface_distance_m(east) as f32;
        chunk.heights = HeightField {
            width: 2,
            height: 2,
            samples_m: vec![0., rise, 0., rise],
        };
        surface.register_chunk(&chunk);
        let mut position = origin;
        position.altitude_m = surface.terrain_height_m(origin).unwrap();
        assert!(surface.sample(position).unwrap().slope_degrees > 44.5);
        assert!(
            surface
                .resolve_movement(ActorKind::Vehicle, position, position)
                .is_none()
        );
        assert!(!surface.allows_placement(position, 1., true));
        assert!(
            surface
                .resolve_movement(ActorKind::Helicopter, position, position)
                .is_some()
        );
    }
}
