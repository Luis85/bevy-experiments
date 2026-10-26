//! The `walls` observation section: the wall runs and openings (doors,
//! gates, gaps) around a unit, so an external agent can find its way out of
//! a walled place instead of finding a gate only when a long Move happens
//! to path through it (`docs/_notes/explorer-e2e.md`, finding P3).
//!
//! ```text
//! "walls": {
//!   "range_mm": 15000,
//!   "walls":    [{"id":3,"from":[x,z],"to":[x,z],"runs":"east-west","length_mm":12000,
//!                 "nearest":[x,z],"dist_mm":3000,"dir":"south"}],
//!   "openings": [{"kind":"door","name":"gate","door":12,"state":"closed","at":[x,z],
//!                 "width_mm":1000,"in_wall":"east-west","dist_mm":3041,"dir":"south"}]
//! }
//! ```
//!
//! Text: `Walls within 15.00 m: an east-west wall from (..) to (..), 12 m
//! long, nearest 3.00 m south; ..` and `Openings within 15.00 m: the gate
//! (closed door #12) at (0.50 m, -9.50 m) in an east-west wall, 3.04 m
//! south; ..`.
//!
//! **Geometry, all integer.** A wall run is a `Prop { Wall }` entity's
//! `GridBlocker` footprint (one straight run of cells, see
//! [`crate::layout`]) or a straight run of blocking cells in the
//! `SpatialGrid` static layer (rows first, then the cells left over as
//! columns; such runs have no `id`). An **opening** is a gap of at most
//! [`MAX_GAP_CELLS`] cells between two runs on the same row (or column)
//! that no other run covers; a `Prop { Doorway }` in the gap makes it a
//! `door` (its `Door` state, and the testbed name, e.g. `gate`), else it is
//! a `gap`. A doorway outside every gap is an opening of its own
//! (`in_wall: null`).
//!
//! **What is listed.** Runs whose nearest point, and openings whose centre,
//! lies within the unit's vision range (its largest `Sensors` vision;
//! without one there is no section). It is a sense of the layout, not sight:
//! no field of view or line of sight, so a gate behind the unit or a wall
//! behind another wall is listed too. Sorted by distance, then position.
//! Read-only: it changes no state and no hash.

use std::collections::{BTreeMap, BTreeSet};

use bevy_ecs::prelude::*;
use npc_framework::agent_io::observe::metres;
use npc_framework::agent_io::{AgentCtx, Section, observe};
use npc_framework::prelude::*;
use serde_json::json;

use crate::world::{Prop, PropKind, TestbedIds};

/// Name of the section.
pub const WALLS_SECTION: &str = "walls";

/// Widest gap between two runs on one line that counts as an opening, in
/// cells (wider is open ground).
pub const MAX_GAP_CELLS: i32 = 4;

/// Lines listing at most this many things each (the JSON has them all).
const TEXT_LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Axis {
    /// Along x (one row).
    EastWest,
    /// Along z (one column).
    NorthSouth,
}

impl Axis {
    fn name(self) -> &'static str {
        match self {
            Axis::EastWest => "east-west",
            Axis::NorthSouth => "north-south",
        }
    }

    /// `"an east-west"` / `"a north-south"`.
    fn with_article(self) -> &'static str {
        match self {
            Axis::EastWest => "an east-west",
            Axis::NorthSouth => "a north-south",
        }
    }
}

/// A run of wall cells `lo..=hi` (a rectangle; straight runs are one cell
/// thick).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Run {
    lo: CellCoord,
    hi: CellCoord,
    id: Option<SimId>,
}

impl Run {
    fn covers(&self, c: CellCoord) -> bool {
        (self.lo.x..=self.hi.x).contains(&c.x) && (self.lo.z..=self.hi.z).contains(&c.z)
    }

    /// The axes it runs along (a single cell runs along both).
    fn along(&self, axis: Axis) -> bool {
        match axis {
            Axis::EastWest => self.lo.z == self.hi.z,
            Axis::NorthSouth => self.lo.x == self.hi.x,
        }
    }

