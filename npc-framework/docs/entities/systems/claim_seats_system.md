---
type: System
---
# `claim_seats_system`

**Crate:** `npc_vehicles` · **File:** [`crates/npc_vehicles/src/verbs.rs`](../../../crates/npc_vehicles/src/verbs.rs) · **Plugin:** `VehiclesPlugin`

Keeps seat claims in [`Occupants`](../components/Occupants.md) in step with the `mount` interactions in
progress. The `mount` verb takes no target reservation (so several units can mount different seats of one
vehicle at once); seats are claimed one by one here instead. First, every `Claimed(id)` seat whose claimant
is no longer running `mount` on that vehicle is released. Then every unit with an `Interacting { verb:
"mount" }` and no claim on its target takes `default_seat` (its claim, else the first free seat of its
[`SeatPreference`](../components/SeatPreference.md) role, else any free seat) via `claim_seat`. When none is
left its interaction is cancelled with the reason (`cancel_interaction`: journal `Interacted { outcome:
"cancelled" }`, and a `Tree` `Interact` leaf gets `Failure`). Of two units mounting the last seat in one
tick, the lower `SimId` gets it. The `mount` effect itself (seating) runs later in
[`complete_interactions_system`](complete_interactions_system.md).

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcVehicleSet::Crew`; exclusive (`claim_seats_system(world: &mut World, mounting_q: &mut MountingQuery,
  claims_q: &mut ClaimsQuery)`, cached `QueryState`s of the units in an interaction and of the vehicles' `Occupants`)
- **Ordering:** `(seat_follow_system, claim_seats_system).chain()`; the set runs `.before(NpcInteractionSet::Complete)`,
  so a mount with `mount_ticks = 0` has its seat claimed before it completes in the same tick.

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`, `Interacting` (`verb == "mount"`, `target`), `Vehicle`, `Occupants`, `SeatPreference` |
| writes | `Occupants` (claims added/released); removes `Interacting` and may set `ActiveLeaf.status` on cancel; `Reservations` release via `npc_interaction::cancel` when the verb reserves (mount does not) |
| messages / events | journal `Interacted { outcome: "cancelled", reason }` |

## Determinism

Mounting units are collected as `(SimId, Entity, target)` and sorted by `SimId`; vehicles with claims are
sorted by `SimId`; seats are chosen in index order. That is what makes the tie-break deterministic.

## Performance

- **Mechanic tag:** `vehicles` (prefix `npc_vehicles`) · **Budget:** 30 µs + 2 µs/agent (`vehicles`)
  · opens `info_span!("mechanic", name = "vehicles")` (nested in the system's own `vehicles`-tagged span, so `PerfLayer` does not time it a second time).

## Related

- [`seat_follow_system`](seat_follow_system.md) (before), [`complete_interactions_system`](complete_interactions_system.md) (after).
- Components: [`Occupants`](../components/Occupants.md), [`Interacting`](../components/Interacting.md), [`SeatPreference`](../components/SeatPreference.md), [`MountedIn`](../components/MountedIn.md)
