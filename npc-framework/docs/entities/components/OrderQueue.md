---
type: Component
---
# `OrderQueue`

**Crate:** `npc_command` · **File:** [`crates/npc_command/src/types.rs`](../../../crates/npc_command/src/types.rs) · **Capability:** command (RTS orders)

A unit's RTS orders: the one it is running and the shift-queue behind it (front = next). It is written only by
the order commands and the command engine. `IssueOrder` with `queue: false` replaces everything (the running
order is journaled `OrderFailed { reason: "replaced" }`, or `"stopped"` for a `Stop`); `queue: true` appends.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `current` | `Option<ActiveOrder>` | The running order: `order: QueuedOrder`, `started: Option<u64>` (tick it started), `progress: OrderProgress`. |
| `queued` | `Vec<QueuedOrder>` | Waiting orders, in execution order. |
| `last_ok` | `bool` | How the last finished order ended (`true` done, `false` failed); the `ExecuteOrder` leaf reports it when the queue runs dry. |

`QueuedOrder` is `{ order: UnitOrder, group: Option<GroupLink>, player: PlayerId, issued: u64 }` (tick issued).
`GroupLink { leader: SimId, offset: (i32, i32) }` is set for members of a group move (offset in mm from the
leader's slot). `OrderProgress` variants: `NotStarted`, `Moving`, `WaitingForLeader { since: u64 }`,
`Holding { at: SimPos }`, `Patrolling { route: Vec<SimPos>, next: u32 }`, `Hooked` (run by a registered executor).
Helpers: `has_order()`, `len()`, `is_empty()`, `orders()`. At most `CommandConfig::max_queue` (default 16) orders.

## Lifecycle

- **Added by:** required by [`Commandable`](Commandable.md), so it appears empty on every commandable unit.
- **Read by:** [`command_engine_system`](../systems/command_engine_system.md); the `has_unit_order` condition
  (`cond_has_unit_order`, [`leaf.rs`](../../../crates/npc_command/src/leaf.rs)); `IssueOrder` validation
  (queue limit); the command × vehicles glue (`crew_redirect` forwards only to a driver with a queue); the
  command × agent-io glue (`command` section, `execute_orders`, busy predicate `npc_command`); `command_json`;
  `npc_view_helpers::command_ui` (order gizmos); `testbed_content::summary`, `testbed_view` overlay.
- **Written by:** `enqueue_order` / `cancel_orders` (from `IssueOrder::apply` via `issue` / `issue_to`);
  `step_unit` and `pause_orders` (the engine and the `ExecuteOrder` leaf start/stop), in
  [`exec.rs`](../../../crates/npc_command/src/exec.rs). Never removed.

## Hashing and snapshots

- **State hash:** yes, as `order_queue` (`register_sim_component` in `register_command_hash`,
  [`crates/npc_command/src/hash.rs`](../../../crates/npc_command/src/hash.rs)). `UnitOrder` hashes its `u8`
  discriminant plus fields; a `Custom` order hashes its `args` as JSON text.
- **World snapshot (ADR 0013):** saved as `order_queue` (automatic with hashing).

## Related

- [`Commandable`](Commandable.md) — requires it.
- [`ExecutingOrder`](ExecutingOrder.md) — when a `Brain` unit's queue may run.
- [`InteractOrder`](InteractOrder.md) — per-unit state of a running `Interact` / `Mount` order (`progress: Hooked`).

## Example

```rust
// Shift-queue a patrol behind the unit's current orders, then read the queue back.
let cmd = IssueOrder::new(PlayerId(0), [SimId(5)],
    UnitOrder::Patrol { points: vec![(0, 0), (5000, 0)] }).queued(true);
submit_order(world, &cmd)?;
// ... after the command applied:
let q = world.get::<OrderQueue>(unit).unwrap();
let kinds: Vec<&str> = q.orders().map(UnitOrder::kind).collect(); // e.g. ["Move", "Patrol"]
```
