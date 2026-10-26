---
type: Component
---
# `Teacher`

**Crate:** `npc_framework` (glue `progression_interaction`) · **File:** [`crates/npc_framework/src/glue/progression_interaction.rs`](../../../crates/npc_framework/src/glue/progression_interaction.rs) · **Capability:** progression × interaction glue (`ProgressionInteractionPlugin`, features `progression` + `interaction`)

A unit that teaches. It makes the unit the actor of the `teach` verb, whose target is a student with
`Experience` within reach. The verb teaches the first of `skills` (name order) that passes `teach_check`: the
teacher has the skill at a level of at least the student's + `teaching_min_lead` (default 1). A session lasts
`session_ticks` and then grants that many teaching ticks via `npc_progression::grant_teaching`. The student
never passes the teacher's level.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `skills` | `Vec<String>` | skills it teaches, sorted and deduplicated by `Teacher::new` |
| `session_ticks` | `u32` | ticks: the `teach` verb's duration, and the teaching ticks granted per session |

The teacher needs a `CharacterSheet` but not `Experience`.

## Lifecycle

- **Added by:** content. The testbed's `spawn_unit` inserts `content::teacher(template)` for templates listed in
  `TEACHERS` (a Smithing teacher)
  ([`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs),
  [`content.rs`](../../../content/testbed_content/src/content.rs)).
- **Read by:** the `teach` verb's offer (actor has `Teacher`, target has `Experience`, actor ≠ target),
  precondition and effect (`teach_skill`), duration and `requires` (`teaches:<skill>`). Refusals reuse the
  `ProgressionError` text, for example "teacher level 10 must be at least the student's 11 + 1".
- **Written / removed by:** nothing.

## Hashing and snapshots

- **State hash:** yes, as `teacher` (`register_sim_component` in `ProgressionInteractionPlugin::build`).
  Encoding: the skill count (`u32`) and each skill name, then `session_ticks` (`u32`).
- **World snapshot (ADR 0013):** saved as `teacher` (automatic with hashing).

## Related

- [`Teaches`](Teaches.md) — the item counterpart (`study` verb).
- [`Learning`](Learning.md) — receives the teaching ticks (`SkillProgress::teaching`).
- [`CharacterSheet`](CharacterSheet.md) — the teacher's level comes from it.

## Example

```rust
use npc_framework::glue::Teacher;

// A smith who teaches in 100-tick sessions.
world.entity_mut(smith).insert(Teacher::new(["Smithing"], 100));
```
