---
type: System
---
# `flush_per_roll_log`

**Crate:** `npc_framework` (glue `perception_rules`) · **File:** [`crates/npc_framework/src/glue/perception_rules.rs`](../../../crates/npc_framework/src/glue/perception_rules.rs) · **Plugin:** `PerceptionRulesPlugin` (features `perception` + `rules`)

Records the Per rolls that the `per_roll` detection check made during the perception pass. A detection check
gets only `&World`, so `per_roll_check` cannot write the journal. Instead it pushes each
`Roll { skill: "Per", purpose: "perception.detect", .. }` entry into the `PerRollLog` resource (a
`Mutex<Vec<(SimId, JournalKind)>>`), but only while the journal is recording. This system takes the buffer and
records the entries in the same tick, right after that tick's `Perceived` / `Lost` entries.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcPerceptionSet` (which `PerceptionPlugin` configures `in_set(NpcSet::Sense)`). Exclusive (`&mut World`).
- **Ordering:** `.after(npc_perception::perception_system)`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `PerRollLog` (resource; returns at once without it) |
| writes | `PerRollLog` (emptied with `std::mem::take`) |
| messages / events | journal `Roll` entries, via `journal::record(world, Some(unit), ..)` |

## Determinism

It writes no sim state: the log is not hashed and is empty at every tick boundary. Entries are recorded in
visit order, which is fixed: observers by `SimId`, candidates in the pass's sorted order. The rolls themselves
use a pure dice key `(world seed, observer, tick, purpose, index)` (`detect_key`), not `RollCounters`, so
they never depend on visit order. With the journal off nothing is buffered.

## Performance

- **Mechanic tag:** `perception` (via `tags.rs` prefix `npc_framework::glue::perception_rules`) · **Budget:**
  shares the `perception` row, 50 µs + 5 µs per agent (`NPC_BUDGET_TABLE`) · opens no span.

## Related

- [`perception_system`](perception_system.md) — calls `per_roll_check`, which fills the log.
- Components: [`Stealthy`](../components/Stealthy.md), [`CharacterSheet`](../components/CharacterSheet.md)
