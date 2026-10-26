---
type: Component
---
# `DriveGoal`

**Crate:** `npc_vehicles` · **File:** [`crates/npc_vehicles/src/drive.rs`](../../../crates/npc_vehicles/src/drive.rs) · **Capability:** vehicles (`VehiclesPlugin`)

A destination for the `ByCrew` vehicle this unit drives (or will drive). It is the first of three goal
sources `drive_goal` checks, in order: (1) `DriveGoal` on the driver, (2) the driver's standing order
(`CurrentOrder(Some(OrderKind::GoTo { pos }))`), (3) a `BbValue::Pos` in blackboard slot
`VehicleConfig::goal_slot` (default 5). The `DriveVehicle` action clears the source it used when the vehicle
arrives. It is what orders addressed to a vehicle end up as (`command_vehicle_to` on a `ByCrew` vehicle).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `0` | `SimPos` | goal position, mm |

## Lifecycle

- **Added by:** `set_drive_goal` (also inserts `ThinkRequest`), called by `command_vehicle_to`
  ([`drive.rs`](../../../crates/npc_vehicles/src/drive.rs)); that is reached from the agent glue `DriveTo`
  command ([`vehicles_agent.rs`](../../../crates/npc_framework/src/glue/vehicles_agent.rs)) and RTS order glue
  ([`command_vehicles.rs`](../../../crates/npc_framework/src/glue/command_vehicles.rs)). The testbed's
  `TestbedDrive` command calls `set_drive_goal` directly
  ([`content/testbed_content/src/commands.rs`](../../../content/testbed_content/src/commands.rs)).
- **Read by:** `drive_goal` (`drive_start`, [`drive_vehicle_system`](../systems/drive_vehicle_system.md),
  condition `HasDriveGoal`), `vehicle_json`.
- **Written / removed by:** `clear_goal` on arrival (in `drive_start` when already there, or
  `drive_vehicle_system`); `clear_drive_goal` (glue `command_vehicles.rs`).

## Hashing and snapshots

- **State hash:** yes, as `drive_goal` (`register_vehicle_hash`, [`hash.rs`](../../../crates/npc_vehicles/src/hash.rs)):
  the `SimPos`.
- **World snapshot (ADR 0013):** saved as `drive_goal` (automatic with hashing).

## Related

- [`Driving`](Driving.md) — the running drive toward it.
- [`MountedIn`](MountedIn.md) — the driver must sit in a `Driver` seat.

## Example

```rust
use npc_framework::vehicles::{command_vehicle_to, set_drive_goal};

command_vehicle_to(world, cart, SimPos::new(8000, 0))?;    // ByCrew: driver gets DriveGoal
set_drive_goal(world, driver, SimPos::new(8000, 0));        // or directly on the driver
// agent glue: {"type":"DriveTo","unit":5,"pos":[8000,0]}
```
