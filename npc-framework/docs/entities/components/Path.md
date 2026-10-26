---
type: Component
---
# `Path`

**Crate:** `npc_nav` · **File:** [`crates/npc_nav/src/lib.rs`](../../../crates/npc_nav/src/lib.rs) · **Capability:** nav

A smoothed grid path for a [`NavAgent`](NavAgent.md). The waypoints come from integer A* plus line-of-sight
string-pulling (`plan::smooth`). In an open field the path is a single waypoint: the goal. `SparseSet`
storage. Present only while the agent's [`NavState`](NavState.md) is `Following`.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `waypoints` | `Vec<SimPos>` | mm; excludes the start, ends at the goal; inner points are cell centres |
| `markers` | `Vec<PathMarker>` | waypoints that need an interaction (e.g. a door), from `PassCost::Marked` |
| `next` | `u32` | index into `waypoints` of the waypoint being walked to |

`PathMarker` (not a component): `waypoint: u32` (index into `waypoints`), `cell: CellCoord`, and `tag: u8`
(the cell tag bit whose `PassCostFn` marked it). Smoothing never skips a marked cell. The mover waits in front
of a blocking marked cell, and no replan happens while it waits there. Helpers: `remaining()` gives the
waypoints from `next` on; `marker_at(index)` gives the marker on that waypoint, if any.

## Lifecycle

- **Added by:** [`nav_plan_system`](../systems/nav_plan_system.md) when a request is found (journal
  `PathFound { len, nodes }`); `npc_command` group followers (`derive_path` in `exec.rs`: the leader's path
  shifted by the follower's offset).
- **Read by:** [`grid_path_mover_system`](../systems/grid_path_mover_system.md);
  [`nav_plan_system`](../systems/nav_plan_system.md) (replan when a `GridChanged` cell lies on the rest of the path);
  `npc_interaction` `auto_door_system` (opens the door at the next marker); `npc_command` `follow_leader`;
  `npc_view_helpers::nav_gizmos` (view only).
- **Written by:** [`grid_path_mover_system`](../systems/grid_path_mover_system.md) (advances `next`).
- **Removed by:** [`nav_plan_system`](../systems/nav_plan_system.md) on any transition to a state that is not `Following`.

## Hashing and snapshots

- **State hash:** yes, as `path` (`register_sim_component` in `NavPlugin`). Encoding: waypoint count (u32) and
  each waypoint; marker count (u32) and per marker `waypoint` (u32), `cell.x`/`cell.z` (i32), `tag` (u8); then `next` (u32).
- **World snapshot (ADR 0013):** saved as `path` (automatic with hashing).

## Related

- [`NavState`](NavState.md) — `Following { goal }` goes with it.
- [`MoveIntent`](MoveIntent.md) — the goal and the speed along the path.

## Example

```rust
use npc_framework::nav::{NavAppExt, PassCost, Path};

// A tag bit whose cells need an interaction: the path gets a marker there.
app.register_pass_cost(0, |_world, _agent, _cell, _info| PassCost::Marked(20));

// Reading the next waypoint (e.g. for a door system):
if let Some(path) = world.get::<Path>(agent)
    && let Some(m) = path.marker_at(path.next)
{
    let _door_cell = m.cell;
}
```
