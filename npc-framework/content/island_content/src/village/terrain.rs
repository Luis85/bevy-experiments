//! Recorded, bounded terrain editing for the colony showcase.
//!
//! Terrain owns the static navigation layer. Structures retain their dynamic
//! blockers. Edits never relocate actors and must keep all dry ground connected.
use super::geography;
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::commands::CommandError;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

pub const WIDTH: usize = 64;
pub const MAX_HEIGHT_MM: i32 = 3000;
pub const MAX_WALK_HEIGHT_MM: i32 = 1500;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(rename_all = "snake_case")]
pub enum TerrainSurface {
    Grass,
    Soil,
    Stone,
    Water,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct TerrainCell {
    pub height_mm: i32,
    pub surface: TerrainSurface,
}
impl TerrainCell {
    pub fn walkable(&self) -> bool {
        self.surface != TerrainSurface::Water && self.height_mm <= MAX_WALK_HEIGHT_MM
    }
    pub fn navigation(self) -> CellInfo {
        CellInfo {
            flags: CellFlags {
                blocking: !self.walkable(),
                occluding: self.height_mm > MAX_WALK_HEIGHT_MM,
                tags: 0,
            },
            extra_cost: match self.surface {
                TerrainSurface::Grass => 2,
                TerrainSurface::Stone => 4,
                _ => 0,
            },
        }
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct TerrainState {
    /// Row-major 64 by 64 cells, starting at (-32, -32).
    pub cells: Vec<TerrainCell>,
    pub revision: u64,
}
impl TerrainState {
    pub fn cell(&self, x: i32, z: i32) -> Option<&TerrainCell> {
        index(x, z).and_then(|index| self.cells.get(index))
    }
}
impl SimHashResource for TerrainState {
    const NAME: &'static str = "island_colony_terrain_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u64(self.revision);
        h.u32(self.cells.len() as u32);
        for cell in &self.cells {
            h.i32(cell.height_mm);
            h.u8(cell.surface as u8);
        }
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

fn index(x: i32, z: i32) -> Option<usize> {
    if !(-32..32).contains(&x) || !(-32..32).contains(&z) {
        return None;
    }
    Some(((z + 32) as usize) * WIDTH + (x + 32) as usize)
}

pub fn register(app: &mut App) {
    app.register_command::<Terraform>();
}

/// Replace just the known shoreline spans; keep all IDs and building blockers.
pub fn spawn(world: &mut World) {
    if world.contains_resource::<TerrainState>() {
        return;
    }
    let geo = *geography::for_world(world);
    let shore: Vec<_> = (-32..32)
        .flat_map(|z| geo.shoreline_segments(z).into_iter().flatten())
        .collect();
    let old: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|entity| {
            world.get::<GridOccluder>(*entity).is_none()
                && world.get::<GridBlocker>(*entity).is_some_and(|blocker| {
                    world.get::<SimPos>(*entity).is_some_and(|pos| {
                        shore
                            .iter()
                            .any(|span| span.pos == *pos && span.footprint == blocker.0)
                    })
                })
        })
        .collect();
    for entity in old {
        world.entity_mut(entity).remove::<GridBlocker>();
    }
    let mut cells = Vec::with_capacity(WIDTH * WIDTH);
    for z in -32..32 {
        for x in -32..32 {
            let land = geo.land_cell(x, z);
            let at = SimPos::new(x * 1000 + 500, z * 1000 + 500);
            let road = land
                && geo.roads.iter().any(|road| {
                    i64::from(at.x).abs_diff(i64::from(road.x)) * 2 <= road.width_mm as u64
                        && i64::from(at.z).abs_diff(i64::from(road.z)) * 2 <= road.depth_mm as u64
                });
            let cell = TerrainCell {
                height_mm: if land { 0 } else { -500 },
                surface: if road {
                    TerrainSurface::Soil
                } else if land {
                    TerrainSurface::Grass
                } else {
                    TerrainSurface::Water
                },
            };
            world
                .resource_mut::<SpatialGrid>()
                .set_static(CellCoord::new(x, z), cell.navigation());
            cells.push(cell);
        }
    }
    world.insert_resource(TerrainState { cells, revision: 0 });
}

/// Absolute brush values make repeated commands and replay unambiguous.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Terraform {
    pub x: i32,
    pub z: i32,
    pub radius: u32,
    pub height_mm: i32,
    pub surface: TerrainSurface,
}
impl Terraform {
    pub fn check(&self, world: &World) -> Result<(), String> {
        super::frontier::terrain::terraform_check(
            world,
            self.x,
            self.z,
            self.radius,
            self.height_mm,
            self.surface,
        )
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        super::frontier::terrain::terraform(
            world,
            self.x,
            self.z,
            self.radius,
            self.height_mm,
            self.surface,
        )
    }
}
impl ContentCommand for Terraform {
    const NAME: &'static str = "Terraform";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world)
            .map_err(|reason| CommandError::Unavailable { reason })
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
}
