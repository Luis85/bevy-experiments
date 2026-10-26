---
type: Component
---
# `NavAgent`

**Crate:** `npc_nav` · **File:** [`crates/npc_nav/src/lib.rs`](../../../crates/npc_nav/src/lib.rs) · **Capability:** nav (feature `nav`, opt-in `NavPlugin` a.k.a. `GridPathMover`)

Per-unit opt-in to grid pathfinding. A `NavAgent`'s [`MoveIntent`](MoveIntent.md)s get an A* path on the
kernel's `SpatialGrid` and are walked along it by the grid path mover. The `StraightLineMover` does not
walk them. Actions do not change: they still write `MoveIntent` and read `MoveStatus`.
`#[require(ExternalMover, NavState)]`, so inserting it also adds [`ExternalMover`](ExternalMover.md) and
[`NavState::Idle`](NavState.md).

## Fields

Marker component, no fields.

## Lifecycle

- **Added by:** content spawn bundles. Testbed actors and RTS units in
  [`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs); the village's
  `village_nav` scenario ([`content/village_content/src/scenario.rs`](../../../content/village_content/src/scenario.rs));
  `apps/rts_units_demo`. The app must also add `NavPlugin`; it is not in `NpcPlugins`.
- **Read by:** [`nav_plan_system`](../systems/nav_plan_system.md) and
  [`grid_path_mover_system`](../systems/grid_path_mover_system.md) (`With<NavAgent>`); `retry_failed_path`;
  `npc_command` group follow (`exec.rs`: followers with a `NavAgent` reuse the leader's path);
  `npc_interaction` `auto_door_system` and `key_replan_system` (`door.rs`).
- **Removed by:** nothing in the framework.

## Hashing and snapshots

- **State hash:** yes, as `nav_agent` (`register_sim_component` in `NavPlugin`; presence only, empty encoding).
  Adding `NavPlugin` also registers `nav_state`, `path`, `nav_queue`, the locomotion hash (`move_intent`,
  `move_status`) and the spatial hash (`spatial_grid`, `grid_blocker`, `grid_occluder`). That changes the
  content hash of any app that adds it, which is why it is opt-in.
- **World snapshot (ADR 0013):** saved as `nav_agent` (automatic with hashing).

## Related

- [`NavState`](NavState.md), [`Path`](Path.md) — managed for every `NavAgent`.
- [`ExternalMover`](ExternalMover.md) — required; keeps the straight-line mover away.

## Example

```rust
use npc_framework::nav::{NavAgent, NavBudget, NavPlugin};

app.add_plugins((NpcPlugins::default(), NavPlugin))
   .insert_resource(NavBudget(8000)); // optional: A* expansions per tick (default 4000)

spawn_sim(world, (SimPos::new(0, 0), NavAgent)); // + ExternalMover, NavState::Idle
set_move_intent(world, agent, MoveIntent::to(SimPos::new(9000, 4000), 50));
```
