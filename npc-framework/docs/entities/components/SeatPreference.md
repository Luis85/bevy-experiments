---
type: Component
---
# `SeatPreference`

**Crate:** `npc_vehicles` · **File:** [`crates/npc_vehicles/src/vehicle.rs`](../../../crates/npc_vehicles/src/vehicle.rs) · **Capability:** vehicles (`VehiclesPlugin`)

Optional on an actor: the seat role it prefers when it mounts **without** naming one (the `Interact "mount"`
leaf or command, the `Mount` command without `role`, agent `mount@<vehicle>`). `default_seat` then takes the
seat the actor already claimed, else the first free seat (index order) of this role, else the first free
seat of any role. A `Mount` command **with** an explicit role is strict and ignores it.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `0` | `SeatRole` | `Driver`, `Passenger` or `Gunner` |

## Lifecycle

- **Added by:** content. No framework or testbed code inserts it on this checkout.
- **Read by:** `preferred_role` → `default_seat` ([`seats.rs`](../../../crates/npc_vehicles/src/seats.rs)), used by
  `mount`, the `mount` verb's precondition/effect and [`claim_seats_system`](../systems/claim_seats_system.md).
- **Written / removed by:** nothing in the framework.

## Hashing and snapshots

- **State hash:** yes, as `seat_preference` (`register_vehicle_hash`, [`hash.rs`](../../../crates/npc_vehicles/src/hash.rs)):
  role code as `u8` (Driver 0, Passenger 1, Gunner 2).
- **World snapshot (ADR 0013):** saved as `seat_preference` (automatic with hashing).

## Related

- [`Occupants`](Occupants.md) — which seats are free.
- [`Vehicle`](Vehicle.md) — seat roles.

## Example

```rust
use npc_framework::vehicles::{SeatPreference, SeatRole};

world.entity_mut(guard).insert(SeatPreference(SeatRole::Gunner));
// {"type":"Interact","unit":7,"verb":"mount","target":30}  -> first free Gunner seat, else any free seat
```
