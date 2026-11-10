//! Integer heightfield, navigation projection and atomic regional terrain edits.
use super::{Biome, LivingWorldSpec};
pub use crate::village::terrain::TerrainSurface;
use crate::village::{
    ResourcePatch, VillagePlace, Villager, colony::Household, homestead::HomesteadPlot,
};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct TerrainCell {
    pub height_mm: i32,
    pub surface: TerrainSurface,
    pub biome: Biome,
}
impl TerrainCell {
    pub fn walkable(&self) -> bool {
        self.surface != TerrainSurface::Water
    }
    pub fn navigation(self) -> CellInfo {
        CellInfo {
            flags: CellFlags {
                blocking: !self.walkable(),
                occluding: false,
                tags: 0,
            },
            extra_cost: match self.surface {
                TerrainSurface::Soil => 0,
                TerrainSurface::Grass => 2,
                TerrainSurface::Stone => 4,
                TerrainSurface::Water => 0,
            },
        }
    }
}
#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct TerrainState {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<TerrainCell>,
    pub revision: u64,
}
impl TerrainState {
    pub fn bounds(&self) -> GridBounds {
        GridBounds::centered(self.width, self.height)
    }
    pub fn cell(&self, x: i32, z: i32) -> Option<&TerrainCell> {
        self.bounds()
            .index(CellCoord::new(x, z))
            .and_then(|i| self.cells.get(i as usize))
    }
    pub fn iter(&self) -> impl Iterator<Item = (CellCoord, &TerrainCell)> {
        let bounds = self.bounds();
        self.cells
            .iter()
            .enumerate()
            .map(move |(i, c)| (bounds.coord(i as u32), c))
    }
}
impl SimHashResource for TerrainState {
    const NAME: &'static str = "living_world_terrain_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.width);
        h.u32(self.height);
        h.u64(self.revision);
        h.u32(self.cells.len() as u32);
        for c in &self.cells {
            h.i32(c.height_mm);
            h.u8(c.surface as u8);
            h.u8(c.biome as u8);
        }
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}
pub fn register(app: &mut App) {
    app.register_sim_resource::<TerrainState>();
}
pub fn bounds(world: &World) -> GridBounds {
    world
        .get_resource::<TerrainState>()
        .map(TerrainState::bounds)
        .unwrap_or_else(|| world.resource::<SpatialGrid>().bounds())
}
pub fn cell(world: &World, x: i32, z: i32) -> Option<TerrainCell> {
    world.get_resource::<TerrainState>()?.cell(x, z).copied()
}
pub fn height_mm(world: &World, at: SimPos) -> i32 {
    cell(world, at.x.div_euclid(1000), at.z.div_euclid(1000)).map_or(0, |c| c.height_mm)
}
pub fn biome_at(world: &World, at: SimPos) -> Biome {
    cell(world, at.x.div_euclid(1000), at.z.div_euclid(1000)).map_or(Biome::Coast, |c| c.biome)
}

