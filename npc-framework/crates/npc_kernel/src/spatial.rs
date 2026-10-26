//! Integer spatial grid (actor-capabilities spec §4.1, plan Task C1).
//!
//! [`SpatialGrid`] is one resource with three parts:
//! - **Geometry:** square cells of [`SpatialGrid::cell_mm`] (default 1000 mm)
//!   and [`GridBounds`] (default 128 × 128 cells centred on the origin).
//!   [`SpatialGrid::cell_of`] floors, so every position has exactly one cell;
//!   [`SpatialGrid::center`] is that cell's centre.
//! - **Cell data** inside the bounds: [`CellInfo`] = [`CellFlags`]
//!   (`blocking`, `occluding`, content `tags`) plus an `extra_cost`. The
//!   effective value is the static layer ([`SpatialGrid::set_static`]) merged
//!   with the dynamic layer from [`GridBlocker`] / [`GridOccluder`] entities
//!   (their [`Footprint`] around their `SimPos`). Outside the bounds every
//!   cell is open ground with no cost.
//! - **Index:** every entity with a `SimId` and a `SimPos`, by cell. Queries
//!   ([`SpatialGrid::in_radius`], [`SpatialGrid::in_box`],
//!   [`SpatialGrid::ids_in_cell`]) return `SimId`s sorted ascending.
//!
//! [`spatial_update_system`] keeps the index and the dynamic layer current in
//! `NpcSet::Inputs`, after commands (and host positions): queries during a
//! tick see the positions at the end of the previous tick. Every tick in
//! which effective cell data changed it writes one [`GridChanged`] with the
//! sorted changed cells. With no blockers it touches no cell data at all.
//!
//! The grid is **not hashed by default**: the index is derived from `SimPos`
//! and, without blockers or static cells, the grid holds no state of its own.
//! So adding [`SpatialPlugin`] (part of `NpcPlugins`) changes neither the
//! state hash nor the content hash. Capabilities that put state in it
//! (navigation, interaction) call [`register_spatial_hash`], which adds the
//! static layer (resource `spatial_grid`) and the `grid_blocker` /
//! `grid_occluder` components to the state hash.

use std::collections::BTreeMap;

use bevy_app::prelude::*;
use bevy_ecs::message::{Message, Messages};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::clock::SimClock;
use crate::driver::apply_host_positions_system;
use crate::hash::{Fnv64, SimHash, SimHashRegistry, SimHashResource};
use crate::ids::SimId;
use crate::math::{SimPos, copy_prev_pos_system};
use crate::registry::KernelAppExt;
use crate::schedule::NpcSet;
use crate::snapshot::SnapshotAppExt;

/// Default cell edge in mm (1 m).
pub const DEFAULT_CELL_MM: i32 = 1000;
/// Default bounds: this many cells per axis, centred on the origin.
pub const DEFAULT_GRID_CELLS: u32 = 128;

/// A cell, in cell units (`floor(mm / cell_mm)` per axis). Orders by `x`,
/// then `z`.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Reflect,
)]
pub struct CellCoord {
    pub x: i32,
    pub z: i32,
}

impl CellCoord {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

/// The rectangle of cells that carries cell data: `min` and `width × height`
/// cells from it. Cells inside have a dense index `(z - min.z) * width + (x - min.x)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GridBounds {
    pub min: CellCoord,
    pub width: u32,
    pub height: u32,
}

impl Default for GridBounds {
    fn default() -> Self {
        Self::centered(DEFAULT_GRID_CELLS, DEFAULT_GRID_CELLS)
    }
}

impl GridBounds {
    /// `width × height` cells with the origin's cell at the centre (cells
    /// `-width/2 .. width - width/2`).
    pub fn centered(width: u32, height: u32) -> Self {
        let half = |n: u32| -i32::try_from(n / 2).expect("grid too large");
        Self {
            min: CellCoord::new(half(width), half(height)),
            width,
            height,
        }
    }

    /// Number of cells.
    pub fn len(&self) -> usize {
        self.width as usize * self.height as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The dense index of `cell`, or `None` outside the bounds.
    pub fn index(&self, cell: CellCoord) -> Option<u32> {
        let dx = i64::from(cell.x) - i64::from(self.min.x);
        let dz = i64::from(cell.z) - i64::from(self.min.z);
        if dx < 0 || dz < 0 || dx >= i64::from(self.width) || dz >= i64::from(self.height) {
            return None;
        }
        u32::try_from(dz * i64::from(self.width) + dx).ok()
    }

    /// The cell at dense index `index` (`index < len()`).
    pub fn coord(&self, index: u32) -> CellCoord {
        let w = self.width.max(1);
        CellCoord::new(
            self.min.x + (index % w) as i32,
            self.min.z + (index / w) as i32,
        )
    }

    pub fn contains(&self, cell: CellCoord) -> bool {
        self.index(cell).is_some()
    }
}

/// Per-cell flags. `tags` is a bit set for content (e.g. "door"); nav
/// consumers can register pass-cost functions per tag bit.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Reflect,
)]
pub struct CellFlags {
    pub blocking: bool,
    pub occluding: bool,
    pub tags: u8,
}

/// Flags plus the extra cost of entering the cell (path-cost units, on top
/// of the 10/14 step cost).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Reflect,
)]
pub struct CellInfo {
    pub flags: CellFlags,
    pub extra_cost: u16,
}

/// Half extents in mm of the box a blocker or occluder covers around its
/// `SimPos` (inclusive). `Footprint::default()` covers the position's cell.
/// Negative extents count as 0 and extents beyond the grid's own size are
/// clamped to it (a footprint never covers more than the grid can hold).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct Footprint {
    pub half_x_mm: i32,
    pub half_z_mm: i32,
}

/// Marks the cells of its footprint (around the entity's `SimPos`) blocking.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct GridBlocker(pub Footprint);

/// Marks the cells of its footprint (around the entity's `SimPos`) occluding.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct GridOccluder(pub Footprint);

