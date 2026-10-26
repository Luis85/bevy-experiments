# Task 29 report: work-counter perf gates (entity mix, counting allocator, budgets)

Status: **waiting on the lead to bless** (Step 3 hand-off). The code is committed (`22f98cb`). The protected
artifacts exist only as `*.new` files. Until they are blessed, the gate's only failures are the two perf
tests that need `budgets.ron`. Steps 4 and 5 (the final PASS run) have to be re-run after the bless.

## What I implemented

### fl_sim (`scale.rs`, `feature.rs`, `step.rs`, `spawn.rs`, `lib.rs`)
- `ScaleScenario` has four new fields: `followers_per_unit`, `projectiles`, `projectile_ttl` and `sensor_every`.
  `Default` uses the brief's values (3000 units / 4 players / 2000 m / 150 m / seed 1 / 2 followers / 1000
  projectiles / ttl 20 / sensor_every 10). `with_units(n)` keeps the defaults and sets `projectiles = n / 3`.
- `populate` spawns the units exactly as before, so unit ids and placement are unchanged. It then spawns each
  unit's followers in id order:
  - each follower is a non-unit entity (`unit: None`);
  - its offset is drawn ±10 m on each axis from the same `setup.scale` draws, after all the units' draws;
  - it stands at `map.clamp(home + offset)` and gets `Follower { leader, offset }`.

  Finally `populate` inserts `ScaleMix`, which holds the projectile count, the ttl, `sensor_every` and the
  unit id list. It still returns only the unit ids.
- New components and resource:
  - `Follower` (Hashed `core.follower`) and `Projectile { vel, ttl: u16 }` (Hashed `core.projectile`), each
    with a `SimHash` impl;
  - `ScaleMix` (External `core.scale_mix`, fields private);
  - the stream `scale.fire`.
- Systems, registered by `scale::build`, which `CoreFeature::build` calls (one added line in `feature.rs`):
  - `follow_system` (ActSet::Move, `.after(move_system)`): `pos = map.clamp(leader.pos + offset)`. Counts
    `scale.follows`.
  - `scale_fire` (ActSet::Fire, only with `ScaleMix`):
    - fires `ceil(projectiles / ttl)` `SpawnRequest`s per tick;
    - for each shot it draws the shooter index, then the angle, from `rolls(seed, tick, SimId(0), "scale.fire")`;
    - the `init` fn inserts `Projectile { vel: from_angle(heading) × 10 m/tick, ttl }`.
  - `projectile_step` (ResolveSet::Projectiles): `pos = clamp(pos + vel)`, then `ttl -= 1`, and at 0 it
    writes a `DespawnRequest`. Counts `scale.projectile_steps`.
  - `scale_sense` (PerceiveSet::Vision, only with `ScaleMix` and `sensor_every > 0`): every unit with
    `id % every == tick % every` runs `query_radius(pos, 150 m)`. Counts `spatial.queries` and
    `spatial.query_candidates`.
- `sort.items` is counted in two places:
  - `despawn_funnel` counts the number of requests before the sort;
  - `step_tick` counts the inbox commands. It adds them after `run_schedule`, because `Housekeeping` clears
    the counters at the start of the tick.

  Like the other counters, both are counted only when non-zero.

### fl_test_utils (`src/perf.rs`, `src/lib.rs`, `Cargo.toml` + `serde`, `ron`; `tests/perf_counters.rs`)
- `BudgetFile`, `CounterRun`, `load_budgets`, `run_counter_scenario`, `check_budgets` and `record_budgets`, as
  specified. I also added a `budgets_path()` helper that resolves `docs/perf/budgets.ron`.
- `run_counter_scenario` runs on `fl_catalog::features()` and `base_content()`:
  - the hash interval is the rules default and the journal is off, as in release;
  - the warmup ticks are not measured;
  - after each measured tick it collects the `SimCounters` snapshot;
  - with an allocator it also counts:
    - `alloc.count` = allocations + reallocations during `step_tick`;
    - `alloc.bytes` = bytes allocated + positive realloc growth.
