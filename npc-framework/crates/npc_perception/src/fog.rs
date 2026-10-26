//! Fog of war, sim side (plan Task 9, spec G8): what an actor has explored,
//! what it senses right now, and the last-seen state of the static things
//! in the ground it explored.
//!
//! Opt-in per actor with the [`RevealsFog`] marker (it brings
//! [`ExploredCells`], [`VisibleCells`] and [`SeenStatics`]). On the actor's
//! perception scan ticks ([`is_due`]), right after the perception systems
//! ([`fog_system`], mechanic `fog`):
//!
//! 1. **visible** = the cells its senses reach now, seen **from the centre
//!    of the actor's cell** (fog is per cell): every vision cone
//!    ([`reveal_cells`]: cell centre within range and in the integer field
//!    of view, with grid [`line_of_sight`](crate::line_of_sight) between the
//!    two centres) and its [`ReachSense`] ([`reveal_reach_cells`]: all
//!    round, same line of sight). The actor's own cell is always visible.
//!    Hearing and proximity reveal nothing (proximity has no line of
//!    sight); robot sensors come later (Task E3). Only cells inside the grid
//!    bounds are recorded.
//! 2. **explored** |= visible (it never shrinks).
//! 3. **statics** ([`SeenStatics`]): what content declares static
//!    ([`FogStatics`]) and stands in a visible cell is recorded as seen now
//!    ([`SeenState`]: kinds, position, door open, tree felled, a wall's
//!    footprint); a record with a visible cell (its position or, for a
//!    wall, any cell of its footprint) whose thing is not there any more
//!    (taken, moved, despawned) is dropped. Records in cells not visible now
//!    keep their last-seen state.
//!
//! **Cost.** Lines of sight between cell centres depend only on the offset
//! between the cells, so they are traced once per range into a table and a
//! scan only tests the table's cells against the occluding cells of its
//! area (read once per scan, [`SpatialGrid::for_each_occluding`]). Step 1
//! is skipped (the visible cells kept) while the actor's cell, facing,
//! sensors and the grid's cells ([`SpatialGrid::revision`]) are unchanged
//! since its last scan: an idle actor, or one walking inside one cell.
//! Steps 2 and 3 run on every scan; they touch only the things that can be
//! static and reuse their buffers, so a scan allocates nothing once warm.
//!
//! Integer-only; cells are dense bit sets over the grid bounds (one `u64`
//! per 64 cells, row by row: `GridBounds::index`); records are `BTreeMap`s
//! by `SimId`. All four components are **snapshot-only** (ADR 0013,
//! [`REVEALS_FOG`], [`EXPLORED_CELLS`], [`VISIBLE_CELLS`], [`SEEN_STATICS`]):
//! the explored ground survives save/resume, and no state or content hash
//! changes (nothing in the sim reads them). After a load, a record that
//! does not fit the grid starts over ([`FogResets`]).
//!
//! The core never names content: [`FogStatics`] (set with
//! [`PerceptionAppExt::set_fog_statics`](crate::PerceptionAppExt::set_fog_statics))
//! says which [`Perceivable`] kind bits are static, which kind to record
//! `GridOccluder` entities without `Perceivable` under (wall runs), and how
//! to read "open" and "felled" from an entity.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use bevy_ecs::world::EntityRef;
use bevy_reflect::Reflect;
use npc_kernel::spatial::{CellCoord, Footprint, GridBounds, GridOccluder, SpatialGrid};
use npc_kernel::{SimClock, SimId, SimPos};
use serde::{Deserialize, Serialize};

use crate::sensors::{Facing, Perceivable, ReachSense, Sensor, Sensors, Vision};
use crate::system::is_due;

/// [`Perceivable`] kind bits (content-defined).
pub type KindBits = u32;

/// Snapshot name of [`RevealsFog`].
pub const REVEALS_FOG: &str = "reveals_fog";
/// Snapshot name of [`ExploredCells`].
pub const EXPLORED_CELLS: &str = "explored_cells";
/// Snapshot name of [`VisibleCells`].
pub const VISIBLE_CELLS: &str = "visible_cells";
/// Snapshot name of [`SeenStatics`].
pub const SEEN_STATICS: &str = "seen_statics";

/// Marker: this actor reveals fog (see the module docs). Brings an empty
/// [`ExploredCells`], [`VisibleCells`] and [`SeenStatics`]; inserting it
/// again keeps them. Snapshot-only ([`REVEALS_FOG`]).
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[require(ExploredCells, VisibleCells, SeenStatics)]
#[reflect(Component)]
pub struct RevealsFog;

/// Every cell the actor has ever sensed: one bit per cell of the grid
/// bounds (dense index `GridBounds::index`, bit `i % 64` of word `i / 64`).
/// `width` and `min` are the bounds the bits were made for (`min` is absent
/// in records written before it existed, and then not checked): a record
/// that does not fit the current grid starts over. Snapshot-only
/// ([`EXPLORED_CELLS`]).
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct ExploredCells {
    pub width: u32,
    pub bits: Vec<u64>,
    /// The bounds' lowest cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<CellCoord>,
}