/// Cells whose effective [`CellInfo`] changed during `tick` (sorted, no
/// duplicates). Written once per tick at most, by [`spatial_update_system`].
///
/// The buffer is advanced by that system (not by `First`), so a reader in
/// any later `NpcSet` of the same tick, running every tick, sees each
/// message exactly once.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct GridChanged {
    pub tick: u64,
    pub cells: Vec<CellCoord>,
}

/// Why a path request failed (journal `PathFailed`, `npc_nav`).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
pub enum PathFailReason {
    /// The agent stands in a cell it may not occupy.
    StartBlocked,
    /// The goal lies in a cell it may not enter.
    GoalBlocked,
    /// Start or goal lies outside the grid bounds.
    OutOfBounds,
    /// No path exists (the search ran out of cells).
    Unreachable,
    /// The request used up its share of search work (restarted by grid
    /// changes too often) and gave up so the requests behind it get served;
    /// retried like any failure.
    SearchLimit,
}

/// What stopped a walk (`npc_nav::explain_blocked`; journal `OrderFailed
/// { blocker }`): a blocking cell, a door entity, or the world's edge. Only
/// in events and observations, never in hashed state. JSON
/// `{"kind": "Wall", "cell": [x, z], "pos": [x, z]}`, `{"kind": "Door",
/// "id": 12, "locked": true}`, `{"kind": "OutOfBounds"}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Blocker {
    /// A blocking cell (`cell` in grid coordinates, `pos` its centre, mm).
    Wall { cell: [i32; 2], pos: [i32; 2] },
    /// A closed (or locked) door the walker could not get through.
    Door { id: SimId, locked: bool },
    /// The goal lies outside the grid.
    OutOfBounds,
}

/// One indexed entity.
#[derive(Clone, Copy, Debug)]
struct Member {
    entity: Entity,
    pos: SimPos,
    cell: CellCoord,
}

/// The spatial grid. See the module docs.
#[derive(Resource, Clone, Debug)]
pub struct SpatialGrid {
    cell_mm: i32,
    bounds: GridBounds,
    /// Dense static layer.
    statics: Vec<CellInfo>,
    /// Dense index → (blockers, occluders) covering it.
    dynamic: BTreeMap<u32, (u16, u16)>,
    /// Cells whose effective `blocking` is true.
    blocking_cells: u32,
    /// Static changes not yet reported in a `GridChanged`.
    pending: Vec<u32>,
    cells: BTreeMap<CellCoord, Vec<SimId>>,
    members: BTreeMap<SimId, Member>,
    by_entity: BTreeMap<Entity, SimId>,
    /// Set by a snapshot load: the next [`spatial_update_system`] re-indexes
    /// every entity and recomputes the dynamic layer (change detection does
    /// not carry over a load; ADR 0013).
    resync: bool,
    /// Bumped whenever effective cell data changes ([`SpatialGrid::revision`]).
    /// Not state: neither hashed nor saved.
    revision: u64,
}

impl Default for SpatialGrid {
    fn default() -> Self {
        Self::new(DEFAULT_CELL_MM, GridBounds::default())
    }
}

impl SpatialGrid {
    /// A grid of `cell_mm` cells (must be > 0) with cell data inside `bounds`.
    pub fn new(cell_mm: i32, bounds: GridBounds) -> Self {
        assert!(cell_mm > 0, "cell_mm must be > 0");
        Self {
            cell_mm,
            bounds,
            statics: vec![CellInfo::default(); bounds.len()],
            dynamic: BTreeMap::new(),
            blocking_cells: 0,
            pending: Vec::new(),
            cells: BTreeMap::new(),
            members: BTreeMap::new(),
            by_entity: BTreeMap::new(),
            resync: false,
            revision: 0,
        }
    }

    /// A counter that changes whenever any cell's effective data (static
    /// layer or blockers/occluders) changes, at once (unlike [`GridChanged`],
    /// written by the next [`spatial_update_system`]). Equal revisions of the
    /// same grid mean equal cell data: a cache of something derived from the
    /// cells (fog of war's revealed cells) stays valid while it is unchanged.
    /// Not sim state: neither hashed nor saved (a loaded grid starts over).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Cell edge in mm.
    pub fn cell_mm(&self) -> i32 {
        self.cell_mm
    }

    pub fn bounds(&self) -> GridBounds {
        self.bounds
    }

    /// The cell containing `pos` (floor division per axis).
    pub fn cell_of(&self, pos: SimPos) -> CellCoord {
        CellCoord::new(
            pos.x.div_euclid(self.cell_mm),
            pos.z.div_euclid(self.cell_mm),
        )
    }

    /// The centre of `cell` in mm (saturating at the `i32` range).
    pub fn center(&self, cell: CellCoord) -> SimPos {
        let c = i64::from(self.cell_mm);
        let at = |v: i32| {
            (i64::from(v) * c + c / 2).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
        };
        SimPos::new(at(cell.x), at(cell.z))
    }

    // -- cell data ----------------------------------------------------------

    fn effective(&self, index: u32) -> CellInfo {
        let mut info = self.statics[index as usize];
        if let Some(&(blockers, occluders)) = self.dynamic.get(&index) {
            info.flags.blocking |= blockers > 0;
            info.flags.occluding |= occluders > 0;
        }
        info
    }

    /// Effective cell data (static ∪ blockers); default outside the bounds.
    pub fn cell(&self, cell: CellCoord) -> CellInfo {
        self.bounds
            .index(cell)
            .map_or_else(CellInfo::default, |i| self.effective(i))
    }

    /// Effective cell data by dense index (`index < bounds().len()`).
    pub fn cell_at(&self, index: u32) -> CellInfo {
        self.effective(index)
    }

    pub fn is_blocking(&self, cell: CellCoord) -> bool {
        self.cell(cell).flags.blocking
    }

    pub fn is_occluding(&self, cell: CellCoord) -> bool {
        self.cell(cell).flags.occluding
    }

    /// Whether any cell is blocking (movers skip all checks when not).
    pub fn has_blocking(&self) -> bool {
        self.blocking_cells > 0
    }

