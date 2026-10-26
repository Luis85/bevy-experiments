---
type: System
---
# `rethink_timer_system`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/clock.rs`](../../../crates/npc_kernel/src/clock.rs) · **Plugin:** `NpcKernelPlugin`

The staggered re-think timer. Every `Brain` agent gets a `ThinkRequest` when
`(tick + sim_id) % RETHINK_INTERVAL == 0` (`RETHINK_INTERVAL = 40`), so each agent re-thinks at least every 40
ticks, and agents are spread over different ticks. This keeps a long-running leaf (a night's sleep) from hiding
a reactive guard that became true without a need-level change (food appeared).

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Needs`, not exclusive
- **Ordering:** `(day_night_system, rethink_timer_system).chain()`

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimClock.tick`, `SimId` (with `Brain`) |
| writes | `Commands`: inserts `ThinkRequest` (applied before `NpcSet::Think`) |
| messages / events | none |

## Determinism

The decision depends only on the tick and the agent's `SimId` (`wrapping_add`, integer modulo). Purely
per-entity, so iteration order does not matter.

## Performance

- **Mechanic tag:** `think` (via `tags.rs` prefix `npc_kernel::clock::rethink_timer_system`, which beats the
  shorter `npc_kernel::clock` → `clock`) · **Budget:** 40 µs + 20 µs per agent (`NPC_BUDGET_TABLE`, the shared
  `think` row) · no spans.

## Related

- [`day_night_system`](day_night_system.md) — chained before.
- [`think_system`](think_system.md) — consumes the requests.
- Components: [`ThinkRequest`](../components/ThinkRequest.md), [`SimId`](../components/SimId.md), [`Brain`](../components/Brain.md)
