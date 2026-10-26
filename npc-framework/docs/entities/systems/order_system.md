---
type: System
---
# `order_system`

**Crate:** `village_content` · **File:** [`content/village_content/src/orders.rs`](../../../content/village_content/src/orders.rs) · **Plugin:** `VillagePlugin` (`add_systems` in [`lib.rs`](../../../content/village_content/src/lib.rs))

Keeps a villager's order walk in sync with its standing kernel order. The tree's `FollowOrder` guard starts
the walk once; after that, a new `GoTo` position re-targets the running walk here (no restart), and a cleared
order fails the leaf. Order walks are the ones marked with `OrderMove`.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Actions`, not exclusive
- **Ordering:** first in the chain `(order_system, move_to_system, eat_system, drink_system, work_system,
  sleep_system, idle_system).chain()` (they all write `ActiveLeaf`). Running before `move_to_system` lets a
  failed order walk drop its intent in the same tick.

## Reads / writes

| Access | Data |
|---|---|
| reads | `CurrentOrder` (only agents `With<OrderMove>`), `ActiveLeaf.status` (skips non-`Running`) |
| writes | `MoveTo.goal`, `MoveIntent.goal` (when present), `Blackboard[BB_DEST]` = `Pos(x, z)` on a changed `GoTo`; `ActiveLeaf.status = Failure` when the order is `None` |
| messages / events | none |

## Determinism

Each agent changes only its own components, so query order does not matter. Integer `SimPos` only. The
order itself arrives as a recorded `SimCommand::Order` in `NpcSet::Inputs`.

## Performance

- **Mechanic tag:** `inputs` (via `village_content::perf_tags` prefix `village_content::orders`, active when the
  host adds the village tags, e.g. `apps/village`, `npcctl bench`) · **Budget:** 20 µs + 1 µs/agent
  (`inputs` in `NPC_BUDGET_TABLE`) · no span.

## Related

- [`move_to_system`](move_to_system.md) — removes the intent of a walk this system failed.
- [`move_arrival_system`](move_arrival_system.md) — ends the walk on arrival.
- Components: [`OrderMove`](../components/OrderMove.md), [`MoveTo`](../components/MoveTo.md),
  [`CurrentOrder`](../components/CurrentOrder.md), [`MoveIntent`](../components/MoveIntent.md)
