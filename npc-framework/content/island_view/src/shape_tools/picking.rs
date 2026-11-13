//! Terrain sampling and cursor picking shared by the Shape tools.
use super::model::{Cell, Limits};
use bevy::prelude::*;
use island_content::village;

/// The authoritative cell under `(x, z)`.
pub(crate) fn terrain_cell(world: &World, x: i32, z: i32) -> Option<Cell> {
    village::frontier::terrain::cell(world, x, z).map(|cell| Cell {
        height_mm: cell.height_mm,
        surface: cell.surface,
    })
}

pub(crate) fn limits(_world: &World) -> Limits {
    Limits::REGIONAL
}

pub(crate) fn revision(world: &World) -> u64 {
    world
        .get_resource::<village::frontier::terrain::TerrainState>()
        .map_or(0, |terrain| terrain.revision)
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
    crate::colony_scene::living_scene::terrain_hit(world, ray)
}
