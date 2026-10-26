//! The terrain grid (spec §7, ADR-0014): one terrain kind and one height per map cell,
//! `Hashed` state that `debug.set_terrain` (and later craters and bridges) changes in a match.
//!
//! [`TerrainGrid`] names terrain kinds through a palette of terrain ids (strings, never dense
//! content ids, R52/R54), so content renumbering never moves a hash or breaks a save. The
//! navigation data built from it ([`NavGrid`](crate::nav::NavGrid)) is `Derived`.

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use fl_content::{MapDef, MapLayers, TerrainDef};
use fl_math::Fx;
use serde::{Deserialize, Serialize};

use crate::build::Content;
use crate::hash::{SimHash, StateHasher};
use crate::map::MapInfo;

/// The map's cells: a terrain kind and a height each. Hashed `"core.terrain"` v1, with the
/// palette hashed and saved as terrain names.
///
/// Cell `(x, y)` (y north) is at index `y × cells_x + x` of [`types`](Self::types) and
/// [`heights`](Self::heights). Empty `types` mean every cell is [`OPEN`](Self::OPEN); empty
/// `heights` mean every cell is at `height_m.0`.
#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainGrid {
    pub cells_x: u32,
    pub cells_y: u32,
    /// Terrain ids; [`types`](Self::types) indexes it. New names are appended, so an index
    /// never moves.
    pub palette: Vec<String>,
    /// One palette index per cell, or [`OPEN`](Self::OPEN).
    pub types: Vec<u8>,
    /// One raw height sample per cell, 0..=65535 over [`height_m`](Self::height_m).
    pub heights: Vec<u16>,
    /// The height in metres at sample 0 and at sample 65535.
    pub height_m: (Fx, Fx),
}

/// A rectangle of cells, both corners inclusive: `min.0..=max.0` by `min.1..=max.1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellRect {
    pub min: (u32, u32),
    pub max: (u32, u32),
}

impl CellRect {
    /// The number of cells in the rect (0 if `min > max` on an axis).
    pub fn area(&self) -> u64 {
        let side = |lo: u32, hi: u32| if hi < lo { 0 } else { u64::from(hi - lo) + 1 };
        side(self.min.0, self.max.0) * side(self.min.1, self.max.1)
    }
}

impl TerrainGrid {
    /// No terrain kind: open ground, pass 1, blocks nothing.
    pub const OPEN: u8 = u8::MAX;

    /// A grid of `cells_x` × `cells_y` open cells at height 0.
    pub fn open(cells_x: u32, cells_y: u32) -> Self {
        TerrainGrid {
            cells_x,
            cells_y,
            palette: Vec::new(),
            types: Vec::new(),
            heights: Vec::new(),
            height_m: (Fx::ZERO, Fx::ZERO),
        }
    }

    /// The grid of a map's decoded layers, whose samples span `height_m`.
    pub fn from_layers(layers: &MapLayers, height_m: (Fx, Fx)) -> Self {
        TerrainGrid {
            cells_x: layers.cells_x,
            cells_y: layers.cells_y,
            palette: layers.palette.clone(),
            types: layers.types.clone(),
            heights: layers.heights.clone(),
            height_m,
        }
    }

    /// The index of cell `(x, y)`, if it is on the grid.
    fn index(&self, x: u32, y: u32) -> Option<usize> {
        (x < self.cells_x && y < self.cells_y)
            .then(|| y as usize * self.cells_x as usize + x as usize)
    }

    /// The palette index of cell `(x, y)`: [`OPEN`](Self::OPEN) for open ground and for a
    /// cell off the grid.
    pub fn type_at(&self, x: u32, y: u32) -> u8 {
        self.index(x, y)
            .and_then(|i| self.types.get(i).copied())
            .unwrap_or(Self::OPEN)
    }

    /// The terrain id of cell `(x, y)`; `None` for open ground or a cell off the grid.
    pub fn terrain_at(&self, x: u32, y: u32) -> Option<&str> {
        self.palette
            .get(usize::from(self.type_at(x, y)))
            .map(String::as_str)
    }

    /// The height of cell `(x, y)` in metres: `lo + (hi − lo) × v / 65535`, on raw integers
    /// (truncated toward zero). A cell off the grid, or a grid without heights, is at `lo`.
    pub fn height_at(&self, x: u32, y: u32) -> Fx {
        let (lo, hi) = self.height_m;
        let v = self
            .index(x, y)
            .and_then(|i| self.heights.get(i).copied())
            .unwrap_or(0);
        let span = i128::from((hi - lo).raw());
        let lift = span * i128::from(v) / 65535;
        lo + Fx::from_raw(i64::try_from(lift).expect("a lift lies between lo and hi"))
    }

