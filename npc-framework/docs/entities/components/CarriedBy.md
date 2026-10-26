---
type: Component
---
# `CarriedBy`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/items.rs`](../../../crates/npc_interaction/src/items.rs) · **Capability:** interaction (`InteractionPlugin`)

The back-link on a carried [`Item`](Item.md): who holds it and which equipment slot it is in. A carried item
has no `SimPos` (the helpers remove `SimPos` and `PrevSimPos`); `position_of` resolves it through the
holder. Only the item helpers set or remove it, together with the holder's [`Inventory`](Inventory.md) /
[`Equipment`](Equipment.md).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `holder` | `SimId` | the unit carrying the item |
| `slot` | `Option<String>` | equipment slot name; `None` = in the inventory |

## Lifecycle

- **Added by:** `put_in_inventory` (`pick_up`, `give`, `unequip`, craft outputs) and `put_in_slot` (`equip`)
  in [`items.rs`](../../../crates/npc_interaction/src/items.rs).
- **Read by:** `holder_of` / `position_of` (reach checks on carried targets), `can_store`, `equip_offer` /
  `unequip_offer` / `give_pre` / `tool_for` ([`builtin.rs`](../../../crates/npc_interaction/src/builtin.rs)),
  `can_craft` / `check_claimed` (inputs must not be equipped; claimed items must still be carried),
  glue `interaction_agent.rs` and `command_interaction.rs`, testbed view (`visuals.rs`, hides carried items).
- **Written / removed by:** `take_from_holder` and `place_in_world` (`drop`, outputs that do not fit) remove
  it; `drop_orphans` removes it when the holder leaves the sim (the item gets the holder's last position).

## Hashing and snapshots

- **State hash:** yes, as `carried_by` (`register_interaction_hash`, [`hash.rs`](../../../crates/npc_interaction/src/hash.rs)):
  `holder`, optional `slot`.
- **World snapshot (ADR 0013):** saved as `carried_by` (automatic with hashing).

## Related

- [`Item`](Item.md), [`Inventory`](Inventory.md), [`Equipment`](Equipment.md).

## Example

```rust
use npc_framework::interaction::{CarriedBy, Item};

fn carried_by_me(q: &Query<(&SimId, &Item, &CarriedBy)>, me: SimId) -> Vec<SimId> {
    let mut v: Vec<SimId> = q.iter().filter(|(_, _, c)| c.holder == me).map(|(&id, _, _)| id).collect();
    v.sort_unstable();
    v
}
```
