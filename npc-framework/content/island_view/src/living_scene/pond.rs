//! Rounded pond banks. View geometry only: the cells stay as the simulation
//! knows them.
//!
//! The terrain's wetland ponds are axis-aligned blocks of water cells,
//! often well below the turf around them (up to 4.5 m on the shipped
//! island). Drawn cell by cell they were rectangles with straight, vertical
//! banks. Here the ground inside a pond's cells is a smooth slope instead:
//! it starts exactly at the neighbouring land cell's own height on the
//! cells' edge (so no riser shows), eases down and meets the water a little
//! way in, further in the deeper the pond lies. The slope follows a distance
//! field from the land with the pond's corners rounded (up to [`ROUND_MAX`]
//! m, less for a small or narrow pond) and its edge pulled in by a wobble
//! seeded by position, so the waterline the water tint makes against this
//! ground is a soft, natural outline. The outline only ever moves into the
//! pond's cells, never into the land, so the pond tint over the cells still
//! covers all of its water, and every pond keeps water at its middle.
//!
//! Built once with the terrain chunk's mesh, never per frame.

use super::{Geometry, SHALLOWS_Y};
use crate::island_atmosphere::ground;
use bevy::math::Vec3;
use island_content::village::frontier::{
    terrain::{TerrainState, TerrainSurface},
    world_spec::Biome,
};

/// Largest corner radius (m) of a pond's outline.
const ROUND_MAX: f32 = 5.0;
/// Distance (m) from the land to the waterline on a narrow pond (a channel
/// a cell or two wide) and, before the drop widens it, on a roomy one.
const SHORE_NARROW: f32 = 0.35;
const SHORE_WIDE: f32 = 0.6;
/// Extra distance to the waterline per metre the land stands over the pond.
const SHORE_PER_DROP: f32 = 0.3;
/// The waterline stays within this share of the pond's width from each
/// side, the wobble within [`WOBBLE_SHARE`]: the middle stays wet.
const SHORE_SHARE: f32 = 0.35;
const WOBBLE_SHARE: f32 = 0.12;
/// How far (m) the wobble pulls the edge into the pond, at most: a fine
/// ripple, and a slow sway along long edges.
const RIPPLE_NARROW: f32 = 0.1;
const RIPPLE_WIDE: f32 = 0.45;
const RIPPLE_PERIOD: f32 = 2.7;
const SWAY_WIDE: f32 = 0.7;
const SWAY_PERIOD: f32 = 8.0;
const RIPPLE_SALT: u32 = 71;
const SWAY_SALT: u32 = 72;
/// Pond width (cells) from which a pond counts as roomy.
const ROOMY_FROM: f32 = 1.5;
const ROOMY_AT: f32 = 4.0;
/// Widths across are counted up to this many cells each way.
const WIDTH_RUN: i32 = 8;
/// The slope aims a little below the water's surface at the waterline's
/// distance, so it crosses the surface cleanly instead of grazing it.
const SLOPE_AIM: f32 = SHALLOWS_Y - 0.12;
/// Sub-quads per cell edge. A power of two, so a vertex on an edge two
/// cells share has bit-identical coordinates in both: no cracks.
const STEPS: i32 = 4;
/// Cells around a point searched for land. Beyond it a point is deep water.
const REACH: i32 = 4;
/// How much darker the bank's wet earth at the water is than its turf.
const WET_SHADE: f32 = 0.35;

/// Whether `(x, z)` is a pond cell: wetland water.
pub(super) fn is_pond(terrain: &TerrainState, x: i32, z: i32) -> bool {
    terrain
        .cell(x, z)
        .is_some_and(|cell| cell.surface == TerrainSurface::Water && cell.biome == Biome::Wetland)
}

/// Any water, pond or sea; beyond the map is sea.
fn is_water(terrain: &TerrainState, x: i32, z: i32) -> bool {
    terrain
        .cell(x, z)
        .is_none_or(|cell| cell.surface == TerrainSurface::Water)
}

fn smoothstep(from: f32, to: f32, value: f32) -> f32 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A pond's convex corner: the grid point `(x, z)`, the quadrant the pond
/// lies in (`sx`, `sz`: +1 or -1) and its rounding radius.
struct Corner {
    x: f32,
    z: f32,
    sx: f32,
    sz: f32,
    radius: f32,
}

