---
type: Component
---
# `CharacterSheet`

**Crate:** `npc_rules` · **File:** [`crates/npc_rules/src/sheet.rs`](../../../crates/npc_rules/src/sheet.rs) · **Capability:** rules (opt-in, `RulesPlugin`)

A unit's point-buy character sheet. It stores only what was **bought**: attribute levels, secondary deltas,
trait levels, skill points and banked points. Everything derived (secondary values, skill levels, point
totals) is computed on demand by `Rules` from the `RuleSet` and `RulesRegistry`, so it never goes stale.
Units need it to roll skills and stats (`roll_skill`, `roll_stat`), to progress, craft with a skill and pass
skill-checked interactions.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `attributes` | `Attributes { st, dx, iq, ht: i32 }` | absolute attribute levels (default 10 each; the rule set's base costs nothing) |
| `secondary` | `SecondaryBuys { hp, will, per, fp, speed_q, basic_move: i32 }` | bought deltas on top of the derived secondary values; `speed_q` is in quarters of Basic Speed |
| `traits` | `BTreeMap<String, u8>` | trait name → level (≥ 1); names are registered `TraitDef`s |
| `skills` | `BTreeMap<String, i32>` | skill name → points spent (≥ 1); names are registered `SkillDef`s |
| `unspent` | `i32` | banked, unspent character points |

Helpers: `from_template(&SheetTemplate)`, `skill_points(skill)`, `trait_level(name)`, `bought(stat)`.

## Lifecycle

- **Added by:** content spawns, usually from a registered template: `template_sheet(world, name)` or
  `apply_template(world, entity, name)` ([`crates/npc_rules/src/roll.rs`](../../../crates/npc_rules/src/roll.rs)).
  The testbed's `spawn_unit` uses `template_sheet`
  ([`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** the roll API (`attribute`, `effective_skill`, `roll_skill`, `roll_stat`, `quick_contest`, ..);
  the `per_roll` detection check (observer Per); `npc_crafting` (`run.rs`); the interaction × rules and
  interaction × agent-io glue; progression ([`progression_system`](../systems/progression_system.md), the grant
  functions); the rules × agent-io `sheet` section and `spend_points` affordance; the progression ×
  interaction `study` / `teach` verbs; BRP `npc/sheet` (`npc_tools`). View only: `testbed_view` F5 panel.
- **Written by:** `spend_points` (directly or via the recorded `SpendPoints` command), `grant_points`,
  `apply_template`; `npc_progression` (`add_xp` banks `points_per_level` per level, `ai_spend` buys skill points,
  `learn` adds free skill points from practice, study or teaching).

## Hashing and snapshots

- **State hash:** yes, as `character_sheet` (`register_sim_component_with(sheet_json)` in `RulesPlugin::build`,
  [`crates/npc_rules/src/lib.rs`](../../../crates/npc_rules/src/lib.rs)). Inspect and diff JSON is the full
  derived `SheetView`. Encoding: 4 attributes, 6 secondary deltas, length-prefixed traits and skills (name order),
  `unspent`.
- **World snapshot (ADR 0013):** saved as `character_sheet` (automatic with hashing; snapshots use the serde
  form of the bought values, not the `SheetView`).

## Related

- [`Experience`](Experience.md) — progression on top of the sheet.
- [`Stealthy`](Stealthy.md) — a modifier against the sheet's Per in the `per_roll` check.
- [`Teacher`](Teacher.md) — teachers need a sheet.

## Example

```rust
use npc_framework::rules::{CharacterSheet, Difficulty, RulesAppExt, SkillDef, Stat, roll_skill};

app.register_skill(SkillDef::new("Climbing", Stat::Dx, Difficulty::Average).default_stat(Stat::Dx, -5));
// ... after app.finish():
let mut sheet = CharacterSheet::default();
sheet.skills.insert("Climbing".into(), 2); // DX+0 = 10
let (unit, _) = spawn_sim(app.world_mut(), sheet);
let out = roll_skill(app.world_mut(), unit, "Climbing", -2, "climb.wall").unwrap();
assert_eq!(out.target, 8);
```