    /// The axis it is listed with: the longer one (east-west for a cell).
    fn axis(&self) -> Axis {
        if self.hi.z - self.lo.z > self.hi.x - self.lo.x {
            Axis::NorthSouth
        } else {
            Axis::EastWest
        }
    }
}

/// An opening: gap cells `lo..=hi` on a line of `axis`, or a doorway's cell
/// outside every gap (`axis: None`).
#[derive(Clone, Copy, Debug)]
struct Opening {
    lo: CellCoord,
    hi: CellCoord,
    axis: Option<Axis>,
    door: Option<(SimId, Entity)>,
}

/// Every wall run: `Prop { Wall }` footprints (by `SimId`), then the static
/// layer's blocking cells as runs.
fn runs(world: &World, grid: &SpatialGrid) -> Vec<Run> {
    let mut out = Vec::new();
    for (&id, &e) in &world.resource::<SimIndex>().map {
        let (Some(p), Some(b), Some(&pos)) = (
            world.get::<Prop>(e),
            world.get::<GridBlocker>(e),
            world.get::<SimPos>(e),
        ) else {
            continue;
        };
        if p.kind == PropKind::Wall {
            let (lo, hi) = grid.footprint_cell_rect(pos, b.0);
            out.push(Run {
                lo,
                hi,
                id: Some(id),
            });
        }
    }
    let bounds = grid.bounds();
    let statics: BTreeSet<CellCoord> = grid
        .static_cells()
        .filter(|(_, info)| info.flags.blocking)
        .map(|(i, _)| bounds.coord(i))
        .filter(|&c| !out.iter().any(|r| r.covers(c)))
        .collect();
    out.extend(static_runs(&statics));
    out
}

/// `cells` as straight runs: maximal rows of two or more cells, then the
/// rest as maximal columns.
fn static_runs(cells: &BTreeSet<CellCoord>) -> Vec<Run> {
    let rows: BTreeSet<(i32, i32)> = cells.iter().map(|c| (c.z, c.x)).collect();
    let mut out = Vec::new();
    let mut rest = BTreeSet::new();
    for (z, x0, x1) in spans(&rows) {
        if x1 > x0 {
            out.push(Run {
                lo: CellCoord::new(x0, z),
                hi: CellCoord::new(x1, z),
                id: None,
            });
        } else {
            rest.insert((x0, z));
        }
    }
    for (x, z0, z1) in spans(&rest) {
        out.push(Run {
            lo: CellCoord::new(x, z0),
            hi: CellCoord::new(x, z1),
            id: None,
        });
    }
    out
}

/// The maximal spans `(line, start, end)` of consecutive `(line, v)` keys.
fn spans(keys: &BTreeSet<(i32, i32)>) -> Vec<(i32, i32, i32)> {
    let mut out: Vec<(i32, i32, i32)> = Vec::new();
    for &(line, v) in keys {
        match out.last_mut() {
            Some(s) if s.0 == line && s.2 + 1 == v => s.2 = v,
            _ => out.push((line, v, v)),
        }
    }
    out
}

/// The gaps of at most [`MAX_GAP_CELLS`] between consecutive runs on one
/// line that no run covers.
fn gaps(runs: &[Run]) -> Vec<Opening> {
    let mut out = Vec::new();
    for axis in [Axis::EastWest, Axis::NorthSouth] {
        // (line, start, end) along the axis.
        let mut on_line: Vec<(i32, i32, i32)> = runs
            .iter()
            .filter(|r| r.along(axis))
            .map(|r| match axis {
                Axis::EastWest => (r.lo.z, r.lo.x, r.hi.x),
                Axis::NorthSouth => (r.lo.x, r.lo.z, r.hi.z),
            })
            .collect();
        on_line.sort_unstable();
        // The furthest end so far on the current line (runs may overlap).
        let mut reach: Option<(i32, i32)> = None;
        for &(line, b_start, b_end) in &on_line {
            let prev = reach.filter(|r| r.0 == line).map(|r| r.1);
            reach = Some((line, prev.map_or(b_end, |e| e.max(b_end))));
            let Some(a_end) = prev else { continue };
            let width = b_start - a_end - 1;
            if !(1..=MAX_GAP_CELLS).contains(&width) {
                continue;
            }
            let cell = |along: i32| match axis {
                Axis::EastWest => CellCoord::new(along, line),
                Axis::NorthSouth => CellCoord::new(line, along),
            };
            let (lo, hi) = (cell(a_end + 1), cell(b_start - 1));
            let covered = (a_end + 1..b_start).any(|v| runs.iter().any(|r| r.covers(cell(v))));
            if !covered {
                out.push(Opening {
                    lo,
                    hi,
                    axis: Some(axis),
                    door: None,
                });
            }
        }
    }
    out
}

