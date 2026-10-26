//! Fog x agent-io (features `perception` + `agent-io`): the "searched
//! ground" summary of the observation (proposal 004, spec F4), built from
//! the explored cells of Task 9 ([`ExploredCells`]) and shown by the
//! `perception` section ([`perception_section`](super::perception_agent)).
//!
//! The record is coarsened to 10 m cells ([`COARSE_MM`], aligned to the
//! world origin); a coarse cell counts as explored when at least half of its
//! fine cells inside the grid are. Then, for the coarse cells whose centre
//! lies within [`RADIUS_MM`] of the actor (and inside the grid):
//!
//! - `within_radius_pct`: the share of them explored (whole percent, down);
//! - `sectors`: the same count per compass sector (45 degrees each, `+z`
//!   north, `+x` east), in the fixed order N, NE, E, SE, S, SW, W, NW; a cell
//!   belongs to the sector of its centre's offset from the actor;
//! - `map_pct`: the explored share of the whole grid's fine cells (the
//!   whole map, not a distance from where the actor started: no start
//!   position is stored).
//!
//! Text: `Searched: 34% within 60 m, 12% of the map; least searched:
//! north-east, east.` (the [`LEAST_LISTED`] sectors with the lowest share,
//! ties in the fixed order; sectors fully searched, outside the grid or
//! made of fewer than [`MIN_SECTOR_CELLS`] coarse cells (edge slivers) are
//! left out, and so is the clause when none is left).
//!
//! Read-only and never hashed: it reads the snapshot-only record, writes
//! nothing, and is bounded: at most about `(2 * radius / edge + 3)^2` coarse
//! cells, each tested on at most [`MAX_SAMPLES_PER_AXIS`]^2 = 400 fine cells
//! (a coarse cell of more fine cells is sampled with a stride), so an
//! observation never costs more than ~90 000 bit tests with 10 m cells. The
//! JSON `cell_mm` is the real coarse edge (10 m, or the grid's cell edge when
//! that is larger).

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_ecs::prelude::*;
use npc_kernel::SimPos;
use npc_kernel::spatial::{CellCoord, SpatialGrid};
use npc_perception::ExploredCells;
use serde_json::{Value, json};

/// Edge of a coarse cell (mm).
pub const COARSE_MM: i32 = 10_000;
/// How far around the actor the summary looks (mm).
pub const RADIUS_MM: i32 = 60_000;
/// Fine cells tested per axis of one coarse cell (a larger cell is sampled
/// with a stride, so at most 20 x 20 tests).
pub const MAX_SAMPLES_PER_AXIS: i64 = 20;
/// A sector of fewer coarse cells than this (an edge sliver) is never named
/// as least searched.
pub const MIN_SECTOR_CELLS: u64 = 2;
/// How many least searched sectors the text names.
pub const LEAST_LISTED: usize = 2;

/// The compass sectors in their fixed order: JSON `dir` and text name.
pub const SECTORS: [(&str, &str); 8] = [
    ("N", "north"),
    ("NE", "north-east"),
    ("E", "east"),
    ("SE", "south-east"),
    ("S", "south"),
    ("SW", "south-west"),
    ("W", "west"),
    ("NW", "north-west"),
];

/// The searched-ground summary of one actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Searched {
    /// The real coarse cell edge (mm).
    pub cell_mm: i64,
    pub within_radius_pct: u64,
    pub map_pct: u64,
    /// `(explored, total)` coarse cells per sector, in [`SECTORS`] order.
    pub sectors: [(u64, u64); 8],
}

/// The index in [`SECTORS`] of an offset (`+z` north, `+x` east): the
/// tangent of 22.5 degrees taken as 12/29, in integers. No offset counts as
/// north. The glue's generic compass (the robot sensors' text uses it too).
pub fn sector_of(dx: i64, dz: i64) -> usize {
    let (a, b) = (dx.abs(), dz.abs());
    if a * 29 <= b * 12 {
        if dz >= 0 { 0 } else { 4 }
    } else if b * 29 <= a * 12 {
        if dx > 0 { 2 } else { 6 }
    } else {
        match (dx > 0, dz > 0) {
            (true, true) => 1,
            (true, false) => 3,
            (false, false) => 5,
            (false, true) => 7,
        }
    }
}

