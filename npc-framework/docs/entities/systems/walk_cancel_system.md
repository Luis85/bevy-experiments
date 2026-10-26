---
type: System
---
# `walk_cancel_system`

**Crate:** `testbed_content` · **File:** [`content/testbed_content/src/actions.rs`](../../../content/testbed_content/src/actions.rs) · **Plugin:** `TestbedPlugin` ([`lib.rs`](../../../content/testbed_content/src/lib.rs))

Stops a testbed walk whose leaf is no longer running, before the movers run: it removes the locomotion seam
components so the actor does not take another step. The leaf's `walk_stop` removes `TbWalking` later.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Actions`, not exclusive
- **Ordering:** `(walk_cancel_system, idle_system).chain().in_set(NpcSet::Actions)`. Actions run before
  `NpcSet::Locomotion`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `Entity`, `ActiveLeaf.status` for agents `(With<TbWalking>, With<MoveIntent>)` |
| writes | `Commands::remove::<(MoveIntent, MoveStatus)>()` when the leaf is not `Running` |
| messages / events | none |

## Determinism

Per-agent, order-independent, deferred commands only.

## Performance

- **Mechanic tag:** `actions.move` (via `testbed_content::perf_tags` prefix
  `testbed_content::actions::walk_cancel_system`) · **Budget:** 10 µs + 2 µs/agent (`actions.move`,
  the framework's `NPC_BUDGET_TABLE`) · no span. The tag applies where the host adds `testbed_content::perf_tags`: the
  actor testbed app (`debug-tools`, default) and `npcctl bench`.

## Related

- [`walk_arrival_system`](walk_arrival_system.md) — the success/failure side.
- [`move_to_system`](move_to_system.md) — the village's version.
- Components: [`TbWalking`](../components/TbWalking.md), [`MoveIntent`](../components/MoveIntent.md),
  [`MoveStatus`](../components/MoveStatus.md)
