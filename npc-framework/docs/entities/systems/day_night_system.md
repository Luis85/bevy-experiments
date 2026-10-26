---
type: System
---
# `day_night_system`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/clock.rs`](../../../crates/npc_kernel/src/clock.rs) · **Plugin:** `NpcKernelPlugin`

On the first tick of every night and every day, gives every `Brain` agent a `ThinkRequest`: nightfall and
morning are re-think triggers. Journals `PhaseChange { night }`.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Needs`, not exclusive
- **Ordering:** `(day_night_system, rethink_timer_system).chain()`; `npc_needs::decay_needs_system` runs
  `.before(day_night_system)`

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimClock.tick`, `SimConfig` (`day_length`, default 2400 ticks; `night_permille`, default 300 = the last 30% of each day), `Entity` with `Brain` |
| writes | `Commands`: inserts `ThinkRequest` on every `Brain` agent (applied before `NpcSet::Think`) |
| messages / events | `Journal::push(tick, None, PhaseChange { night })` (a no-op while the journal is disabled) |

`SimConfig::phase_changes_at(tick)` is true when `is_night(tick) != is_night(tick - 1)`, never at tick 0.

## Determinism

Pure integer tick arithmetic (`time_of_day_permille = (tick % day_length) * 1000 / day_length`). Inserting a
marker is per-entity, so query order does not matter. No wall clock is read.

## Performance

- **Mechanic tag:** `clock` (via `tags.rs` prefix `npc_kernel::clock`) · **Budget:** 10 µs + 0 per agent
  (`NPC_BUDGET_TABLE`) · no spans.

## Related

- [`rethink_timer_system`](rethink_timer_system.md) — the other re-think trigger, chained after.
- [`think_system`](think_system.md) — consumes the requests the same tick.
- Components: [`ThinkRequest`](../components/ThinkRequest.md), [`Brain`](../components/Brain.md)
