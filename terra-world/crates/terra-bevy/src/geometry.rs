//! CPU mesh generation is independent of ECS and bounded by chunk budgets.
use crate::environment::{inside_polygon, road_deck_height, road_width_m, sample_chunk_height};
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
            self.colors.push(linear_color(color));
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
    /// A bounded rounded primitive with smooth normals; dimensions are radii.
    pub fn ellipsoid(&mut self, center: Vec3, radii: Vec3, color: [f32; 4], detail: usize) {
        let segments = detail.clamp(6, 16);
        let rings = (segments / 2).max(3);
        let point = |i: usize, j: usize| {
            let lat = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * j as f32 / rings as f32;
            let lon = std::f32::consts::TAU * i as f32 / segments as f32;
            let unit = Vec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
            (
                center + unit * radii,
                (unit / radii.max(Vec3::splat(0.001))).normalize_or_zero(),
            )
        };
        for j in 0..rings {
            for i in 0..segments {
                let a = point(i, j);
                let b = point(i + 1, j);
                let c = point(i, j + 1);
                let d = point(i + 1, j + 1);
                self.smooth_triangle(a, c, b, color);
                self.smooth_triangle(b, c, d, color);
            }
        }
    }
    fn smooth_triangle(
        &mut self,
        a: (Vec3, Vec3),
        b: (Vec3, Vec3),
        c: (Vec3, Vec3),
        color: [f32; 4],
    ) {
        if (b.0 - a.0).cross(c.0 - a.0).length_squared() < 1e-12 {
            return;
        }
        let start = self.positions.len() as u32;
        for (p, n) in [a, b, c] {
            self.positions.push(p.to_array());
            self.normals.push(n.to_array());
            self.colors.push(linear_color(color));
        }
        self.indices.extend([start, start + 1, start + 2]);
    }
    pub fn cylinder(&mut self, start: Vec3, end: Vec3, radius: f32, color: [f32; 4], sides: usize) {
        let axis = (end - start).normalize_or_zero();
        if axis.length_squared() < 0.1 || radius <= 0. {
            return;
        }
        let tangent = axis
            .cross(if axis.y.abs() > 0.9 { Vec3::X } else { Vec3::Y })
            .normalize();
        let bitangent = axis.cross(tangent);
        let sides = sides.clamp(5, 16);
        for i in 0..sides {
            let a = i as f32 * std::f32::consts::TAU / sides as f32;
            let b = (i + 1) as f32 * std::f32::consts::TAU / sides as f32;
            let va = (tangent * a.cos() + bitangent * a.sin()) * radius;
            let vb = (tangent * b.cos() + bitangent * b.sin()) * radius;
            self.quad(start + va, start + vb, end + vb, end + va, color);
            self.triangle(start, start + vb, start + va, color);
            self.triangle(end, end + va, end + vb, color);
        }
    }
    pub fn oriented_cuboid(&mut self, center: Vec3, size: Vec3, yaw: f32, color: [f32; 4]) {
        let mut box_mesh = Self::default();
        box_mesh.cuboid(Vec3::ZERO, size, color);
        let rot = Quat::from_rotation_y(yaw);
        let offset = self.positions.len() as u32;
        self.positions.extend(
            box_mesh
                .positions
                .into_iter()
                .map(|p| (center + rot * Vec3::from_array(p)).to_array()),
        );
        self.normals.extend(
            box_mesh
                .normals
                .into_iter()
                .map(|n| (rot * Vec3::from_array(n)).to_array()),
        );
        self.colors.extend(box_mesh.colors);
        self.indices
            .extend(box_mesh.indices.into_iter().map(|i| i + offset));
    }
    pub fn tree(&mut self, base: Vec3, height: f32, seed: u64) {
        self.tree_lod(base, height, seed, 0);
    }
    pub fn tree_lod(&mut self, base: Vec3, height: f32, seed: u64, lod: u8) {
        if lod > 0 {
            let h = height.clamp(1., 35.);
            self.cylinder(
                base,
                base + Vec3::Y * h * 0.65,
                h * 0.035,
                [0.31, 0.23, 0.15, 1.],
                5,
            );
            if seed % 3 == 0 {
                self.ellipsoid(
                    base + Vec3::Y * h * 0.69,
                    Vec3::new(h * 0.35, h * 0.32, h * 0.33),
                    [0.27, 0.42, 0.20, 1.],
                    6,
                );
            } else {
                let sides = if lod == 1 { 7 } else { 5 };
                for i in 0..sides {
                    let a = i as f32 * std::f32::consts::TAU / sides as f32;
                    let b = (i + 1) as f32 * std::f32::consts::TAU / sides as f32;
                    self.triangle(
                        base + Vec3::new(a.cos() * h * 0.28, h * 0.3, a.sin() * h * 0.28),
                        base + Vec3::Y * h,
                        base + Vec3::new(b.cos() * h * 0.28, h * 0.3, b.sin() * h * 0.28),
                        [0.15, 0.33, 0.19, 1.],
                    );
                }
            }
            return;
        }
        let h = height.clamp(1., 35.);
        let bark = [0.31, 0.23, 0.15, 1.];
        self.cylinder(base, base + Vec3::Y * h * 0.68, h * 0.035, bark, 7);
        if seed % 3 == 0 {
            // Broadleaf canopy: several intersecting smooth crowns and branches.
            let shades = [
                [0.24, 0.40, 0.18, 1.],
                [0.31, 0.46, 0.21, 1.],
                [0.27, 0.43, 0.23, 1.],
            ];
            for i in 0..3 {
                let angle = (seed % 31) as f32 + i as f32 * 2.1;
                let crown = base
                    + Vec3::new(
                        angle.cos() * h * 0.16,
                        h * (0.62 + i as f32 * 0.08),
                        angle.sin() * h * 0.16,
                    );
                self.cylinder(base + Vec3::Y * h * 0.4, crown, h * 0.018, bark, 5);
                self.ellipsoid(crown, Vec3::new(h * 0.27, h * 0.26, h * 0.25), shades[i], 8);
                let lobe =
                    crown + Vec3::new(angle.sin() * h * 0.18, h * 0.08, angle.cos() * h * 0.16);
                self.ellipsoid(
                    lobe,
                    Vec3::new(h * 0.17, h * 0.20, h * 0.17),
                    shades[(i + 1) % 3],
                    6,
                );
            }
        } else {
            for tier in 0..6 {
                let bottom = base + Vec3::Y * h * (0.20 + tier as f32 * 0.12);
                let radius = h * (0.32 - tier as f32 * 0.043);
                let green = 0.29 + (seed % 9) as f32 * 0.009 + tier as f32 * 0.013;
                // Separate drooping bough volumes break the old stacked-cone
                // silhouette without transparent leaf cards or alpha overdraw.
                for branch in 0..7 {
                    let angle = branch as f32 * std::f32::consts::TAU / 7.
                        + tier as f32 * 0.41
                        + (seed % 31) as f32 * 0.13;
                    let direction = Vec3::new(angle.cos(), 0., angle.sin());
                    let spread = Vec3::new(-direction.z, 0., direction.x) * radius * 0.32;
                    let length = radius
                        * (0.84 + (seed.wrapping_add(branch * 7 + tier * 13) % 17) as f32 / 100.);
                    let root = bottom + direction * h * 0.025;
                    let tip = bottom + direction * length - Vec3::Y * h * 0.025;
                    let crest = bottom + direction * length * 0.34 + Vec3::Y * h * 0.23;
                    self.triangle(root + spread, crest, tip, [0.16, green, 0.22, 1.]);
                    self.triangle(tip, crest, root - spread, [0.14, green * 0.92, 0.19, 1.]);
                    self.triangle(
                        root - spread,
                        crest,
                        root + spread,
                        [0.17, green * 1.04, 0.21, 1.],
                    );
                    self.triangle(
                        root + spread,
                        tip,
                        root - spread,
                        [0.13, green * 0.77, 0.18, 1.],
                    );
                }
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
fn linear_color(c: [f32; 4]) -> [f32; 4] {
    Color::srgba(c[0], c[1], c[2], c[3])
        .to_linear()
        .to_f32_array()
}
fn blend(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0., 1.);
    std::array::from_fn(|i| a[i] * (1. - t) + b[i] * t)
}
/// A continuous material field in earth-centered coordinates. Keeping the
/// large coordinates as f64 until the final color avoids stair steps and keeps
/// the date-line seam consistent. This is authored shading, not soil imagery.
fn terrain_color(height: f32, slope: f32, p: GeoPosition) -> [f32; 4] {
    let lat = p.latitude.to_radians();
    let lon = p.longitude.to_radians();
    let x = lon.cos() * lat.cos() * 6_378_137.0;
    let y = lat.sin() * 6_378_137.0;
    let z = lon.sin() * lat.cos() * 6_378_137.0;
    // Smooth stochastic fields avoid the old repeating sinusoidal stripes.
    // Earth-centered sampling preserves the date line and independent tiles.
    let broad = material_noise(x * 0.004, y * 0.004, z * 0.004);
    let patches = material_noise(x * 0.026 + 13., y * 0.026, z * 0.026 - 8.);
    let dry = (p.latitude.abs() as f32 - 24.).abs() < 10.;
    let meadow = if dry {
        [0.49, 0.43, 0.26, 1.]
    } else {
        [0.39, 0.47, 0.26, 1.]
    };
    let moss = if dry {
        [0.39, 0.36, 0.22, 1.]
    } else {
        [0.24, 0.35, 0.22, 1.]
    };
    let soil = [0.53, 0.43, 0.31, 1.];
    let mut color = blend(meadow, moss, (broad * 0.70).clamp(0., 0.75));
    color = blend(color, soil, ((patches - 0.38) * 1.20).clamp(0., 0.45));
    let variation = (broad - 0.5) * 0.075 + (patches - 0.5) * 0.04;
    for component in color.iter_mut().take(3) {
        *component = (*component + variation).clamp(0.04, 0.94);
    }
    // Rock appears on steep relief instead of painting every altitude green.
    let rock = [
        0.52 + broad * 0.10,
        0.51 + broad * 0.09,
        0.47 + broad * 0.08,
        1.,
    ];
    let cliff = ((slope - 0.055) / 0.24).clamp(0., 1.);
    let highland = ((height - 1000.) / 1600.).clamp(0., 0.35) * (0.4 + patches * 0.6);
    color = blend(color, rock, cliff.max(highland));
    let snowline = (3300. - p.latitude.abs() as f32 * 15.).max(250.);
    let snow = ((height - snowline + 120.) / 300.).clamp(0., 1.) * (1. - slope * 0.75);
    color = blend(color, [0.86, 0.91, 0.94, 1.], snow);
    if height < 0. {
        color = blend(color, [0.42, 0.39, 0.27, 1.], 0.55);
    }
    color
}

fn material_noise(x: f64, y: f64, z: f64) -> f32 {
    let smooth = |x: f64| x * x * (3. - 2. * x);
    let cell = [x.floor() as i64, y.floor() as i64, z.floor() as i64];
    let fraction = [
        smooth(x - x.floor()),
        smooth(y - y.floor()),
        smooth(z - z.floor()),
    ];
    let mut result = 0.;
    for dz in 0..2 {
        for dy in 0..2 {
            for dx in 0..2 {
                let mut h = (cell[0] + dx) as u64;
                h = h.wrapping_mul(0x9e3779b185ebca87) ^ (cell[1] + dy) as u64;
                h = h.wrapping_mul(0xc2b2ae3d27d4eb4f) ^ (cell[2] + dz) as u64;
                h ^= h >> 29;
                h = h.wrapping_mul(0x165667b19e3779f9);
                let value = (h & 65535) as f64 / 65535.;
                let weight = [dx, dy, dz]
                    .iter()
                    .enumerate()
                    .map(|(i, &a)| {
                        if a == 0 {
                            1. - fraction[i]
                        } else {
                            fraction[i]
                        }
                    })
                    .product::<f64>();
                result += value * weight;
            }
        }
    }
    result as f32
}

pub fn terrain_mesh(chunk: &WorldChunk) -> Option<Mesh> {
    terrain_mesh_lod(chunk, 0)
}

/// Render-only reduction. Every outer edge retains every original source
/// sample, so neighboring chunks remain watertight even at different tiers.
/// Resident terrain queries continue to use the original height field.
pub fn terrain_mesh_lod(chunk: &WorldChunk, detail: u8) -> Option<Mesh> {
    let frame = LocalFrame::new(chunk.key.center());
    let bounds = chunk.key.bounds();
    let w = chunk.heights.width;
    let h = chunk.heights.height;
    if w < 2 || h < 2 || w > 257 || h > 257 || chunk.heights.samples_m.len() != w * h {
        return None;
    }
    let mut positions = Vec::with_capacity(w * h);
    let mut geographic = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let p = bounds
                .point(
                    x as f64 / (w - 1) as f64,
                    y as f64 / (h - 1) as f64,
                    chunk.heights.samples_m[y * w + x] as f64,
                )
                .ok()?;
            geographic.push(p);
            positions.push(local(&frame, p));
        }
    }
    let mut normals = Vec::with_capacity(w * h);
    let mut colors = Vec::with_capacity(w * h);
    let cover: Vec<_> = chunk
        .features
        .iter()
        .filter(|f| {
            matches!(
                f.kind,
                FeatureKind::Forest | FeatureKind::Farm | FeatureKind::Grass
            )
        })
        .take(96)
        .collect();
    for y in 0..h {
        for x in 0..w {
            let dx = positions[y * w + (x + 1).min(w - 1)] - positions[y * w + x.saturating_sub(1)];
            let dz = positions[(y + 1).min(h - 1) * w + x] - positions[y.saturating_sub(1) * w + x];
            let normal = dz.cross(dx).normalize_or_zero();
            let slope = 1. - normal.y.abs();
            let p = geographic[y * w + x];
            let mut color = terrain_color(chunk.heights.samples_m[y * w + x], slope, p);
            for f in &cover {
                if inside_polygon(p, &f.points) {
                    let mask = match f.kind {
                        FeatureKind::Forest => [0.29, 0.33, 0.21, 1.],
                        FeatureKind::Farm => [0.54, 0.43, 0.27, 1.],
                        _ => [0.40, 0.49, 0.27, 1.],
                    };
                    color = blend(color, mask, 0.6);
                    break;
                }
            }
            normals.push(normal.to_array());
            colors.push(linear_color(color));
        }
    }
    let raw_indices = terrain_indices(w, h, 1usize << detail.min(2));
    let mut kept = Vec::new();
    let mut remap = BTreeMap::new();
    let mut indices = Vec::with_capacity(raw_indices.len());
    for i in raw_indices {
        let next = remap.len() as u32;
        let mapped = *remap.entry(i).or_insert_with(|| {
            kept.push(i as usize);
            next
        });
        indices.push(mapped);
    }
    // Whole-number repetitions at tile edges remove texture phase seams.
    // The nominal repeat spans about 8 m at the equator; it is authored detail.
    let repeats = (40_075_016.685 / (1u64 << chunk.key.level) as f64 / 8.)
        .round()
        .max(1.) as f32;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        kept.iter()
            .map(|&i| positions[i].to_array())
            .collect::<Vec<_>>(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        kept.iter().map(|&i| normals[i]).collect::<Vec<_>>(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_COLOR,
        kept.iter().map(|&i| colors[i]).collect::<Vec<_>>(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        kept.iter()
            .map(|&i| {
                [
                    (i % w) as f32 / (w - 1) as f32 * repeats,
                    (i / w) as f32 / (h - 1) as f32 * repeats,
                ]
            })
            .collect::<Vec<_>>(),
    )
    .with_inserted_indices(Indices::U32(indices));
    let _ = mesh.generate_tangents();
    Some(mesh)
}

fn terrain_indices(w: usize, h: usize, stride: usize) -> Vec<u32> {
    let mut indices = Vec::new();
    if stride == 1 || w <= 2 || h <= 2 {
        for y in 0..h - 1 {
            for x in 0..w - 1 {
                let a = (y * w + x) as u32;
                let c = a + w as u32;
                indices.extend([a, c, a + 1, a + 1, c, c + 1]);
            }
        }
        return indices;
    }
    let axis = |length: usize| {
        let mut values: Vec<_> = (0..length).step_by(stride).collect();
        if values.last().copied() != Some(length - 1) {
            values.push(length - 1);
        }
        // Merge a one-sample tail into its neighbor: a fan center on a shared
        // edge would otherwise create a T-junction for non-power-of-two grids.
        if values.len() > 2 && values[values.len() - 1] - values[values.len() - 2] == 1 {
            values.remove(values.len() - 2);
        }
        values
    };
    let xs = axis(w);
    let ys = axis(h);
    for vertical in ys.windows(2) {
        for horizontal in xs.windows(2) {
            let (x, right) = (horizontal[0], horizontal[1]);
            let (y, bottom) = (vertical[0], vertical[1]);
            let center = ((x + right) / 2, (y + bottom) / 2);
            let mut ring = vec![(x, y)];
            if y == 0 {
                ring.extend((x + 1..right).map(|i| (i, y)));
            }
            ring.push((right, y));
            if right == w - 1 {
                ring.extend((y + 1..bottom).map(|i| (right, i)));
            }
            ring.push((right, bottom));
            if bottom == h - 1 {
                ring.extend((x + 1..right).rev().map(|i| (i, bottom)));
            }
            ring.push((x, bottom));
            if x == 0 {
                ring.extend((y + 1..bottom).rev().map(|i| (x, i)));
            }
            for i in 0..ring.len() {
                let a = ring[i];
                let b = ring[(i + 1) % ring.len()];
                let area = (a.0 as i64 - center.0 as i64) * (b.1 as i64 - center.1 as i64)
                    - (a.1 as i64 - center.1 as i64) * (b.0 as i64 - center.0 as i64);
                if area != 0 {
                    indices.extend([
                        (center.1 * w + center.0) as u32,
                        (b.1 * w + b.0) as u32,
                        (a.1 * w + a.0) as u32,
                    ]);
                }
            }
        }
    }
    indices
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

fn number(feature: &WorldFeature, key: &str) -> Option<f32> {
    feature
        .properties
        .get(key)
        .and_then(|s| terra_stream::providers::parse_osm_length(s))
}
fn tag<'a>(feature: &'a WorldFeature, key: &str) -> &'a str {
    feature
        .properties
        .get(key)
        .map(String::as_str)
        .unwrap_or("")
}
fn tag_color(value: &str, fallback: [f32; 4]) -> [f32; 4] {
    let v = value.trim().to_lowercase();
    if let Some(hex) = v.strip_prefix('#') {
        if hex.len() == 6 {
            if let Ok(rgb) = u32::from_str_radix(hex, 16) {
                return [
                    ((rgb >> 16) & 255) as f32 / 255.,
                    ((rgb >> 8) & 255) as f32 / 255.,
                    (rgb & 255) as f32 / 255.,
                    1.,
                ];
            }
        }
    }
    match v.as_str() {
        "red" => [0.57, 0.22, 0.16, 1.],
        "brown" => [0.43, 0.29, 0.18, 1.],
        "white" => [0.84, 0.83, 0.77, 1.],
        "grey" | "gray" => [0.43, 0.46, 0.46, 1.],
        "black" => [0.19, 0.21, 0.22, 1.],
        "green" => [0.29, 0.40, 0.27, 1.],
        "beige" => [0.75, 0.69, 0.55, 1.],
        "yellow" => [0.79, 0.68, 0.36, 1.],
        "blue" => [0.30, 0.42, 0.53, 1.],
        "orange" => [0.72, 0.38, 0.20, 1.],
        _ => fallback,
    }
}
fn top_triangle(builder: &mut MeshBuilder, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
    if (b - a).cross(c - a).y >= 0. {
        builder.triangle(a, b, c, color);
    } else {
        builder.triangle(a, c, b, color);
    }
}
fn signed_area(points: &[Vec3]) -> f32 {
    (0..points.len())
        .map(|i| {
            points[i].x * points[(i + 1) % points.len()].z
                - points[(i + 1) % points.len()].x * points[i].z
        })
        .sum::<f32>()
        * 0.5
}

fn building(
    builder: &mut MeshBuilder,
    feature: &WorldFeature,
    mut points: Vec<Vec3>,
    triangles: &[[usize; 3]],
    seed: u64,
    lod: u8,
) {
    let lowest = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let ground = points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
    let bottom = lowest + number(feature, "min_height").unwrap_or(0.).clamp(0., 100.);
    let total = feature.height_m.clamp(2.5, 400.);
    let shape = tag(feature, "roof:shape");
    let pitched =
        points.len() == 4 && matches!(shape, "gabled" | "hipped" | "pyramidal" | "skillion");
    let roof_height = if pitched {
        number(feature, "roof:height")
            .unwrap_or((total * 0.24).clamp(1.1, 4.5))
            .clamp(0.4, total * 0.65)
    } else {
        0.
    };
    let wall_top = (ground + total - roof_height).max(bottom + 1.5);
    let area = signed_area(&points);
    let facade = tag_color(
        tag(feature, "building:colour"),
        match tag(feature, "building:material") {
            "brick" => [0.58, 0.34, 0.25, 1.],
            "wood" => [0.48, 0.34, 0.22, 1.],
            "glass" => [0.38, 0.51, 0.55, 1.],
            "stone" => [0.61, 0.59, 0.51, 1.],
            "concrete" => [0.66, 0.66, 0.61, 1.],
            _ => [
                0.70 + (seed % 5) as f32 * 0.025,
                0.67 + (seed % 3) as f32 * 0.025,
                0.56,
                1.,
            ],
        },
    );
    let roof_color = tag_color(
        tag(feature, "roof:colour"),
        match tag(feature, "roof:material") {
            "metal" => [0.37, 0.43, 0.44, 1.],
            "slate" => [0.27, 0.31, 0.34, 1.],
            "grass" => [0.31, 0.42, 0.24, 1.],
            _ => [0.48, 0.25 + (seed % 4) as f32 * 0.025, 0.18, 1.],
        },
    );
    let levels = number(feature, "building:levels")
        .unwrap_or(((wall_top - ground) / 3.).round())
        .clamp(1., 8.) as usize;
    let shop = !tag(feature, "shop").is_empty()
        || matches!(
            tag(feature, "amenity"),
            "cafe" | "restaurant" | "marketplace" | "bank"
        );
    let mut detail_budget = match lod {
        0 => 144,
        1 => 12,
        _ => 0,
    };
    for i in 0..points.len() {
        let j = (i + 1) % points.len();
        let a = Vec3::new(points[i].x, bottom, points[i].z);
        let b = Vec3::new(points[j].x, bottom, points[j].z);
        let at = Vec3::new(a.x, wall_top, a.z);
        let bt = Vec3::new(b.x, wall_top, b.z);
        if area > 0. {
            builder.quad(b, a, at, bt, facade);
        } else {
            builder.quad(a, b, bt, at, facade);
        }
        let edge = b - a;
        let length = edge.length();
        let tangent = edge.normalize_or_zero();
        let outside = Vec3::new(edge.z, 0., -edge.x).normalize_or_zero() * area.signum();
        if lod == 0 && length > 1. {
            // Grounded plinth and cornice make building scale readable while
            // retaining the exact source footprint and height.
            let offset = outside * 0.018;
            let plinth = blend(facade, [0.38, 0.37, 0.32, 1.], 0.55);
            builder.quad(
                a + offset,
                b + offset,
                b + offset + Vec3::Y * 0.32,
                a + offset + Vec3::Y * 0.32,
                plinth,
            );
            builder.quad(
                at + offset - Vec3::Y * 0.15,
                bt + offset - Vec3::Y * 0.15,
                bt + offset,
                at + offset,
                blend(facade, [0.87, 0.84, 0.75, 1.], 0.35),
            );
        }
        let columns = ((length / 3.).floor() as usize).clamp(0, 7);
        if length > 3. && detail_budget > 0 {
            for level in 0..levels {
                for column in 0..columns {
                    if detail_budget == 0 {
                        break;
                    }
                    detail_budget -= 1;
                    let center = a
                        + tangent * (length * (column as f32 + 0.5) / columns as f32)
                        + Vec3::Y * ((wall_top - bottom) * (level as f32 + 0.55) / levels as f32)
                        + outside * 0.035;
                    let half = tangent * (length / (columns.max(1) as f32) * 0.22).min(0.65);
                    let up = Vec3::Y * ((wall_top - bottom) / levels as f32 * 0.22).min(0.62);
                    let glass = [0.19, 0.31 + (seed % 3) as f32 * 0.015, 0.34, 1.];
                    if lod == 0 && level == 0 && column < 4 {
                        let frame_center = center + outside * 0.012;
                        let narrow = tangent * 0.035;
                        let frame_color = blend(facade, [0.90, 0.87, 0.76, 1.], 0.7);
                        for side in [-1., 1.] {
                            let edge_center = frame_center + half * side;
                            builder.quad(
                                edge_center - narrow - up,
                                edge_center + narrow - up,
                                edge_center + narrow + up,
                                edge_center - narrow + up,
                                frame_color,
                            );
                        }
                        builder.quad(
                            frame_center - half + up - Vec3::Y * 0.035,
                            frame_center + half + up - Vec3::Y * 0.035,
                            frame_center + half + up + Vec3::Y * 0.035,
                            frame_center - half + up + Vec3::Y * 0.035,
                            frame_color,
                        );
                    }
                    if area > 0. {
                        builder.quad(
                            center + half - up,
                            center - half - up,
                            center - half + up,
                            center + half + up,
                            glass,
                        );
                    } else {
                        builder.quad(
                            center - half - up,
                            center + half - up,
                            center + half + up,
                            center - half + up,
                            glass,
                        );
                    }
                    // A narrow facade strip reads as a sill at gameplay distances.
                    // Keep it flat: a cylinder here would multiply every window's budget.
                    let sill_center = center - up * 1.06 + outside * 0.008;
                    let sill_up = Vec3::Y * 0.035;
                    let sill = [0.81, 0.79, 0.70, 1.];
                    if area > 0. {
                        builder.quad(
                            sill_center + half - sill_up,
                            sill_center - half - sill_up,
                            sill_center - half + sill_up,
                            sill_center + half + sill_up,
                            sill,
                        );
                    } else {
                        builder.quad(
                            sill_center - half - sill_up,
                            sill_center + half - sill_up,
                            sill_center + half + sill_up,
                            sill_center - half + sill_up,
                            sill,
                        );
                    }
                }
            }
        }
        if i == 0 && length > 2.5 && lod < 2 {
            let center = (a + b) * 0.5 + outside * 0.06 + Vec3::Y * 1.1;
            let half = tangent * 0.55;
            let up = Vec3::Y * 1.1;
            let door = [0.28, 0.24, 0.19, 1.];
            if area > 0. {
                builder.quad(
                    center + half - up,
                    center - half - up,
                    center - half + up,
                    center + half + up,
                    door,
                );
            } else {
                builder.quad(
                    center - half - up,
                    center + half - up,
                    center + half + up,
                    center - half + up,
                    door,
                );
            }
            if shop && length > 4. && lod == 0 {
                for stripe in 0..8 {
                    let left = (a + b) * 0.5
                        + tangent * ((stripe as f32 / 8. - 0.5) * length * 0.65)
                        + outside * 0.1
                        + Vec3::Y * 2.7;
                    let right = left + tangent * (length * 0.65 / 8.);
                    let c = if stripe % 2 == 0 {
                        [0.24, 0.40, 0.33, 1.]
                    } else {
                        [0.86, 0.81, 0.62, 1.]
                    };
                    builder.quad(
                        left,
                        right,
                        right + outside * 1.4 - Vec3::Y * 0.3,
                        left + outside * 1.4 - Vec3::Y * 0.3,
                        c,
                    );
                }
            }
        }
    }
    for p in &mut points {
        p.y = wall_top;
    }
    if pitched {
        if shape == "gabled" {
            let (a, b, c, d) =
                if (points[1] - points[0]).length() > (points[2] - points[1]).length() {
                    (points[1], points[2], points[3], points[0])
                } else {
                    (points[0], points[1], points[2], points[3])
                };
            let r1 = (a + b) * 0.5 + Vec3::Y * roof_height;
            let r2 = (c + d) * 0.5 + Vec3::Y * roof_height;
            top_triangle(builder, a, b, r1, facade);
            top_triangle(builder, c, d, r2, facade);
            top_triangle(builder, a, r1, r2, roof_color);
            top_triangle(builder, a, r2, d, roof_color);
            top_triangle(builder, b, c, r2, roof_color);
            top_triangle(builder, b, r2, r1, roof_color);
            if lod == 0 {
                builder.cylinder(
                    r1,
                    r2,
                    0.085,
                    blend(roof_color, [0.73, 0.52, 0.37, 1.], 0.2),
                    6,
                );
            }
        } else if shape == "skillion" {
            points[0].y += roof_height;
            points[1].y += roof_height;
            for &[a, b, c] in triangles {
                top_triangle(builder, points[a], points[b], points[c], roof_color);
            }
        } else {
            let center = points.iter().sum::<Vec3>() / points.len() as f32 + Vec3::Y * roof_height;
            for i in 0..points.len() {
                top_triangle(
                    builder,
                    points[i],
                    points[(i + 1) % points.len()],
                    center,
                    roof_color,
                );
            }
        }
    } else {
        for &[a, b, c] in triangles {
            top_triangle(builder, points[a], points[b], points[c], roof_color);
        }
        if points.len() <= 8 && total < 45. && lod == 0 {
            let center = points.iter().sum::<Vec3>() / points.len() as f32;
            builder.cuboid(
                center + Vec3::Y * 0.35,
                Vec3::new(1.2, 0.7, 1.6),
                [0.42, 0.45, 0.44, 1.],
            );
        }
    }
    if pitched && seed % 3 == 0 && lod == 0 {
        let center = points[0].lerp(points[2], 0.4) + Vec3::Y * (roof_height + 0.4);
        builder.cuboid(center, Vec3::new(0.65, 1.6, 0.7), [0.49, 0.34, 0.28, 1.]);
    }
}

fn linear_feature(
    builder: &mut MeshBuilder,
    chunk: &WorldChunk,
    frame: &LocalFrame,
    feature: &WorldFeature,
    lod: u8,
) {
    let water = feature.kind == FeatureKind::Waterway;
    let width = if water {
        feature.height_m.clamp(0.6, 100.)
    } else {
        road_width_m(feature) as f32
    };
    let deck_height = road_deck_height(chunk, feature);
    let bridge = deck_height.is_some();
    let pedestrian = matches!(
        tag(feature, "highway"),
        "path" | "footway" | "steps" | "cycleway" | "track"
    );
    let surface_tag = tag(feature, "surface");
    let color = if water {
        [0.13, 0.37, 0.42, 1.]
    } else if pedestrian || matches!(surface_tag, "gravel" | "dirt" | "ground" | "compacted") {
        [0.53, 0.47, 0.35, 1.]
    } else if matches!(surface_tag, "paving_stones" | "cobblestone") {
        [0.47, 0.47, 0.43, 1.]
    } else {
        [0.25, 0.28, 0.28, 1.]
    };
    let mut subdivisions = 0usize;
    for pair in feature.points.windows(2).take(128) {
        let length = pair[0].surface_distance_m(pair[1]) as f32;
        if length < 0.02 {
            continue;
        }
        let spacing = match lod {
            0 => 8.,
            1 => 16.,
            _ => 30.,
        };
        let steps = ((length / spacing).ceil() as usize).clamp(1, 128);
        for i in 0..steps {
            if subdivisions >= 512 {
                break;
            }
            subdivisions += 1;
            let lerp = |t: f64| GeoPosition {
                latitude: pair[0].latitude + (pair[1].latitude - pair[0].latitude) * t,
                longitude: pair[0].longitude + (pair[1].longitude - pair[0].longitude) * t,
                altitude_m: 0.,
            };
            let pa = lerp(i as f64 / steps as f64);
            let pb = lerp((i + 1) as f64 / steps as f64);
            let mut a = surface(chunk, frame, pa, if water { 0.05 } else { 0.16 });
            let mut b = surface(chunk, frame, pb, if water { 0.05 } else { 0.16 });
            if let Some(deck) = deck_height {
                a = local(
                    frame,
                    GeoPosition {
                        altitude_m: deck,
                        ..pa
                    },
                );
                b = local(
                    frame,
                    GeoPosition {
                        altitude_m: deck,
                        ..pb
                    },
                );
            }
            let side = (b - a).cross(Vec3::Y).normalize_or_zero();
            builder.quad(
                a - side * width * 0.5,
                a + side * width * 0.5,
                b + side * width * 0.5,
                b - side * width * 0.5,
                color,
            );
            if !water && lod < 2 {
                let edge_color = if pedestrian {
                    [0.42, 0.41, 0.29, 1.]
                } else {
                    [0.62, 0.61, 0.53, 1.]
                };
                for s in [-1., 1.] {
                    let offset = side * (width * 0.5 * s);
                    builder.quad(
                        a + offset,
                        a + offset + side * s * 0.35,
                        b + offset + side * s * 0.35,
                        b + offset,
                        edge_color,
                    );
                    if bridge {
                        builder.cylinder(
                            a + offset + Vec3::Y * 0.8,
                            b + offset + Vec3::Y * 0.8,
                            0.07,
                            [0.44, 0.46, 0.43, 1.],
                            5,
                        );
                        if i % 3 == 0 {
                            builder.cylinder(
                                a + offset,
                                a + offset + Vec3::Y * 0.85,
                                0.055,
                                [0.44, 0.46, 0.43, 1.],
                                5,
                            );
                        }
                    }
                }
                if !pedestrian && width > 4.5 && i % 2 == 0 {
                    let center_a = a + Vec3::Y * 0.025;
                    let center_b = a.lerp(b, 0.62) + Vec3::Y * 0.025;
                    builder.quad(
                        center_a - side * 0.06,
                        center_a + side * 0.06,
                        center_b + side * 0.06,
                        center_b - side * 0.06,
                        [0.86, 0.83, 0.66, 1.],
                    );
                }
                if bridge && i % 5 == 0 {
                    let ground = surface(chunk, frame, pa, 0.).y;
                    builder.cuboid(
                        Vec3::new(a.x, (ground + a.y) * 0.5, a.z),
                        Vec3::new(width * 0.35, (a.y - ground).max(0.1), 0.8),
                        [0.52, 0.53, 0.49, 1.],
                    );
                }
            }
        }
    }
}

pub fn feature_meshes(
    chunk: &WorldChunk,
    budget: usize,
    removed: &BTreeSet<String>,
) -> BTreeMap<Layer, Mesh> {
    feature_meshes_lod(chunk, budget, removed, 0)
}

/// Feature-only distance tiers preserve the full terrain heightfield and source
/// metadata. Reduced far detail never changes simulation/collision geometry.
pub fn feature_meshes_lod(
    chunk: &WorldChunk,
    budget: usize,
    removed: &BTreeSet<String>,
    lod: u8,
) -> BTreeMap<Layer, Mesh> {
    let lod = lod.min(2);
    let frame = LocalFrame::new(chunk.key.center());
    let mut batches: BTreeMap<Layer, MeshBuilder> = BTreeMap::new();
    let mut trees_left = match lod {
        0 => 160usize,
        1 => 48,
        _ => 24,
    };
    // Mapped tree nodes get a reserved slot; distribute procedural canopy
    // samples across polygons so an early forest cannot strip the rest bare.
    let mut explicit_trees_left = chunk
        .features
        .iter()
        .take(budget)
        .filter(|f| f.kind == FeatureKind::Tree && !f.points.is_empty() && !removed.contains(&f.id))
        .count()
        .min(trees_left);
    let mut canopy_areas_left = chunk
        .features
        .iter()
        .take(budget)
        .filter(|f| {
            (f.kind == FeatureKind::Forest
                || (f.kind == FeatureKind::Farm && tag(f, "landuse") == "orchard"))
                && f.points.len() >= 3
                && !removed.contains(&f.id)
        })
        .count();
    let mut detail_left = match lod {
        0 => 900usize,
        1 => 120,
        _ => 0,
    };
    for feature in chunk.features.iter().take(budget) {
        if removed.contains(&feature.id) {
            continue;
        }
        let builder = batches.entry(feature.kind.layer()).or_default();
        if builder.positions.len() > 420_000 {
            continue;
        }
        let seed = hash(&feature.id);
        if feature.kind == FeatureKind::Tree {
            if trees_left > 0 {
                if let Some(&p) = feature.points.first() {
                    trees_left -= 1;
                    explicit_trees_left = explicit_trees_left.saturating_sub(1);
                    builder.tree_lod(
                        surface(chunk, &frame, p, 0.),
                        feature.height_m.clamp(3., 28.),
                        match tag(feature, "leaf_type") {
                            "broadleaved" => seed / 3 * 3,
                            "needleleaved" => seed / 3 * 3 + 1,
                            _ => seed,
                        },
                        lod,
                    );
                }
            }
            continue;
        }
        if matches!(feature.kind, FeatureKind::Road | FeatureKind::Waterway) {
            linear_feature(builder, chunk, &frame, feature, lod);
            continue;
        }
        if feature.kind == FeatureKind::PointOfInterest {
            if let Some(&p) = feature.points.first() {
                let base = surface(chunk, &frame, p, 0.);
                // Semantic nodes get a modest map marker, not an invented building footprint.
                builder.cylinder(base, base + Vec3::Y * 2.2, 0.045, [0.35, 0.39, 0.36, 1.], 6);
                let c = if tag(feature, "amenity") == "drinking_water" {
                    [0.22, 0.54, 0.68, 1.]
                } else if !tag(feature, "shop").is_empty() {
                    [0.84, 0.63, 0.28, 1.]
                } else {
                    [0.38, 0.60, 0.48, 1.]
                };
                builder.cuboid(base + Vec3::Y * 1.9, Vec3::new(0.75, 0.45, 0.12), c);
            }
            continue;
        }
        let points = polygon_points(feature);
        if points.len() < 3 || points.len() > 512 {
            continue;
        }
        let mut vertices: Vec<_> = points
            .iter()
            .map(|&p| surface(chunk, &frame, p, 0.06))
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
            building(builder, feature, vertices, &triangles, seed, lod);
            continue;
        }
        if feature.kind == FeatureKind::Water {
            let h = number(feature, "ele")
                .unwrap_or_else(|| vertices.iter().map(|p| p.y).fold(f32::INFINITY, f32::min))
                + 0.12;
            for p in &mut vertices {
                p.y = h;
            }
            for [a, b, c] in triangles {
                top_triangle(
                    builder,
                    vertices[a],
                    vertices[b],
                    vertices[c],
                    [0.15, 0.39, 0.43, 1.],
                );
            }
            for i in 0..vertices.len() {
                let a = vertices[i];
                let b = vertices[(i + 1) % vertices.len()];
                let side = (b - a).cross(Vec3::Y).normalize_or_zero() * 0.6;
                builder.quad(
                    a - side,
                    a + side,
                    b + side,
                    b - side,
                    [0.42, 0.47, 0.32, 1.],
                );
            }
            continue;
        }
        // Forest/farm/meadow material is painted on the actual height field, so
        // polygon triangulation never floats across hills as a flat cover sheet.
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
        let orchard = feature.kind == FeatureKind::Farm && tag(feature, "landuse") == "orchard";
        if feature.kind == FeatureKind::Forest || orchard {
            let allowance = trees_left
                .saturating_sub(explicit_trees_left)
                .div_ceil(canopy_areas_left.max(1));
            canopy_areas_left = canopy_areas_left.saturating_sub(1);
            let mut placed = 0;
            let mut rng = seed;
            for sample in 0..if orchard { 42 } else { 200 } {
                if trees_left == 0 || placed >= allowance {
                    break;
                }
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                let u = if orchard {
                    (sample % 6) as f64 / 6. + 1. / 12.
                } else {
                    (rng >> 32) as f64 / u32::MAX as f64
                };
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                let v = if orchard {
                    (sample / 6) as f64 / 7. + 1. / 14.
                } else {
                    (rng >> 32) as f64 / u32::MAX as f64
                };
                let p = GeoPosition {
                    latitude: min_lat + (max_lat - min_lat) * u,
                    longitude: min_lon + (max_lon - min_lon) * v,
                    altitude_m: 0.,
                };
                if inside_polygon(p, &points) {
                    trees_left -= 1;
                    placed += 1;
                    builder.tree_lod(
                        surface(chunk, &frame, p, 0.),
                        if orchard {
                            4. + (rng % 3) as f32
                        } else {
                            8. + (rng % 9) as f32
                        },
                        if orchard {
                            rng / 3 * 3
                        } else {
                            match tag(feature, "leaf_type") {
                                "broadleaved" => rng / 3 * 3,
                                "needleleaved" => rng / 3 * 3 + 1,
                                _ => rng,
                            }
                        },
                        lod,
                    );
                }
            }
        } else if feature.kind == FeatureKind::Farm {
            for row in 0..18 {
                for column in 0..20 {
                    if detail_left == 0 {
                        break;
                    }
                    let lat = min_lat + (max_lat - min_lat) * (row as f64 + 0.5) / 18.;
                    let p = GeoPosition {
                        latitude: lat,
                        longitude: min_lon + (max_lon - min_lon) * (column as f64 + 0.1) / 20.,
                        altitude_m: 0.,
                    };
                    let q = GeoPosition {
                        longitude: min_lon + (max_lon - min_lon) * (column as f64 + 0.9) / 20.,
                        ..p
                    };
                    if inside_polygon(p, &points) && inside_polygon(q, &points) {
                        detail_left -= 1;
                        let a = surface(chunk, &frame, p, 0.1);
                        let b = surface(chunk, &frame, q, 0.1);
                        let side = (b - a).cross(Vec3::Y).normalize_or_zero() * 0.45;
                        builder.quad(
                            a - side,
                            a + side,
                            b + side,
                            b - side,
                            if matches!(tag(feature, "crop"), "wheat" | "barley" | "oat" | "maize")
                            {
                                [0.64, 0.59, 0.30, 1.]
                            } else if (row + seed as usize) % 3 == 0 {
                                [0.60, 0.56, 0.27, 1.]
                            } else {
                                [0.45, 0.52, 0.24, 1.]
                            },
                        );
                    }
                }
            }
        }
    }
    batches
        .into_iter()
        .filter_map(|(layer, b)| {
            let mut mesh = b.finish()?;
            if layer == Layer::Water {
                if let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                {
                    let uvs: Vec<_> = points
                        .iter()
                        .map(|p| [p[0] * 0.035, p[2] * 0.035])
                        .collect();
                    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
                    let _ = mesh.generate_tangents();
                }
            }
            Some((layer, mesh))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    fn positions(mesh: &Mesh) -> &[[f32; 3]] {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
            VertexAttributeValues::Float32x3(values) => values,
            _ => panic!("expected geometric vertices"),
        }
    }
    #[test]
    fn authored_terrain_material_is_continuous_at_the_date_line() {
        let east = terrain_color(
            400.,
            0.15,
            GeoPosition {
                latitude: 48.,
                longitude: 180.,
                altitude_m: 400.,
            },
        );
        let west = terrain_color(
            400.,
            0.15,
            GeoPosition {
                latitude: 48.,
                longitude: -180.,
                altitude_m: 400.,
            },
        );
        for i in 0..4 {
            assert!((east[i] - west[i]).abs() < 0.0001);
        }
        let steep = terrain_color(400., 0.7, GeoPosition::new(48., 11., 400.).unwrap());
        assert!(
            steep
                .iter()
                .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
        );
    }
    #[test]
    fn heightfield_uses_shared_vertices_smooth_normals_and_detail_uvs() {
        let key =
            terra_core::TileKey::at(GeoPosition::new(48.137, 11.575, 0.).unwrap(), 15).unwrap();
        let chunk = terra_stream::procedural_chunk(key, 33, 42);
        let mesh = terrain_mesh(&chunk).unwrap();
        assert_eq!(positions(&mesh).len(), 33 * 33);
        assert_eq!(mesh.indices().unwrap().len(), 32 * 32 * 6);
        assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
        let VertexAttributeValues::Float32x3(normals) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap()
        else {
            panic!()
        };
        for normal in normals {
            let n = Vec3::from_array(*normal);
            assert!((n.length() - 1.).abs() < 0.001);
            assert!(n.y > 0.);
        }
        let heights: Vec<_> = positions(&mesh).iter().map(|p| p[1]).collect();
        assert!(
            heights.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - heights.iter().copied().fold(f32::INFINITY, f32::min)
                > 10.
        );
    }
    #[test]
    fn distant_terrain_reduces_interiors_without_cracks_or_changed_source_heights() {
        let key =
            terra_core::TileKey::at(GeoPosition::new(48.137, 11.575, 0.).unwrap(), 15).unwrap();
        let chunk = terra_stream::procedural_chunk(key, 65, 42);
        let before = chunk.heights.samples_m.clone();
        let near = terrain_mesh_lod(&chunk, 0).unwrap();
        let middle = terrain_mesh_lod(&chunk, 1).unwrap();
        let far = terrain_mesh_lod(&chunk, 2).unwrap();
        assert!(middle.count_vertices() < near.count_vertices() * 3 / 5);
        assert!(far.count_vertices() < near.count_vertices() / 5);
        assert!(far.indices().unwrap().len() < near.indices().unwrap().len() / 5);
        assert_eq!(chunk.heights.samples_m, before);
        for (width, height) in [(65, 65), (34, 18), (66, 34), (2, 5)] {
            for stride in [1, 2, 4] {
                let triangles = terrain_indices(width, height, stride);
                let used: BTreeSet<_> = triangles.iter().copied().collect();
                let mut edges = BTreeMap::<(u32, u32), usize>::new();
                let mut area = 0.;
                for triangle in triangles.chunks_exact(3) {
                    let point = |i: u32| {
                        Vec2::new((i as usize % width) as f32, (i as usize / width) as f32)
                    };
                    let cross = (point(triangle[1]) - point(triangle[0]))
                        .perp_dot(point(triangle[2]) - point(triangle[0]));
                    assert!(cross < 0., "upward winding, nondegenerate triangles");
                    area += cross.abs() * 0.5;
                    for i in 0..3 {
                        let a = triangle[i];
                        let b = triangle[(i + 1) % 3];
                        *edges.entry((a.min(b), a.max(b))).or_default() += 1;
                    }
                }
                assert_eq!(area, ((width - 1) * (height - 1)) as f32);
                for (&(a, b), &count) in &edges {
                    assert!(count <= 2, "nonmanifold terrain edge");
                    if count == 1 {
                        let (ax, ay) = (a as usize % width, a as usize / width);
                        let (bx, by) = (b as usize % width, b as usize / width);
                        assert!(
                            (ax == 0 && bx == 0)
                                || (ay == 0 && by == 0)
                                || (ax == width - 1 && bx == width - 1)
                                || (ay == height - 1 && by == height - 1),
                            "interior hole"
                        );
                    }
                }
                for y in 0..height {
                    for x in 0..width {
                        if x == 0 || y == 0 || x == width - 1 || y == height - 1 {
                            assert!(
                                used.contains(&((y * width + x) as u32)),
                                "missing source border sample"
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn terrain_palette_exposes_relief_and_water_has_tangent_space_detail() {
        let point = GeoPosition::new(48., 11., 400.).unwrap();
        let meadow = terrain_color(400., 0.01, point);
        let cliff = terrain_color(400., 0.6, point);
        let snow = terrain_color(4000., 0.01, point);
        assert!(cliff[0] > meadow[0]);
        assert!(snow[0] > cliff[0] && snow[2] > cliff[2]);
        let key =
            terra_core::TileKey::at(GeoPosition::new(48.137, 11.575, 0.).unwrap(), 15).unwrap();
        let chunk = terra_stream::procedural_chunk(key, 33, 42);
        let meshes = feature_meshes_lod(&chunk, 128, &BTreeSet::new(), 0);
        let water = &meshes[&Layer::Water];
        assert!(water.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
        assert!(water.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
    }
    #[test]
    fn enriched_source_geometry_is_finite_and_stays_within_render_budget() {
        let key =
            terra_core::TileKey::at(GeoPosition::new(48.137, 11.575, 0.).unwrap(), 15).unwrap();
        let chunk = terra_stream::procedural_chunk(key, 33, 42);
        let meshes = feature_meshes(&chunk, 128, &BTreeSet::new());
        for layer in [
            Layer::Buildings,
            Layer::Roads,
            Layer::Vegetation,
            Layer::Water,
        ] {
            assert!(meshes.contains_key(&layer));
        }
        let mut count = 0;
        for mesh in meshes.values() {
            for p in positions(mesh) {
                assert!(p.iter().all(|v| v.is_finite()));
            }
            count += positions(mesh).len();
        }
        assert!(count < 300_000, "bounded source detail: {count} vertices");
        for seed in [0, 1, u64::MAX - 1, u64::MAX] {
            for lod in 0..3 {
                let mut tree = MeshBuilder::default();
                tree.tree_lod(Vec3::ZERO, 35., seed, lod);
                let mesh = tree.finish().unwrap();
                assert!(mesh.count_vertices() < 3000);
                assert!(positions(&mesh).iter().flatten().all(|v| v.is_finite()));
            }
        }
    }
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
