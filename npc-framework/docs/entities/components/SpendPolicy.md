---
type: Component
---
# `SpendPolicy`

**Crate:** `npc_progression` · **File:** [`crates/npc_progression/src/model.rs`](../../../crates/npc_progression/src/model.rs) · **Capability:** progression (opt-in, `ProgressionPlugin`)

How a unit's level-granted character points are spent. It comes with [`Experience`](Experience.md) (required
component, default `Ai`).

## Variants

| Variant | Meaning |
|---|---|
| `Ai` (default) | After a level-up, spend the whole bank at once (`ai_spend`): one skill point at a time, each on the used skill with the highest `uses / (points + 1)` (cross-multiplied, ties by name). Units that used no skill keep their points banked. Journals one `SkillImproved { via: "spend" }` per skill. |
| `Player` | Bank the points on `CharacterSheet::unspent`; a player or agent spends them with the `npc_rules` `SpendPoints` command (registered by the rules × agent-io glue, or `register_command::<SpendPoints>()`). |

## Lifecycle

- **Added by:** required by `Experience`. Content inserts `SpendPolicy::Player` to bank instead. The testbed maps
  the unit spec's `spend_policy` (`"ai"` / `"player"`)
  ([`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** [`progression_system`](../systems/progression_system.md) and the world API (`award_xp`, grants):
  `add_xp` calls `ai_spend` only when levels were gained and the policy is `Ai`; `experience_json` (`policy`);
  the agent-io `progression` section (`"spending": "auto"` or `"bank"`). View only: `testbed_view` panels.
- **Written / removed by:** nothing in the framework (the world API's write-back skips it).

## Hashing and snapshots

- **State hash:** yes, as `spend_policy` (`register_sim_component` in `ProgressionPlugin::build`,
  [`crates/npc_progression/src/lib.rs`](../../../crates/npc_progression/src/lib.rs)). Encoding: `u8` tag
  (0 `Ai`, 1 `Player`).
- **World snapshot (ADR 0013):** saved as `spend_policy` (automatic with hashing).

## Related

- [`Experience`](Experience.md) — levels trigger the policy.
- [`CharacterSheet`](CharacterSheet.md) — `unspent` holds banked points.
- [`Learning`](Learning.md) — `uses` drives the `Ai` choice.

## Example

```rust
use npc_framework::progression::{Experience, SpendPolicy};

// Player-controlled hero: points stay banked for the SpendPoints command.
spawn_sim(world, (SimPos::new(0, 0), sheet, Experience::default(), SpendPolicy::Player));
// {"type":"SpendPoints","unit":5,"purchase":{"type":"Skill","skill":"Climbing","points":2}}
```
