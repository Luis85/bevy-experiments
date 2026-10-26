---
status: "accepted"
date: 2026-10-23
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: architecture-best-practices.md N5/C4/C8/C15; workflow-and-verification.md §4.5; baseline-recommendations.md A5, A7"
---

# Sim phases, explicit sync points, spawn funnel, driver housekeeping, narrowed ordering rule

## Context and Problem Statement

Revision 1 (§5.6) ran nine global sets, `ApplyCommands → Orders → Behaviour → Movement → Combat → Damage → Vision →
Lifecycle → Hash`, with the state hash inside the schedule and every state-affecting iteration sorted by `SimId`
(§5.4). Problems, all checked in the bevy 0.20 sources:
- `auto_insert_apply_deferred` defaults to `true`. Any feature system that uses `Commands` plus an ordering edge can add a
  sync point, which changes when *other* features' spawns become visible. It also limits schedule shuffling.
- Stepping a World with `run_schedule` skips `App` housekeeping: Messages never rotate, and `clear_trackers()` never
  runs, so removal buffers grow.
- A hash set inside the schedule invites features to schedule work after it.
- There was no phase for economy, production or publishing.
- The executor kind was implicit; `bevy_ecs/multi_threaded` gets unified in by the client binary.
- "Sort everything by `SimId`" costs time on 3,000+ entities per system and forbids `par_iter_mut` for entity-local
  updates, which is deterministic.

## Decision Drivers

* Visibility timing must not depend on which other features are linked.
* Housekeeping must happen the same way in every mode.
* Parallel agents adding systems must not create silent order dependencies.
* Room for later layers (economy, production) without a schedule rewrite.
* Scale (G7): no gratuitous sorting; allow safe parallelism.

## Considered Options

* Keep the nine sets; add sync points as needed
* Closed, documented phases with named hook sub-sets, explicit sync points, a spawn funnel, and housekeeping in the
  driver
* Fully explicit per-system ordering (`chain` everything)

## Decision Outcome

Chosen option: "Closed phases with hook sub-sets, explicit sync points, a spawn funnel, and housekeeping in the
driver".

- **Phases:** `Input → Think → Act → Resolve → Economy → Perceive → Lifecycle → Publish`, with public hook sub-sets:
  `Input::{Housekeeping, Validate, Apply}`, `Think::{Orders, Behaviour}`, `Act::{Move, Fire}`,
  `Resolve::{Projectiles, Effects, Stats}`, `Economy::Main`, `Perceive::{Spatial, Vision}`,
  `Lifecycle::{Death, Spawn, Despawn}`, `Publish::{Snapshot, Events}`. New sub-sets are a contract change.
- **Settings:** `ambiguity_detection: LogLevel::Error`, `auto_insert_apply_deferred: false`, and an explicit executor
  (`SingleThreadedExecutor` in M0).
- **Each phase ends with one explicit `ApplyDeferred`.** Deferred commands become visible at the next phase, no matter
  which features are linked.
- **Spawn funnel:** every in-tick spawn is a `SpawnRequest` Message handled in `Lifecycle::Spawn`, in request order,
  which assigns `SimId`s and instantiates defs. Nothing allocates a `SimId` in a hook, an observer or unordered
  `Commands`. Setup and save loading reuse the funnel code outside `SimTick`.
- **Messages** rotate once per tick in `Input::Housekeeping`; consumers read in the tick the message was written.
- **Driver steps** (`step_tick`, outside the schedule): (1) the inbox gets the tick's commands sorted by
  `(player, seq)`; (2) run `SimTick`; (3) hash if due; (4) take the snapshot, events and journal; (5)
  `world.clear_trackers()`; (6) advance the clock.
- **Ordering rule, narrowed (A7).** Only order-sensitive operations follow `SimIndex` order: shared writes, spawns,
  despawns and `SimId` allocation, budgets and queues, and cross-entity reads of same-tick writes. Entity-local updates
  may use any order, including `par_iter_mut`. The pure-compute-then-ordered-apply pattern stays for cross-entity
  parallel work. `SimIndex` is hook-maintained.
