---
type: Component
---
# `Learning`

**Crate:** `npc_progression` · **File:** [`crates/npc_progression/src/model.rs`](../../../crates/npc_progression/src/model.rs) · **Capability:** progression (opt-in, `ProgressionPlugin`)

Per-skill learning state: how often each skill was rolled and the ticks accrued toward the next free skill
point from practice, study and teaching. Only skills the unit used or learned appear. It comes with
[`Experience`](Experience.md) (required component).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `skills` | `BTreeMap<String, SkillProgress>` | skill name → state, name order |

`SkillProgress`:

| Field | Type | Unit / meaning |
|---|---|---|
| `uses` | `u32` | rolls of this skill (what the AI spend policy weighs) |
| `practice` | `u32` | practice ticks toward the next point (held at one point's worth while capped) |
| `study` | `u32` | study ticks toward the next point |
| `teaching` | `u32` | teaching ticks toward the next point |

Each source converts every `*_ticks_per_point` ticks (defaults: practice 300, study 200, teaching 100) into one
skill point, under its cap: practice attribute + 2, study attribute + 4 (and the book's `max_level`),
teaching the teacher's level (`ProgressionConfig`). Helpers: `get(skill)`, `uses(skill)`.

## Lifecycle

- **Added by:** required by [`Experience`](Experience.md).
- **Written by:** [`progression_system`](../systems/progression_system.md) via `apply_rolls` (each roll of a
  registered skill: `uses += 1`, `practice_per_roll` = 30 practice ticks); `grant_practice` (the crafting hook
  adds a completed craft's duration); `grant_study` (`study` verb); `grant_teaching` (`teach` verb).
- **Read by:** `ai_spend` (candidates with `uses > 0`); `experience_json` (`skills`); the agent-io
  `progression` section. View only: `testbed_view` panels.

## Hashing and snapshots

- **State hash:** yes, as `learning` (`register_sim_component` in `ProgressionPlugin::build`,
  [`crates/npc_progression/src/lib.rs`](../../../crates/npc_progression/src/lib.rs)). Encoding: length, then per
  skill the name and the four `u32` counters.
- **World snapshot (ADR 0013):** saved as `learning` (automatic with hashing).

## Related

- [`Experience`](Experience.md) — its owner.
- [`Teaches`](Teaches.md), [`Teacher`](Teacher.md) — sources of study and teaching ticks.
- [`SpendPolicy`](SpendPolicy.md) — `Ai` uses `uses` to pick skills.

## Example

```rust
use npc_framework::progression::{Learning, grant_practice};

let out = grant_practice(world, unit, "Smithing", 120).unwrap();
let progress = world.get::<Learning>(entity).unwrap().get("Smithing");
println!("{} points now, practice {}/300", out.points, progress.practice);
```