    /// The static layer of `cell` (default outside the bounds).
    pub fn static_cell(&self, cell: CellCoord) -> CellInfo {
        self.bounds
            .index(cell)
            .map_or_else(CellInfo::default, |i| self.statics[i as usize])
    }

    /// Sets the static layer of `cell`; reported in the next tick's
    /// [`GridChanged`]. Returns false (and does nothing) outside the bounds.
    pub fn set_static(&mut self, cell: CellCoord, info: CellInfo) -> bool {
        let Some(i) = self.bounds.index(cell) else {
            return false;
        };
        if self.statics[i as usize] == info {
            return true;
        }
        let before = self.effective(i).flags.blocking;
        self.statics[i as usize] = info;
        self.count_blocking(before, self.effective(i).flags.blocking);
        self.pending.push(i);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    fn count_blocking(&mut self, before: bool, after: bool) {
        match (before, after) {
            (false, true) => self.blocking_cells += 1,
            (true, false) => self.blocking_cells -= 1,
            _ => {}
        }
    }

    /// Every non-default static cell as `(dense index, info)`, ascending.
    pub fn static_cells(&self) -> impl Iterator<Item = (u32, CellInfo)> + '_ {
        self.statics
            .iter()
            .enumerate()
            .filter(|(_, c)| **c != CellInfo::default())
            .map(|(i, c)| (i as u32, *c))
    }

    /// Calls `f` for every in-bounds occluding cell (effective: static or
    /// [`GridOccluder`]) of the rectangle `lo..=hi` (inclusive), row by row
    /// (ascending `z`, then `x`); a cell occluding in both layers may be
    /// reported twice. Reads the static layer by row and the occluder layer
    /// by range: much cheaper than [`SpatialGrid::is_occluding`] per cell
    /// for a whole area (fog of war reads its scan area once).
    pub fn for_each_occluding(&self, lo: CellCoord, hi: CellCoord, f: impl FnMut(CellCoord)) {
        self.for_each_flagged(lo, hi, false, f);
    }

    /// Like [`SpatialGrid::for_each_occluding`], for every in-bounds cell
    /// that is blocking **or** occluding (effective: static, [`GridBlocker`]
    /// or [`GridOccluder`]): what a range sensor's beam stops at.
    pub fn for_each_solid(&self, lo: CellCoord, hi: CellCoord, f: impl FnMut(CellCoord)) {
        self.for_each_flagged(lo, hi, true, f);
    }

    /// The occluding cells of `lo..=hi`, plus the blocking ones when
    /// `blocking` (see [`SpatialGrid::for_each_occluding`]).
    fn for_each_flagged(
        &self,
        lo: CellCoord,
        hi: CellCoord,
        blocking: bool,
        mut f: impl FnMut(CellCoord),
    ) {
        let b = self.bounds;
        if b.is_empty() {
            return;
        }
        let last =
            |min: i32, n: u32| i32::try_from(i64::from(min) + i64::from(n) - 1).unwrap_or(i32::MAX);
        let (x0, x1) = (lo.x.max(b.min.x), hi.x.min(last(b.min.x, b.width)));
        let (z0, z1) = (lo.z.max(b.min.z), hi.z.min(last(b.min.z, b.height)));
        if x0 > x1 || z0 > z1 {
            return;
        }
        for z in z0..=z1 {
            let (Some(start), Some(end)) = (
                b.index(CellCoord::new(x0, z)),
                b.index(CellCoord::new(x1, z)),
            ) else {
                continue;
            };
            for (i, info) in self.statics[start as usize..=end as usize]
                .iter()
                .enumerate()
            {
                if info.flags.occluding || (blocking && info.flags.blocking) {
                    f(CellCoord::new(x0 + i as i32, z));
                }
            }
            for (&i, &(blockers, occluders)) in self.dynamic.range(start..=end) {
                if occluders > 0 || (blocking && blockers > 0) {
                    f(b.coord(i));
                }
            }
        }
    }

    /// The cell rectangle `(lo, hi)` (inclusive) `footprint` covers around
    /// `pos`. Half extents are clamped to `0..=` the grid's own size per
    /// axis, so a huge footprint never spans more than about twice the
    /// grid.
    fn footprint_rect(&self, pos: SimPos, footprint: Footprint) -> (CellCoord, CellCoord) {
        let limit = |cells: u32| {
            i32::try_from(i64::from(cells).saturating_mul(i64::from(self.cell_mm)))
                .unwrap_or(i32::MAX)
        };
        let hx = footprint.half_x_mm.clamp(0, limit(self.bounds.width));
        let hz = footprint.half_z_mm.clamp(0, limit(self.bounds.height));
        let lo = self.cell_of(SimPos::new(
            pos.x.saturating_sub(hx),
            pos.z.saturating_sub(hz),
        ));
        let hi = self.cell_of(SimPos::new(
            pos.x.saturating_add(hx),
            pos.z.saturating_add(hz),
        ));
        (lo, hi)
    }

    /// Cells covered by `footprint` around `pos` (x-major, ascending). Half
    /// extents are clamped to the grid's size (see [`Footprint`]).
    pub fn footprint_cells(&self, pos: SimPos, footprint: Footprint) -> Vec<CellCoord> {
        let (lo, hi) = self.footprint_rect(pos, footprint);
        let mut out = Vec::new();
        for x in lo.x..=hi.x {
            for z in lo.z..=hi.z {
                out.push(CellCoord::new(x, z));
            }
        }
        out
    }

