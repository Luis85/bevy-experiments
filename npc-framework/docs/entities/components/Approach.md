---
type: Component
---
# `Approach`

**Crate:** `npc_jobs` · **File:** [`crates/npc_jobs/src/lib.rs`](../../../crates/npc_jobs/src/lib.rs) · **Capability:** jobs / smart objects (always in `NpcPlugins`)

Where an agent stands to use a place, e.g. the side of a bed or the front of a well. A place's own `SimPos`
is often inside a blocking footprint, so actions walk to its `Approach`. `approach_of(world, place)`
returns the `Approach`, or the place's `SimPos` when it has none.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `0` | `SimPos` | stand point, millimetres (`i32` x/z) |

## Lifecycle

- **Added by:** scenario spawns of places (village house, bed, stockpile, field, well in
  [`content/village_content/src/scenario.rs`](../../../content/village_content/src/scenario.rs)). Crafting
  workstations may carry one (optional, see [`crates/npc_crafting/src/station.rs`](../../../crates/npc_crafting/src/station.rs)).
- **Read by:** `npc_jobs::approach_of`. Its callers are village actions
  ([`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs): the walk
  destination), village agent affordances (`agent.rs`), `find_well` (`villager.rs`). The village F1
  overlay (`village_view/src/overlay.rs`) queries `&Approach` directly.
- **Written / removed by:** nothing in the framework.

## Hashing and snapshots

- **State hash:** no. It has no `SimHash` impl. It is static place data.
- **World snapshot (ADR 0013):** saved as `approach` (snapshot-only, `register_snapshot_component_named` in
  `NpcJobsPlugin`).

## Reservations

`Approach` does not claim anything. Claims go through the `Reservations` resource, documented in
[`Provides`](Provides.md#reservations-resource-same-crate). A multi-slot place, such as a well with 2 slots,
shares one `Approach`. `try_reserve_slot` returns the claimant's slot index (`Reservations::slot_of`)
if content wants to tell the users apart; the village does not.

## Related

- [`Provides`](Provides.md) — what the place offers; Reservations.
- [`MoveIntent`](MoveIntent.md) — the walk to the approach point.

## Example

```rust
use npc_framework::jobs::{Approach, approach_of};
use npc_framework::locomotion::{MoveIntent, set_move_intent};

spawn_sim(world, (SimPos::new(-2500, -5500), Approach(SimPos::new(-2500, -4000))));

// Walk to a place's stand point.
if let Some(dest) = approach_of(world, place) {
    set_move_intent(world, agent, MoveIntent::to(dest, 60));
}
```