impl ExploredCells {
    /// An empty record for `bounds`.
    pub fn sized(bounds: GridBounds) -> Self {
        Self {
            width: bounds.width,
            bits: vec![0; words(bounds)],
            min: Some(bounds.min),
        }
    }

    /// Whether the record was made for `bounds` (width, cell count and, when
    /// recorded, the lowest cell).
    pub fn fits(&self, bounds: GridBounds) -> bool {
        self.width == bounds.width
            && self.bits.len() == words(bounds)
            && self.min.is_none_or(|m| m == bounds.min)
    }

    /// Whether `cell` is explored (false outside the grid or for a record
    /// that does not fit it).
    pub fn is_explored(&self, grid: &SpatialGrid, cell: CellCoord) -> bool {
        self.fits(grid.bounds()) && cell_bit(&self.bits, grid.bounds(), cell)
    }

    /// Whether dense cell `index` is explored.
    pub fn get(&self, index: u32) -> bool {
        bit(&self.bits, index)
    }

    /// How many cells are explored.
    pub fn count(&self) -> u32 {
        count(&self.bits)
    }

    /// Adds the cells of `visible` (bits over `bounds`), first starting over
    /// when the record does not fit `bounds`.
    fn merge(&mut self, bounds: GridBounds, visible: &[u64]) {
        if !self.fits(bounds) {
            *self = Self::sized(bounds);
        }
        self.min = Some(bounds.min);
        for (x, v) in self.bits.iter_mut().zip(visible) {
            *x |= *v;
        }
    }
}

/// The cells the actor senses as of its last scan (same layout as
/// [`ExploredCells`], over the current grid bounds), recomputed on every
/// scan tick. Snapshot-only ([`VISIBLE_CELLS`]).
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct VisibleCells {
    pub bits: Vec<u64>,
}

impl VisibleCells {
    /// Whether the bits are sized for `bounds` (not after a load into a
    /// grid of another size, until the next scan).
    pub fn fits(&self, bounds: GridBounds) -> bool {
        self.bits.len() == words(bounds)
    }

    /// Whether `cell` was visible at the last scan (false outside the grid,
    /// and for bits that do not fit it).
    pub fn is_visible(&self, grid: &SpatialGrid, cell: CellCoord) -> bool {
        self.fits(grid.bounds()) && cell_bit(&self.bits, grid.bounds(), cell)
    }

    /// Whether dense cell `index` is visible.
    pub fn get(&self, index: u32) -> bool {
        bit(&self.bits, index)
    }

    /// How many cells are visible.
    pub fn count(&self) -> u32 {
        count(&self.bits)
    }
}

/// The last-seen state of one static thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct SeenState {
    /// Its [`Perceivable`] kinds, or [`FogStatics::occluder_kind`] for a wall.
    pub kind: KindBits,
    /// Where it was (a wall run: its centre).
    pub pos: SimPos,
    /// Open (a door), from [`FogStatics::open`]; `None` when not a door.
    pub open: Option<bool>,
    /// Felled (a tree), from [`FogStatics::felled`]; `None` when not a tree.
    pub felled: Option<bool>,
    /// A wall run's `GridOccluder` footprint around `pos` (the cells it
    /// covered); `None` for everything else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint: Option<Footprint>,
}

/// The last-seen state of the statics (walls, doors, trees, rocks,
/// stations, items) the actor saw in its explored cells, by `SimId`.
/// Snapshot-only ([`SEEN_STATICS`]).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SeenStatics(pub BTreeMap<SimId, SeenState>);

/// Reads one fact (open, felled) of an entity; `None`: does not apply.
pub type StaticStateFn = for<'a> fn(EntityRef<'a>) -> Option<bool>;

/// What content counts as static for [`SeenStatics`] (set with
/// `PerceptionAppExt::set_fog_statics`). The default records nothing:
/// explored and visible cells still work.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct FogStatics {
    /// `Perceivable` entities with any of these kind bits are statics.
    pub kinds: KindBits,
    /// `GridOccluder` entities without `Perceivable` (wall runs) are
    /// recorded with this kind when any cell of their footprint is
    /// visible; 0: not recorded.
    pub occluder_kind: KindBits,
    /// Reads "open" (doors).
    pub open: Option<StaticStateFn>,
    /// Reads "felled" (trees).
    pub felled: Option<StaticStateFn>,
}

impl FogStatics {
    fn state(
        &self,
        e: EntityRef,
        kind: KindBits,
        pos: SimPos,
        footprint: Option<Footprint>,
    ) -> SeenState {
        SeenState {
            kind,
            pos,
            open: self.open.and_then(|f| f(e)),
            felled: self.felled.and_then(|f| f(e)),
            footprint,
        }
    }
}

/// The actors whose explored record the last snapshot load started over
/// (it did not fit the loaded grid), ascending. Inserted by the load (only
/// when there are some) for the host to report; a `warn!` names them too.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct FogResets(pub Vec<SimId>);

