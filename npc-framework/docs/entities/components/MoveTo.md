---
type: Component
---
# `MoveTo`

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Capability:** village content (actions)

The hashed record of a villager's running walk. The movement itself goes through the locomotion seam: the
start also inserts a `MoveIntent` (at `Villager.speed`) that `StraightLineMover`, or `GridPathMover` for
`NavAgent`s, follows. `SparseSet` storage, like every action component.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `goal` | `SimPos` | destination, mm (the `BB_DEST` position, or the order's `GoTo` position) |

## Lifecycle

- **Added by:** `start_walk` (`MoveTo { goal }` + `set_move_intent(MoveIntent::to(goal, speed))`), called by
  `move_to_dest_start` (leaf `MoveToDest`) and `follow_order_start` (leaf `MoveToOrder`, action
  `FollowOrder`). A start at the goal already returns `Success` and adds nothing.
- **Read by:** [`move_to_system`](../systems/move_to_system.md) and
  [`move_arrival_system`](../systems/move_arrival_system.md) (filter `(With<MoveTo>, With<MoveIntent>)`).
- **Written by:** [`order_system`](../systems/order_system.md) re-targets `goal` (and the intent and
  `BB_DEST`) when the `GoTo` order changes.
- **Removed by:** `end_walk` (takes `MoveTo`, clears the intent) from `stop_move` / `stop_follow_order`.

## Hashing and snapshots

- **State hash:** yes, as `move_to` (`goal.sim_hash`; `register_sim_component` in `register_components`,
  [`lib.rs`](../../../content/village_content/src/lib.rs)).
- **World snapshot (ADR 0013):** saved as `move_to` (automatic with hashing).

## Related

- [`MoveIntent`](MoveIntent.md), [`MoveStatus`](MoveStatus.md) — the locomotion seam it drives.
- [`OrderMove`](OrderMove.md) — marks an order walk.
- [`Activity`](Activity.md) — `Walk` while it runs.

## Example

```rust
// Inside an action start (exclusive world access):
let speed = world.get::<Villager>(agent).map_or(0, |v| v.speed);
world.entity_mut(agent).insert(MoveTo { goal });
set_move_intent(world, agent, MoveIntent::to(goal, speed));
```