/// The bank shape of the ponds in and around one terrain chunk.
pub(super) struct PondField<'a> {
    terrain: &'a TerrainState,
    corners: Vec<Corner>,
    /// Grid-corner cache over `[x0, x0 + side)`: the pond's width (cells)
    /// across there.
    x0: i32,
    z0: i32,
    side: i32,
    width: Vec<f32>,
}

/// What the land around a point says: the distance (m) to the pond's
/// rounded edge (negative where a rounded corner leaves the point dry) and
/// the height (m) of the land the bank falls from.
struct Shore {
    distance: f32,
    top: f32,
}

impl<'a> PondField<'a> {
    /// The field for the `size` x `size` cells at `(x, z)`, or `None` when
    /// none of them is a pond cell.
    pub(super) fn around(terrain: &'a TerrainState, x: i32, z: i32, size: i32) -> Option<Self> {
        if !(z..z + size).any(|cz| (x..x + size).any(|cx| is_pond(terrain, cx, cz))) {
            return None;
        }
        // A point reads corners up to ROUND_MAX away; its normal samples
        // reach a little beyond the chunk.
        let reach = ROUND_MAX.ceil() as i32 + 2;
        let mut corners = Vec::new();
        for gz in z - reach..=z + size + reach {
            for gx in x - reach..=x + size + reach {
                for (sx, sz) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                    if let Some(radius) = corner_radius(terrain, gx, gz, sx, sz) {
                        corners.push(Corner {
                            x: gx as f32,
                            z: gz as f32,
                            sx: sx as f32,
                            sz: sz as f32,
                            radius,
                        });
                    }
                }
            }
        }
        let (x0, z0, side) = (x - 1, z - 1, size + 3);
        let mut width = Vec::with_capacity((side * side) as usize);
        for gz in z0..z0 + side {
            for gx in x0..x0 + side {
                width.push(width_at(terrain, gx, gz));
            }
        }
        Some(Self {
            terrain,
            corners,
            x0,
            z0,
            side,
            width,
        })
    }

    /// The pond's width (cells) at a point, bilinear between grid corners.
    fn width(&self, px: f32, pz: f32) -> f32 {
        let (cx, cz) = (px.floor() as i32, pz.floor() as i32);
        let (fx, fz) = (px - cx as f32, pz - cz as f32);
        let at = |gx: i32, gz: i32| {
            let (i, j) = (gx - self.x0, gz - self.z0);
            if (0..self.side).contains(&i) && (0..self.side).contains(&j) {
                self.width[(j * self.side + i) as usize]
            } else {
                width_at(self.terrain, gx, gz)
            }
        };
        let near = at(cx, cz) + (at(cx + 1, cz) - at(cx, cz)) * fx;
        let far = at(cx, cz + 1) + (at(cx + 1, cz + 1) - at(cx, cz + 1)) * fx;
        near + (far - near) * fz
    }

    /// Distance to the land and the land's height at a point. The height
    /// is the land cells' own, weighted steeply by nearness: on the edge a
    /// pond cell shares with a land cell it is exactly that cell's height,
    /// so the bank meets the turf with no step and its riser stays hidden.
    fn shore(&self, px: f32, pz: f32) -> Shore {
        let (cx, cz) = (px.floor() as i32, pz.floor() as i32);
        let mut distance = (REACH + 1) as f32;
        let (mut touching, mut touching_count) = (0.0_f32, 0);
        let (mut weighted, mut weights) = (0.0_f32, 0.0_f32);
        for z in cz - REACH..=cz + REACH {
            for x in cx - REACH..=cx + REACH {
                let Some(cell) = self.terrain.cell(x, z) else {
                    continue;
                };
                if cell.surface == TerrainSurface::Water {
                    continue;
                }
                let height = cell.height_mm as f32 / 1000.0;
                let dx = (x as f32 - px).max(px - (x + 1) as f32).max(0.0);
                let dz = (z as f32 - pz).max(pz - (z + 1) as f32).max(0.0);
                let d2 = dx * dx + dz * dz;
                distance = distance.min(d2.sqrt());
                if d2 == 0.0 {
                    touching += height;
                    touching_count += 1;
                } else {
                    let weight = 1.0 / (d2 * d2);
                    weighted += height * weight;
                    weights += weight;
                }
            }
        }
        let top = if touching_count > 0 {
            touching / touching_count as f32
        } else if weights > 0.0 {
            weighted / weights
        } else {
            0.0
        };
        for corner in &self.corners {
            let u = (px - corner.x) * corner.sx;
            let v = (pz - corner.z) * corner.sz;
            let r = corner.radius;
            if (0.0..=r).contains(&u) && (0.0..=r).contains(&v) {
                distance = distance.min(r - (u - r).hypot(v - r));
            }
        }
        Shore { distance, top }
    }

    /// Height (m) of the pond's ground at a point, and whether it lies on
    /// the bed (under the water, out of sight).
    fn sample(&self, px: f32, pz: f32) -> (f32, bool) {
        let shore = self.shore(px, pz);
        let width = self.width(px, pz);
        let room = smoothstep(ROOMY_FROM, ROOMY_AT, width);
        let ripple = ground::value_noise(px, pz, RIPPLE_PERIOD, RIPPLE_SALT)
            * (RIPPLE_NARROW + (RIPPLE_WIDE - RIPPLE_NARROW) * room);
        let sway = ground::value_noise(px, pz, SWAY_PERIOD, SWAY_SALT) * SWAY_WIDE * room;
        let wobble = (ripple + sway).min(WOBBLE_SHARE * width);
        let drop = (shore.top - 0.15).max(0.0);
        let to_water =
            (SHORE_NARROW + (SHORE_WIDE - SHORE_NARROW) * room + SHORE_PER_DROP * drop * room)
                .min(SHORE_SHARE * width);
        // 0 on the land's edge, 1 where the bank meets the water.
        let q = ((shore.distance - wobble) / to_water).max(0.0);
        if q < 1.0 {
            // Eases out of the turf, steepening towards the water.
            (shore.top + (SLOPE_AIM - shore.top) * q * q, false)
        } else {
            let under = smoothstep(1.0, 1.6, q);
            (
                SLOPE_AIM + (ground::BEACH_FOOT - SLOPE_AIM) * under,
                under >= 1.0,
            )
        }
    }

    /// Height (m) of the pond's ground at a point.
    pub(super) fn height(&self, px: f32, pz: f32) -> f32 {
        self.sample(px, pz).0
    }

    /// The bank over pond cell `(cx, cz)`: a smooth sub-grid coloured from
    /// the cell's corner colours (`(0, 0)`, `(1, 0)`, `(0, 1)`, `(1, 1)`),
    /// darker where it is wet. A cell wholly on the bed draws nothing: the
    /// water covers it.
    pub(super) fn bank(&self, geometry: &mut Geometry, cx: i32, cz: i32, colors: [[f32; 4]; 4]) {
        let side = (STEPS + 1) as usize;
        let step = 1.0 / STEPS as f32;
        let mut points = Vec::with_capacity(side * side);
        let mut on_bed = true;
        for j in 0..=STEPS {
            for i in 0..=STEPS {
                let (px, pz) = (cx as f32 + i as f32 * step, cz as f32 + j as f32 * step);
                let (height, bed) = self.sample(px, pz);
                on_bed &= bed;
                points.push([px, height, pz]);
            }
        }
        if on_bed {
            return;
        }
        let eps = step * 0.5;
        let normals: Vec<[f32; 3]> = points
            .iter()
            .map(|p| {
                let (px, pz) = (p[0], p[2]);
                let ddx = (self.height(px + eps, pz) - self.height(px - eps, pz)) / (2.0 * eps);
                let ddz = (self.height(px, pz + eps) - self.height(px, pz - eps)) / (2.0 * eps);
                Vec3::new(-ddx, 1.0, -ddz).normalize().to_array()
            })
            .collect();
        let [c00, c10, c01, c11] = colors;
        let colours: Vec<[f32; 4]> = (0..side * side)
            .map(|k| {
                let (fx, fz) = ((k % side) as f32 * step, (k / side) as f32 * step);
                let wet = 1.0 - WET_SHADE * smoothstep(0.35, 0.0, points[k][1] - SHALLOWS_Y);
                let mut colour = [0.0, 0.0, 0.0, 1.0];
                for (channel, value) in colour.iter_mut().take(3).enumerate() {
                    let near = c00[channel] + (c10[channel] - c00[channel]) * fx;
                    let far = c01[channel] + (c11[channel] - c01[channel]) * fx;
                    *value = (near + (far - near) * fz) * wet;
                }
                colour
            })
            .collect();
        let at = |i: usize, j: usize| j * side + i;
        for j in 0..STEPS as usize {
            for i in 0..STEPS as usize {
                let quad = [at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j)];
                geometry.smooth_quad(
                    quad.map(|k| points[k]),
                    quad.map(|k| normals[k]),
                    quad.map(|k| colours[k]),
                );
            }
        }
    }
}

