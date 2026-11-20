//! Reference traffic paint derived from the same bounded road topology as AI.
//! These authored markings are presentation, not observed regulatory map data.
use bevy::prelude::*;
use terra_bevy::rules::{RoadDirection, RoadNetwork, RoadRoute, RoadStructure};
use terra_bevy::{ResidentChunks, environment::physical_surface, geometry::MeshBuilder};
use terra_core::{GeoPosition, LocalFrame, WeatherSample};

const MAX_PAINT_PRIMITIVES: usize = 4096;
const PAINT: [f32; 4] = [0.88, 0.88, 0.76, 1.];

struct Paint<'a> {
    mesh: MeshBuilder,
    frame: LocalFrame,
    resident: &'a ResidentChunks,
    remaining: usize,
}
impl Paint<'_> {
    fn sample(&self, point: Vec3) -> Option<Vec3> {
        let mut p = self.frame.to_geo(point.to_array().map(f64::from)).ok()?;
        let surface = physical_surface(self.resident, p, &WeatherSample::default())?;
        if surface.water {
            return None;
        }
        p.altitude_m = surface.terrain_height_m + 0.16;
        Some(Vec3::from_array(self.frame.to_local(p).map(|v| v as f32)))
    }
    fn ribbon(&mut self, a: Vec3, b: Vec3, half_width: f32) {
        if self.remaining == 0 {
            return;
        }
        let direction = Vec3::new(b.x - a.x, 0., b.z - a.z).normalize_or_zero();
        if direction == Vec3::ZERO {
            return;
        }
        let side = Vec3::new(-direction.z, 0., direction.x) * half_width;
        if let (Some(a), Some(b), Some(c), Some(d)) = (
            self.sample(a + side),
            self.sample(b + side),
            self.sample(b - side),
            self.sample(a - side),
        ) {
            self.mesh.quad(a, b, c, d, PAINT);
            self.remaining -= 1;
        }
    }
    fn arrow(&mut self, center: Vec3, direction: Vec3) {
        if self.remaining < 2 {
            return;
        }
        let side = Vec3::new(-direction.z, 0., direction.x);
        self.ribbon(center - direction * 1.4, center + direction * 0.2, 0.12);
        if let (Some(a), Some(b), Some(c)) = (
            self.sample(center + side * 0.62),
            self.sample(center + direction * 1.3),
            self.sample(center - side * 0.62),
        ) {
            self.mesh.triangle(a, b, c, PAINT);
            self.remaining -= 1;
        }
    }
}