- `check_budgets` fails in three cases:
  - a counter is over its budget: `"scale_500: spawn.entities max 31 > budget 25"`;
  - **a counter the run saw has no budget**: `"…: X max N has no budget"`, so a new counter cannot slip past
    the gate;
  - the scenario has no entry at all: `"…: no budgets"`.
- `record_budgets` writes `<path>.new` with `ceil(max × 1.25)`. It keeps the other scenarios from `.new`
  (or else from the blessed file). A `.new.lock` file serializes the nextest processes recording in
  parallel.
- `tests/perf_counters.rs`:
  - `#[global_allocator] static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;` (safe code);
  - `smoke_counters_scale_500`, `counters_scale_3000` and `milestone_counters_scale_5000` each run 20 warmup
    + 200 measured ticks, then either record (`FL_RECORD_BUDGETS=1`) or check the budgets, then take a named
    insta RON snapshot of the totals without `alloc.*` (`perf_counters__scale_{500,3000,5000}.snap`);
  - `budget_violation_names_counter` is the synthetic test.
- There is also a unit test, `perf::tests::record_then_check_round_trips` (temp dir).

### Existing tests adjusted (sim-core)
`Default` now includes the mix, so tests that assume units only had to change:
- `fl_sim/tests/movement.rs` (`scale_poses`) and `fl_sim/tests/spatial.rs` (`scale_sim_cheats`) now set
  `followers_per_unit: 0, projectiles: 0`. Their intent was units only: exact grid lengths, `spatial.inserts`
  == 0, quadrant checks, and grid equality after save/load.
- `fl_sim/tests/replay.rs`: the full struct literal needed the new fields. It uses 1 follower per unit and 0
  projectiles, because `ScaleMix` is External and does not survive the save in that test.
- `fl_sim/tests/feature.rs`: the expected RNG stream table now includes `"scale.fire"`. This is the one
  **test-guard finding** (`RemovedAssert` at feature.rs:153). The value was updated, not weakened.

## TDD evidence
- RED: I put the `src` changes aside and ran `cargo nextest run --profile agent -p fl_sim --test scale_mix`.
  It failed to compile with `unresolved imports fl_sim::Follower, fl_sim::Projectile` and `no associated
  function with_units` ×4. That was the expected failure: the API did not exist yet.
- GREEN: the same command reports `Summary [0.952s] 4 tests run: 4 passed`.

## Tests run
- `cargo nextest run --profile agent -p fl_sim --no-fail-fast`: 189/189 passed (after the test adjustments
  above).
- `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`: 205/208 passed. The 3 failures are the
  perf tests reading the not-yet-blessed `budgets.ron`. The golden replay, the scale twin-run and the
  determinism tests all pass.
- `FL_RECORD_BUDGETS=1 cargo nextest run --profile agent -p fl_test_utils --test perf_counters -E 'test(/scale/)'`
  wrote `docs/perf/budgets.ron.new` and the three `crates/fl_test_utils/tests/snapshots/perf_counters__scale_*.snap.new`
  files (the tests fail on the new snapshots, as insta does). Three record runs gave byte-identical
  `budgets.ron.new`, alloc counts included, so the alloc counts are deterministic.
- **O(n²) check (Step 4):**
  - I temporarily added a loop in `scale_sense` in which every unit queries every unit (radius 5000 m), and
    pointed the test at `budgets.ron.new`, since the blessed file does not exist yet;
  - `smoke_counters_scale_500` failed with
    `scale_500: alloc.bytes max 25308 > budget 12435`, `scale_500: spatial.queries max 550 > budget 63`,
    `scale_500: spatial.query_candidates max 842488 > budget 3110`;
  - both temporary edits were reverted (verified: no `TEMPORARY` left; the committed files are the originals).

