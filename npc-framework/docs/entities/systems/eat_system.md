---
type: System
---
# `eat_system`

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Plugin:** `VillagePlugin`

Runs every meal (`Eat`). It counts `remaining` down and fails when the stockpile is gone or empty. At 0 it
adds `Provides.amount_per_tick × EAT_TICKS` to the matching need (clamped to `0..=NEED_MAX`), writes a
`TakeFood { amount: MEAL_FOOD }` effect, releases the eater's stockpile claim and sets `Success`. The claim
goes in the same tick as the food it stood for.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Actions`, not exclusive
- **Ordering:** third in the village action chain (after `move_to_system`, before `drink_system`).

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`; `Stockpile`, `Option<Provides>` (target); `SimIndex`, `SimClock` |
| writes | `Eat.remaining`, `Needs` (the `Provides.need` entry), `ActiveLeaf.status`; `Reservations` (release); `Journal` (`Release { target, reason: Finished }`, when enabled) |
| messages / events | writes `SimEffect { source: me, kind: TakeFood { stockpile, amount: 1 } }` |

## Determinism

Needs math is integer (`saturating_mul`, `clamp`). The stockpile is not changed here: the food comes off in
[`resolve_effects_system`](resolve_effects_system.md), sorted by `(source, kind)`. Finished meals are
collected as `(SimId, stockpile)` and sorted before the claims are released and journaled, because query
order is not `SimId` order.

## Performance

- **Mechanic tag:** `actions.eat` (prefix `village_content::actions::eat_system`) · **Budget:** 10 µs +
  2 µs/villager (`VILLAGE_BUDGETS`, registered with feature `perf`) · no span.

## Related

- [`drink_system`](drink_system.md) — same pattern for the well.
- [`resolve_effects_system`](resolve_effects_system.md) — applies `TakeFood`.
- Components: [`Eat`](../components/Eat.md), [`Stockpile`](../components/Stockpile.md),
  [`Provides`](../components/Provides.md), [`Needs`](../components/Needs.md)
