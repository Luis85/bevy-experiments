---
type: Component
---
# `MountedIn`

**Crate:** `npc_vehicles` · **File:** [`crates/npc_vehicles/src/vehicle.rs`](../../../crates/npc_vehicles/src/vehicle.rs) · **Capability:** vehicles (`VehiclesPlugin`)

The actor side of "this actor sits in `vehicle`, seat `seat`" (by `SimId`, so it hashes and orders
deterministically; [`Occupants`](Occupants.md) is the vehicle side). While present the actor's `SimPos` is
set every tick to the vehicle's position plus the seat offset, its own `MoveIntent` is dropped (no mover
moves it), and its tree can branch on the conditions `Mounted`, `MountedAsDriver` and `HasDriveGoal`. A view
can read it to play a sitting pose; interpolation needs nothing extra.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `vehicle` | `SimId` | the vehicle |
| `seat` | `u8` | seat index in `Vehicle::seats` |

## Lifecycle

- **Added by:** `seat_actor` ([`seats.rs`](../../../crates/npc_vehicles/src/seats.rs)), the `mount` verb's
  effect, applied in [`complete_interactions_system`](../systems/complete_interactions_system.md); also
  inserts the seat position and `ThinkRequest`, journals `Mounted { vehicle, seat, role }`.
- **Read by:** [`vehicle_gate_system`](../systems/vehicle_gate_system.md), [`seat_follow_system`](../systems/seat_follow_system.md),
  `mounted_in`, `driven_vehicle`, the three conditions, `mount` / `dismount` verb checks, the dismount-spot
  search (mounted units do not occupy cells); glue `command_vehicles.rs`; `npc_view_helpers::vehicle_gizmos`;
  testbed content/view.
- **Written / removed by:** `unseat` (`dismount`, `Unload`; journal `Dismounted`, `ThinkRequest`);
  `vehicle_gate_system` removes it when the vehicle is gone or no longer seats the actor.

## Hashing and snapshots

- **State hash:** yes, as `mounted_in` (`register_vehicle_hash`, [`hash.rs`](../../../crates/npc_vehicles/src/hash.rs)):
  vehicle id, seat.
- **World snapshot (ADR 0013):** saved as `mounted_in` (automatic with hashing).

## Related

- [`Occupants`](Occupants.md), [`Vehicle`](Vehicle.md), [`Driving`](Driving.md).

## Example

```rust
use npc_framework::vehicles::MountedIn;

// {"type":"Mount","unit":5,"vehicle":30}  -> later: MountedIn { vehicle: SimId(30), seat: 0 }
if let Some(m) = world.get::<MountedIn>(unit) { println!("in #{} seat {}", m.vehicle.0, m.seat); }
```
