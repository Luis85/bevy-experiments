# Task 18 report: sim observability core (journal, phase markers, tick span, replays)

Track: sim-core + contract. Refs: ADR-0007. Base: `.gate-base` = ccb40ab.

## What I implemented

### `fl_sim::observe` (new, `crates/fl_sim/src/observe.rs`)
- `JournalRecord` trait (`Serialize + Debug + Send + Sync + 'static`, `const KIND`).
- `Journal` resource: `enabled()`, `record(tick, phase, sim_id, player, &rec)` (no-op when disabled;
  `debug_assert!` that `R::KIND` is registered; data is `serde_json::to_value(rec)`), `len()`,
  `is_empty()`, and crate-private `take_sorted()`.
- `take_sorted` stable-sorts by `(tick, phase, kind, sim_id, emitted)`, then **renumbers `emitted`
  to the record's rank inside its `(tick, phase, kind, sim_id)` group**. Decision: a raw global
  emission counter differs between shuffle seeds whenever two unordered systems swap places, so
  the journal would not be identical across seeds; the per-group rank keeps the meaning
  ("emission order") and is shuffle-invariant as long as one system emits each group (the
  determinism contract). The `fl_view::JournalEntry` doc ("Emission order within the tick")
  still holds in that sense; fl_view was not touched.
