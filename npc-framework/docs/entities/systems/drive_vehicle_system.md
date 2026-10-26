---
type: System
---
# `drive_vehicle_system`

**Crate:** `npc_vehicles` · **File:** [`crates/npc_vehicles/src/drive.rs`](../../../crates/npc_vehicles/src/drive.rs) · **Plugin:** `VehiclesPlugin`

The per-tick part of the `DriveVehicle` action. For every driver with [`Driving`](../components/Driving.md) and
a `Running` `ActiveLeaf` it checks that the driver still sits in a `Driver` seat of the same `ByCrew`
vehicle, that the vehicle may move (`can_move`) and that a goal still exists (`drive_goal`: [`DriveGoal`](../components/DriveGoal.md),
then a `GoTo` order, then the blackboard `goal_slot`). Then:

- **arrived** (same goal, vehicle `MoveStatus::Arrived`, intent goal = goal): halt the vehicle, clear the goal
  source, leaf `Success`;
- lost seat / vehicle changed: leaf `Failure`; cannot move: journal `VehicleStalled`, halt, `Failure`; no goal:
  halt, `Failure`;
- otherwise: re-insert `Driving` if the goal changed, and write the vehicle's `MoveIntent` (vehicle speed,
  `VehicleConfig::drive_tolerance_mm`) when it differs.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcVehicleSet::Drive`; exclusive (`drive_vehicle_system(world: &mut World, drivers_q: &mut DriverQuery)`,
  a cached `QueryState<(&SimId, Entity, &Driving, &ActiveLeaf)>`)
- **Ordering:** `(NpcVehicleSet::Drive, NpcVehicleSet::Gate).chain().after(NpcSet::Actions).before(NpcSet::Locomotion)`,
  so the intent it writes is moved by the movers this tick, after the gate had its say.

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`, `Driving`, `ActiveLeaf`, `MountedIn`, `Vehicle`, `Occupants`, `DriveGoal`, `CurrentOrder`, `Blackboard`, `MoveStatus`, `MoveIntent`, `VehicleConfig` |
| writes | `ActiveLeaf.status` (only while `Running`); vehicle `MoveIntent` (`set_move_intent`, or removed by `halt`) and `MoveStatus` (`Idle` on halt); `Driving`; clears `DriveGoal` / the order (`clear_order`) / the blackboard slot |
| messages / events | journal `VehicleStalled { reason }` |

## Determinism

Drivers are collected as `(SimId, Entity, Driving)` and sorted by `SimId`. Arrival is an exact status/goal
comparison; no float or distance math here (`drive_start` uses integer `dist2`).

## Performance

- **Mechanic tag:** `vehicles` (via `tags.rs` prefix `npc_vehicles`) · **Budget:** 30 µs + 2 µs/agent
  (`vehicles` row) · opens `info_span!("mechanic", name = "vehicles")` (nested in the system's own `vehicles`-tagged span, so `PerfLayer` does not time it a second time).

## Related

- [`vehicle_gate_system`](vehicle_gate_system.md) — runs next, stops vehicles that may not move.
- [`seat_follow_system`](seat_follow_system.md) — crew follows after the movers.
- Components: [`Driving`](../components/Driving.md), [`DriveGoal`](../components/DriveGoal.md), [`Vehicle`](../components/Vehicle.md), [`MountedIn`](../components/MountedIn.md)
