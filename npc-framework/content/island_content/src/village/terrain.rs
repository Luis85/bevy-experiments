//! Recorded, bounded terrain editing for the colony showcase.
//!
//! Terrain owns the static navigation layer. Structures retain their dynamic
//! blockers. Edits never relocate actors and must keep all dry ground connected.
use super::{ResourcePatch, VillagePlace, Villager, colony, geography, homestead};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::commands::CommandError;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeSet, VecDeque};

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
    if colony::enabled(app.world()) {
        if !super::frontier::enabled(app.world()) {
            app.register_sim_resource::<TerrainState>();
        }
        app.register_command::<Terraform>();
    }
}

/// Replace just the known shoreline spans; keep all IDs and building blockers.
pub fn spawn(world: &mut World) {
    if !colony::enabled(world) || world.contains_resource::<TerrainState>() {
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
    fn brush(&self) -> Result<Vec<CellCoord>, String> {
        if self.radius > 3 || !(-31..31).contains(&self.x) || !(-31..31).contains(&self.z) {
            return Err("brush must be inside the island map with radius 0..3 cells".into());
        }
        if !(-1000..=MAX_HEIGHT_MM).contains(&self.height_mm) {
            return Err("terrain height must be -1000..3000 millimetres".into());
        }
        if (self.surface == TerrainSurface::Water && self.height_mm > 0)
            || (self.surface != TerrainSurface::Water && self.height_mm < 0)
        {
            return Err(
                "water height must be at or below sea level; dry ground cannot be below sea level"
                    .into(),
            );
        }
        let radius = self.radius as i32;
        let mut cells = Vec::new();
        for z in self.z - radius..=self.z + radius {
            for x in self.x - radius..=self.x + radius {
                if (-31..31).contains(&x)
                    && (-31..31).contains(&z)
                    && (x - self.x).pow(2) + (z - self.z).pow(2) <= radius.pow(2)
                {
                    cells.push(CellCoord::new(x, z));
                }
            }
        }
        Ok(cells)
    }
    pub fn check(&self, world: &World) -> Result<(), String> {
        if super::frontier::enabled(world) {
            return super::frontier::terrain::terraform_check(
                world,
                self.x,
                self.z,
                self.radius,
                self.height_mm,
                self.surface,
            );
        }
        let terrain = world
            .get_resource::<TerrainState>()
            .ok_or("terrain editing requires the island colony")?;
        let brush = self.brush()?;
        let value = TerrainCell {
            height_mm: self.height_mm,
            surface: self.surface,
        };
        let changed: Vec<_> = brush
            .into_iter()
            .filter(|cell| terrain.cell(cell.x, cell.z) != Some(&value))
            .collect();
        if changed.is_empty() {
            return Ok(());
        }
        let grid = world.resource::<SpatialGrid>();
        let mut dynamic = BTreeSet::new();
        for entity in world.resource::<SimIndex>().map.values() {
            let Some(pos) = world.get::<SimPos>(*entity).copied() else {
                continue;
            };
            if let Some(blocker) = world.get::<GridBlocker>(*entity) {
                dynamic.extend(grid.footprint_cells(pos, blocker.0));
            }
            let fixed = world.get::<VillagePlace>(*entity).is_some()
                || world.get::<ResourcePatch>(*entity).is_some()
                || world
                    .get::<super::hunting::WildlifeHabitat>(*entity)
                    .is_some()
                || world.get::<homestead::HomesteadPlot>(*entity).is_some();
            let occupied = world.get::<Villager>(*entity).is_some()
                || world.get::<super::hunting::Wildlife>(*entity).is_some()
                || world.get::<colony::AwaitingArrival>(*entity).is_some();
            let protected = if fixed {
                2200_i64.pow(2)
            } else {
                1000_i64.pow(2)
            };
            if (fixed || occupied)
                && changed
                    .iter()
                    .any(|cell| grid.center(*cell).dist2(pos) <= protected)
            {
                return Err(
                    "brush overlaps a resident, workplace, resource patch or homestead site".into(),
                );
            }
            if let Some(plot) = homestead::definition(world, *entity)
                && changed.iter().any(|cell| {
                    let at = grid.center(*cell);
                    i64::from(at.x).abs_diff(i64::from(plot.x)) <= (plot.width_mm / 2) as u64
                        && i64::from(at.z).abs_diff(i64::from(plot.z)) <= (plot.depth_mm / 2) as u64
                })
            {
                return Err("brush overlaps a reserved cottage or garden footprint".into());
            }
            if let Some(person) = world.get::<Villager>(*entity)
                && changed
                    .iter()
                    .any(|cell| grid.center(*cell).dist2(person.home) <= 1000_i64.pow(2))
            {
                return Err("brush overlaps an assigned home entrance".into());
            }
        }
        if changed.iter().any(|cell| dynamic.contains(cell)) {
            return Err("brush overlaps a building or other solid object".into());
        }
        let mut candidate = terrain.clone();
        for cell in &changed {
            candidate.cells[index(cell.x, cell.z).expect("checked cell")] = value;
        }
        let passable = |cell: CellCoord| {
            candidate
                .cell(cell.x, cell.z)
                .is_some_and(TerrainCell::walkable)
                && !dynamic.contains(&cell)
        };
        let origin = CellCoord::new(0, 0);
        if !passable(origin) {
            return Err("the central village crossroads must remain open".into());
        }
        let mut reached = BTreeSet::from([origin]);
        let mut queue = VecDeque::from([origin]);
        while let Some(cell) = queue.pop_front() {
            for (dx, dz) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let next = CellCoord::new(cell.x + dx, cell.z + dz);
                if passable(next) && reached.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        for z in -32..32 {
            for x in -32..32 {
                let cell = CellCoord::new(x, z);
                if passable(cell) && !reached.contains(&cell) {
                    return Err("terrain would disconnect walkable land from the village".into());
                }
            }
        }
        Ok(())
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        if super::frontier::enabled(world) {
            return super::frontier::terrain::terraform(
                world,
                self.x,
                self.z,
                self.radius,
                self.height_mm,
                self.surface,
            );
        }
        self.check(world)?;
        let brush = self.brush()?;
        let value = TerrainCell {
            height_mm: self.height_mm,
            surface: self.surface,
        };
        let mut changed = 0;
        for cell in brush {
            let index = index(cell.x, cell.z).expect("checked cell");
            if world.resource::<TerrainState>().cells[index] != value {
                world.resource_mut::<TerrainState>().cells[index] = value;
                world
                    .resource_mut::<SpatialGrid>()
                    .set_static(cell, value.navigation());
                changed += 1;
            }
        }
        if changed > 0 {
            let mut terrain = world.resource_mut::<TerrainState>();
            terrain.revision = terrain.revision.saturating_add(1);
            record_effect(
                world,
                None,
                "IslandTerraformed",
                &json!({"x":self.x,"z":self.z,"radius":self.radius,"height_mm":self.height_mm,"surface":self.surface,"cells":changed}),
            );
        }
        Ok(())
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
