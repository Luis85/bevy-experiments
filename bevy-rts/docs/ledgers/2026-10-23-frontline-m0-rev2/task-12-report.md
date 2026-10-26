# Task 12 report: SimRunner, drivers, snapshot publishing

Commit: `4f4040d feat(fl_sim): SimRunner (inline + threaded), Test/Replay/loopback Lockstep drivers, snapshot publishing`
(on `m0/phase-a`, parent `36adb7c`). Trailers: `Refs: ADR-0002, ADR-0007`, `Co-Authored-By`.

## What I implemented

- `crates/fl_sim/src/publish.rs`: `SnapshotOut` (Transient `"core.snapshot_out"`, `current()` accessor) holding the
  current and previous `Arc<SimSnapshot>`; `publish_snapshot` in the core slot of `PublishSet::Snapshot`.
  - One record per entity with `SimEntity + UnitType + Owner + Pose`, walked in `SimIndex` order and merged with
    last tick's (sorted) records.
  - A record is rebuilt only if `Ref::is_changed` on UnitType/Owner/Pose/Health/MoveOrder/Invulnerable, or if an
    optional component was **removed** (removal is not a change: detected as "record disagrees with the component
    being absent"); otherwise copied. A mutation check (disabling the removal check) made
    `incremental_publish_equals_full_rebuild` fail at tick 60 (invulnerable off), so the path is covered.
  - `hp_frac` from Health (`u16::MAX` without Health), flags MOVING / INVULNERABLE, `visible_to = 0xFF`, no turrets.
  - Buffer recycling: the previous Arc is reused when `Arc::get_mut` succeeds, else a fresh one; then swap.
  - Carried item (1): `debug_assert!` that records are sorted by sim_id.
  - Counters `"publish.records"` and `"publish.rebuilt"` (only when non-zero, like the other counters; the existing
    `counters_reset_each_tick` test asserts the exact counter set of an empty sim).
- `step.rs`: `TickOutput.snapshot` = `SnapshotOut::current()`, `TickOutput.events` = this tick's `SimEvents`.
- `runner/mod.rs`: `SimRunner { world }` (`new(app)` takes the App's World: `App` is not `Send`, the World holds
  the `SimTick` schedule), `next_tick`, `step`, `run` (panics `"driver stalled at tick T"`), `query`, `world`,
  `world_mut`. Query answers per brief: Entity → `inspect_entity`, WorldSummary → `inspect_world`, Perf →
  `{"counters": {...}}`, Header, JournalSince → `Journal([])`, Save → `Error("save: not implemented")`, anything
  else → `Error("unsupported query: …")` (SimQuery is `#[non_exhaustive]`).
- `runner/threaded.rs`: `spawn_threaded` via `std::thread::Builder::new().name("fl-sim")` behind
  `#[expect(clippy::disallowed_methods, reason = "SimRunner owns the sim thread (spec §5.3)")]`. Tick and Query
  are each run under `catch_unwind`; a panic sends `Failed { tick, message }` (message from `&str`/`String`
  payloads) and exits. Shutdown, a disconnected request channel, or a dropped reply receiver also exit.
  Replies carry the query id (carried item 3).
- `runner/test_driver.rs`: `TestDriver` (`new`/`Default` delay 0, `with_delay`, `starting_at`, `restore` →
  `CommandStamper::restore`).
- `runner/replay_driver.rs`: `ReplayDriver::new(inputs, start, until)`: sparse inputs (ticks without one run
  empty), inputs before `start` dropped, `until` exclusive (poll returns `None` once `until` is next), `issue`
  panics (a replay is read-only), `pending` = undelivered recorded commands.
- `runner/lockstep.rs`: `NetSession`, `LoopbackSession`, `loopback(players)` (every send reaches every session,
  sender included), `LockstepDriver::new(local, players, session, delay)` implementing the brief's 3-step poll.
- `inspect.rs`: `inspect_world` gains `"last_seq": {"<player>": seq}` (carried item 2: `LastSeq` readable through
  `SimQuery::WorldSummary`; inline callers use `last_seqs(world)`).
- `feature.rs` / `lib.rs`: core builds `publish`, exports `SimRunner`, the drivers, `NetSession`,
  `LoopbackSession`, `loopback`, `SnapshotOut`; crate docs updated.
- `xtask/clippy-template.toml` (+ regenerated `clippy.toml` of fl_sim, fl_view, fl_protocol, fl_math,
  fl_content): new ban `std::thread::Builder::spawn` (sim + edge profiles), same reason as `std::thread::spawn`.
  Without it, the brief's two requirements (thread named `fl-sim`, behind an `#[expect(disallowed_methods)]`)
  conflict: `Builder::spawn` was not banned, so the `#[expect]` would be unfulfilled. The ban also closes the
  loophole. Contract track (clippy configs); not in the Files list — flagged below.

## Tests (`crates/fl_sim/tests/runner.rs`, 19 tests + 1 unit test in threaded.rs)

The 13 brief tests plus: `other_queries_answer` (Perf/Header/JournalSince/Save/last_seq),
`test_driver_starting_at_and_restore`, `replay_driver_stops_at_until`, `run_panics_when_the_driver_stalls`,
`loopback_reaches_every_session`, `lockstep_command_issued_while_waiting_goes_to_the_next_batch`;
`threaded::tests::panic_messages_are_read_from_both_payload_kinds`.

`incremental_publish_equals_full_rebuild` (500 scale units, 200 ticks) scripts stop-wander, teleport, set_hp,
invulnerable on, invulnerable **off** on stationary units, and a debug spawn, and asserts every snapshot equals the
independent `publish_full(world)` helper and that, at the end, fewer records were rebuilt than published.

### TDD evidence

RED — `cargo nextest run --profile agent -p fl_sim --test runner` before any implementation:
```
error[E0432]: unresolved imports `fl_sim::LockstepDriver`, `fl_sim::ReplayDriver`, `fl_sim::SimRunner`, `fl_sim::TestDriver`, `fl_sim::loopback`
error[E0433]: cannot find `NetSession` in `fl_sim`   (x3)
error: could not compile `fl_sim` (test "runner") due to 4 previous errors
```
Expected: none of the runner/driver API existed.

GREEN — same command after implementing:
```
Starting 19 tests across 1 binary
Summary [   1.267s] 19 tests run: 19 passed, 0 skipped
```
Full crate — `cargo nextest run --profile agent -p fl_sim`:
```
Summary [  13.011s] 132 tests run: 132 passed, 0 skipped
```
(First full run failed `schedule::counters_reset_each_tick` because publish counted zero-valued counters on an
empty sim; fixed by counting only non-zero work.)

### Gate — `cargo xtask gate task --track contract --track sim-core`
```
PASS fmt (7.8s)
PASS clippy-drift (5.3s)
PASS claude-md (0.0s)
PASS check-scope (0.5s)
PASS protected (0.5s)
WARN test-guard (1.9s): test-guard: 2 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/tests/runner.rs:461 NewShouldPanic: #[should_panic(expected = "driver stalled at tick 2")]
    TEST-GUARD crates/fl_sim/tests/schedule.rs:387 RemovedAssert: assert_eq!( world.resource::<AfterCore>().rejected_in_apply, [1, 0], "the command was applied (and rejected) before the feature system ran; the log          hol…
PASS clippy (65.4s)
PASS arch-check (8.6s)
PASS test-headless (96.9s)
SKIP test-client: no client package touched since 36adb7c (.gate-base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 188s
```
- `runner.rs:461 NewShouldPanic`: mine, tests the brief's `"driver stalled at tick T"` panic.
- `schedule.rs:387 RemovedAssert`: **not mine** — an uncommitted edit of the concurrent Task 11 fix round in this
  same worktree (message-text rewrap); the gate sees the working tree.

## Files changed (commit 4f4040d)

- Created: `crates/fl_sim/src/publish.rs`, `crates/fl_sim/src/runner/{mod,threaded,test_driver,replay_driver,lockstep}.rs`,
  `crates/fl_sim/tests/runner.rs`
- Modified: `crates/fl_sim/src/{step,inspect,feature,lib}.rs`, `xtask/clippy-template.toml`,
  `crates/{fl_sim,fl_view,fl_protocol,fl_math,fl_content}/clippy.toml` (generated)

## Self-review / concerns

1. **Events are copied, not drained.** `step_tick` copies `SimEvents` into `TickOutput.events`; the buffer is
   still cleared at the next tick's `Housekeeping`, so each output holds exactly its tick's events (the
   `events_drained_per_tick` test passes). A true drain would empty the resource after the tick and break the
   T11-owned tests that read `SimEvents` after stepping (`tests/commands.rs` `events()`, `tests/funnel.rs`
   `events()`, `tests/schedule.rs::sim_events_cleared_each_tick`), which the concurrent Task 11 fix round is
   editing right now in this worktree. Switching to `drain()` is a one-line change in `step.rs` plus moving those
   helpers to `TickOutput.events`; I can do it once T11's fix is committed if the controller wants it.
2. **Clippy template change** (`std::thread::Builder::spawn` ban) is outside the Files list but inside the contract
   track; needed so the named thread's `#[expect]` is fulfilled (see above).
3. **LockstepDriver::issue** stamps `exec_tick = next + delay`, except while the driver is waiting on a peer at a
   tick whose `T + delay` batch was already sent: then the command goes one tick later (the batch is gone; adding
   to it could desync). This differs from the `TickDriver::issue` doc wording ("next_tick + input_delay"); covered
   by `lockstep_command_issued_while_waiting_goes_to_the_next_batch`. `issue` debug-asserts `draft.player == local`.
4. `ReplayDriver::issue` panics; `until` is exclusive. Task 18's verify path should be fine with that.
5. `hp_frac` without `Health` is `u16::MAX` (my choice; the brief does not say).
6. `last_seq` in `inspect_world` uses JSON object keys = player ids as strings (`{"1": 0}`).
7. **Shared worktree:** the T11 fix round is editing `debug_cmds.rs`, `scenario.rs`, `spawn.rs`,
   `tests/commands.rs`, `tests/schedule.rs` here uncommitted. I committed only my files (verified with
   `git diff --cached --stat`); my tests and gate ran against the combined tree. None of my files depend on their
   edits (the debug spawns in my tests are on the map, so the T11 clamp change does not matter).
8. Panics in the threaded tests print the standard panic-hook message to stderr (`boom`), as expected.
