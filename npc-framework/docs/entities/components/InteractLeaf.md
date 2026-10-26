---
type: Component
---
# `InteractLeaf`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/leaf.rs`](../../../crates/npc_interaction/src/leaf.rs) · **Capability:** interaction (`InteractionPlugin`)

The action component of the generic `Interact` behaviour-tree action (`INTERACT_ACTION = "Interact"`,
`SparseSet`). `b.leaf("OpenDoor", "Interact", "open")` (`TreeBuilder::leaf(name, action, param)`): the `ParamValue::Name` is the verb; the target
is the `SimId` in blackboard slot `InteractionConfig::target_slot` (default 7, `BbValue::U32`), the item the
one in `item_slot` (default 6, read only for verbs that take an item). A `Direct` agent's queue runs the same
leaf. There is **no per-tick action system**: the leaf stays `Running` until
[`complete_interactions_system`](../systems/complete_interactions_system.md) writes `Success` / `Failure`
into `ActiveLeaf` (only for `Source::Tree` interactions with this component present).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `verb` | `String` | verb the leaf started |
| `target` | `SimId` | target read from the blackboard |
| `item` | `Option<SimId>` | item read from the blackboard, if the verb takes one |

## Lifecycle

- **Added by:** `interact_start` after a successful `run::begin` (start returns `Running`; a refusal returns
  `Failure` and inserts nothing).
- **Read by:** `run::complete` (writes the leaf status), `npc_vehicles::verbs::cancel_interaction` (fails the
  leaf when a mount loses its seat).
- **Written / removed by:** `interact_stop` removes it; on abort (or finish) while a `Tree` interaction still
  runs it calls `run::cancel` (`"aborted"` / `"finished"`).

## Hashing and snapshots

- **State hash:** yes, as `interact_leaf` (`register_interaction_hash`, [`hash.rs`](../../../crates/npc_interaction/src/hash.rs)):
  verb, target, optional item.
- **World snapshot (ADR 0013):** saved as `interact_leaf` (automatic with hashing).

## Related

- [`Interacting`](Interacting.md) — the interaction the leaf waits on.
- [`CraftLeaf`](CraftLeaf.md) — the same pattern for crafting.

## Example

```rust
// Tree: put the door's SimId in slot 7, then open it.
b.leaf("OpenDoor", "Interact", "open");
// Testbed: b.leaf("UseTool", "Interact", "use_tool_on");  (content/testbed_content/src/tree.rs)
// Direct agent queue: SetBlackboard { slot: 7, value: U32(door) }, then Action "Interact" "open".
```
