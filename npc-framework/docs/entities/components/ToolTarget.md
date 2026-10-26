---
type: Component
---
# `ToolTarget`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/targets.rs`](../../../crates/npc_interaction/src/targets.rs) · **Capability:** interaction (`InteractionPlugin`)

Something worked with a tool through the built-in verb `use_tool_on` (a tree chopped with an axe, an ore
rock mined with a pickaxe). The actor needs an **equipped** item whose kind name or tag matches
`required_tag`. The work takes `duration_ticks`, may need a skill check (rolled through the skill-check hook;
the facade's `interaction_rules` glue rolls it with `npc_rules`, purpose `interaction.use_tool_on`), spawns
`yields` at the actor's feet on success and uses up one of `remaining` uses. A success also makes a noise
(radius 10 000 mm) through the noise hook.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `required_tag` | `String` | item kind name or tag of the needed tool (e.g. `"tool:axe"`) |
| `yields` | `Vec<ItemYield>` | items spawned on success: `ItemYield { kind: String, stack: u32, quality: i32 }` |
| `duration_ticks` | `u32` | ticks the work takes (0 = completes in the start tick's completion pass) |
| `check` | `Option<SkillCheck>` | `SkillCheck { skill: String, modifier: i32 }`; `None` = no roll |
| `remaining` | `Option<u32>` | uses left; `None` = unlimited, `Some(0)` = depleted (`"is used up"`) |

## Lifecycle

- **Added by:** content (testbed trees and ore rocks in
  [`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** `use_tool_pre`, `use_tool_duration`, `use_tool_check`, `use_tool_requires`, `tool_for`
  ([`builtin.rs`](../../../crates/npc_interaction/src/builtin.rs)); glue `interaction_agent.rs` (affordances, odds,
  describer); testbed content/view.
- **Written / removed by:** `use_tool_effect` decrements `remaining` (saturating) on success. A failed check
  spends the time and changes nothing.

## Hashing and snapshots

- **State hash:** yes, as `tool_target` (`register_interaction_hash`, [`hash.rs`](../../../crates/npc_interaction/src/hash.rs)):
  tag, yields, duration, optional check, optional `remaining`.
- **World snapshot (ADR 0013):** saved as `tool_target` (automatic with hashing).

## Related

- [`Equipment`](Equipment.md) — where the tool must be.
- [`Interacting`](Interacting.md) — the running `use_tool_on`.
- [`Item`](Item.md) — what it yields.

## Example

```rust
use npc_framework::interaction::{ItemYield, ToolTarget};

spawn_sim(world, (
    SimPos::new(6000, 3000),
    ToolTarget::new("tool:axe").yields(ItemYield::new("log", 2)).duration(60).check("Woodcutting", 2).uses(3),
));
```
