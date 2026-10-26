---
type: Component
---
# `Interacting`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/run.rs`](../../../crates/npc_interaction/src/run.rs) · **Capability:** interaction (`InteractionPlugin`)

A running interaction on its actor (`SparseSet` storage). Every entry point (the `Interact` leaf, the
`Interact` command, the door-on-path behaviour, `npc_vehicles::mount`/`dismount`, RTS `Interact` orders)
goes through `run::begin`, which checks, reserves the target when the verb says so, inserts this and
journals `Interacted { outcome: "started" }`. [`complete_interactions_system`](../systems/complete_interactions_system.md)
removes it when `ends <= tick` and applies the verb. While it is present the unit is busy: `begin` refuses
new verbs (`"busy with open #12"`) and crafting refuses to start.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `verb` | `String` | registered verb name (`"open"`, `"use_tool_on"`, `"mount"`, ..) |
| `target` | `SimId` | the target entity |
| `item` | `Option<SimId>` | item argument, for verbs that take one |
| `started` | `u64` | tick it started (`SimClock.tick`) |
| `ends` | `u64` | tick its effect applies: `started + duration_for(..)` |
| `source` | `Source` | `Tree` (leaf or `Direct` agent queue), `Command`, `Auto` (door on path) |

## Lifecycle

- **Added by:** `run::begin` ([`run.rs`](../../../crates/npc_interaction/src/run.rs)), called by
  `leaf::interact_start`, `Interact::apply`, [`auto_door_system`](../systems/auto_door_system.md),
  `npc_vehicles::mount` / `dismount`, glue `command_interaction.rs`.
- **Read by:** `busy_with` (refuses new interactions); `npc_crafting::busy_with`;
  [`claim_seats_system`](../systems/claim_seats_system.md) (mounts in progress); `auto_door_system`
  (`Without<Interacting>`); glue `interaction_busy` / `inventory_section` (`interaction_agent.rs`),
  `command_interaction.rs` (order done?); testbed view `anim.rs` / `overlay.rs`; `interaction_json`.
- **Written / removed by:** `run::complete` (outcome `success` / `failed` / `invalid`) and `run::cancel`
  (`cancelled`: leaf abort, `cancel_interaction` in vehicles when no seat is left, `unload`, an RTS
  `Interact` order cancelled in glue `command_interaction.rs`). Both release
  the target reservation and call the finished hooks (`InteractionListeners`, by name).

## Hashing and snapshots

- **State hash:** yes, as `interacting` (`register_interaction_hash`, [`hash.rs`](../../../crates/npc_interaction/src/hash.rs)):
  verb, target, item, `started`, `ends`, source as `u8` (Tree 0, Command 1, Auto 2).
- **World snapshot (ADR 0013):** saved as `interacting` (automatic with hashing), so a saved mid-interaction
  unit finishes at the same tick after load.

## Related

- [`InteractLeaf`](InteractLeaf.md) — the BT action that started a `Tree` interaction.
- [`Crafting`](Crafting.md) — the other "busy" state; the `crafting_interaction` glue blocks verbs while it runs.

## Example

```rust
use npc_framework::interaction::{Interacting, Source, begin, cancel};

begin(world, actor, "open", door_id, None, Source::Command)?;
if let Some(i) = world.get::<Interacting>(actor) { assert_eq!(i.verb, "open"); }
cancel(world, actor, "player changed their mind");
```