/// Snapshot rebuild (after every load): an [`ExploredCells`] that does not
/// fit the loaded grid starts over (and is listed in [`FogResets`]); a
/// [`VisibleCells`] of another size is emptied until the next scan.
pub(crate) fn fog_after_load(world: &mut World) {
    let Some(bounds) = world.get_resource::<SpatialGrid>().map(SpatialGrid::bounds) else {
        return;
    };
    let mut q = world.query::<(&SimId, &mut ExploredCells, Option<&mut VisibleCells>)>();
    let mut reset = Vec::new();
    for (&id, mut explored, visible) in q.iter_mut(world) {
        if !explored.fits(bounds) {
            *explored = ExploredCells::sized(bounds);
            reset.push(id);
        }
        if let Some(mut visible) = visible
            && !visible.fits(bounds)
        {
            visible.bits.clear();
        }
    }
    world.remove_resource::<FogResets>();
    if reset.is_empty() {
        return;
    }
    reset.sort_unstable();
    tracing::warn!(
        actors = ?reset.iter().map(|id| id.0).collect::<Vec<_>>(),
        "fog of war: the explored ground of these actors was saved for another grid and starts over"
    );
    world.insert_resource(FogResets(reset));
}

// ---------------------------------------------------------------------------
// Bits
// ---------------------------------------------------------------------------

fn words(bounds: GridBounds) -> usize {
    bounds.len().div_ceil(64)
}

fn bit(bits: &[u64], index: u32) -> bool {
    bits.get((index / 64) as usize)
        .is_some_and(|w| w & (1 << (index % 64)) != 0)
}

fn set_bit(bits: &mut [u64], index: u32) {
    if let Some(w) = bits.get_mut((index / 64) as usize) {
        *w |= 1 << (index % 64);
    }
}

fn cell_bit(bits: &[u64], bounds: GridBounds, cell: CellCoord) -> bool {
    bounds.index(cell).is_some_and(|i| bit(bits, i))
}

fn count(bits: &[u64]) -> u32 {
    bits.iter().map(|w| w.count_ones()).sum()
}

/// The dense indices of the set bits, ascending.
fn set_indices(bits: &[u64]) -> impl Iterator<Item = u32> + '_ {
    bits.iter().enumerate().flat_map(|(w, &word)| {
        let mut rest = word;
        std::iter::from_fn(move || {
            if rest == 0 {
                return None;
            }
            let b = rest.trailing_zeros();
            rest &= rest - 1;
            Some(w as u32 * 64 + b)
        })
    })
}

// ---------------------------------------------------------------------------
// Revealing
// ---------------------------------------------------------------------------

/// Every cell offset whose centre is within `range_mm` of the origin cell's
/// centre (the origin excluded), each with the cells strictly between the
/// two centres that line of sight crosses (the grid's own supercover trace,
/// [`SpatialGrid::trace_segment`]). A trace between two cell centres depends
/// only on their offset, so one table serves every scan of that range.
struct RayTable {
    cell_mm: i32,
    range_mm: i32,
    /// The table reaches this many cells from the origin per axis.
    radius_cells: i32,
    /// `(dx, dz, start, end)`: the offset and its crossed cells in
    /// `between[start..end]`, nearest first.
    offsets: Vec<(i32, i32, u32, u32)>,
    /// Crossed cells, relative to the origin.
    between: Vec<(i32, i32)>,
}

/// The largest range, in cells, a [`RayTable`] is built for (about 5000
/// offsets, under 1.5 MB). A longer range is revealed by tracing each target
/// cell's line of sight ([`reveal_direct`]): the same cells, no table.
pub const MAX_TABLE_RADIUS_CELLS: i32 = 40;

/// Whole cells a range reaches from a cell centre (per axis).
fn radius_cells(cell_mm: i32, range_mm: i32) -> i32 {
    i32::try_from(i64::from(range_mm.max(0)) / i64::from(cell_mm.max(1))).unwrap_or(i32::MAX / 4)
}

impl RayTable {
    fn new(cell_mm: i32, range_mm: i32) -> Self {
        let range_mm = range_mm.max(0);
        let c = i64::from(cell_mm.max(1));
        let r2 = i64::from(range_mm) * i64::from(range_mm);
        let radius_cells = radius_cells(cell_mm, range_mm);
        debug_assert!(radius_cells <= MAX_TABLE_RADIUS_CELLS);
        // A grid of this cell size: only its geometry is used.
        let geometry = SpatialGrid::new(cell_mm.max(1), GridBounds::centered(0, 0));
        let origin = geometry.center(CellCoord::new(0, 0));
        let mut table = Self {
            cell_mm,
            range_mm,
            radius_cells,
            offsets: Vec::new(),
            between: Vec::new(),
        };
        for dz in -radius_cells..=radius_cells {
            for dx in -radius_cells..=radius_cells {
                let (x, z) = (i64::from(dx), i64::from(dz));
                if (dx, dz) == (0, 0) || (x * x + z * z) * c * c > r2 {
                    continue;
                }
                let target = CellCoord::new(dx, dz);
                let start = table.between.len() as u32;
                geometry.trace_segment(origin, geometry.center(target), |t| {
                    if t != CellCoord::new(0, 0) && t != target {
                        table.between.push((t.x, t.z));
                    }
                    true
                });
                table
                    .offsets
                    .push((dx, dz, start, table.between.len() as u32));
            }
        }
        table
    }
}

