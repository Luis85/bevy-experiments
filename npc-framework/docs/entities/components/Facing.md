---
type: Component
---
# `Facing`

**Crate:** `npc_perception` · **File:** [`crates/npc_perception/src/sensors.rs`](../../../crates/npc_perception/src/sensors.rs) · **Capability:** perception (opt-in, `PerceptionPlugin`)

The direction an agent looks in: a ground-plane vector in millimetres, of any length. Vision cones are centred
on it. The default is `+z` (`{x: 0, z: 1}`). Movement updates it at the end of every tick unless the entity
has [`FixedFacing`](FixedFacing.md).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `x` | `i32` | x component, mm (only the direction matters) |
| `z` | `i32` | z component, mm |

`Facing::towards(from, to)` builds the vector `to - from`, reduced so each axis is at most 2¹⁵; equal points
give the default. `reduced()` returns that shrunk vector. `in_fov` uses it so its `i128` math cannot overflow.

## Lifecycle

- **Added by:** required by [`Sensors`](Sensors.md) (`#[require(Facing, PerceptionMemory)]`), so every
  observer gets the default. Content may insert its own: the testbed's `spawn_unit` sets
  `Facing::new(x, z)` from the unit spec's `facing`
  ([`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** [`perception_system`](../systems/perception_system.md) (gathered with `Sensors` at the start of
  the pass, then used by the vision FOV test); `perception_json` (inspect). View only:
  `npc_view_helpers::perception_gizmos`, `testbed_view` panels and visuals.
- **Written by:** [`facing_from_movement_system`](../systems/facing_from_movement_system.md) in
  `NpcSet::Cleanup`. When `SimPos != PrevSimPos` it sets `Facing::towards(prev, pos)`, and only writes when the
  value changes. Hosts or content write it directly on entities with `FixedFacing`.

## Hashing and snapshots

- **State hash:** yes, as `facing` (`register_sim_component` in `register_perception_hash`,
  [`crates/npc_perception/src/lib.rs`](../../../crates/npc_perception/src/lib.rs)). Encoding: `x`, `z` as `i32`.
- **World snapshot (ADR 0013):** saved as `facing` (automatic with hashing).

## Related

- [`FixedFacing`](FixedFacing.md) — stops movement from turning the agent.
- [`Sensors`](Sensors.md) — its vision cones use this direction.

## Example

```rust
use npc_framework::perception::{Facing, FixedFacing, Sensors};

// A guard that looks east and keeps looking east while it walks.
let (_, guard) = spawn_sim(world, (SimPos::new(0, 0), Sensors::new().vision(10_000, 45)));
world.entity_mut(guard).insert((Facing::new(1, 0), FixedFacing));
```
