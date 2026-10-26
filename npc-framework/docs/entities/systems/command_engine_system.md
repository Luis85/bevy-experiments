---
type: System
---
# `command_engine_system`

**Crate:** `npc_command` · **File:** [`crates/npc_command/src/exec.rs`](../../../crates/npc_command/src/exec.rs) · **Plugin:** `CommandPlugin`

The RTS order engine, and the only system `npc_command` schedules. Each tick it prunes squads, then starts and
advances the orders of every unit that executes orders now: units without a `Brain`, and `Brain` units whose
tree runs the `ExecuteOrder` leaf. Built-in orders (`Move`, `AttackMove`, `Stop`, `Hold`, `Follow`, `Patrol`)
walk through the locomotion seam. Registered executors (glue and game) run first, so `Interact`, `Mount`,
`Unload`, `Custom` and the vehicle drive also run here. Formation slots are not computed here: `issue_to`
(`IssueOrder::apply`, `NpcSet::Inputs`) plans them; the engine only runs the group follower logic.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcCommandSet::Execute`; exclusive (`command_engine_system(world: &mut World, q: Local<CommandQueries>)`;
  `CommandQueries` caches the member, squad and unit `QueryState`s, built once via `FromWorld`)
- **Ordering:** `NpcCommandSet::Execute.after(NpcSet::Sense).before(NpcSet::Think)` (configured in
  `CommandPlugin::build`, [`lib.rs`](../../../crates/npc_command/src/lib.rs)). It reads last tick's
  `MoveStatus`, so arrival is seen one tick after the mover reports it.

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`, `OrderQueue`, `Has<Brain>`, `Has<ExecutingOrder>` (query `With<Commandable>`), `Commandable`, `SimPos`, `MoveStatus`, `MoveIntent`, `NavAgent`; a leader's `NavState` / `Path`; `Squad`, `MemberOf` |
| reads (res) | `SimClock`, `SimIndex`, `CommandConfig`, `OrderExecutors`, `SpatialGrid` (re-validates a follower's shifted waypoints) |
| writes | `OrderQueue` (`current`, `queued`, `last_ok`); `MoveIntent` + `MoveStatus` (`set_move_intent` / `clear_move_intent`); a follower's `NavState::Following` + derived `Path`; `Squad.members`; removes orphaned `MemberOf`; `ActiveLeaf.status` (`Success` / `Failure` when a `Brain` unit's queue ran dry) |
| writes (executors) | whatever a registered executor writes, e.g. [`InteractOrder`](../components/InteractOrder.md), interactions, vehicle intents |
| messages / events | journal `OrderStarted`, `OrderDone`, `OrderFailed { order, reason }` (`"blocked"`, `"target gone"`, `"no executor for .."`, an executor's reason) |

Per unit and tick, `step_unit` handles at most `MAX_TRANSITIONS` (8) order starts / ends, so a queue of instant
orders cannot loop. A walk fails `"blocked"` after `CommandConfig::block_fail_ticks` (default 100) blocked ticks.
A group follower with a `NavAgent` waits up to `leader_wait_ticks` (default 40) for the leader's path, then reuses
it shifted by its `GroupLink.offset`.

## Determinism

`prune_squads` collects `(SimId, Entity)` for orphaned members and sorts them, and walks squads sorted by
`SimId`. Units are collected as `(SimId, Entity, brain)` and sorted by `SimId`, then stepped one at a time
(an earlier unit's writes are visible to later ones, e.g. a leader's path). Executors live in `BTreeMap`s,
redirects in a name-sorted `Vec`. All distances are integer `dist2`; no rng.

## Performance

- **Mechanic tag:** `command` (via `tags.rs` prefix `npc_command`) · **Budget:** 15 µs fixed + 2 µs per agent
  (`NPC_BUDGET_TABLE`) · spans opened: none. Executors (glue included) run inside it, so their time counts as
  `command`.

## Related

- [`external_action_system`](external_action_system.md) — runs a `Direct` unit's `ExecuteOrder` step and
  handles the leaf the engine finished before the think.
- Components: [`OrderQueue`](../components/OrderQueue.md), [`Commandable`](../components/Commandable.md),
  [`Owner`](../components/Owner.md), [`ExecutingOrder`](../components/ExecutingOrder.md),
  [`Squad`](../components/Squad.md), [`MemberOf`](../components/MemberOf.md),
  [`InteractOrder`](../components/InteractOrder.md)
