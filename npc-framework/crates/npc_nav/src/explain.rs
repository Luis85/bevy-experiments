//! Why a walk is blocked (agent feedback, proposal 001): the reachable cell
//! closest to the goal and what is in the way.
//!
//! [`explain_blocked`] is **read-only tooling**: it floods the grid with
//! its own arrays (no [`NavQueue`](crate::NavQueue) scratch, no
//! [`NavBudget`](crate::NavBudget)), so calling it never changes the sim. It
//! judges cells like the planner ([`passage_for`]: pass-cost hooks, the
//! agent's own blocker). 8-connected, no corner cutting, every tie broken by
//! cell index: the result is deterministic. Its work is capped at
//! [`EXPLAIN_CELL_BUDGET`] flooded cells (past it: no report).
//!
//! 1. **The planner's rules**: a marked cell (a door the agent may open on
//!    the way) is passable. If the goal is unreachable so, this pass gives
//!    the nearest reachable cell and the blocker (below).
//! 2. **Strict**, only when the goal is reachable under the planner's rules
//!    (the walk stalled for another reason, e.g. at a door that did not
//!    open): a marked cell is passable only while it is not blocking, so a
//!    walk that gave up in front of a closed door names that door. Reachable
//!    even so: no report (nothing on the grid explains the failure).
//!
//! The blocker, in either pass:
//! - the goal outside the grid: [`Blocker::OutOfBounds`], nearest to the
//!   goal's cell clamped into the grid;
//! - the goal's cell impassable: that cell (a door by its
//!   [`CellBlockerFn`](crate::CellBlockerFn), else a wall);
//! - otherwise the impassable cells between the agent's region and the
//!   goal's: a door first, then the one closest to the nearest reachable
//!   cell. Regions apart by more than one cell: the impassable cell next to
//!   the agent's region closest to the goal.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bevy_ecs::prelude::*;
use npc_kernel::SimPos;
use npc_kernel::spatial::{Blocker, CellCoord, GridBounds, SpatialGrid};

use crate::astar::{Passage, octile};
use crate::plan::{own_cells, passage_for};
use crate::{CellBlockers, PassCosts};

/// Most cells one [`explain_blocked`] call floods over all its passes
/// (`2^18`: four 256 × 256 grids; a report needs at most two grids' worth).
/// Past it the call gives up and returns `None`.
pub const EXPLAIN_CELL_BUDGET: usize = 1 << 18;

/// What [`explain_blocked`] found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockedReport {
    /// The centre of the reachable cell closest to the goal (octile
    /// distance, then cell index).
    pub nearest_reachable: Option<SimPos>,
    pub blocker: Option<Blocker>,
}

/// Neighbour offsets (orthogonal first, as in A*).
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

struct Judge<'w> {
    world: &'w World,
    grid: &'w SpatialGrid,
    costs: &'w PassCosts,
    agent: Entity,
    own: Vec<u32>,
    cache: BTreeMap<u32, Passage>,
    /// Pass 2: a marked cell is passable only while it is not blocking.
    strict: bool,
    /// Cells left to flood.
    budget: usize,
}

impl Judge<'_> {
    fn open(&mut self, i: u32) -> bool {
        let (world, grid, costs, agent) = (self.world, self.grid, self.costs, self.agent);
        match passage_for(world, grid, costs, agent, &self.own, i, &mut self.cache) {
            Passage::Open(_) => true,
            Passage::Marked { .. } if !self.strict => true,
            Passage::Marked { .. } => {
                let info = if self.own.binary_search(&i).is_ok() {
                    grid.cell_without_one_blocker(i)
                } else {
                    grid.cell_at(i)
                };
                !info.flags.blocking
            }
            Passage::Blocked => false,
        }
    }

    /// The cells reachable from `from` (which counts as reached); `None`
    /// once the budget is spent.
    fn flood(&mut self, bounds: GridBounds, from: u32) -> Option<Vec<bool>> {
        let mut reached = vec![false; bounds.len()];
        reached[from as usize] = true;
        self.budget = self.budget.checked_sub(1)?;
        let mut queue = VecDeque::from([from]);
        while let Some(i) = queue.pop_front() {
            for d in NEIGHBOURS {
                let Some(n) = step(bounds, i, d) else {
                    continue;
                };
                if reached[n as usize] || !self.open(n) {
                    continue;
                }
                if d.0 != 0 && d.1 != 0 {
                    let a = step(bounds, i, (d.0, 0));
                    let b = step(bounds, i, (0, d.1));
                    if !a.is_some_and(|a| self.open(a)) || !b.is_some_and(|b| self.open(b)) {
                        continue;
                    }
                }
                self.budget = self.budget.checked_sub(1)?;
                reached[n as usize] = true;
                queue.push_back(n);
            }
        }
        Some(reached)
    }
}