/// The doorways and their cells, in `SimId` order.
fn doorways(world: &World, grid: &SpatialGrid) -> Vec<(CellCoord, (SimId, Entity))> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(&id, &e)| {
            let p = world.get::<Prop>(e)?;
            let pos = world.get::<SimPos>(e)?;
            (p.kind == PropKind::Doorway).then(|| (grid.cell_of(*pos), (id, e)))
        })
        .collect()
}

/// The area (mm) of `r`'s cells, `(min, max)` inclusive.
fn area(r: &Run, cell_mm: i64) -> (SimPos, SimPos) {
    let mm = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    let lo = |v: i32| mm(i64::from(v) * cell_mm);
    let hi = |v: i32| mm((i64::from(v) + 1) * cell_mm - 1);
    (
        SimPos::new(lo(r.lo.x), lo(r.lo.z)),
        SimPos::new(hi(r.hi.x), hi(r.hi.z)),
    )
}

fn dist_mm(a: SimPos, b: SimPos) -> i64 {
    // `dist2` is a non-negative i64.
    i64::try_from(u64::try_from(a.dist2(b)).unwrap_or(0).isqrt()).unwrap_or(i64::MAX)
}

/// The 8-way compass direction from `from` to `to` (`"here"` when equal),
/// integer only: a component under tan 22.5° (≈ 29/70) of the other is
/// dropped.
fn compass(from: SimPos, to: SimPos) -> &'static str {
    let dx = i64::from(to.x) - i64::from(from.x);
    let dz = i64::from(to.z) - i64::from(from.z);
    let (ax, az) = (dx.abs(), dz.abs());
    let ns = if 70 * az > 29 * ax { dz.signum() } else { 0 };
    let ew = if 70 * ax > 29 * az { dx.signum() } else { 0 };
    match (ns, ew) {
        (1, 0) => "north",
        (1, 1) => "north-east",
        (0, 1) => "east",
        (-1, 1) => "south-east",
        (-1, 0) => "south",
        (-1, -1) => "south-west",
        (0, -1) => "west",
        (1, -1) => "north-west",
        _ => "here",
    }
}

fn at_text(p: SimPos) -> String {
    format!(
        "({} m, {} m)",
        metres(i64::from(p.x)),
        metres(i64::from(p.z))
    )
}

/// Whole metres when exact, else two decimals.
fn length_text(mm: i64) -> String {
    if mm % 1000 == 0 {
        format!("{} m", mm / 1000)
    } else {
        format!("{} m", metres(mm))
    }
}

fn mid(a: SimPos, b: SimPos) -> SimPos {
    SimPos::new(a.x.midpoint(b.x), a.z.midpoint(b.z))
}

fn list(items: &[String]) -> String {
    let mut s = items
        .iter()
        .take(TEXT_LIMIT)
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    if items.len() > TEXT_LIMIT {
        s.push_str(&format!("; and {} more", items.len() - TEXT_LIMIT));
    }
    s
}

