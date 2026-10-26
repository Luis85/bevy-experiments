---
type: System
---
# `door_sync_system`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/door.rs`](../../../crates/npc_interaction/src/door.rs) · **Plugin:** `InteractionPlugin`

Keeps the spatial grid's **static** layer in step with every [`Door`](../components/Door.md). For each added,
changed or moved door it sets the door tag bit on the door's cell, makes the cell blocking and occluding
when closed (clears both when open), and sets `extra_cost` to 1 when locked. Cells of removed doors (and a
door's old cell when it moved) are cleared. The `DoorCells` resource maps cell ↔ door entity for
`door_pass_cost` and [`auto_door_system`](auto_door_system.md). Door verbs also sync their door immediately
(`sync_door`), so this system mainly catches content spawns, edits and removals.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Inputs`; normal (non-exclusive) system
- **Ordering:** `.after(spatial_update_system)` (kernel `spatial.rs`). Static edits are queued in the grid's
  `pending` list and reported as `GridChanged` by the next `spatial_update_system` run.

## Reads / writes

| Access | Data |
|---|---|
| reads | `Door`, `SimId`, `SimPos` (`Changed<Door>` or `Changed<SimPos>`; all doors on resync), `RemovedComponents<Door>`, `InteractionConfig` (`door_tag_bit`) |
| writes | `SpatialGrid` (static cells via `set_static`), `DoorCells` (`by_cell`, `by_entity`, takes `orphans` and `resync`) |
| messages / events | none directly; `GridChanged` follows from the grid's pending list |

## Determinism

Removed entities are collected, sorted and deduped before clearing (a removed-then-readded door is skipped).
Changed doors are collected as `(SimId, Entity, Door, SimPos)` and sorted by `SimId` before syncing. After a
world-snapshot load (`rebuild_door_cells`), `resync` makes the first run sync every door once and clear the
saved `orphans` (cells of doors removed after the saved tick's sync), since change detection does not carry
over a load. `DoorCells` is saved as `door_cells`; it is not hashed (the grid's static layer is).

## Performance

- **Mechanic tag:** `interaction` (via `tags.rs` prefix `npc_interaction`) · **Budget:** 25 µs + 2 µs/agent
  (`NPC_BUDGET_TABLE` row `interaction`) · no span of its own.

## Related

- [`auto_door_system`](auto_door_system.md), [`key_replan_system`](key_replan_system.md),
  [`complete_interactions_system`](complete_interactions_system.md) (door verbs).
- Components: [`Door`](../components/Door.md), [`Interactable`](../components/Interactable.md)
