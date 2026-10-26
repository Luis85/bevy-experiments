---
type: Component
---
# `Equipment`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/items.rs`](../../../crates/npc_interaction/src/items.rs) · **Capability:** interaction (`InteractionPlugin`)

Named equipment slots on a unit (`"main_hand"`, `"off_hand"`, ..), each empty or holding one carried item.
The slot set belongs to the unit: `equip` only uses a slot that exists here, and the item kind's
`equip_slot` says which one. Tools for `use_tool_on` must be **equipped**; crafting tools may be equipped or
in the inventory (equipped preferred).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `slots` | `BTreeMap<String, Option<SimId>>` | slot name → item `SimId` or `None` (empty); name order |

Helpers: `Equipment::new(names)`, `Equipment::hands()` (`MAIN_HAND`, `OFF_HAND`), `items()` (ascending ids),
`slot_of(id)`.

## Lifecycle

- **Added by:** content spawns of actors (testbed units get `Equipment::hands()` in
  [`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** `equip_pre` (slot exists and is empty), `tool_for` / `equipped_matching` (`use_tool_on`),
  `carried_matching` (keys), `carried_weight`, `can_craft` (tools, equipped first);
  [`key_replan_system`](../systems/key_replan_system.md) (`Changed<Equipment>`); glue `inventory_section`
  ([`interaction_agent.rs`](../../../crates/npc_framework/src/glue/interaction_agent.rs)); `interaction_json`.
- **Written / removed by:** `put_in_slot` (`equip`), `take_from_holder` (any move out, `unequip`, crafting
  consume/wear), `items::on_sim_id_leave` (a carried item that leaves the sim is cleared from its slot).

## Hashing and snapshots

- **State hash:** yes, as `equipment` (`register_interaction_hash`, [`hash.rs`](../../../crates/npc_interaction/src/hash.rs)):
  count, then per slot (name order) the name and optional id.
- **World snapshot (ADR 0013):** saved as `equipment` (automatic with hashing); change ticks for
  `key_replan_system` are carried by the `key_replan_pending` snapshot resource.

## Related

- [`Inventory`](Inventory.md) — weight limit covers both; `unequip` needs a free inventory slot.
- [`CarriedBy`](CarriedBy.md) — an equipped item has `slot: Some(name)`.
- [`ToolTarget`](ToolTarget.md) — needs an equipped item tagged `required_tag`.

## Example

```rust
use npc_framework::interaction::{Equipment, Interact};

let eq = Equipment::new(["main_hand", "off_hand", "back"]);
// Equip a carried axe (its kind's equip_slot is "main_hand"):
// {"type":"Interact","unit":5,"verb":"equip","target":42}
let cmd = Interact::new(unit, "equip", axe);
```
