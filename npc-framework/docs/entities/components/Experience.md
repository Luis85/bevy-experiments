---
type: Component
---
# `Experience`

**Crate:** `npc_progression` · **File:** [`crates/npc_progression/src/model.rs`](../../../crates/npc_progression/src/model.rs) · **Capability:** progression (opt-in, `ProgressionPlugin`, needs `RulesPlugin`)

A unit's experience: total XP and level. Units progress only with this component **and** a
[`CharacterSheet`](CharacterSheet.md). It brings [`Learning`](Learning.md) and [`SpendPolicy`](SpendPolicy.md)
(default `Ai`) with it (`#[require(Learning, SpendPolicy)]`). Level 0 is the starting level.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `xp` | `u64` | total XP ever gained (saturating add) |
| `level` | `u32` | current level, 0..=`ProgressionConfig::max_level` (default 50) |

Levels follow `ProgressionConfig::curve` (default `LevelCurve::Triangular { base: 100 }`: totals 100, 300, 600,
1000, ..). Each level banks `points_per_level` (default 5) character points on the sheet.

## Lifecycle

- **Added by:** content spawns. The testbed's `spawn_unit` adds `(Experience::default(), policy)`
  ([`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Written by:** `add_xp` (XP, then each `LevelUp`), reached from
  [`progression_system`](../systems/progression_system.md) (roll XP from the rules roll feed) and from
  `award_xp` (craft XP through the progression × crafting hook `craft_progress`, or content sources).
  The world API writes it back only when it changed.
- **Read by:** `experience_json` (inspect JSON of this component, and the base of the agent-io `progression`
  section, [`progression_agent.rs`](../../../crates/npc_framework/src/glue/progression_agent.rs)); the
  `study` / `teach` verbs (a learner needs it,
  [`progression_interaction.rs`](../../../crates/npc_framework/src/glue/progression_interaction.rs)). View only:
  `testbed_view` panels.

## Hashing and snapshots

- **State hash:** yes, as `experience` (`register_sim_component_with(experience_json)` in
  `ProgressionPlugin::build`, [`crates/npc_progression/src/lib.rs`](../../../crates/npc_progression/src/lib.rs)).
  Encoding: `xp` (`u64`), `level` (`u32`).
- **World snapshot (ADR 0013):** saved as `experience` (automatic with hashing; serde form).

## Related

- [`Learning`](Learning.md), [`SpendPolicy`](SpendPolicy.md) — required with it.
- [`CharacterSheet`](CharacterSheet.md) — where level points land.

## Example

```rust
use npc_framework::progression::{Experience, SpendPolicy, XP_JOB, award_xp};

let (unit, _) = spawn_sim(world, (sheet, Experience::default(), SpendPolicy::Player));
// From sim code (a system, effect or hook), not from tooling:
let award = award_xp(world, unit, 250, XP_JOB).unwrap();
assert_eq!(award.level, 1); // 100 XP for level 1, 300 for level 2
```