## Gate: `cargo xtask gate task --track sim-core --track perf --track contract` (`.gate-base` 8c7f0e5)
The contract track is needed because `step.rs`, `feature.rs` and `Cargo.lock` are contract paths.
```
PASS fmt (2.3s)
PASS clippy-drift (3.6s)
PASS claude-md (0.0s)
PASS check-scope (0.5s)
PASS protected (0.5s)
WARN test-guard (2.5s): test-guard: 1 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/tests/feature.rs:153 RemovedAssert: assert_eq!(names(tables::RNG_STREAMS), ["debug.wander", "setup.scale"]);
PASS clippy (13.1s)
PASS arch-check (8.1s)
FAIL test-headless (76.8s)
        FAIL fl_test_utils::perf_counters smoke_counters_scale_500
        FAIL fl_test_utils::perf_counters counters_scale_3000
        panicked at crates\fl_test_utils\src\perf.rs:62:9: reading the perf budgets
        C:\fl\a\crates\fl_test_utils\../../docs/perf/budgets.ron: ... (os error 2) (record them with
        FL_RECORD_BUDGETS=1; the lead blesses the .new file)
     Summary [62.611s] 454 tests run: 452 passed, 2 failed, 1 skipped
PASS test-client (37.7s)
PASS data-check (4.2s)
gate task: FAIL (test-headless) in 150s
```
These two failures are expected until the bless. `milestone_counters_scale_5000` is excluded by the gate.

## For the lead: bless, then re-run
```
cargo xtask bless docs/perf/budgets.ron crates/fl_test_utils/tests/snapshots --reason "M0 counter baseline (entity mix)"
```
The commit that adds the blessed files carries `Blessed: M0 counter baseline (entity mix)`. After that,
`cargo xtask gate task --track sim-core --track perf --track contract` should pass. Resume me if you want me
to run it and make the bless commit.

Recorded budgets (per-tick max × 1.25):

| scenario | spawn.entities | spatial.query_candidates | scale.follows | alloc.count | alloc.bytes |
|---|---|---|---|---|---|
| scale_500 | 12 | 3110 | 1250 | 108 | 12435 |
| scale_3000 | 63 | 98048 | 7500 | 187 | 351620 |
| scale_5000 | 105 | 267520 | 12500 | 189 | 75855 |

The full set is in `docs/perf/budgets.ron.new`. It also covers `despawn.entities`, `sort.items`,
`spatial.inserts`, `spatial.relocations`, `spatial.queries`, `publish.records`, `publish.rebuilt` and
`scale.projectile_steps`.

## Files
- Modified:
  - `Cargo.lock` (+`ron`, +`serde` for fl_test_utils)
  - `crates/fl_sim/src/{scale,feature,step,spawn,lib}.rs`
  - `crates/fl_sim/tests/{feature,movement,replay,spatial}.rs`
  - `crates/fl_test_utils/{Cargo.toml,src/lib.rs}`
- New:
  - `crates/fl_sim/tests/scale_mix.rs`
  - `crates/fl_test_utils/src/perf.rs`
  - `crates/fl_test_utils/tests/perf_counters.rs`
- Uncommitted, waiting for the bless:
  - `docs/perf/budgets.ron.new`
  - `crates/fl_test_utils/tests/snapshots/perf_counters__scale_{500,3000,5000}.snap.new`

## Concerns
1. **`feature.rs` is outside the brief's Files list.** I added one line there (`crate::scale::build(sim)`),
   because that is where the core systems are registered.
2. **`ScaleMix` is External, so it is not saved.** A save of a scale run therefore loads without firing or
   sensing; followers and projectiles in flight still update. Any save/branch test with the mix on would
   diverge. `ReplayStart::Scale` re-populates, so it is fine. If save/load of scale runs matters, `ScaleMix`
   should become Hashed. That is a brief-level decision.
3. **Changing `Default` changes behaviour elsewhere.** Every `..ScaleScenario::default()` caller now gets the
   mix. In particular, `fl_game` stress mode (`builder.rs:333`, outside my files) now spawns 2 followers per
   unit plus 1000 projectiles, whatever the unit count, rather than scaling with `with_units`. It still
   compiles and `test-client` passes. Followers and projectiles have no `UnitType`, so they are not
   published to the client.
