---
type: Component
---
# `Well`

**Crate:** `village_content` · **File:** [`content/village_content/src/world.rs`](../../../content/village_content/src/world.rs) · **Capability:** village content

Drinking water for any villager. Up to `slots` villagers drink at once; each claims one slot through
`Reservations` (capacity `slots`). The water never runs out. The thirst rate is the well's `Provides`
(thirst).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `slots` | `u32` | villagers that may drink at the same time (scenario: `WELL_SLOTS` = 2) |

## Lifecycle

- **Added by:** `spawn_well` in [`scenario.rs`](../../../content/village_content/src/scenario.rs), with
  `Place { kind: Well }`, `SimPos`, `Approach` and `Provides { need: thirst, amount_per_tick:
  WELL_THIRST_PER_TICK }` (2500). It is spawned after the plot, so in `village` it is `SimId` 6.
- **Read by:** `find_well` (nearest by approach, ties by `SimId`, with `free_slots(.., slots, me) > 0`) in
  [`villager.rs`](../../../content/village_content/src/villager.rs) (conditions `thirst_urgent_well`,
  `thirsty_well`); `reserve_well_start` (claims with capacity `slots`), `drink_water_start` (target must be
  a well) and [`drink_system`](../systems/drink_system.md) (`Provides` with `With<Well>`) in
  [`actions.rs`](../../../content/village_content/src/actions.rs); the `drink` agent affordance; the
  view (`PlaceKind::Well` model).
- **Written / removed by:** never.

## Hashing and snapshots

- **State hash:** no (no `SimHash` impl).
- **World snapshot (ADR 0013):** saved as `well` (snapshot-only, `register_snapshot_component_named` in
  `register_snapshot_entries`). Not reflected.

## Related

- [`Drink`](Drink.md) — the running drink action.
- [`Provides`](Provides.md), [`Approach`](Approach.md) — the advertisement it is found by.

## Example

```rust
spawn_sim(world, (
    Place { kind: PlaceKind::Well, yaw_deg: 0 },
    SimPos::new(-4500, 4500),
    Approach(SimPos::new(-4500, 3300)),
    Well { slots: WELL_SLOTS },
    Provides { need: thirst, amount_per_tick: WELL_THIRST_PER_TICK },
));
```