/// The occluding cells of the square area a table covers around one cell,
/// read once per scan (one byte per cell: the scan's inner loop is a load).
#[derive(Default)]
struct AreaOcclusion {
    /// The area last filled: its centre, radius and the grid's revision
    /// (`None`: nothing yet).
    filled: Option<(CellCoord, i32, u64)>,
    /// `2 · radius + 1`.
    side: i32,
    /// Index of the centre cell.
    centre: i32,
    /// The Chebyshev distance from the centre to the nearest occluding cell
    /// other than the centre (`i32::MAX`: none): a ray whose target is
    /// nearer crosses no occluder.
    nearest: i32,
    /// Empty when the area is larger than [`AreaOcclusion::MAX_RADIUS`]:
    /// then every cell is asked of the grid.
    occluding: Vec<bool>,
}

impl AreaOcclusion {
    /// Above this radius (cells) the area is not read into memory.
    const MAX_RADIUS: i32 = 512;

    /// Reads the area of `radius` cells around `at` (unless it holds it
    /// already for the same grid revision).
    fn fill(&mut self, grid: &SpatialGrid, at: CellCoord, radius: i32) {
        let key = (at, radius, grid.revision());
        if self.filled == Some(key) {
            return;
        }
        self.filled = Some(key);
        if radius > Self::MAX_RADIUS {
            self.occluding.clear();
            self.nearest = 0;
            return;
        }
        let side = 2 * i64::from(radius) + 1;
        self.side = i32::try_from(side).unwrap_or(i32::MAX);
        self.centre = i32::try_from(i64::from(radius) * side + i64::from(radius)).unwrap_or(0);
        self.occluding.clear();
        self.occluding
            .resize(usize::try_from(side * side).unwrap_or(0), false);
        let lo = CellCoord::new(at.x.saturating_sub(radius), at.z.saturating_sub(radius));
        let hi = CellCoord::new(at.x.saturating_add(radius), at.z.saturating_add(radius));
        let (occluding, nearest) = (&mut self.occluding, &mut self.nearest);
        *nearest = i32::MAX;
        grid.for_each_occluding(lo, hi, |c| {
            let i = (i64::from(c.z) - i64::from(lo.z)) * side + i64::from(c.x) - i64::from(lo.x);
            if let Some(o) = usize::try_from(i).ok().and_then(|i| occluding.get_mut(i)) {
                *o = true;
            }
            let cheb = (c.x.abs_diff(at.x)).max(c.z.abs_diff(at.z));
            if cheb > 0 {
                *nearest = (*nearest).min(i32::try_from(cheb).unwrap_or(i32::MAX));
            }
        });
    }
}

/// Sets the bit of every in-bounds cell `table` reaches from cell `at` that
/// is inside `cone` (when given: the facing and the vision) and has no
/// occluding cell strictly between the two centres.
/// `occlusion` holds at least `table.radius_cells` around `at`.
fn reveal_with(
    table: &RayTable,
    grid: &SpatialGrid,
    occlusion: &AreaOcclusion,
    at: CellCoord,
    cone: Option<(Facing, Vision)>,
    bits: &mut [u64],
) {
    let bounds = grid.bounds();
    let (side, centre) = (occlusion.side, occlusion.centre);
    let c = i64::from(table.cell_mm);
    let cone = cone.map(|(facing, v)| Cone::new(facing, v, table.radius_cells));
    for &(dx, dz, start, end) in &table.offsets {
        let (x, z) = (at.x.saturating_add(dx), at.z.saturating_add(dz));
        let Some(i) = bounds.index(CellCoord::new(x, z)) else {
            continue;
        };
        if bit(bits, i) {
            continue;
        }
        if let Some(cone) = &cone
            && !cone.contains(dx, dz, c)
        {
            continue;
        }
        // Every crossed cell is at most as far (Chebyshev) as the target.
        let near = dx.unsigned_abs().max(dz.unsigned_abs()) < occlusion.nearest.unsigned_abs();
        let between = &table.between[start as usize..end as usize];
        let clear = near
            || if occlusion.occluding.is_empty() {
                between.iter().all(|&(bx, bz)| {
                    !grid.is_occluding(CellCoord::new(
                        at.x.saturating_add(bx),
                        at.z.saturating_add(bz),
                    ))
                })
            } else {
                between
                    .iter()
                    .all(|&(bx, bz)| !occlusion.occluding[(centre + bz * side + bx) as usize])
            };
        if clear {
            set_bit(bits, i);
        }
    }
}

/// [`in_fov`](crate::in_fov) for cell offsets `o` (cells) of one scan: the
/// direction is `o · cell_mm`, and `cell_mm²` cancels out of every term, so
/// the same test runs in `i64` on the offsets (exact while the offsets are
/// at most [`Cone::SMALL`] cells; beyond that, `in_fov` itself).
struct Cone {
    facing: Facing,
    vision: Vision,
    /// The reduced facing and its squared length.
    f: (i64, i64),
    f2: i64,
    small: bool,
}

impl Cone {
    /// Offsets up to this many cells per axis keep every product below
    /// `i64::MAX` (`|f| ≤ 2¹⁵`, `cos2 ≤ 10⁶`, `|o|² ≤ 2 · 40²`).
    const SMALL: i32 = 40;

