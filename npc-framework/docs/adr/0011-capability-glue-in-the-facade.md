# ADR 0011: Capability glue lives in feature-gated facade modules

- Status: Accepted (implemented in actor capabilities wave E2)
- Date: 2026-10-24
- Source: `docs/superpowers/specs/2026-10-23-actor-capabilities-design.md` §4.2 (detection with Per), §4.6
  (hooks); `docs/superpowers/specs/2026-10-23-agent-control-design.md` §2.3, §3.1, §4 (perception anti-goal);
  ADRs 0007, 0009, 0010

## Context

Wave D shipped three opt-in capability crates that deliberately do not know each other:
`npc_perception` (a `DetectionChecks` hook by name), `npc_rules` (sheets, `roll_stat`, `spend_points`) and
`npc_agent_io` (observation/perception/affordance/handler registries). The specs ask for behaviour that needs
two of them at once: detection by a Perception roll, observations limited to what a unit perceives, the
character sheet and point spending for agents. That code has to live somewhere that may depend on both
crates of a pair, without making one capability crate depend on another (which would make every user of
perception pay for rules, and grow a dependency web as waves E/F add interaction, command, vehicles,
progression and crafting).

Options considered:

1. **A dependency between capability crates** (e.g. `npc_perception` → `npc_rules`). Rejected: breaks the
   "each capability works alone" anti-goal and creates order/cycle problems as crates multiply.
2. **A new crate `npc_glue`** with optional deps on every capability. Workable, but it is one more crate
   content would have to reach through the facade anyway (ADR 0007: content depends on the facade only),
   it needs its own rule rows in `arch_fitness.rs` (an oracle we do not edit casually), and its features
   would mirror the facade's one for one.
3. **Feature-gated modules of the facade** (`npc_framework::glue::*`), each compiled only when both
   features are on (`#[cfg(all(feature = "perception", feature = "rules"))]`). The facade already depends
   on every capability crate optionally and already owns the feature switches.

## Decision

- Glue between capability crates lives in `crates/npc_framework/src/glue/<a>_<b>.rs`, compiled only when
  both facade features are on, and exposed as its own plugin. Wave E2 adds:
  - `perception_rules` (`perception` + `rules`): `PerceptionRulesPlugin { activate }` registers the
    detection check `"per_roll"` (Per + distance band + sense + target modifiers by name, built-in
    `"stealthy"` reading `Stealthy { penalty }`), `PerRollConfig`; rolls are journaled as `Roll` with
    purpose `perception.detect`.
  - `perception_agent` (`perception` + `agent-io`): `PerceptionAgentPlugin` registers the perception
    provider `perception` (sensed + remembered percepts with last-seen ticks), so observations are
    `omniscient: false`.
  - `rules_agent` (`rules` + `agent-io`): `RulesAgentPlugin` registers the `sheet` section, the
    `spend_points` affordance/handler and the recorded command `npc_rules::SpendPoints`.
- Capability crates never depend on each other. They expose hooks by stable name (ADR 0009); a crate may
  define a pair-neutral type that glue registers (e.g. `SpendPoints` lives in `npc_rules`, but
  `RulesPlugin` does not register it, so the rules content hash is unchanged).
- Tooling that needs a capability goes behind a feature of the tooling crate (`npc_tools` feature
  `rules`: BRP `npc/sheet {unit}` with `remote`); the facade's `rules` feature forwards it.
- `NpcPlugins` gets opt-in builder switches (`with_perception`, `with_rules`, `with_agent_io`,
  `with_per_roll_detection`) that add the capability plugins and the glue plugin of every pair that is on.
  Glue plugins can also be added by hand (documented plugin tuple); the builder registers `per_roll`
  without activating it unless asked.
- A detection check only gets `&World`. `per_roll` therefore derives its dice purely from `(seed,
  observer, tick, "perception.detect", target · 4 + sense)` instead of the rules crate's per-tick counter,
  and buffers its journal entries in a non-hashed `PerRollLog` (a `Mutex`, filled in the perception pass's
  deterministic visit order), recorded by a system in `NpcPerceptionSet` right after the pass.
- Glue modules are sim code: they deny the determinism lints per module (the facade has no
  `clippy.toml`) and follow `.claude/rules/sim.md`, which now lists `crates/npc_framework/src/glue/**`.

## Consequences

- Each capability still works alone; a pair's glue costs nothing unless both features are on and its
  plugin is added. Village oracles are untouched (the village adds none of the glue plugins).
- Adding a glue plugin changes the content hash (new names: `per_roll`, `stealthy`, `per_roll_config`,
  the `SpendPoints` command, providers), so replays recorded with it verify only with it.
- The facade grows with every pair. If glue modules become large or need their own dependencies, move
  them to a `npc_glue` crate behind the same facade features; the public paths (`npc_framework::glue::*`)
  can stay as re-exports.
- The facade's own tests enable every capability through a self dev-dependency
  (`npc_framework = { path = ".", features = [...] }`), so the glue tests run in the workspace gate.
