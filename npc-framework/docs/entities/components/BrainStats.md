---
type: Component
---
# `BrainStats`

**Crate:** `npc_ai` · **File:** [`crates/npc_ai/src/runtime.rs`](../../../crates/npc_ai/src/runtime.rs) · **Capability:** AI (behavior trees)

Debug counters of an agent's think pass. Useful to spot an agent that keeps aborting and restarting work
(a `LowerPriority` guard whose child cannot start). Never read by sim decisions.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `leaf_starts` | `u32` | leaves whose `start` returned `Running` (count since spawn) |
| `aborts` | `u32` | branches aborted by a `LowerPriority` guard, plus stale leaves stopped before a new start |

## Lifecycle

- **Added by:** required by [`Brain`](Brain.md) (zeros).
- **Written by:** `npc_ai::think` only ([`think_system`](../systems/think_system.md)); a copy is updated during
  the evaluation and written back at the end.
- **Read by:** the inspector (`npc_kernel::inspect`, `content/village_content/src/inspect.rs`), `npcctl`
  (`apps/npcctl/src/sim.rs`).

## Hashing and snapshots

- **State hash:** no.
- **World snapshot (ADR 0013):** saved as `brain_stats`, snapshot-only (`register_kernel_entries` in
  [`snapshot.rs`](../../../crates/npc_kernel/src/snapshot.rs)), so inspect output continues across a load.

## Related

- [`Brain`](Brain.md), [`ActiveLeaf`](ActiveLeaf.md).

## Example

```rust
let stats = world.get::<BrainStats>(agent).copied().unwrap_or_default();
println!("starts {} aborts {}", stats.leaf_starts, stats.aborts);
```