    /// Make the cells of `rect` terrain `terrain`. A name not yet in the palette is appended
    /// (existing indices never move).
    ///
    /// # Panics
    /// If `rect` is not inside the grid or `min > max`, or the palette would exceed 255 names.
    pub fn set_rect(&mut self, rect: CellRect, terrain: &str) {
        assert!(
            rect.min.0 <= rect.max.0
                && rect.min.1 <= rect.max.1
                && rect.max.0 < self.cells_x
                && rect.max.1 < self.cells_y,
            "{rect:?} is not a rect inside the {}x{} grid",
            self.cells_x,
            self.cells_y
        );
        let index = match self.palette.iter().position(|name| name == terrain) {
            Some(at) => at,
            None => {
                self.palette.push(terrain.to_owned());
                self.palette.len() - 1
            }
        };
        let index = u8::try_from(index)
            .ok()
            .filter(|&i| i != Self::OPEN)
            .expect("a terrain palette holds at most 255 names");
        if self.types.is_empty() {
            self.types = vec![Self::OPEN; self.cells_x as usize * self.cells_y as usize];
        }
        for y in rect.min.1..=rect.max.1 {
            let row = y as usize * self.cells_x as usize;
            self.types[row + rect.min.0 as usize..=row + rect.max.0 as usize].fill(index);
        }
    }
}

/// How many height samples [`TerrainGrid`]'s hash writes at once.
const HASH_BLOCK: usize = 512;

/// The palette as names (length, then each name's bytes), then the cells.
impl SimHash for TerrainGrid {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.cells_x.sim_hash(h);
        self.cells_y.sim_hash(h);
        h.write_u64(self.palette.len() as u64);
        for name in &self.palette {
            h.write_u64(name.len() as u64);
            h.write_bytes(name.as_bytes());
        }
        h.write_u64(self.types.len() as u64);
        h.write_bytes(&self.types);
        h.write_u64(self.heights.len() as u64);
        // Little-endian samples, written in blocks: the streaming hash of the blocks is the
        // hash of the whole byte string, so the block size never changes the value.
        let mut block = [0u8; 2 * HASH_BLOCK];
        for chunk in self.heights.chunks(HASH_BLOCK) {
            for (bytes, height) in block.as_chunks_mut::<2>().0.iter_mut().zip(chunk) {
                *bytes = height.to_le_bytes();
            }
            h.write_bytes(&block[..2 * chunk.len()]);
        }
        self.height_m.0.sim_hash(h);
        self.height_m.1.sim_hash(h);
    }
}

/// Check a loaded [`TerrainGrid`] (saved or filled) against the restored [`MapInfo`] and the
/// content, so a malformed save is a load error rather than a panic or silent open ground:
/// the grid has the map's cells; `types` and `heights` are empty or one per cell; the
/// palette holds at most 255 names, each a terrain kind of the content; every type indexes
/// the palette or is [`OPEN`](TerrainGrid::OPEN). No terrain is fine.
///
/// # Errors
/// The first problem found.
pub(crate) fn check_loaded(world: &World) -> Result<(), String> {
    let Some(terrain) = world.get_resource::<TerrainGrid>() else {
        return Ok(());
    };
    let map = world
        .get_resource::<MapInfo>()
        .ok_or("the save has terrain but no map (core.map)")?;
    let (x, y) = (terrain.cells_x, terrain.cells_y);
    if (x, y) != (map.cells_x, map.cells_y) {
        return Err(format!(
            "{x}x{y} cells, but the map has {}x{} cells",
            map.cells_x, map.cells_y
        ));
    }
    let n = x as usize * y as usize;
    for (field, len) in [
        ("types", terrain.types.len()),
        ("heights", terrain.heights.len()),
    ] {
        if len != 0 && len != n {
            return Err(format!("{field} holds {len} cells, not 0 or {n}"));
        }
    }
    let max = usize::from(TerrainGrid::OPEN);
    if terrain.palette.len() > max {
        return Err(format!(
            "the palette holds {} names; at most {max} fit",
            terrain.palette.len()
        ));
    }
    let kinds = world.resource::<Content>().0.kind::<TerrainDef>();
    if let Some(name) = terrain
        .palette
        .iter()
        .find(|name| kinds.get(name).is_none())
    {
        return Err(format!("unknown terrain '{name}'"));
    }
    let names = terrain.palette.len();
    if let Some((i, t)) = terrain
        .types
        .iter()
        .enumerate()
        .find(|&(_, &t)| t != TerrainGrid::OPEN && usize::from(t) >= names)
    {
        return Err(format!(
            "cell {i} indexes palette entry {t}, beyond its {names} names"
        ));
    }
    Ok(())
}

