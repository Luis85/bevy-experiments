---
status: "accepted"
date: 2026-10-25
decision-makers: "controller (accepts under the user's delegation, R85), lead"
consulted: "M2 prelude planner (planner-m2p-report.md; R148); research: architecture-best-practices.md N13, C21; baseline-recommendations.md C7"
informed: "M2 prelude implementers (Tasks 13, 14, 17); reviewers"
---

# Sim event model: Messages for gameplay, hooks for invariants, observers only for order-independent reactions

## Context and Problem Statement

Bevy 0.20 has four ways to react to something in the world: buffered Messages read by ordered systems, component
hooks, observers (`On<E>`, entity events) and entity disabling (a default query filter). Spec §5.4 has said "no
observers in sim crates until the event-model ADR (C7)", and the R73 check (ADR-0006 refinement) rejects `Observer`
entities in the sim world. M2 brings damage, death, suppression and effects, each a chain of events across features,
and C7 asks the ADR to say which mechanism each kind of reaction uses. Observers run immediately when their trigger
fires, and several observers of one event run in an order that is deterministic for a fixed build but unspecified
(research N13, `observer/centralized_storage.rs`), so an observer that writes shared state could change outcomes with
an unrelated feature added.

## Decision Drivers

* Every order-sensitive effect follows `SimIndex` or an explicit sort (ADR-0005, narrowed ordering rule).
* Reactions must not depend on the order of observer registration, entity storage or command flushing.
* Hashed state, saves and snapshots see every sim entity (ADR-0006); nothing hides entities from queries.
* The rule is simple enough for agents to follow and reviewers to check.

## Considered Options

* Messages by default, hooks for invariants, limited observers
* Observers as the default
* No observers at all

## Decision Outcome

Chosen option: "Messages by default, hooks for invariants, limited observers", because
it keeps every gameplay reaction in an ordered system while leaving a narrow, checkable use for observers.

- **Messages are the default for gameplay events** (damage, death, spawn and despawn requests, effects, shots). They
  are added only with `SimBuilder::add_message` under a `"core.msg.<x>"` or feature name (R47), read by systems in a
  fixed hook, and cleared at the end of the tick (R72): a message is read in the tick it was written, and anything
  that must reach a later tick is Hashed state. A reader whose result depends on order sorts first (by `SimId`, then
  emission order), as `apply_effects` does (ADR-0017).
- **Every message has an emission window**: its writers run before its reader in the schedule. A writer that runs
  later in the tick, or that reacts to the reader's own output, must not write the message, because R72 clears it
  unread at the end of the tick. It puts the event into Hashed pending state that the reader drains first on the next
  tick instead. Effects are the first case: an effect requested after the core `ResolveSet::Effects` slot goes into
  the Hashed `EffectQueue`, and a debug assertion catches an `EffectRequest` message written after `apply_effects`
  ran (ADR-0017, R151). A feature message with late writers follows the same pattern.
- **Component hooks maintain indices and invariants only:** `SimIndex`, the spatial index, `Related<R>`
  (ADR-0018). A hook never draws randomness, allocates a `SimId`, spawns or despawns, or applies a gameplay rule.
- **Observers** are allowed in sim crates only for order-independent reactions: insert a marker component on the
  observed entity, or push into an allowed buffer. The allowed buffers are a Message registered with
  `SimBuilder::add_message` whose reader sorts it (by `SimId`, then emission order) before use, and a named Transient
  resource registered as such, whose consumer sorts it the same way. Those two are the only resource writes an
  observer may make. Otherwise an observer never draws randomness, allocates `SimId`s, spawns or despawns, or writes
  a resource or another entity's components.
  "Order-independent" means the world after the flush is the same whatever order the observers of one trigger run
  in. Until a feature needs one, the R73 check keeps rejecting `Observer` entities (the test
  `observers_are_not_allowed_in_the_sim_world`); the first user adds a `SimBuilder` method that spawns named observers
  at build time, never mid-match, so the check can tell them apart, and ships a test that its reaction is
  order-independent. That method is a contract change under this ADR.
- **Entity disabling is not used in the sim.** A disabled entity drops out of every query without `Allow<T>`,
  including the hash, save and snapshot queries. Rare states that need filtering use a SparseSet marker and
  `Without<T>` (`Embarked`, ADR-0018).
- **Unchanged rules.** No gameplay logic on `Changed`, `Added` or `RemovedComponents` (`Changed<T>` only in
  `Phase::Publish`), no `Local<T>` in sim systems, no `Entity` in hashed or saved state (spec §5.4).
- **Where the rule lives.** `.claude/rules/sim.md` (Task 13) and the reviewer checklist in `.claude/agents/reviewer.md`
  (Tasks 13 and 14) carry it; spec §5.4's hazard line points here.

### Consequences

* Good, because every gameplay chain (shot → hit → damage → death) is a sequence of ordered systems that the
  system-order snapshot shows and the shuffle runs and branch test check.
* Good, because the observer rule is narrow enough to check in review: a marker insert or a buffer push, nothing else.
* Bad, because a reaction that would be one observer is a message plus a reader system in a hook, which is more code.
* Neutral, because the client keeps observers as its default for UI and picking; this ADR covers sim crates only.

### Confirmation

* The R73 classification tests stay green: `observers_are_not_allowed_in_the_sim_world` and
  `relationships_are_not_allowed_in_the_sim_world` (`fl_sim/tests/classify.rs`), until a feature adds the observer
  method above with its own order-independence test.
* Effects apply in source order regardless of emission order: `effects_apply_in_source_order` (Task 17). Late
  effects wait in the `EffectQueue` and a late message is caught: `late_effect_is_queued_for_next_tick`,
  `late_effect_message_is_debug_asserted` (Task 17).
* Relationship bookkeeping by hooks: `relate_maintains_sources`, `despawned_target_strips_sources` (Task 13).
* Review: the reviewer checklist item on observers, hooks and disabling (Tasks 13 and 14), applied at tier A
  (ADR-0022); the system-order snapshot (`protected`) shows every new reader system in its hook.

## Pros and Cons of the Options

### Messages by default, hooks for invariants, limited observers

* Good, because ordering stays explicit and visible in the schedule.
* Good, because it leaves room for cheap, safe observers (markers, buffers) later.
* Bad, because the boundary between "order-independent" and not needs judgement in review.

### Observers as the default

* Good, because it is Bevy's recommended reactive style and needs the least plumbing.
* Bad, because observers of one event run in an unspecified order and immediately inside command flushes, so a shared
  write would make outcomes depend on registration order and flush points, which no hash test pins until it breaks.
* Bad, because observer chains are invisible in the system-order snapshot.

### No observers at all

* Good, because it is the simplest rule and the one M0 and M1 followed.
* Bad, because it forbids harmless uses (inserting a marker when a component is added) that would otherwise need a
  per-tick scan system.

## More Information

* Spec §5.4 (Bevy hidden-state hazards), §5.6 (hooks, messages, `ApplyDeferred`), §9.2 C7.
* Research: `architecture-best-practices.md` N13 (event model table), C21.
* Related: ADR-0005 (phases, spawn funnel), ADR-0006 (R72 message clearing, R73 forbidden components), ADR-0017
  (effect messages and the `EffectQueue`), ADR-0018 (relationship hooks).
* Rulings: R47 (message names), R72 (messages cleared at the end of the tick), R73, R151 (late effects).
* Revisit: when the first sim feature wants an observer (it adds the builder method), or if Bevy specifies observer
  order.
