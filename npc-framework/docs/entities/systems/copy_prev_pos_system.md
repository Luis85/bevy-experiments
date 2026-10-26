---
type: System
---
# `copy_prev_pos_system`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/math.rs`](../../../crates/npc_kernel/src/math.rs) · **Plugin:** `NpcKernelPlugin`

Sets `PrevSimPos = SimPos` for every positioned entity at the start of the tick, before anything moves. The view
interpolates `PrevSimPos → SimPos` between ticks; perception uses the pair for facing.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Inputs`, not exclusive
- **Ordering:** last of `(apply_commands_system, apply_host_positions_system, copy_prev_pos_system).chain()`;
  [`spatial_update_system`](spatial_update_system.md) is `.after(copy_prev_pos_system)`

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimPos` |
| writes | `PrevSimPos` (only when it differs, to avoid change detection on static entities); `Commands` inserts `PrevSimPos(pos)` on entities with `SimPos` and no `PrevSimPos` (applied at the next sync point, before `NpcSet::Needs`) |
| messages / events | none |

## Determinism

Per-entity copy, no cross-entity effects, so query order does not matter. `PrevSimPos` is neither hashed nor
saved; snapshot load resets it to `SimPos`.

## Performance

- **Mechanic tag:** `actions.move` (via `tags.rs` prefix `npc_kernel::math`) · **Budget:** 10 µs + 2 µs per agent
  (`NPC_BUDGET_TABLE`) · no spans.

## Related

- [`apply_host_positions_system`](apply_host_positions_system.md) — runs before.
- [`spatial_update_system`](spatial_update_system.md) — runs after.
- Components: [`PrevSimPos`](../components/PrevSimPos.md), [`SimPos`](../components/SimPos.md)