    fn new(facing: Facing, vision: Vision, radius_cells: i32) -> Self {
        let (fx, fz) = facing.reduced();
        let f = (i64::from(fx), i64::from(fz));
        Self {
            facing,
            vision,
            f,
            f2: f.0 * f.0 + f.1 * f.1,
            // The `i64` products are bounded for at most `SMALL` cells and a
            // `cos2_ppm` of at most `COS2_SCALE` (`Vision`'s fields are
            // public and deserialized: anything else takes `in_fov`'s `i128`).
            small: radius_cells <= Self::SMALL && vision.cos2_ppm <= crate::COS2_SCALE,
        }
    }

    fn contains(&self, dx: i32, dz: i32, cell_mm: i64) -> bool {
        let half = self.vision.half_fov_deg;
        if half >= 180 || self.f2 == 0 {
            return true;
        }
        if !self.small {
            let d = (i64::from(dx) * cell_mm, i64::from(dz) * cell_mm);
            return self.vision.sees_direction(self.facing, d);
        }
        let (ox, oz) = (i64::from(dx), i64::from(dz));
        let dot = self.f.0 * ox + self.f.1 * oz;
        if half == 90 {
            return dot >= 0;
        }
        if half < 90 && dot < 0 {
            return false;
        }
        let lhs = dot * dot * i64::from(crate::COS2_SCALE);
        let rhs = i64::from(self.vision.cos2_ppm) * self.f2 * (ox * ox + oz * oz);
        if half < 90 {
            lhs >= rhs
        } else {
            dot >= 0 || lhs <= rhs
        }
    }
}

/// The ray tables of the ranges in use (a handful), built on first use.
#[derive(Default)]
struct RayTables(Vec<RayTable>);

impl RayTables {
    fn get(&mut self, cell_mm: i32, range_mm: i32) -> usize {
        let range_mm = range_mm.max(0);
        if let Some(i) = self
            .0
            .iter()
            .position(|t| t.cell_mm == cell_mm && t.range_mm == range_mm)
        {
            return i;
        }
        self.0.push(RayTable::new(cell_mm, range_mm));
        self.0.len() - 1
    }
}

/// What one scan senses with.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Senses<'a> {
    visions: &'a [Vision],
    reach: Option<ReachSense>,
}

/// Computes the visible cells from cell `at` into `bits` (sized for the
/// grid, cleared here); returns the area that holds every visible cell.
fn reveal_all(
    grid: &SpatialGrid,
    at: CellCoord,
    facing: Facing,
    senses: &Senses,
    tables: &mut RayTables,
    occlusion: &mut AreaOcclusion,
    bits: &mut Vec<u64>,
) -> (CellCoord, CellCoord) {
    let bounds = grid.bounds();
    bits.clear();
    bits.resize(words(bounds), 0);
    if let Some(i) = bounds.index(at) {
        set_bit(bits, i);
    }
    let cell_mm = grid.cell_mm();
    let table = |range_mm: i32| radius_cells(cell_mm, range_mm) <= MAX_TABLE_RADIUS_CELLS;
    // One occluder read for the table sensors, as large as their largest
    // range; the area (for the statics) covers every sensor.
    let mut radius = 0;
    let mut table_radius = 0;
    let ranges = senses
        .visions
        .iter()
        .map(|v| v.range_mm)
        .chain(senses.reach.map(|r| r.radius_mm));
    for range_mm in ranges {
        let r = radius_cells(cell_mm, range_mm);
        radius = radius.max(r);
        if table(range_mm) {
            table_radius = table_radius.max(r);
        }
    }
    occlusion.fill(grid, at, table_radius);
    for v in senses.visions {
        if table(v.range_mm) {
            let t = tables.get(cell_mm, v.range_mm);
            reveal_with(&tables.0[t], grid, occlusion, at, Some((facing, *v)), bits);
        } else {
            reveal_direct(grid, at, v.range_mm, |d| v.sees_direction(facing, d), bits);
        }
    }
    if let Some(r) = senses.reach {
        if table(r.radius_mm) {
            let t = tables.get(cell_mm, r.radius_mm);
            reveal_with(&tables.0[t], grid, occlusion, at, None, bits);
        } else {
            reveal_direct(grid, at, r.radius_mm, |_| true, bits);
        }
    }
    (
        CellCoord::new(at.x.saturating_sub(radius), at.z.saturating_sub(radius)),
        CellCoord::new(at.x.saturating_add(radius), at.z.saturating_add(radius)),
    )
}