pub(super) fn mesh(
    roads: &[RoadRoute],
    anchor: GeoPosition,
    resident: &ResidentChunks,
) -> Option<Mesh> {
    let network = RoadNetwork::new(roads);
    let mut paint = Paint {
        mesh: MeshBuilder::default(),
        frame: LocalFrame::new(anchor),
        resident,
        remaining: MAX_PAINT_PRIMITIVES,
    };
    let frame = paint.frame;
    let local = |p| Vec3::from_array(frame.to_local(p).map(|v| v as f32));
    // Paint stop bars first, before the decorative direction-arrow budget.
    for junction in network.junctions() {
        let center = local(junction.position);
        for road in roads
            .iter()
            .take(32)
            .filter(|r| r.rules.motor_access && r.rules.structure != RoadStructure::Tunnel)
        {
            for pair in road.points.windows(2).take(63) {
                let a = local(pair[0]);
                let b = local(pair[1]);
                let delta = Vec3::new(b.x - a.x, 0., b.z - a.z);
                let distance = delta.length();
                if distance < 1. {
                    continue;
                }
                let direction = delta / distance;
                let along = (center - a).dot(direction);
                let nearest = a + direction * along.clamp(0., distance);
                if Vec2::new(nearest.x - center.x, nearest.z - center.z).length() > 0.8
                    || (nearest.y - center.y).abs() > 1.5
                {
                    continue;
                }
                let offset = junction.radius_m as f32 + 1.;
                let right = Vec3::new(-direction.z, 0., direction.x);
                let half = road.width_m * 0.5;
                if road.rules.direction != RoadDirection::Reverse && along >= offset {
                    let stop = center - direction * offset;
                    paint.ribbon(
                        stop + right * 0.15,
                        stop + right * (half - 0.25).max(0.3),
                        0.17,
                    );
                }
                if road.rules.direction != RoadDirection::Forward && distance - along >= offset {
                    let stop = center + direction * offset;
                    paint.ribbon(
                        stop - right * 0.15,
                        stop - right * (half - 0.25).max(0.3),
                        0.17,
                    );
                }
            }
        }
    }
    for road in roads
        .iter()
        .take(32)
        .filter(|r| r.rules.motor_access && r.rules.structure != RoadStructure::Tunnel)
    {
        if road.width_m < 4.5 && road.rules.direction == RoadDirection::Both {
            continue;
        }
        for pair in road.points.windows(2).take(63) {
            let a = local(pair[0]);
            let b = local(pair[1]);
            let delta = Vec3::new(b.x - a.x, 0., b.z - a.z);
            let distance = delta.length();
            if distance < 16. {
                continue;
            }
            let direction = delta / distance;
            let right = Vec3::new(-direction.z, 0., direction.x);
            let lane = if road.width_m < 4.5 {
                0.
            } else {
                (road.width_m * 0.25 + 0.25)
                    .min(road.width_m * 0.5 - 1.)
                    .clamp(1.1, 1.8)
            };
            let count = (distance / 40.).ceil().clamp(1., 24.) as usize;
            for i in 0..count {
                let center = a.lerp(b, (i as f32 + 0.5) / count as f32);
                if network.junctions().iter().any(|j| {
                    let p = local(j.position);
                    Vec2::new(center.x - p.x, center.z - p.z).length() < j.radius_m as f32 + 5.
                        && (center.y - p.y).abs() < 1.5
                }) {
                    continue;
                }
                if road.rules.direction != RoadDirection::Reverse {
                    paint.arrow(center + right * lane, direction);
                }
                if road.rules.direction != RoadDirection::Forward {
                    paint.arrow(center - right * lane, -direction);
                }
            }
        }
    }
    paint.mesh.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use terra_core::{HeightField, TileKey, WorldChunk};
    #[test]
    fn directed_paint_uses_loaded_ground_and_topology_without_inventing_tunnel_markings() {
        let origin = GeoPosition::new(48.137, 11.575, 20.).unwrap();
        let frame = LocalFrame::new(origin);
        let p = |x, z| frame.to_geo([x, 0., z]).unwrap();
        let mut roads = vec![
            RoadRoute {
                id: "test:ew".into(),
                points: vec![p(-50., 0.), p(50., 0.)],
                width_m: 6.,
                rules: Default::default(),
            },
            RoadRoute {
                id: "test:ns".into(),
                points: vec![p(0., -50.), p(0., 50.)],
                width_m: 6.,
                rules: Default::default(),
            },
        ];
        let key = TileKey::at(origin, 15).unwrap();
        let resident = ResidentChunks {
            level: 15,
            chunks: [(
                key,
                Arc::new(WorldChunk {
                    key,
                    heights: HeightField::constant(20.),
                    features: vec![],
                    sources: vec![],
                    warnings: vec![],
                }),
            )]
            .into(),
        };
        let two_way = mesh(&roads, origin, &resident).unwrap().count_vertices();
        roads[0].rules.direction = RoadDirection::Forward;
        roads[1].rules.direction = RoadDirection::Forward;
        let one_way = mesh(&roads, origin, &resident).unwrap().count_vertices();
        assert!(one_way > 0 && one_way < two_way);
        assert!(
            mesh(
                &roads,
                origin,
                &ResidentChunks {
                    level: 15,
                    ..Default::default()
                }
            )
            .is_none()
        );
        for road in &mut roads {
            road.rules.structure = RoadStructure::Tunnel;
        }
        assert!(mesh(&roads, origin, &resident).is_none());
    }
}
