---
type: Component
---
# `Driving`

**Crate:** `npc_vehicles` · **File:** [`crates/npc_vehicles/src/drive.rs`](../../../crates/npc_vehicles/src/drive.rs) · **Capability:** vehicles (`VehiclesPlugin`)

The action component of the `DriveVehicle` behaviour-tree action (`DRIVE_VEHICLE = "DriveVehicle"`,
`SparseSet`), on the **driver**. `drive_start` needs the driver in a `Driver` seat of a `ByCrew` vehicle, a
goal ([`DriveGoal`](DriveGoal.md) / order / blackboard) and a vehicle that may move; it writes the
vehicle's `MoveIntent` (vehicle speed, `VehicleConfig::drive_tolerance_mm`) and inserts this.
[`drive_vehicle_system`](../systems/drive_vehicle_system.md) then steers each tick and finishes the leaf.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `vehicle` | `SimId` | the vehicle being driven |
| `goal` | `SimPos` | mm; the goal currently steered to (updated when the goal changes) |

## Lifecycle

- **Added by:** `drive_start` ([`drive.rs`](../../../crates/npc_vehicles/src/drive.rs)), returns `Running`
  (or `Success` at once if already within tolerance, `Failure` when it cannot drive).
- **Read by:** `drive_vehicle_system` (drivers with `ActiveLeaf.status == Running`), `drive_stop`,
  `vehicle_json` (inspect), the agent busy predicate `npc_vehicles` (`vehicles_busy` in
  [`vehicles_agent.rs`](../../../crates/npc_framework/src/glue/vehicles_agent.rs): the driver is busy for
  `agent/events {until_idle}` and `agent/act_and_wait` while it has `Driving`).
- **Written / removed by:** `drive_vehicle_system` re-inserts it with the new `goal` on retarget;
  `drive_stop` removes it and, on `Aborted`, halts the vehicle if its intent still points at `goal`.

## Hashing and snapshots

- **State hash:** yes, as `driving` (`register_vehicle_hash`, [`hash.rs`](../../../crates/npc_vehicles/src/hash.rs)):
  vehicle id, goal.
- **World snapshot (ADR 0013):** saved as `driving` (automatic with hashing).

## Related

- [`DriveGoal`](DriveGoal.md), [`Vehicle`](Vehicle.md), [`MountedIn`](MountedIn.md).
- [`vehicle_gate_system`](../systems/vehicle_gate_system.md) — stops a vehicle that may not move.

## Example

```rust
use npc_framework::vehicles::{COND_HAS_DRIVE_GOAL, DRIVE_VEHICLE};

// Testbed unit tree (content/testbed_content/src/tree.rs):
b.guard("Drive", COND_HAS_DRIVE_GOAL, Abort::LowerPriority, |b| {
    b.leaf("DriveVehicle", DRIVE_VEHICLE, ());
});
```
