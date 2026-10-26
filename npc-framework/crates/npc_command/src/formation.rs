//! Integer formations for group orders (pure functions, no `World`).
//!
//! A group order to N units becomes N slots around the order's target (the
//! **anchor**), facing from the group's centroid towards it:
//!
//! - **Local slots** ([`local_slots`]): `(lateral, depth)` offsets in mm,
//!   front row first, each row left to right. `lateral > 0` is to the
//!   right, `depth > 0` behind.
//!   - `Line`: one row, centred on the anchor: lateral `(2i - (n-1)) * spacing / 2`.
//!   - `Box`: `cols = ⌈√n⌉` columns, `⌈n / cols⌉` rows; every row centred
//!     laterally (the last one may be shorter), rows centred in depth:
//!     depth `(2r - (rows-1)) * spacing / 2`.
//!   - `Wedge`: the tip on the anchor; row `k ≥ 1` has a left and a right
//!     slot at lateral `∓k·spacing`, depth `k·spacing` (the last row may
//!     only have its left slot).
//!
//!   Halves truncate towards zero: use an even spacing for exact slots.
//! - **Facing** ([`facing`]): `target - centroid` scaled to length
//!   [`FACING_SCALE`] with integer `isqrt`; north `(0, -1)` when they
//!   coincide. A slot is `anchor + (right · lateral + back · depth) /
//!   FACING_SCALE` (rounded to nearest), with `right = (-fz, fx)` and `back
//!   = -facing`, so axis-aligned facings give exact positions.
//! - **Assignment** ([`plan_group`]): units sorted front to back by their
//!   own projection on the facing, cut into the rows' sizes, and inside a
//!   row matched left to right, so paths rarely cross. Ties by `SimId`.
//! - **Grid validation** ([`relocate_blocked`]): a slot in a blocking cell
//!   (or outside the grid bounds while the anchor is inside) moves to the
//!   nearest free cell centre within a ring radius, never onto a cell
//!   another slot uses; candidates are ordered by `(distance², x, z)`.
//! - **Leader**: the unit whose local slot is closest to the anchor (ties:
//!   the earlier slot).
//! - **Limits**: spacing from input is `1..=POS_LIMIT`
//!   ([`Formation::validate`]); world slots are clamped to ±`POS_LIMIT`.

use std::collections::BTreeSet;

use bevy_reflect::Reflect;
use npc_kernel::commands::{CommandError, POS_LIMIT, invalid};
use npc_kernel::spatial::{CellCoord, SpatialGrid};
use npc_kernel::{SimId, SimPos};
use serde::{Deserialize, Serialize};

/// Facing vectors are scaled to this length.
pub const FACING_SCALE: i64 = 1024;

/// The shape of a group's slots.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    Reflect,
)]
pub enum FormationKind {
    Line,
    #[default]
    Box,
    Wedge,
}

/// A formation: shape plus slot spacing in mm. JSON `{"kind":"Line","spacing_mm":1500}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct Formation {
    pub kind: FormationKind,
    pub spacing_mm: i32,
}

impl Default for Formation {
    fn default() -> Self {
        Self {
            kind: FormationKind::Box,
            spacing_mm: 1500,
        }
    }
}

impl Formation {
    pub const fn new(kind: FormationKind, spacing_mm: i32) -> Self {
        Self { kind, spacing_mm }
    }

    /// Checks a formation from external input (`IssueOrder`, `CreateSquad`):
    /// `spacing_mm` in `1..=POS_LIMIT`.
    pub fn validate(&self) -> Result<(), CommandError> {
        if self.spacing_mm <= 0 || self.spacing_mm > POS_LIMIT {
            return Err(invalid(
                "formation.spacing_mm",
                format!("{} is outside 1..={POS_LIMIT}", self.spacing_mm),
            ));
        }
        Ok(())
    }
}

/// One slot in the formation's own frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalSlot {
    /// Row, 0 = front.
    pub row: u32,
    /// mm to the right of the anchor.
    pub lateral: i32,
    /// mm behind the anchor.
    pub depth: i32,
}

