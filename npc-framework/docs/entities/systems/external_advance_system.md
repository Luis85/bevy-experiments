---
type: System
---
# `external_advance_system`

**Crate:** `npc_agent_io` · **File:** [`crates/npc_agent_io/src/control.rs`](../../../crates/npc_agent_io/src/control.rs) · **Plugin:** `AgentIoPlugin`

The second of the two `ExternalAction` passes: `external_action_system(world, Pass::Advance, queues)`. It runs after
the tick's actions and effects, so the next step of a queued action starts as soon as the previous one
completed (no tick lost waiting for the next think). It is also the only pass that counts `Wait` steps down,
once per tick, so a `Wait { ticks }` ends after `ticks` advance passes.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `AgentIoSet::Advance`; exclusive (`external_advance_system(world: &mut World, queues: &mut
  ExternalQueueQuery)`, its own cached `QueryState` of the units with an `ExternalQueue`)
- **Ordering:** `AgentIoSet::Advance.after(NpcSet::Effects).before(NpcSet::Cleanup)` (`control::configure_sets`,
  called from `AgentIoPlugin::build` in [`lib.rs`](../../../crates/npc_agent_io/src/lib.rs))

## Reads / writes

| Access | Data |
|---|---|
| reads | as [`external_action_system`](external_action_system.md) |
| writes | as [`external_action_system`](external_action_system.md), plus `RunningAction.wait_left` (minus 1; at 0 the step ends) |
| messages / events | journal `ExternalAction { handle, verb, status }` (`started`, `done`, `failed`, `cancelled`) |

## Determinism

The shared body: units collected as `(SimId, Entity)`, sorted by `SimId`, handled one at a time; integer
countdown with `saturating_sub`; no rng, no clock.

## Performance

- **Mechanic tag:** `agent_io` (via `tags.rs` prefix `npc_agent_io`) · **Budget:** 15 µs fixed + 2 µs per agent
  (`NPC_BUDGET_TABLE`, shared with the dispatch pass) · spans opened: none

## Related

- [`external_action_system`](external_action_system.md) — the shared body.
- [`external_dispatch_system`](external_dispatch_system.md) — the first pass.
- Components: [`ExternalQueue`](../components/ExternalQueue.md), [`ExternalController`](../components/ExternalController.md)
