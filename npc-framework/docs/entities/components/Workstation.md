---
type: Component
---
# `Workstation`

**Crate:** `npc_crafting` · **File:** [`crates/npc_crafting/src/station.rs`](../../../crates/npc_crafting/src/station.rs) · **Capability:** crafting (`CraftingPlugin`, needs `InteractionPlugin` + `RulesPlugin`)

A smart object that hosts recipes. It says which workstation kinds it counts as; a `RecipeDef` with a
non-empty `stations` list can only be crafted at a workstation of one of those kinds. Like other smart
objects it can carry `npc_jobs::Approach` (where to stand), and `Reservations` record who uses it: a craft
claims it with capacity 1 for its whole duration, so two crafters contend for it. Colony work is posted on it
as [`CraftingOrders`](CraftingOrders.md).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `kinds` | `Vec<String>` | kinds it counts as (e.g. `["anvil", "forge"]`), sorted and unique (`Workstation::new`) |

## Lifecycle

- **Added by:** content (testbed `station(..)` in
  [`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** `check_recipe_and_station` (`can_craft`, `check_claimed`: right kind, within
  `CraftingConfig::reach_mm` (default 1500 mm), not in use), `check_order` / `post_order`
  ([`station.rs`](../../../crates/npc_crafting/src/station.rs)); the inspect kind namer (`"workstation"`);
  `crafting_json`; glue `known_workstations` / `nearby_workstations` / `crafting_affordances`
  ([`crafting_agent.rs`](../../../crates/npc_framework/src/glue/crafting_agent.rs)).
- **Written / removed by:** nothing in the framework.

## Hashing and snapshots

- **State hash:** yes, as `workstation` (`register_crafting_hash`, [`hash.rs`](../../../crates/npc_crafting/src/hash.rs)):
  length-prefixed kind strings.
- **World snapshot (ADR 0013):** saved as `workstation` (automatic with hashing).

## Related

- [`CraftingOrders`](CraftingOrders.md) — work posted on it.
- [`Crafting`](Crafting.md) — `workstation` names it while a craft runs.
- [`CraftLeaf`](CraftLeaf.md) — reads the workstation from blackboard slot `InteractionConfig::target_slot`.

## Example

```rust
use npc_framework::crafting::Workstation;
use npc_framework::jobs::Approach;

spawn_sim(world, (SimPos::new(-3000, 6000), Workstation::new(["oven"]), Approach(SimPos::new(-3000, 5000))));
// {"type":"Craft","unit":5,"recipe":"bread","workstation":21}
```
