---
type: Component
---
# `ExecutingOrder`

**Crate:** `npc_command` · **File:** [`crates/npc_command/src/types.rs`](../../../crates/npc_command/src/types.rs) · **Capability:** command (RTS orders)

Marker: this `Brain` unit's tree is running the `ExecuteOrder` leaf, so the command engine runs its orders.
Units without a `Brain` never need it. Storage is `SparseSet` (it comes and goes with the leaf).

## Fields

Marker component, no fields.

## Lifecycle

- **Added by:** `execute_order_start` (the `ExecuteOrder` leaf's start,
  [`crates/npc_command/src/leaf.rs`](../../../crates/npc_command/src/leaf.rs)), when the unit holds an order.
  A `Direct` agent-controlled unit gets the leaf through the `execute_orders` / `interact` handlers of the
  command × agent-io glue.
- **Read by:** [`command_engine_system`](../systems/command_engine_system.md): a `Brain` unit is stepped only
  `Has<ExecutingOrder>`; `inspect::command_json` (`executing` field).
- **Removed by:** `execute_order_start` when the queue ran dry during start; `execute_order_stop` (any stop).
  An `Aborted` stop also calls `pause_orders` (movement stops; an executor's order is cancelled and restarts
  on resume).

## Hashing and snapshots

- **State hash:** yes, as `executing_order` (`register_sim_component` in `register_command_hash`,
  [`crates/npc_command/src/hash.rs`](../../../crates/npc_command/src/hash.rs)); presence only, no payload.
- **World snapshot (ADR 0013):** saved as `executing_order` (automatic with hashing).

## Related

- [`OrderQueue`](OrderQueue.md) — the orders it lets run.
- [`command_engine_system`](../systems/command_engine_system.md) — finishes the leaf (`Success`, or `Failure`
  when `last_ok` is false) once the queue is empty.
- [`external_action_system`](../systems/external_action_system.md) — handles an `ExecuteOrder` step that the
  engine finished before the think.

## Example

```rust
use npc_framework::command::{EXECUTE_ORDER_ACTION, HAS_UNIT_ORDER};

// The tree branch that makes orders win over lower work:
b.guard("Orders", HAS_UNIT_ORDER, Abort::LowerPriority, |b| {
    b.leaf("ExecuteOrder", EXECUTE_ORDER_ACTION, ());
});
```
