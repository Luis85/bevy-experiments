---
type: Component
---
# `Bed`

**Crate:** `village_content` · **File:** [`content/village_content/src/world.rs`](../../../content/village_content/src/world.rs) · **Capability:** village content

A bed of one home. A villager of that home claims it (capacity 1) and sleeps in it. The energy rate comes
from the bed's `Provides` (energy), not from this component.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `home` | `SimId` | the `Home` this bed belongs to |

## Lifecycle

- **Added by:** `spawn_plot` in [`scenario.rs`](../../../content/village_content/src/scenario.rs), with
  `Place { kind: Bed, yaw_deg: 90 }`, `Approach` and `Provides { need: energy, amount_per_tick:
  BED_ENERGY_PER_TICK }` (200 milli-units per tick).
- **Read by:** `find_bed` (lowest-`SimId` bed of the villager's home that is free for it) in
  [`villager.rs`](../../../content/village_content/src/villager.rs); `sleep_in_bed_start` (the target must be
  a bed) and [`sleep_system`](../systems/sleep_system.md) (its `Provides`) in
  [`actions.rs`](../../../content/village_content/src/actions.rs); the `sleep` agent affordance; the lying
  pose in [`villager_transforms`](../systems/villager_transforms.md).
- **Written / removed by:** never.

## Hashing and snapshots

- **State hash:** no (no `SimHash` impl). Who sleeps where shows up in the hashed [`Sleep`](Sleep.md)
  component and the `reservations` resource.
- **World snapshot (ADR 0013):** saved as `bed` (snapshot-only, `register_snapshot_component_named` in
  `register_snapshot_entries`). Not reflected.

## Related

- [`Home`](Home.md) — its owner.
- [`Sleep`](Sleep.md) — the running sleep action, which names the bed.
- [`Provides`](Provides.md) — the energy rate.

## Example

```rust
spawn_sim(world, (
    Place { kind: PlaceKind::Bed, yaw_deg: 90 },
    SimPos::new(-2500, -5500),
    Approach(SimPos::new(-2500, -4000)),
    Bed { home: house },
    Provides { need: energy, amount_per_tick: 200 },
));
```
