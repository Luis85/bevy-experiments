---
type: Component
---
# `MoveStatus`

**Crate:** `npc_locomotion` · **File:** [`crates/npc_locomotion/src/lib.rs`](../../../crates/npc_locomotion/src/lib.rs) · **Capability:** locomotion

The locomotion seam's output. It is what the mover made of the agent's [`MoveIntent`](MoveIntent.md),
updated every tick a mover runs. Actions read it to succeed on arrival or give up when blocked.
`SparseSet` storage. `MoveIntent` requires it, so it appears with the intent.

## Variants

| Variant | Meaning |
|---|---|
| `Idle` (default) | no intent, or not yet seen by a mover |
| `Moving` | on the way, or waiting for a path (`NavState::Pending`) |
| `Arrived` | within `tolerance_mm` of the goal |
| `Blocked { ticks: u32 }` | could not move for `ticks` consecutive ticks: a blocking cell ahead, or the path request failed |

`blocked_again()` returns `Blocked { ticks + 1 }` (saturating), or `Blocked { ticks: 1 }` from any other variant.

## Lifecycle

- **Added by:** the `#[require(MoveStatus)]` on `MoveIntent` (as `Idle`), or `set_move_intent` (as `Moving`).
- **Written by:** [`straight_line_mover_system`](../systems/straight_line_mover_system.md) and
  [`grid_path_mover_system`](../systems/grid_path_mover_system.md) (with `set_if_neq`, so change detection
  fires only on a real change). Also by host movers carrying [`ExternalMover`](ExternalMover.md), by
  `npc_command` group followers (`exec.rs`, set to `Moving` with a derived path), and by `npc_vehicles`
  (`seats.rs` sets `Idle`).
- **Read by:** [`nav_plan_system`](../systems/nav_plan_system.md) (`Blocked { ticks }` at a multiple of
  `REPLAN_BLOCKED_TICKS` = 10 triggers a replan); village `move_arrival_system`; testbed walk; `npc_command`
  (`exec.rs`, `approach.rs`: fail after `block_fail_ticks`); glue `command_vehicles`; `npc_vehicles/drive.rs`;
  `apps/rts_units_demo`.
- **Removed by:** `clear_move_intent` (together with the intent).

## Hashing and snapshots

- **State hash:** only when `register_locomotion_hash(app)` is called, as `move_status`. `NavPlugin` and
  `npc_command`'s `register_command_hash` (called by `CommandPlugin::build`, together with
  `register_spatial_hash`) call it, and so does `npc_vehicles`' `register_vehicle_hash` (`VehiclesPlugin`), so it is
  hashed whenever `NavPlugin`, `with_command()` or `with_vehicles()` is on. Encoding: u8 discriminant (Idle 0, Moving 1, Arrived 2,
  Blocked 3), then `ticks` (u32) for `Blocked`.
- **World snapshot (ADR 0013):** always saved as `move_status` (`register_snapshot_component_named` in
  `NpcLocomotionPlugin`).

## Related

- [`MoveIntent`](MoveIntent.md) — the input.
- [`NavState`](NavState.md) — the grid mover's view: `Pending` shows as `Moving`, `Failed` as `Blocked`.

## Example

```rust
// NpcSet::Locomotion, .after(NpcMoverSet): finish the leaf the tick the mover arrives.
pub fn move_arrival_system(mut walkers: Query<(&MoveStatus, &mut ActiveLeaf), With<MoveIntent>>) {
    for (status, mut leaf) in &mut walkers {
        match *status {
            MoveStatus::Arrived => leaf.status = Status::Success,
            MoveStatus::Blocked { ticks } if ticks >= 40 => leaf.status = Status::Failure,
            _ => {}
        }
    }
}
```
