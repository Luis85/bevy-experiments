---
type: System
---
# `seat_follow_system`

**Crate:** `npc_vehicles` · **File:** [`crates/npc_vehicles/src/systems.rs`](../../../crates/npc_vehicles/src/systems.rs) · **Plugin:** `VehiclesPlugin`

After the movers, puts every mounted actor exactly on its seat: `SimPos` = vehicle `SimPos` + the seat's
`offset` (`seat_pos`), every tick. It writes only when the position differs. Because the rider moves in the
same tick as its vehicle and both copied `PrevSimPos` in `NpcSet::Inputs`, view interpolation keeps the
rider on its seat with no extra work. Riders whose vehicle entity or `Vehicle`/`SimPos` is missing are
skipped (the next [`vehicle_gate_system`](vehicle_gate_system.md) dismounts them).

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcVehicleSet::Crew` (configured `.after(NpcSet::Locomotion).before(NpcInteractionSet::Complete).before(NpcSet::Effects)`);
  exclusive (`seat_follow_system(world: &mut World, mounted_q: &mut MountedQuery)`, a cached `QueryState` of the
  mounted actors)
- **Ordering:** `(seat_follow_system, claim_seats_system).chain()`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`, `MountedIn`, `Vehicle` (`seats[i].offset`), vehicle `SimPos` |
| writes | rider `SimPos` |
| messages / events | none |

## Determinism

Riders are collected and sorted by `SimId` (positions are independent, so order does not matter, but it is
still fixed). Integer `SimPos` addition.

## Performance

- **Mechanic tag:** `vehicles` (prefix `npc_vehicles`) · **Budget:** 30 µs + 2 µs/agent (`vehicles`)
  · opens `info_span!("mechanic", name = "vehicles")` (nested in the system's own `vehicles`-tagged span, so `PerfLayer` does not time it a second time).

## Related

- [`claim_seats_system`](claim_seats_system.md) (next), [`vehicle_gate_system`](vehicle_gate_system.md).
- Components: [`MountedIn`](../components/MountedIn.md), [`Vehicle`](../components/Vehicle.md)
