---
type: Component
---
# `SimPos`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/math.rs`](../../../crates/npc_kernel/src/math.rs) · **Capability:** kernel

Position on the ground plane in integer millimetres. It is the one sim position: movers write it, sensors and
the spatial grid read it, the view interpolates towards it. No floats ever touch it.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `x` | `i32` | millimetres, east (+X) |
| `z` | `i32` | millimetres, south (Bevy's +Z) |

JSON: `{"x":0,"z":0}`. In commands positions are `[x, z]` and must lie within ±`POS_LIMIT` (1 000 000 mm)
(`check_pos` in [`commands.rs`](../../../crates/npc_kernel/src/commands.rs)).

Helpers: `SimPos::new(x, z)`, `dist2(other) -> i64` (mm², saturates at `i64::MAX`), and
`step_towards(goal, step_mm)` (integer `isqrt`, truncates towards 0, returns `goal` exactly when within one step).

## Lifecycle

- **Added by:** content spawn bundles via `spawn_sim`; snapshot load.
- **Read by:** [`copy_prev_pos_system`](../systems/copy_prev_pos_system.md), [`spatial_update_system`](../systems/spatial_update_system.md)
  (`Changed<SimPos>` re-indexes), perception, nav, interaction, vehicles, content actions, the view.
- **Written by:** movers in `NpcSet::Locomotion` (`npc_locomotion::StraightLineMover`, `npc_nav` grid mover)
  from a `MoveIntent`; in `DeterminismMode::Relaxed` also [`apply_host_positions_system`](../systems/apply_host_positions_system.md)
  from [`HostPosition`](HostPosition.md). Actions should write a `MoveIntent`, not `SimPos`.

## Hashing and snapshots

- **State hash:** yes, as `pos` (`register_sim_component::<SimPos>()` in
  [`schedule.rs`](../../../crates/npc_kernel/src/schedule.rs); encoding `i32 x`, `i32 z` in
  [`hash.rs`](../../../crates/npc_kernel/src/hash.rs)).
- **World snapshot (ADR 0013):** saved as `pos` (automatic with hashing).
- Reflect-registered, type path `npc_sim::math` (BRP compatibility).

## Related

- [`PrevSimPos`](PrevSimPos.md) — last tick's value, for render interpolation.
- [`HostPosition`](HostPosition.md) — host-owned position in Relaxed mode.
- [`GridBlocker`](GridBlocker.md) / [`GridOccluder`](GridOccluder.md) — footprints are centred on `SimPos`.

## Example

```rust
let pos = SimPos::new(0, 0);
let next = pos.step_towards(SimPos::new(3000, 4000), 1000); // 1 m along the line
assert_eq!(next, SimPos::new(600, 800));
assert_eq!(pos.dist2(SimPos::new(3000, 4000)), 25_000_000);
```
