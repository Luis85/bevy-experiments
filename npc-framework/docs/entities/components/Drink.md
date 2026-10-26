---
type: Component
---
# `Drink`

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Capability:** village content (actions)

A running drink at a well (leaf `DrinkWater`). After `DRINK_TICKS` (40) it restores thirst by the well's
`Provides × DRINK_TICKS`. The water never runs out, so there is no cross-entity effect. An interrupted drink
restores nothing. `SparseSet` storage.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `well` | `SimId` | the well whose slot is claimed (`BB_TARGET`) |
| `remaining` | `u32` | ticks left (starts at `DRINK_TICKS` = 40) |

## Lifecycle

- **Added by:** `drink_water_start` (the target must have a `Well`) in
  [`actions.rs`](../../../content/village_content/src/actions.rs). Also sets `Activity::Drink`.
- **Read / written by:** [`drink_system`](../systems/drink_system.md) (decrements; fails when the well is gone
  or provides no thirst; at 0 adds the water, releases the slot and sets `Success`).
- **Removed by:** `stop_drink` (takes it; the slot is normally already released by `drink_system`).

## Hashing and snapshots

- **State hash:** yes, as `drink` (`well.0`, `remaining`; `register_sim_component` in `register_components`).
- **World snapshot (ADR 0013):** saved as `drink` (automatic with hashing).

## Related

- [`Well`](Well.md), [`Provides`](Provides.md) — the water and its thirst rate.
- [`Activity`](Activity.md) — `Drink` while it runs (the view reuses the eating clip).

## Example

```rust
// Tree (villager.rs): ReserveWell -> MoveToDest -> DrinkWater.
b.sequence("DrinkSeq", |b| {
    b.leaf("ReserveWell", "ReserveWell", ());
    b.leaf("MoveToDest", "MoveToDest", ());
    b.leaf("DrinkWater", "DrinkWater", ());
});
```