fn near_segment(p: SimPos, a: SimPos, b: SimPos, radius: i32) -> bool {
    let dx = i64::from(b.x) - i64::from(a.x);
    let dz = i64::from(b.z) - i64::from(a.z);
    let px = i64::from(p.x) - i64::from(a.x);
    let pz = i64::from(p.z) - i64::from(a.z);
    let length = dx * dx + dz * dz;
    if length == 0 {
        return p.dist2(a) <= i64::from(radius).pow(2);
    }
    let projection = (px * dx + pz * dz).clamp(0, length);
    let nearest = SimPos::new(
        a.x + (dx * projection / length) as i32,
        a.z + (dz * projection / length) as i32,
    );
    p.dist2(nearest) <= i64::from(radius).pow(2)
}
fn road(spec: &LivingWorldSpec, p: SimPos) -> bool {
    for settlement in &spec.settlements {
        if near_segment(p, spec.actor_start, settlement.center, 1700) {
            return true;
        }
        let center = settlement.center;
        for entrance in settlement
            .sites
            .iter()
            .map(|s| s.position)
            .chain(settlement.homes.iter().map(|h| h.door))
        {
            let bend = SimPos::new(center.x, entrance.z);
            if near_segment(p, center, bend, 1200) || near_segment(p, bend, entrance, 1000) {
                return true;
            }
        }
    }
    false
}
fn generated_cell(spec: &LivingWorldSpec, p: SimPos, seed: u64, index: u32) -> TerrainCell {
    let rx = i128::from(spec.width_cells) * 500 - 7000;
    let rz = i128::from(spec.height_cells) * 500 - 9000;
    let x = i128::from(p.x);
    let z = i128::from(p.z);
    let on_road = road(spec, p);
    let occupied = spec
        .settlements
        .iter()
        .any(|s| p.dist2(s.center) < 34000_i64.pow(2));
    let ellipse = x * x * rz * rz + z * z * rx * rx;
    let land = ellipse < rx * rx * rz * rz || occupied || on_road;
    if !land {
        return TerrainCell {
            height_mm: -700,
            surface: TerrainSurface::Water,
            biome: Biome::Coast,
        };
    }
    let nearest = spec.settlements.iter().min_by_key(|s| p.dist2(s.center));
    let biome = if let Some(s) = nearest.filter(|s| p.dist2(s.center) < 45000_i64.pow(2)) {
        s.biome
    } else if p.z > 52000 && p.x.abs() < 26000 {
        Biome::Wetland
    } else if p.x < -22000 {
        Biome::Woodland
    } else if p.x > 38000 && p.z > 0 {
        Biome::Ridge
    } else if ellipse > rx * rx * rz * rz * 8 / 10 {
        Biome::Coast
    } else {
        Biome::Meadow
    };
    // Broad integer tent functions avoid per-cell random stairs. A seeded phase
    // changes hill placement while every local slope remains walkable.
    let phase =
        (npc_framework::kernel::rng::sim_rand(seed, SimId(0), 0, 0x4c574801) % 12001) as i32 - 6000;
    let ridge = (8500 - (p.x - 75000 - phase).abs() / 13 - (p.z - 48000).abs() / 18).max(0);
    let hill = (2000 - (p.x + 48000).abs() / 30 - (p.z - 48000 - phase).abs() / 28).max(0);
    let mut height = 150 + ridge + hill;
    if let Some(s) = nearest {
        let distance = (p.dist2(s.center) as u64).isqrt() as i32;
        let plateau = match s.biome {
            Biome::Ridge => 4700,
            Biome::Woodland => 1100,
            _ => 200,
        };
        let weight = (46000 - distance).clamp(0, 10000);
        height = (height * (10000 - weight) + plateau * weight) / 10000;
    }
    let pond = spec
        .settlements
        .iter()
        .any(|s| p.dist2(SimPos::new(s.center.x + 42000, s.center.z - 9000)) < 3500_i64.pow(2));
    let wet = pond
        || (biome == Biome::Wetland
            && !on_road
            && !occupied
            && ((p.x / 12000 + p.z / 14000).rem_euclid(3) == 0));
    let surface = if wet {
        TerrainSurface::Water
    } else if on_road {
        TerrainSurface::Soil
    } else if biome == Biome::Ridge && (index % 19 < 4 || height > 6500) {
        TerrainSurface::Stone
    } else {
        TerrainSurface::Grass
    };
    TerrainCell {
        height_mm: if wet { -200 } else { height },
        surface,
        biome,
    }
}
pub fn spawn(world: &mut World) {
    if world.contains_resource::<TerrainState>() {
        return;
    }
    let spec = super::spec(world).expect("regional construction").clone();
    let seed = world.resource::<WorldSeed>().0;
    let bounds = spec.bounds();
    let mut cells = Vec::with_capacity(bounds.len());
    for index in 0..bounds.len() as u32 {
        let cell = bounds.coord(index);
        let p = SimPos::new(cell.x * 1000 + 500, cell.z * 1000 + 500);
        let value = generated_cell(&spec, p, seed, index);
        world
            .resource_mut::<SpatialGrid>()
            .set_static(cell, value.navigation());
        cells.push(value);
    }
    world.insert_resource(TerrainState {
        width: spec.width_cells,
        height: spec.height_cells,
        cells,
        revision: 0,
    });
}

