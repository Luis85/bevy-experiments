---
type: System
---
# `advance_clock_system`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/clock.rs`](../../../crates/npc_kernel/src/clock.rs) · **Plugin:** `NpcKernelPlugin`

`SimClock.tick += 1`: the last thing every `NpcSimTick` run does. Sim time is only this counter; the tick rate
(`SimRate`, default 20 Hz) is the driver's business and never seen by sim systems.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Cleanup`, not exclusive
- **Ordering:** `(flush_journal_system, advance_clock_system).chain()`; anything else in `Cleanup` that reads the
  tick should order itself before this system.

## Reads / writes

| Access | Data |
|---|---|
| reads | — |
| writes | `ResMut<SimClock>` (`tick: u64`) |
| messages / events | none |

Between ticks `SimClock.tick` is the *next* tick to run (commands submitted then default to it; snapshots store it).

## Determinism

Exactly +1 per schedule run, integer. `SimClock` is part of the state hash (the hash starts with `tick`) and
saved as resource `sim_clock`.

## Performance

- **Mechanic tag:** `clock` (via `tags.rs` prefix `npc_kernel::clock`) · **Budget:** 10 µs + 0 per agent
  (`NPC_BUDGET_TABLE`) · no spans.

## Related

- [`flush_journal_system`](flush_journal_system.md) — chained before.
- [`drive_sim`](drive_sim.md) — decides how many ticks run per host frame.
- [`day_night_system`](day_night_system.md), [`rethink_timer_system`](rethink_timer_system.md) — read the tick.