fn half(v: i64, spacing: i32) -> i32 {
    i32::try_from(v * i64::from(spacing) / 2).unwrap_or(i32::MAX)
}

fn ceil_sqrt(n: usize) -> usize {
    let r = n.isqrt();
    if r * r < n { r + 1 } else { r }
}

/// The `n` local slots of `formation`, front row first, each row left to
/// right (see the module docs for the shapes).
pub fn local_slots(formation: Formation, n: usize) -> Vec<LocalSlot> {
    let s = formation.spacing_mm.max(0);
    let mut out = Vec::with_capacity(n);
    match formation.kind {
        FormationKind::Line => {
            let k = n as i64;
            for i in 0..k {
                out.push(LocalSlot {
                    row: 0,
                    lateral: half(2 * i - (k - 1), s),
                    depth: 0,
                });
            }
        }
        FormationKind::Box => {
            if n == 0 {
                return out;
            }
            let cols = ceil_sqrt(n);
            let rows = n.div_ceil(cols);
            for r in 0..rows {
                let k = cols.min(n - r * cols) as i64;
                let depth = half(2 * r as i64 - (rows as i64 - 1), s);
                for c in 0..k {
                    out.push(LocalSlot {
                        row: r as u32,
                        lateral: half(2 * c - (k - 1), s),
                        depth,
                    });
                }
            }
        }
        FormationKind::Wedge => {
            if n == 0 {
                return out;
            }
            out.push(LocalSlot {
                row: 0,
                lateral: 0,
                depth: 0,
            });
            let mut k: i32 = 1;
            while out.len() < n {
                let off = k.saturating_mul(s);
                out.push(LocalSlot {
                    row: k as u32,
                    lateral: -off,
                    depth: off,
                });
                if out.len() < n {
                    out.push(LocalSlot {
                        row: k as u32,
                        lateral: off,
                        depth: off,
                    });
                }
                k += 1;
            }
        }
    }
    out
}

/// The facing from `from` towards `to`, scaled to [`FACING_SCALE`]; north
/// `(0, -FACING_SCALE)` when they coincide.
pub fn facing(from: SimPos, to: SimPos) -> (i64, i64) {
    let dx = i64::from(to.x) - i64::from(from.x);
    let dz = i64::from(to.z) - i64::from(from.z);
    let len = (dx * dx + dz * dz).isqrt();
    if len == 0 {
        return (0, -FACING_SCALE);
    }
    (dx * FACING_SCALE / len, dz * FACING_SCALE / len)
}

/// `a / b` rounded to nearest (halves away from zero), `b > 0`.
fn div_round(a: i64, b: i64) -> i64 {
    if a >= 0 {
        (a + b / 2) / b
    } else {
        -((-a + b / 2) / b)
    }
}

fn clamp_i32(v: i64) -> i32 {
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn clamp_pos(v: i64) -> i32 {
    v.clamp(-i64::from(POS_LIMIT), i64::from(POS_LIMIT)) as i32
}

/// The world position of `slot` around `anchor` for `facing`, clamped to
/// ±`POS_LIMIT` per axis (the range every commanded position has, so a
/// derived slot is a valid order target too).
pub fn world_slot(anchor: SimPos, facing: (i64, i64), slot: LocalSlot) -> SimPos {
    let (fx, fz) = facing;
    let (rx, rz) = (-fz, fx);
    let (bx, bz) = (-fx, -fz);
    let lat = i64::from(slot.lateral);
    let dep = i64::from(slot.depth);
    let ox = div_round(rx * lat + bx * dep, FACING_SCALE);
    let oz = div_round(rz * lat + bz * dep, FACING_SCALE);
    SimPos::new(
        clamp_pos(i64::from(anchor.x) + ox),
        clamp_pos(i64::from(anchor.z) + oz),
    )
}

/// The integer centroid of `points` (`None` when empty).
pub fn centroid(points: impl IntoIterator<Item = SimPos>) -> Option<SimPos> {
    let (mut sx, mut sz, mut n) = (0i64, 0i64, 0i64);
    for p in points {
        sx += i64::from(p.x);
        sz += i64::from(p.z);
        n += 1;
    }
    (n > 0).then(|| SimPos::new(clamp_i32(sx.div_euclid(n)), clamp_i32(sz.div_euclid(n))))
}

/// A group order's plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupPlan {
    /// Every unit's slot (grid-validated), in `SimId` order.
    pub slots: Vec<(SimId, SimPos)>,
    /// The unit whose slot is closest to the anchor.
    pub leader: SimId,
    /// The facing used (scaled to [`FACING_SCALE`]).
    pub facing: (i64, i64),
}

