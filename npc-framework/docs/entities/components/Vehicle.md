---
type: Component
---
# `Vehicle`

**Crate:** `npc_vehicles` · **File:** [`crates/npc_vehicles/src/vehicle.rs`](../../../crates/npc_vehicles/src/vehicle.rs) · **Capability:** vehicles (`VehiclesPlugin`, needs `InteractionPlugin`)

A vehicle: a sim entity with a `SimPos` and seats that moves through the locomotion seam like any agent
(`MoveIntent`, optionally `NavAgent`). `#[require(Occupants)]`, so inserting it also inserts an empty
[`Occupants`](Occupants.md). The control mode decides who moves it:

- `ByCrew` (default): only its driver moves it. The driver's `DriveVehicle` action writes the vehicle's
  `MoveIntent`. With no driver aboard (or a required seat empty) it cannot move.
- `Direct`: it is a unit itself (own `MoveIntent`, `NavAgent`, orders, brain); the crew rides along.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `seats` | `Vec<Seat>` | index = seat number (`u8`, < 255) |
| `speed_mm_per_tick` | `i32` | mm/tick; speed of driven moves and `command_vehicle_to` |
| `control` | `VehicleControl` | `ByCrew` or `Direct` |

`Seat { role: SeatRole, required: bool, offset: SimPos }`: `SeatRole` is `Driver`, `Passenger` (default) or
`Gunner`; `required` = the vehicle cannot move while it is empty (`Seat::driver()` is required); `offset` is
mm added to the vehicle's `SimPos` (world axes, vehicles have no heading).

## Lifecycle

- **Added by:** content (testbed `cart_vehicle` / `wagon_vehicle` in
  [`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** `mount` verb offer/precondition ([`verbs.rs`](../../../crates/npc_vehicles/src/verbs.rs)),
  `can_move`, `driver_of`, `seat_pos`, `choose_seat`, `driven_vehicle`; systems
  [`vehicle_gate_system`](../systems/vehicle_gate_system.md), [`seat_follow_system`](../systems/seat_follow_system.md),
  [`claim_seats_system`](../systems/claim_seats_system.md), [`drive_vehicle_system`](../systems/drive_vehicle_system.md);
  glue `vehicles_agent.rs` (affordances, section, and busy predicate `npc_vehicles`: a `Direct` vehicle is busy for
  agents while its own intent is `Moving` / `Blocked`), `command_vehicles.rs`; `npc_view_helpers::vehicle_gizmos`;
  testbed view.
- **Written / removed by:** nothing in the framework.

## Hashing and snapshots

- **State hash:** yes, as `vehicle` (`register_vehicle_hash`, [`hash.rs`](../../../crates/npc_vehicles/src/hash.rs)):
  per seat role code, `required`, offset; speed; control as `u8` (ByCrew 0, Direct 1). The plugin also turns
  on `move_intent` / `move_status` hashing (`register_locomotion_hash`).
- **World snapshot (ADR 0013):** saved as `vehicle` (automatic with hashing).

## Related

- [`Occupants`](Occupants.md), [`MountedIn`](MountedIn.md) — who sits where.
- [`DriveGoal`](DriveGoal.md), [`Driving`](Driving.md) — how a `ByCrew` driver steers it.

## Example

```rust
use npc_framework::vehicles::{Seat, Vehicle};

let cart = Vehicle::by_crew(60).seat(Seat::driver()).seat(Seat::passenger().offset(0, -700));
spawn_sim(world, (SimPos::new(4000, -2000), cart, NavAgent));
// {"type":"Mount","unit":5,"vehicle":30,"role":"Driver"}
```