4. **I fail the check when a counter has no budget.** This is stricter than the brief's wording, so that new
   counters cannot slip past the gate. The cost is a re-bless whenever a mechanic adds a counter.
5. **Environment: sccache served stale `fl_content` test binaries.** They had `CARGO_MANIFEST_DIR` baked in
   as `C:\fl\t17fix\...`, so `fl_content::load`/`objects` failed in the gate; my `Cargo.lock` change made the
   gate test every package. I fixed it locally by touching `crates/fl_content/tests/*.rs` and rebuilding them
   with `RUSTC_WRAPPER=` empty; no source change. Other worktrees and agents may hit the same problem.
   `fl_catalog::base_mod_dir()` uses `CARGO_MANIFEST_DIR` too; I checked that my perf binaries embed `C:\fl\a`
   paths.
6. **The alloc budgets are not monotonic in the unit count.** `alloc.bytes` is 351620 at 3000 units against
   75855 at 5000, which looks like a one-off buffer growth inside the 3000 run's window. The numbers are
   deterministic across runs, but they are spiky.

---

# Fix round 1 (commit `a4aad35`)

## Changes, by review item
1. **R60: `ScaleMix` is Hashed.**
   - It is registered as `Class::hashed(1)`, with Serialize/Deserialize/PartialEq.
   - `SimHash` writes the projectile count, the ttl, `sensor_every`, then the unit count and each `SimId`.
   - `tests/replay.rs` `scale_and_save_starts_verify` no longer uses the `projectiles: 0` workaround. It now
     runs 2 followers per unit, 12 projectiles, ttl 4 and sensor_every 5, so projectiles are in flight when
     the save is taken at tick 30.
   - New test: `fl_test_utils/tests/determinism.rs::scale_mix_branch`. It calls
     `assert_branch(ScaleScenario::with_units(300), 33, 80)` with the full mix.
   - RED: with `Class::External` temporarily restored, both `scale_and_save_starts_verify` and
     `scale_mix_branch` FAIL. GREEN: both pass with Hashed.
2. **Runner-independent alloc counters.**
   - A `static SERIAL: Mutex<()>` (poison-tolerant) is held for the whole of every test in `perf_counters.rs`,
     `budget_violation_names_counter` included.
   - The module docs and `gate()` state the assumption: the allocator counts the whole process; nextest runs
     each test in its own process; `cargo test` runs them as threads of one process.
3. **The `alloc.bytes` spike.**
   - I logged per-tick allocations at 3000 units. The spike is at tick 20: 281296 bytes, 92 reallocations,
     136576 net realloc bytes. Ticks 21–23 settle down: 24.7k, 16.8k, 12.8k. From about tick 24 on, each tick
     stays at roughly 11–14k bytes.
   - Cause: the first despawns arrive at one flight time (20 ticks). The projectile population peaks at
     1000 + 50 there, and the tables grow (1024 → 2048 rows).
   - Fix: `WARMUP` is now 40 ticks (two flight times), and the reason is documented on the constant and on
     `run_counter_scenario`.
   - The `alloc.bytes` doc now reads: bytes requested by allocations, plus the net growth of reallocations
     (requested minus freed, when positive).
   - After the fix, `alloc.bytes` budgets are monotonic in the unit count: 5155 / 18030 / 28430 for 500 /
     3000 / 5000 units, against 12435 / 351620 / 75855 before.
4. **R59:** I removed the `[env] SCCACHE_BASEDIRS = "C:/fl"` block from `.cargo/config.toml` and kept
   `rustc-wrapper`.
5. **R61: `fl_game` stress mode.** It now uses
   `ScaleScenario { seed: *seed, ..ScaleScenario::with_units(*units) }`. The docs on `StartMode::Stress` and
   `GameBuilder::stress` and the `--stress` doc in `main.rs` were updated to match.
