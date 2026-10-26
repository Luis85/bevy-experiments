---
type: System
---
# `spatial_update_system`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/spatial.rs`](../../../crates/npc_kernel/src/spatial.rs) · **Plugin:** `SpatialPlugin` (part of `NpcPlugins`)

Keeps `SpatialGrid` current: the index of every entity with a `SimId` and a `SimPos` by cell, and the dynamic
layer from `GridBlocker`/`GridOccluder` footprints. Writes one `GridChanged` per tick in which effective cell
data changed. Queries during a tick (`in_radius`, `in_box`, `ids_in_cell`) therefore see positions at the end of
the previous tick plus this tick's commands.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Inputs`, not exclusive
- **Ordering:** `.after(apply_host_positions_system).after(copy_prev_pos_system)` (so also after
  `apply_commands_system`). `npc_interaction::door::door_sync_system` runs `.after(spatial_update_system)`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`, `SimPos` (`Changed<SimPos>` or `Added<SimId>`), `GridBlocker`, `GridOccluder`, `RemovedComponents` of `SimPos`/`SimId`/`GridBlocker`/`GridOccluder`, `Option<Res<SimClock>>` |
| writes | `SpatialGrid` (index, dynamic layer, pending static changes) |
| messages / events | `Messages<GridChanged>`: calls `update()` itself (not in `First`), then writes `GridChanged { tick, cells }` (sorted, deduplicated) when any cell changed |

Steps: after a snapshot load (`resync`) re-index everything; unindex removed entities; index moved/spawned
ones; recompute the dynamic layer only when a blocker/occluder appeared, moved, changed, or was removed (or on
resync); merge in static changes from `set_static`; emit `GridChanged`. With no blockers it touches no cell data.

## Determinism

Index and cells are `BTreeMap`s; queries return `SimId`s sorted ascending. Footprint counts are summed per cell,
so query order does not matter. Changed cells are sorted and deduplicated. Integer cell math (`cell_of` floors).
The grid is not hashed unless a capability calls `register_spatial_hash`; it is saved as resource `spatial_grid`.

## Performance

- **Mechanic tag:** `spatial` (via `tags.rs` prefix `npc_kernel::spatial`) · **Budget:** 10 µs + 1 µs per agent
  (`NPC_BUDGET_TABLE`) · no spans.

## Related

- [`copy_prev_pos_system`](copy_prev_pos_system.md), [`apply_host_positions_system`](apply_host_positions_system.md) — run before.
- Components: [`GridBlocker`](../components/GridBlocker.md), [`GridOccluder`](../components/GridOccluder.md), [`SimPos`](../components/SimPos.md), [`SimId`](../components/SimId.md)
