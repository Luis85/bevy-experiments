---
type: System
---
# `external_action_system`

**Crate:** `npc_agent_io` · **File:** [`crates/npc_agent_io/src/control.rs`](../../../crates/npc_agent_io/src/control.rs) · **Plugin:** `AgentIoPlugin`

The `ExternalAction` leaf's own system: it advances every `Direct` unit's [`ExternalQueue`](../components/ExternalQueue.md).
It is not scheduled itself; it takes a `Pass` argument and the wrapper's cached `ExternalQueueQuery`
(`QueryState<(&SimId, Entity), With<ExternalQueue>>`), and runs through two thin wrappers,
[`external_dispatch_system`](external_dispatch_system.md) (`Pass::Dispatch`) and
[`external_advance_system`](external_advance_system.md) (`Pass::Advance`).

Per unit with an `ExternalQueue`:

- not (or no longer) `Direct`: `cancel_all`, remove the `ExternalQueue`, and end a running `ExternalAction`
  leaf with `Failure` so the tree takes over;
- the tree has not picked the queue up yet (`ActiveLeaf` is not `ExternalAction`): only clear a moot
  `cancel_current`, then skip;
- `cancel_current` set: abort the current action (its running step is stopped with `StopReason::Aborted`);
- else, when the leaf status is no longer `Running`, a behaviour step finished (its own system wrote the
  status): stop it with `Finished`, go to the next step on `Success`, journal `failed` otherwise;
- `Advance` pass only: count a running `Wait` step down by one;
- `advance`: start steps until one is running or the queue is empty, then write the leaf status: `Running`
  while working or idling (`Idle` policy), `Failure` when there is nothing to do under `Autopilot`.

## Schedule

- **Schedule:** `NpcSimTick`, through the two wrappers
- **Set:** `AgentIoSet::Dispatch` and `AgentIoSet::Advance`; exclusive (`external_action_system(world: &mut World,
  pass: Pass, queues: &mut ExternalQueueQuery)`)
- **Ordering:** `Dispatch.after(NpcSet::Inputs).before(NpcSet::Needs)`, `Advance.after(NpcSet::Effects)
  .before(NpcSet::Cleanup)` (`control::configure_sets`)

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`, `ExternalQueue`, `ExternalController`, `ActiveLeaf`; res `AiRegistry` (action fns by name) |
| writes | `ExternalQueue` (all fields; removed when not `Direct`), `ActiveLeaf.status`, `Blackboard` (`SetBlackboard` steps), anything a step's action `start` / `stop` writes; res `ExternalActionId` (cached once after the freeze) |
| messages / events | journal `ExternalAction { handle, verb, status, latency_ticks }` (`started`, `done`, `failed`, `cancelled`); `Dispatch` only: `journal_skipped_commands` (see the dispatch doc) |

## Determinism

Collects `(SimId, Entity)` for `With<ExternalQueue>`, sorts by `SimId`, handles one unit at a time. The
`advance` loop is bounded (`MAX_QUEUE * (MAX_STEPS + 2) + 2`). The queue changes only through recorded
commands (`AgentAct`, `AgentCancel`, `AgentClear`, `AttachController`, `DetachController`), so a replay runs the
session without the agent. Returns early before the content freeze (no `ExternalAction` id yet).

## Performance

- **Mechanic tag:** `agent_io` (via `tags.rs` prefix `npc_agent_io`) · **Budget:** 15 µs fixed + 2 µs per agent
  (`NPC_BUDGET_TABLE`), for both passes · spans opened: none. Step actions started here run inside this time.

## Related

- [`command_engine_system`](command_engine_system.md) — finishes an `ExecuteOrder` step before the think;
  `external_action_stop` then treats the `Finished` stop as completed, not cancelled.
- Components: [`ExternalQueue`](../components/ExternalQueue.md), [`ExternalController`](../components/ExternalController.md)
