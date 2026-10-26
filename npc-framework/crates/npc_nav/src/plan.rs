//! Request bookkeeping, serving under the budget, and path smoothing.

use std::collections::{BTreeMap, BTreeSet};

use bevy_ecs::message::Messages;
use bevy_ecs::prelude::*;
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::spatial::{
    CellCoord, CellInfo, GridBlocker, GridChanged, PathFailReason, SpatialGrid,
};
use npc_kernel::{SimClock, SimId, SimIndex, SimPos};
use npc_locomotion::{MoveIntent, MoveStatus};
use tracing::info_span;

use crate::astar::{Passage, Search, Step};
use crate::{
    ActiveSearch, NavAgent, NavBudget, NavQueue, NavState, PassCost, PassCosts, Path, PathMarker,
    PathRequest, REPLAN_BLOCKED_TICKS, SEARCH_LIMIT_BUDGETS,
};

/// Tooling counters (not hashed, never read by the sim).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NavStats {
    /// Expansions in the last tick.
    pub nodes_last_tick: u32,
    pub nodes_total: u64,
    pub paths_found: u64,
    pub paths_failed: u64,
}

/// Whether `agent` may enter dense cell `index` (pass-cost hooks first,
/// then the cell's flags). Tagged cells are cached per call site.
pub fn passage(
    world: &World,
    grid: &SpatialGrid,
    costs: &PassCosts,
    agent: Entity,
    index: u32,
    cache: &mut BTreeMap<u32, Passage>,
) -> Passage {
    passage_of(world, grid, costs, agent, index, grid.cell_at(index), cache)
}

/// [`passage`] for an agent standing with its own blocker on `own` (sorted
/// dense indices, [`own_cells`]): those cells are judged as they would be
/// without it.
#[allow(clippy::too_many_arguments)]
pub fn passage_for(
    world: &World,
    grid: &SpatialGrid,
    costs: &PassCosts,
    agent: Entity,
    own: &[u32],
    index: u32,
    cache: &mut BTreeMap<u32, Passage>,
) -> Passage {
    let info = if own.binary_search(&index).is_ok() {
        grid.cell_without_one_blocker(index)
    } else {
        grid.cell_at(index)
    };
    passage_of(world, grid, costs, agent, index, info, cache)
}

/// The cells (sorted dense indices) `agent`'s own `GridBlocker` blocks: its
/// footprint around its indexed grid position (where the grid's dynamic
/// layer has it; its `SimPos` if it is not indexed yet). Empty without a
/// blocker.
pub fn own_cells(world: &World, grid: &SpatialGrid, agent: Entity) -> Vec<u32> {
    let Some(blocker) = world.get::<GridBlocker>(agent) else {
        return Vec::new();
    };
    let at = world
        .get::<SimId>(agent)
        .and_then(|&id| grid.position_of(id))
        .or_else(|| world.get::<SimPos>(agent).copied());
    at.map_or_else(Vec::new, |p| grid.footprint_indices(p, blocker.0))
}

fn passage_of(
    world: &World,
    grid: &SpatialGrid,
    costs: &PassCosts,
    agent: Entity,
    index: u32,
    info: CellInfo,
    cache: &mut BTreeMap<u32, Passage>,
) -> Passage {
    if info.flags.tags != 0 {
        if let Some(&p) = cache.get(&index) {
            return p;
        }
        let cell = grid.bounds().coord(index);
        let decided = costs
            .by_tag
            .iter()
            .filter(|&(&bit, _)| info.flags.tags & (1 << bit) != 0)
            .map(|(&bit, f)| (bit, f(world, agent, cell, info)))
            .find(|(_, c)| *c != PassCost::Default);
        if let Some((bit, cost)) = decided {
            let p = match cost {
                PassCost::Blocked => Passage::Blocked,
                PassCost::Cost(c) => Passage::Open(c),
                PassCost::Marked(c) => Passage::Marked { extra: c, tag: bit },
                PassCost::Default => unreachable!("filtered above"),
            };
            cache.insert(index, p);
            return p;
        }
    }
    if info.flags.blocking {
        Passage::Blocked
    } else {
        Passage::Open(info.extra_cost)
    }
}

