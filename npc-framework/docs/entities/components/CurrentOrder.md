---
type: Component
---
# `CurrentOrder`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/orders.rs`](../../../crates/npc_kernel/src/orders.rs) · **Capability:** kernel (standing orders)

A unit's standing order from `SimCommand::Order`. A unit takes orders only when it has this component (content
usually `#[require(CurrentOrder)]`s it). What the unit does with the order is content: its tree guards a
"follow order" branch with a `has_order` condition and runs an action that walks there and clears it.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `0` | `Option<OrderKind>` | `None` = no order |

`OrderKind` variants:

| Variant | Fields | Meaning |
|---|---|---|
| `GoTo` | `pos: SimPos` | walk to `pos` (mm), then the order is done |

The command form is `Order` (`{"type":"GoTo","pos":[x,z]}` or `{"type":"Clear"}`); `Clear` stores `None`.

## Lifecycle

- **Added by:** content spawn bundles (e.g. village `Villager` has `#[require(CurrentOrder)]` in
  `content/village_content/src/world.rs`).
- **Written by:** `apply_order` (called by [`apply_commands_system`](../systems/apply_commands_system.md) for
  `SimCommand::Order`) inserts `(CurrentOrder(..), ThinkRequest)`, so the unit re-thinks the same tick;
  `clear_order` sets it to `None` (content actions when they arrive).
- **Read by:** `cond_has_order` (register it under your own condition name), content order actions
  (`content/village_content/src/orders.rs`), `npc_vehicles::drive` (a driver's `GoTo` becomes the vehicle's
  destination), the village inspector. Validation: `Order` on a unit without it fails with `WrongKind`.
- `npc_command`'s RTS orders (`IssueOrder`, `OrderQueue`) are separate and never write this component.

## Hashing and snapshots

- **State hash:** yes, as `order` (`register_sim_component::<CurrentOrder>()` in
  [`schedule.rs`](../../../crates/npc_kernel/src/schedule.rs); encoding: presence tag, `u8` 0, then `pos`).
- **World snapshot (ADR 0013):** saved as `order` (automatic with hashing).

## Related

- [`ThinkRequest`](ThinkRequest.md) — inserted with every new order.
- [`SimPos`](SimPos.md) — the order's target.

## Example

```rust
// Command JSON (npcctl --commands, BRP npc/command):
// {"type":"Order","unit":5,"order":{"type":"GoTo","pos":[1000,2000]}}
app.register_cond("has_order", npc_framework::kernel::orders::cond_has_order);
```
