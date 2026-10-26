---
type: System
---
# `progression_system`

**Crate:** `npc_progression` · **File:** [`crates/npc_progression/src/engine.rs`](../../../crates/npc_progression/src/engine.rs) · **Plugin:** `ProgressionPlugin`

The roll-feed pass. `ProgressionPlugin` switches on `npc_rules`' deterministic roll feed
(`RollCounters::feed.enabled = true`). Every roll this tick (`roll_against` and everything built on it, and
`roll_with`) appended a `RollRecord`. This system drains the feed and, per unit, calls `apply_rolls`: for each
roll of a **registered skill** by a unit with `Experience`, `uses += 1`, `practice_per_roll` practice ticks
(capped), and the XP of every roll-driven source (default `roll`: successes only). Then `add_xp` per source,
which may level the unit, bank points and, for `SpendPolicy::Ai`, spend them. Stat rolls (`Per`, `HT`) earn
nothing. Awards from crafts and the `study` / `teach` grants do not go through this system; they call the
world API (`award_xp`, `grant_*`) directly.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Cleanup`, not exclusive.
- **Ordering:** `.before(flush_journal_system)` (the kernel's journal flush, also in `Cleanup`).

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimClock`, `ProgressionConfig`, `XpSources`, `RuleSet`, `RulesRegistry`, `SimIndex` (resources); [`SpendPolicy`](../components/SpendPolicy.md) |
| writes | `RollCounters` (drains `feed.records`); [`Experience`](../components/Experience.md), [`Learning`](../components/Learning.md), [`CharacterSheet`](../components/CharacterSheet.md) |
| messages / events | journal (`Option<ResMut<Journal>>`): `XpGained`, `LevelUp`, `SkillImproved { via: "practice" \| "spend" }` |

Returns at once when the feed is empty. Units without `Experience` (or without a sheet) are skipped; their
records are still drained.

## Determinism

Records are stable-sorted by unit `SimId`, so each unit's rolls stay in roll order. XP per source is summed
in a `BTreeMap` (source name order). `ai_spend` compares `uses / (points + 1)` by integer cross-multiplication
with ties by name. All integer, saturating. The feed is drained in the same tick, so it is empty at every tick
boundary and needs no hashing. It never reads the journal: it works with the journal off.

## Performance

- **Mechanic tag:** `progression` (via `tags.rs` prefix `npc_progression`) · **Budget:** 10 µs + 2 µs per
  agent (`NPC_BUDGET_TABLE`) · opens `info_span!("mechanic", name = "progression")` when the feed is not empty (nested in the system's own `progression`-tagged span, so `PerfLayer` does not time it a second time).

## Related

- Rolls come from `npc_rules`; `RulesPlugin` adds **no** systems (rolls run inside callers, in a `rules`
  mechanic span).
- Components: [`Experience`](../components/Experience.md), [`Learning`](../components/Learning.md),
  [`SpendPolicy`](../components/SpendPolicy.md), [`CharacterSheet`](../components/CharacterSheet.md)
