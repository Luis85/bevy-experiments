---
type: Component
---
# `MoveIntent`

**Crate:** `npc_locomotion` · **File:** [`crates/npc_locomotion/src/lib.rs`](../../../crates/npc_locomotion/src/lib.rs) · **Capability:** locomotion (feature `locomotion`, `StraightLineMover` in `NpcPlugins`)

The locomotion seam's input. It says where an agent wants to go, how fast, and how close is close enough.
Actions and orders never move `SimPos` themselves. They write a `MoveIntent` and read
[`MoveStatus`](MoveStatus.md). A mover in `NpcMoverSet` does the moving. `SparseSet` storage, and
`#[require(MoveStatus)]`, so inserting an intent also adds a status.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `goal` | `SimPos` | target position, millimetres |
| `speed_mm_per_tick` | `i32` | mm per tick (movers clamp negative to 0) |
| `tolerance_mm` | `i32` | arrival radius in mm; 0 = exactly on `goal`. `reached(pos)` is `dist2 ≤ tol²` |

`MoveIntent::to(goal, speed)` builds an intent with `tolerance_mm: 0`.

## Lifecycle

- **Added by:** `set_move_intent(world, agent, intent)` (inserts the intent and `MoveStatus::Moving`;
  also used to retarget). Callers: village walks (`start_walk` in
  [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs)), the testbed
  walk action, `npc_command` orders (`exec.rs`, `approach.rs`), vehicles (`npc_vehicles/src/drive.rs`), glue
  `command_interaction` / `command_vehicles`, `apps/rts_units_demo`.
- **Read by:** [`straight_line_mover_system`](../systems/straight_line_mover_system.md) (agents without
  [`ExternalMover`](ExternalMover.md)), [`nav_plan_system`](../systems/nav_plan_system.md) (goal changes →
  path requests), [`grid_path_mover_system`](../systems/grid_path_mover_system.md), `retry_failed_path`.
- **Removed by:** `clear_move_intent(world, agent)` (removes intent and status), or directly with
  `Commands` (village `move_to_system` in `NpcSet::Actions` removes it before the movers run).

## Hashing and snapshots

- **State hash:** only when `register_locomotion_hash(app)` is called, as `move_intent`. `NavPlugin` calls it,
  and so does `npc_command`'s `register_command_hash`, which `CommandPlugin::build` calls (it also calls
  `register_spatial_hash`), and `npc_vehicles`' `register_vehicle_hash` (`VehiclesPlugin`). So with `with_command()`,
  `with_vehicles()` or `NavPlugin` on, the intent is hashed. The plain village does not:
  its own `MoveTo` already carries the goal. Calling it changes the content hash and so invalidates
  recorded replays. Encoding: `goal`, `speed_mm_per_tick` (i32), `tolerance_mm` (i32).
- **World snapshot (ADR 0013):** always saved as `move_intent`. `NpcLocomotionPlugin` registers it with
  `register_snapshot_component_named`, whether or not it is hashed.

## Related

- [`MoveStatus`](MoveStatus.md) — what the mover made of the intent.
- [`ExternalMover`](ExternalMover.md) / [`NavAgent`](NavAgent.md) — choose which mover handles it.

## Example

```rust
use npc_framework::locomotion::{MoveIntent, MoveStatus, clear_move_intent, set_move_intent};

// Action start: walk to within 300 mm of the goal at 60 mm/tick.
set_move_intent(world, agent, MoveIntent { goal, speed_mm_per_tick: 60, tolerance_mm: 300 });

// A system in NpcSet::Locomotion, .after(NpcMoverSet), reacts in the same tick:
match world.get::<MoveStatus>(agent) {
    Some(MoveStatus::Arrived) => clear_move_intent(world, agent),
    Some(MoveStatus::Blocked { ticks }) if *ticks >= 40 => { /* give up */ }
    _ => {}
}
```
