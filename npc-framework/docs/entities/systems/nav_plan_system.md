---
type: System
---
# `nav_plan_system`

**Crate:** `npc_nav` · **File:** [`crates/npc_nav/src/plan.rs`](../../../crates/npc_nav/src/plan.rs) (re-exported as `npc_nav::nav_plan_system`) · **Plugin:** `NavPlugin` (alias `GridPathMover`, opt-in, feature `nav`)

The planning half of the grid path mover. It is exclusive. It handles request bookkeeping and replanning
for every [`NavAgent`](../components/NavAgent.md), then serves the request queue under the per-tick A* budget.

1. **Changed cells:** it reads every `GridChanged` message since its last run, through its own cursor in
   `NavQueue`, into a `BTreeSet<CellCoord>`.
2. **Bookkeeping**, per agent (`decide`):
   - No intent: `Idle`, and the `Path` is removed.
   - A new or changed goal: `Pending { goal, tick }`, and a `PathRequest` is queued. Any older request of
     this agent is dropped.
   - When `Following`, it replans if a changed cell lies on the rest of the path (`path_touches`), or if
     `MoveStatus::Blocked { ticks }` reaches a multiple of `REPLAN_BLOCKED_TICKS` (10). A blocked agent
     waiting next to a marked waypoint (a door) does not replan. Changes to cells covered by the agent's
     own `GridBlocker` never cause a replan.
   - When `Failed`, any `GridChanged` re-queues the request.
3. **Serve:** it walks `NavQueue.requests` in `(enqueue tick, SimId)` order while `NavBudget` expansions
   remain. Requests whose agent is gone, is no longer a `NavAgent`, or no longer has the matching `Pending`
   state are dropped. The head request may pause mid-search and resume next tick; the requests behind it
   wait. A `GridChanged` restarts the paused search only if the search touched a changed cell
   (`Search::touches`). One request's work over all its restarts is capped at
   `max(NavBudget × SEARCH_LIMIT_BUDGETS (8), grid cell count)`. Past the cap it fails with `SearchLimit`.
   - Found: it inserts `Path` (smoothed) and `Following { goal }`, and journals `PathFound { len, nodes }`.
   - Failed: it inserts `Failed { goal, reason }` and journals `PathFailed { reason }`.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcMoverSet` (inside `NpcSet::Locomotion`). Exclusive (`nav_plan_system(world: &mut World, agents:
  &mut NavAgentQuery)`; `NavAgentQuery` is the cached `QueryState` of the agent rows, `With<NavAgent>`).
- **Ordering:** `(nav_plan_system, mover::grid_path_mover_system).chain().in_set(NpcMoverSet).after(straight_line_mover_system)`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `NavAgent`, `SimId`, `SimPos`, `MoveIntent`, `MoveStatus`, `GridBlocker`; `SimClock`, `SpatialGrid` (returns early without it), `Messages<GridChanged>`, `NavBudget`, `PassCosts` (pass-cost hooks, called with `&World`), `SimIndex` |
| writes | `NavState`, `Path` (insert/remove), `NavQueue` (requests, paused search, cursor), `NavStats` (tooling counters, not hashed) |
| journal | `PathFound { len, nodes }`, `PathFailed { reason }` |

## Determinism

Agent rows are collected and sorted by `SimId` before any decision. Requests live in a `BTreeMap` keyed
`(tick, SimId)`, and changed cells in a `BTreeSet`. A* is integer-only with a total-order heap, with ties
broken by `(f, h, cell index)`. `NavQueue` is hashed as `nav_queue`, including the paused search's whole
progress, and it is saved in snapshots (the `GridChanged` cursor is not saved). `NavBudget` itself is
**not** hashed or registered for snapshots. It changes *when* paths are found, so every peer and every
replay must use the same value. `NavStats` is never read by the sim.

## Performance

- **Mechanic tag:** `nav` (via `tags.rs` prefix `npc_nav`; the module path is `npc_nav::plan`, which does not
  match the longer `npc_nav::mover`) · **Budget:** `nav` 30 µs fixed + 2 µs per agent.
- **Span:** the serving step runs inside `info_span!("mechanic", name = "nav.astar")`. Budget `nav.astar`:
  2500 µs fixed, 0 per agent (`NPC_BUDGET_TABLE`). Bookkeeping time counts under `nav`.
- **Knob:** `NavBudget(u32)` resource, A* expansions per tick over all requests (default
  `DEFAULT_NAV_BUDGET` = 4000). `NavStats.nodes_last_tick` shows the spend.

## Related

- [`grid_path_mover_system`](grid_path_mover_system.md) — chained right after; walks the `Path`.
- `retry_failed_path` (same file) — re-queues a `Failed` agent outside a `GridChanged` (used by
  `npc_interaction`'s `key_replan_system`).
- Components: [`NavAgent`](../components/NavAgent.md), [`NavState`](../components/NavState.md),
  [`Path`](../components/Path.md), [`MoveIntent`](../components/MoveIntent.md), [`MoveStatus`](../components/MoveStatus.md)