- **Interim event rule** until the event-model ADR (C7): Messages for gameplay events, hooks only for indices and
  invariants, no observers in sim crates.

### Consequences

* Good, because phase boundaries are the only visibility boundaries, independent of the feature set.
* Good, because there is one housekeeping path for every driver; Messages and trackers behave as in a normal `App`.
* Good, because `Economy` and `Publish` exist before anything needs them.
* Good, because entity-local systems can run in parallel later without a rule change.
* Bad, because a phase-level sync point means a spawn requested in `Think` appears only after `Lifecycle`. That is
  intended (spawns are end-of-tick), but tests that expect an immediate entity must step one tick.
* Bad, because the narrowed rule relies on authors classifying their access correctly. The branch test (ADR-0006),
  shuffle runs and review catch mistakes.

### Confirmation

* Schedule build fails on ambiguity (`LogLevel::Error`).
* `gate task` runs one `shuffle_seed` and `gate pr` runs three; hashes equal the unshuffled run.
* An insta snapshot of the built `SimTick` system order (phases, sub-sets, systems) is reviewed on every change.
* A test that `clear_trackers` and message rotation happen once per `step_tick`.
* The branch test (ADR-0006) catches iteration-order dependence.

## Pros and Cons of the Options

### Keep nine sets

* Good, because there is no rework of existing code.
* Bad, because of auto sync points, missing housekeeping and no room for economy or publishing.

### Phases, sub-sets, explicit syncs, funnel, driver housekeeping

* Good, because timing is deterministic and composable, with one housekeeping path.
* Bad, because it is a slightly more complex contract to document and snapshot.

### Chain everything

* Good, because it gives a total order.
* Bad, because it is unmaintainable across crates and kills all parallelism.

## More Information

* Spec §5.4 (ordering, executor), §5.6 (schedule), §12.3 (determinism checks).
* Research: `architecture-best-practices.md` N5, C4, C8, C15; `workflow-and-verification.md` §4.5.
* Related: ADR-0002 (`step_tick` callers), ADR-0006 (branch test), ADR-0009 (gates).

## Refinements (2026-10-23)

* **R46: core and feature slots per hook.** Every hook sub-set has a private core slot that runs first and a feature
  slot after it. `CoreFeature`'s systems go in the core slot; `SimBuilder::add_systems(hook, …)` puts every other
  feature's systems in the feature slot. Exclusive core systems (command apply in `Input::Apply`, message rotation in
  `Input::Housekeeping`, the spawn funnel in `Lifecycle::Spawn`) therefore never make a hook unusable for features,
  and feature systems see their effects in the same tick. `system_order` keeps its `"<Phase>/<Hook> <system>"`
  lines; core systems come first within a hook.
* **R47: named message buffers.** `SimBuilder::add_message::<M>(name)` takes a namespaced name
  (`"<feature>.msg.<message>"`, core uses `"core.msg.<x>"`) and classifies the `Messages<M>` buffer `Transient` under
  it, so the `sim_resources` string table never carries compiler-dependent Rust type names. Adding a message type a
  second time does nothing (deduplicated by type in the builder). Rotation runs in the core slot of
  `Input::Housekeeping`.

## Refinements (2026-10-24)

* **R72 replaces message rotation.** `step_tick` clears every `add_message` buffer at the end of the tick (after the
  outputs are taken, before `clear_trackers`), so a message is read only in the tick it was written and none survives
  to a save. The core slot of `Input::Housekeeping` now holds one exclusive system, `reset_transients`, which removes
  every `Transient` component and resets every resettable `Transient` resource (ADR-0006).
* **`SimCounters` is ambiguity-allowed,** like the `Journal`: counter adds commute, so systems that only count need
  no order among themselves (spec §15).
* **Debug checks after each tick.** In debug builds `step_tick` re-checks the classification when the tick registered
  new types and asserts that every entity with a classified component has a `SimEntity` (ADR-0006).

