---
type: Component
---
# `CraftLeaf`

**Crate:** `npc_crafting` · **File:** [`crates/npc_crafting/src/leaf.rs`](../../../crates/npc_crafting/src/leaf.rs) · **Capability:** crafting (`CraftingPlugin`)

The action component of the generic `Craft` behaviour-tree action (`CRAFT_ACTION = "Craft"`, `SparseSet`).
`b.leaf("Bake", "Craft", "bread")`: the `ParamValue::Name` is the recipe; the workstation (only for recipes
that need one) is the `SimId` in blackboard slot `InteractionConfig::target_slot` (default 7). With no name
(`ParamValue::None`) the leaf works the first order on that workstation it can do (`next_order_for`). There
is no per-tick action system: the leaf stays `Running` until
[`crafting_complete_system`](../systems/crafting_complete_system.md) writes `Success` (success or critical
success) or `Failure` (failed, critical failure, invalid) into `ActiveLeaf`, for `CraftSource::Tree` crafts.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `recipe` | `String` | recipe the leaf started (the picked order's recipe when unnamed) |
| `workstation` | `Option<SimId>` | workstation read from the blackboard, `None` for station-less recipes |

## Lifecycle

- **Added by:** `craft_start` after a successful `begin_craft` (refusal: `Failure`, nothing inserted).
- **Read by:** `set_leaf` in `complete_craft` ([`run.rs`](../../../crates/npc_crafting/src/run.rs)).
- **Written / removed by:** `craft_stop` removes it and, while a `Tree` craft still runs, calls `cancel_craft`
  (`"aborted"` / `"finished"`).

## Hashing and snapshots

- **State hash:** yes, as `craft_leaf` (`register_crafting_hash`, [`hash.rs`](../../../crates/npc_crafting/src/hash.rs)):
  recipe, optional workstation.
- **World snapshot (ADR 0013):** saved as `craft_leaf` (automatic with hashing).

## Related

- [`Crafting`](Crafting.md) — the craft the leaf waits on.
- [`InteractLeaf`](InteractLeaf.md) — same pattern for verbs.

## Example

```rust
// Work whatever is posted on the workstation in slot 7:
b.leaf("WorkOrders", "Craft", ());
// A named recipe:
b.leaf("Bake", "Craft", "bread");
```
