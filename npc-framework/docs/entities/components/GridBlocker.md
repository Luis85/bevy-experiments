---
type: Component
---
# `GridBlocker`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/spatial.rs`](../../../crates/npc_kernel/src/spatial.rs) · **Capability:** kernel (spatial grid)

Marks the cells of a box around the entity's `SimPos` as **blocking** in the dynamic layer of `SpatialGrid`.
Grid A* (`npc_nav`) will not path through them. Use it for walls (the testbed's wall runs) or a
`Direct` vehicle's own body (`npc_nav` docs). Doors do not use it: `npc_interaction` writes the door cell into the grid's static
layer (`SpatialGrid::set_static`) instead.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `0` | `Footprint` | half extents around `SimPos` |

`Footprint`:

| Field | Type | Unit / meaning |
|---|---|---|
| `half_x_mm` | `i32` | millimetres, inclusive half width on X. Negative counts as 0. |
| `half_z_mm` | `i32` | millimetres, inclusive half depth on Z. Negative counts as 0. |

`Footprint::default()` covers exactly the cell of the position. Extents beyond the grid size are clamped.

## Lifecycle

- **Added by:** content at spawn (`spawn_sim(world, (pos, GridBlocker(fp)))`, e.g.
  `content/testbed_content/src/layout.rs` walls, which carry a `GridOccluder` with the same footprint).
- **Read by:** [`spatial_update_system`](../systems/spatial_update_system.md): recomputes the dynamic layer when a
  blocker appears, moves (`Changed<SimPos>`), changes or is removed, and writes `GridChanged` with the changed
  cells. Counts per cell are summed, so query order does not matter. Also `npc_nav::plan` (an agent's own
  blocker cells are not obstacles to itself), `npc_nav::mover`, `nearest_blocker_by_edge`.
- **Removed by:** content/capabilities; removal is seen through `RemovedComponents<GridBlocker>`.

## Hashing and snapshots

- **State hash:** only when a capability calls `register_spatial_hash` (`npc_nav`, `npc_perception`,
  `npc_interaction`, `npc_command` do), then as `grid_blocker` (`register_sim_component`; encoding
  `i32 half_x_mm`, `i32 half_z_mm`). Not hashed with the bare `NpcPlugins`.
- **World snapshot (ADR 0013):** always saved as `grid_blocker`, hashed or not: `SpatialPlugin` registers it
  (`register_snapshot_component_named`), because `spatial_update_system` reads it on the next tick.
  `register_spatial_hash` registers it again under the same name, a no-op in either order. The grid's dynamic
  layer is saved too, inside resource `spatial_grid`; the first update after a load re-syncs it against the
  loaded blockers.

## Related

- [`GridOccluder`](GridOccluder.md) — the line-of-sight counterpart.
- [`SimPos`](SimPos.md) — the footprint centre.

## Example

```rust
use npc_framework::kernel::spatial::{Footprint, GridBlocker};

// A 2 m x 1 m wall centred at (1000, 500).
spawn_sim(world, (SimPos::new(1000, 500), GridBlocker(Footprint { half_x_mm: 500, half_z_mm: 0 })));
```
