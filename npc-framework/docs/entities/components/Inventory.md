---
type: Component
---
# `Inventory`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/items.rs`](../../../crates/npc_interaction/src/items.rs) · **Capability:** interaction (`InteractionPlugin`)

What a unit can carry: at most `slots` items (a stack counts once) and `max_weight_g` grams in total,
counting inventory **plus** equipment. `items` holds item `SimId`s kept sorted, so every walk over a
holder's items is in `SimId` order. Change it only through the verbs or the helpers `put_in_inventory`,
`take_from_holder`, `place_in_world`, `put_in_slot` (same file); they keep [`CarriedBy`](CarriedBy.md) in step.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `slots` | `u32` | max items in the inventory (equipment not counted) |
| `max_weight_g` | `u32` | grams; limit for inventory + equipment (`carried_weight`) |
| `items` | `Vec<SimId>` | carried items not in an equipment slot, ascending, unique |

## Lifecycle

- **Added by:** content spawns of actors (`Inventory::new(slots, max_weight_g)`, testbed units in
  [`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)). A target with an
  `Inventory` can receive `give`.
- **Read by:** `can_store` (capacity and weight checks for `pick_up`, `give`, craft outputs); `unequip_pre`;
  `can_craft` / `check_claimed` (inputs come from the inventory only; a crafter needs `Inventory` +
  [`Interactor`](Interactor.md)); [`key_replan_system`](../systems/key_replan_system.md) (`Changed<Inventory>`);
  glue `inventory_section` ([`interaction_agent.rs`](../../../crates/npc_framework/src/glue/interaction_agent.rs)),
  `crafting_agent.rs`, `vehicles_agent.rs`; `interaction_json` (inspect).
- **Written / removed by:** `put_in_inventory`, `take_from_holder` (all verbs, crafting outputs/consume);
  `items::on_sim_id_leave` drops the id of a carried item that leaves the sim.

## Hashing and snapshots

- **State hash:** yes, as `inventory` (`register_interaction_hash`, [`hash.rs`](../../../crates/npc_interaction/src/hash.rs)):
  `slots`, `max_weight_g`, then the length-prefixed id list.
- **World snapshot (ADR 0013):** saved as `inventory` (automatic with hashing). The snapshot also saves the
  resource `key_replan_pending` (which inventories changed since `key_replan_system` last ran) and marks them
  changed again after a load.

## Related

- [`Equipment`](Equipment.md) — the other half of what a unit carries.
- [`Item`](Item.md), [`CarriedBy`](CarriedBy.md) — the carried entities and their back-link.

## Example

```rust
use npc_framework::interaction::{Equipment, Interactor, Inventory};

spawn_sim(world, (SimPos::new(0, 0), Interactor, Inventory::new(8, 30_000), Equipment::hands()));
```
