---
type: Component
---
# `Crafting`

**Crate:** `npc_crafting` · **File:** [`crates/npc_crafting/src/run.rs`](../../../crates/npc_crafting/src/run.rs) · **Capability:** crafting (`CraftingPlugin`)

A running craft on its crafter (`SparseSet`). `begin_craft` (called by the `Craft` leaf, the `Craft`
command and order picking) checks with `can_craft`, claims the workstation and every planned tool and input
in `Reservations` (capacity 1 each), inserts this and journals `CraftStarted`. When `ends <= tick`,
[`crafting_complete_system`](../systems/crafting_complete_system.md) re-checks the **claimed** items
(`check_claimed`), rolls the skill and applies the result. While present the unit is busy for crafting and,
through the facade's `crafting_interaction` glue (busy predicate `npc_crafting`), for interactions too.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `recipe` | `String` | `RecipeDef` name |
| `workstation` | `Option<SimId>` | the claimed workstation (`None` for station-less recipes) |
| `claimed` | `Vec<SimId>` | claimed tool and input items, ascending |
| `started` | `u64` | start tick |
| `ends` | `u64` | `started + recipe.duration_ticks` |
| `source` | `CraftSource` | `Tree` (leaf) or `Command` |
| `order` | `Option<u32>` | the workstation order it works on |
| `tools` | `Vec<SimId>` | planned tools, one per recipe tool key, in recipe order (what completion wears) |
| `inputs` | `Vec<(SimId, u32)>` | planned `(input item, units taken)`, in plan order (what completion consumes) |

## Lifecycle

- **Added by:** `begin_craft` ([`run.rs`](../../../crates/npc_crafting/src/run.rs)).
- **Read by:** `crafting_complete_system` (due crafts), `npc_crafting::busy_with`, glue `crafting_busy`
  ([`crafting_interaction.rs`](../../../crates/npc_framework/src/glue/crafting_interaction.rs)) and the agent
  busy predicate / section ([`crafting_agent.rs`](../../../crates/npc_framework/src/glue/crafting_agent.rs)),
  `craft_stop`, `crafting_json`, testbed view `anim.rs` / `overlay.rs`.
- **Written / removed by:** `complete_craft` (removes it first, then `CraftDone` / `CraftFailed` with outcome
  `failed`, `critical` or `invalid`), `cancel_craft` (`cancelled`, e.g. leaf abort). Both release every claim.

## Hashing and snapshots

- **State hash:** yes, as `crafting` (`register_crafting_hash`, [`hash.rs`](../../../crates/npc_crafting/src/hash.rs)):
  every field; source as `u8` (Tree 0, Command 1).
- **World snapshot (ADR 0013):** saved as `crafting` (automatic with hashing). `tools` / `inputs` are
  `#[serde(default)]`.

## Related

- [`CraftLeaf`](CraftLeaf.md), [`Workstation`](Workstation.md), [`CraftingOrders`](CraftingOrders.md), [`Durability`](Durability.md).
- [`Interacting`](Interacting.md) — `begin_craft` refuses while one runs.

## Example

```rust
use npc_framework::crafting::{CraftSource, Crafting, begin_craft, can_craft};

let plan = can_craft(world, smith, "horseshoe", Some(anvil))?;   // Err(CraftError) Display = why_not
begin_craft(world, smith, "horseshoe", Some(anvil), CraftSource::Command, None)?;
let ends = world.get::<Crafting>(smith).unwrap().ends;
```