6. **R62: `REPLAY_FORMAT_VERSION` = 2**, documented with the reason.
   - `FL_RECORD_GOLDEN=1` wrote `crates/fl_test_utils/tests/replays/testbed_wander.replay.new`.
     `testbed_wander_replay_verifies` passes, so a fresh recording verifies.
   - The blessed v1 `.replay` now fails in `golden_replays_verify` with
     `replay format version 1 is not supported (this build reads 2)` until the bless.
   - `replay_bytes_are_checked` had hard-coded 2 as "another version". It now uses
     `REPLAY_FORMAT_VERSION + 1`. This is test-guard finding 2: the asserted value changed, not the check.
7. **Minor items.**
   - The snapshots now record `Totals { entities, totals }`. For example, scale_500 ends with 1680 entities.
   - The units-only tests (`movement.rs` `scale_poses`, `spatial.rs` `scale_sim_cheats`) also set
     `sensor_every: 0`.
   - `record_budgets` documents that it merges into an existing `.new` and that the file must be deleted
     before a recording session for a clean result. I deleted it before recording.

## Re-recorded artifacts
I deleted the old `.new` files, then ran
`FL_RECORD_BUDGETS=1 cargo nextest run --profile agent -p fl_test_utils --test perf_counters -E 'test(/scale/)'`.
I ran the recording twice and the two `budgets.ron.new` files were byte-identical.
- `docs/perf/budgets.ron.new`
- `crates/fl_test_utils/tests/snapshots/perf_counters__scale_500.snap.new`
- `crates/fl_test_utils/tests/snapshots/perf_counters__scale_3000.snap.new`
- `crates/fl_test_utils/tests/snapshots/perf_counters__scale_5000.snap.new`
- `crates/fl_test_utils/tests/replays/testbed_wander.replay.new`

## Tests run
- `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_game --no-fail-fast`: 218/223 passed.
  The 5 failures were:
  - `replay_bytes_are_checked`, now fixed: the `--test replay` rerun passes 10/10;
  - `golden_replays_verify`, expected until the bless;
  - the three perf tests, expected until the bless.
- Gate: `cargo xtask gate task --track sim-core --track perf --track contract --track client` (`.gate-base` 8c7f0e5):
```
PASS fmt (1.5s)
PASS clippy-drift (1.7s)
PASS claude-md (0.0s)
PASS check-scope (0.2s)
PASS protected (0.2s)
WARN test-guard (1.6s): test-guard: 2 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/tests/feature.rs:153 RemovedAssert: assert_eq!(names(tables::RNG_STREAMS), ["debug.wander", "setup.scale"]);
    TEST-GUARD crates/fl_sim/tests/replay.rs:132 RemovedAssert: assert!(matches!( Replay::from_bytes(&other_version), Err(ReplayError::Version { found: 2 }) ));
PASS clippy (14.5s)
PASS arch-check (3.7s)
FAIL test-headless (167.3s)
        FAIL fl_test_utils::golden golden_replays_verify          (blessed v1 replay; v2 .new awaits bless)
        FAIL fl_test_utils::perf_counters smoke_counters_scale_500 (budgets.ron awaits bless)
        FAIL fl_test_utils::perf_counters counters_scale_3000      (budgets.ron awaits bless)
     Summary [64.514s] 455 tests run: 452 passed, 3 failed, 1 skipped
PASS test-client (129.2s)
PASS data-check (39.3s)
WARN gate task took 360s (budget 300s)
gate task: FAIL (test-headless) in 360s
```
All three failures are expected until the bless. The fl_content stale-binary failures from round 0 did not
come back.

## Bless command (lead)
`cargo xtask bless docs/perf/budgets.ron crates/fl_test_utils/tests/snapshots crates/fl_test_utils/tests/replays --reason "M0 counter baseline (entity mix); golden replay v2"`.
After the bless, the gate is expected to pass.