/// [`reveal_with`] without a table, for a range beyond
/// [`MAX_TABLE_RADIUS_CELLS`]: every in-bounds cell whose centre is within
/// `range_mm` of `at`'s centre and passes `in_shape` (the offset between the
/// centres, mm), with [`line_of_sight`](crate::line_of_sight) traced from
/// centre to centre. Allocates nothing.
fn reveal_direct(
    grid: &SpatialGrid,
    at: CellCoord,
    range_mm: i32,
    in_shape: impl Fn((i64, i64)) -> bool,
    bits: &mut [u64],
) {
    let bounds = grid.bounds();
    if bounds.is_empty() {
        return;
    }
    let r = radius_cells(grid.cell_mm(), range_mm);
    let r2 = i64::from(range_mm.max(0)) * i64::from(range_mm.max(0));
    let last =
        |min: i32, n: u32| i32::try_from(i64::from(min) + i64::from(n) - 1).unwrap_or(i32::MAX);
    let (x0, x1) = (
        at.x.saturating_sub(r).max(bounds.min.x),
        at.x.saturating_add(r).min(last(bounds.min.x, bounds.width)),
    );
    let (z0, z1) = (
        at.z.saturating_sub(r).max(bounds.min.z),
        at.z.saturating_add(r)
            .min(last(bounds.min.z, bounds.height)),
    );
    let eye = grid.center(at);
    for z in z0..=z1 {
        for x in x0..=x1 {
            let cell = CellCoord::new(x, z);
            let Some(i) = bounds.index(cell) else {
                continue;
            };
            if cell == at || bit(bits, i) {
                continue;
            }
            let p = grid.center(cell);
            let d = (
                i64::from(p.x) - i64::from(eye.x),
                i64::from(p.z) - i64::from(eye.z),
            );
            if d.0 * d.0 + d.1 * d.1 <= r2 && in_shape(d) && crate::line_of_sight(grid, eye, p) {
                set_bit(bits, i);
            }
        }
    }
}

/// The cells in `bits` as coordinates, sorted (`CellCoord` order).
fn cells_of(bounds: GridBounds, bits: &[u64]) -> Vec<CellCoord> {
    let mut out: Vec<CellCoord> = set_indices(bits).map(|i| bounds.coord(i)).collect();
    out.sort_unstable();
    out
}

/// The in-bounds cells `sensor` reveals for an actor at `from` looking along
/// `facing`, sorted by `CellCoord`, no duplicates. Fog is per cell: it is
/// seen from the centre of `from`'s cell. Vision: that cell, plus every cell
/// whose centre is within range of it and inside the cone (the integer FOV
/// test, edges inclusive) with [`line_of_sight`](crate::line_of_sight)
/// between the two centres (occluding cells themselves are revealed, the
/// cells behind them are not; the actor's own cell never blocks). Hearing
/// and proximity reveal nothing (robot sensors: Task E3).
pub fn reveal_cells(
    grid: &SpatialGrid,
    from: SimPos,
    facing: Facing,
    sensor: &Sensor,
) -> Vec<CellCoord> {
    match *sensor {
        Sensor::Vision(v) => reveal_once(
            grid,
            from,
            facing,
            &Senses {
                visions: &[v],
                reach: None,
            },
        ),
        // The LIDAR, camera and radar (plan Task 12) reveal fog from Task
        // 13 (E3) on.
        Sensor::Hearing(_)
        | Sensor::Proximity(_)
        | Sensor::Lidar(_)
        | Sensor::Camera(_)
        | Sensor::Radar(_) => Vec::new(),
    }
}

/// The in-bounds cells a [`ReachSense`] reveals for an actor at `from`: the
/// actor's cell, plus every cell whose centre is within the radius of its
/// centre, any direction, with line of sight; sorted.
pub fn reveal_reach_cells(grid: &SpatialGrid, from: SimPos, reach: ReachSense) -> Vec<CellCoord> {
    reveal_once(
        grid,
        from,
        Facing::default(),
        &Senses {
            visions: &[],
            reach: Some(reach),
        },
    )
}

fn reveal_once(
    grid: &SpatialGrid,
    from: SimPos,
    facing: Facing,
    senses: &Senses,
) -> Vec<CellCoord> {
    let mut bits = Vec::new();
    reveal_all(
        grid,
        grid.cell_of(from),
        facing,
        senses,
        &mut RayTables::default(),
        &mut AreaOcclusion::default(),
        &mut bits,
    );
    cells_of(grid.bounds(), &bits)
}

// ---------------------------------------------------------------------------
// The pass
// ---------------------------------------------------------------------------

/// The actors that reveal fog (the fog pass's cached query).
pub type FogAgentQuery = QueryState<
    (
        Entity,
        &'static SimId,
        &'static SimPos,
        Option<&'static Sensors>,
        Option<&'static Facing>,
        Option<&'static ReachSense>,
    ),
    With<RevealsFog>,
>;

/// The perceivable things, candidates for statics (the fog pass's cached
/// query).
pub type FogThingQuery = QueryState<(
    Entity,
    &'static SimId,
    &'static SimPos,
    &'static Perceivable,
)>;

/// The wall-like occluders recorded as statics (the fog pass's cached query).
pub type FogOccluderQuery = QueryState<
    (
        Entity,
        &'static SimId,
        &'static SimPos,
        &'static GridOccluder,
    ),
    Without<Perceivable>,
>;

/// One due actor; its visions are `FogScratch::visions[vis.0..vis.1]`.
#[derive(Clone, Copy)]
struct FogAgent {
    id: SimId,
    entity: Entity,
    cell: CellCoord,
    facing: Facing,
    vis: (usize, usize),
    reach: Option<ReachSense>,
}

/// What an actor's visible cells were last computed from.
#[derive(Clone, Debug)]
struct FogKey {
    cell: CellCoord,
    facing: Facing,
    visions: Vec<Vision>,
    reach: Option<ReachSense>,
    bounds: GridBounds,
    cell_mm: i32,
    revision: u64,
    /// The area the scan read.
    area: (CellCoord, CellCoord),
}

