//! The terrain as the client draws and picks it: a heightfield built from the map's layers in
//! content (ADR-0014), never from the sim.
//!
//! Cell `(cx, cy)` has its centre at world `((cx + 0.5) × cell, height, (cy + 0.5) × cell)`;
//! `y` is north, as in sim space, so world `z` is sim `y`.
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use fl_content::{KindTable, MapDef, TerrainDef};

/// The most 1 m steps [`ray_ground`] marches before giving up, so a grazing ray stays cheap.
const MAX_MARCH_STEPS: u32 = 4096;
/// Bisection steps after the march brackets the surface.
const BISECT_STEPS: u32 = 8;

/// A map's heights and colours, one per cell, y north (`index = cy × cells.x + cx`).
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainView {
    pub cells: UVec2,
    /// Cell side in metres.
    pub cell_m: f32,
    /// Height of each cell centre in metres.
    pub heights_m: Vec<f32>,
    /// Linear RGB of each cell's terrain type.
    pub colors: Vec<[f32; 3]>,
}

impl TerrainView {
    /// The view of `map`'s layers, coloured by `terrains`; `None` for a map without layers.
    /// A palette id that names no loaded terrain falls back to the map's ground colour.
    #[must_use]
    pub fn from_content(map: &MapDef, terrains: &KindTable<TerrainDef>) -> Option<TerrainView> {
        let layers = map.layers.as_ref()?;
        let (lo, hi) = map.terrain.as_ref()?.height_m;
        let (lo, hi) = (lo.to_f32(), hi.to_f32());
        let ground = [map.ground_color.0, map.ground_color.1, map.ground_color.2];
        let palette: Vec<[f32; 3]> = layers
            .palette
            .iter()
            .map(|id| {
                terrains
                    .get(id)
                    .map_or(ground, |t| [t.color.0, t.color.1, t.color.2])
            })
            .collect();
        let heights_m = layers
            .heights
            .iter()
            .map(|&raw| lo + (hi - lo) * (f32::from(raw) / f32::from(u16::MAX)))
            .collect();
        let colors = layers
            .types
            .iter()
            .map(|&t| palette.get(usize::from(t)).copied().unwrap_or(ground))
            .collect();
        Some(TerrainView {
            cells: UVec2::new(layers.cells_x, layers.cells_y),
            cell_m: map.cell_m.to_f32(),
            heights_m,
            colors,
        })
    }

    /// The height at `(x_m, y_m)`: bilinear between the four nearest cell centres, and the
    /// edge cells' height beyond the outermost centres.
    #[must_use]
    pub fn height_at(&self, x_m: f32, y_m: f32) -> f32 {
        if self.cells.x == 0 || self.cells.y == 0 {
            return 0.0;
        }
        let (max_x, max_y) = ((self.cells.x - 1) as f32, (self.cells.y - 1) as f32);
        let gx = (x_m / self.cell_m - 0.5).clamp(0.0, max_x);
        let gy = (y_m / self.cell_m - 0.5).clamp(0.0, max_y);
        let (x0, y0) = (gx.floor(), gy.floor());
        let (fx, fy) = (gx - x0, gy - y0);
        let (x0, y0) = (x0 as u32, y0 as u32);
        let (x1, y1) = (
            (x0 + 1).min(self.cells.x - 1),
            (y0 + 1).min(self.cells.y - 1),
        );
        let h = |x, y| self.heights_m[self.index(x, y)];
        let south = h(x0, y0) + (h(x1, y0) - h(x0, y0)) * fx;
        let north = h(x0, y1) + (h(x1, y1) - h(x0, y1)) * fx;
        south + (north - south) * fy
    }

    /// Whether `(x_m, y_m)` lies on the map, `[0, cells × cell_m]` on both axes.
    fn contains(&self, x_m: f32, y_m: f32) -> bool {
        let (w, d) = (
            self.cells.x as f32 * self.cell_m,
            self.cells.y as f32 * self.cell_m,
        );
        (0.0..=w).contains(&x_m) && (0.0..=d).contains(&y_m)
    }

    fn index(&self, x: u32, y: u32) -> usize {
        y as usize * self.cells.x as usize + x as usize
    }

    /// The lowest and highest cell height.
    fn height_range(&self) -> (f32, f32) {
        self.heights_m
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &h| {
                (lo.min(h), hi.max(h))
            })
    }

    /// The surface normal at cell `(x, y)` from central height differences.
    fn normal(&self, x: u32, y: u32) -> [f32; 3] {
        let h = |x: u32, y: u32| self.heights_m[self.index(x, y)];
        let (xm, xp) = (x.saturating_sub(1), (x + 1).min(self.cells.x - 1));
        let (ym, yp) = (y.saturating_sub(1), (y + 1).min(self.cells.y - 1));
        let dx = (h(xp, y) - h(xm, y)) / ((xp - xm).max(1) as f32 * self.cell_m);
        let dz = (h(x, yp) - h(x, ym)) / ((yp - ym).max(1) as f32 * self.cell_m);
        Vec3::new(-dx, 1.0, -dz).normalize().to_array()
    }
}

