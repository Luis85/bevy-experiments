---
type: Component
---
# `Item`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/items.rs`](../../../crates/npc_interaction/src/items.rs) · **Capability:** interaction (`InteractionPlugin`, facade feature `interaction`)

An item is a sim entity: `stack` units of a registered item kind, of some quality. It is either **in the
world** (it has a `SimPos` and the spatial grid indexes it) or **carried** (no `SimPos`; [`CarriedBy`](CarriedBy.md)
names the holder, whose [`Inventory`](Inventory.md) or [`Equipment`](Equipment.md) lists its `SimId`).
The kind (`ItemKindDef`, registered with `InteractionAppExt::register_item_kind`) gives the weight per unit
in grams, stacking, content tags (`"tool:axe"`, `"key:gate"`) and the equipment slot.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `kind` | `String` | registered `ItemKindDef` name; an unknown kind weighs 0 and cannot be stored |
| `stack` | `u32` | units in this stack (`Item::stack(n)` clamps to ≥ 1) |
| `quality` | `i32` | content-defined, 0 = standard; crafting sets it from the roll margin |

## Lifecycle

- **Added by:** content spawns (`spawn_sim(world, (pos, Item::new(kind)))`, e.g. testbed `spawn_item` in
  [`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)); `use_tool_on`
  yields (`use_tool_effect`, [`builtin.rs`](../../../crates/npc_interaction/src/builtin.rs)); craft outputs
  (`spawn_outputs`, [`crates/npc_crafting/src/run.rs`](../../../crates/npc_crafting/src/run.rs)).
- **Read by:** every verb's checks (`can_store`, `carried_matching`, `merge_target`); `door_pass_cost` /
  [`auto_door_system`](../systems/auto_door_system.md) / [`key_replan_system`](../systems/key_replan_system.md)
  (keys by kind or tag); `can_craft` (inputs and tools); the agent glue `inventory_section` and affordances
  ([`interaction_agent.rs`](../../../crates/npc_framework/src/glue/interaction_agent.rs)); the vehicles
  dismount-spot search (items do not block a cell).
- **Written / removed by:** `pick_up` merges a stack into a carried one of the same kind and quality and
  despawns the picked item; crafting `consume` lowers `stack` and despawns at 0; a critical failure lowers a
  tool's `quality` when it has no [`Durability`](Durability.md); `wear_tool` despawns broken tools.
- **On leave:** `items::on_sim_id_leave` drops a leaving holder's items where it stood (journal `ItemMoved`).

## Hashing and snapshots

- **State hash:** yes, as `item` (`register_sim_component` in `register_interaction_hash`,
  [`hash.rs`](../../../crates/npc_interaction/src/hash.rs)): `kind`, `stack`, `quality`.
- **World snapshot (ADR 0013):** saved as `item` (automatic with hashing).

## Related

- [`CarriedBy`](CarriedBy.md), [`Inventory`](Inventory.md), [`Equipment`](Equipment.md) — where it is held.
- [`Durability`](Durability.md) — optional wear on tools.
- [`Door`](Door.md) — keys are items whose kind or tag equals the door's key string.

## Example

```rust
use npc_framework::interaction::{InteractionAppExt, Item, ItemKindDef};

app.register_item_kind(ItemKindDef::new("axe").weight_g(1500).tag("tool:axe").equip_slot("main_hand"));
app.register_item_kind(ItemKindDef::new("log").weight_g(4000).stackable(10));

spawn_sim(world, (SimPos::new(2000, 0), Item::new("log").stack(2)));
```
