---
type: System
---
# `move_arrival_system`

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Plugin:** `VillagePlugin`

Ends a villager's walk from the mover's report. `MoveStatus::Arrived` sets the leaf to `Success` in the same
tick the villager arrives. `MoveStatus::Blocked { ticks }` with `ticks >= MOVE_GIVE_UP_TICKS` (200) sets
`Failure`; that only happens with blocking grid cells (`village_nav`), never in the plain village.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Locomotion`, not exclusive
- **Ordering:** `.after(NpcMoverSet)`, so it sees this tick's `MoveStatus` from `StraightLineMover` or
  `GridPathMover`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `MoveStatus` for agents `(With<MoveTo>, With<MoveIntent>)`; `ActiveLeaf.status` (skips non-`Running`) |
| writes | `ActiveLeaf.status` (`Success` / `Failure`) |
| messages / events | none |

## Determinism

Per-agent and order-independent; integer tick count only.

## Performance

- **Mechanic tag:** `actions.move` (via `village_content::perf_tags` prefix
  `village_content::actions::move_arrival_system`) · **Budget:** 10 µs + 2 µs/agent (`actions.move`) · no span.

## Related

- [`move_to_system`](move_to_system.md) — drops the intent of a cancelled walk.
- [`straight_line_mover_system`](straight_line_mover_system.md), [`grid_path_mover_system`](grid_path_mover_system.md) — write `MoveStatus`.
- [`walk_arrival_system`](walk_arrival_system.md) — the testbed's version.
- Components: [`MoveTo`](../components/MoveTo.md), [`MoveStatus`](../components/MoveStatus.md)
