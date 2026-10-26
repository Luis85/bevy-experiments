---
type: System
---
# `think_system`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/schedule.rs`](../../../crates/npc_kernel/src/schedule.rs) · **Plugin:** `NpcKernelPlugin`

Runs the behavior-tree runtime `npc_ai::think` once over every agent with `Brain` and a `SimId`, in ascending
`SimId` order, then drains the `AiTrace` into the `Journal`. With no `Brain` agents it returns without calling
`think`. Agents without `Brain` are untouched (their `ThinkRequest`s stay).

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Think`, exclusive (`think_system(world: &mut World, brains: &mut BrainQuery)`; `BrainQuery`
  is a cached `QueryState<(&SimId, Entity), With<Brain>>`, not rebuilt every tick)
- **Ordering:** alone in its set; the chained sets put it after `Needs`/`Sense` and before `Actions`

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`, `Brain`, `ThinkRequest`, `ActiveLeaf`, `AiRegistry` (trees, actions, conditions, scorers), `AiTrace` flags; plus whatever registered conditions/scorers read |
| writes | `BrainState`, `BrainStats`, `ActiveLeaf` (insert/remove), removes `ThinkRequest`; actions' `start`/`stop` write their own components, `Blackboard`, reservations, ... |
| messages / events | `AiTrace` events (when enabled) → `drain_ai_trace` → journal `ThinkStart`, `LeafStart`, `LeafStop`, `Abort`, `RootDone`, `CondEval` (verbose) |

## The `npc_ai::think` runtime ([`crates/npc_ai/src/runtime.rs`](../../../crates/npc_ai/src/runtime.rs))

- Freezes `AiRegistry` if still open; copies its function pointers once per call.
- An agent is evaluated when it has a `ThinkRequest`, has no `ActiveLeaf`, or its leaf is not `Running`;
  otherwise skipped. Each agent at most once per call. `ThinkRequest` is removed from every evaluated agent.
- Takes `BrainState` out, resizes `mem` to the tree's node count, evaluates from the root (node 0), writes the
  state back: `running_path` is kept only when the root returned `Running`.
- **Leaf:** the active leaf still `Running` → `Running`; active and finished → `stop(Finished)`, remove
  `ActiveLeaf`, return its status. Else call `start`; on `Running` insert `ActiveLeaf` (`leaf_starts += 1`).
  A stale leaf of another node is first stopped with `Aborted`.
- **Sequence** resumes at the child index stored in `mem`. **Guard:** cond false → `Failure`; on the running
  path it is not re-checked. **Selector:** before the running branch only `Guard { abort: LowerPriority }`
  children are checked; a passing one aborts the branch (`stop(Aborted)`, subtree `mem` zeroed,
  `aborts += 1`). A failed running branch makes the selector re-evaluate from child 0, skipping that child.
  **UtilitySelector:** continue the running child, else order children by score desc (ties → lower index).
- Trace reasons: `Request`, `LeafDone`, `NoLeaf`.

## Determinism

Collects `(SimId, Entity)`, sorts by `SimId` (`sort_unstable_by_key`; ids are unique so the order is total),
then evaluates in that order. An `EntityHashSet` only deduplicates (never iterated). Scores are `i32`. The
journal/trace never changes decisions. `NpcSimTick` runs on a `SingleThreadedExecutor` (set in
`NpcKernelPlugin`, `schedule.rs`); the system order is the same as with the multi-threaded one (ambiguities
are errors), so the state hash is too.

## Performance

- **Mechanic tag:** `think` (via `tags.rs` prefix `npc_kernel::schedule::think_system`) · **Budget:**
  40 µs + 20 µs per agent (`NPC_BUDGET_TABLE`).
- **Spans** (`info_span!("mechanic", name = ..)`, nested in `think`, no budget rows): `think.agent` (one agent's
  evaluation), `think.cond` (a condition), `think.start` (an action's start), `think.stop` (an action's stop).
  Scorers have no span. `drain_ai_trace` opens a `journal` span when it records.

## Related

- [`day_night_system`](day_night_system.md), [`rethink_timer_system`](rethink_timer_system.md) — request re-thinks.
- [`flush_journal_system`](flush_journal_system.md) — flushes what the trace produced.
- Components: [`Brain`](../components/Brain.md), [`BrainState`](../components/BrainState.md), [`ActiveLeaf`](../components/ActiveLeaf.md),
  [`ThinkRequest`](../components/ThinkRequest.md), [`BrainStats`](../components/BrainStats.md), [`Blackboard`](../components/Blackboard.md)
