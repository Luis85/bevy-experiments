# Task 17 report: fl_test_utils (ScenarioBuilder, twin-run, shuffle runs, save fitness)

**Status:** DONE_WITH_CONCERNS (one contract file touched to satisfy ADR-0004; one test placed outside the brief's file; see Concerns)

## Commits (m0/phase-a, on 3735d01)
- `02a7439` fix(fl_sim): absent Hashed state leaves the state hash unchanged (Refs: ADR-0004)
- `70cfb83` fix(fl_sim): the scale scenario's seed must be the match seed (Refs: ADR-0003)
- `4501e75` test(fl_catalog): base testbed run checks poses and snapshot records
- `ccb40ab` feat(fl_test_utils): ScenarioBuilder, twin-run, shuffle runs, save round-trip and branch helpers (Refs: ADR-0005, ADR-0006)

## What I implemented
- `crates/fl_test_utils/src/lib.rs`: module docs and re-exports.
- `builder.rs`: `base_content()` (OnceLock; catalog features + base mod), `ScenarioBuilder` with the brief's full
  API (`new`, `with_content`, `feature`, `seed`, `cheats`, `hash_interval`, `shuffle_seed`, `scenario`, `map_flat`,
  `player`, `unit`, `scale`, `wander_all`, `command`, `build`), `Default`, and `Scenario` (`runner`, `driver`
  public; `run_ticks`, `tick`, `hashes`, `last_output`, `world`, `pose`, `issue`, `save` via `SimQuery::Save`
  with the driver's pending commands, `load` restoring the driver from `Loaded`).
  - Setup order: scenario or scale (both panics), flat map (`MapInfo::flat` 4 m cells + `rebuild_spatial`),
    players (ids after the scenario's), units (SimIds after the scenario's, call order), wander orders.
  - Seed: `.seed()` > scenario seed > scale seed > 0; with a scale scenario the override also places the units
    (ScaleScenario now requires seed == MatchSeed).
  - Scheduled commands (wander per player at tick 0, then `command(...)` in call order) are issued through the
    TestDriver before their tick (delay 0, so they apply in that tick).
  - `RunConfig` is built from `..RunConfig::default()` so fields later tasks add keep defaults.
- `determinism.rs`: `assert_deterministic` (panics `"twin run of N ticks: diverged at tick T: a=0x… b=0x…"`),
  `shuffle_seeds()` (`FL_SHUFFLE_SEEDS` 0..=3, default 1, first k of `0x5EED_0001..3`), `assert_shuffle_invariant`.
- `fitness.rs`: `assert_save_round_trip` (save → load with the same features/content/run config → save is byte
  identical; same tick and state hash) and `assert_branch` (A to m; B to n, save, load, B's remaining builder
  commands carried over, run to m; hashes of n..m equal A's, and A's and the resumed sim's saves at m are byte
  identical).

### Carried items
1. **T14 testbed run** (`fl_catalog/tests/base_content.rs`): runs via `SimRunner` + `TestDriver`; asserts ids 1..=10,
   live count 10, and per unit in the last snapshot (tick 99): unit type, owner, position, heading, flags 0, and
   that the sim's `Pose` equals the record. Replaces the TODO(Task 12).
2. **ScaleScenario seed**: `populate` no longer inserts `MatchSeed`; it returns
   `SetupError::SeedMismatch { scale, matched }` (world unchanged) unless the sim was built with `seed`. This
   keeps the `seed` field the plan needs (T18 `ReplayStart::Scale`, T29) and lane 2's T22 already builds with
   `MatchSetup.seed == scale.seed`. fl_sim tests updated to build with matching seeds (movement `sim_seeded`,
   `scale_hashes`, runner `seed: 7`, spatial `seed: 3`) + new test `scale_seed_must_be_the_match_seed`.
   `different_seeds_diverge` also asserts header seed == `MatchSeed`.
3. **No mutation after PublishSet::Snapshot**: `crates/fl_test_utils/tests/schedule.rs`
   `smoke_nothing_writes_published_state_after_the_snapshot` walks `SimTick` (`systems_with_access` zipped with
   `system_order`) for the catalog feature set and fails for any system after the first `Publish/Snapshot`
   system (except the `Publish/Sync` point) that writes Pose/Owner/UnitType/Health/MoveOrder/Invulnerable, is
   exclusive, or has deferred commands. `a_late_writer_is_found` proves the check fires (a feature writing Pose in
   `PublishSet::Events`).
4. **smoke_ set**: `smoke_testbed_twin_run`, `smoke_shuffle_invariant`, `smoke_save_round_trip`, `smoke_branch`
   (determinism.rs) and `smoke_nothing_writes_published_state_after_the_snapshot` (schedule.rs).

### Bug found and fixed (ADR-0004)
`golden_stability_extra_feature` failed: hashes diverged at tick 0. `compute_state_hash` wrote a `0` byte per
entity for every registered Hashed component the entity lacked (and name key + `0` for absent Hashed resources),
so a feature that only *registers* a type changed every existing match's hash, contrary to ADR-0004's
confirmation ("a dummy feature registering an early-sorting command, component and stream leaves an existing
scenario's hash stream unchanged"). Fix: absent state writes nothing; present state writes its name key then its
data (`ComponentCodec::hash`/`ResourceCodec::hash` take the key; `classify.rs` + `hash.rs`). No stored golden
hashes exist yet, so nothing needed re-blessing.

## Tests and results
- `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_test_utils`: 11/11 passed.
- `cargo nextest run --profile agent -p fl_sim -p fl_catalog -p fl_test_utils`: 176/176 passed.
- `smoke_shuffle_invariant` also asserts each shuffle seed really changes the executed system order (not vacuous);
  twin/command tests assert units actually moved.

## TDD evidence
- RED (fl_test_utils): `cargo nextest run --profile agent -p fl_test_utils` before implementation:
  `error[E0432]: unresolved imports fl_test_utils::ScenarioBuilder, …assert_branch, …assert_deterministic, …`,
  `error[E0433]: cannot find Scenario in fl_test_utils` (expected: the crate was empty).
- RED (golden): `golden_stability_extra_feature` before the hash fix:
  `the extra feature moved the hashes  left: Some(((Tick(0), 17351959138190176585), (Tick(0), 7548618270992664124)))  right: None`.
- RED (schedule check): first run flagged `Publish/Sync bevy_ecs::apply_deferred: takes the whole world` (label
  format is `"<label> <system>"`); fixed the sync-point skip.
- GREEN: as above (11/11, 176/176) and the gate.

## Gate
`cargo xtask gate task --track sim-core --track contract` (contract because of `classify.rs`):
```
PASS fmt (1.5s)
PASS clippy-drift (2.2s)
PASS claude-md (0.0s)
PASS check-scope (0.2s)
PASS protected (0.2s)
WARN test-guard (1.3s): test-guard: 22 findings (reviewer must accept each)
    TEST-GUARD crates/fl_catalog/tests/base_content.rs:29 RemovedAssert: assert_eq!(ids.len(), 10);
    TEST-GUARD crates/fl_catalog/tests/base_content.rs:39 RemovedAssert: assert!( app.world().resource::<SimIndex>().get(id).is_some(), "{id:?} alive" );
    TEST-GUARD crates/fl_sim/tests/movement.rs:399 NewSeedOverride: seed: 2,
    TEST-GUARD crates/fl_sim/tests/movement.rs:410 NewSeedOverride: assert_eq!(*world.resource::<MatchSeed>(), MatchSeed(1));
    TEST-GUARD crates/fl_test_utils/src/builder.rs:57 NewSeedOverride: shuffle_seed: Option<u64>,
    … (17 more NewSeedOverride lines in fl_test_utils: the shuffle_seed API itself)
    TEST-GUARD crates/fl_test_utils/tests/determinism.rs:198 NewShouldPanic: #[should_panic(expected = "diverged at tick")]
PASS clippy (6.2s)
PASS arch-check (5.0s)
PASS test-headless (32.4s)
SKIP test-client: no client package touched since 3735d01 (.gate-base)
PASS data-check (11.8s)
gate task: PASS in 62s
```
test-headless ran 176 tests (all of fl_sim, fl_catalog, fl_test_utils, including the 5 smoke_ tests).
Test-guard findings: the two RemovedAssert are replaced by stronger asserts (`ids == 1..=10`, live count 10 via
SimIndex::len); movement.rs:399/410 are the new seed-mismatch test; the rest are the shuffle-seed API the brief
asks for and the brief's `should_panic` test.

## Files changed
- Created: `crates/fl_test_utils/src/{builder,determinism,fitness}.rs`, `crates/fl_test_utils/tests/{determinism,schedule}.rs`,
  `crates/fl_sim/tests/golden.rs`
- Modified: `crates/fl_test_utils/src/lib.rs`, `crates/fl_sim/src/{classify,hash,scale,scenario}.rs`,
  `crates/fl_sim/tests/{movement,runner,spatial}.rs`, `crates/fl_catalog/tests/base_content.rs`

## Concerns
1. **Contract file touched**: `crates/fl_sim/src/classify.rs` (codec hash trait, pub(crate)) to make absent state
   hash to nothing, per ADR-0004's confirmation. It changes every state hash value (no stored hashes exist yet).
   Commit cites `Refs: ADR-0004`. Lead may want an ADR-0004/0005 refinement note recording the hash layout
   ("present state only: name key + data").
2. **golden_stability_extra_feature lives in `crates/fl_sim/tests/golden.rs`, on the core_mod duel + wander, not in
   fl_test_utils on base.testbed**: the dummy payload/component need `serde` derives and fl_test_utils has no
   `serde` dependency; adding it would touch `Cargo.toml`/`Cargo.lock` outside the Files list. If the lead prefers
   the brief's placement, add `serde.workspace = true` to fl_test_utils `[dev-dependencies]` (R50-style lock delta)
   and move the test.
3. New test file `crates/fl_test_utils/tests/schedule.rs` (not in the Files list) for carried item 3.
4. `SetupError` gained a variant (`SeedMismatch`); lane 2's T22 maps SetupError to strings and already passes
   matching seeds, so it is unaffected.

---

# Fix round 1 (review: Needs fixes)

**Commit:** `2d4221e` fix(fl_sim): prefix-free state hash counts; stricter publish guard (Refs: ADR-0004)

## Changes
1. **Publish guard** (`crates/fl_test_utils/tests/schedule.rs`):
   - Finds the publisher by system name (`…::publish_snapshot`) and asserts it is labelled `Publish/Snapshot`.
   - Asserts every `system_order` line ends with its zipped system's name.
   - Flags *every* Publish-phase system (everything after `Lifecycle/Sync`, except `Publish/Sync`) whose
     `has_deferred()` is true, wherever it sits. It also still flags systems after the publisher that are
     exclusive or write published components.
   - New negative case, `deferred_commands_before_the_publisher_are_found`. A feature adds a `Commands` system
     with `in_set(Phase::Publish).before(PublishSet::Snapshot)` through `app_mut()`; the test asserts that system
     is found and sits before the publisher. Deviation: the reviewer asked for it *in* `PublishSet::Snapshot`
     before the publisher. The hook's core slot is private and feature systems always run after it, so a feature
     cannot put a system inside the Snapshot hook ahead of the publisher. The Publish phase ahead of the hook is
     the nearest reachable position.
2. **Prefix-free hash** (`classify.rs`: `contains` added to both codec traits; `hash.rs`):
   - The hash now writes the count of present Hashed resources before them, and per entity the count of present
     Hashed components right after the `SimId`.
   - Absent state still contributes nothing.
   - New `presence_of_a_marker_changes_hash` (`fl_sim/tests/hash.rs`): inserting `Invulnerable` changes the hash,
     removing it restores the hash, and the marker on #1 vs #2 gives different hashes.
   - `golden_stability_extra_feature` stays green.
3. **`shuffle_seeds`** rejects 0: `FL_SHUFFLE_SEEDS must be 1, 2 or 3 (shuffled runs per check), not '0'`.
   I checked this by running `FL_SHUFFLE_SEEDS=0` on `smoke_shuffle_invariant`, which panicked with that message.
4. **ADR-0004** "Refinements (2026-10-23)" R54: describes the hash layout (name-keyed presence; counts for
   prefix-freedom), motivated by golden stability.

## Verification
The shared tree `C:\fl\a` did not build: Task 18's uncommitted `observe` module is mid-edit. I verified in a
throwaway worktree at `ccb40ab` that held only my six changed files. It used the same build dir
(`CARGO_BUILD_BUILD_DIR=C:/t/fl/c5/fdf61b108f2dc9`), so cargo's lock kept builds one at a time. I committed in
`C:\fl\a` with only my files staged. Task 18's files are untouched and still uncommitted. I removed the
throwaway worktree afterwards.
- `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_catalog`: 178/178 passed.
- `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_test_utils`: 12/12 passed.
- `cargo xtask gate task --track sim-core --track contract --base 3735d01`:
```
PASS fmt (1.9s)
PASS clippy-drift (2.4s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
WARN test-guard (1.5s): test-guard: 22 findings (reviewer must accept each)
    TEST-GUARD crates/fl_catalog/tests/base_content.rs:29 RemovedAssert: assert_eq!(ids.len(), 10);
    TEST-GUARD crates/fl_catalog/tests/base_content.rs:39 RemovedAssert: assert!( app.world().resource::<SimIndex>().get(id).is_some(), "{id:?} alive" );
    TEST-GUARD crates/fl_sim/tests/movement.rs:399 NewSeedOverride: seed: 2,
    TEST-GUARD crates/fl_sim/tests/movement.rs:410 NewSeedOverride: assert_eq!(*world.resource::<MatchSeed>(), MatchSeed(1));
    TEST-GUARD crates/fl_test_utils/src/builder.rs:{57,88,129,130,230} NewSeedOverride (shuffle_seed field/setter/RunConfig)
    TEST-GUARD crates/fl_test_utils/src/determinism.rs:{8,70,76,82,92,93,94} NewSeedOverride (SHUFFLE_SEEDS, shuffle_seeds, assert_shuffle_invariant)
    TEST-GUARD crates/fl_test_utils/src/lib.rs:17 NewSeedOverride (re-export)
    TEST-GUARD crates/fl_test_utils/tests/determinism.rs:{15,49,51,62} NewSeedOverride (shuffle tests)
    TEST-GUARD crates/fl_test_utils/tests/determinism.rs:198 NewShouldPanic: #[should_panic(expected = "diverged at tick")]
PASS clippy (39.5s)
PASS arch-check (5.9s)
PASS test-headless (147.5s)
SKIP test-client: no client package touched since 3735d01 (--base)
PASS data-check (72.2s)
gate task: PASS in 272s
```
I abbreviated the grouped lines above; the full list of 22 is in the first gate output in this report.

## Test-guard list vs the reviewer's description
The raw list does **not** fully match "shuffle/seed API + runner.rs/spatial.rs seed edits":
- No runner.rs or spatial.rs lines are flagged. Those seed edits produced no findings.
- Besides the shuffle/seed API (17 findings), the list has:
  - two `RemovedAssert` lines in `fl_catalog/tests/base_content.rs`. Stronger checks replace them: `ids == 1..=10`,
    a live count of 10 via `SimIndex::len`, and a per-record snapshot check.
  - `movement.rs:399/410`, from the new `scale_seed_must_be_the_match_seed` test.
  - one `NewShouldPanic`, for the brief's `divergence_is_reported`.
