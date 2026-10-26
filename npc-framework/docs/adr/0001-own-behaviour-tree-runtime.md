# ADR 0001: Our own behaviour-tree runtime (`npc_ai`)

- Status: Accepted
- Date: 2026-10-23
- Source: `docs/superpowers/specs/2026-10-23-villager-npc-design.md` §6.3, `docs/research/ai-prior-art.md`

## Context

NPCs need reactive decisions (an urgent need aborts lower-priority work), multi-tick actions, and a decision
model that scales to hundreds of agents in a deterministic, lockstep-ready sim. The Bevy crates surveyed in
late 2026 did not fit: `bevior_tree` boxes per-agent node state and ticks through an exclusive `World`;
`bevy_behave` is not on Bevy 0.20, clones trees per agent, uses `f32` `Time` timers and has no reactive
aborts; `beet` is entity-per-node; `big-brain` is archived. Shipped games (Unreal BT, RimWorld ThinkTree,
Halo) share one pattern: a shared tree definition plus small per-agent memory, event-driven re-thinking,
and decorator aborts.

## Decision

Build an ECS-native runtime in `npc_ai`, depending only on `bevy_ecs`/`bevy_app` and knowing nothing about
any game:

- A flat, serializable `TreeDef` (pre-order nodes with child ranges) with a content hash, shared by all
  agents through a registry; per-agent `BrainState`, `Blackboard` and `ActiveLeaf`.
- Node kinds `Selector`, `Sequence`, `Guard { cond, abort: None | LowerPriority }`, `Leaf { action, param }`
  and `UtilitySelector` (integer scores, ties go to the lower index).
- `think` runs only for dirty agents (leaf finished, `ThinkRequest`), sorted by `SimId`.
- Leaves start actions that are ordinary components (ADR 0005).

## Consequences

- Full control over determinism, aborts and performance; no dependency on a third-party crate's Bevy
  release cadence.
- We maintain the runtime and its tests (`crates/npc_ai/tests/runtime.rs`) ourselves.
- Trees are built in Rust for now. The IR is serializable, so RON definitions can follow (a non-goal this round).
- A rule came out of it: every `Abort::LowerPriority` guard condition must imply that its child can start;
  otherwise each re-think aborts and restarts the lower-priority work.
