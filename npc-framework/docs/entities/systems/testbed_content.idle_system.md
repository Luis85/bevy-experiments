---
type: System
---
# `idle_system` (testbed)

**Crate:** `testbed_content` · **File:** [`content/testbed_content/src/actions.rs`](../../../content/testbed_content/src/actions.rs) · **Plugin:** `TestbedPlugin`

Counts `TbIdling.remaining` down and sets the leaf to `Success` at 0 (action `TbIdle`). (The village has its
own `idle_system` for `Idle`: [`village_content.idle_system`](village_content.idle_system.md).)

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Actions`, not exclusive
- **Ordering:** after `walk_cancel_system` (`.chain()`).

## Reads / writes

| Access | Data |
|---|---|
| reads | `ActiveLeaf.status` (skips non-`Running`) |
| writes | `TbIdling.remaining`, `ActiveLeaf.status` |
| messages / events | none |

## Determinism

Per-agent, integer only; the length is the leaf param.

## Performance

- **Mechanic tag:** `actions.idle` (via `testbed_content::perf_tags` prefix
  `testbed_content::actions::idle_system`) · **Budget:** 10 µs + 2 µs/agent (`actions.idle`,
  `TESTBED_BUDGETS`, registered by `TestbedPlugin`) · no span. The tag applies where the host adds
  `testbed_content::perf_tags`: the actor testbed app (`debug-tools`, default) and `npcctl bench`.

## Related

- [`walk_cancel_system`](walk_cancel_system.md) — runs just before it.
- Components: [`TbIdling`](../components/TbIdling.md)