/// Whether grid point `(x, z)` is a convex corner of a pond lying in the
/// quadrant `(sx, sz)` from it, and how round it is drawn: up to
/// [`ROUND_MAX`], within the square of water from the corner and half the
/// pond's extent along both edges, so a small pond becomes an oval and a
/// narrow channel ends in a half circle.
fn corner_radius(terrain: &TerrainState, x: i32, z: i32, sx: i32, sz: i32) -> Option<f32> {
    let qx = if sx > 0 { x } else { x - 1 };
    let qz = if sz > 0 { z } else { z - 1 };
    if !is_pond(terrain, qx, qz) || is_water(terrain, qx - sx, qz) || is_water(terrain, qx, qz - sz)
    {
        return None;
    }
    let limit = ROUND_MAX.ceil() as i32;
    let square = (1..=limit)
        .take_while(|k| {
            (0..*k).all(|i| (0..*k).all(|j| is_water(terrain, qx + sx * i, qz + sz * j)))
        })
        .last()
        .unwrap_or(0);
    let run = |dx: i32, dz: i32| {
        (0..2 * limit)
            .take_while(|n| is_water(terrain, qx + dx * n, qz + dz * n))
            .count() as f32
    };
    let radius = ROUND_MAX
        .min(square as f32)
        .min(0.5 * run(sx, 0))
        .min(0.5 * run(0, sz));
    (radius > 0.0).then_some(radius)
}

