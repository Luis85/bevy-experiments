---
type: Component
---
# `CraftingOrders`

**Crate:** `npc_crafting` · **File:** [`crates/npc_crafting/src/station.rs`](../../../crates/npc_crafting/src/station.rs) · **Capability:** crafting (`CraftingPlugin`)

The work posted on a [`Workstation`](Workstation.md), in posting order: "make `recipe` `remaining` more
times". A successful craft started from an order counts it down by one; at 0 the order is removed.
`crafting_jobs` lists every open order of every workstation (by workstation `SimId`, then order id): the
crafting job board. `next_order_for` picks the first order an agent can craft now.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `orders` | `Vec<CraftOrder>` | open orders in posting order; `CraftOrder { id: u32, recipe: String, remaining: u32 }` |
| `next_id` | `u32` | id the next posted order gets (saturating increment) |

At most `MAX_CRAFT_ORDERS` (64) open orders per workstation (`TooManyOrders` otherwise).

## Lifecycle

- **Added by:** `post_order` inserts it (default) on the first order; called by the `PostCraftOrder` command
  (`{"type":"PostCraftOrder","workstation":21,"recipe":"bread","count":3}`,
  [`command.rs`](../../../crates/npc_crafting/src/command.rs)). Content may also spawn it.
- **Read by:** `crafting_jobs`, `next_order_for` (the `Craft` leaf with `ParamValue::None`), `check_order`,
  `crafting_json`, glue `crafting_agent.rs` (affordances and the `crafting` section).
- **Written / removed by:** `post_order` (push), `count_down` (after a `CraftDone` with an order, in
  `complete_craft`, [`run.rs`](../../../crates/npc_crafting/src/run.rs)). Failed, critical or invalid crafts do
  not count down. The component is never removed (an empty list stays).

## Hashing and snapshots

- **State hash:** yes, as `crafting_orders` (`register_crafting_hash`, [`hash.rs`](../../../crates/npc_crafting/src/hash.rs)):
  per order `id`, `recipe`, `remaining`, then `next_id`.
- **World snapshot (ADR 0013):** saved as `crafting_orders` (automatic with hashing).

## Related

- [`Workstation`](Workstation.md), [`Crafting`](Crafting.md) (`order` field), [`CraftLeaf`](CraftLeaf.md).
- [`crafting_complete_system`](../systems/crafting_complete_system.md) — counts orders down.

## Example

```rust
use npc_framework::crafting::{crafting_jobs, post_order};

let order = post_order(world, oven_id, "bread", 3)?;   // sim code only
for job in crafting_jobs(world) { println!("#{} {} x{}", job.workstation.0, job.recipe, job.remaining); }
```
