---
type: System
---
# `facing_from_movement_system`

**Crate:** `npc_perception` · **File:** [`crates/npc_perception/src/system.rs`](../../../crates/npc_perception/src/system.rs) · **Plugin:** `PerceptionPlugin`

At the end of the tick, an agent that moved faces its movement direction. "Moved" means `SimPos` differs from
`PrevSimPos`, which the kernel copies at the start of the tick (`copy_prev_pos_system`, `NpcSet::Inputs`). The
new value is `Facing::towards(prev, pos)`. Entities with [`FixedFacing`](../components/FixedFacing.md) are
skipped. The next tick's vision scan uses the result.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Cleanup`, not exclusive.
- **Ordering:** none (`facing_from_movement_system.in_set(NpcSet::Cleanup)` in `PerceptionPlugin::build`).

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimPos`, `PrevSimPos` (components); filter `Without<FixedFacing>` |
| writes | [`Facing`](../components/Facing.md), only when the value changes (keeps change detection quiet) |
| messages / events | none |

## Determinism

Per-entity and order-independent: each agent's result depends only on its own `SimPos` and `PrevSimPos`, so
query order does not matter. Integer math: the delta is reduced by halving until each axis is at most 2¹⁵.

## Performance

- **Mechanic tag:** `perception` (via `tags.rs` prefix `npc_perception`) · **Budget:** shares the `perception`
  row, 50 µs + 5 µs per agent (`NPC_BUDGET_TABLE`) · opens no span.

## Related

- [`perception_system`](perception_system.md) — reads `Facing` for vision cones.
- Components: [`Facing`](../components/Facing.md), [`FixedFacing`](../components/FixedFacing.md)