/// The water's width (cells) across at grid point `(x, z)`: the narrowest
/// of its water cells' widths, each the shorter of its row and column run.
/// A channel keeps a short shore and little wobble and never runs dry.
fn width_at(terrain: &TerrainState, x: i32, z: i32) -> f32 {
    let width = |cx: i32, cz: i32| {
        let run = |dx: i32, dz: i32| {
            (1..=WIDTH_RUN)
                .take_while(|n| is_water(terrain, cx + dx * n, cz + dz * n))
                .count() as i32
        };
        (1 + run(1, 0) + run(-1, 0)).min(1 + run(0, 1) + run(0, -1)) as f32
    };
    [(-1, -1), (0, -1), (-1, 0), (0, 0)]
        .into_iter()
        .filter(|(dx, dz)| is_water(terrain, x + dx, z + dz))
        .map(|(dx, dz)| width(x + dx, z + dz))
        .fold((2 * WIDTH_RUN + 1) as f32, f32::min)
}

impl Geometry {
    /// A quad with its own normal per vertex, split along the diagonal
    /// whose ends are closer in height, so a slope folds along itself.
    fn smooth_quad(
        &mut self,
        points: [[f32; 3]; 4],
        normals: [[f32; 3]; 4],
        colors: [[f32; 4]; 4],
    ) {
        let start = self.vertices.len() as u32;
        self.vertices.extend(points);
        self.normals.extend(normals);
        self.colors.extend(colors);
        self.uvs.extend(points.map(super::grain_uv));
        let level = |a: usize, b: usize| (points[a][1] - points[b][1]).abs();
        self.indices.extend(
            if level(0, 2) <= level(1, 3) {
                [0, 1, 2, 0, 2, 3]
            } else {
                [0, 1, 3, 1, 2, 3]
            }
            .map(|corner| start + corner),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use island_content::village::frontier::terrain::TerrainCell;

    /// Grass at +0.15 m (64 x 64 cells, -32..32) with wetland water in the
    /// given cells, as the generator lays its ponds.
    fn meadow(pond: impl Fn(i32, i32) -> bool) -> TerrainState {
        let mut cells = Vec::with_capacity(4096);
        for index in 0..4096_i32 {
            let (x, z) = (index % 64 - 32, index / 64 - 32);
            cells.push(if pond(x, z) {
                TerrainCell {
                    height_mm: -200,
                    surface: TerrainSurface::Water,
                    biome: Biome::Wetland,
                }
            } else {
                TerrainCell {
                    height_mm: 150,
                    surface: TerrainSurface::Grass,
                    biome: Biome::Wetland,
                }
            });
        }
        TerrainState {
            width: 64,
            height: 64,
            cells,
            revision: 0,
        }
    }

    /// The waterline along a ray from `from`: the first point where the
    /// bank rises above the water's surface.
    fn waterline(field: &PondField, from: [f32; 2], angle: f32) -> [f32; 2] {
        let (dx, dz) = (angle.cos(), angle.sin());
        let at = |r: f32| [from[0] + dx * r, from[1] + dz * r];
        let wet = |r: f32| {
            let [x, z] = at(r);
            field.height(x, z) < super::super::SHALLOWS_Y
        };
        let mut far = 0.0;
        while wet(far) {
            far += 0.02;
        }
        let mut near = far - 0.02;
        for _ in 0..24 {
            let mid = 0.5 * (near + far);
            if wet(mid) {
                near = mid;
            } else {
                far = mid;
            }
        }
        at(near)
    }

    /// A 4 x 3 pond is drawn as a rounded pool: its waterline stays inside
    /// its cells, keeps well clear of the block's four corners and turns
    /// gently everywhere, with no right-angle corner left.
    #[test]
    fn a_small_block_pond_has_a_rounded_outline_inside_its_cells() {
        let terrain = meadow(|x, z| (0..4).contains(&x) && (0..3).contains(&z));
        let field = PondField::around(&terrain, 0, 0, 16).expect("a pond here");
        let centre = [2.0, 1.5];
        assert!(field.height(centre[0], centre[1]) < super::super::SHALLOWS_Y);
        let samples = 720;
        let outline: Vec<[f32; 2]> = (0..samples)
            .map(|k| {
                waterline(
                    &field,
                    centre,
                    k as f32 / samples as f32 * std::f32::consts::TAU,
                )
            })
            .collect();
        for p in &outline {
            assert!(
                p[0] > 0.05 && p[0] < 3.95 && p[1] > 0.05 && p[1] < 2.95,
                "{p:?} lies inside the pond's cells"
            );
        }
        // Each corner of the cell box is dry, and the water keeps a rounded
        // distance from it (a square outline 0.4 m in would be 0.57 m away).
        for corner in [[0.0, 0.0], [4.0, 0.0], [0.0, 3.0], [4.0, 3.0]] {
            let nearest = outline
                .iter()
                .map(|p| (p[0] - corner[0]).hypot(p[1] - corner[1]))
                .fold(f32::MAX, f32::min);
            assert!(nearest > 0.85, "corner {corner:?}: water {nearest} m away");
        }
        // The outline turns gently all the way round: no sharp corner.
        let turn = (0..samples)
            .map(|k| {
                let [a, b, c] = [k, (k + 1) % samples, (k + 2) % samples].map(|i| outline[i]);
                let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
                let cross = u[0] * v[1] - u[1] * v[0];
                let dot = u[0] * v[0] + u[1] * v[1];
                cross.atan2(dot).abs().to_degrees()
            })
            .fold(0.0_f32, f32::max);
        assert!(turn < 20.0, "the outline turns {turn} degrees at once");
        // The bank meets the turf at the land's height: no step at the
        // cells' edge.
        for x in [0.5, 1.75, 3.25] {
            assert!((field.height(x, 0.0) - 0.15).abs() < 1e-4);
            assert!((field.height(x, 3.0) - 0.15).abs() < 1e-4);
        }
    }

    /// A channel one cell wide keeps water along its whole length, though
    /// narrower than its cells.
    #[test]
    fn a_narrow_channel_never_runs_dry() {
        let terrain = meadow(|x, z| (0..10).contains(&x) && z == 0);
        let field = PondField::around(&terrain, 0, 0, 16).expect("a pond here");
        for i in 0..40 {
            let x = 0.5 + i as f32 * 0.225;
            assert!(
                field.height(x, 0.5) < super::super::SHALLOWS_Y,
                "dry at {x}"
            );
        }
    }

    /// The same cells give the same bank whichever chunk asks: neighbouring
    /// chunks meet without a crack, and a rebuild draws the same mesh.
    #[test]
    fn the_bank_is_the_same_from_every_chunk_and_every_build() {
        let terrain = meadow(|x, z| (10..24).contains(&x) && (-6..6).contains(&z));
        let left = PondField::around(&terrain, 0, -16, 16).expect("pond");
        let right = PondField::around(&terrain, 16, -16, 16).expect("pond");
        let again = PondField::around(&terrain, 0, -16, 16).expect("pond");
        for j in 0..=48 {
            let z = -6.0 + j as f32 * 0.25;
            for x in [15.5, 16.0, 16.25] {
                assert_eq!(left.height(x, z).to_bits(), right.height(x, z).to_bits());
                assert_eq!(left.height(x, z).to_bits(), again.height(x, z).to_bits());
            }
        }
        let mesh = || {
            super::super::terrain_mesh(
                &terrain,
                &ground::Roads::default(),
                0,
                -16,
                island_content::village::Season::Autumn,
            )
            .expect("land and pond here")
        };
        let (first, second) = (mesh(), mesh());
        for attribute in [
            bevy::mesh::Mesh::ATTRIBUTE_POSITION,
            bevy::mesh::Mesh::ATTRIBUTE_NORMAL,
            bevy::mesh::Mesh::ATTRIBUTE_COLOR,
        ] {
            assert_eq!(
                format!("{:?}", first.attribute(attribute)),
                format!("{:?}", second.attribute(attribute)),
            );
        }
        // The bank is there: vertices off the cell grid, inside the pond.
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            first.attribute(bevy::mesh::Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        assert!(points.iter().any(|p| p[0] > 10.0 && p[0].fract() == 0.25));
    }

    /// On the shipped island, where the land stands up to 4.5 m over the
    /// ponds and steps from cell to cell, every pond cell's bank starts
    /// exactly at the height of the land cell beside it, all along the edge
    /// they share, so no riser or step shows; and every pond keeps open water.
    #[test]
    fn on_the_shipped_island_every_bank_meets_its_land_without_a_step() {
        let app = island_content::new_sim_app(7, "living_world");
        let terrain = app.world().resource::<TerrainState>();
        let (mut edges, mut tall, mut inner, mut inner_wet, mut ponds) = (0, 0, 0, 0, 0);
        for (coord, _) in terrain.iter() {
            let (cx, cz) = (coord.x, coord.z);
            if !is_pond(terrain, cx, cz) {
                continue;
            }
            ponds += 1;
            let field = PondField::around(terrain, cx, cz, 1).expect("a pond cell");
            let open = (-3..=3).all(|j| (-3..=3).all(|i| is_water(terrain, cx + i, cz + j)));
            if open {
                inner += 1;
                let middle = field.height(cx as f32 + 0.5, cz as f32 + 0.5);
                inner_wet += usize::from(middle < SHALLOWS_Y);
            }
            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let Some(land) = terrain.cell(cx + dx, cz + dz) else {
                    continue;
                };
                if land.surface == TerrainSurface::Water {
                    continue;
                }
                edges += 1;
                let top = land.height_mm as f32 / 1000.0;
                tall += usize::from(top > ground::BEACH_TOP);
                // The edge's ends are shared with the land diagonal to the
                // pond cell, a riser of the land's own; inside it, one cell.
                for k in 1..STEPS {
                    let s = k as f32 / STEPS as f32;
                    let (px, pz) = match (dx, dz) {
                        (-1, 0) => (cx as f32, cz as f32 + s),
                        (1, 0) => (cx as f32 + 1.0, cz as f32 + s),
                        (0, -1) => (cx as f32 + s, cz as f32),
                        _ => (cx as f32 + s, cz as f32 + 1.0),
                    };
                    let bank = field.height(px, pz);
                    assert!(
                        (bank - top).abs() < 1e-4,
                        "pond cell ({cx}, {cz}) at ({px}, {pz}): bank {bank} m, land {top} m"
                    );
                }
            }
        }
        assert!(
            ponds > 500 && edges > 300,
            "{ponds} pond cells, {edges} land edges"
        );
        assert!(
            tall > 0,
            "some ponds lie under land taller than the beach cap"
        );
        // The banks take a pond's edge cells; its open middle, every cell
        // three or more cells from land, is always water.
        assert!(
            inner > 0 && inner_wet == inner,
            "{inner_wet} of {inner} inner cells wet"
        );
    }
}
