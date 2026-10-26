---
type: System
---
# `perception_system`

**Crate:** `npc_perception` · **File:** [`crates/npc_perception/src/system.rs`](../../../crates/npc_perception/src/system.rs) · **Plugin:** `PerceptionPlugin`

The perception pass. It drains the tick's noises, lets every listener hear them, scans vision and proximity
on each agent's stagger tick, runs every candidate through the active detection check, and updates each
agent's [`PerceptionMemory`](../components/PerceptionMemory.md) (and blackboard facts). There is no separate
noise system: noise handling is step 1–2 below.

1. Drain `Messages<Noise>` and sort by `Noise::sort_key` (`source, x, z, radius, kind`).
2. **Hearing, every tick:** for each noise, `SpatialGrid::in_radius(noise.pos, noise.radius_mm)` gives sorted
   ids; an agent with hearing hears it when within its own radius too (a source never hears itself).
3. **Scan, only when `is_due(id, tick, stagger)`** (`(id + tick) % stagger == 0`, default stagger 4): every
   `Perceivable` within the scan radius and in a vision cone (range, integer FOV, then grid line of sight) or
   within proximity. Perceivables are only gathered (and sorted) on a tick with a noise or at least one due
   agent; otherwise the pass skips them. Candidates go nearest first (ties: lower `SimId`); the scan stops at `memory_capacity`.
4. Every candidate calls the active `DetectionCheckFn` (`"automatic"` by default, or `"per_roll"`).
5. Per agent in `SimId` order: apply hearing, then the scan (sensed flags, forgetting after
   `forget_after_ticks`), then capacity eviction (every tick), then write [`PerceptionFacts`](../components/PerceptionFacts.md)
   slots on due ticks. Journal `Perceived` / `Lost` entries are buffered and recorded at the end.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcPerceptionSet`, configured `in_set(NpcSet::Sense)`. Exclusive (`perception_system(world: &mut World,
  agents_q: &mut AgentQuery, things_q: &mut PerceivableQuery)`: cached `QueryState`s of the sensing agents and
  the `Perceivable`s).
- **Ordering:** none inside the set from this plugin. The perception × rules glue orders
  [`flush_per_roll_log`](flush_per_roll_log.md) `.after(perception_system)`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimId`, `SimPos`, [`Sensors`](../components/Sensors.md), [`Facing`](../components/Facing.md), [`Perceivable`](../components/Perceivable.md) (components); `SimClock`, `PerceptionConfig`, `DetectionChecks`, `SpatialGrid` (occluders, static cells) (resources); whatever the active check reads (`&World`) |
| writes | [`PerceptionMemory`](../components/PerceptionMemory.md); `Blackboard` slots named by `PerceptionFacts` (only on change) |
| messages / events | drains `Messages<Noise>` (the buffer is not drained by `First`); journal `Perceived { target, via }`, `Lost { target, forgotten }` |

## Determinism

Agents and perceivables are collected then sorted by `SimId`; the grid's radius queries return sorted
`SimId`s; noises are sorted before use, so write order never matters. Memory is a `BTreeMap`. FOV is integer
(`i128` dot products against the `COS2_PPM` table), distances are `dist2`. Noises written after `Sense` are
heard next tick; they are hashed and saved meanwhile as `pending_noises`. Detection checks get `&World` and
must be pure (the `per_roll` dice key is `(seed, observer, tick, purpose, index)`).

## Performance

- **Mechanic tag:** `perception` (via `tags.rs` prefix `npc_perception`) · **Budget:** 50 µs + 5 µs per agent
  (`NPC_BUDGET_TABLE`) · opens `info_span!("mechanic", name = "perception")` (nested in the system's own `perception`-tagged span, so `PerfLayer` does not time it a second time).
- Cheap pruning: candidates outside every cone and outside proximity are dropped before the sort; line of
  sight is traced only for cone candidates actually visited.

## Related

- [`facing_from_movement_system`](facing_from_movement_system.md) — updates `Facing` at the end of the tick.
- [`flush_per_roll_log`](flush_per_roll_log.md) — journals `per_roll` detection rolls right after this pass.
- Components: [`Sensors`](../components/Sensors.md), [`Facing`](../components/Facing.md),
  [`Perceivable`](../components/Perceivable.md), [`PerceptionMemory`](../components/PerceptionMemory.md),
  [`PerceptionFacts`](../components/PerceptionFacts.md), [`Stealthy`](../components/Stealthy.md)