impl GroupPlan {
    /// The slot of `unit`.
    pub fn slot_of(&self, unit: SimId) -> Option<SimPos> {
        self.slots
            .binary_search_by_key(&unit, |(id, _)| *id)
            .ok()
            .map(|i| self.slots[i].1)
    }
}

/// Plans a group order to `anchor` for `units` (`(id, from)`, where `from`
/// is where the unit will start: its position, or the end of its queued
/// orders). `grid` re-validates the slots ([`relocate_blocked`],
/// `radius_cells` rings). Deterministic for any input order.
///
/// # Panics
/// When `units` is empty.
pub fn plan_group(
    formation: Formation,
    anchor: SimPos,
    units: &[(SimId, SimPos)],
    grid: Option<&SpatialGrid>,
    radius_cells: u32,
) -> GroupPlan {
    assert!(!units.is_empty(), "a group plan needs units");
    let mut units = units.to_vec();
    units.sort_unstable_by_key(|(id, _)| *id);
    units.dedup_by_key(|(id, _)| *id);
    let center = centroid(units.iter().map(|(_, p)| *p)).expect("non-empty");
    let face = facing(center, anchor);
    let local = local_slots(formation, units.len());
    let mut world: Vec<SimPos> = local.iter().map(|&s| world_slot(anchor, face, s)).collect();
    if let Some(grid) = grid {
        relocate_blocked(grid, anchor, &mut world, radius_cells);
    }

    // Units front to back by their own projection, cut into rows.
    let (fx, fz) = face;
    let (rx, rz) = (-fz, fx);
    let proj = |p: SimPos| {
        let dx = i64::from(p.x) - i64::from(center.x);
        let dz = i64::from(p.z) - i64::from(center.z);
        // depth (behind = larger), lateral (right = larger)
        (-(dx * fx + dz * fz), dx * rx + dz * rz)
    };
    let mut order: Vec<(i64, i64, SimId)> = units
        .iter()
        .map(|&(id, p)| {
            let (d, l) = proj(p);
            (d, l, id)
        })
        .collect();
    order.sort_unstable();
    let mut assigned: Vec<(SimId, SimPos)> = Vec::with_capacity(units.len());
    let mut start = 0;
    while start < local.len() {
        let row = local[start].row;
        let end = local[start..]
            .iter()
            .position(|s| s.row != row)
            .map_or(local.len(), |o| start + o);
        let mut chunk: Vec<(i64, SimId)> = order[start..end]
            .iter()
            .map(|&(_, l, id)| (l, id))
            .collect();
        chunk.sort_unstable();
        // Row slots are generated left to right.
        for (i, (_, id)) in chunk.into_iter().enumerate() {
            assigned.push((id, world[start + i]));
        }
        start = end;
    }
    let leader_slot = local
        .iter()
        .enumerate()
        .min_by_key(|(i, s)| {
            let (l, d) = (i64::from(s.lateral), i64::from(s.depth));
            (l * l + d * d, *i)
        })
        .map_or(0, |(i, _)| i);
    let leader_pos = world[leader_slot];
    let leader = assigned
        .iter()
        .find(|(_, p)| *p == leader_pos)
        .map_or(units[0].0, |(id, _)| *id);
    assigned.sort_unstable_by_key(|(id, _)| *id);
    GroupPlan {
        slots: assigned,
        leader,
        facing: face,
    }
}

