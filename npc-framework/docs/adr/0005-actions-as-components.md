# ADR 0005: Actions are components, ticked by ordinary systems

- Status: Accepted
- Date: 2026-10-23
- Source: villager spec §6.3, `docs/research/ai-prior-art.md` (bevy_behave, RimWorld JobDriver)

## Context

Behaviour-tree leaves must run for many ticks (walk, work, sleep). Running them inside the tree, as
callbacks holding boxed state, hides them from the ECS: no parallelism, no per-action profiling, and no way
to inspect or hash their state like other components.

## Decision

- Starting a leaf calls the action's `start` fn. It may return `Success` or `Failure` at once, or insert a
  `SparseSet` action component (`MoveTo`, `Work`, `Eat`, `Sleep`, `Idle`, ...) and return `Running`.
- One ordinary system per action kind, in `SimSet::Actions`, advances it and writes the `ActiveLeaf`
  status. After `start`, only the action's own system changes the status.
- Abort is synchronous: `stop` removes the component and releases reservations.
- The tree only waits on the status: the tree chooses, the action runs (like RimWorld's ThinkTree and
  JobDriver).

## Consequences

- Actions are visible to queries, inspect, the state hash and the per-mechanic profiler.
- `SparseSet` storage keeps insert/remove churn cheap.
- All action systems write `ActiveLeaf`, so they are chained in a fixed order to satisfy ambiguity detection.
- A new action is a component, start/stop fns and one system, registered by name (ADR 0007).
