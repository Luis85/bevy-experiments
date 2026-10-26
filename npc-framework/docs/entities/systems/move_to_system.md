---
type: System
---
# `move_to_system`

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Plugin:** `VillagePlugin`

Stops a walk whose leaf is no longer running before the movers run. Today that is an order walk that
[`order_system`](order_system.md) just failed. It removes the locomotion seam components so the villager
does not take one more step; the leaf's stop (`stop_move` / `stop_follow_order`) removes `MoveTo` later.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Actions`, not exclusive
- **Ordering:** second in the village action chain, after `order_system`, before `eat_system`. `NpcSet::Actions`
  runs before `NpcSet::Locomotion`, so the movers never see the removed intent.

## Reads / writes

| Access | Data |
|---|---|
| reads | `Entity`, `ActiveLeaf.status` for agents `(With<MoveTo>, With<MoveIntent>)` |
| writes | `Commands::remove::<(MoveIntent, MoveStatus)>()` on agents whose leaf is not `Running` |
| messages / events | none |

## Determinism

Per-agent and order-independent; the removals are deferred commands applied at the set's sync point.

## Performance

- **Mechanic tag:** `actions.move` (via `village_content::perf_tags` prefix
  `village_content::actions::move_to_system`) · **Budget:** 10 µs + 2 µs/agent (`actions.move` in
  `NPC_BUDGET_TABLE`) · no span.

## Related

- [`move_arrival_system`](move_arrival_system.md) — the success/failure side of a walk.
- [`straight_line_mover_system`](straight_line_mover_system.md), [`grid_path_mover_system`](grid_path_mover_system.md) — the movers.
- Components: [`MoveTo`](../components/MoveTo.md), [`MoveIntent`](../components/MoveIntent.md),
  [`MoveStatus`](../components/MoveStatus.md)
