---
type: Component
---
# `FixedFacing`

**Crate:** `npc_perception` · **File:** [`crates/npc_perception/src/sensors.rs`](../../../crates/npc_perception/src/sensors.rs) · **Capability:** perception (opt-in, `PerceptionPlugin`)

Marker: movement does not turn this agent. The host or content sets its [`Facing`](Facing.md), for example a
sentry that walks a patrol but always watches one door.

## Fields

Marker component, no fields.

## Lifecycle

- **Added by:** content. The testbed's `spawn_unit` inserts it when the unit spec has `fixed_facing: true`
  ([`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** [`facing_from_movement_system`](../systems/facing_from_movement_system.md), as a
  `Without<FixedFacing>` query filter.
- **Written / removed by:** nothing in the framework.

## Hashing and snapshots

- **State hash:** yes, as `fixed_facing` (`register_sim_component` in `register_perception_hash`,
  [`crates/npc_perception/src/lib.rs`](../../../crates/npc_perception/src/lib.rs)). It hashes no bytes; the
  registry's presence tag records whether it is there.
- **World snapshot (ADR 0013):** saved as `fixed_facing` (automatic with hashing).

## Related

- [`Facing`](Facing.md) — the direction this marker freezes.

## Example

```rust
use npc_framework::perception::{Facing, FixedFacing};

world.entity_mut(sentry).insert((Facing::new(0, -1), FixedFacing)); // always faces -z
```