/// The `walls` section of `ctx.unit` (see the module docs); `None` without
/// a position or a vision sensor.
pub fn walls_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let here = *world.get::<SimPos>(ctx.entity)?;
    let range = world
        .get::<Sensors>(ctx.entity)?
        .visions()
        .map(|v| v.range_mm)
        .max()?;
    let range_i = i64::from(range);
    let grid = world.resource::<SpatialGrid>();
    let cell_mm = i64::from(grid.cell_mm());
    let runs = runs(world, grid);

    // Wall runs within range of their nearest point.
    let mut walls: Vec<(i64, Run, SimPos)> = runs
        .iter()
        .filter_map(|r| {
            let (min, max) = area(r, cell_mm);
            let nearest = SimPos::new(here.x.clamp(min.x, max.x), here.z.clamp(min.z, max.z));
            let d = dist_mm(here, nearest);
            (d <= range_i).then_some((d, *r, nearest))
        })
        .collect();
    walls.sort_by_key(|&(d, r, _)| (d, r.lo, r.hi, r.id));

    // Openings: gaps (with the doorway in them), then lone doorways.
    let mut openings = gaps(&runs);
    for (c, door) in doorways(world, grid) {
        match openings
            .iter_mut()
            .find(|o| (o.lo.x..=o.hi.x).contains(&c.x) && (o.lo.z..=o.hi.z).contains(&c.z))
        {
            Some(o) if o.door.is_none() => o.door = Some(door),
            Some(_) => {}
            None => openings.push(Opening {
                lo: c,
                hi: c,
                axis: None,
                door: Some(door),
            }),
        }
    }
    let mut openings: Vec<(i64, SimPos, Opening)> = openings
        .into_iter()
        .filter_map(|o| {
            let at = mid(grid.center(o.lo), grid.center(o.hi));
            let d = dist_mm(here, at);
            (d <= range_i).then_some((d, at, o))
        })
        .collect();
    openings.sort_by_key(|&(d, at, o)| (d, at.x, at.z, o.lo, o.hi));

    let names: BTreeMap<SimId, &str> = world
        .get_resource::<TestbedIds>()
        .map(|ids| ids.names.iter().map(|(n, &id)| (id, n.as_str())).collect())
        .unwrap_or_default();

    let mut walls_json = Vec::new();
    let mut walls_text = Vec::new();
    for (d, r, nearest) in &walls {
        let axis = r.axis();
        let cells = match axis {
            Axis::EastWest => r.hi.x - r.lo.x + 1,
            Axis::NorthSouth => r.hi.z - r.lo.z + 1,
        };
        let length = i64::from(cells) * cell_mm;
        let (from, to) = (grid.center(r.lo), grid.center(r.hi));
        let dir = compass(here, *nearest);
        walls_json.push(json!({
            "id": r.id, "from": [from.x, from.z], "to": [to.x, to.z], "runs": axis.name(),
            "length_mm": length, "nearest": [nearest.x, nearest.z], "dist_mm": d, "dir": dir,
        }));
        walls_text.push(format!(
            "{} wall from {} to {}, {} long, nearest {} m {dir}",
            axis.with_article(),
            at_text(from),
            at_text(to),
            length_text(length),
            metres(*d),
        ));
    }

    let mut openings_json = Vec::new();
    let mut openings_text = Vec::new();
    for (d, at, o) in &openings {
        let cells = (o.hi.x - o.lo.x).max(o.hi.z - o.lo.z) + 1;
        let width = i64::from(cells) * cell_mm;
        let dir = compass(here, *at);
        let door = o.door.map(|(id, e)| {
            let state = world.get::<Door>(e).map(|d| match (d.open, d.is_locked()) {
                (true, _) => "open",
                (false, true) => "locked",
                (false, false) => "closed",
            });
            (id, state)
        });
        let name = door.and_then(|(id, _)| names.get(&id).copied());
        let in_wall = o.axis.map(Axis::name);
        openings_json.push(json!({
            "kind": if door.is_some() { "door" } else { "gap" },
            "name": name,
            "door": door.map(|(id, _)| id),
            "state": door.and_then(|(_, s)| s),
            "at": [at.x, at.z], "width_mm": width, "in_wall": in_wall, "dist_mm": d, "dir": dir,
        }));
        let what = match (door, name) {
            (Some((id, _)), Some(n)) => {
                format!(
                    "the {} ({})",
                    n.replace('_', " "),
                    observe::describe(world, id)
                )
            }
            (Some((id, _)), None) => observe::describe(world, id),
            (None, _) => format!("a {} gap", length_text(width)),
        };
        let wall = o
            .axis
            .map_or(String::new(), |a| format!(" in {} wall", a.with_article()));
        openings_text.push(format!(
            "{what} at {}{wall}, {} m {dir}",
            at_text(*at),
            metres(*d)
        ));
    }

    let within = metres(range_i);
    let mut section = Section::new(json!({
        "range_mm": range,
        "walls": walls_json,
        "openings": openings_json,
    }));
    section = section.line(if walls_text.is_empty() {
        format!("No walls within {within} m.")
    } else {
        format!("Walls within {within} m: {}.", list(&walls_text))
    });
    if !openings_text.is_empty() {
        section = section.line(format!(
            "Openings within {within} m: {}.",
            list(&openings_text)
        ));
    }
    Some(section)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(x: i32, z: i32) -> CellCoord {
        CellCoord::new(x, z)
    }

    #[test]
    fn compass_is_eight_way() {
        let o = SimPos::new(0, 0);
        assert_eq!(compass(o, SimPos::new(0, 5)), "north");
        assert_eq!(compass(o, SimPos::new(1, 5)), "north");
        assert_eq!(compass(o, SimPos::new(5, 5)), "north-east");
        assert_eq!(compass(o, SimPos::new(5, -1)), "east");
        assert_eq!(compass(o, SimPos::new(-3, -3)), "south-west");
        assert_eq!(compass(o, o), "here");
    }

    #[test]
    fn gaps_between_collinear_runs() {
        let run = |lo, hi| Run { lo, hi, id: None };
        let runs = [
            run(c(-5, 0), c(-1, 0)),
            run(c(1, 0), c(5, 0)),
            // Too wide a gap: open ground.
            run(c(11, 0), c(12, 0)),
            // A column with a two-cell gap.
            run(c(-5, 1), c(-5, 3)),
            run(c(-5, 6), c(-5, 9)),
        ];
        let g = gaps(&runs);
        let cells: Vec<_> = g.iter().map(|o| (o.lo, o.hi, o.axis)).collect();
        assert_eq!(
            cells,
            vec![
                (c(0, 0), c(0, 0), Some(Axis::EastWest)),
                (c(-5, 4), c(-5, 5), Some(Axis::NorthSouth)),
            ]
        );
    }

    #[test]
    fn a_covered_gap_is_no_opening() {
        let run = |lo, hi| Run { lo, hi, id: None };
        let runs = [
            run(c(-3, 0), c(-1, 0)),
            run(c(1, 0), c(3, 0)),
            // A column crossing the gap.
            run(c(0, -2), c(0, 2)),
        ];
        assert!(gaps(&runs).iter().all(|o| o.axis != Some(Axis::EastWest)));
    }

    #[test]
    fn a_run_inside_another_hides_no_gap() {
        let run = |lo, hi| Run { lo, hi, id: None };
        let runs = [
            run(c(0, 0), c(10, 0)),
            run(c(2, 0), c(3, 0)),
            run(c(12, 0), c(14, 0)),
        ];
        let g: Vec<_> = gaps(&runs).iter().map(|o| (o.lo, o.hi)).collect();
        assert_eq!(g, vec![(c(11, 0), c(11, 0))]);
    }

    #[test]
    fn static_cells_become_rows_then_columns() {
        let cells = BTreeSet::from([c(0, 0), c(1, 0), c(2, 0), c(5, 3), c(5, 4), c(9, 9)]);
        let runs: Vec<_> = static_runs(&cells).iter().map(|r| (r.lo, r.hi)).collect();
        assert_eq!(
            runs,
            vec![(c(0, 0), c(2, 0)), (c(5, 3), c(5, 4)), (c(9, 9), c(9, 9)),]
        );
    }
}