/// The summary for the actor `entity` at `here`, or `None` without an
/// explored record that fits the grid (or without any cell in range).
pub fn searched_summary(world: &World, entity: Entity, here: SimPos) -> Option<Searched> {
    let rec = world.get::<ExploredCells>(entity)?;
    let grid = world.get_resource::<SpatialGrid>()?;
    let bounds = grid.bounds();
    if !rec.fits(bounds) || bounds.is_empty() || grid.cell_mm() <= 0 {
        return None;
    }
    let fine_mm = i64::from(grid.cell_mm());
    // Fine cells per coarse edge (at least one).
    let per = (i64::from(COARSE_MM) / fine_mm).max(1);
    let coarse_mm = per * fine_mm;
    // Stride through the fine cells of a coarse cell: at most
    // `MAX_SAMPLES_PER_AXIS` per axis.
    let step =
        usize::try_from((per + MAX_SAMPLES_PER_AXIS - 1) / MAX_SAMPLES_PER_AXIS).unwrap_or(1);
    let r = i64::from(RADIUS_MM);
    let (px, pz) = (i64::from(here.x), i64::from(here.z));
    let (min_x, min_z) = (i64::from(bounds.min.x), i64::from(bounds.min.z));
    let (max_x, max_z) = (
        min_x + i64::from(bounds.width),
        min_z + i64::from(bounds.height),
    );
    // Coarse cells whose centre can be within the radius.
    let lo = |p: i64| (p - r).div_euclid(coarse_mm) - 1;
    let hi = |p: i64| (p + r).div_euclid(coarse_mm) + 1;
    let mut sectors = [(0u64, 0u64); 8];
    let (mut explored, mut total) = (0u64, 0u64);
    for cz in lo(pz)..=hi(pz) {
        for cx in lo(px)..=hi(px) {
            let dx = cx * coarse_mm + coarse_mm / 2 - px;
            let dz = cz * coarse_mm + coarse_mm / 2 - pz;
            if dx * dx + dz * dz > r * r {
                continue;
            }
            // The fine cells of this coarse cell inside the grid.
            let (mut fine, mut seen) = (0u64, 0u64);
            for fz in ((cz * per).max(min_z)..(cz * per + per).min(max_z)).step_by(step) {
                for fx in ((cx * per).max(min_x)..(cx * per + per).min(max_x)).step_by(step) {
                    let cell = CellCoord::new(i32::try_from(fx).ok()?, i32::try_from(fz).ok()?);
                    if let Some(i) = bounds.index(cell) {
                        fine += 1;
                        seen += u64::from(rec.get(i));
                    }
                }
            }
            if fine == 0 {
                continue;
            }
            let done = u64::from(2 * seen >= fine);
            let s = &mut sectors[sector_of(dx, dz)];
            s.0 += done;
            s.1 += 1;
            explored += done;
            total += 1;
        }
    }
    if total == 0 {
        return None;
    }
    let cells = u64::try_from(bounds.len()).unwrap_or(u64::MAX).max(1);
    Some(Searched {
        cell_mm: coarse_mm,
        within_radius_pct: explored * 100 / total,
        map_pct: u64::from(rec.count()) * 100 / cells,
        sectors,
    })
}

impl Searched {
    /// The JSON of the `searched` key of the perception section.
    pub fn json(&self) -> Value {
        let sectors: Vec<Value> = SECTORS
            .iter()
            .zip(self.sectors)
            .map(|((dir, _), (explored, total))| {
                json!({"dir": dir, "explored": explored, "total": total})
            })
            .collect();
        json!({
            "cell_mm": self.cell_mm, "radius_mm": RADIUS_MM,
            "within_radius_pct": self.within_radius_pct,
            "map_pct": self.map_pct,
            "sectors": sectors,
        })
    }

    /// The text line (see the module docs).
    pub fn text(&self) -> String {
        let mut order: Vec<usize> = (0..SECTORS.len())
            .filter(|&i| {
                self.sectors[i].1 >= MIN_SECTOR_CELLS && self.sectors[i].0 < self.sectors[i].1
            })
            .collect();
        // Lowest explored share first (cross-multiplied), then fixed order.
        order.sort_by(|&a, &b| {
            let ((ea, ta), (eb, tb)) = (self.sectors[a], self.sectors[b]);
            (ea * tb).cmp(&(eb * ta)).then(a.cmp(&b))
        });
        let names: Vec<&str> = order
            .iter()
            .take(LEAST_LISTED)
            .map(|&i| SECTORS[i].1)
            .collect();
        let mut line = format!(
            "Searched: {}% within {} m, {}% of the map",
            self.within_radius_pct,
            RADIUS_MM / 1000,
            self.map_pct
        );
        if !names.is_empty() {
            line.push_str("; least searched: ");
            line.push_str(&names.join(", "));
        }
        line.push('.');
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_kernel::spatial::GridBounds;

    fn searched(sectors: [(u64, u64); 8]) -> Searched {
        Searched {
            cell_mm: 10_000,
            within_radius_pct: 10,
            map_pct: 5,
            sectors,
        }
    }

    #[test]
    fn edge_slivers_are_never_least_searched() {
        // North is one unexplored border cell; east and west are real.
        let mut s = [(2, 4); 8];
        s[0] = (0, 1);
        s[2] = (1, 4);
        s[6] = (2, 5);
        assert_eq!(
            searched(s).text(),
            "Searched: 10% within 60 m, 5% of the map; least searched: east, west."
        );
        // Only slivers left: no clause.
        let mut s = [(1, 1); 8];
        s[0] = (0, 1);
        assert_eq!(
            searched(s).text(),
            "Searched: 10% within 60 m, 5% of the map."
        );
    }

    #[test]
    fn small_fine_cells_are_sampled_and_cell_mm_is_real() {
        let mut world = World::new();
        // 100 mm cells: a 10 m coarse cell holds 100 x 100 fine cells.
        let grid = SpatialGrid::new(100, GridBounds::centered(400, 400));
        let mut rec = ExploredCells::sized(grid.bounds());
        rec.bits.iter_mut().for_each(|w| *w = u64::MAX);
        world.insert_resource(grid);
        let e = world.spawn(rec).id();
        let s = searched_summary(&world, e, SimPos::new(0, 0)).unwrap();
        assert_eq!(s.cell_mm, 10_000);
        assert_eq!((s.within_radius_pct, s.map_pct), (100, 100));
        // A 20 m grid cell is its own coarse cell.
        let mut world = World::new();
        let grid = SpatialGrid::new(20_000, GridBounds::centered(8, 8));
        let rec = ExploredCells::sized(grid.bounds());
        world.insert_resource(grid);
        let e = world.spawn(rec).id();
        let s = searched_summary(&world, e, SimPos::new(0, 0)).unwrap();
        assert_eq!((s.cell_mm, s.within_radius_pct, s.map_pct), (20_000, 0, 0));
    }
}
