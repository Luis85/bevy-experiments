---
type: System
---
# `key_replan_system`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/door.rs`](../../../crates/npc_interaction/src/door.rs) · **Plugin:** `InteractionPlugin`

Retries the failed path of a nav agent that just got the key of a locked door. A locked door blocks only
agents **without** its key: that is a per-agent pass-cost decision, not cell data, so picking up the key
emits no `GridChanged` and nav would otherwise wait for an unrelated grid change. For every `NavAgent` +
[`Interactor`](../components/Interactor.md) whose `NavState` is `Failed` and whose `Inventory` or `Equipment`
changed since this system last ran, it calls `npc_nav::retry_failed_path` when the agent now carries an item
matching the key of any locked door. Returns early when no door is locked.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcInteractionSet::Complete`; exclusive (`key_replan_system(world: &mut World, doors: &mut DoorQuery,
  changed: &mut KeyChangeQuery)`, cached `QueryState`s of every `Door` and of the interactors whose
  `Inventory`/`Equipment` changed)
- **Ordering:** `(complete_interactions_system, key_replan_system).chain()`, so a key picked up this tick
  is seen this tick.

## Reads / writes

| Access | Data |
|---|---|
| reads | `Door` (`locked_by`), `SimId`, `npc_nav::NavState`, `With<NavAgent>`, `With<Interactor>`, `Changed<Inventory>` / `Changed<Equipment>`, carried `Item`s and `InteractionRegistry` kinds (`carried_matching`) |
| writes | via `retry_failed_path`: a `NavQueue` path request and the agent's `NavState` (back to `Pending` at the current tick) |
| messages / events | none |

## Determinism

Keys are collected, sorted and deduped; agents are collected as `(SimId, Entity)` and sorted by `SimId`.
Change detection is the only non-obvious input: a world snapshot saves which inventories/equipment changed
since the last run (resource `key_replan_pending`, computed from the system's last-run tick) and
`rebuild_key_replan_pending` marks them changed again after load, so a loaded run retries the same agents.

## Performance

- **Mechanic tag:** `interaction` (prefix `npc_interaction`) · **Budget:** 25 µs + 2 µs/agent (`interaction`)
  · no span of its own.

## Related

- [`complete_interactions_system`](complete_interactions_system.md), [`auto_door_system`](auto_door_system.md),
  [`door_sync_system`](door_sync_system.md).
- Components: [`Door`](../components/Door.md), [`Inventory`](../components/Inventory.md), [`Equipment`](../components/Equipment.md)
