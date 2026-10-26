---
type: Component
---
# `InteractOrder`

**Crate:** `npc_framework` (glue) · **File:** [`crates/npc_framework/src/glue/command_interaction.rs`](../../../crates/npc_framework/src/glue/command_interaction.rs) · **Capability:** command × interaction glue (features `command` + `interaction`)

The running interaction order of a unit: which verb on which target, and how far it got. It exists only while
an `Interact { verb, target }` order (executor `npc_interaction.interact`) or a `Mount { vehicle }` order
(executor `npc_vehicles.mount`, command × vehicles glue) runs. Storage is `SparseSet`.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `verb` | `String` | The interaction verb (`"pick_up"`, `"open"`, `"unlock"`, `"mount"`, ..). |
| `target` | `SimId` | The target entity (item, door, vehicle, ..). |
| `phase` | `InteractPhase` | `Approach` (walking into reach, or about to start the verb), `Running` (the verb runs: `Interacting` on the unit), `Finished { ok: bool, reason: Option<String> }` (reported by the finished hook). |

## Lifecycle

- **Added by:** `interaction_order_start`, called from the `Interact` executor's `start` and the `Mount`
  executor's `start` (`start_mount` in [`glue/command_vehicles.rs`](../../../crates/npc_framework/src/glue/command_vehicles.rs)).
  The executors run inside [`command_engine_system`](../systems/command_engine_system.md).
- **Read / written by:** `interaction_order_step` (each engine pass: `Approach` walks with `approach_step`,
  then calls `npc_interaction::begin` or `npc_vehicles::mount` with `Source::Command` and moves to `Running`);
  the finished hook `npc_command.interact_order` (`on_interaction_finished`, an `InteractionListeners` hook)
  sets `Finished { ok, reason }` when the verb ends.
- **Removed by:** `finish` when the order ends `Done` / `Failed`; `interaction_order_cancel` (the executor's
  `cancel`: replaced, stopped, paused), which also cancels a `Source::Command` interaction still running.

## Hashing and snapshots

- **State hash:** yes, as `interact_order` (`register_sim_component` in `CommandInteractionPlugin::build`):
  verb, target, phase discriminant (`u8`), and `ok` / `reason` for `Finished`.
- **World snapshot (ADR 0013):** saved as `interact_order` (automatic with hashing).

## Related

- [`OrderQueue`](OrderQueue.md) — the order's `progress` is `Hooked` while this component drives it.
- [`command_engine_system`](../systems/command_engine_system.md) — runs the executor that owns it.
- An `Interact` verb with its own executor (`CommandAppExt::register_interact_verb_executor`) bypasses this
  executor and never gets an `InteractOrder`.

## Example

```rust
// Shift-queue "pick up the key, then unlock the door" for unit 33 (player 1).
submit_order(world, &IssueOrder::new(PlayerId(1), [SimId(33)],
    UnitOrder::Interact { verb: "pick_up".into(), target: SimId(26) }))?;
submit_order(world, &IssueOrder::new(PlayerId(1), [SimId(33)],
    UnitOrder::Interact { verb: "unlock".into(), target: SimId(12) }).queued(true))?;
// While the first one walks: InteractOrder { verb: "pick_up", target: SimId(26), phase: Approach }
```
