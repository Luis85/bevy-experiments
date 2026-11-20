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
            let Some(deck) = bridge_deck_height(chunk, feature) else {
                continue;
            };
            sample.height_m = sample.height_m.max(deck);
            sample.slope_degrees = 0.;
            sample.water = false;
        }
        Some(sample)
    }
    pub fn vehicle_ground_sample(
        &self,
        position: GeoPosition,
    ) -> Option<terra_core::VehicleGroundSample> {
        let sample = self.sample(position)?;
        if sample.water || sample.slope_degrees > 32. {
            return None;
        }
        let key = TileKey::at(position, self.level).ok()?;
        let chunk = self.chunks.get(&key)?;
        let road = chunk
            .features
            .iter()
            .filter(|feature| {
                feature.kind == FeatureKind::Road
                    && inside_ribbon(position, &feature.points, road_width(feature))
            })
            .max_by(|a, b| {
                bridge_deck_height(chunk, a)
                    .unwrap_or(f64::NEG_INFINITY)
                    .total_cmp(&bridge_deck_height(chunk, b).unwrap_or(f64::NEG_INFINITY))
            });
        let land = || {
            chunk.features.iter().find(|feature| {
                matches!(
                    feature.kind,
                    FeatureKind::Grass | FeatureKind::Forest | FeatureKind::Farm
                ) && inside_polygon(position, &feature.points)
            })
        };
        Some(terra_core::VehicleGroundSample {
            height_m: sample.height_m,
            surface: road.or_else(land).map_or(
                terra_core::VehicleSurface::Unknown,
                terra_core::VehicleSurface::from_feature,
            ),
        })
    }
    pub fn building_at(&self, position: GeoPosition, radius_m: f64) -> bool {
        self.building_at_with_clearance(position, radius_m, 8.)
    }
    /// Height-aware source footprint admission; min_height is terrain-relative.
    pub fn building_at_with_clearance(
        &self,
        position: GeoPosition,
        radius_m: f64,
        body_height_m: f64,
    ) -> bool {
        if !body_height_m.is_finite() || !(0.1..=32.).contains(&body_height_m) {
            return true;
        }
        let Ok(key) = TileKey::at(position, self.level) else {
            return true;
        };
        let frame = LocalFrame::new(position);
        key.neighbors(1)
            .into_iter()
            .filter_map(|k| self.chunks.get(&k))
            .flat_map(|chunk| chunk.features.iter().map(move |feature| (chunk, feature)))
            .filter(|(_, feature)| feature.kind == FeatureKind::Building)
            .any(|(chunk, feature)| {
                let ground_at = |p| {
                    let [u, v] = chunk.key.bounds().uv(p);
                    f64::from(chunk.heights.sample(u, v))
                };
                let ground = feature
                    .points
                    .iter()
                    .map(|p| ground_at(*p))
                    .min_by(f64::total_cmp);
                let body_base = position.altitude_m.max(ground_at(position));
                if feature
                    .properties
                    .get("min_height")
                    .and_then(|v| parse_length(v))
                    .zip(ground)
                    .is_some_and(|(height, ground)| {
                        terra_core::has_underpass_clearance(
                            ground,
                            body_base,
                            body_height_m,
                            height,
                        )
                    })
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
    pub fn vehicle_support(&self, position: GeoPosition) -> Option<GeoPosition> {
        self.vehicle_support_with_clearance(position, 8.)
    }
    pub fn vehicle_support_with_clearance(
        &self,
        mut position: GeoPosition,
        body_height_m: f64,
    ) -> Option<GeoPosition> {
        let sample = self.sample(position)?;
        position.altitude_m = position.altitude_m.max(sample.height_m + 0.025);
        if sample.water
            || sample.slope_degrees > 32.
            || self.building_at_with_clearance(position, 0.2, body_height_m)
        {
            return None;
        }
        position.altitude_m = sample.height_m + 0.025;
        Some(position)
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
                    if s.water
                        || s.slope_degrees > slope
                        || self.building_at_with_clearance(
                            p,
                            radius,
                            match kind {
                                ActorKind::Vehicle => 8.,
                                ActorKind::Animal => 1.3,
                                ActorKind::Creature => 2.,
                                _ => 1.8,
                            },
                        )
                    {
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

fn bridge_deck_height(chunk: &WorldChunk, feature: &WorldFeature) -> Option<f64> {
    if feature.kind != FeatureKind::Road
        || !feature
            .properties
            .get("bridge")
            .is_some_and(|v| !matches!(v.as_str(), "no" | "false" | "0"))
    {
        return None;
    }
    let (first, last) = feature.points.first().zip(feature.points.last())?;
    let endpoint_height = |position| {
        let [u, v] = chunk.key.bounds().uv(position);
        f64::from(chunk.heights.sample(u, v))
    };
    let layer = feature
        .properties
        .get("layer")
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(1.)
        .clamp(1., 10.);
    Some(endpoint_height(*first).max(endpoint_height(*last)) + 4. * layer)
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
    fn vehicle_ground_sample(
        &self,
        position: GeoPosition,
    ) -> Option<terra_core::VehicleGroundSample> {
        ResidentSurface::vehicle_ground_sample(self, position)
    }
    fn vehicle_support_with_clearance(
        &self,
        position: GeoPosition,
        body_height_m: f64,
    ) -> Option<GeoPosition> {
        ResidentSurface::vehicle_support_with_clearance(self, position, body_height_m)
    }
    fn vehicle_support(&self, position: GeoPosition) -> Option<GeoPosition> {
        ResidentSurface::vehicle_support(self, position)
    }
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
    #[test]
    fn underpasses_distinguish_car_bus_and_current_support_height() {
        let (mut chunk, mut origin, mut surface) = scene();
        chunk.heights = terra_core::HeightField::constant(250.);
        origin.altitude_m = 250.025;
        let mut ceiling = rectangle(origin, "overhead-part", FeatureKind::Building, 0., 0., 8.);
        // OSM way geometry commonly carries no measured elevation.
        for point in &mut ceiling.points {
            point.altitude_m = 0.;
        }
        ceiling
            .properties
            .insert("min_height".into(), "240 cm".into());
        chunk.features.push(ceiling);
        surface.register_chunk(&chunk);
        assert!(
            surface
                .vehicle_support_with_clearance(origin, 1.55)
                .is_some()
        );
        assert!(
            surface
                .vehicle_support_with_clearance(origin, 3.25)
                .is_none()
        );
        assert!(
            surface.vehicle_support(origin).is_none(),
            "unknown body height is conservative"
        );
        assert!(!surface.building_at_with_clearance(origin, 0.2, 1.55));
        assert!(surface.building_at_with_clearance(origin, 0.2, 3.25));
        let elevated = GeoPosition {
            altitude_m: 252.,
            ..origin
        };
        assert!(
            surface
                .vehicle_support_with_clearance(elevated, 1.55)
                .is_none()
        );
        assert!(
            surface
                .vehicle_support_with_clearance(origin, f64::NAN)
                .is_none()
        );
        assert!(
            surface
                .resolve_movement(ActorKind::Human, origin, origin)
                .is_some()
        );
        chunk.features[0].properties.clear();
        surface.register_chunk(&chunk);
        assert!(
            surface
                .vehicle_support_with_clearance(origin, 1.55)
                .is_none(),
            "missing underside never implies a tunnel"
        );
    }
}
