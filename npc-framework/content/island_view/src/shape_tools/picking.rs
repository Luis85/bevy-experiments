//! Terrain sampling and cursor picking shared by the Shape tools.
use super::model::{Cell, Limits};
use bevy::prelude::*;
use island_content::village::{
    self,
    terrain::{TerrainState, TerrainSurface},
};

/// The authoritative cell under `(x, z)`, for either island profile.
pub(crate) fn terrain_cell(world: &World, x: i32, z: i32) -> Option<Cell> {
    if village::frontier::enabled(world) {
        return village::frontier::terrain::cell(world, x, z).map(|cell| Cell {
            height_mm: cell.height_mm,
            surface: cell.surface,
        });
    }
    // The legacy command refuses the outermost ring of its 64 x 64 map.
    if !(-31..31).contains(&x) || !(-31..31).contains(&z) {
        return None;
    }
    world
        .get_resource::<TerrainState>()?
        .cell(x, z)
        .map(|cell| Cell {
            height_mm: cell.height_mm,
            surface: cell.surface,
        })
}

pub(crate) fn limits(world: &World) -> Limits {
    if village::frontier::enabled(world) {
        Limits::REGIONAL
    } else {
        Limits::LEGACY
    }
}

pub(crate) fn revision(world: &World) -> u64 {
    if village::frontier::enabled(world) {
        world
            .get_resource::<village::frontier::terrain::TerrainState>()
            .map_or(0, |terrain| terrain.revision)
    } else {
        world
            .get_resource::<TerrainState>()
            .map_or(0, |terrain| terrain.revision)
    }
}

/// The cell under the pointer, or `None` over open sky or outside the map.
pub(crate) fn cursor_cell(world: &mut World) -> Option<(i32, i32)> {
    let cursor = world
        .query::<&Window>()
        .iter(world)
        .find_map(Window::cursor_position)?;
    let ray = world
        .query_filtered::<(&Camera, &GlobalTransform), With<crate::RtsCamera>>()
        .iter(world)
        .find_map(|(camera, transform)| camera.viewport_to_world(transform, cursor).ok())?;
    if village::frontier::enabled(world) {
        crate::colony_scene::living_scene::terrain_hit(world, ray)
    } else {
        terrain_hit(world.get_resource::<TerrainState>()?, ray)
    }
}

/// Intersect every bounded column's top, so raised cells stay pickable at their real elevation.
pub(crate) fn terrain_hit(terrain: &TerrainState, ray: Ray3d) -> Option<(i32, i32)> {
    if ray.direction.y.abs() < 0.000_01 {
        return None;
    }
    let mut best: Option<(f32, i32, i32)> = None;
    for z in -32..32 {
        for x in -32..32 {
            let Some(cell) = terrain.cell(x, z) else {
                continue;
            };
            let y = if cell.surface == TerrainSurface::Water {
                -0.12
            } else {
                cell.height_mm as f32 / 1000.0
            };
            let distance = (y - ray.origin.y) / ray.direction.y;
            if distance < 0.0 || best.is_some_and(|(old, _, _)| old <= distance) {
                continue;
            }
            let point = ray.get_point(distance);
            if point.x >= x as f32
                && point.x < (x + 1) as f32
                && point.z >= z as f32
                && point.z < (z + 1) as f32
            {
                best = Some((distance, x, z));
            }
        }
    }
    best.map(|(_, x, z)| (x, z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use island_content::village::terrain::TerrainCell;

    #[test]
    fn terrain_picking_hits_raised_cells_at_their_actual_height() {
        let mut terrain = TerrainState {
            cells: vec![
                TerrainCell {
                    height_mm: 0,
                    surface: TerrainSurface::Grass
                };
                4096
            ],
            revision: 0,
        };
        terrain.cells[32 * 64 + 32].height_mm = 3000;
        // At y=3 the oblique ray hits cell(0,0); at y=0 it would hit cell(1,0).
        let ray = Ray3d::new(
            Vec3::new(-0.8, 6.0, 0.5),
            Dir3::new(Vec3::new(0.4, -1.0, 0.0)).unwrap(),
        );
        assert_eq!(terrain_hit(&terrain, ray), Some((0, 0)));
        assert_eq!(
            terrain_hit(
                &terrain,
                Ray3d::new(Vec3::new(80.0, 10.0, 80.0), Dir3::NEG_Y)
            ),
            None
        );
    }
}
