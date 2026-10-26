---
type: Component
---
# `OrderMove`

**Crate:** `village_content` · **File:** [`content/village_content/src/orders.rs`](../../../content/village_content/src/orders.rs) · **Capability:** village content (RTS orders)

Marker: the agent's running [`MoveTo`](MoveTo.md) belongs to the `FollowOrder` action (walking to a kernel
`OrderKind::GoTo` position), not to a `MoveToDest` leaf. It lets `order_system` keep that walk in sync with
the order. `SparseSet` storage.

## Fields

Marker component, no fields.

## Lifecycle

- **Added by:** `follow_order_start` in [`orders.rs`](../../../content/village_content/src/orders.rs)
  (only when the villager is not already at the goal), together with `MoveTo`, a `MoveIntent` and
  `Activity::Walk`.
- **Read by:** [`order_system`](../systems/order_system.md) (`With<OrderMove>` filter).
- **Removed by:** `stop_follow_order` (on both `Finished` and `Aborted`).

## Hashing and snapshots

- **State hash:** yes, as `order_move` (presence only: `sim_hash` writes nothing, the registry adds the
  presence tag; `register_sim_component` in `register_components`,
  [`lib.rs`](../../../content/village_content/src/lib.rs)).
- **World snapshot (ADR 0013):** saved as `order_move` (automatic with hashing).

## Related

- [`MoveTo`](MoveTo.md) — the walk it marks.
- [`CurrentOrder`](CurrentOrder.md) — the kernel order it follows.
- [`Villager`](Villager.md) — requires `CurrentOrder`.

## Example

```rust
// Give villager 5 a GoTo order (recorded SimCommand); the tree's FollowOrder
// guard starts the walk and inserts OrderMove.
// npc/command: {"cmd":{"type":"Order","unit":5,"order":{"type":"GoTo","pos":[8000,8000]}}}
let walking_an_order = world.get::<OrderMove>(agent).is_some();
```