fn usable(grid: &SpatialGrid, cell: CellCoord, need_bounds: bool) -> bool {
    !grid.is_blocking(cell) && (!need_bounds || grid.bounds().contains(cell))
}

/// Moves every slot that lies in a blocking cell (or outside the grid
/// bounds while `anchor` is inside them) to the nearest usable cell centre
/// within `radius_cells` rings that no other slot uses. Slots are handled
/// in order; candidates by `(distance² to the slot, x, z)`. A slot with no
/// candidate stays where it is.
pub fn relocate_blocked(
    grid: &SpatialGrid,
    anchor: SimPos,
    slots: &mut [SimPos],
    radius_cells: u32,
) {
    let need_bounds = grid.bounds().contains(grid.cell_of(anchor));
    let mut taken: BTreeSet<CellCoord> = slots
        .iter()
        .map(|&p| grid.cell_of(p))
        .filter(|&c| usable(grid, c, need_bounds))
        .collect();
    let radius = i32::try_from(radius_cells).unwrap_or(i32::MAX);
    for slot in slots.iter_mut() {
        let cell = grid.cell_of(*slot);
        if usable(grid, cell, need_bounds) {
            continue;
        }
        let mut found = None;
        for r in 1..=radius {
            let mut best: Option<(i64, i32, i32)> = None;
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dz.abs() != r {
                        continue;
                    }
                    let c = CellCoord::new(cell.x.saturating_add(dx), cell.z.saturating_add(dz));
                    if taken.contains(&c) || !usable(grid, c, need_bounds) {
                        continue;
                    }
                    let key = (grid.center(c).dist2(*slot), c.x, c.z);
                    if best.is_none_or(|b| key < b) {
                        best = Some(key);
                    }
                }
            }
            if let Some((_, x, z)) = best {
                found = Some(CellCoord::new(x, z));
                break;
            }
        }
        if let Some(c) = found {
            taken.insert(c);
            *slot = grid.center(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_kernel::spatial::{CellFlags, CellInfo, GridBounds};

    const NORTH: (i64, i64) = (0, -FACING_SCALE);

    fn slots(kind: FormationKind, n: usize, anchor: SimPos, face: (i64, i64)) -> Vec<(i32, i32)> {
        local_slots(Formation::new(kind, 2000), n)
            .into_iter()
            .map(|s| {
                let p = world_slot(anchor, face, s);
                (p.x, p.z)
            })
            .collect()
    }

    #[test]
    fn line_slots_are_exact() {
        let a = SimPos::new(10_000, 5_000);
        assert_eq!(
            slots(FormationKind::Line, 4, a, NORTH),
            vec![
                (7_000, 5_000),
                (9_000, 5_000),
                (11_000, 5_000),
                (13_000, 5_000)
            ]
        );
        assert_eq!(
            slots(FormationKind::Line, 3, a, NORTH),
            vec![(8_000, 5_000), (10_000, 5_000), (12_000, 5_000)]
        );
        // Facing east: the line runs north-south, right = south.
        let east = facing(SimPos::new(0, 5_000), a);
        assert_eq!(east, (FACING_SCALE, 0));
        assert_eq!(
            slots(FormationKind::Line, 3, a, east),
            vec![(10_000, 3_000), (10_000, 5_000), (10_000, 7_000)]
        );
    }

    #[test]
    fn box_slots_are_exact() {
        let a = SimPos::new(0, 0);
        // 5 units: 3 columns, 2 rows (3 + 2), rows centred.
        assert_eq!(
            slots(FormationKind::Box, 5, a, NORTH),
            vec![
                (-2_000, -1_000),
                (0, -1_000),
                (2_000, -1_000),
                (-1_000, 1_000),
                (1_000, 1_000)
            ]
        );
        assert_eq!(
            slots(FormationKind::Box, 4, a, NORTH),
            vec![
                (-1_000, -1_000),
                (1_000, -1_000),
                (-1_000, 1_000),
                (1_000, 1_000)
            ]
        );
        let rows: Vec<u32> = local_slots(Formation::new(FormationKind::Box, 2000), 9)
            .iter()
            .map(|s| s.row)
            .collect();
        assert_eq!(rows, vec![0, 0, 0, 1, 1, 1, 2, 2, 2]);
    }

    #[test]
    fn wedge_slots_are_exact() {
        let a = SimPos::new(0, 0);
        assert_eq!(
            slots(FormationKind::Wedge, 4, a, NORTH),
            vec![(0, 0), (-2_000, 2_000), (2_000, 2_000), (-4_000, 4_000)]
        );
        // Facing south flips both axes.
        let south = (0, FACING_SCALE);
        assert_eq!(
            slots(FormationKind::Wedge, 3, a, south),
            vec![(0, 0), (2_000, -2_000), (-2_000, -2_000)]
        );
    }

    #[test]
    fn plan_is_deterministic_and_order_free() {
        let units: Vec<(SimId, SimPos)> = (1..=7)
            .map(|i| {
                (
                    SimId(i),
                    SimPos::new(i as i32 * 700 - 3000, 20_000 + (i as i32 * 311) % 900),
                )
            })
            .collect();
        let mut reversed = units.clone();
        reversed.reverse();
        let f = Formation::new(FormationKind::Box, 1500);
        let a = plan_group(f, SimPos::new(0, 0), &units, None, 8);
        let b = plan_group(f, SimPos::new(0, 0), &reversed, None, 8);
        assert_eq!(a, b);
        let unique: BTreeSet<(i32, i32)> = a.slots.iter().map(|(_, p)| (p.x, p.z)).collect();
        assert_eq!(unique.len(), 7, "one slot per unit");
        // Moving (almost) north.
        assert!(a.facing.1 < -1000, "{:?}", a.facing);
    }

    #[test]
    fn line_assignment_keeps_left_to_right() {
        // Units spread west→east, moving north: west unit gets the west slot.
        let units = [
            (SimId(3), SimPos::new(-4_000, 10_000)),
            (SimId(1), SimPos::new(4_000, 10_000)),
            (SimId(2), SimPos::new(0, 10_000)),
        ];
        let plan = plan_group(
            Formation::new(FormationKind::Line, 2000),
            SimPos::new(0, 0),
            &units,
            None,
            4,
        );
        assert_eq!(plan.slot_of(SimId(3)), Some(SimPos::new(-2_000, 0)));
        assert_eq!(plan.slot_of(SimId(2)), Some(SimPos::new(0, 0)));
        assert_eq!(plan.slot_of(SimId(1)), Some(SimPos::new(2_000, 0)));
        assert_eq!(plan.leader, SimId(2));
    }

    #[test]
    fn blocked_slot_moves_to_nearest_free_cell() {
        let mut grid = SpatialGrid::new(1000, GridBounds::centered(32, 32));
        let wall = CellInfo {
            flags: CellFlags {
                blocking: true,
                ..Default::default()
            },
            extra_cost: 0,
        };
        // Slot (2000, 0) lies in cell (2, 0): block it and its east neighbour.
        grid.set_static(CellCoord::new(2, 0), wall);
        grid.set_static(CellCoord::new(3, 0), wall);
        let mut slots = vec![
            SimPos::new(-2_000, 0),
            SimPos::new(0, 0),
            SimPos::new(2_000, 0),
        ];
        relocate_blocked(&grid, SimPos::new(0, 0), &mut slots, 4);
        // Ring 1 around cell (2,0): the centres of (1,-1), (1,0) and (2,-1)
        // are all 500² + 500² from (2000, 0); ties → smaller x, then z.
        assert_eq!(slots[0], SimPos::new(-2_000, 0));
        assert_eq!(slots[1], SimPos::new(0, 0));
        assert_eq!(slots[2], SimPos::new(1_500, -500), "cell (1,-1) centre");
        // Deterministic: same again.
        let mut again = vec![
            SimPos::new(-2_000, 0),
            SimPos::new(0, 0),
            SimPos::new(2_000, 0),
        ];
        relocate_blocked(&grid, SimPos::new(0, 0), &mut again, 4);
        assert_eq!(slots, again);
    }
}