/// The terrain mesh: a vertex per cell centre with its height, colour and normal, and two
/// upward-facing triangles per quad of four neighbouring centres.
#[must_use]
pub fn terrain_mesh(t: &TerrainView) -> Mesh {
    let (nx, ny) = (t.cells.x, t.cells.y);
    let mut positions = Vec::with_capacity(t.heights_m.len());
    let mut normals = Vec::with_capacity(t.heights_m.len());
    let mut colors = Vec::with_capacity(t.heights_m.len());
    for y in 0..ny {
        for x in 0..nx {
            let i = t.index(x, y);
            positions.push([
                (x as f32 + 0.5) * t.cell_m,
                t.heights_m[i],
                (y as f32 + 0.5) * t.cell_m,
            ]);
            normals.push(t.normal(x, y));
            let [r, g, b] = t.colors[i];
            colors.push([r, g, b, 1.0]);
        }
    }
    let mut indices = Vec::new();
    for y in 0..ny.saturating_sub(1) {
        for x in 0..nx.saturating_sub(1) {
            let (a, b) = (t.index(x, y) as u32, t.index(x + 1, y) as u32);
            let (c, d) = (t.index(x, y + 1) as u32, t.index(x + 1, y + 1) as u32);
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

/// Where `ray` meets the ground: the `y = 0` plane without terrain; otherwise the first point
/// where the ray passes below the heightfield, found by marching 1 m steps from the plane of
/// the highest cell and bisecting [`BISECT_STEPS`] times. The point's `y` is the terrain height.
/// `None` when the ray points up or level, starts underground, or misses the map.
#[must_use]
pub fn ray_ground(ray: Ray3d, terrain: Option<&TerrainView>) -> Option<Vec3> {
    let Some(t) = terrain else {
        return ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y));
    };
    let dy = ray.direction.y;
    let (lo, hi) = t.height_range();
    if dy >= 0.0 || ray.origin.y <= lo {
        return None;
    }
    let start = ((hi - ray.origin.y) / dy).max(0.0);
    let end = (lo - ray.origin.y) / dy;
    let below = |d: f32| {
        let p = ray.get_point(d);
        // Off the map there is no ground to pass below.
        t.contains(p.x, p.z) && p.y < t.height_at(p.x, p.z)
    };
    // A ray that starts inside the height range and already under the surface hits nothing.
    if start == 0.0 && below(0.0) {
        return None;
    }
    let mut near = start;
    for _ in 0..MAX_MARCH_STEPS {
        let mut far = (near + 1.0).min(end);
        // The plane of the lowest cell is at or under the surface everywhere.
        if far >= end || below(far) {
            for _ in 0..BISECT_STEPS {
                let mid = 0.5 * (near + far);
                if below(mid) {
                    far = mid;
                } else {
                    near = mid;
                }
            }
            let p = ray.get_point(far);
            if !t.contains(p.x, p.z) {
                return None;
            }
            return Some(Vec3::new(p.x, t.height_at(p.x, p.z), p.z));
        }
        near = far;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(cells: UVec2, heights_m: Vec<f32>) -> TerrainView {
        let n = heights_m.len();
        TerrainView {
            cells,
            cell_m: 4.0,
            heights_m,
            colors: vec![[0.5; 3]; n],
        }
    }

    #[test]
    fn ray_hits_a_slope_between_marches() {
        // Heights 0 and 8 over one 4 m cell: a vertical ray at the midpoint hits y = 4.
        let t = view(UVec2::new(2, 1), vec![0.0, 8.0]);
        let ray = Ray3d::new(Vec3::new(4.0, 50.0, 2.0), Dir3::NEG_Y);
        let hit = ray_ground(ray, Some(&t)).unwrap();
        assert!((hit.y - 4.0).abs() < 0.05, "{hit}");
        assert!((hit.x - 4.0).abs() < 0.05, "{hit}");
    }

    #[test]
    fn upward_ray_misses() {
        let t = view(UVec2::new(2, 1), vec![0.0, 8.0]);
        let ray = Ray3d::new(Vec3::new(4.0, 50.0, 2.0), Dir3::Y);
        assert_eq!(ray_ground(ray, Some(&t)), None);
    }
}
