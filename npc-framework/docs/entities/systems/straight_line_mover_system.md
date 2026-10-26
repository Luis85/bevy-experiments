---
type: System
---
# `straight_line_mover_system`

**Crate:** `npc_locomotion` · **File:** [`crates/npc_locomotion/src/lib.rs`](../../../crates/npc_locomotion/src/lib.rs) · **Plugin:** `StraightLineMover` (in `NpcPlugins` with feature `locomotion`)

The default mover. For every agent with a [`MoveIntent`](../components/MoveIntent.md) and no
[`ExternalMover`](../components/ExternalMover.md), it calls the pure `straight_step`:

- Already within tolerance: no move, `Arrived`.
- Otherwise it steps `pos.step_towards(goal, speed_mm_per_tick)`: `Arrived` if the new position is within
  tolerance, else `Moving`.
- If the kernel `SpatialGrid` has blocking cells and the step would enter a blocking cell (other than the
  one the agent stands in, checked with `trace_segment`), it does not step, and the status becomes
  `blocked_again()` (`Blocked { ticks + 1 }`). Without blocking cells, no grid checks are made.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcMoverSet`, which `NpcLocomotionPlugin` configures `.in_set(NpcSet::Locomotion)`. Not exclusive.
- **Ordering:** none of its own. `NavPlugin` orders its systems `.after(straight_line_mover_system)`. Systems
  that react to arrival in the same tick run `.after(NpcMoverSet)` in `NpcSet::Locomotion` (e.g. village
  `move_arrival_system`).

## Reads / writes

| Access | Data |
|---|---|
| reads | `MoveIntent`, `SpatialGrid` (optional resource, read-only) |
| writes | `SimPos` (only when it changes), `MoveStatus` (`set_if_neq`) |
| filter | `Without<ExternalMover>`, so `NavAgent`s and host-moved agents are skipped |

## Determinism

Entities are independent and the grid is read-only here, so query order does not matter. The step is
integer only (`SimPos::step_towards`, `dist2`). Writes happen only on change, so change detection is exact.

## Performance

- **Mechanic tag:** `locomotion` (via `tags.rs` prefix `npc_locomotion`) · **Budget:** 10 µs fixed + 2 µs per
  agent (`NPC_BUDGET_TABLE`) · spans opened: none.

## Hashing note

The positions it writes are hashed (`SimPos` as `pos`). `MoveIntent`/`MoveStatus` are hashed only after
`register_locomotion_hash(app)`, which `NavPlugin`, `npc_command`'s `register_command_hash` and `npc_vehicles`' `register_vehicle_hash` call. A game
whose decisions depend on `MoveStatus`, for example because it has blockers, should call it.

## Related

- [`grid_path_mover_system`](grid_path_mover_system.md) — the grid alternative, after this one.
- Components: [`MoveIntent`](../components/MoveIntent.md), [`MoveStatus`](../components/MoveStatus.md),
  [`ExternalMover`](../components/ExternalMover.md)
