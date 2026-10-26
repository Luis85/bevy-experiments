---
type: System
---
# `work_system`

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Plugin:** `VillagePlugin`

Runs every work shift (`Work`). It counts `remaining` down and fails when the work site is gone. At 0 it writes
an `AddFood` effect for the site's stockpile and yield, and sets `Success`. An interrupted shift yields
nothing. The site claim is released later, by `stop_work` on `Finished`.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Actions`, not exclusive
- **Ordering:** fifth in the village action chain (after `drink_system`, before `sleep_system`).

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`; `WorkSite` (target); `SimIndex` |
| writes | `Work.remaining`, `ActiveLeaf.status` |
| messages / events | writes `SimEffect { source: me, kind: AddFood { stockpile: site.stockpile, amount: site.yield_food } }` |

## Determinism

Per-agent; the cross-entity stockpile change goes through `SimEffect` and is applied sorted in
[`resolve_effects_system`](resolve_effects_system.md), so query order does not matter.

## Performance

- **Mechanic tag:** `actions.work` (prefix `village_content::actions::work_system`) · **Budget:** 10 µs +
  2 µs/villager (`VILLAGE_BUDGETS`) · no span.

## Related

- [`resolve_effects_system`](resolve_effects_system.md) — adds the food, clamped to capacity.
- Components: [`Work`](../components/Work.md), [`WorkSite`](../components/WorkSite.md),
  [`Stockpile`](../components/Stockpile.md)
