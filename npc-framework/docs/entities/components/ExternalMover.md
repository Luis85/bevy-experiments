---
type: Component
---
# `ExternalMover`

**Crate:** `npc_locomotion` · **File:** [`crates/npc_locomotion/src/lib.rs`](../../../crates/npc_locomotion/src/lib.rs) · **Capability:** locomotion

A marker that says a mover other than the `StraightLineMover` owns this agent's
[`MoveIntent`](MoveIntent.md)s. [`straight_line_mover_system`](../systems/straight_line_mover_system.md)
queries `Without<ExternalMover>`, so marked agents are skipped. Use it for host adapters (physics, navmesh)
and for the grid path mover.

## Fields

Marker component, no fields.

## Lifecycle

- **Added by:** [`NavAgent`](NavAgent.md) through `#[require(ExternalMover, NavState)]`, or by a host adapter
  that inserts it on the agents it moves. The adapter's systems run in `NpcMoverSet`, read `MoveIntent`,
  write `SimPos` and [`MoveStatus`](MoveStatus.md).
- **Read by:** [`straight_line_mover_system`](../systems/straight_line_mover_system.md) (as a `Without` filter).
- **Removed by:** nothing in the framework.

## Hashing and snapshots

- **State hash:** no. It has no `SimHash` impl, and `register_locomotion_hash` hashes only `move_intent` and
  `move_status`. For a `NavAgent` its effect is still covered, because `nav_agent` is hashed.
- **World snapshot (ADR 0013):** saved as `external_mover` (snapshot-only, `register_snapshot_component_named`
  in `NpcLocomotionPlugin`), so a loaded agent keeps its mover choice.

## Related

- [`NavAgent`](NavAgent.md) — requires it.
- [`MoveIntent`](MoveIntent.md), [`MoveStatus`](MoveStatus.md) — the seam the external mover serves.

## Example

```rust
use npc_framework::locomotion::{ExternalMover, MoveIntent, MoveStatus, NpcMoverSet};

// A host-owned mover: physics moves these agents, not the StraightLineMover.
app.add_systems(NpcSimTick, my_physics_mover.in_set(NpcMoverSet));

fn my_physics_mover(mut q: Query<(&mut SimPos, &MoveIntent, &mut MoveStatus), With<ExternalMover>>) {
    for (mut pos, intent, mut status) in &mut q {
        // ... integer step, then:
        status.set_if_neq(if intent.reached(*pos) { MoveStatus::Arrived } else { MoveStatus::Moving });
    }
}
```

Note: a host adapter that queries `With<ExternalMover>` also matches `NavAgent`s. Filter them out
(`Without<NavAgent>`) when both run in one app. A host that owns every agent can instead
call `.disable::<StraightLineMover>()` on `NpcPlugins::new().build()` (`PluginGroup` itself has no `disable`).
