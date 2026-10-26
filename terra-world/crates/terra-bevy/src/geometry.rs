//! CPU mesh generation is independent of ECS and bounded by chunk budgets.
use crate::environment::{inside_polygon, sample_chunk_height};
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use std::collections::{BTreeMap, BTreeSet};
use terra_core::{FeatureKind, GeoPosition, Layer, LocalFrame, WorldChunk, WorldFeature};

#[derive(Default)]
pub struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}
impl MeshBuilder {
    pub fn triangle(&mut self, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
        let n = (b - a).cross(c - a).normalize_or_zero();
        if n.length_squared() < 0.1 {
            return;
        }
        let start = self.positions.len() as u32;
        for p in [a, b, c] {
            self.positions.push(p.to_array());
            self.normals.push(n.to_array());
            self.colors.push(color);
        }
        self.indices.extend([start, start + 1, start + 2]);
    }
    pub fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: [f32; 4]) {
        self.triangle(a, b, c, color);
        self.triangle(a, c, d, color);
    }
    pub fn cuboid(&mut self, center: Vec3, size: Vec3, color: [f32; 4]) {
        let p = |x: f32, y: f32, z: f32| center + size * Vec3::new(x, y, z) * 0.5;
        self.quad(
            p(-1., 1., -1.),
            p(-1., 1., 1.),
            p(1., 1., 1.),
            p(1., 1., -1.),
            color,
        );
        self.quad(
            p(-1., -1., 1.),
            p(-1., -1., -1.),
            p(1., -1., -1.),
            p(1., -1., 1.),
            color,
        );
        self.quad(
            p(-1., -1., -1.),
            p(-1., 1., -1.),
            p(1., 1., -1.),
            p(1., -1., -1.),
            color,
        );
        self.quad(
            p(1., -1., 1.),
            p(1., 1., 1.),
            p(-1., 1., 1.),
            p(-1., -1., 1.),
            color,
        );
        self.quad(
            p(-1., -1., 1.),
            p(-1., 1., 1.),
            p(-1., 1., -1.),
            p(-1., -1., -1.),
            color,
        );
        self.quad(
            p(1., -1., -1.),
            p(1., 1., -1.),
            p(1., 1., 1.),
            p(1., -1., 1.),
            color,
        );
    }
    pub fn tree(&mut self, base: Vec3, height: f32, seed: u64) {
        self.cuboid(
            base + Vec3::Y * height * 0.22,
            Vec3::new(height * 0.08, height * 0.44, height * 0.08),
            [0.24, 0.16, 0.09, 1.],
        );
        let green = 0.23 + (seed % 11) as f32 * 0.012;
        for tier in 0..3 {
            let bottom = base + Vec3::Y * (height * (0.25 + tier as f32 * 0.18));
            let tip = base + Vec3::Y * (height * (0.67 + tier as f32 * 0.15));
            let radius = height * (0.28 - tier as f32 * 0.06);
            for i in 0..6 {
                let a = i as f32 * std::f32::consts::TAU / 6.;
                let b = (i + 1) as f32 * std::f32::consts::TAU / 6.;
                let pa = bottom + Vec3::new(a.cos() * radius, 0., a.sin() * radius);
                let pb = bottom + Vec3::new(b.cos() * radius, 0., b.sin() * radius);
                self.triangle(pa, tip, pb, [0.075, green, 0.145, 1.]);
            }
        }
    }
    pub fn finish(self) -> Option<Mesh> {
        if self.indices.is_empty() {
            return None;
        }
        Some(
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
            .with_inserted_indices(Indices::U32(self.indices)),
        )
    }
}

fn local(frame: &LocalFrame, p: GeoPosition) -> Vec3 {
    Vec3::from_array(frame.to_local(p).map(|v| v as f32))
}
fn surface(chunk: &WorldChunk, frame: &LocalFrame, mut p: GeoPosition, offset: f64) -> Vec3 {
    p.altitude_m = sample_chunk_height(chunk, p) + offset;
    local(frame, p)
}
fn terrain_color(h: f32, slope: f32, lat: f64) -> [f32; 4] {
    if h < 0.0 {
        return [0.18, 0.28, 0.27, 1.];
    }
    let snowline = (2_900.0 - lat.abs() as f32 * 13.0).max(200.);
    if h > snowline || lat.abs() > 72. {
        return [0.81, 0.87, 0.87, 1.];
    }
    if h > snowline - 650. || slope > 0.7 {
        return [0.38, 0.42, 0.36, 1.];
    }
    let v = ((h * 0.037).sin() * 0.025).clamp(-0.04, 0.04);
    [0.24 + v, 0.39 + v, 0.245 + v * 0.5, 1.]
}

