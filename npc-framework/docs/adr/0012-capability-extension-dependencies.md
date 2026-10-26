# ADR 0012: Extension crates may depend on the capability they extend

- Status: Accepted (coordinator decision before wave F)
- Date: 2026-10-24
- Amends: ADR 0011
- Source: `docs/superpowers/specs/2026-10-23-actor-capabilities-design.md` §4.4, §4.5, §4.7, §4.8

## Context

ADR 0011 says capability crates never depend on each other, and that pair code lives in facade glue. Wave F adds four crates. Two of them are *defined* as extensions of an earlier capability:
- the spec calls `npc_progression` "opt-in, needs `npc_rules`";
- the spec calls `npc_crafting` "opt-in, needs `npc_interaction` + `npc_rules`".

Expressing those through glue alone would hollow the crates out: their core types (XP sources that reward rolls, recipes with skill checks and item inputs) are built from the extended crate's types.

## Decision

Two tiers.

**Foundations** may be depended on by any capability crate. They are kernel, ai, needs, jobs, locomotion and nav. This was already true in wave E: `npc_interaction` depends on locomotion and nav.

**Capabilities** form a layered whitelist, with no other edges:

| Crate | May depend on (besides foundations) |
|---|---|
| `npc_perception`, `npc_rules`, `npc_agent_io`, `npc_interaction`, `npc_command` | none |
| `npc_progression` | `npc_rules` |
| `npc_crafting` | `npc_interaction`, `npc_rules` |
| `npc_vehicles` | `npc_interaction` (to register its own `mount`/`dismount` verbs) |

Rules for these dependencies:
- **Everything else is facade glue.** Any other capability pair still goes through `npc_framework::glue::<a>_<b>` (ADR 0011). That covers:
  - vehicles × command, e.g. `ByCrew` order forwarding;
  - command × interaction, e.g. `Interact` orders;
  - every `* × agent_io` pair: affordances and observations.
- **No cycles, no skipping.** A crate may not reach a capability that is not in its row, even transitively through another row. `arch_fitness.rs` encodes this table, and wave F agents add their row there (tightening only).
- **Opt-in still holds.** Adding the plugin of an extended crate does not activate the extension. `RulesPlugin` alone registers no progression content; progression registers its own plugin.

## Consequences

- Progression and crafting can use the rules and interaction types directly, with no adapter layer.
- Users who add `npc_crafting` also compile `npc_interaction` and `npc_rules`. That is acceptable, because crafting is meaningless without them.
- The dependency graph stays a DAG with at most two capability layers, and it is checked mechanically.