/// String-pulls the A* `cells` (dense indices, start..=goal) into waypoints
/// from `start` (exclusive) to `goal` (inclusive): from each anchor, the
/// furthest later point still in line of sight, where line of sight means
/// every cell the segment touches (other than its two end cells) is open
/// with no extra cost. Marked cells are always kept as waypoints (with a
/// [`PathMarker`]); smoothing never jumps over them.
pub fn smooth(
    grid: &SpatialGrid,
    cells: &[u32],
    start: SimPos,
    goal: SimPos,
    pass: &mut dyn FnMut(u32) -> Passage,
) -> Path {
    let n = cells.len();
    let bounds = grid.bounds();
    let point = |i: usize| -> SimPos {
        if i == 0 {
            start
        } else if i + 1 == n {
            goal
        } else {
            grid.center(bounds.coord(cells[i]))
        }
    };
    let marker = |i: usize, pass: &mut dyn FnMut(u32) -> Passage| match pass(cells[i]) {
        Passage::Marked { tag, .. } => Some(tag),
        _ => None,
    };
    let mut path = Path::default();
    if n <= 1 {
        path.waypoints.push(goal);
        return path;
    }
    let marked: Vec<Option<u8>> = (0..n).map(|i| marker(i, pass)).collect();
    let mut anchor = 0;
    while anchor + 1 < n {
        let limit = (anchor + 1..n)
            .find(|&i| marked[i].is_some())
            .unwrap_or(n - 1);
        let mut best = anchor + 1;
        let from = point(anchor);
        for k in anchor + 2..=limit {
            let to = point(k);
            let (c0, c1) = (grid.cell_of(from), grid.cell_of(to));
            let visible = grid.trace_segment(from, to, |c| {
                c == c0 || c == c1 || bounds.index(c).is_some_and(|i| pass(i) == Passage::Open(0))
            });
            if !visible {
                break;
            }
            best = k;
        }
        path.waypoints.push(point(best));
        if let Some(tag) = marked[best] {
            path.markers.push(PathMarker {
                waypoint: (path.waypoints.len() - 1) as u32,
                cell: bounds.coord(cells[best]),
                tag,
            });
        }
        anchor = best;
    }
    path
}

/// Whether the rest of `path` from `pos` touches a cell in `changed`.
fn path_touches(
    grid: &SpatialGrid,
    pos: SimPos,
    path: &Path,
    changed: &BTreeSet<CellCoord>,
) -> bool {
    let mut from = pos;
    for &wp in path.remaining() {
        if !grid.trace_segment(from, wp, |c| !changed.contains(&c)) {
            return true;
        }
        from = wp;
    }
    false
}

struct AgentRow {
    id: SimId,
    entity: Entity,
    pos: SimPos,
    intent: Option<MoveIntent>,
    state: NavState,
    status: MoveStatus,
    path: Option<Path>,
    /// Cells its own `GridBlocker` covers (their changes are its own
    /// movement, never a reason to replan).
    own: BTreeSet<CellCoord>,
}

/// The nav agents ([`nav_plan_system`]'s cached query).
pub type NavAgentQuery = QueryState<
    (
        Entity,
        &'static SimId,
        &'static SimPos,
        Option<&'static MoveIntent>,
        &'static NavState,
        Option<&'static MoveStatus>,
        Option<&'static Path>,
    ),
    With<NavAgent>,
>;