pub fn terrain_mesh(chunk: &WorldChunk) -> Option<Mesh> {
    let frame = LocalFrame::new(chunk.key.center());
    let bounds = chunk.key.bounds();
    let w = chunk.heights.width;
    let h = chunk.heights.height;
    let mut builder = MeshBuilder::default();
    if w < 2 || h < 2 {
        return None;
    }
    let p = |x: usize, y: usize| {
        let position = GeoPosition {
            latitude: bounds.north - (bounds.north - bounds.south) * y as f64 / (h - 1) as f64,
            longitude: bounds.west + (bounds.east - bounds.west) * x as f64 / (w - 1) as f64,
            altitude_m: chunk.heights.samples_m[y * w + x] as f64,
        };
        local(&frame, position)
    };
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            let a = p(x, y);
            let b = p(x + 1, y);
            let c = p(x, y + 1);
            let d = p(x + 1, y + 1);
            let color = terrain_color(
                (a.y + b.y + c.y + d.y) * 0.25,
                1.0 - (c - a).cross(b - a).normalize_or_zero().y.abs(),
                frame.origin.latitude,
            );
            builder.triangle(a, c, b, color);
            builder.triangle(b, c, d, color);
        }
    }
    // Downward skirts conceal sub-pixel edge differences between source tiles.
    for x in 0..w - 1 {
        for y in [0, h - 1] {
            let a = p(x, y);
            let b = p(x + 1, y);
            builder.quad(
                a,
                b,
                b - Vec3::Y * 20.,
                a - Vec3::Y * 20.,
                [0.22, 0.28, 0.20, 1.],
            );
        }
    }
    for y in 0..h - 1 {
        for x in [0, w - 1] {
            let a = p(x, y);
            let b = p(x, y + 1);
            builder.quad(
                a,
                b,
                b - Vec3::Y * 20.,
                a - Vec3::Y * 20.,
                [0.22, 0.28, 0.20, 1.],
            );
        }
    }
    builder.finish()
}

/// Ear clipping supports simple concave polygon exteriors; holes are not flattened.
pub fn triangulate(points: &[Vec2]) -> Vec<[usize; 3]> {
    if points.len() < 3 || points.len() > 512 {
        return Vec::new();
    }
    let cross = |a: Vec2, b: Vec2, c: Vec2| (b - a).perp_dot(c - a);
    let area: f32 = (0..points.len())
        .map(|i| points[i].perp_dot(points[(i + 1) % points.len()]))
        .sum();
    if area.abs() < 0.001 {
        return Vec::new();
    }
    let sign = area.signum();
    let mut remaining: Vec<_> = (0..points.len()).collect();
    let mut out = Vec::new();
    for _ in 0..points.len() * points.len() {
        if remaining.len() < 3 {
            break;
        }
        let mut found = false;
        for i in 0..remaining.len() {
            let a = remaining[(i + remaining.len() - 1) % remaining.len()];
            let b = remaining[i];
            let c = remaining[(i + 1) % remaining.len()];
            if cross(points[a], points[b], points[c]) * sign <= 0.0001 {
                continue;
            }
            let contains = remaining.iter().any(|&j| {
                j != a
                    && j != b
                    && j != c
                    && cross(points[a], points[b], points[j]) * sign >= -0.0001
                    && cross(points[b], points[c], points[j]) * sign >= -0.0001
                    && cross(points[c], points[a], points[j]) * sign >= -0.0001
            });
            if contains {
                continue;
            }
            out.push([a, b, c]);
            remaining.remove(i);
            found = true;
            break;
        }
        if !found {
            return Vec::new();
        }
    }
    out
}

fn polygon_points(feature: &WorldFeature) -> Vec<GeoPosition> {
    let mut p = feature.points.clone();
    if p.len() > 1 && p[0].surface_distance_m(*p.last().unwrap()) < 0.01 {
        p.pop();
    }
    p.dedup_by(|a, b| a.surface_distance_m(*b) < 0.01);
    p
}

fn hash(s: &str) -> u64 {
    s.bytes().fold(14695981039346656037, |h, b| {
        (h ^ b as u64).wrapping_mul(1099511628211)
    })
}

