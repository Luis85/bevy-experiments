---
type: Component
---
# `Eat`

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Capability:** village content (actions)

A running meal at a stockpile (leaf `EatFood`). After `EAT_TICKS` (60) it restores hunger by the stockpile's
`Provides × EAT_TICKS` and takes `MEAL_FOOD` (1) food. An interrupted meal restores nothing and takes
nothing. `SparseSet` storage.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `stockpile` | `SimId` | the claimed stockpile (`BB_TARGET`) |
| `remaining` | `u32` | ticks left (starts at `EAT_TICKS` = 60) |

## Lifecycle

- **Added by:** `eat_food_start` (the target must have a `Stockpile`) in
  [`actions.rs`](../../../content/village_content/src/actions.rs). Also sets `Activity::Eat`.
- **Read / written by:** [`eat_system`](../systems/eat_system.md) (decrements; fails when the pile is gone
  or empty; at 0 adds the meal to `Needs`, writes `TakeFood`, releases the claim and sets `Success`).
- **Removed by:** `stop_eat` (takes it; the claim is normally already released by `eat_system`).

## Hashing and snapshots

- **State hash:** yes, as `eat` (`stockpile.0`, `remaining`; `register_sim_component` in
  `register_components`).
- **World snapshot (ADR 0013):** saved as `eat` (automatic with hashing).

## Related

- [`Stockpile`](Stockpile.md), [`Provides`](Provides.md) — the food and its hunger rate.
- [`Needs`](Needs.md) — what the meal restores.
- [`Activity`](Activity.md) — `Eat` while it runs.

## Example

```rust
// Tree (villager.rs): ReserveFood -> MoveToDest -> EatFood.
b.sequence("EatSeq", |b| {
    b.leaf("ReserveFood", "ReserveFood", ());
    b.leaf("MoveToDest", "MoveToDest", ());
    b.leaf("EatFood", "EatFood", ());
});
```
