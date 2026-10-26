//! Resumable, deterministic grid A* over the dense cells of a
//! [`GridBounds`](npc_kernel::spatial::GridBounds).
//!
//! - 8-connected; step cost 10 orthogonal, 14 diagonal, plus the entered
//!   cell's extra cost. Octile heuristic (admissible: extra costs are ≥ 0).
//! - **No corner cutting:** a diagonal step needs both orthogonal neighbours
//!   it passes to be open (not blocked and not a marked cell such as a door).
//! - The open list is a min-heap on `(f, h, cell index)`: every tie is broken
//!   by a total order, so the expansion order (and the path) never depends
//!   on insertion order, `Entity` order or anything platform-specific.
//! - A search can pause when the node budget runs out and resume next tick
//!   ([`Search::run`]). Scratch arrays are reused across searches with a
//!   generation stamp (no per-request allocation of grid-sized arrays).
//! - A paused search knows what it has looked at ([`Search::touches`]): a
//!   changed cell it never opened and that is not next to a cell it
//!   expanded cannot change its result, so it may resume.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use npc_kernel::hash::Fnv64;
use npc_kernel::spatial::{CellCoord, GridBounds, PathFailReason};
use serde::{Deserialize, Serialize};

/// Orthogonal step cost.
pub const COST_ORTHOGONAL: u32 = 10;
/// Diagonal step cost.
pub const COST_DIAGONAL: u32 = 14;

/// Whether (and at what extra cost) an agent may enter a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Passage {
    /// Walkable; the extra cost is added to the step cost.
    Open(u16),
    /// Walkable at an extra cost, but only through an interaction (e.g.
    /// opening a door); `tag` is the cell tag bit. Becomes a path marker.
    Marked {
        extra: u16,
        tag: u8,
    },
    Blocked,
}

impl Passage {
    fn extra(self) -> Option<u32> {
        match self {
            Passage::Open(c) | Passage::Marked { extra: c, .. } => Some(u32::from(c)),
            Passage::Blocked => None,
        }
    }

    /// Open without interaction (may be a corner of a diagonal step).
    pub fn is_open(self) -> bool {
        matches!(self, Passage::Open(_))
    }
}

/// Reused per-cell arrays (valid where `stamp == generation`).
#[derive(Clone, Debug, Default)]
pub struct Scratch {
    g: Vec<u32>,
    parent: Vec<u32>,
    seen: Vec<u32>,
    closed: Vec<u32>,
    generation: u32,
}

impl Scratch {
    fn begin(&mut self, cells: usize) {
        if self.g.len() != cells {
            self.g = vec![0; cells];
            self.parent = vec![0; cells];
            self.seen = vec![0; cells];
            self.closed = vec![0; cells];
            self.generation = 0;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            // Wrapped: clear the stamps once.
            self.seen.fill(0);
            self.closed.fill(0);
            self.generation = 1;
        }
    }

    fn g(&self, i: u32) -> u32 {
        if self.seen[i as usize] == self.generation {
            self.g[i as usize]
        } else {
            u32::MAX
        }
    }

    fn is_closed(&self, i: u32) -> bool {
        self.closed[i as usize] == self.generation
    }
}

/// The saved form of a paused [`Search`] and the part of its [`Scratch`] it
/// owns (world snapshots, ADR 0013): everything [`Search::run`] and
/// [`Search::touches`] continue from, as [`Search::sim_hash`] hashes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSearch {
    pub start: u32,
    pub goal: u32,
    /// The open heap's entries `(f, h, cell)`, sorted.
    pub open: Vec<(u32, u32, u32)>,
    pub nodes: u32,
    pub started: bool,
    pub generation: u32,
    /// Whether the scratch still held this search's stamps.
    pub owns: bool,
    /// Its opened cells `(index, g, parent, expanded)`, ascending (when `owns`).
    pub cells: Vec<(u32, u32, u32, bool)>,
}