fn brush(
    world: &World,
    x: i32,
    z: i32,
    radius: u32,
    height_mm: i32,
    surface: TerrainSurface,
) -> Result<Vec<CellCoord>, String> {
    let bounds = bounds(world);
    if radius > 8 || !bounds.contains(CellCoord::new(x, z)) {
        return Err("Brush must be within the regional map; radius is 0..8 metres".into());
    }
    if !(-2000..=12000).contains(&height_mm)
        || (surface == TerrainSurface::Water && height_mm > 0)
        || (surface != TerrainSurface::Water && height_mm < 0)
    {
        return Err("Dry terrain must be 0..12000 mm; water -2000..0 mm".into());
    }
    let mut cells = Vec::new();
    let r = radius as i32;
    for cz in z - r..=z + r {
        for cx in x - r..=x + r {
            let at = CellCoord::new(cx, cz);
            if bounds.contains(at) && (cx - x).pow(2) + (cz - z).pow(2) <= r * r {
                cells.push(at);
            }
        }
    }
    Ok(cells)
}
pub fn terraform_check(
    world: &World,
    x: i32,
    z: i32,
    radius: u32,
    height_mm: i32,
    surface: TerrainSurface,
) -> Result<(), String> {
    let terrain = world
        .get_resource::<TerrainState>()
        .ok_or("Regional terrain is missing")?;
    let changed: Vec<_> = brush(world, x, z, radius, height_mm, surface)?
        .into_iter()
        .filter(|c| {
            terrain
                .cell(c.x, c.z)
                .is_some_and(|v| v.height_mm != height_mm || v.surface != surface)
        })
        .collect();
    if changed.is_empty() {
        return Ok(());
    }
    let grid = world.resource::<SpatialGrid>();
    let mut blockers = BTreeSet::new();
    for e in world.resource::<SimIndex>().map.values() {
        let Some(pos) = world.get::<SimPos>(*e).copied() else {
            continue;
        };
        if let Some(blocker) = world.get::<GridBlocker>(*e) {
            blockers.extend(grid.footprint_cells(pos, blocker.0));
        }
        let fixed = world.get::<VillagePlace>(*e).is_some()
            || world.get::<ResourcePatch>(*e).is_some()
            || world.get::<Household>(*e).is_some()
            || world.get::<HomesteadPlot>(*e).is_some();
        let occupied = world.get::<Villager>(*e).is_some()
            || world.get::<crate::village::hunting::Wildlife>(*e).is_some()
            || world
                .get::<crate::village::colony::AwaitingArrival>(*e)
                .is_some();
        if (fixed || occupied)
            && changed.iter().any(|c| {
                grid.center(*c).dist2(pos)
                    <= if fixed {
                        5000_i64.pow(2)
                    } else {
                        1200_i64.pow(2)
                    }
            })
        {
            return Err(
                "Brush overlaps a resident, home, workplace, resource or reserved plot".into(),
            );
        }
    }
    if changed.iter().any(|c| blockers.contains(c)) {
        return Err("Brush overlaps a solid building".into());
    }
    let mut candidate = terrain.clone();
    for c in &changed {
        let value = &mut candidate.cells[terrain.bounds().index(*c).expect("brush") as usize];
        value.height_mm = height_mm;
        value.surface = surface;
    }
    // Heightfield traversal has one ground surface. Prevent introducing a
    // discontinuous dry step that would make visual traversal misleading.
    for c in &changed {
        for (dx, dz) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            if let (Some(a), Some(b)) =
                (candidate.cell(c.x, c.z), candidate.cell(c.x + dx, c.z + dz))
                && a.walkable()
                && b.walkable()
                && a.height_mm.abs_diff(b.height_mm) > 1500
            {
                return Err(
                    "Brush would create a dry cliff; shape the slope in smaller height steps"
                        .into(),
                );
            }
        }
    }
    let origin = grid.cell_of(super::spec(world).expect("spec").actor_start);
    let passable = |c: CellCoord| {
        candidate.cell(c.x, c.z).is_some_and(TerrainCell::walkable) && !blockers.contains(&c)
    };
    if !passable(origin) {
        return Err("The arrival route must remain open".into());
    }
    let mut reached = BTreeSet::from([origin]);
    let mut queue = VecDeque::from([origin]);
    while let Some(c) = queue.pop_front() {
        for (dx, dz) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            let next = CellCoord::new(c.x + dx, c.z + dz);
            if passable(next) && reached.insert(next) {
                queue.push_back(next);
            }
        }
    }
    for (c, _) in candidate.iter() {
        if passable(c) && !reached.contains(&c) {
            return Err("Terrain would disconnect accessible land from the settlements".into());
        }
    }
    Ok(())
}
pub fn terraform(
    world: &mut World,
    x: i32,
    z: i32,
    radius: u32,
    height_mm: i32,
    surface: TerrainSurface,
) -> Result<(), String> {
    terraform_check(world, x, z, radius, height_mm, surface)?;
    let cells = brush(world, x, z, radius, height_mm, surface)?;
    let mut changed = 0;
    for c in cells {
        let index = world
            .resource::<TerrainState>()
            .bounds()
            .index(c)
            .expect("brush") as usize;
        let mut value = world.resource::<TerrainState>().cells[index];
        if value.height_mm == height_mm && value.surface == surface {
            continue;
        }
        value.height_mm = height_mm;
        value.surface = surface;
        world.resource_mut::<TerrainState>().cells[index] = value;
        world
            .resource_mut::<SpatialGrid>()
            .set_static(c, value.navigation());
        changed += 1;
    }
    if changed > 0 {
        let mut state = world.resource_mut::<TerrainState>();
        state.revision = state.revision.saturating_add(1);
        record_effect(
            world,
            None,
            "RegionalTerrainChanged",
            &json!({"x":x,"z":z,"radius":radius,"height_mm":height_mm,"cells":changed}),
        );
    }
    Ok(())
}
pub fn validate(world: &World) -> Result<(), String> {
    let terrain = world
        .get_resource::<TerrainState>()
        .ok_or("Regional terrain is missing")?;
    let spec = super::spec(world).ok_or("Regional construction is missing")?;
    if terrain.width != spec.width_cells
        || terrain.height != spec.height_cells
        || terrain.cells.len() != terrain.bounds().len()
    {
        return Err("Regional terrain dimensions do not match frozen construction".into());
    }
    let grid = world.resource::<SpatialGrid>();
    if grid.bounds() != terrain.bounds() {
        return Err("Regional navigation dimensions differ from terrain".into());
    }
    for (c, v) in terrain.iter() {
        if !(-2000..=12000).contains(&v.height_mm)
            || (v.walkable() && v.height_mm < 0)
            || (!v.walkable() && v.height_mm > 0)
            || grid.static_cell(c) != v.navigation()
        {
            return Err("Regional terrain and navigation are inconsistent".into());
        }
    }
    Ok(())
}
