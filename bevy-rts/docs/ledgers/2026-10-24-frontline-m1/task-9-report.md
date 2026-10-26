# Task 9 report — `core.rotate`, `core.follow`, Shift-queue, formation offsets

**Status:** DONE_WITH_CONCERNS (two interpretations of the brief, see Concerns). Commit `5294571`
on `m1/phase-a` (base `9c732d7`). Protected artifacts are regenerated as `.new` only, at the
commit (clean build id); nothing blessed.

## What was implemented

- **`commands.rs`** (contract track).
  - Shared Shift-queue helpers: `any_full` (any unit already at `MAX_WAYPOINTS` → the whole
    command is `core.queue_full`, checked before anything changes) and `give_order` (append with
    `queue`, else replace; returns whether the order is now current).
  - `core.move`: unchanged for one unit. More than one unit → each unit's target is its slot of
    `formation_targets(positions at command time, clamped target, Movement.formation_max_spread_m,
    map)`, queued moves too. Path request when the move is current, as before.
  - `core.rotate` (new, `unit_can` = `Mobility`): `facing = (toward − from).angle()`, the current
    heading if `toward == from`. `from` is the unit's position, or — queued only — the target of
    the last `Move` already in its orders (see Concern 1). Unqueued → orders `[Rotate]`, path
    `Idle` (a pending request turns stale); queued → appended (path set `Idle` if it became
    current).
  - `core.follow` (new): `unit_can` = `Mobility` and not the leader itself, so "the leader is
    removed from the units" happens in validation: a unit told to follow itself alone is
    `core.no_valid_units`. A leader that is missing or has no `Pose` →
    `RejectReason::Feature("core.invalid_target")` (R79), checked before the cap. Offset
    `pos − leader`, shortened to `follow_distance_m` when longer (direction kept). Unqueued →
    `[Follow]`, path `Idle`, so `advance_orders` plans in the same tick; queued → appended.
  - Registered `RotateCmd` and `FollowCmd`; module doc states the shared Shift-queue rules.
- **`orders.rs`.**
  - `pub fn formation_targets(positions, target, max_spread, map) -> Vec<FxVec2>` exactly as the
    brief's formula (centroid, offsets, `R`, scale by `max_spread / R` when `R > max_spread`,
    `map.clamp(target + offset_i)`); empty in → empty out. Re-exported from `lib.rs`.
  - Follow re-planning (`follow_needs_path`, Task 8 review Minor 7): `Idle` → request now;
    `Waiting` → never; otherwise only once `tick − planned ≥ follow_repath_ticks`, then
    `Following`/`Flow` when the goal drifted more than one map cell, `Done` when it drifted more
    than `arrive_m`, `Failed` always (see Concern 2). A missing leader still pops the order
    (`completed`) and the unit stops (`idle`).
  - `idle` is now `pub(crate)` (used by the rotate/follow handlers).
