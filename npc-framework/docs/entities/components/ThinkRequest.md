---
type: Component
---
# `ThinkRequest`

**Crate:** `npc_ai` · **File:** [`crates/npc_ai/src/runtime.rs`](../../../crates/npc_ai/src/runtime.rs) · **Capability:** AI (behavior trees)

Marker: this agent must think this tick even though its leaf is still `Running`. Without it, `think` skips an
agent whose `ActiveLeaf` is `Running`. Insert it whenever something changed that a reactive guard may care
about. `SparseSet` storage.

## Fields

Marker component, no fields.

## Lifecycle

- **Added by:**
  - [`day_night_system`](../systems/day_night_system.md): every `Brain` agent on the first tick of each night and day.
  - [`rethink_timer_system`](../systems/rethink_timer_system.md): each agent every `RETHINK_INTERVAL` (40) ticks, staggered by `SimId`.
  - `npc_needs::decay_needs_system`: agents whose need level changed.
  - `apply_order` (with a new [`CurrentOrder`](CurrentOrder.md)), `npc_command::exec`, `npc_agent_io::control`,
    `npc_vehicles` (seats, drive, systems).
- **Removed by:** `npc_ai::think` from every agent it evaluates ([`think_system`](../systems/think_system.md)).
  Agents without `Brain` keep it.
- **Read by:** `think` (`ThinkReason::Request` in the trace when present).

Inserted in `NpcSet::Needs` (via `Commands`, applied before `NpcSet::Think`) it is consumed the same tick. One
inserted after `Think` waits for the next tick's think.

## Hashing and snapshots

- **State hash:** no.
- **World snapshot (ADR 0013):** saved as `think_request`, snapshot-only (`register_kernel_entries` in
  [`snapshot.rs`](../../../crates/npc_kernel/src/snapshot.rs)), so a request pending between ticks survives a load.

## Related

- [`ActiveLeaf`](ActiveLeaf.md) — what it overrides.
- [`Brain`](Brain.md) — only agents with a brain consume it.

## Example

```rust
// Food appeared: let nearby agents reconsider at the next think.
commands.entity(agent).insert(ThinkRequest);
```
