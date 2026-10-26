---
type: Component
---
# `Interactable`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/targets.rs`](../../../crates/npc_interaction/src/targets.rs) · **Capability:** interaction (`InteractionPlugin`)

The verbs a target advertises. Built-in verbs work on targets without it (an item can be picked up, a door
opened). When a target **has** it, only the listed verbs apply to it: `check_def` refuses others with
`"#12 does not offer close"`. Content verbs usually require it in their `offer` filter
(`Interactable::offers`). It also makes the agent glue list the entity as an interaction candidate.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `verbs` | `Vec<String>` | verb names, sorted and unique (`Interactable::new` sorts and dedups) |

## Lifecycle

- **Added by:** content spawns. No framework code inserts it.
- **Read by:** `check_def` ([`run.rs`](../../../crates/npc_interaction/src/run.rs)); `may_pass` (a closed door
  whose `Interactable` lacks `open` blocks nav, [`door.rs`](../../../crates/npc_interaction/src/door.rs));
  glue `is_candidate` / affordances ([`interaction_agent.rs`](../../../crates/npc_framework/src/glue/interaction_agent.rs)),
  `vehicles_agent.rs` (a vehicle with `Interactable` is left to the interaction provider),
  `command_interaction.rs`.
- **Written / removed by:** nothing in the framework.

## Hashing and snapshots

- **State hash:** yes, as `interactable` (`register_interaction_hash`, [`hash.rs`](../../../crates/npc_interaction/src/hash.rs)):
  length-prefixed verb strings.
- **World snapshot (ADR 0013):** saved as `interactable` (automatic with hashing).

## Related

- [`Door`](Door.md), [`ToolTarget`](ToolTarget.md), [`Item`](Item.md) — targets of the built-in verbs.

## Example

```rust
use npc_framework::interaction::{Door, Interactable};

// A door that can be opened but never closed again.
spawn_sim(world, (SimPos::new(0, 4000), Door::closed(), Interactable::new(["open"])));
```