- **`path.rs`** (docs only, Task 8 review Minor 9): module docs, `enqueue_path` and
  `process_path_queue` state when a request is processed. `process_path_queue` is the only core
  system of `ThinkSet::Behaviour`; hooks are chained and each core slot runs before its feature
  slot, so requests from `Input`, `ThinkSet::Orders` (incl. `advance_orders`' follow re-plans) or
  a core system ordered `.before(process_path_queue)` are processed in their own tick; requests
  from feature `Behaviour` systems or later hooks in the next tick. A second exclusive core system
  in `Behaviour` would fail the build (error-level ambiguity check) until ordered. Pinned by
  `path_request_timing_follows_the_hook`, with and without a schedule shuffle. No system order
  change was needed (system-order snapshot unchanged).
- **`movement.rs`.** `MovementRules::link` range-checks the three group-order rules:
  `formation_max_spread_m ≥ 0`, `follow_distance_m ≥ 0`, `follow_repath_ticks ≥ 1`.
- **`fl_test_utils/src/builder.rs`.** `Scenario::world_mut()` (brief) and
  `ScenarioBuilder::queued_command(..)` (Shift-queued scheduled commands, for the twin/shuffle
  builder; `command` is unchanged).
- **`fl_sim/tests/feature.rs`** (outside the Files list, mechanical): the header's command table
  now lists `core.follow` and `core.rotate`.

## Tests (`crates/fl_sim/tests/orders.rs`, 15 tests)

All 11 brief tests, plus:
- `follow_replans_ended_paths_on_a_cadence` — Minor 7 carry: a stopped follower asks for nothing
  for 100 ticks; a `Failed` path is retried exactly at `planned + follow_repath_ticks`, not before;
  a stopped follower whose leader drives off re-plans at once, then at most once per cadence.
- `path_request_timing_follows_the_hook` — Minor 9 carry (above).
- `group_order_rules_are_range_checked` — the three new link checks.

Notes on the brief's tests: `follow_tracks_moving_leader` asserts total `core.path_request`s
(leader + follower) ≤ `ticks / 20 + 2` over 600 ticks and ≥ 3; `formation_spread_is_capped`
uses three units with a 200 m spread; `follow_self_or_missing_leader_rejected` compares the full
state hash with a twin whose commands name no units (same seqs taken), and checks `LastSeq` = 2;
`queued_orders_share_the_cap` fills 32 orders (move/rotate/follow), then a queued rotate and a
queued follow naming the full unit and a unit with room are both `core.queue_full` with both
units' orders and #3's path unchanged; an unqueued rotate still replaces the full queue.

### TDD evidence

RED (`formation_targets` as a stub returning the target for every unit, handlers not yet
registered): `cargo nextest run --profile agent -p fl_sim --test orders` →
`12 tests run: 1 passed, 11 failed`. Failures: `panicked at crates\fl_sim\src\commands.rs:206:17`
(command `RotateCmd`/`FollowCmd` "is not registered") for the rotate/follow/queue/twin tests, and
the formation assertions (`orders.rs:128`, `:162`, `:199`). Expected: neither handler nor the
formula existed. The one pass was `path_request_timing_follows_the_hook`, which pins existing
behaviour (documented, not changed, by this task).

GREEN: `cargo nextest run --profile agent -p fl_sim --test orders` →
`14 tests run: 14 passed` (before the rules-range test was added; 15/15 in the full run below).

## Full suite

`cargo nextest run --profile agent --no-fail-fast -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli`
at `5294571`: `400 tests run: 399 passed, 1 failed` — the only failure is
`fl_test_utils::golden golden_replays_verify` (committed golden diverges at tick 10; `.new` pending
bless). The earlier pre-commit run also failed `fl_sim::feature header_lists_features_mods_and_tables`
(command table now has `core.follow`/`core.rotate`); fixed in `tests/feature.rs`.
`orders.rs` 15/15; `cargo clippy -p fl_sim -p fl_test_utils --all-targets -- -D warnings` clean
(only the known clippy.toml `rand::*` config warnings).

Gate: `cargo xtask gate task --track contract,sim-core,perf` (contract: `commands.rs`; perf per R103):

```
PASS fmt (3.8s)
PASS clippy-drift (4.4s)
PASS claude-md (0.0s)
PASS check-scope (0.5s)
PASS protected (0.4s)
WARN test-guard (1.6s): test-guard: 1 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/tests/orders.rs:592 NewSeedOverride: .shuffle_seed(shuffle)
PASS clippy (41.1s)
PASS arch-check (3.8s)
FAIL test-headless (166.5s)   452 tests run: 451 passed, 1 failed, 2 skipped — golden_replays_verify
FAIL api (27.0s)              fl_sim: changed (formation_targets)
PASS data-check (12.2s)
gate task: FAIL (test-headless, api) in 263s
```

Both failures are bless-pending artifacts only. The test-guard finding is the deliberate
`shuffle_seed(Some(0x5EED_0001))` in `path_request_timing_follows_the_hook`, which checks the
timing holds under a schedule shuffle; please accept. Perf counter tests pass against the
committed budgets (no `budgets.ron.new`).

## Protected artifacts (`.new`, regenerated at `5294571`)

- `crates/fl_test_utils/tests/replays/testbed_wander.replay.new` — the testbed moves #1+#2 at
  tick 10 and #6+#7 at tick 60, so formation applies. Decoded old vs new: header equal except
  `build_id` (`c3f82c1` → `5294571`) and the `commands` table (adds `core.follow`, `core.rotate`;
  `core.move` 0 → 1, `debug.wander` 8 → 10 — recorded ids are remapped by name); start, interval,
  400 ticks, the 4 commands (payloads, units, seqs) identical; **hash stream equal for ticks 0–9,
  differs from tick 10 on (390 of 400)**. Control experiment: with formation disabled (every unit
  given the plain target), the committed golden replay verifies unchanged on this build — so the
  divergence is formation alone, and the command-id shift needs no re-record (R73/I1).
- `xtask/snapshots/api/fl_sim.txt.new` — one line added:
  `pub fn fl_sim::formation_targets(&[fl_math::vec::FxVec2], fl_math::vec::FxVec2, fl_math::fx::Fx, &fl_sim::MapInfo) -> alloc::vec::Vec<fl_math::vec::FxVec2>`.
- Unchanged: system-order snapshot (no new system), save-codec snapshots (no codec change),
  perf budgets and counter snapshots (scale mix issues no move/rotate/follow; perf tests pass).

Bless: `cargo xtask bless crates/fl_test_utils/tests/replays xtask/snapshots/api --reason "M1 Task 9: rotate, follow, formation offsets"`.

## Files changed

`crates/fl_sim/src/{commands,orders,path,movement,lib}.rs`, `crates/fl_sim/tests/{orders,feature}.rs`,
`crates/fl_test_utils/src/builder.rs`. `lib.rs` (one `pub use`) and `tests/feature.rs` (two table
names) are outside the Files list but are direct consequences of the brief's public
`formation_targets` and the two new command names.

## Concerns

1. **Queued rotate facing (interpretation).** The brief says `facing_i = (toward − pos_i).angle()`.
   Read literally (position at command time), a rotate Shift-queued after a move faces the point
   as seen from where the unit *started*, so `rotate_queued_after_move` ("arrives, then faces the
   point") would only hold for collinear points. I take `pos_i` as where the turn starts as far
   as the command can know: for a queued rotate, the target of the last `Move` already in the
   unit's orders, else its position. Unqueued rotates use the current position, as written. The
   test uses a non-collinear point (start (20,20), move to (100,20), face (100,100): north, not
   north-east). If the controller wants the literal reading, it is a two-line change in
   `RotateHandler::apply` plus the test's expected facing.
2. **Follow re-plan rule (interpretation).** The brief's rule "re-plan when the state is
   Idle/Done, or when tick − planned ≥ follow_repath_ticks and drift > one cell", read literally,
   re-plans a `Done` follower every tick or two while its leader stands still (plan → one-corner
   path → `Done` → plan …), which breaks the brief's own bound (requests ≤ ticks/20 + 2) and the
   carry ("re-request with a deterministic cadence/backoff, not every tick"). Implemented: `Idle`
   at once; `Done`/`Failed`/`Following` only on the cadence — `Following` when drift > one cell
   (as written), `Done` when drift > `arrive_m` (so a stopped follower closes the last gap: with
   a one-cell threshold a follower stopped behind a leader could end 8 + 4 + 0.5 m away, outside
   the test's `follow_distance + 4 m`), `Failed` always (terrain may have changed; one retry per
   cadence). Pinned by `follow_replans_ended_paths_on_a_cadence`.
3. **A queued follow never completes while its leader lives**, so orders queued after it wait
   until the leader is gone (or a new unqueued order replaces the queue). That is the brief's
   semantics ("a missing leader pops the order"); noting it for the client/UX (Task 16) and C7.
4. `ScenarioBuilder::queued_command` is a small addition beyond `Scenario::world_mut`, needed to
   put Shift-queued commands into the twin/shuffle builder (`command` hard-codes `queue: false`).

## Fix round 1

Commit `a8d9b41` (on `5294571`). Review: `task-9-review.md`; rulings R104–R108.

### Changes

- **R104 (Important 1): wire coordinates are clamped before any arithmetic.**
  - `core.rotate` now does `map.clamp(payload.toward)` before `toward − from`. It also needs a
    map, like `core.move`, and gives `core.no_valid_units` without one.
  - I checked the other handlers:
    - `core.move`: the target is clamped first, and formation runs on the clamped target.
    - `core.follow`: no Fx in the payload; leader and unit positions come from sim state.
    - `debug.spawn` and `debug.teleport`: the position is clamped first.
    - `debug.wander`: the radius is capped at the map's side before use. A negative radius
      removes wandering.
    - `debug.set_terrain`: u32 cells are validated against the grid before anything else.
    - `debug.set_hp`: `u32::min`.
    - `debug.set_owner`, `debug.invulnerable`: no numbers.
  - The `commands.rs` module docs state the rule.
- **Minor 1.** Added `branch_mid_follow` = `assert_branch(mixed_orders, 70, 300)`.
  `mixed_orders` gains #5 and #6, which follow each other from tick 3 (an A→B→A cycle);
  twin/shuffle cover it too.
- **Minor 4.** A follower exactly on its leader gets the offset
  `−from_angle(leader.heading) × follow_distance_m`, never zero.
- **Minor 6.** The module docs and the `StopHandler` docs say `core.stop` is never queued.
  `stop_with_queue_is_never_queued` pins it.
- **Minor 7.** A unit without a `Pose` still gets the rotate, journaled `command`. Its `from`
  falls back to the queued move's target, then `toward`, and its heading to `Angle::ZERO`.
  `advance_orders` completes it at once, as it already did for a Pose-less rotate.
- **R105/R106 docs.** The plan's Task 9 interface block and spec §7 (Movement) now state:
  - the queued-rotate facing;
  - the follow re-plan rule, including the on-leader offset;
  - the payload clamp rule and that stop is never queued.

  The spec text also covers formation slots and follow semantics.
- **R107, R108:** accepted; no change.

### TDD

RED: `cargo nextest run --profile agent --no-fail-fast -p fl_sim --test orders` →
`19 tests run: 16 passed, 3 failed`.
- `extreme_payload_values_never_panic`: `fixed-1.31.0 arith.rs:573 attempt to subtract with
  overflow`, the rotate overflow.
- `follower_on_its_leader_keeps_behind_it`: `left: [Follow { … offset: (0, 0) }]`.
- `rotate_without_pose_applies_and_completes`: `left: []` (no journal record).

These were the expected failures. `stop_with_queue_is_never_queued` and `branch_mid_follow`
passed at once: they pin behaviour that was already correct.

GREEN: same command → `19 tests run: 19 passed`.

### Verification

- Full suite: `cargo nextest run --profile agent --no-fail-fast -p fl_sim -p fl_test_utils
  -p fl_catalog -p fl_cli` → `405 tests run: 404 passed, 1 failed`. The failure is
  `golden_replays_verify`, which waits on the bless.
- Gate: `cargo xtask gate task --track contract,sim-core,perf,docs`. Only the two bless-pending
  rows fail; the one test-guard finding is the accepted `shuffle_seed`, now at line 618:

```
PASS fmt (3.2s)
PASS clippy-drift (1.7s)
PASS claude-md (0.0s)
PASS check-scope (0.2s)
PASS protected (0.2s)
WARN test-guard (0.8s): test-guard: 1 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/tests/orders.rs:618 NewSeedOverride: .shuffle_seed(shuffle)
PASS clippy (33.5s)
PASS arch-check (4.5s)
FAIL test-headless (134.7s)   457 tests run: 456 passed, 1 failed, 2 skipped — golden_replays_verify
FAIL api (25.8s)              fl_sim: changed (formation_targets)
PASS data-check (31.5s)
gate task: FAIL (test-headless, api) in 237s
```

### `.new` artifacts, regenerated at `a8d9b41`

- `xtask/snapshots/api/fl_sim.txt.new`: byte-identical to the round-0 file.
- `crates/fl_test_utils/tests/replays/testbed_wander.replay.new`: not byte-identical, but only
  because of the commit id. It has the same length (6477 bytes), and only 6 bytes differ: the
  header's `build_id` short SHA, `5294571` → `a8d9b41`. The hash stream and everything else are
  identical. The fix touches nothing the testbed runs: no rotate, no follow, no extreme values.

Bless (unchanged): `cargo xtask bless crates/fl_test_utils/tests/replays xtask/snapshots/api --reason "M1 Task 9: rotate, follow, formation offsets"`.
