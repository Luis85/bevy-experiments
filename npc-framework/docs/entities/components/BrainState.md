---
type: Component
---
# `BrainState`

**Crate:** `npc_ai` · **File:** [`crates/npc_ai/src/runtime.rs`](../../../crates/npc_ai/src/runtime.rs) · **Capability:** AI (behavior trees)

An agent's own per-node memory of its tree: where it is running and where each sequence resumes. The tree
(`TreeDef`) is shared; all per-agent state lives here, so it can be hashed and saved.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `running_path` | `Vec<NodeIdx>` (`u16`) | node indices from the root (0) to the running leaf; empty when nothing runs |
| `mem` | `Vec<u32>` | one word per tree node, lazily resized to the tree's node count; sequences store their current child index |

## Lifecycle

- **Added by:** required by [`Brain`](Brain.md) (default: empty).
- **Written by:** `npc_ai::think` only (via [`think_system`](../systems/think_system.md)). It takes the state
  out, evaluates, and writes it back: `running_path` is the new path when the root returns `Running`, else
  empty; an abort zeroes the aborted subtree's `mem` range.
- **Read by:** `think` (resume a sequence, re-check only `LowerPriority` guards before the running branch);
  the inspector (`npc_kernel::inspect`), the village and testbed overlays (`*_view/src/overlay.rs`).

## Hashing and snapshots

- **State hash:** yes, as `brain_state` (added with `hashed.add_component::<BrainState>()` in
  `NpcKernelPlugin::build`, [`schedule.rs`](../../../crates/npc_kernel/src/schedule.rs); encoding:
  `u32` length + `u16` per path node, `u32` length + `u32` per `mem` word).
- **World snapshot (ADR 0013):** saved as `brain_state` (registered separately in `register_kernel_entries`,
  [`snapshot.rs`](../../../crates/npc_kernel/src/snapshot.rs)).

## Related

- [`Brain`](Brain.md) — which tree the indices refer to.
- [`ActiveLeaf`](ActiveLeaf.md) — the leaf at the end of `running_path`.

## Example

```rust
// Debug read: is the agent running something, and how deep?
fn depth(world: &World, agent: Entity) -> usize {
    world.get::<BrainState>(agent).map_or(0, |s| s.running_path.len())
}
```
