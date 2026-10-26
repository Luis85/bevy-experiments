---
type: Component
---
# `Interactor`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/items.rs`](../../../crates/npc_interaction/src/items.rs) · **Capability:** interaction (`InteractionPlugin`)

Marks a unit that can perform interactions: verbs, the `Interact` leaf and command, and opening doors on its
nav path. Without it `check_def` refuses every verb (`"#5 cannot interact"`). Crafting also requires it (plus
an [`Inventory`](Inventory.md)), and vehicles need it on riders (`mount` is an interaction verb).

## Fields

Marker component, no fields.

## Lifecycle

- **Added by:** content spawns of actors (testbed units in
  [`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** `check_def` ([`run.rs`](../../../crates/npc_interaction/src/run.rs)); the `Interact` command's
  `validate` (`entity_with::<Interactor>`, [`command.rs`](../../../crates/npc_interaction/src/command.rs));
  `may_pass` / `door_pass_cost` (only an `Interactor` may pass a closed door);
  [`auto_door_system`](../systems/auto_door_system.md) and [`key_replan_system`](../systems/key_replan_system.md)
  (`With<Interactor>` + `NavAgent`); `can_craft` (`NotACrafter` without it); `npc_vehicles` commands;
  facade glue `interaction_agent.rs`, `crafting_agent.rs`, `vehicles_agent.rs`, `command_interaction.rs`,
  `command_vehicles.rs` (who gets affordances / orders).
- **Written / removed by:** nothing in the framework.

## Hashing and snapshots

- **State hash:** yes, as `interactor` (`register_interaction_hash`, [`hash.rs`](../../../crates/npc_interaction/src/hash.rs));
  empty encoding, only presence is hashed.
- **World snapshot (ADR 0013):** saved as `interactor` (automatic with hashing).

## Related

- [`Inventory`](Inventory.md), [`Equipment`](Equipment.md) — usually spawned with it.
- [`Interacting`](Interacting.md) — what an interactor is doing now.

## Example

```rust
use npc_framework::interaction::{Equipment, Interactor, Inventory};

spawn_sim(world, (SimPos::new(0, 0), NavAgent, Interactor, Inventory::new(6, 25_000), Equipment::hands()));
```
