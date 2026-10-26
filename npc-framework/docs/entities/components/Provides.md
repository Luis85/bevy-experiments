---
type: Component
---
# `Provides`

**Crate:** `npc_jobs` · **File:** [`crates/npc_jobs/src/lib.rs`](../../../crates/npc_jobs/src/lib.rs) · **Capability:** jobs / smart objects (always in `NpcPlugins`)

A smart-object advertisement on a place: using it restores `need` by `amount_per_tick` while the matching
action runs. The framework only stores it. Content actions decide how to apply it (per tick, or
`amount × duration` at the end). Agents find a place by its `Provides`, then claim it in the
`Reservations` resource before using it.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `need` | `NeedId` | the need it restores (look it up with `NeedRegistry::id(name)`; ids are frozen by name) |
| `amount_per_tick` | `i32` | milli-units per tick of use |

## Lifecycle

- **Added by:** scenario spawns of places. The village bed (energy), stockpile (hunger) and well (thirst) in
  [`content/village_content/src/scenario.rs`](../../../content/village_content/src/scenario.rs).
- **Read by:** content only. Village `find_bed` / `find_well` in
  [`content/village_content/src/villager.rs`](../../../content/village_content/src/villager.rs) (pick a place
  by its need); `eat_system`, `drink_system`, `sleep_system` in
  [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) (apply the amount).
- **Written / removed by:** nothing in the framework.

## Reservations (resource, same crate)

`Reservations { claims: BTreeMap<SimId, Vec<SimId>> }` maps a target to its claimants in claim order. It
guarantees that two agents never use the same bed, field or food unit. Use these:

- `free_for(&res, target, me)` checks capacity 1. `free_slots(&res, target, capacity, me)` checks
  multi-slot targets; `me`'s own claim does not count.
- `reserve(world, me, target, capacity)` / `try_reserve_slot(..)` claim a slot (journaled `Reserve { target, ok }`).
- `release(world, agent, target)` / `release_all(world, agent, reason)` drop claims (journaled `Release`).
  Actions release synchronously in their stop.
- `NpcJobsPlugin` adds an `on_sim_id_leave("npc_jobs.reservations", ..)` hook. When an entity leaves the sim,
  its claims are dropped, and so are the claims on it (`Release { Despawn }`).
- Hashed and saved as `reservations` (`register_sim_resource`). Per-entity inspect shows `reservations_held`.

## Hashing and snapshots

- **State hash:** no. `Provides` has no `SimHash` impl. It is static place data; what it causes shows up in
  the hashed `needs` and `reservations`.
- **World snapshot (ADR 0013):** saved as `provides` (snapshot-only, `register_snapshot_component_named` in
  `NpcJobsPlugin`).

## Related

- [`Approach`](Approach.md) — where to stand to use the place.
- [`Needs`](Needs.md) — what it restores.

## Example

```rust
use npc_framework::jobs::{Approach, Provides};

let thirst = world.resource::<NeedRegistry>().id("Thirst").unwrap();
spawn_sim(world, (
    SimPos::new(-4500, 4500),
    Approach(SimPos::new(-4500, 3300)),
    Provides { need: thirst, amount_per_tick: 2500 },
));

// In an action's start: claim it, or fail.
if !npc_framework::jobs::reserve(world, me, well_id, 2) { return Status::Failure; }
```
