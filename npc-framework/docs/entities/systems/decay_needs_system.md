---
type: System
---
# `decay_needs_system`

**Crate:** `npc_needs` · **File:** [`crates/npc_needs/src/lib.rs`](../../../crates/npc_needs/src/lib.rs) · **Plugin:** `NpcNeedsPlugin` (always in `NpcPlugins`)

The needs tick. For every agent with [`Needs`](../components/Needs.md), each need decays by its def's
`decay_per_tick` (clamped to `0..=NEED_MAX`), unless [`PauseDecay`](../components/PauseDecay.md) pauses it.
Then its `level` is recomputed with hysteresis (`update_level`). Every agent whose level changed on any need
gets a `ThinkRequest`, so the BT re-thinks this tick. Level changes are journaled as `NeedLevel`.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Needs`, not exclusive
- **Ordering:** `.before(npc_kernel::clock::day_night_system)`. The kernel chains
  `(day_night_system, rethink_timer_system)` in the same set.

## Reads / writes

| Access | Data |
|---|---|
| reads | `NeedRegistry` (defs: `decay_per_tick`, `thresholds`, `order`, `name`), `PauseDecay` (optional), `SimId` (optional, journal only), `SimClock` (optional, journal tick) |
| writes | `Needs` (`value`, `level`), `Journal` (optional, only when `enabled`) |
| commands | inserts `ThinkRequest` on agents whose level changed |
| journal | `NeedLevel { need, from, to }` per change, sorted by `(SimId, def order, name)` |

The `i`-th def is applied to `values[i]`. An agent whose `values` is shorter than the registry skips the
missing needs.

## Determinism

The work is per entity, so query order does not matter. Integer subtraction and clamp only. Journal entries
are collected as `(SimId, order, name, from, to)` and `sort_unstable`d before they are pushed, so the
journal does not depend on query order. Agents without `SimId` still decay but are not journaled. The
journal is written only when it exists and is enabled; nothing else depends on it.

## Performance

- **Mechanic tag:** `needs` (via `tags.rs` prefix `npc_needs`) · **Budget:** 15 µs fixed + 2 µs per agent
  (`NPC_BUDGET_TABLE`) · spans opened: none.

## Related

- `day_night_system` (kernel, `NpcSet::Needs`) — runs after it.
- `think_system` (kernel, `NpcSet::Think`) — consumes the `ThinkRequest`s.
- Components: [`Needs`](../components/Needs.md), [`PauseDecay`](../components/PauseDecay.md)
