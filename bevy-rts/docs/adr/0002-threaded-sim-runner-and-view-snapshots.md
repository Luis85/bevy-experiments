---
status: "accepted"
date: 2026-10-23
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: architecture-best-practices.md N2/C2/C19; baseline-recommendations.md A2"
---

# Threaded `SimRunner`, `fl_view` snapshots and `TickDriver`

## Context and Problem Statement

Revision 1 (§5.3, §14) left open whether the client owns the sim as a Bevy `SubApp` or as a `World` stepped from the
client's `FixedUpdate`; the M0 plan chose a `SimHost` resource holding the World. Both run the sim synchronously on the
main thread: `SubApp::update` is synchronous inside `App::update` (bevy_app 0.20 source). With a 15 ms tick (G7) a tick
frame blows the 16.7 ms frame budget. Separately, the planned "sync step" copied sim components into client components,
coupling `fl_client` to every sim component type, so every sim refactor would break the client. Change ticks are per
World and per system, so a sync step outside a system in another World also gets change detection wrong easily.

## Decision Drivers

* ≥ 60 fps with a sim tick of up to 15 ms (G7).
* The client must not depend on sim internals (two-crate rule, ADR-0001).
* One tick code path for every mode: client, headless tests, CLI, replays, lockstep, a future server.
* Lockstep later: the driver must be able to wait for peers' input.
* Debug tooling (inspector, BRP, CLI) needs read access to sim state without racing the sim.

## Considered Options

* Sim as a Bevy `SubApp` of the client
* Sim `World` in a client resource, stepped from `FixedUpdate` (M0 plan, revision 1)
* Threaded `SimRunner` behind a snapshot protocol, with pluggable `TickDriver`s

## Decision Outcome

Chosen option: "Threaded `SimRunner` behind a snapshot protocol", because it is the only option that keeps the tick off
the frame's critical path and decouples the client from sim types.

- `fl_sim::step_tick(world, TickInput) -> TickOutput` is the one function that advances a tick (ADR-0005 lists its
  steps). Nothing else runs `SimTick`.
- `SimRunner` owns the World. **Threaded** mode runs it on a dedicated thread, with commands in and outputs out over
  channels. **Inline** mode calls `step_tick` synchronously for tests, `fl_cli` and a future server. The API is the same.
- `TickOutput { tick, hash: Option<u64>, snapshot: Arc<SimSnapshot>, events: Vec<SimEvent>, journal (debug) }`.
  Snapshots are double-buffered and recycled once the client releases them.
- A new crate `fl_view` (depends on `fl_math` and `fl_protocol` only, no Bevy) defines `SimSnapshot`, `SimEvent`, view
  channels, `PlayerView` (later), and the runner protocol types (`TickInput`, `TickOutput`, `SimQuery`, `SimReply`).
  The sim's `Publish` phase writes the snapshot; `Changed<T>` is allowed there only.
- Features extend the snapshot with string-keyed **view channels**. Their data types are plain data in the feature sim
  crate's `view` module.
- `SimQuery` is answered between ticks, read-only: inspect entity, world summary, journal since tick, perf, save.
- `TickDriver` decides when to tick and which commands go in: `LocalDriver` (wall-clock accumulator, pause, step,
  speed, input delay 1), `LockstepDriver` (waits for every peer's commands; M0 ships an in-process loopback
  `NetSession`), `ReplayDriver` (as fast as possible), `TestDriver` (explicit, input delay 0).
- The client keeps the last two snapshots and interpolates by the overstep fraction. Interpolation already delays the
  view by one tick, so the extra thread adds no visible latency.
- This resolves revision 1's §14 open question ("SubApp vs client-held World").

### Consequences

* Good, because the frame costs about max(main, render, sim) instead of their sum.
* Good, because `fl_client` depends on `fl_view`, never on `fl_sim`; sim refactors don't break the client.
* Good, because headless, client, replay and lockstep runs share `step_tick`, so single-player bugs show up in tests.
* Bad, because the client can't read arbitrary sim state; anything it shows must be published (snapshot, view channel,
  event) or queried (`SimQuery`).
* Bad, because a full snapshot per tick costs memory bandwidth (about 12k records at 20 Hz). The `Publish` phase writes
  only changed records, and a `summary_tick` optimisation is available if profiling shows a need.
* Neutral, because sim systems that use the multi-threaded executor later share Bevy's global task pools with the
  client; measure contention before enabling it (spec §14).

### Confirmation

* `fl_client` has no dependency edge to `fl_sim` (`xtask arch-check`).
* A runner test: threaded and inline modes produce identical hash streams for the same inputs.
* A loopback `LockstepDriver` test: two in-process sims with input delay 2 stay hash-identical.
* The M0 render stress test (3,000 placeholders at ≥ 60 fps) runs with the threaded runner.

## Pros and Cons of the Options

### `SubApp`

* Good, because Bevy manages it, with an extract step.
* Bad, because `SubApp::update` is synchronous on the main thread.
* Bad, because extraction still couples the client to sim component types.

### World in a client resource, stepped from `FixedUpdate`

* Good, because it is simple, and it is what the M0 plan specified.
* Bad, because it is synchronous on the main thread, same as `SubApp`.
* Bad, because `world_mut()` invites the client to mutate the sim, breaking the commands-only rule.

### Threaded runner with snapshots

* Good, because it keeps the tick off the frame path and gives a clean read model.
* Bad, because it adds a protocol crate and channel plumbing.

## More Information

* Spec §5.3, §10 (units follow the snapshot), §11 (inspector via `SimQuery`), §15 C (BRP reaches the runner).
* Research: `architecture-best-practices.md` N2, C2, C19; `baseline-recommendations.md` A2.
* Related: ADR-0005 (`step_tick`), ADR-0007 (envelope, input delay), ADR-0008 (debug commands).

## Refinements (2026-10-25, M2 prelude)

Status: proposed (M2 prelude Task 3); the controller accepts it at that task's review under the user's delegation
(R85), and the plan's final report lists it for ratification. From the M1 final review (`final-review-m1.md` M3) and
controller ruling R123.

* **`FromRunner` is `#[non_exhaustive]`** (M3), as `ToRunner`, `SimQuery`, `SimReply` and `RunnerControl` already
  are, so the first new runner message (a desync report, late-join progress) is not a breaking change for every client
  `match`. Client code that matches it gains a wildcard arm.
* **`ToRunner::reply_id() -> Option<u64>`** (R123): the id a variant expects its reply under (`Query` and `Control`:
  `Some(id)`; `Tick` and `Shutdown`: `None`). The threaded runner answers a `ToRunner` variant it does not handle
  with `FromRunner::Reply { id, reply: SimReply::Error("unsupported runner message") }` when `reply_id()` is `Some`,
  so a newer client never waits forever on an older runner.
* **Confirmation.** `reply_id_of_query_and_control` (`fl_view`), and the `fl_view` and `fl_sim` API snapshots (Task
  16).
