---
type: Component
---
# `ActiveLeaf`

**Crate:** `npc_ai` · **File:** [`crates/npc_ai/src/runtime.rs`](../../../crates/npc_ai/src/runtime.rs) · **Capability:** AI (behavior trees)

The leaf currently running on an agent, and the seam between the tree and an action's own system. `think`
inserts it when a leaf's `start` returns `Running`; from then on **only that action's system** writes
`status` (to `Success` or `Failure`); the next think consumes the result, calls `stop` and removes it.
`SparseSet` storage (added and removed often).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `node` | `NodeIdx` (`u16`) | the leaf's node index in the agent's tree |
| `action` | `ActionId` (`u16`) | the action's frozen id (actions sorted by name) |
| `status` | `Status` | `Running`, `Success` or `Failure` |

## Lifecycle

- **Added by:** `npc_ai::think` (in [`think_system`](../systems/think_system.md)) when `start` returns `Running`
  (`BrainStats.leaf_starts += 1`).
- **Written by:** the action's own system in `NpcSet::Actions` (or after the movers in `NpcSet::Locomotion`),
  e.g. village `sleep_system`, `move_arrival_system`, `idle_system` in `content/village_content/src/actions.rs`.
- **Removed by:** `think`: when it consumes a finished status (`stop(.., Finished)`), when a
  `Guard { abort: LowerPriority }` aborts the branch (`stop(.., Aborted)`, even if already finished), or as a
  safety net when another leaf is about to start.
- **Read by:** `think` (an agent with a `Running` leaf and no `ThinkRequest` is skipped), actions, the
  inspector, `npc_command::exec`, `npc_agent_io`.

## Hashing and snapshots

- **State hash:** yes, as `active_leaf` (`hashed.add_component::<ActiveLeaf>()` in
  [`schedule.rs`](../../../crates/npc_kernel/src/schedule.rs); encoding `u16 node`, `u16 action`, `u8 status`).
- **World snapshot (ADR 0013):** saved as `active_leaf` (`register_kernel_entries`,
  [`snapshot.rs`](../../../crates/npc_kernel/src/snapshot.rs)).

## Related

- [`Brain`](Brain.md), [`BrainState`](BrainState.md) — the tree and path the leaf belongs to.
- [`ThinkRequest`](ThinkRequest.md) — forces a think even while `Running`.

## Example

```rust
// An action system: finish the leaf when the timer runs out.
pub fn wait_system(mut q: Query<(&mut Wait, &mut ActiveLeaf)>) {
    for (mut wait, mut leaf) in &mut q {
        if leaf.status != Status::Running { continue; }
        wait.ticks_left = wait.ticks_left.saturating_sub(1);
        if wait.ticks_left == 0 { leaf.status = Status::Success; }
    }
}
```