- Phase markers: `add_phase_markers(app)` adds, per phase, a start marker system in a private
  `PhaseMarker(phase, true)` set `.before(phase)` (and after the previous phase's end marker) and
  an end marker `.after(phase)` (so after its `ApplyDeferred`) and `.before(next phase)`. Each emits
  exactly `tracing::trace!(target: "fl_sim::phase", phase = "<Phase>", edge = "start" | "end")`
  (the format fl_observe's profiler parses). Markers access nothing, so no ambiguities; no clock.
  `PHASE_TARGET = "fl_sim::phase"` is exported. I placed them adjacent to the phase sets from
  `observe.rs` rather than editing `schedule.rs` (not in my Files list); `system_order` labels them
  `?` only when `phase_markers` is on.
- Unit tests: sort + rank, disabled journal, unregistered kind panics in debug.

### Builder / build / step / runner (contract files)
- `SimBuilder::register_journal_record::<R>()`: stable-string registry exported as table
  `"journal_records"`. `finish()` classifies the `Journal` Transient `"core.journal"` (registrant
  `core`), freezes/exports the kinds, inserts the disabled `Journal`, and calls
  `allow_ambiguous_resource::<Journal>()`.
- `RunConfig` gains `journal: Option<bool>` (None = `cfg!(debug_assertions)`) and
  `phase_markers: bool`; `build_sim_app` enables the journal and adds the markers.
- `step_tick` drains `Journal::take_sorted()` into `TickOutput.journal` (the `sim_tick` span with
  field `tick` already existed from T9/T12 and is now tested).
- `SimRunner` keeps a ring of the latest `JOURNAL_RING = 10_000` entries (exported); `SimQuery::
  JournalSince { tick, limit }` answers the first `limit` ring entries with `tick >= tick`.

### Core records
- `commands.rs` (phase Input): `"core.command_applied" { command, seq, units, queue }` (units are
  the validated ones; `RunFn`/`run_command` now return them), `"core.command_rejected" { command,
  seq, reason }` (reason = `RejectReason` variant name, e.g. `"NoValidUnits"`; command = type name
  or `"#<id>"` for an unknown type), `"core.move_order" { waypoints }` per unit whose `MoveOrder`
  the move/stop handler changed. All with `player = Some(cmd.player)`.
- `spawn.rs` (phase Lifecycle, funnels only — setup/load spawns are not journaled):
  `"core.unit_spawned" { unit: Option<content id>, owner, pos }` with `sim_id`,
  `"core.unit_despawned" {}` with `sim_id`.

### `fl_sim::replay` (new, `crates/fl_sim/src/replay.rs`)
- `REPLAY_FORMAT_VERSION = 1`, `ReplayStart { Scenario, Scale, Save }` (`ScaleScenario` now derives
  `Serialize, Deserialize`), `ReplayEvent::ContentReload`, `Replay` (fields per brief),
  `to_bytes` (postcard), `from_bytes` (reads the leading `u32` version first → `Version { found }`;
  trailing bytes → `Decode`), `to_json` (serde_json of the whole replay), `reproducible()`.
- `ReplayRecorder::{new, record, finish}` (Clone, so `Scenario::replay(&self)` can snapshot it).
- `verify_replay`: version check → `NonReproducible` if any reload → build (Save starts go through
  `load_world`, mapping `LoadError::Header` to `ReplayError::Header`) with the recorded seed/cheats
  and `hash_interval`, journal off → `check_replay` (warnings kept) → set up the start → run
  `ticks` ticks through a `ReplayDriver` → `first_divergence`. A recorded hash whose tick the
  playback did not hash is `ReplayError::Setup` (so a replay never "verifies" vacuously).
- `first_divergence(a, b)`: earliest tick hashed by both with different hashes (input order
  irrelevant).
- `ReplayDriver::issue` still panics (decision): `TickDriver::issue` must return a stamped
  `PlayerCommand`, so neither an error nor "ignore" is expressible without inventing a command;
  a replay taking new commands is a programming error.

### fl_test_utils
- `Scenario` records every tick through a `ReplayRecorder`; `Scenario::journal()` (every entry so
  far) and `Scenario::replay()`. The start is `Scenario(id)` / `Scale(s)` when the builder set up
  nothing else, otherwise `Save(save_world(world, &[]))` of the world as set up; `Scenario::load`
  starts from `Save(bytes)`. The recorder's interval comes from the builder / the world's
  `HashInterval`.
- `tests/golden.rs`: `golden_replays_verify` verifies every `tests/replays/*.replay` (with
  `FL_RECORD_GOLDEN=1` it first writes `testbed_wander.replay.new`: base.testbed + `wander_all(100)`
  + moves at ticks 10 (P0 #1,#2) and 60 (P1 #6,#7), 400 ticks, hash every tick), plus
  `testbed_wander_replay_verifies` (a fresh in-memory recording verifies, so the check is never
  vacuous before blessing).

### Other test edits
- `tests/movement.rs`: one exhaustive `RunConfig { .. }` initializer gained `..RunConfig::default()`.
- `tests/runner.rs::other_queries_answer`: `JournalSince` used to assert an empty reply (the stub);
  it now asserts the move's `["core.command_applied", "core.move_order"]` when the journal is on
  (debug), empty otherwise. test-guard will list this replaced assertion.

## Golden replay — needs the lead
`crates/fl_test_utils/tests/replays/testbed_wander.replay.new` (6019 bytes) was produced with
`FL_RECORD_GOLDEN=1 cargo nextest run -p fl_test_utils --test golden` and is left **uncommitted**
(protected path). Please bless:
`cargo xtask bless crates/fl_test_utils/tests/replays --reason "M0 golden: testbed wander"` and
commit with `Blessed: M0 golden: testbed wander`. Until then `golden_replays_verify` checks zero
files (the fresh-recording test covers the same match).

## Not done (outside my brief)
- fl_observe's `profiler_sim` test was **not** switched to real markers: `crates/fl_observe/**` is
  the `tools` track, not in this task's Files/tracks. Carry: set `RunConfig { phase_markers: true,
  .. }` there and assert 8 phases (note its `log_filter` must let `fl_sim::phase=trace` through to
  the profiler layer, if the profiler sits under the filter).
- `SimCounters` were not given `allow_ambiguous_resource` (rev-1 did that; rev-2 brief only
  specifies it for the Journal).

## TDD evidence
I wrote the implementation and the tests in the same pass (not strictly test-first). RED was
captured afterwards by temporarily restoring `crates/fl_sim/src` to HEAD (new modules removed) and
running the new tests, then restoring my sources:

```
$ cargo nextest run --profile agent -p fl_sim --test observe --test replay
error[E0432]: unresolved imports `fl_sim::Journal`, `fl_sim::JournalRecord`, `fl_sim::PHASE_TARGET`
error[E0432]: unresolved imports `fl_sim::REPLAY_FORMAT_VERSION`, `fl_sim::Replay`, `fl_sim::ReplayError`, `fl_sim::ReplayEvent`, `fl_sim::ReplayRecorder`, `fl_sim::ReplayStart`, `fl_sim::VerifyOutcome`, `fl_sim::first_divergence`, `fl_sim::verify_replay`
error: could not compile `fl_sim` (test "observe") due to 1 previous error
error: could not compile `fl_sim` (test "replay") due to 1 previous error
```
Expected: the API does not exist before the task.

First green attempt then failed 2 tests (`phase_markers_emit_16_events_per_tick`,
`tick_span_carries_tick`): my capturing layer looked for `sim_tick` among the event's span
*ancestors*, but bevy_ecs builds each system's span at schedule build time, outside any tick, so a
system's events have no `sim_tick` ancestor (fl_observe's layer handles the same thing). The test
layer now tracks the stack of *entered* `sim_tick` spans; the spec wording "inside a span with
field tick" is satisfied in that sense.

GREEN:
```
$ cargo nextest run --profile agent -p fl_sim --test observe --test replay
     Summary 18 tests run: 18 passed, 0 skipped
$ cargo nextest run --profile agent -p fl_sim -p fl_test_utils
     Summary [  13.761s] 198 tests run: 198 passed, 0 skipped
```

## Tests added
- `fl_sim/tests/observe.rs` (9): applied_and_rejected_commands_are_journaled (+ move_order on move
  and stop), spawns_and_despawns_are_journaled (extra), journal_disabled_records_nothing,
  journal_does_not_change_state_hash, journal_identical_across_shuffle_seeds (seeds 1–3, two
  unordered recording systems), journal_entries_sorted (exact tick-3 order incl. emission rank),
  phase_markers_emit_16_events_per_tick (exact Input..Publish start/end sequence per tick, and none
  without the flag), tick_span_carries_tick, journal_since_query.
- `fl_sim/tests/replay.rs` (9): replay_round_trip_verifies, replay_bytes_are_checked (extra:
  trailing/truncated/version), tampered_replay_reports_divergence (tick 37, interval 1),
  replay_with_reload_event_is_non_reproducible, replay_build_id_mismatch_warns,
  replay_rules_hash_mismatch_fails, first_divergence_finds_earliest, replay_json_lists_inputs,
  scale_and_save_starts_verify (extra: Scale and Save starts).
- `fl_sim/src/observe.rs` unit tests (3).
- `fl_test_utils/tests/golden.rs` (2): golden_replays_verify, testbed_wander_replay_verifies.

## Files changed
- New: `crates/fl_sim/src/{observe,replay}.rs`, `crates/fl_sim/tests/{observe,replay}.rs`,
  `crates/fl_test_utils/tests/golden.rs`.
- Modified: `crates/fl_sim/src/{build,builder,commands,lib,scale,spawn,step,runner/mod}.rs`,
  `crates/fl_sim/tests/{movement,runner}.rs`, `crates/fl_test_utils/src/builder.rs`.
- Untracked for blessing: `crates/fl_test_utils/tests/replays/testbed_wander.replay.new`.
- No Cargo.toml/Cargo.lock change. LF line endings.

## Self-review
- Journal is observe-only: sim systems only write it; never hashed (Transient), never saved;
  `journal_does_not_change_state_hash` covers it.
- No `Entity` in records; unit types are content ids (names), not dense ids.
- Sorting includes `tick` first so records written between ticks (none in core) stay grouped.
- `brief` asked for `build.rs`/`builder.rs` to hold the registry and config; `feature.rs` and
  `schedule.rs` (contract, not in my list) are untouched.

## Gate

`cargo xtask gate task --track sim-core --track contract` (base ccb40ab):

```
PASS fmt (4.3s)
PASS clippy-drift (5.2s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
WARN test-guard (2.2s): test-guard: 10 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/src/observe.rs:259 NewShouldPanic: #[should_panic(expected = "'test.unknown' is not registered")]
    TEST-GUARD crates/fl_sim/src/observe.rs:260 NewIgnore: #[cfg(debug_assertions)]
    TEST-GUARD crates/fl_sim/tests/observe.rs:359 NewSeedOverride: fn recorded_journal(shuffle_seed: Option<u64>) -> Vec<JournalEntry> {
    TEST-GUARD crates/fl_sim/tests/observe.rs:361 NewSeedOverride: shuffle_seed,
    TEST-GUARD crates/fl_sim/tests/observe.rs:387 NewSeedOverride: fn journal_identical_across_shuffle_seeds() {
    TEST-GUARD crates/fl_sim/tests/runner.rs:314 RemovedAssert: assert_eq!( runner.query(SimQuery::JournalSince { tick: Tick(0), limit: 10 }), SimReply::Journal(Vec::new()) );
    TEST-GUARD crates/fl_test_utils/src/determinism.rs:76 NewSeedOverride: .filter(|k| (1..=SHUFFLE_SEEDS.len()).contains(k))
    TEST-GUARD crates/fl_test_utils/tests/schedule.rs:50 RemovedAssert: assert_eq!( systems.len(), labels.len(), "system_order lists every system" );
    TEST-GUARD crates/fl_test_utils/tests/schedule.rs:85 RemovedAssert: assert!(found.is_empty(), "changed after the snapshot: {found:#?}");
    TEST-GUARD crates/fl_test_utils/tests/schedule.rs:111 RemovedAssert: assert!( found[0].starts_with("Publish/Events ") && found[0].ends_with(": writes Pose"), "{found:#?}" );
PASS clippy (33.6s)
PASS arch-check (6.2s)
PASS test-headless (111.9s)
SKIP test-client: no client package touched since ccb40ab (.gate-base)
PASS data-check (35.4s)
gate task: PASS in 200s
```

test-guard findings of this task: observe.rs:259/260 (new should_panic, gated on debug_assertions because the kind check is a debug_assert), tests/observe.rs:359/361/387 (shuffle seeds are the point of the test), runner.rs:314 (the empty-journal stub assertion replaced by the real records). The determinism.rs:76 and fl_test_utils/tests/schedule.rs findings are in files this task did not touch (Task 17 diff).

## Commit

`40af03b feat(fl_sim): event journal with registry and per-tick sort, phase markers, tick span, postcard replays` (trailers `Refs: ADR-0007, ADR-0004`, `Co-Authored-By`). No `Blessed:` trailer: the golden `.new` is uncommitted, for the lead to bless.

## Follow-up (coordinator review after approval; golden blessed in ab35b19)

1. `journal_identical_across_shuffle_seeds` is no longer vacuous. The two recording systems are now the named fns `record_pings` / `record_pongs`. The new helper `recording_order(seed)` reads their order from `fl_sim::system_order`, and the test asserts that both appear and that seeds 1–3 do not all give the same order. The assertion passes, so the shuffle really reorders them.
2. Public docs on `JournalRecord` and `Journal` now state the rule: one system per `(phase, kind, sim_id)` group in a tick. If unordered systems write the same group, the sort does not make the journal deterministic. No debug detector was added, because the journal does not know which system emits a record.
3. `fl_view::JournalEntry::emitted` doc now reads: "The record's rank, in emission order, within its `(tick, phase, kind, sim_id)` group." This is the authorized contract doc edit.
4. `verify_replay` returns `ReplayError::Setup` when `ticks > 0 && hashes.is_empty()`. New test: `replay_without_hashes_does_not_verify`.
5. `golden_replays_verify` asserts that `tests/replays/testbed_wander.replay` is among the files it verifies.

Tests: `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_view` gave `210 tests run: 210 passed, 0 skipped`.

Gate: `cargo xtask gate task --track sim-core --track contract --base ab35b19`
```
PASS fmt (3.5s)
PASS clippy-drift (2.3s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
WARN test-guard (1.0s): test-guard: 2 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/tests/observe.rs:387 NewSeedOverride: fn recording_order(shuffle_seed: Option<u64>) -> Vec<&'static str> {
    TEST-GUARD crates/fl_sim/tests/observe.rs:389 NewSeedOverride: shuffle_seed,
PASS clippy (50.7s)
PASS arch-check (8.6s)
PASS test-headless (134.4s)
SKIP test-client: no client package touched since ab35b19 (--base)
PASS data-check (37.3s)
gate task: PASS in 239s
```
Both test-guard findings are the shuffle seeds of the new order check. Using those seeds is the point of the check.

Follow-up commit: `fb3ae78 test(fl_sim): non-vacuous journal shuffle and replay checks`
