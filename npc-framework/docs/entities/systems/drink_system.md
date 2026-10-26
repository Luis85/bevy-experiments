---
type: System
---
# `drink_system`

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Plugin:** `VillagePlugin`

Runs every drink at a well (`Drink`). It counts `remaining` down and fails when the well is gone or does not
provide thirst. At 0 it adds `Provides.amount_per_tick × DRINK_TICKS` to thirst (clamped), releases the
drinker's well slot and sets `Success`. The water never runs out, so there is no `SimEffect`. Returns early
when no `Thirst` need is registered.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Actions`, not exclusive
- **Ordering:** fourth in the village action chain (after `eat_system`, before `work_system`).

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`; `Provides` `With<Well>` (target); `SimIndex`, `NeedRegistry` (`id(THIRST)`), `SimClock` |
| writes | `Drink.remaining`, `Needs` (thirst), `ActiveLeaf.status`; `Reservations` (release); `Journal` (`Release { target, reason: Finished }`, when enabled) |
| messages / events | none |

## Determinism

Integer needs math. Finished drinks are collected as `(SimId, well)` and sorted before releasing and
journaling. The need id is looked up by name (`NeedRegistry::id`), never by registration order.

## Performance

- **Mechanic tag:** `actions.drink` (prefix `village_content::actions::drink_system`) · **Budget:** 10 µs +
  2 µs/villager (`VILLAGE_BUDGETS`) · no span.

## Related

- [`eat_system`](eat_system.md) — the same pattern for meals.
- Components: [`Drink`](../components/Drink.md), [`Well`](../components/Well.md),
  [`Provides`](../components/Provides.md), [`Needs`](../components/Needs.md)