fn step(bounds: GridBounds, i: u32, (dx, dz): (i32, i32)) -> Option<u32> {
    let c = bounds.coord(i);
    bounds.index(CellCoord::new(c.x + dx, c.z + dz))
}

fn clamp(bounds: GridBounds, c: CellCoord) -> CellCoord {
    let max_x = bounds.min.x + bounds.width as i32 - 1;
    let max_z = bounds.min.z + bounds.height as i32 - 1;
    CellCoord::new(
        c.x.clamp(bounds.min.x, max_x),
        c.z.clamp(bounds.min.z, max_z),
    )
}

/// A door (by the cell's [`CellBlockerFn`](crate::CellBlockerFn)s, lowest
/// tag bit first) or a wall.
fn classify(world: &World, grid: &SpatialGrid, i: u32) -> Blocker {
    let c = grid.bounds().coord(i);
    let tags = grid.cell_at(i).flags.tags;
    if tags != 0
        && let Some(hooks) = world.get_resource::<CellBlockers>()
        && let Some(b) = hooks
            .by_tag
            .iter()
            .filter(|&(&bit, _)| tags & (1 << bit) != 0)
            .find_map(|(_, f)| f(world, c))
    {
        return b;
    }
    let p = grid.center(c);
    Blocker::Wall {
        cell: [c.x, c.z],
        pos: [p.x, p.z],
    }
}

/// Why `agent` cannot walk to `goal` (see the module docs): `None` without
/// a grid or position, when the agent stands outside the grid, when the
/// goal is reachable now, or past [`EXPLAIN_CELL_BUDGET`].
pub fn explain_blocked(world: &World, agent: Entity, goal: SimPos) -> Option<BlockedReport> {
    explain_blocked_within(world, agent, goal, EXPLAIN_CELL_BUDGET)
}

/// [`explain_blocked`] with a budget of `budget` flooded cells.
pub fn explain_blocked_within(
    world: &World,
    agent: Entity,
    goal: SimPos,
    budget: usize,
) -> Option<BlockedReport> {
    let grid = world.get_resource::<SpatialGrid>()?;
    let pos = *world.get::<SimPos>(agent)?;
    let bounds = grid.bounds();
    let start = bounds.index(grid.cell_of(pos))?;
    let goal_c = grid.cell_of(goal);
    let (target, outside) = match bounds.index(goal_c) {
        Some(i) => (i, false),
        None => (bounds.index(clamp(bounds, goal_c))?, true),
    };
    let fallback = PassCosts::default();
    let mut judge = Judge {
        world,
        grid,
        costs: world.get_resource::<PassCosts>().unwrap_or(&fallback),
        agent,
        own: own_cells(world, grid, agent),
        cache: BTreeMap::new(),
        strict: false,
        budget,
    };
    let mut reached = judge.flood(bounds, start)?;
    if !outside && reached[target as usize] {
        judge.strict = true;
        reached = judge.flood(bounds, start)?;
        if reached[target as usize] {
            return None;
        }
    }
    let mut nearest = start;
    let mut best = octile(bounds, start, target);
    for (i, _) in reached.iter().enumerate().filter(|(_, r)| **r) {
        let i = i as u32;
        let d = octile(bounds, i, target);
        if d < best {
            (nearest, best) = (i, d);
        }
    }
    let nearest_reachable = Some(grid.center(bounds.coord(nearest)));
    let blocker = if outside {
        Some(Blocker::OutOfBounds)
    } else if !judge.open(target) {
        Some(classify(world, grid, target))
    } else {
        // The impassable cells next to the agent's region.
        let mut frontier = BTreeSet::new();
        for (i, _) in reached.iter().enumerate().filter(|(_, r)| **r) {
            for d in NEIGHBOURS {
                if let Some(n) = step(bounds, i as u32, d)
                    && !reached[n as usize]
                    && !judge.open(n)
                {
                    frontier.insert(n);
                }
            }
        }
        let goal_side = judge.flood(bounds, target)?;
        let between = frontier
            .iter()
            .copied()
            .filter(|&c| {
                NEIGHBOURS
                    .iter()
                    .any(|&d| step(bounds, c, d).is_some_and(|n| goal_side[n as usize]))
            })
            .map(|c| {
                let b = classify(world, grid, c);
                let not_door = !matches!(b, Blocker::Door { .. });
                ((not_door, octile(bounds, c, nearest), c), b)
            })
            .min_by_key(|(k, _)| *k)
            .map(|(_, b)| b);
        between.or_else(|| {
            frontier
                .iter()
                .copied()
                .min_by_key(|&c| (octile(bounds, c, target), c))
                .map(|c| classify(world, grid, c))
        })
    };
    Some(BlockedReport {
        nearest_reachable,
        blocker,
    })
}
