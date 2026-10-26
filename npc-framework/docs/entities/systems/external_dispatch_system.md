---
type: System
---
# `external_dispatch_system`

**Crate:** `npc_agent_io` · **File:** [`crates/npc_agent_io/src/control.rs`](../../../crates/npc_agent_io/src/control.rs) · **Plugin:** `AgentIoPlugin`

The first of the two `ExternalAction` passes: `external_action_system(world, Pass::Dispatch, queues)`. It runs right
after the tick's commands applied, so an action queued by `AgentAct` starts in the same tick, and a detach
hands control back before `Think`. Before the queue work it journals agent commands that were skipped.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `AgentIoSet::Dispatch`; exclusive (`external_dispatch_system(world: &mut World, queues: &mut
  ExternalQueueQuery)`, its own cached `QueryState` of the units with an `ExternalQueue`)
- **Ordering:** `AgentIoSet::Dispatch.after(NpcSet::Inputs).before(NpcSet::Needs)` (`control::configure_sets`,
  called from `AgentIoPlugin::build` in [`lib.rs`](../../../crates/npc_agent_io/src/lib.rs))

## Reads / writes

| Access | Data |
|---|---|
| reads | as [`external_action_system`](external_action_system.md); plus res `SkippedCommands` (kernel tooling record), `SkippedSeen` |
| writes | as [`external_action_system`](external_action_system.md) (no `Wait` countdown in this pass); res `SkippedSeen` (tooling, not hashed) |
| messages / events | `journal_skipped_commands`: a skipped `AgentAct` → `ExternalAction { status: "rejected" }` + `ActionRejected { controller, verb, reason: "skipped at tick T: .." }`; other skipped agent commands, or any skipped command for a controlled unit → `ActionRejected { verb: <command type> }`. Then the queue journal kinds |

## Determinism

`journal_skipped_commands` reads only the kernel's record of skipped commands (in order) and writes only the
journal and `SkippedSeen`: sim state is untouched. The queue pass is the shared body: units sorted by `SimId`.

## Performance

- **Mechanic tag:** `agent_io` (via `tags.rs` prefix `npc_agent_io`) · **Budget:** 15 µs fixed + 2 µs per agent
  (`NPC_BUDGET_TABLE`, shared with the advance pass) · spans opened: none

## Related

- [`external_action_system`](external_action_system.md) — the shared body.
- [`external_advance_system`](external_advance_system.md) — the second pass.
- Components: [`ExternalQueue`](../components/ExternalQueue.md), [`ExternalController`](../components/ExternalController.md)
