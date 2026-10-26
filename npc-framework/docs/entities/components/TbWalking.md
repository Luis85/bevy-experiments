---
type: Component
---
# `TbWalking`

**Crate:** `testbed_content` · **File:** [`content/testbed_content/src/actions.rs`](../../../content/testbed_content/src/actions.rs) · **Capability:** testbed content (actions)

The running `TbWalk` action: walk to the `Pos` in blackboard slot `BB_DEST` (`BbKey(0)`) through the
locomotion seam. Actors are `NavAgent`s, so they path-find and open doors they may open. `SparseSet` storage.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `goal` | `SimPos` | destination, mm |

The tolerance (leaf param `Int`, mm, default 0) goes into the `MoveIntent.tolerance_mm`, not here.

## Lifecycle

- **Added by:** `walk_start` in [`actions.rs`](../../../content/testbed_content/src/actions.rs), with
  `MoveIntent { goal, speed_mm_per_tick: TestbedUnit speed, tolerance_mm }`. A start already within
  tolerance returns `Success` and adds nothing. Tree params: `WalkToWork` 1000, `WalkToSpot` 300.
- **Read by:** [`walk_cancel_system`](../systems/walk_cancel_system.md) and
  [`walk_arrival_system`](../systems/walk_arrival_system.md).
- **Removed by:** `walk_stop` (takes it and, if it was there, clears the move intent).

## Hashing and snapshots

- **State hash:** yes, as `tb_walking` (`goal.sim_hash`; `register_sim_component` in `TestbedPlugin`).
- **World snapshot (ADR 0013):** saved as `tb_walking` (automatic with hashing).

## Related

- [`MoveIntent`](MoveIntent.md), [`MoveStatus`](MoveStatus.md) — the seam it drives.
- [`TestbedUnit`](TestbedUnit.md) — the speed.
- [`MoveTo`](MoveTo.md) — the village equivalent.

## Example

```rust
// tree.rs: the wander branch.
b.sequence("Wander", |b| {
    b.leaf("PickSpot", TB_PICK_WANDER, ());
    b.leaf("WalkToSpot", TB_WALK, 300); // arrive within 300 mm
    b.leaf("Linger", TB_IDLE, 60);
});
```
