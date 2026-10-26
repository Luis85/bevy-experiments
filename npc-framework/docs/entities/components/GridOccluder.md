---
type: Component
---
# `GridOccluder`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/spatial.rs`](../../../crates/npc_kernel/src/spatial.rs) · **Capability:** kernel (spatial grid)

Marks the cells of a box around the entity's `SimPos` as **occluding** in the dynamic layer of `SpatialGrid`.
`npc_perception` grid line of sight (`SpatialGrid::trace_segment`) stops at occluding cells. It does not block
movement; add a [`GridBlocker`](GridBlocker.md) too for a solid wall.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `0` | `Footprint` | half extents around `SimPos` (`half_x_mm`, `half_z_mm`: `i32` mm, inclusive; negative counts as 0) |

`Footprint::default()` covers the position's own cell.

## Lifecycle

- **Added by:** content at spawn (e.g. testbed walls in `content/testbed_content/src/layout.rs`). Doors use
  the static layer instead (`npc_interaction::door`, `SpatialGrid::set_static`).
- **Read by:** [`spatial_update_system`](../systems/spatial_update_system.md) (recomputes the dynamic layer and
  writes `GridChanged` when an occluder appears, moves, changes or is removed); perception LOS reads the merged
  cell (`grid.is_occluding(c)` inside `trace_segment`, `crates/npc_perception/src/system.rs`).
- **Removed by:** content/capabilities (`RemovedComponents<GridOccluder>` triggers a recompute).

## Hashing and snapshots

- **State hash:** only after `register_spatial_hash` (called by `npc_nav`, `npc_perception`,
  `npc_interaction`, `npc_command`), as `grid_occluder` (encoding `i32 half_x_mm`, `i32 half_z_mm`).
- **World snapshot (ADR 0013):** always saved as `grid_occluder`, hashed or not: `SpatialPlugin` registers it
  (`spatial_update_system` reads it on the next tick); `register_spatial_hash` registering it again under the
  same name is a no-op in either order. The grid's dynamic layer is saved too, in resource `spatial_grid`.

## Related

- [`GridBlocker`](GridBlocker.md) — movement blocking.
- [`SimPos`](SimPos.md) — the footprint centre.

## Example

```rust
use npc_framework::kernel::spatial::{Footprint, GridOccluder};

// A hedge you can walk around but not see through.
spawn_sim(world, (SimPos::new(500, 6_500), GridOccluder(Footprint { half_x_mm: 1500, half_z_mm: 0 })));
```
