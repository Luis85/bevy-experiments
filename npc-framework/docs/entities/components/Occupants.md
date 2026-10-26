---
type: Component
---
# `Occupants`

**Crate:** `npc_vehicles` · **File:** [`crates/npc_vehicles/src/vehicle.rs`](../../../crates/npc_vehicles/src/vehicle.rs) · **Capability:** vehicles (`VehiclesPlugin`)

The crew view of a [`Vehicle`](Vehicle.md): one slot per seat (index = seat index). It is the vehicle side of
the "sits in" relationship whose actor side is [`MountedIn`](MountedIn.md). Only `npc_vehicles` writes it
(the setter is `pub(crate)`); read it, never write it. Seats are claimed one at a time while a `mount` runs,
so two units never take the same seat (the lower `SimId` wins a tie).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `seats` | `Vec<SeatSlot>` | per seat: `Free`, `Claimed(SimId)` (mount in progress) or `Taken(SimId)`; missing index = `Free` |

Readers: `slot(seat)`, `units()` (ascending ids aboard), `seated()` (`(seat, unit)` in seat order),
`seat_of(unit)`, `claim_of(unit)`, `is_empty()`.

## Lifecycle

- **Added by:** `#[require(Occupants)]` on `Vehicle` (empty default).
- **Read by:** `can_move` / `crew_allows_move`, `driver_of`, `choose_seat` / `default_seat`, `mount`,
  [`vehicle_gate_system`](../systems/vehicle_gate_system.md), [`claim_seats_system`](../systems/claim_seats_system.md),
  `vehicle_json`; glue `vehicles_agent.rs`; `npc_vehicles` commands; `npc_view_helpers::vehicle_gizmos`.
- **Written / removed by:** `claim_seat` / `release_claim` (`mount`, `claim_seats_system`), `seat_actor`
  (mount effect: `Taken`), `unseat` (dismount: `Free`), `vehicle_gate_system` (frees seats whose occupant is
  gone or no longer `MountedIn` that seat).

## Hashing and snapshots

- **State hash:** yes, as `occupants` (`register_vehicle_hash`, [`hash.rs`](../../../crates/npc_vehicles/src/hash.rs)):
  slot count, then per slot a `u8` tag (Free 0, Claimed 1, Taken 2) plus the id for `Claimed` / `Taken`.
- **World snapshot (ADR 0013):** saved as `occupants` (automatic with hashing).

## Related

- [`MountedIn`](MountedIn.md) — the other side; kept in step by this crate.
- [`SeatPreference`](SeatPreference.md) — which free seat a mount picks.

## Example

```rust
use npc_framework::vehicles::{Occupants, driver_of};

let occ = world.get::<Occupants>(cart).unwrap();
let aboard = occ.units();                       // ascending SimId
let driver = driver_of(world, cart).map(|(id, _)| id);
```
