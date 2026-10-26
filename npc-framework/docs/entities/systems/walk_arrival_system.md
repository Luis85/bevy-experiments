---
type: System
---
# `walk_arrival_system`

**Crate:** `testbed_content` · **File:** [`content/testbed_content/src/actions.rs`](../../../content/testbed_content/src/actions.rs) · **Plugin:** `TestbedPlugin`

Ends a testbed walk (`TbWalking`) from the mover's report: `Success` on `MoveStatus::Arrived`, `Failure` after
`Blocked { ticks }` with `ticks >= WALK_GIVE_UP_TICKS` (60), and `Failure` when the `MoveIntent` is gone (the
actor mounted a vehicle, which takes the intent away).

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Locomotion`, not exclusive
- **Ordering:** `.after(NpcMoverSet)` (sees this tick's `MoveStatus` from the grid mover; actors are
  `NavAgent`s).

## Reads / writes

| Access | Data |
|---|---|
| reads | `Option<MoveStatus>`, `Has<MoveIntent>` for agents `With<TbWalking>`; `ActiveLeaf.status` (skips non-`Running`) |
| writes | `ActiveLeaf.status` |
| messages / events | none |

## Determinism

Per-agent, order-independent, integer tick count.

## Performance

- **Mechanic tag:** `actions.move` (via `testbed_content::perf_tags` prefix
  `testbed_content::actions::walk_arrival_system`) · **Budget:** 10 µs + 2 µs/agent (`actions.move`,
  the framework's `NPC_BUDGET_TABLE`) · no span. The tag applies where the host adds `testbed_content::perf_tags`: the
  actor testbed app (`debug-tools`, default) and `npcctl bench`.

## Related

- [`walk_cancel_system`](walk_cancel_system.md) — drops the intent of a cancelled walk.
- [`grid_path_mover_system`](grid_path_mover_system.md) — writes `MoveStatus` for `NavAgent`s.
- [`move_arrival_system`](move_arrival_system.md) — the village's version (gives up after 200 ticks).
- Components: [`TbWalking`](../components/TbWalking.md), [`MoveStatus`](../components/MoveStatus.md)