pub fn feature_meshes(
    chunk: &WorldChunk,
    budget: usize,
    removed: &BTreeSet<String>,
) -> BTreeMap<Layer, Mesh> {
    let frame = LocalFrame::new(chunk.key.center());
    let mut batches: BTreeMap<Layer, MeshBuilder> = BTreeMap::new();
    let mut trees_left = 256usize;
    for feature in chunk.features.iter().take(budget) {
        if removed.contains(&feature.id) {
            continue;
        }
        let builder = batches.entry(feature.kind.layer()).or_default();
        let seed = hash(&feature.id);
        if feature.kind == FeatureKind::Tree {
            if trees_left > 0 {
                if let Some(&p) = feature.points.first() {
                    trees_left -= 1;
                    builder.tree(
                        surface(chunk, &frame, p, 0.2),
                        feature.height_m.clamp(4., 25.),
                        seed,
                    );
                }
            }
            continue;
        }
        if feature.kind == FeatureKind::Road {
            let width = feature.height_m.clamp(3.0, 20.0);
            for segment in feature.points.windows(2).take(256) {
                let a = surface(chunk, &frame, segment[0], 0.45);
                let b = surface(chunk, &frame, segment[1], 0.45);
                let side = (b - a).cross(Vec3::Y).normalize_or_zero() * width * 0.5;
                builder.quad(
                    a - side,
                    a + side,
                    b + side,
                    b - side,
                    [0.27, 0.29, 0.28, 1.],
                );
                let mark = side.normalize_or_zero() * 0.12;
                builder.quad(
                    a - mark + Vec3::Y * 0.02,
                    a + mark + Vec3::Y * 0.02,
                    b + mark + Vec3::Y * 0.02,
                    b - mark + Vec3::Y * 0.02,
                    [0.80, 0.78, 0.59, 1.],
                );
            }
            continue;
        }
        let points = polygon_points(feature);
        if points.len() < 3 || points.len() > 512 {
            continue;
        }
        let mut vertices: Vec<Vec3> = points
            .iter()
            .map(|&p| surface(chunk, &frame, p, 0.25))
            .collect();
        let triangles = triangulate(
            &vertices
                .iter()
                .map(|p| Vec2::new(p.x, p.z))
                .collect::<Vec<_>>(),
        );
        if triangles.is_empty() {
            continue;
        }
        if feature.kind == FeatureKind::Building {
            let ground = vertices.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            let roof = vertices
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max)
                + feature.height_m.clamp(2.5, 150.);
            let color = [0.65 + (seed % 7) as f32 * 0.02, 0.60, 0.48, 1.];
            for i in 0..vertices.len() {
                let j = (i + 1) % vertices.len();
                let a = Vec3::new(vertices[i].x, ground, vertices[i].z);
                let b = Vec3::new(vertices[j].x, ground, vertices[j].z);
                builder.quad(
                    a,
                    b,
                    Vec3::new(b.x, roof, b.z),
                    Vec3::new(a.x, roof, a.z),
                    color,
                );
            }
            for p in &mut vertices {
                p.y = roof;
            }
        }
        if feature.kind == FeatureKind::Water {
            let h = vertices.iter().map(|p| p.y).fold(f32::INFINITY, f32::min) + 0.35;
            for p in &mut vertices {
                p.y = h;
            }
        }
        let color = match feature.kind {
            FeatureKind::Building => [0.36, 0.29, 0.24, 1.],
            FeatureKind::Water => [0.08, 0.36, 0.45, 0.95],
            FeatureKind::Farm => [0.53, 0.47, 0.25, 1.],
            FeatureKind::Forest => [0.16, 0.29, 0.17, 1.],
            _ => [0.30, 0.43, 0.21, 1.],
        };
        for [a, b, c] in triangles {
            let (a, b, c) = (vertices[a], vertices[b], vertices[c]);
            if (b - a).cross(c - a).y >= 0.0 {
                builder.triangle(a, b, c, color)
            } else {
                builder.triangle(a, c, b, color)
            }
        }
        if feature.kind == FeatureKind::Forest {
            let min_lat = points
                .iter()
                .map(|p| p.latitude)
                .fold(f64::INFINITY, f64::min);
            let max_lat = points
                .iter()
                .map(|p| p.latitude)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_lon = points
                .iter()
                .map(|p| p.longitude)
                .fold(f64::INFINITY, f64::min);
            let max_lon = points
                .iter()
                .map(|p| p.longitude)
                .fold(f64::NEG_INFINITY, f64::max);
            let mut rng = seed;
            for _ in 0..80 {
                if trees_left == 0 {
                    break;
                }
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                let u = (rng >> 32) as f64 / u32::MAX as f64;
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                let v = (rng >> 32) as f64 / u32::MAX as f64;
                let p = GeoPosition {
                    latitude: min_lat + (max_lat - min_lat) * u,
                    longitude: min_lon + (max_lon - min_lon) * v,
                    altitude_m: 0.,
                };
                if inside_polygon(p, &points) {
                    trees_left -= 1;
                    builder.tree(surface(chunk, &frame, p, 0.2), 9. + (rng % 8) as f32, rng);
                }
            }
        }
    }
    batches
        .into_iter()
        .filter_map(|(layer, b)| b.finish().map(|m| (layer, m)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concave_polygon_does_not_fill_the_notch() {
        let points = [
            Vec2::new(0., 0.),
            Vec2::new(3., 0.),
            Vec2::new(3., 1.),
            Vec2::new(1., 1.),
            Vec2::new(1., 3.),
            Vec2::new(0., 3.),
        ];
        let triangles = triangulate(&points);
        assert_eq!(triangles.len(), 4);
        let area: f32 = triangles
            .iter()
            .map(|[a, b, c]| {
                (points[*b] - points[*a])
                    .perp_dot(points[*c] - points[*a])
                    .abs()
                    * 0.5
            })
            .sum();
        assert!((area - 5.).abs() < 0.001);
    }
    #[test]
    fn degenerate_polygon_is_rejected() {
        assert!(triangulate(&[Vec2::ZERO, Vec2::X, Vec2::X * 2.]).is_empty());
    }
}