/// `NpcMoverSet` (exclusive, before the grid mover): bookkeeping for every
/// `NavAgent` in `SimId` order, then serving the queue under [`NavBudget`].
/// See the crate docs.
pub fn nav_plan_system(world: &mut World, agents: &mut NavAgentQuery) {
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    if !world.contains_resource::<SpatialGrid>() {
        return;
    }

    // 1. Cells changed since the last run.
    let changed: BTreeSet<CellCoord> = world.resource_scope(|world, mut queue: Mut<NavQueue>| {
        let Some(messages) = world.get_resource::<Messages<GridChanged>>() else {
            return BTreeSet::new();
        };
        queue
            .cursor
            .read(messages)
            .flat_map(|m| m.cells.iter().copied())
            .collect()
    });

    // 2. Bookkeeping, in SimId order.
    let mut rows: Vec<AgentRow> = agents
        .iter(world)
        .map(
            |(entity, &id, &pos, intent, &state, status, path)| AgentRow {
                id,
                entity,
                pos,
                intent: intent.copied(),
                state,
                status: status.copied().unwrap_or_default(),
                path: path.cloned(),
                own: BTreeSet::new(),
            },
        )
        .collect();
    rows.sort_unstable_by_key(|r| r.id);
    if !changed.is_empty() {
        let grid = world.resource::<SpatialGrid>();
        let bounds = grid.bounds();
        for row in &mut rows {
            row.own = own_cells(world, grid, row.entity)
                .into_iter()
                .map(|i| bounds.coord(i))
                .collect();
        }
    }

    for row in &rows {
        let new_state = {
            let grid = world.resource::<SpatialGrid>();
            decide(grid, row, &changed, tick)
        };
        let Some(new_state) = new_state else { continue };
        let mut queue = world.resource_mut::<NavQueue>();
        // Drop any older request of this agent.
        if let NavState::Pending { tick: t, .. } = row.state {
            queue.requests.remove(&(t, row.id));
            if queue.active.as_ref().is_some_and(|a| a.key == (t, row.id)) {
                queue.active = None;
            }
        }
        if let NavState::Pending { goal, tick: t } = new_state {
            queue.requests.insert(
                (t, row.id),
                PathRequest {
                    agent: row.id,
                    goal,
                    tick: t,
                },
            );
        }
        let mut e = world.entity_mut(row.entity);
        e.insert(new_state);
        if !matches!(new_state, NavState::Following { .. }) {
            e.remove::<Path>();
        }
    }

    // 3. Serve under the budget.
    let full_budget = world.get_resource::<NavBudget>().map_or(0, |b| b.0);
    let mut budget = full_budget;
    let cells = u32::try_from(world.resource::<SpatialGrid>().bounds().len()).unwrap_or(u32::MAX);
    let limit = full_budget.saturating_mul(SEARCH_LIMIT_BUDGETS).max(cells);
    world.resource_scope(|world, mut queue: Mut<NavQueue>| {
        let _span = info_span!("mechanic", name = "nav.astar").entered();
        // A change the paused search looked at restarts it (its work so far
        // still counts towards the limit); any other change cannot alter
        // its result.
        if !changed.is_empty() {
            let bounds = world.resource::<SpatialGrid>().bounds();
            let NavQueue {
                active, scratch, ..
            } = &mut *queue;
            if let Some(a) = active.as_mut()
                && changed
                    .iter()
                    .any(|&c| a.search.touches(scratch, bounds, c))
            {
                a.search = Search::new(a.search.start, a.search.goal);
            }
        }
        let keys: Vec<(u64, SimId)> = queue.requests.keys().copied().collect();
        for key in keys {
            if budget == 0 {
                break;
            }
            let req = queue.requests[&key];
            let entity = world.resource::<SimIndex>().get(req.agent);
            let valid = entity.filter(|&e| {
                world.get::<NavAgent>(e).is_some()
                    && world.get::<NavState>(e)
                        == Some(&NavState::Pending {
                            goal: req.goal,
                            tick: req.tick,
                        })
            });
            let Some(entity) = valid else {
                queue.requests.remove(&key);
                if queue.active.as_ref().is_some_and(|a| a.key == key) {
                    queue.active = None;
                }
                continue;
            };
            let Some(&pos) = world.get::<SimPos>(entity) else {
                queue.requests.remove(&key);
                continue;
            };
            let outcome = serve(
                world,
                &mut queue,
                key,
                entity,
                (pos, req.goal),
                &mut budget,
                limit,
            );
            let Some((outcome, nodes)) = outcome else {
                // Paused: carry over to the next tick.
                break;
            };
            queue.requests.remove(&key);
            queue.active = None;
            match outcome {
                Ok(path) => {
                    let len = path.waypoints.len() as u32;
                    world
                        .entity_mut(entity)
                        .insert((path, NavState::Following { goal: req.goal }));
                    journal::record(world, Some(req.agent), || JournalKind::PathFound {
                        len,
                        nodes,
                    });
                    world.resource_mut::<NavStats>().paths_found += 1;
                }
                Err(reason) => {
                    world.entity_mut(entity).insert(NavState::Failed {
                        goal: req.goal,
                        reason,
                    });
                    journal::record(world, Some(req.agent), || JournalKind::PathFailed {
                        reason,
                    });
                    world.resource_mut::<NavStats>().paths_failed += 1;
                }
            }
        }
    });
    let nodes_this_tick = full_budget - budget;
    let mut stats = world.resource_mut::<NavStats>();
    stats.nodes_last_tick = nodes_this_tick;
    stats.nodes_total += u64::from(nodes_this_tick);
}

/// The new [`NavState`] for one agent, or `None` to leave it as is.
fn decide(
    grid: &SpatialGrid,
    row: &AgentRow,
    changed: &BTreeSet<CellCoord>,
    tick: u64,
) -> Option<NavState> {
    let Some(intent) = row.intent else {
        return (row.state != NavState::Idle || row.path.is_some()).then_some(NavState::Idle);
    };
    let request = Some(NavState::Pending {
        goal: intent.goal,
        tick,
    });
    if row.state.goal() != Some(intent.goal) {
        return request;
    }
    match row.state {
        NavState::Following { .. } => {
            let Some(path) = &row.path else {
                return request;
            };
            if intent.reached(row.pos) {
                return None;
            }
            let touched = if row.own.is_empty() {
                !changed.is_empty() && path_touches(grid, row.pos, path, changed)
            } else {
                let others: BTreeSet<CellCoord> = changed.difference(&row.own).copied().collect();
                !others.is_empty() && path_touches(grid, row.pos, path, &others)
            };
            if touched {
                return request;
            }
            if let MoveStatus::Blocked { ticks } = row.status
                && ticks >= REPLAN_BLOCKED_TICKS
                && ticks % REPLAN_BLOCKED_TICKS == 0
                && !waiting_at_marker(grid, row.pos, path)
            {
                return request;
            }
            None
        }
        NavState::Failed { .. } if !changed.is_empty() => request,
        _ => None,
    }
}