    /// Dense indices of the in-bounds cells `footprint` covers around `pos`
    /// (ascending): the cells it can block or occlude.
    pub fn footprint_indices(&self, pos: SimPos, footprint: Footprint) -> Vec<u32> {
        let (lo, hi) = self.footprint_rect(pos, footprint);
        let b = self.bounds;
        let to_i32 = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
        let (bx1, bz1) = (
            b.min.x.saturating_add(to_i32(b.width)) - 1,
            b.min.z.saturating_add(to_i32(b.height)) - 1,
        );
        let (x0, x1) = (lo.x.max(b.min.x), hi.x.min(bx1));
        let (z0, z1) = (lo.z.max(b.min.z), hi.z.min(bz1));
        let mut out = Vec::new();
        for x in x0..=x1 {
            for z in z0..=z1 {
                if let Some(i) = b.index(CellCoord::new(x, z)) {
                    out.push(i);
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// The cell rectangle `(lo, hi)` (inclusive) that `footprint` covers
    /// around `pos` (the cells [`SpatialGrid::footprint_cells`] lists).
    pub fn footprint_cell_rect(&self, pos: SimPos, footprint: Footprint) -> (CellCoord, CellCoord) {
        self.footprint_rect(pos, footprint)
    }

    /// The area (mm) of the cells `footprint` covers around `pos`, as the
    /// rectangle `(min, max)`: `min` the low corner of the lowest cell,
    /// `max` the high edge of the highest (saturating). What a blocker
    /// really blocks, for edge distances (`npc_command::Reach::Area`,
    /// [`nearest_blocker_by_edge`]).
    pub fn footprint_area(&self, pos: SimPos, footprint: Footprint) -> (SimPos, SimPos) {
        let (lo, hi) = self.footprint_rect(pos, footprint);
        let c = i64::from(self.cell_mm);
        let mm = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        (
            SimPos::new(mm(i64::from(lo.x) * c), mm(i64::from(lo.z) * c)),
            SimPos::new(mm((i64::from(hi.x) + 1) * c), mm((i64::from(hi.z) + 1) * c)),
        )
    }

    /// The cells exactly `ring` cells (Chebyshev) outside the rectangle
    /// `footprint` covers around `pos` (`ring` 1: the cells touching it,
    /// diagonals included; 0: the footprint's own cells), x-major
    /// ascending. Out-of-bounds cells are included (filter with
    /// `bounds().contains`).
    pub fn cells_around(&self, pos: SimPos, footprint: Footprint, ring: u32) -> Vec<CellCoord> {
        let (lo, hi) = self.footprint_rect(pos, footprint);
        let r = i32::try_from(ring).unwrap_or(i32::MAX);
        let (x0, x1) = (lo.x.saturating_sub(r), hi.x.saturating_add(r));
        let (z0, z1) = (lo.z.saturating_sub(r), hi.z.saturating_add(r));
        let mut out = Vec::new();
        for x in x0..=x1 {
            if ring == 0 || x == x0 || x == x1 {
                out.extend((z0..=z1).map(|z| CellCoord::new(x, z)));
            } else {
                out.push(CellCoord::new(x, z0));
                if z1 != z0 {
                    out.push(CellCoord::new(x, z1));
                }
            }
        }
        out
    }

    /// The centre of the in-bounds, non-blocking cell around `footprint`
    /// (at `pos`) nearest to `from`: ring 1 first, then outwards up to
    /// `max_ring`; ties by cell (lowest `x`, then `z`). Where a unit
    /// stands to work on a wall or building. `None` when every cell up to
    /// `max_ring` is blocked or out of bounds.
    pub fn nearest_free_cell_around(
        &self,
        from: SimPos,
        pos: SimPos,
        footprint: Footprint,
        max_ring: u32,
    ) -> Option<SimPos> {
        for ring in 1..=max_ring.max(1) {
            let best = self
                .cells_around(pos, footprint, ring)
                .into_iter()
                .filter(|&c| self.bounds.contains(c) && !self.is_blocking(c))
                .min_by_key(|&c| (self.center(c).dist2(from), c));
            if let Some(c) = best {
                return Some(self.center(c));
            }
        }
        None
    }

    /// How many [`GridBlocker`] footprints cover dense cell `index`.
    pub fn blockers_at(&self, index: u32) -> u16 {
        self.dynamic.get(&index).map_or(0, |&(b, _)| b)
    }

    /// The effective cell data of dense cell `index` as if one of the
    /// blockers covering it were not there: what a blocker sees of its own
    /// cells (`npc_nav` lets an agent with a `GridBlocker` path through its
    /// own footprint).
    pub fn cell_without_one_blocker(&self, index: u32) -> CellInfo {
        let mut info = self.statics[index as usize];
        if let Some(&(blockers, occluders)) = self.dynamic.get(&index) {
            info.flags.blocking |= blockers > 1;
            info.flags.occluding |= occluders > 0;
        }
        info
    }

    /// Visits every cell the segment `a → b` touches, from `a`'s cell to
    /// `b`'s cell (both included), in order. Where the segment passes
    /// exactly through a cell corner both side cells are visited before the
    /// diagonal one (a conservative supercover: line of sight never slips
    /// between two blocked cells that touch diagonally). Integer-only.
    /// Returns false as soon as `visit` does.
    pub fn trace_segment(
        &self,
        a: SimPos,
        b: SimPos,
        mut visit: impl FnMut(CellCoord) -> bool,
    ) -> bool {
        let cs = i64::from(self.cell_mm);
        let start = self.cell_of(a);
        let end = self.cell_of(b);
        let (mut cx, mut cz) = (start.x, start.z);
        if !visit(start) {
            return false;
        }
        let dx = i64::from(b.x) - i64::from(a.x);
        let dz = i64::from(b.z) - i64::from(a.z);
        let (sx, sz) = (dx.signum() as i32, dz.signum() as i32);
        let (adx, adz) = (dx.abs(), dz.abs());
        // Distance along each axis from `a` to the next boundary crossing.
        let first = |c: i32, p: i32, s: i32| -> i64 {
            match s {
                1 => (i64::from(c) + 1) * cs - i64::from(p),
                -1 => i64::from(p) - i64::from(c) * cs,
                _ => 0,
            }
        };
        let mut nx = first(cx, a.x, sx);
        let mut nz = first(cz, a.z, sz);
        while (cx, cz) != (end.x, end.z) {
            let step_x = if cx == end.x {
                false
            } else if cz == end.z {
                true
            } else {
                // Compare nx/adx with nz/adz without division.
                let (lhs, rhs) = (nx as i128 * adz as i128, nz as i128 * adx as i128);
                if lhs == rhs {
                    // Exactly through a corner.
                    if !visit(CellCoord::new(cx + sx, cz)) || !visit(CellCoord::new(cx, cz + sz)) {
                        return false;
                    }
                    cx += sx;
                    cz += sz;
                    nx += cs;
                    nz += cs;
                    if !visit(CellCoord::new(cx, cz)) {
                        return false;
                    }
                    continue;
                }
                lhs < rhs
            };
            if step_x {
                cx += sx;
                nx += cs;
            } else {
                cz += sz;
                nz += cs;
            }
            if !visit(CellCoord::new(cx, cz)) {
                return false;
            }
        }
        true
    }

    // -- index --------------------------------------------------------------

    /// Number of indexed entities.
    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// `SimId`s indexed in `cell`, ascending.
    pub fn ids_in_cell(&self, cell: CellCoord) -> &[SimId] {
        self.cells.get(&cell).map_or(&[], Vec::as_slice)
    }

    /// The indexed position of `id` (as of this tick's `NpcSet::Inputs`).
    pub fn position_of(&self, id: SimId) -> Option<SimPos> {
        self.members.get(&id).map(|m| m.pos)
    }

    /// The entity indexed for `id`.
    pub fn entity_of(&self, id: SimId) -> Option<Entity> {
        self.members.get(&id).map(|m| m.entity)
    }

    /// Indexed entities within `r_mm` of `pos` (distance ≤ r, inclusive),
    /// sorted by `SimId`.
    pub fn in_radius(&self, pos: SimPos, r_mm: i32) -> Vec<SimId> {
        let r = r_mm.max(0);
        let r2 = i64::from(r) * i64::from(r);
        let lo = SimPos::new(pos.x.saturating_sub(r), pos.z.saturating_sub(r));
        let hi = SimPos::new(pos.x.saturating_add(r), pos.z.saturating_add(r));
        self.collect(lo, hi, |p| pos.dist2(p) <= r2)
    }

    /// Indexed entities with `min.x ≤ x ≤ max.x` and `min.z ≤ z ≤ max.z`,
    /// sorted by `SimId`.
    pub fn in_box(&self, min: SimPos, max: SimPos) -> Vec<SimId> {
        self.collect(min, max, |p| {
            p.x >= min.x && p.x <= max.x && p.z >= min.z && p.z <= max.z
        })
    }

    fn collect(&self, lo: SimPos, hi: SimPos, keep: impl Fn(SimPos) -> bool) -> Vec<SimId> {
        let (a, b) = (self.cell_of(lo), self.cell_of(hi));
        let span = (i64::from(b.x) - i64::from(a.x) + 1) * (i64::from(b.z) - i64::from(a.z) + 1);
        let mut out: Vec<SimId> = if span > self.members.len() as i64 {
            // Cheaper to scan every member (already in SimId order).
            self.members
                .iter()
                .filter(|(_, m)| keep(m.pos))
                .map(|(&id, _)| id)
                .collect()
        } else {
            let mut out = Vec::new();
            for x in a.x..=b.x {
                for (_, ids) in self
                    .cells
                    .range(CellCoord::new(x, a.z)..=CellCoord::new(x, b.z))
                {
                    out.extend(ids.iter().copied().filter(|id| keep(self.members[id].pos)));
                }
            }
            out
        };
        out.sort_unstable();
        out
    }

    /// Replaces the index with `rows` (every sim entity with a position).
    fn reindex_all(&mut self, rows: impl IntoIterator<Item = (Entity, SimId, SimPos)>) {
        self.cells.clear();
        self.members.clear();
        self.by_entity.clear();
        for (entity, id, pos) in rows {
            self.index_at(entity, id, pos);
        }
    }

    fn unindex(&mut self, entity: Entity) {
        let Some(id) = self.by_entity.remove(&entity) else {
            return;
        };
        if let Some(m) = self.members.remove(&id) {
            self.remove_from_cell(m.cell, id);
        }
    }

    fn remove_from_cell(&mut self, cell: CellCoord, id: SimId) {
        if let Some(ids) = self.cells.get_mut(&cell) {
            if let Ok(i) = ids.binary_search(&id) {
                ids.remove(i);
            }
            if ids.is_empty() {
                self.cells.remove(&cell);
            }
        }
    }

    fn index_at(&mut self, entity: Entity, id: SimId, pos: SimPos) {
        let cell = self.cell_of(pos);
        if let Some(old_id) = self.by_entity.insert(entity, id)
            && old_id != id
            && let Some(m) = self.members.remove(&old_id)
        {
            self.remove_from_cell(m.cell, old_id);
        }
        if let Some(old) = self.members.insert(id, Member { entity, pos, cell }) {
            if old.cell == cell {
                return;
            }
            self.remove_from_cell(old.cell, id);
        }
        let ids = self.cells.entry(cell).or_default();
        if let Err(i) = ids.binary_search(&id) {
            ids.insert(i, id);
        }
    }

    /// Replaces the dynamic layer; returns the dense indices whose effective
    /// data changed (ascending).
    fn set_dynamic(&mut self, dynamic: BTreeMap<u32, (u16, u16)>) -> Vec<u32> {
        let mut keys: Vec<u32> = self.dynamic.keys().chain(dynamic.keys()).copied().collect();
        keys.sort_unstable();
        keys.dedup();
        let before: Vec<CellInfo> = keys.iter().map(|&i| self.effective(i)).collect();
        self.dynamic = dynamic;
        let mut changed = Vec::new();
        for (&i, old) in keys.iter().zip(before) {
            let new = self.effective(i);
            if new != old {
                self.count_blocking(old.flags.blocking, new.flags.blocking);
                changed.push(i);
            }
        }
        if !changed.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }
}

/// A blocker found by [`nearest_blocker_by_edge`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockerHit {
    pub id: SimId,
    pub entity: Entity,
    /// Squared distance (mm²) from the query point to the blocker's area
    /// ([`SpatialGrid::footprint_area`]); 0 inside it.
    pub dist2: i64,
    /// The blocker's area `(min, max)` in mm.
    pub area: (SimPos, SimPos),
}

/// The [`GridBlocker`] entity nearest to `from` by **edge** distance (to
/// the cells its footprint covers, not its centre) within `max_mm`, among
/// those `filter` accepts; ties by `SimId`. Footprint-aware where
/// [`SpatialGrid::in_radius`] only sees positions: a long wall whose
/// centre is far away can still be right next to the unit. Scans every
/// blocker (they are few); deterministic.
pub fn nearest_blocker_by_edge(
    world: &World,
    from: SimPos,
    max_mm: i32,
    filter: impl Fn(&World, Entity) -> bool,
) -> Option<BlockerHit> {
    let grid = world.get_resource::<SpatialGrid>()?;
    let mut q = world.try_query::<(Entity, &SimId, &SimPos, &GridBlocker)>()?;
    let r = i64::from(max_mm.max(0));
    q.iter(world)
        .filter(|(e, _, _, _)| filter(world, *e))
        .filter_map(|(entity, &id, &pos, b)| {
            let area = grid.footprint_area(pos, b.0);
            let dist2 = area_dist2(from, area.0, area.1);
            (dist2 <= r * r).then_some(BlockerHit {
                id,
                entity,
                dist2,
                area,
            })
        })
        .min_by_key(|h| (h.dist2, h.id))
}

/// Squared distance (mm²) from `p` to the rectangle `min..=max` (0 inside).
pub fn area_dist2(p: SimPos, min: SimPos, max: SimPos) -> i64 {
    let axis = |v: i32, lo: i32, hi: i32| -> i64 {
        let (v, lo, hi) = (i64::from(v), i64::from(lo), i64::from(hi));
        (lo - v).max(v - hi).max(0)
    };
    let dx = axis(p.x, min.x.min(max.x), min.x.max(max.x));
    let dz = axis(p.z, min.z.min(max.z), min.z.max(max.z));
    dx * dx + dz * dz
}

/// The saved form of a [`SpatialGrid`] (ADR 0013): its geometry, the static
/// layer, the dynamic layer as of the last [`spatial_update_system`] and the
/// static changes not yet reported. The index is derived (rebuilt on load).
#[derive(Serialize, Deserialize)]
struct SavedGrid {
    cell_mm: i32,
    bounds: GridBounds,
    /// Non-default static cells `(dense index, info)`, ascending.
    statics: Vec<(u32, CellInfo)>,
    /// `(dense index, blockers, occluders)`, ascending.
    dynamic: Vec<(u32, u16, u16)>,
    pending: Vec<u32>,
}

fn save_grid(world: &World) -> Option<Value> {
    let grid = world.get_resource::<SpatialGrid>()?;
    let saved = SavedGrid {
        cell_mm: grid.cell_mm,
        bounds: grid.bounds,
        statics: grid.static_cells().collect(),
        dynamic: grid.dynamic.iter().map(|(&i, &(b, o))| (i, b, o)).collect(),
        pending: grid.pending.clone(),
    };
    Some(serde_json::to_value(saved).expect("the spatial grid serializes to JSON"))
}

/// Replaces the grid's state; the index is rebuilt by [`rebuild_grid`] and
/// the next update re-syncs (`resync`). Drops `GridChanged` messages left
/// from the build (not this world's).
fn load_grid(world: &mut World, value: Value) -> Result<(), String> {
    let saved: SavedGrid = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if saved.cell_mm <= 0 {
        return Err(format!(
            "spatial_grid: cell_mm {} must be > 0",
            saved.cell_mm
        ));
    }
    let mut grid = SpatialGrid::new(saved.cell_mm, saved.bounds);
    // A new revision, so a cache keyed by the replaced grid's never matches.
    grid.revision = world
        .get_resource::<SpatialGrid>()
        .map_or(0, |g| g.revision.wrapping_add(1));
    let len = grid.statics.len();
    let check = |i: u32| {
        if (i as usize) < len {
            Ok(())
        } else {
            Err(format!("spatial_grid: cell index {i} outside {len} cells"))
        }
    };
    for (i, info) in saved.statics {
        check(i)?;
        grid.statics[i as usize] = info;
    }
    for (i, b, o) in saved.dynamic {
        check(i)?;
        grid.dynamic.insert(i, (b, o));
    }
    for &i in &saved.pending {
        check(i)?;
    }
    grid.pending = saved.pending;
    let mut blocking = 0;
    for i in 0..grid.bounds.len() {
        let i = u32::try_from(i).expect("too many cells");
        if grid.effective(i).flags.blocking {
            blocking += 1;
        }
    }
    grid.blocking_cells = blocking;
    grid.resync = true;
    world.insert_resource(grid);
    if let Some(mut messages) = world.get_resource_mut::<Messages<GridChanged>>() {
        messages.clear();
    }
    Ok(())
}

/// Snapshot rebuild: indexes every sim entity with a position (so queries
/// between the load and the next tick see them) and marks the grid for a
/// re-sync on the next update.
fn rebuild_grid(world: &mut World) {
    let rows: Vec<(Entity, SimId, SimPos)> = world
        .query::<(Entity, &SimId, &SimPos)>()
        .iter(world)
        .map(|(e, &id, &pos)| (e, id, pos))
        .collect();
    if let Some(mut grid) = world.get_resource_mut::<SpatialGrid>() {
        grid.reindex_all(rows);
        grid.resync = true;
    }
}

impl SimHashResource for SpatialGrid {
    const NAME: &'static str = "spatial_grid";
    /// Geometry and the static layer (the index and the dynamic layer derive
    /// from hashed `SimPos` / blocker components).
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.cell_mm);
        h.i32(self.bounds.min.x);
        h.i32(self.bounds.min.z);
        h.u32(self.bounds.width);
        h.u32(self.bounds.height);
        let cells: Vec<(u32, CellInfo)> = self.static_cells().collect();
        h.u32(u32::try_from(cells.len()).expect("too many cells"));
        for (i, c) in cells {
            h.u32(i);
            hash_cell(h, c);
        }
    }
}

fn hash_cell(h: &mut Fnv64, c: CellInfo) {
    h.u8(u8::from(c.flags.blocking) | (u8::from(c.flags.occluding) << 1));
    h.u8(c.flags.tags);
    h.u16(c.extra_cost);
}

fn hash_footprint(h: &mut Fnv64, f: Footprint) {
    h.i32(f.half_x_mm);
    h.i32(f.half_z_mm);
}

impl SimHash for GridBlocker {
    const NAME: &'static str = "grid_blocker";
    fn sim_hash(&self, h: &mut Fnv64) {
        hash_footprint(h, self.0);
    }
}

impl SimHash for GridOccluder {
    const NAME: &'static str = "grid_occluder";
    fn sim_hash(&self, h: &mut Fnv64) {
        hash_footprint(h, self.0);
    }
}

/// Adds the grid's static layer and the blocker/occluder components to the
/// state hash (idempotent). Changes the content hash: call it only from a
/// capability that puts state in the grid, never from the kernel defaults.
pub fn register_spatial_hash(app: &mut App) {
    let registry = app.world_mut().get_resource_or_init::<SimHashRegistry>();
    let has_res = registry
        .resources
        .iter()
        .any(|r| r.name == SpatialGrid::NAME);
    let has_blocker = registry
        .components
        .iter()
        .any(|c| c.name == GridBlocker::NAME);
    let has_occluder = registry
        .components
        .iter()
        .any(|c| c.name == GridOccluder::NAME);
    if !has_res {
        app.register_sim_resource::<SpatialGrid>();
    }
    if !has_blocker {
        app.register_sim_component::<GridBlocker>();
    }
    if !has_occluder {
        app.register_sim_component::<GridOccluder>();
    }
}

/// Installs [`SpatialGrid`] (kept if the host inserted one first), the
/// [`GridChanged`] buffer and [`spatial_update_system`] in `NpcSet::Inputs`
/// after commands, host positions and `PrevSimPos`. Hashes nothing (see the
/// module docs). Saves the grid in world snapshots as `spatial_grid` (static
/// and dynamic layers, unreported changes; the index is rebuilt) and the
/// [`GridBlocker`] / [`GridOccluder`] components as `grid_blocker` /
/// `grid_occluder`, hashed or not. Part of `NpcPlugins`.
pub struct SpatialPlugin;

impl Plugin for SpatialPlugin {
    fn build(&self, app: &mut App) {
        // The footprints are read by `spatial_update_system` on the next
        // tick, so they are saved whether or not a capability hashes them
        // (`register_spatial_hash` registers them again under the same
        // names: a no-op in either order, ADR 0013 Task 2).
        app.register_snapshot_resource_with("spatial_grid", save_grid, load_grid)
            .add_snapshot_rebuild("spatial_grid", rebuild_grid)
            .register_snapshot_component_named::<GridBlocker>(GridBlocker::NAME)
            .register_snapshot_component_named::<GridOccluder>(GridOccluder::NAME);
        app.init_resource::<SpatialGrid>()
            .init_resource::<Messages<GridChanged>>()
            .register_type::<GridBlocker>()
            .register_type::<GridOccluder>()
            .add_systems(
                crate::schedule::NpcSimTick,
                spatial_update_system
                    .in_set(NpcSet::Inputs)
                    .after(apply_host_positions_system)
                    .after(copy_prev_pos_system),
            );
    }
}

type Moved = Or<(Changed<SimPos>, Added<SimId>)>;
type HasFootprint = Or<(With<GridBlocker>, With<GridOccluder>)>;
type BlockerRow<'a> = (
    &'a SimPos,
    Option<&'a GridBlocker>,
    Option<&'a GridOccluder>,
);
type BlockerChanged = (
    Or<(With<GridBlocker>, With<GridOccluder>)>,
    Or<(Changed<SimPos>, Changed<GridBlocker>, Changed<GridOccluder>)>,
);

