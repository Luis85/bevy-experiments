---
type: System
---
# `flush_journal_system`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/journal.rs`](../../../crates/npc_kernel/src/journal.rs) · **Plugin:** `NpcKernelPlugin`

Flushes the journal's streaming sink (`JournalSink`, JSON Lines) once per tick, so a file written with
`npcctl run --journal j.jsonl` is complete at every tick boundary. Does nothing when there is no `Journal` or no sink.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Cleanup`, not exclusive
- **Ordering:** `(flush_journal_system, advance_clock_system).chain()`; `npc_progression::progression_system`
  runs `.before(flush_journal_system)`

## Reads / writes

| Access | Data |
|---|---|
| reads | `Option<ResMut<Journal>>` (`sink`) |
| writes | `Journal.sink` (flushes the `BufWriter`; after the first I/O error the sink logs one warning and stops) |
| messages / events | none |

## Determinism

Tooling only: the journal is not hashed and never read by sim decisions, so the state hash is the same with it
on or off. Entries carry `(tick, seq)`, `SimId`s and node names, never `Entity` ids or wall-clock times.

## Performance

- **Mechanic tag:** `journal` (via `tags.rs` prefix `npc_kernel::journal`) · **Budget:** 20 µs + 2 µs per agent
  (`NPC_BUDGET_TABLE`) · no span here (`drain_ai_trace` in the same module opens the `journal` span).

## Related

- [`think_system`](think_system.md) — fills the journal from the AI trace.
- [`advance_clock_system`](advance_clock_system.md) — chained after.