impl Search {
    /// The search and its scratch stamps, for a snapshot.
    pub fn save(&self, scratch: &Scratch) -> SavedSearch {
        let mut open: Vec<(u32, u32, u32)> = self.open.iter().map(|r| r.0).collect();
        open.sort_unstable();
        let owns = self.owns(scratch);
        let g = self.generation;
        let cells = if owns {
            (0..scratch.seen.len())
                .filter(|&i| scratch.seen[i] == g)
                .map(|i| {
                    (
                        i as u32,
                        scratch.g[i],
                        scratch.parent[i],
                        scratch.closed[i] == g,
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        SavedSearch {
            start: self.start,
            goal: self.goal,
            open,
            nodes: self.nodes,
            started: self.started,
            generation: self.generation,
            owns,
            cells,
        }
    }

    /// Restores a saved search into `scratch` (already restored with
    /// [`Scratch::restore`]): its stamps go back exactly when it owned it.
    ///
    /// # Errors
    /// A cell index outside the scratch, or a scratch at another generation
    /// for a search that owned it.
    pub fn load(saved: SavedSearch, scratch: &mut Scratch) -> Result<Self, String> {
        let len = scratch.g.len();
        let check = |i: u32| {
            if (i as usize) < len || !saved.owns {
                Ok(())
            } else {
                Err(format!("nav_queue: cell {i} outside {len} cells"))
            }
        };
        if saved.owns {
            if len == 0 || scratch.generation != saved.generation {
                return Err("nav_queue: the search owned a scratch that was not saved".into());
            }
            for (i, g, parent, expanded) in saved.cells {
                check(i)?;
                check(parent)?;
                let n = i as usize;
                scratch.seen[n] = saved.generation;
                scratch.g[n] = g;
                scratch.parent[n] = parent;
                if expanded {
                    scratch.closed[n] = saved.generation;
                }
            }
            for &(_, _, cell) in &saved.open {
                check(cell)?;
            }
            check(saved.start)?;
            check(saved.goal)?;
        }
        Ok(Self {
            start: saved.start,
            goal: saved.goal,
            open: saved.open.into_iter().map(Reverse).collect(),
            nodes: saved.nodes,
            started: saved.started,
            generation: saved.generation,
        })
    }
}

impl Scratch {
    /// Its array length and stamp generation (what a snapshot keeps; the
    /// stamps belong to the search that owns them, [`Search::save`]).
    pub fn shape(&self) -> (usize, u32) {
        (self.g.len(), self.generation)
    }

    /// A scratch of `len` cells at `generation` without stamps (stamps of
    /// older generations never match a later one, so dropping them changes
    /// nothing).
    pub fn restore(len: usize, generation: u32) -> Self {
        Self {
            g: vec![0; len],
            parent: vec![0; len],
            seen: vec![0; len],
            closed: vec![0; len],
            generation,
        }
    }
}

/// How a [`Search::run`] call ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Cells from start to goal (dense indices, both included).
    Found(Vec<u32>),
    Failed(PathFailReason),
    /// The budget ran out; call `run` again later.
    Paused,
}

/// One A* search from `start` to `goal` (dense indices).
#[derive(Clone, Debug)]
pub struct Search {
    pub start: u32,
    pub goal: u32,
    open: BinaryHeap<Reverse<(u32, u32, u32)>>,
    /// Cells expanded so far (over every `run`).
    pub nodes: u32,
    started: bool,
    /// The scratch generation this search's stamps use (set when it starts).
    generation: u32,
}

/// Octile distance between two cells, in cost units.
pub fn octile(bounds: GridBounds, a: u32, b: u32) -> u32 {
    octile_c(bounds.coord(a), bounds.coord(b))
}

fn octile_c(ca: npc_kernel::spatial::CellCoord, cb: npc_kernel::spatial::CellCoord) -> u32 {
    let dx = ca.x.abs_diff(cb.x);
    let dz = ca.z.abs_diff(cb.z);
    let (lo, hi) = if dx < dz { (dx, dz) } else { (dz, dx) };
    COST_DIAGONAL * lo + COST_ORTHOGONAL * (hi - lo)
}

/// Neighbour offsets in a fixed order (orthogonal first).
const NEIGHBOURS: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

impl Search {
    pub fn new(start: u32, goal: u32) -> Self {
        Self {
            start,
            goal,
            open: BinaryHeap::new(),
            nodes: 0,
            started: false,
            generation: 0,
        }
    }

    /// Whether `scratch` still holds this search's stamps (no other search
    /// began on it since).
    fn owns(&self, scratch: &Scratch) -> bool {
        self.started && scratch.generation == self.generation && !scratch.seen.is_empty()
    }

    /// Whether a change of `cell` can change this (paused) search's result:
    /// the search opened `cell`, or expanded one of its 8 neighbours (an
    /// expansion looks at every neighbour, including the corners of its
    /// diagonal steps). A search that has not started touches nothing; one
    /// whose scratch was reused by another search touches everything.
    pub fn touches(&self, scratch: &Scratch, bounds: GridBounds, cell: CellCoord) -> bool {
        if !self.started {
            return false;
        }
        if !self.owns(scratch) {
            return true;
        }
        if let Some(i) = bounds.index(cell)
            && scratch.seen[i as usize] == self.generation
        {
            return true;
        }
        NEIGHBOURS.iter().any(|&(dx, dz)| {
            bounds
                .index(CellCoord::new(cell.x + dx, cell.z + dz))
                .is_some_and(|n| scratch.closed[n as usize] == self.generation)
        })
    }

    /// Hashes the search's whole progress (everything [`Search::run`]
    /// continues from): start, goal, nodes and, per opened cell in index
    /// order, its cost, parent and whether it is expanded. The open heap is
    /// implied: it holds the opened, unexpanded cells (stale duplicates
    /// never change the result).
    pub fn sim_hash(&self, scratch: &Scratch, h: &mut Fnv64) {
        h.u32(self.start);
        h.u32(self.goal);
        h.u32(self.nodes);
        h.u8(u8::from(self.started));
        if !self.started {
            return;
        }
        if !self.owns(scratch) {
            h.u8(0);
            return;
        }
        h.u8(1);
        let g = self.generation;
        let opened: Vec<usize> = (0..scratch.seen.len())
            .filter(|&i| scratch.seen[i] == g)
            .collect();
        h.u32(u32::try_from(opened.len()).expect("grid too large"));
        for i in opened {
            h.u32(i as u32);
            h.u32(scratch.g[i]);
            h.u32(scratch.parent[i]);
            h.u8(u8::from(scratch.closed[i] == g));
        }
    }

    /// Expands at most `*budget` cells (decrementing it). `passage(i)` says
    /// whether cell `i` may be entered; it is called for the start and goal
    /// first (a blocked start/goal fails at once without expanding).
    pub fn run(
        &mut self,
        scratch: &mut Scratch,
        bounds: GridBounds,
        budget: &mut u32,
        passage: &mut dyn FnMut(u32) -> Passage,
    ) -> Step {
        if !self.started {
            if passage(self.start) == Passage::Blocked {
                return Step::Failed(PathFailReason::StartBlocked);
            }
            if passage(self.goal) == Passage::Blocked {
                return Step::Failed(PathFailReason::GoalBlocked);
            }
            scratch.begin(bounds.len());
            self.generation = scratch.generation;
            let s = self.start as usize;
            scratch.seen[s] = scratch.generation;
            scratch.g[s] = 0;
            scratch.parent[s] = self.start;
            let h = octile_c(bounds.coord(self.start), bounds.coord(self.goal));
            self.open.push(Reverse((h, h, self.start)));
            self.started = true;
        }
        let goal_c = bounds.coord(self.goal);
        while *budget > 0 {
            let Some(Reverse((_, _, cell))) = self.open.pop() else {
                return Step::Failed(PathFailReason::Unreachable);
            };
            if scratch.is_closed(cell) {
                continue;
            }
            scratch.closed[cell as usize] = scratch.generation;
            self.nodes += 1;
            *budget -= 1;
            if cell == self.goal {
                return Step::Found(self.reconstruct(scratch));
            }
            let here = bounds.coord(cell);
            let g_here = scratch.g(cell);
            for (dx, dz) in NEIGHBOURS {
                let next_c = npc_kernel::spatial::CellCoord::new(here.x + dx, here.z + dz);
                let Some(next) = bounds.index(next_c) else {
                    continue;
                };
                if scratch.is_closed(next) {
                    continue;
                }
                let Some(extra) = passage(next).extra() else {
                    continue;
                };
                let diagonal = dx != 0 && dz != 0;
                if diagonal {
                    let side_a =
                        bounds.index(npc_kernel::spatial::CellCoord::new(here.x + dx, here.z));
                    let side_b =
                        bounds.index(npc_kernel::spatial::CellCoord::new(here.x, here.z + dz));
                    let open = |s: Option<u32>, p: &mut dyn FnMut(u32) -> Passage| {
                        s.is_some_and(|i| p(i).is_open())
                    };
                    if !open(side_a, passage) || !open(side_b, passage) {
                        continue;
                    }
                }
                let step = if diagonal {
                    COST_DIAGONAL
                } else {
                    COST_ORTHOGONAL
                };
                let g = g_here.saturating_add(step + extra);
                if g < scratch.g(next) {
                    let n = next as usize;
                    scratch.seen[n] = scratch.generation;
                    scratch.g[n] = g;
                    scratch.parent[n] = cell;
                    let h = octile_c(next_c, goal_c);
                    self.open.push(Reverse((g.saturating_add(h), h, next)));
                }
            }
        }
        Step::Paused
    }

    fn reconstruct(&self, scratch: &Scratch) -> Vec<u32> {
        let mut cells = vec![self.goal];
        let mut at = self.goal;
        while at != self.start {
            at = scratch.parent[at as usize];
            cells.push(at);
        }
        cells.reverse();
        cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_kernel::spatial::CellCoord;

    fn run(bounds: GridBounds, walls: &[CellCoord], s: CellCoord, g: CellCoord) -> Step {
        let blocked: Vec<u32> = walls.iter().filter_map(|&c| bounds.index(c)).collect();
        let mut search = Search::new(bounds.index(s).unwrap(), bounds.index(g).unwrap());
        let mut budget = u32::MAX;
        search.run(&mut Scratch::default(), bounds, &mut budget, &mut |i| {
            if blocked.contains(&i) {
                Passage::Blocked
            } else {
                Passage::Open(0)
            }
        })
    }

    #[test]
    fn diagonal_corner_cutting_is_prevented() {
        let b = GridBounds {
            min: CellCoord::new(0, 0),
            width: 3,
            height: 3,
        };
        // (1,0) blocked: (0,0) -> (1,1) may not cut the corner diagonally.
        let Step::Found(cells) = run(
            b,
            &[CellCoord::new(1, 0)],
            CellCoord::new(0, 0),
            CellCoord::new(1, 1),
        ) else {
            panic!("path expected");
        };
        let coords: Vec<CellCoord> = cells.iter().map(|&i| b.coord(i)).collect();
        assert_eq!(
            coords,
            vec![
                CellCoord::new(0, 0),
                CellCoord::new(0, 1),
                CellCoord::new(1, 1)
            ]
        );
        // Both sides blocked: no diagonal squeeze between them.
        let walls = [CellCoord::new(1, 0), CellCoord::new(0, 1)];
        assert_eq!(
            run(b, &walls, CellCoord::new(0, 0), CellCoord::new(1, 1)),
            Step::Failed(PathFailReason::Unreachable)
        );
    }

    #[test]
    fn pauses_and_resumes_to_the_same_path() {
        let b = GridBounds::centered(32, 32);
        let walls: Vec<CellCoord> = (-10..10).map(|z| CellCoord::new(0, z)).collect();
        let (s, g) = (CellCoord::new(-8, 0), CellCoord::new(8, 3));
        let whole = run(b, &walls, s, g);
        let blocked: Vec<u32> = walls.iter().filter_map(|&c| b.index(c)).collect();
        let mut search = Search::new(b.index(s).unwrap(), b.index(g).unwrap());
        let mut scratch = Scratch::default();
        let mut pauses = 0;
        let result = loop {
            let mut budget = 7;
            match search.run(&mut scratch, b, &mut budget, &mut |i| {
                if blocked.contains(&i) {
                    Passage::Blocked
                } else {
                    Passage::Open(0)
                }
            }) {
                Step::Paused => pauses += 1,
                done => break done,
            }
        };
        assert!(pauses > 3);
        assert_eq!(result, whole);
    }
}
