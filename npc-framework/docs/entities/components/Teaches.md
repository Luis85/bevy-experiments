---
type: Component
---
# `Teaches`

**Crate:** `npc_framework` (glue `progression_interaction`) · **File:** [`crates/npc_framework/src/glue/progression_interaction.rs`](../../../crates/npc_framework/src/glue/progression_interaction.rs) · **Capability:** progression × interaction glue (`ProgressionInteractionPlugin`, features `progression` + `interaction`)

Something to study: a book, a manual, a training dummy. It makes the entity a target of the `study` verb.
A session lasts `session_ticks` and then grants that many study ticks on `skill` through
`npc_progression::grant_study`, capped at attribute + `study_cap_above_stat` and at `max_level`.
It lives in the glue because only an interaction verb can act on it (ADR 0012 keeps `npc_progression` off
`npc_interaction`).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `skill` | `String` | registered skill name it teaches |
| `max_level` | `i32` | highest effective skill level it can teach |
| `session_ticks` | `u32` | ticks: the `study` verb's duration, and the study ticks granted per session |

## Lifecycle

- **Added by:** content. The testbed inserts `content::teaches(kind)` on items of a teaching kind (books teach
  Carpentry, scrolls Smithing) in `spawn_item` and `carried_item`
  ([`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs),
  [`content.rs`](../../../content/testbed_content/src/content.rs)).
- **Read by:** the `study` verb's offer, precondition (`study_pre`: learner has `Experience` + sheet, skill is
  registered, level below `max_level`), duration, effect and `requires` (`learns:<skill>`). The verb runs through
  `npc_interaction` (target carried or within reach, claimed for the session).
- **Written / removed by:** nothing.

## Hashing and snapshots

- **State hash:** yes, as `teaches` (`register_sim_component` in `ProgressionInteractionPlugin::build`).
  Encoding: `skill` (string), `max_level` (`i32`), `session_ticks` (`u32`).
- **World snapshot (ADR 0013):** saved as `teaches` (automatic with hashing).

## Related

- [`Teacher`](Teacher.md) — the unit-to-unit counterpart (`teach` verb).
- [`Learning`](Learning.md) — receives the study ticks (`SkillProgress::study`).

## Example

```rust
use npc_framework::glue::Teaches;

// A carpentry manual: up to level 12, 200-tick sessions.
spawn_sim(world, (pos, Item::new("book"), Perceivable::new(ITEM), Teaches::new("Carpentry", 12, 200)));
```
