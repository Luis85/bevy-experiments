---
type: System
---
# `apply_host_positions_system`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/driver.rs`](../../../crates/npc_kernel/src/driver.rs) · **Plugin:** `NpcKernelPlugin`

In `DeterminismMode::Relaxed` only: copies each entity's [`HostPosition`](../components/HostPosition.md) into its
`SimPos` at the start of the tick, so a host that owns movement (physics) feeds the sim. In `Strict` mode (the
default) or without a `DeterminismMode` resource it returns at once.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Inputs`, not exclusive
- **Ordering:** `(apply_commands_system, apply_host_positions_system, copy_prev_pos_system).chain()`;
  [`spatial_update_system`](spatial_update_system.md) is `.after(apply_host_positions_system)`

## Reads / writes

| Access | Data |
|---|---|
| reads | `Option<Res<DeterminismMode>>`, `HostPosition` |
| writes | `SimPos` (only when it differs from `HostPosition.0`, to avoid spurious change detection) |
| messages / events | none |

## Determinism

Per-entity copy, so iteration order does not matter. The result is only as deterministic as the host values:
in Relaxed mode replays and state hashes are not guaranteed, and `NpcSimTick` ambiguities only warn.

## Performance

- **Mechanic tag:** `inputs` (via `tags.rs` prefix `npc_kernel::driver::apply_host_positions_system`) ·
  **Budget:** 20 µs + 1 µs per agent (`NPC_BUDGET_TABLE`, shared `inputs` row) · no spans.

## Related

- [`apply_commands_system`](apply_commands_system.md) — runs before.
- [`copy_prev_pos_system`](copy_prev_pos_system.md) — runs after, so `PrevSimPos` takes the host position.
- [`drive_sim`](drive_sim.md) — the driver that runs the tick.
- Components: [`HostPosition`](../components/HostPosition.md), [`SimPos`](../components/SimPos.md)
