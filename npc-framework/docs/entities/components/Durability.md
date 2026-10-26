---
type: Component
---
# `Durability`

**Crate:** `npc_crafting` · **File:** [`crates/npc_crafting/src/station.rs`](../../../crates/npc_crafting/src/station.rs) · **Capability:** crafting (`CraftingPlugin`)

Wear on a tool (or any item): `current` of `max`. Every craft attempt wears each planned tool by
`CraftingConfig::wear_per_craft` (default 1), a critical failure by `crit_fail_wear` (default 10). At 0 the
tool breaks: it is taken from its holder and despawned; on a failed or critical attempt the `CraftFailed`
reason adds `tool #N broke` (a `CraftDone` does not mention it). A tool
**without** `Durability` never breaks; a critical failure lowers its `Item::quality` by 1 instead.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `current` | `u32` | wear points left (0 = broken) |
| `max` | `u32` | full durability (`Durability::new(max)` starts at `current = max`) |

## Lifecycle

- **Added by:** content, on tool items (`Durability::new(n)`). No framework or testbed code inserts it on
  this checkout.
- **Read by:** `wear_tool` ([`run.rs`](../../../crates/npc_crafting/src/run.rs)); `crafting_json` (inspect).
- **Written / removed by:** `wear_tool` in `complete_craft` (`current` saturating down). Interaction verbs
  (`use_tool_on`) do **not** wear tools.

## Hashing and snapshots

- **State hash:** yes, as `durability` (`register_crafting_hash`, [`hash.rs`](../../../crates/npc_crafting/src/hash.rs)):
  `current`, `max`.
- **World snapshot (ADR 0013):** saved as `durability` (automatic with hashing).

## Related

- [`Item`](Item.md) — the tool itself; its `quality` adds to the craft roll's modifier.
- [`Crafting`](Crafting.md) — `tools` lists what completion wears.
- [`crafting_complete_system`](../systems/crafting_complete_system.md).

## Example

```rust
use npc_framework::crafting::Durability;
use npc_framework::interaction::Item;

spawn_sim(world, (SimPos::new(0, 0), Item::new("hammer"), Durability::new(50)));
```