/// Retries `agent`'s failed path request now, as a `GridChanged` would: a
/// `NavAgent` in [`NavState::Failed`] whose `MoveIntent` still asks for that
/// goal is queued again (`NavState::Pending` at the current tick, served in
/// the usual `(tick, SimId)` order). For what the grid does not report but a
/// pass-cost hook decides per agent, e.g. a unit that just got the key of a
/// locked door (`npc_interaction`). Returns whether a request was queued.
pub fn retry_failed_path(world: &mut World, agent: Entity) -> bool {
    let Some(&NavState::Failed { goal, .. }) = world.get::<NavState>(agent) else {
        return false;
    };
    if world.get::<NavAgent>(agent).is_none()
        || world.get::<MoveIntent>(agent).map(|i| i.goal) != Some(goal)
    {
        return false;
    }
    let (Some(&id), Some(tick)) = (
        world.get::<SimId>(agent),
        world.get_resource::<SimClock>().map(|c| c.tick),
    ) else {
        return false;
    };
    let Some(mut queue) = world.get_resource_mut::<NavQueue>() else {
        return false;
    };
    queue.requests.insert(
        (tick, id),
        PathRequest {
            agent: id,
            goal,
            tick,
        },
    );
    world
        .entity_mut(agent)
        .insert(NavState::Pending { goal, tick });
    true
}

/// True when the next waypoint is a marked cell (a door to open): the mover
/// waits there instead of replanning around it.
fn waiting_at_marker(grid: &SpatialGrid, pos: SimPos, path: &Path) -> bool {
    path.marker_at(path.next).is_some_and(|m| {
        let here = grid.cell_of(pos);
        (here.x - m.cell.x).abs() <= 1 && (here.z - m.cell.z).abs() <= 1
    })
}

/// Runs (or resumes) the search for `key` from `pos` to `goal`. `None` =
/// paused (budget spent); otherwise the result and the nodes the last
/// search took. A request whose work over all its restarts exceeds `limit`
/// fails with `SearchLimit` instead of pausing again.
#[allow(clippy::too_many_arguments)]
fn serve(
    world: &mut World,
    queue: &mut NavQueue,
    key: (u64, SimId),
    agent: Entity,
    (pos, goal): (SimPos, SimPos),
    budget: &mut u32,
    limit: u32,
) -> Option<(Result<Path, PathFailReason>, u32)> {
    let grid = world.resource::<SpatialGrid>();
    let costs = world.resource::<PassCosts>();
    let bounds = grid.bounds();
    let (Some(start), Some(goal_i)) = (
        bounds.index(grid.cell_of(pos)),
        bounds.index(grid.cell_of(goal)),
    ) else {
        return Some((Err(PathFailReason::OutOfBounds), 0));
    };
    match queue.active.as_mut() {
        Some(a) if a.key == key => {
            if a.search.start != start || a.search.goal != goal_i {
                a.search = Search::new(start, goal_i);
            }
        }
        _ => {
            queue.active = Some(ActiveSearch {
                key,
                search: Search::new(start, goal_i),
                spent: 0,
            });
        }
    }
    let own = own_cells(world, grid, agent);
    let mut cache = BTreeMap::new();
    let mut pass = |i: u32| passage_for(world, grid, costs, agent, &own, i, &mut cache);
    let NavQueue {
        active, scratch, ..
    } = queue;
    let a = active.as_mut().expect("set above");
    let before = a.search.nodes;
    let step = a.search.run(scratch, bounds, budget, &mut pass);
    let nodes = a.search.nodes;
    a.spent = a.spent.saturating_add(nodes - before);
    match step {
        Step::Paused if a.spent > limit => Some((Err(PathFailReason::SearchLimit), nodes)),
        Step::Paused => None,
        Step::Failed(reason) => Some((Err(reason), nodes)),
        Step::Found(cells) => {
            // The search's own start is where the agent stood when it began;
            // a paused search's agent has not moved (it has no path).
            Some((Ok(smooth(grid, &cells, pos, goal, &mut pass)), nodes))
        }
    }
}
