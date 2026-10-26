---
type: System
---
# `apply_commands_system`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/commands.rs`](../../../crates/npc_kernel/src/commands.rs) · **Plugin:** `NpcKernelPlugin`

The input layer. Applies every queued `SimCommand` due at or before the current tick, in `(tick, seq)` order,
then closes the tick for new commands. `SimCommand` is the only way external input (player, CLI, BRP, replays)
changes the sim. Core kinds: `Despawn`, `SetNeed` (via the needs crate's `SetNeedHandler`), `Order`
(`apply_order` → [`CurrentOrder`](../components/CurrentOrder.md) + `ThinkRequest`); every other `"type"` is a
registered `ContentCommand` dispatched by name.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Inputs`, exclusive (`fn(&mut World)`), first in the set
- **Ordering:** `(apply_commands_system, apply_host_positions_system, copy_prev_pos_system).chain()`

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimClock`, `SimIndex`, `CommandRegistry`, `SetNeedHandler`, whatever a command's `validate` reads |
| writes | `CommandQueue` (drains due entries, sets `applied_through = Some(tick)`), `CommandLog` (appends `(tick, seq, cmd)`), `SkippedCommands`, and any state a command's `apply` changes |
| messages / events | journals `JournalKind::Command { cmd }` per applied command (unit from `unit_in`) |

Each due command is re-canonicalized and **validated again** against the world at that moment. If it fails
(e.g. its unit was despawned by an earlier command), it is skipped with a `tracing::warn!`, recorded in
`SkippedCommands` (tooling only) and **not** logged, so a replay of the log reproduces the run. After this
system runs, `submit` inside the tick refuses "next tick" without `at_tick` (`Unavailable`); sim code uses
`submit_deferred`, which queues for `t + 1`.

## Determinism

`CommandQueue.pending` is a `BTreeMap<(u64, u32), SimCommand>`: due entries split off with
`split_off(&(tick + 1, 0))` and applied in key order. Several commands on one tick apply in submission order
(seq). Content payloads are stored in canonical form (the type's own field order). Nothing is applied in
`Entity` order.

## Performance

- **Mechanic tag:** `inputs` (via `tags.rs` prefix `npc_kernel::commands`) · **Budget:** 20 µs + 1 µs per agent
  (`NPC_BUDGET_TABLE`) · no span of its own; `journal::record` drains the AI trace under a `journal` span.

## Related

- [`apply_host_positions_system`](apply_host_positions_system.md), [`copy_prev_pos_system`](copy_prev_pos_system.md) — next in the chain.
- [`spatial_update_system`](spatial_update_system.md) — sees spawns/despawns from commands this tick.
- Components: [`CurrentOrder`](../components/CurrentOrder.md), [`ThinkRequest`](../components/ThinkRequest.md), [`SimId`](../components/SimId.md)
