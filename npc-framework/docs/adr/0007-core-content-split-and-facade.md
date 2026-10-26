# ADR 0007: Core/content split behind an `npc_framework` facade

- Status: Accepted (P3 implemented the split, registration and facade: see ADR 0009; P4 adds the RTS demo)
- Date: 2026-10-23
- Source: `docs/superpowers/specs/2026-10-23-baseline-hardening-design.md` §2, `docs/research/framework-best-practices.md` §0.2

## Context

`npc_sim` mixes kernel code with villager content: `SpawnVillager` in `SimCommand`, content components
hard-coded in `hash.rs`, closed `NeedKind`/`PlaceKind` enums, and `npc_view` naming villager types. Another
Bevy game, such as the user's RTS, could not use the framework without inheriting villagers or forking it.

## Decision

- Generic crates hold no game content: `npc_kernel`, `npc_ai`, `npc_needs`, `npc_jobs`, `npc_locomotion`,
  `npc_perf`, `npc_tools`, the optional `npc_view_helpers`, and the facade `npc_framework` (`NpcPlugins`
  PluginGroup, `prelude`, features `tools`, `remote`, `perf`, `view-helpers`).
- Content lives in `content/*` (first `content/village_content`) and `apps/*`, and depends only on the facade.
- Content registers by **stable name** (`register_action/cond/scorer/tree/need/sim_component/command`).
  Ids are frozen sorted by name; trees, the journal and replays store names; replays carry a content hash.
- Guardrails land before the split: `arch_fitness` (`cargo metadata`) and `cargo-deny` bans fail the build
  if a core crate depends on content, rendering or `npc_view`, and the goldens and replay corpus prove the
  split did not change village behaviour.
- `apps/rts_units_demo`, using only the facade, proves portability.

## Consequences

- New unit types and mechanics need no core edits. A missing extension point is an API gap to fix in core
  deliberately, not a content special case.
- Registration order no longer affects ids or hashes (pinned by `registration_order_independent`).
- The hash encoding changes once (registry-driven), which needs an equivalence proof and a re-baseline.
- More crates to maintain; the facade keeps the user-facing surface small.
