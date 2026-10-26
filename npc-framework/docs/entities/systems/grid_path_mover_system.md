---
type: System
---
# `grid_path_mover_system`

**Crate:** `npc_nav` · **File:** [`crates/npc_nav/src/mover.rs`](../../../crates/npc_nav/src/mover.rs) · **Plugin:** `NavPlugin` (alias `GridPathMover`, opt-in, feature `nav`)

The movement half of the grid path mover. For every [`NavAgent`](../components/NavAgent.md) with a
[`MoveIntent`](../components/MoveIntent.md), it sets [`MoveStatus`](../components/MoveStatus.md):

- Within tolerance: `Arrived`, without moving.
- `NavState::Following` with a `Path`: `follow` moves up to `speed_mm_per_tick` mm along the waypoints. It
  can pass several waypoints in one tick, and advances `Path.next` at each one it reaches. It never enters a
  blocking cell, except the one it stands in or a cell blocked only by its own `GridBlocker` footprint. Blocked
  before moving at all: `blocked_again()`. Otherwise `Arrived` or `Moving`.
- `NavState::Failed`: `blocked_again()`, so actions can give up. `nav_plan_system` retries a failed request
  only on a `GridChanged` (or `retry_failed_path`), not on the blocked count.
- Anything else (`Pending`, `Idle`): `Moving`, meaning it is waiting for a path.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcMoverSet` (inside `NpcSet::Locomotion`). Not exclusive.
- **Ordering:** `(nav_plan_system, mover::grid_path_mover_system).chain().in_set(NpcMoverSet).after(straight_line_mover_system)`,
  so a path found this tick is walked this tick.

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`, `GridBlocker` (optional), `MoveIntent`, `NavState`, `SpatialGrid` (returns early without it) |
| writes | `SimPos` (only on change), `MoveStatus` (`set_if_neq`), `Path.next` |
| filter | `With<NavAgent>` |

## Determinism

Agents are independent, and the grid is read-only here, so query order does not matter. The math is integer
only: `step_towards`, `dist2().isqrt()` for the distance used per waypoint, at least 1 mm per reached
waypoint. The agent's own blocker cells come from `grid.footprint_indices(grid.position_of(id) or pos, footprint)`.

## Performance

- **Mechanic tag:** `locomotion` (via `tags.rs` prefix `npc_nav::mover`, longer than `npc_nav`) · **Budget:**
  10 µs fixed + 2 µs per agent (`NPC_BUDGET_TABLE`) · spans opened: none (A* time is in `nav_plan_system`'s
  `nav.astar` span).

## Related

- [`nav_plan_system`](nav_plan_system.md) — runs right before it; produces `Path` / `NavState`.
- [`straight_line_mover_system`](straight_line_mover_system.md) — the non-grid mover; skips `NavAgent`s
  through `ExternalMover`.
- `npc_interaction` `auto_door_system` — opens the door an agent waits at (a marked waypoint).
- Components: [`Path`](../components/Path.md), [`NavState`](../components/NavState.md),
  [`MoveIntent`](../components/MoveIntent.md), [`MoveStatus`](../components/MoveStatus.md)
