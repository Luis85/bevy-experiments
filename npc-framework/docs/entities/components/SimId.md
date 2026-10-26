---
type: Component
---
# `SimId`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/ids.rs`](../../../crates/npc_kernel/src/ids.rs) · **Capability:** kernel

The stable identity of a sim entity. `Entity` ids differ between machines and runs; `SimId`s are handed out by
`SimIdAllocator` in increasing order from 1, so the same scenario gives the same ids everywhere. Everything
order-sensitive in the sim sorts by `SimId`; commands, the journal, snapshots and the state hash name units by it.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `0` | `u32` | The id. `SimId(0)` is never allocated. Serializes as a plain number (`5`, not `{"0":5}`). |

Derives `Ord`, so it can key a `BTreeMap` and sort a `Vec<(SimId, Entity)>`.

## Lifecycle

- **Added by:** `spawn_sim(world, bundle)` ([`ids.rs`](../../../crates/npc_kernel/src/ids.rs)), the one way to
  create a sim entity. It allocates the id, spawns `(bundle, id)`, adds `PrevSimPos` when the bundle has a
  `SimPos` but no `PrevSimPos`, and journals `Spawn { kind }`. Snapshot loads spawn `(SimId(id),)` directly.
- **Component hooks:**
  - `on_insert`: maps the id to the entity in `SimIndex` (`BTreeMap<SimId, Entity>`).
  - `on_discard` (replace, remove, despawn): unmaps the id (only if it still points at this entity), then runs
    every `SimIdHooks::on_leave` hook sorted by name (e.g. `npc_jobs` releases reservations). Register a hook
    with `KernelAppExt::on_sim_id_leave(name, fn)`.
  - `on_despawn`: journals `Despawn`.
  - While `SnapshotLoading` exists, the leave hooks and the `Spawn`/`Despawn` entries are skipped.
- **Read by:** practically every sim system; [`think_system`](../systems/think_system.md) (agent order),
  [`rethink_timer_system`](../systems/rethink_timer_system.md) (stagger), [`spatial_update_system`](../systems/spatial_update_system.md)
  (the grid index), `state_hash`, the inspector, the journal (`drain_ai_trace` maps `Entity → SimId`).
- **Removed by:** `SimCommand::Despawn { unit }` ([`apply_commands_system`](../systems/apply_commands_system.md)) or any despawn.

## Hashing and snapshots

- **State hash:** not a registered component, but the hash walks every entity with a `SimId` in ascending
  order and hashes the id itself plus each registered component (`state_hash` in
  [`hash.rs`](../../../crates/npc_kernel/src/hash.rs)). Only entities with a `SimId` are sim state.
- **World snapshot (ADR 0013):** saved as `EntitySnapshot.id`, not as a named component. Load checks the ids
  are strictly ascending and moves `SimIdAllocator.next` past the highest one. The allocator itself is saved
  as resource `sim_id_allocator`.
- Reflect-registered (`register_type::<SimId>()`), type path `npc_sim::ids` for BRP compatibility.

## Related

- [`SimPos`](SimPos.md), [`PrevSimPos`](PrevSimPos.md) — `spawn_sim` pairs them.

## Example

```rust
use npc_framework::prelude::*;

let (id, entity) = spawn_sim(app.world_mut(), (SimPos::new(1000, 2000), MyUnit));
let found = app.world().resource::<SimIndex>().get(id);
assert_eq!(found, Some(entity));
```
