---
type: System
---
# `idle_system` (village)

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Plugin:** `VillagePlugin`

Counts a villager's `Idle.remaining` down and sets the leaf to `Success` at 0. (The testbed has its own
`idle_system` for `TbIdling`: [`testbed_content.idle_system`](testbed_content.idle_system.md).)

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Actions`, not exclusive
- **Ordering:** last in the village action chain (after `sleep_system`).

## Reads / writes

| Access | Data |
|---|---|
| reads | `ActiveLeaf.status` (skips non-`Running`) |
| writes | `Idle.remaining`, `ActiveLeaf.status` |
| messages / events | none |

## Determinism

Per-agent, integer only. The idle length was drawn at start with `sim_range` (counter-based RNG, purpose
`RNG_IDLE`).

## Performance

- **Mechanic tag:** `actions.idle` (prefix `village_content::actions::idle_system`) · **Budget:** 10 µs +
  2 µs/villager (`VILLAGE_BUDGETS`) · no span.

## Related

- [`sleep_system`](sleep_system.md) — runs just before it.
- Components: [`Idle`](../components/Idle.md), [`Activity`](../components/Activity.md)