/// The fog pass's working memory, kept across ticks (a `Local` of
/// [`fog_system`]). Not sim state: it only
/// decides whether a scan can reuse the visible cells (a full scan gives
/// the same cells) and holds reused buffers.
#[derive(Default)]
pub struct FogScratch {
    tables: RayTables,
    occlusion: AreaOcclusion,
    /// Last scan per actor entity (a loaded world has new entities: none).
    keys: BTreeMap<Entity, FogKey>,
    agents: Vec<FogAgent>,
    visions: Vec<Vision>,
    seen: Vec<(SimId, SeenState)>,
    gone: Vec<SimId>,
}

/// `NpcSet::Sense` (exclusive), in [`NpcFogSet`](crate::NpcFogSet) after
/// the perception systems: the fog update (see the module docs) of every
/// [`RevealsFog`] actor on its perception scan ticks ([`is_due`] with
/// [`PerceptionConfig::stagger`](crate::PerceptionConfig)), in `SimId`
/// order. Its own system and mechanic (`fog`), so its cost shows apart
/// from `perception`'s.
pub fn fog_system(
    world: &mut World,
    agents_q: &mut FogAgentQuery,
    things_q: &mut FogThingQuery,
    occluders_q: &mut FogOccluderQuery,
    mut scratch: Local<FogScratch>,
) {
    // The system span is tagged `fog` too: the profiler counts it once.
    let _span = tracing::info_span!("mechanic", name = "fog").entered();
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    let stagger = world
        .get_resource::<crate::PerceptionConfig>()
        .map_or(crate::DEFAULT_STAGGER, |c| c.stagger);
    fog_pass(
        world,
        (agents_q, things_q, occluders_q),
        &mut scratch,
        tick,
        stagger,
    );
}

/// The fog update for every [`RevealsFog`] actor due in `tick`.
fn fog_pass(
    world: &mut World,
    queries: (
        &mut FogAgentQuery,
        &mut FogThingQuery,
        &mut FogOccluderQuery,
    ),
    scratch: &mut FogScratch,
    tick: u64,
    stagger: u32,
) {
    let (agents_q, things_q, occluders_q) = queries;
    let Some(cell_mm) = world
        .get_resource::<SpatialGrid>()
        .map(SpatialGrid::cell_mm)
    else {
        return;
    };
    scratch.agents.clear();
    scratch.visions.clear();
    for (entity, &id, &pos, sensors, facing, reach) in agents_q.iter(world) {
        if !is_due(id, tick, stagger) {
            continue;
        }
        let start = scratch.visions.len();
        if let Some(s) = sensors {
            scratch.visions.extend(s.visions());
        }
        scratch.agents.push(FogAgent {
            id,
            entity,
            cell: CellCoord::new(pos.x.div_euclid(cell_mm), pos.z.div_euclid(cell_mm)),
            facing: facing.copied().unwrap_or_default(),
            vis: (start, scratch.visions.len()),
            reach: reach.copied(),
        });
    }
    if scratch.agents.is_empty() {
        return;
    }
    scratch.agents.sort_unstable_by_key(|a| a.id);
    let statics = world
        .get_resource::<FogStatics>()
        .copied()
        .unwrap_or_default();

    for n in 0..scratch.agents.len() {
        let agent = scratch.agents[n];
        // Reuse the actor's own buffer.
        let mut bits = world
            .get_mut::<VisibleCells>(agent.entity)
            .map(|mut v| std::mem::take(&mut v.bits))
            .unwrap_or_default();
        let bounds = {
            let world: &World = world;
            let grid = world.resource::<SpatialGrid>();
            let bounds = grid.bounds();
            let senses = Senses {
                visions: &scratch.visions[agent.vis.0..agent.vis.1],
                reach: agent.reach,
            };
            let unchanged = scratch.keys.get(&agent.entity).is_some_and(|k| {
                k.cell == agent.cell
                    && k.facing == agent.facing
                    && k.visions == senses.visions
                    && k.reach == senses.reach
                    && k.bounds == bounds
                    && k.cell_mm == grid.cell_mm()
                    && k.revision == grid.revision()
            }) && bits.len() == words(bounds);
            if !unchanged {
                let area = reveal_all(
                    grid,
                    agent.cell,
                    agent.facing,
                    &senses,
                    &mut scratch.tables,
                    &mut scratch.occlusion,
                    &mut bits,
                );
                let key = scratch.keys.entry(agent.entity).or_insert_with(|| FogKey {
                    cell: agent.cell,
                    facing: agent.facing,
                    visions: Vec::new(),
                    reach: None,
                    bounds,
                    cell_mm: grid.cell_mm(),
                    revision: grid.revision(),
                    area,
                });
                key.cell = agent.cell;
                key.facing = agent.facing;
                key.visions.clear();
                key.visions.extend_from_slice(senses.visions);
                key.reach = senses.reach;
                key.bounds = bounds;
                key.cell_mm = grid.cell_mm();
                key.revision = grid.revision();
                key.area = area;
            }
            let area = scratch.keys[&agent.entity].area;
            observe(
                world,
                grid,
                (&mut *things_q, &mut *occluders_q),
                &bits,
                area,
                &statics,
                agent.id,
                &mut scratch.seen,
            );
            scratch.gone.clear();
            if let Some(records) = world.get::<SeenStatics>(agent.entity) {
                for (&id, st) in &records.0 {
                    if scratch.seen.binary_search_by_key(&id, |s| s.0).is_ok() {
                        continue;
                    }
                    let visible = match st.footprint {
                        Some(fp) => {
                            let (lo, hi) = grid.footprint_cell_rect(st.pos, fp);
                            rect_visible(&bits, bounds, lo, hi)
                        }
                        None => cell_bit(&bits, bounds, grid.cell_of(st.pos)),
                    };
                    if visible {
                        scratch.gone.push(id);
                    }
                }
            }
            bounds
        };
        if let Some(mut explored) = world.get_mut::<ExploredCells>(agent.entity) {
            explored.merge(bounds, &bits);
        }
        if let Some(mut records) = world.get_mut::<SeenStatics>(agent.entity) {
            for id in &scratch.gone {
                records.0.remove(id);
            }
            for &(id, state) in &scratch.seen {
                records.0.insert(id, state);
            }
        }
        if let Some(mut visible) = world.get_mut::<VisibleCells>(agent.entity) {
            visible.bits = bits;
        }
    }
    // Forget the actors that stopped revealing fog.
    scratch
        .keys
        .retain(|&e, _| world.get::<RevealsFog>(e).is_some());
}

