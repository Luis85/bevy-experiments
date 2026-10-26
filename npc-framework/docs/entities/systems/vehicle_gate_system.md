---
type: System
---
# `vehicle_gate_system`

**Crate:** `npc_vehicles` · **File:** [`crates/npc_vehicles/src/systems.rs`](../../../crates/npc_vehicles/src/systems.rs) · **Plugin:** `VehiclesPlugin`

The gate before the movers. In order:

1. **Lost riders:** an actor whose [`MountedIn`](../components/MountedIn.md) seat is no longer `Taken` by it
   (vehicle gone, or it no longer seats the actor) loses `MountedIn` where it stands, gets a `ThinkRequest`,
   journal `Dismounted`.
1b. **Stale seats:** a `Taken` seat whose occupant is gone or no longer `MountedIn` that seat becomes `Free`.
2. **Crew locomotion suspended:** every mounted actor with a `MoveIntent` is halted (intent removed, status
   `Idle`), so no mover moves a rider.
3. **Vehicles that may not move:** a vehicle with a `MoveIntent` failing `can_move` (a `ByCrew` vehicle
   without a driver, a required seat empty) loses the intent, its `MoveStatus` becomes `blocked_again()`
   (`Blocked { ticks + 1 }`), journal `VehicleStalled { reason }`.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcVehicleSet::Gate`; exclusive (`vehicle_gate_system(world: &mut World, mounted_q: &mut MountedQuery,
  vehicles_q: &mut VehicleQuery)`, cached `QueryState`s of the mounted actors and the vehicles)
- **Ordering:** `(NpcVehicleSet::Drive, NpcVehicleSet::Gate).chain().after(NpcSet::Actions).before(NpcSet::Locomotion)`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`, `MountedIn`, `Occupants`, `Vehicle`, `MoveIntent`, `MoveStatus`, `SimPos` |
| writes | removes `MountedIn`, inserts `ThinkRequest`; `Occupants` (frees seats); removes `MoveIntent` and sets `MoveStatus` on riders and stalled vehicles |
| messages / events | journal `Dismounted { vehicle, seat, at }`, `VehicleStalled { reason }` |

## Determinism

Mounted actors and vehicles are each collected with their `SimId` and sorted by it; every step walks those
sorted lists, seats by index.

## Performance

- **Mechanic tag:** `vehicles` (prefix `npc_vehicles`) · **Budget:** 30 µs + 2 µs/agent (`vehicles`)
  · opens `info_span!("mechanic", name = "vehicles")` (nested in the system's own `vehicles`-tagged span, so `PerfLayer` does not time it a second time).

## Related

- [`drive_vehicle_system`](drive_vehicle_system.md) (before), [`seat_follow_system`](seat_follow_system.md) (after the movers).
- Components: [`MountedIn`](../components/MountedIn.md), [`Occupants`](../components/Occupants.md), [`Vehicle`](../components/Vehicle.md)
