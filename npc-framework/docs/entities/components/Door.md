---
type: Component
---
# `Door`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/door.rs`](../../../crates/npc_interaction/src/door.rs) · **Capability:** interaction (`InteractionPlugin`)

A door standing in one grid cell (the cell of its `SimPos`; one door per cell). Its state is mirrored into
that cell's **static** grid layer: the door tag bit (`InteractionConfig::door_tag_bit`, default 0) is set, a
closed door makes the cell blocking and occluding, an open one clears both, and a locked door sets the
cell's `extra_cost` to 1 (so lock/unlock changes cell data and nav re-plans on `GridChanged`). The nav
pass-cost hook `door_pass_cost` lets an [`Interactor`](Interactor.md) path through a closed door it may open.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `open` | `bool` | open = walkable and see-through |
| `locked_by` | `Option<String>` | key that unlocks it while locked; `None` = unlocked |
| `key` | `Option<String>` | key that fits its lock (what `lock` needs); `None` = cannot be locked |

A key is a carried item whose kind name or one tag equals the string (e.g. `"key:gate"`). Constructors:
`Door::closed()`, `Door::opened()`, `Door::locked(key)`, `Door::lockable(key)`.

## Lifecycle

- **Added by:** content (testbed `spawn_door` in
  [`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** [`door_sync_system`](../systems/door_sync_system.md) (cell flags), `door_pass_cost` / `may_pass`
  (nav), [`auto_door_system`](../systems/auto_door_system.md), [`key_replan_system`](../systems/key_replan_system.md)
  (locked keys), door verbs' preconditions, glue `interaction_agent.rs` / `command_interaction.rs`,
  testbed view (`visuals.rs`).
- **Written / removed by:** verb effects `open`, `close`, `lock`, `unlock` (`mutate_door`, then `sync_door`
  so grid, perception and nav see it in the same tick; journal `DoorChanged`). `close` refuses while any
  other entity's live `SimPos` is in the doorway. The testbed scenario loader's `world.open_doors` sets
  `open = true`, `locked_by = None` directly ([`scenario.rs`](../../../content/testbed_content/src/scenario.rs);
  `door_sync_system` syncs it). Removing `Door` clears the cell on the next sync.

## Hashing and snapshots

- **State hash:** yes, as `door` (`register_interaction_hash`, [`hash.rs`](../../../crates/npc_interaction/src/hash.rs)):
  `open` as `u8`, optional `locked_by`, optional `key`. The grid's static layer is hashed too
  (`register_spatial_hash`).
- **World snapshot (ADR 0013):** saved as `door` (automatic with hashing). The derived `DoorCells` resource
  is saved as `door_cells` and rebuilt after load (`rebuild_door_cells`; the next sync re-syncs all doors).

## Related

- [`Interactable`](Interactable.md) — a door with one that does not offer `open` blocks every agent.
- [`Item`](Item.md) — keys.

## Example

```rust
use npc_framework::interaction::Door;

spawn_sim(world, (SimPos::new(500, -9500), Door::locked("key:store")));
// {"type":"Interact","unit":5,"verb":"unlock","target":12}
```