/// Whether any in-bounds cell of `lo..=hi` is set in `bits`.
fn rect_visible(bits: &[u64], bounds: GridBounds, lo: CellCoord, hi: CellCoord) -> bool {
    (lo.x..=hi.x).any(|x| (lo.z..=hi.z).any(|z| cell_bit(bits, bounds, CellCoord::new(x, z))))
}

/// Whether the rectangles `a` and `b` (inclusive cells) overlap.
fn overlaps(a: (CellCoord, CellCoord), b: (CellCoord, CellCoord)) -> bool {
    a.0.x <= b.1.x && b.0.x <= a.1.x && a.0.z <= b.1.z && b.0.z <= a.1.z
}

/// The statics standing in the cells of `visible` now, into `seen` (sorted
/// by `SimId`). Only things inside `area` (the scan's area, which holds
/// every visible cell) can be visible: everything else is rejected by one
/// comparison before anything is read.
#[allow(clippy::too_many_arguments)]
fn observe(
    world: &World,
    grid: &SpatialGrid,
    queries: (&mut FogThingQuery, &mut FogOccluderQuery),
    visible: &[u64],
    area: (CellCoord, CellCoord),
    statics: &FogStatics,
    observer: SimId,
    seen: &mut Vec<(SimId, SeenState)>,
) {
    let (things_q, occluders_q) = queries;
    seen.clear();
    let bounds = grid.bounds();
    if statics.kinds != 0 {
        for (entity, &id, &pos, p) in things_q.iter(world) {
            if id == observer || p.kinds & statics.kinds == 0 {
                continue;
            }
            let cell = grid.cell_of(pos);
            if !overlaps((cell, cell), area) || !cell_bit(visible, bounds, cell) {
                continue;
            }
            seen.push((id, statics.state(world.entity(entity), p.kinds, pos, None)));
        }
    }
    if statics.occluder_kind != 0 {
        for (entity, &id, &pos, o) in occluders_q.iter(world) {
            if id == observer {
                continue;
            }
            let rect = grid.footprint_cell_rect(pos, o.0);
            if !overlaps(rect, area) {
                continue;
            }
            let lo = CellCoord::new(rect.0.x.max(area.0.x), rect.0.z.max(area.0.z));
            let hi = CellCoord::new(rect.1.x.min(area.1.x), rect.1.z.min(area.1.z));
            if rect_visible(visible, bounds, lo, hi) {
                let e = world.entity(entity);
                seen.push((id, statics.state(e, statics.occluder_kind, pos, Some(o.0))));
            }
        }
    }
    seen.sort_unstable_by_key(|s| s.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_indices_lists_every_bit() {
        let bits = [0b1001_u64, 0, 1 << 63];
        assert_eq!(set_indices(&bits).collect::<Vec<_>>(), vec![0, 3, 191]);
        assert_eq!(count(&bits), 3);
        assert!(bit(&bits, 191) && !bit(&bits, 190) && !bit(&bits, 999));
    }

    #[test]
    fn ray_table_holds_the_disc() {
        let t = RayTable::new(1000, 1500);
        let mut offsets: Vec<(i32, i32)> = t.offsets.iter().map(|o| (o.0, o.1)).collect();
        offsets.sort_unstable();
        // Centres within 1.5 m: the 8 neighbours (diagonals at 1.41 m).
        assert_eq!(offsets.len(), 8);
        assert_eq!(t.radius_cells, 1);
        // Straight neighbours have nothing between; nor do diagonals (the
        // supercover passes exactly through the shared corner: both side
        // cells are crossed).
        for &(dx, dz, s, e) in &t.offsets {
            let n = e - s;
            if dx != 0 && dz != 0 {
                assert_eq!(n, 2, "({dx}, {dz})");
            } else {
                assert_eq!(n, 0, "({dx}, {dz})");
            }
        }
    }
}