/// `NpcSet::Inputs`: updates the index from moved, spawned and removed
/// entities; recomputes the dynamic layer when a blocker/occluder appeared,
/// moved or left; writes [`GridChanged`] when effective cell data changed.
/// After a snapshot load it re-indexes every entity and recomputes the
/// dynamic layer once (against the saved one, so the changed cells are those
/// an uninterrupted run reports).
#[allow(clippy::too_many_arguments)]
pub fn spatial_update_system(
    mut grid: ResMut<SpatialGrid>,
    mut changed: ResMut<Messages<GridChanged>>,
    clock: Option<Res<SimClock>>,
    moved: Query<(Entity, &SimId, &SimPos), Moved>,
    mut removed_pos: RemovedComponents<SimPos>,
    mut removed_id: RemovedComponents<SimId>,
    mut removed_blocker: RemovedComponents<GridBlocker>,
    mut removed_occluder: RemovedComponents<GridOccluder>,
    blockers_changed: Query<(), BlockerChanged>,
    blockers: Query<BlockerRow, HasFootprint>,
    positioned: Query<(Entity, &SimId, &SimPos)>,
) {
    changed.update();
    let resync = std::mem::take(&mut grid.resync);
    if resync {
        grid.reindex_all(positioned.iter().map(|(e, &id, &pos)| (e, id, pos)));
    }
    let mut any_removed = false;
    for entity in removed_pos.read().chain(removed_id.read()) {
        grid.unindex(entity);
        any_removed = true;
    }
    any_removed |= removed_blocker.read().count() > 0;
    any_removed |= removed_occluder.read().count() > 0;
    for (entity, &id, &pos) in &moved {
        grid.index_at(entity, id, pos);
    }

    let mut cells: Vec<u32> = std::mem::take(&mut grid.pending);
    let recompute =
        resync || !blockers_changed.is_empty() || (any_removed && !grid.dynamic.is_empty());
    if recompute {
        let mut dynamic: BTreeMap<u32, (u16, u16)> = BTreeMap::new();
        // Counts are summed, so query order does not matter.
        for (&pos, blocker, occluder) in &blockers {
            for (footprint, is_blocker) in
                [(blocker.map(|b| b.0), true), (occluder.map(|o| o.0), false)]
            {
                let Some(footprint) = footprint else { continue };
                for i in grid.footprint_indices(pos, footprint) {
                    let e = dynamic.entry(i).or_default();
                    if is_blocker {
                        e.0 = e.0.saturating_add(1);
                    } else {
                        e.1 = e.1.saturating_add(1);
                    }
                }
            }
        }
        cells.extend(grid.set_dynamic(dynamic));
    }
    if cells.is_empty() {
        return;
    }
    cells.sort_unstable();
    cells.dedup();
    let bounds = grid.bounds;
    changed.write(GridChanged {
        tick: clock.map_or(0, |c| c.tick),
        cells: cells.into_iter().map(|i| bounds.coord(i)).collect(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_each_occluding_matches_is_occluding() {
        let mut grid = SpatialGrid::new(1000, GridBounds::centered(16, 16));
        for (x, z) in [(-8, -8), (0, 0), (3, 2), (7, 7)] {
            let c = CellCoord::new(x, z);
            let mut info = grid.static_cell(c);
            info.flags.occluding = true;
            grid.set_static(c, info);
        }
        // An occluder footprint over cells 1..=2 on row 5 (dynamic layer).
        let mut dynamic = BTreeMap::new();
        for i in grid.footprint_indices(
            SimPos::new(2000, 5500),
            Footprint {
                half_x_mm: 500,
                half_z_mm: 0,
            },
        ) {
            dynamic.insert(i, (0, 1));
        }
        grid.set_dynamic(dynamic);
        for (lo, hi) in [
            (CellCoord::new(-20, -20), CellCoord::new(20, 20)),
            (CellCoord::new(1, 1), CellCoord::new(3, 5)),
            (CellCoord::new(5, 5), CellCoord::new(4, 4)),
        ] {
            let mut got = Vec::new();
            grid.for_each_occluding(lo, hi, |c| got.push(c));
            got.sort_unstable();
            got.dedup();
            let mut want = Vec::new();
            for x in lo.x..=hi.x {
                for z in lo.z..=hi.z {
                    if grid.is_occluding(CellCoord::new(x, z)) {
                        want.push(CellCoord::new(x, z));
                    }
                }
            }
            assert_eq!(got, want, "{lo:?}..={hi:?}");
        }
    }

    #[test]
    fn footprint_area_rings_and_free_cells() {
        let mut grid = SpatialGrid::new(1000, GridBounds::centered(16, 16));
        // A 2 x 1 cell block over cells (0..=1, 0).
        let pos = SimPos::new(1000, 500);
        let fp = Footprint {
            half_x_mm: 500,
            half_z_mm: 0,
        };
        assert_eq!(
            grid.footprint_cell_rect(pos, fp),
            (CellCoord::new(0, 0), CellCoord::new(1, 0))
        );
        assert_eq!(
            grid.footprint_area(pos, fp),
            (SimPos::new(0, 0), SimPos::new(2000, 1000))
        );
        let ring0 = grid.cells_around(pos, fp, 0);
        assert_eq!(ring0, grid.footprint_cells(pos, fp));
        let ring1 = grid.cells_around(pos, fp, 1);
        assert_eq!(ring1.len(), 4 * 3 - 2, "a 4 x 3 frame around 2 cells");
        assert!(ring1.windows(2).all(|w| w[0] < w[1]), "sorted");
        assert!(!ring1.contains(&CellCoord::new(0, 0)));
        assert!(ring1.contains(&CellCoord::new(-1, -1)));
        assert_eq!(grid.cells_around(pos, fp, 2).len(), 6 * 5 - 4 * 3);
        // Nearest free cell to a point south of the block: (0, -1).
        let from = SimPos::new(400, -3000);
        assert_eq!(
            grid.nearest_free_cell_around(from, pos, fp, 3),
            Some(SimPos::new(500, -500))
        );
        // Block ring 1's south side: the nearest is then on ring 1 elsewhere.
        for x in -1..=2 {
            let info = CellInfo {
                flags: CellFlags {
                    blocking: true,
                    ..CellFlags::default()
                },
                extra_cost: 0,
            };
            grid.set_static(CellCoord::new(x, -1), info);
        }
        assert_eq!(
            grid.nearest_free_cell_around(from, pos, fp, 3),
            Some(SimPos::new(-500, 500))
        );
        assert_eq!(
            area_dist2(
                SimPos::new(2500, 1000),
                SimPos::new(0, 0),
                SimPos::new(2000, 1000)
            ),
            250_000
        );
    }

    #[test]
    fn nearest_blocker_uses_the_edge() {
        let mut world = World::new();
        world.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)));
        // A long wall (cells -10..=9 at z = 0) whose centre is 9.5 m away,
        // and a small post 3 m away by centre.
        let wall = world
            .spawn((
                SimId(1),
                SimPos::new(0, 500),
                GridBlocker(Footprint {
                    half_x_mm: 9500,
                    half_z_mm: 0,
                }),
            ))
            .id();
        let post = world
            .spawn((SimId(2), SimPos::new(9500, 3500), GridBlocker::default()))
            .id();
        let from = SimPos::new(9500, 1500);
        let hit = nearest_blocker_by_edge(&world, from, 5000, |_, _| true).unwrap();
        assert_eq!((hit.id, hit.entity), (SimId(1), wall), "{hit:?}");
        assert_eq!(hit.dist2, 500 * 500);
        let hit = nearest_blocker_by_edge(&world, from, 5000, |_, e| e != wall).unwrap();
        assert_eq!(hit.entity, post);
        assert!(nearest_blocker_by_edge(&world, from, 100, |_, _| true).is_none());
    }

    #[test]
    fn bounds_index_round_trip() {
        let b = GridBounds::centered(128, 128);
        assert_eq!(b.min, CellCoord::new(-64, -64));
        assert_eq!(b.index(CellCoord::new(-64, -64)), Some(0));
        assert_eq!(b.index(CellCoord::new(63, 63)), Some(128 * 128 - 1));
        assert_eq!(b.index(CellCoord::new(64, 0)), None);
        for i in [0, 1, 127, 128, 5000] {
            assert_eq!(b.index(b.coord(i)), Some(i));
        }
    }
}