/// The terrain of `map` for the [`MapInfo`] `info`: its layers if it has them, else open
/// ground of `info`'s cells.
pub(crate) fn terrain_of(map: Option<&MapDef>, info: &MapInfo) -> TerrainGrid {
    match map.and_then(|m| Some((m.layers.as_ref()?, m.terrain.as_ref()?))) {
        Some((layers, terrain)) => TerrainGrid::from_layers(layers, terrain.height_m),
        None => TerrainGrid::open(info.cells_x, info.cells_y),
    }
}

/// Insert the [`TerrainGrid`] of `map` (open ground for `None` or a map without layers) for
/// the [`MapInfo`] in `world`, then rebuild the navigation data; without a map, remove both.
/// Setup calls this next to [`init_spatial`](crate::spatial::init_spatial).
pub(crate) fn init_terrain(world: &mut World, map: Option<&MapDef>) {
    match world.get_resource::<MapInfo>() {
        Some(info) => {
            let terrain = terrain_of(map, info);
            world.insert_resource(terrain);
        }
        None => {
            world.remove_resource::<TerrainGrid>();
        }
    }
    crate::nav::rebuild_nav(world);
}

/// The `fill_missing` of `"core.terrain"` (R78): a save of core v1 (M0) has no terrain. It
/// gets the terrain of the content's map whose `size_m` and `cell_m` equal the restored
/// [`MapInfo`] (saves carry no map id): that map's layers if exactly one such map has them,
/// open ground if none does. Without a [`MapInfo`] there is no terrain, as in a live match
/// without a map.
///
/// # Errors
/// `"the save's map matches several maps: <ids>"` if two or more such maps have layers.
pub(crate) fn fill_terrain(world: &mut World) -> Result<(), String> {
    let Some(info) = world.get_resource::<MapInfo>().cloned() else {
        return Ok(());
    };
    let content = world.resource::<Content>().0.clone();
    let with_layers: Vec<&MapDef> = content
        .kind::<MapDef>()
        .iter()
        .filter(|map| map.size_m == info.size && map.cell_m == info.cell && map.layers.is_some())
        .collect();
    let terrain = match with_layers.as_slice() {
        [] => TerrainGrid::open(info.cells_x, info.cells_y),
        [map] => terrain_of(Some(map), &info),
        several => {
            let ids: Vec<&str> = several.iter().map(|map| map.id.as_str()).collect();
            return Err(format!(
                "the save's map matches several maps: {}",
                ids.join(", ")
            ));
        }
    };
    world.insert_resource(terrain);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_rect_appends_names_and_keeps_indices() {
        let mut grid = TerrainGrid::open(4, 3);
        assert_eq!(grid.terrain_at(1, 1), None);
        grid.set_rect(
            CellRect {
                min: (1, 1),
                max: (2, 2),
            },
            "t.water",
        );
        assert_eq!(grid.types.len(), 12);
        assert_eq!(grid.terrain_at(1, 1), Some("t.water"));
        assert_eq!(grid.terrain_at(2, 2), Some("t.water"));
        assert_eq!(grid.terrain_at(0, 1), None);
        grid.set_rect(
            CellRect {
                min: (0, 0),
                max: (0, 0),
            },
            "t.road",
        );
        grid.set_rect(
            CellRect {
                min: (3, 2),
                max: (3, 2),
            },
            "t.water",
        );
        assert_eq!(grid.palette, ["t.water", "t.road"]);
        assert_eq!(grid.type_at(0, 0), 1);
        assert_eq!(grid.type_at(3, 2), 0);
        assert_eq!(grid.terrain_at(9, 9), None);
    }

    #[test]
    fn heights_span_the_range() {
        let layers = MapLayers {
            cells_x: 3,
            cells_y: 1,
            palette: vec!["t.grass".to_owned()],
            types: vec![0; 3],
            heights: vec![0, 65535, 32768],
        };
        let grid = TerrainGrid::from_layers(&layers, (Fx::from_int(-4), Fx::from_int(16)));
        assert_eq!(grid.height_at(0, 0), Fx::from_int(-4));
        assert_eq!(grid.height_at(1, 0), Fx::from_int(16));
        assert!(grid.height_at(2, 0) > Fx::from_int(6));
        assert_eq!(grid.height_at(5, 0), Fx::from_int(-4));
        assert_eq!(TerrainGrid::open(2, 2).height_at(1, 1), Fx::ZERO);
    }
}
