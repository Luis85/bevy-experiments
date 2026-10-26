---
type: Component
---
# `PrevSimPos`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/math.rs`](../../../crates/npc_kernel/src/math.rs) · **Capability:** kernel

The entity's `SimPos` at the start of the current tick. Presentation lerps `PrevSimPos → SimPos` by
`SimTime::overstep_ppm`; a few sim systems use it to see how an entity moved this tick (perception's facing update).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `0` | [`SimPos`](SimPos.md) | millimetres, the position before this tick's movers ran |

## Lifecycle

- **Added by:** `spawn_sim` (equal to the spawn position, if the bundle has a `SimPos` and no `PrevSimPos`);
  [`copy_prev_pos_system`](../systems/copy_prev_pos_system.md) for any entity that has a `SimPos` but no
  `PrevSimPos` (deferred, applied before `NpcSet::Needs`); content bundles may add it themselves
  (e.g. `villager_bundle`); `npc_interaction` re-adds it with `SimPos` when an item is dropped.
- **Written by:** [`copy_prev_pos_system`](../systems/copy_prev_pos_system.md) every tick in `NpcSet::Inputs`
  (only when it differs, to avoid spurious change detection).
- **Read by:** `npc_perception` facing update (`system.rs`), content visuals (`village_view`, `testbed_view`);
  `npc_view_helpers::interp` only supplies the lerp factor.
- **Removed by:** `npc_interaction` together with `SimPos` when an item is picked up.

## Hashing and snapshots

- **State hash:** no.
- **World snapshot (ADR 0013):** not saved. Load step 6 sets `PrevSimPos = SimPos` on every entity with a
  `SimPos` (`load` in [`snapshot.rs`](../../../crates/npc_kernel/src/snapshot.rs)), so the first tick after a
  load interpolates from rest.

## Related

- [`SimPos`](SimPos.md) — the source value.
- [`copy_prev_pos_system`](../systems/copy_prev_pos_system.md) — the only per-tick writer.

## Example

```rust
fn moved_this_tick(q: Query<(&SimId, &SimPos, &PrevSimPos)>) -> Vec<SimId> {
    let mut ids: Vec<SimId> = q.iter().filter(|(_, p, prev)| **p != prev.0).map(|(id, ..)| *id).collect();
    ids.sort();
    ids
}
```
