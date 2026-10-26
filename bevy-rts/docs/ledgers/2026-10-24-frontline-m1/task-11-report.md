# Task 11 report: steering and separation

**Status:** DONE_WITH_CONCERNS (see Concerns). Commit `4a92672 feat(fl_sim): separation steering, axis sliding at blocked cells, staggered idle steering`, base `49e8a94` (.gate-base).

## What was implemented

### `crates/fl_sim/src/steer.rs` (new)
- **`SteerPushes`** (pub). It is a resource, Transient `"core.steer"`, `Class::TransientReset(SteerPushes::clear)`. It holds one `(SimId, FxVec2)` entry per steered unit, sorted by SimId. Methods: `clear`, `get` (binary search; ZERO for a unit not steered), `len`, `is_empty`, and **`entries()`**. `entries()` goes beyond the brief: the sortedness test needs to read the entries. Per R76 there is no per-tick component, and a test checks that archetypes stay the same.
- **`compute_steering`** runs in the core slot of `ThinkSet::Behaviour`, `.after(process_path_queue)`, and is the only writer of the pushes. It follows the brief:
  - **Who is steered:** units with Mobility + Footprint that are moving (`Following` or `Flow`), plus idle units where `sim_id % idle_every == tick % idle_every`.
  - **Neighbours:** `SpatialGrid::query_radius(pos, radius(u) + widest radius + margin)`, with `r = radius(u) + radius(v) + margin`.
  - **Push:** the sum of `(u−v)/d·(r−d)·gain`. A coincident pair pushes ±x by SimId. "Coincident" means d < 1/1024 m, which guards a debug overflow when dividing by a vanishing d.
  - **Inputs:** only start-of-tick positions are read. Bodies are gathered from the query and sorted by SimId. Neighbours are found by binary search, never by Entity order.
- **Counters:** `steer.units` and `steer.pairs`. `steer.pairs` counts the candidates the spatial index returned (the work).
- **Push cap (interpretation):** `max_push_frac × Mobility.speed` (road speed per tick) **per tick the push stands for**. A moving unit's push covers 1 tick. An idle unit's covers `idle_every` ticks, because it is steered only once every that many ticks. Without this, the brief's own test cannot pass with the brief's values. Two coincident tanks get 0.25 m per steered tick and 8 steered ticks each in 40 ticks, so at most 4 m < 6.4 m. With the amortised cap they reach 6.40 m by about tick 17.
- Scratch lists (`bodies`, `steered`, `near`) live in `OrderScratch` (TransientReset), so steering makes no per-tick allocations. Registration goes through `movement::build`, because `feature.rs` is a contract path.

### `movement.rs` (the step)
- **`take_step`** adds the push to every unit's step, whether it is driving or not. Candidates, in order:
  1. the whole move;
  2. **the step without the push** (an addition: a push never stalls a step that fits on its own);
  3. x alone;
  4. y alone;
  5. else stay and count `move.blocked`.
- **Swept check (Task 5 carry):** a candidate fits only if `nav::step_clear` passes. That function is new and `pub(crate)` in `los.rs`. From a passable cell it is `line_clear`, the supercover walk, where a pass exactly through a corner tests both side cells. This rules out diagonal corner cuts and hops over walls one cell thick. From a blocked cell (the terrain changed under the unit), only the end cell is checked, so a stranded unit can still leave.
- **Flow units (R111):**
  - A candidate that ends in another cell must have a `FlowCache::direction` there, so pushes never drop a unit off the field. This is a pure read.
  - **`look_ahead`** follows the `direction()` chain up to `FLOW_LOOKAHEAD_CELLS = 4` cells. It steers to the farthest cell centre the unit can see by `line_clear`, so units no longer turn in place at 45° bends. Outcomes do not depend on the cache, because direction() is pure.
- **Blocked-step backoff (Task 8 carry):** when a drive step is fully blocked, the path stays as it is while `tick − planned < BLOCKED_REPLAN_TICKS (10)`. The unit retries next tick. After that the path goes to `Idle` and the order re-requests. So a blocked unit makes at most one request per 10 ticks. This uses the existing hashed `planned` field: no codec change.
- **Reached rule:** a corner counts as reached only when the drive's own step was taken (whole or unpushed), or when the unit already stood on it. A push that is blocked never stalls corner advancement.
- **Counter:** `move.los_cells` covers the sweeps, the look-ahead and the arrival LOS. It is movement work, never path budget (Task 10 review). `move.blocked` is unchanged.

### `orders.rs` (Task 9 carry)
- A `Done` follower re-plans only when its goal has drifted more than **`max(arrive_m, cell/2)`**. This is the hysteresis: idle leaders nudged by separation by less than half a cell no longer cause re-plans every `follow_repath_ticks`.

### Docs
- Updated the module and function docs in `path.rs`: process_path_queue is now the *first* core system of Behaviour.

## Tests
New file `crates/fl_sim/tests/steering.rs`, 15 tests:

| Test | Covers |
|---|---|
| `overlapping_idle_units_separate` | brief |
| `separation_never_pushes_into_water` | brief (10 tanks on a 1-cell strip at varied y) |
| `units_never_enter_blocked_cells` (64 cases) | brief proptest |
| `milestone_units_never_enter_blocked_cells_long` (1024 cases, about 16 s) | brief proptest |
| `idle_steering_is_staggered` | brief: ≤ ceil(100/5) per tick, and the sum over 5·idle_every ticks = 500 |
| `steer_pushes_are_per_tick_and_sorted` | brief: entries ascend, len == steer.units, `get` agrees, empty on a no-steer tick, archetypes unchanged |
| `smoke_steering_twin_and_shuffle` | brief |
| `crowd_never_enters_blocked_cells` | crowd scenario |
| `step_never_cuts_a_blocked_corner` | Task 5 carry |
| `fast_unit_never_hops_a_one_cell_wall` | Task 5 carry |
| `blocked_step_replans_with_backoff` | Task 8 carry |
| `stopped_follower_ignores_small_leader_nudges` | Task 9 carry (plus a positive control) |
| `flow_units_stay_on_the_field` | R111: every tick every Flow unit's cell has a direction, no blocked cell, ≤ 2 requests per member |
| `flow_units_look_ahead_along_the_field` | R111: zero turn-in-place stops once under way |
| `base_steering_rules_are_the_brief_values` | brief values |

Proptest design: content is loaded once, and each case paints random terrain over a 20×20 ASCII map with `TerrainGrid::set_rect`, then runs `rebuild_spatial` + `rebuild_nav` + `setup_spawn`. Loading content per case put the 1024-case run at 170–185 s, near nextest's 180 s terminate.

The property tests have teeth. With `step_clear` short-circuited to `true` (temporarily, not committed), `units_never_enter_blocked_cells`, `crowd_never_enters_blocked_cells`, the milestone run and `separation_never_pushes_into_water` all fail (for example "tick 225: #2 at (6, 7)"). With the old end-cell-only rule they pass: standing on a passable cell was already guaranteed. Corner cuts and hops are what the two dedicated tests catch.

### Changed existing tests (outside the brief's Files list; each change follows from the new behaviour)
- `tests/movement.rs` `move_target_outside_map_is_clamped`: #1's straight drive to (1000, 1000) passes through idle #2 at (900, 900). Separation bulldozes #2 into the corner, and #1 stops 6.5 m short. The target is now (5000, −5000), which still clamps on both axes.
- `tests/observe.rs` `journal_identical_across_shuffle_seeds`: with one more system, seeds 1–3 order the two recording systems the same way. The test now searches seeds 1–8 for one that orders them differently, and compares it too. That is robust to future systems.
- `tests/runner.rs` `incremental_publish_equals_full_rebuild`: 72 units teleported onto one point would push apart for the rest of the run. Each now goes to its own point 12 m apart.
- `tests/orders.rs` `follow_replans_ended_paths_on_a_cadence`: with the hysteresis the follower sets off once the leader is more than 2 m away. The test checks after 8 ticks instead of 5, keeping 100 ticks in total.

### TDD evidence
- **RED:** `cargo nextest run --profile agent -p fl_sim --test steering -E 'not test(milestone_)'`, run with a stub `SteerPushes` registered and no behaviour. Result: 14 run, 5 passed, 9 failed:
  - stopped_follower ("the leader moved 0 m")
  - fast_unit ("tick 0: the rifleman crossed the wall to (4.5, 1)")
  - idle_staggered ("left: 0 right: 500")
  - corner ("tick 6: the tank cut the corner, left (1, 1)")
  - steer_pushes ("steered_some")
  - backoff ("88 path requests in 110 ticks")
  - overlapping ("0 m apart")
  - water ("spread 9 m")
  - look_ahead ("flow units stopped 133 unit-ticks to turn")

  Each failure was expected: no steering, end-cell-only checks, no backoff, no hysteresis, cell-centre flow steering. The tests that passed check invariants that already held (crowd/flow on passable cells, twin/shuffle, base rules).
- **GREEN:** the same command gave 14/14 passing. Then `cargo nextest run -p fl_sim --test steering`: 15/15, milestone in 16.3 s.

### Full run (required)
`FL_RECORD_BUDGETS=1 FL_RECORD_GOLDEN=1 cargo nextest run --profile agent --no-fail-fast -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli`: **441 run, 436 passed, 5 failed.** All five are protected artifacts pending bless:
- `system_order smoke_sim_tick_system_order`
- `golden golden_replays_verify`
- `perf_counters smoke_counters_scale_500`, `counters_scale_3000` and `milestone_counters_scale_5000`

`cargo clippy -p fl_sim --all-targets -- -D warnings` is clean; the two `clippy.toml` "rand::rng / rand::random" warnings were there before. `cargo fmt --check` is clean.

## Protected artifacts (.new, untracked, for the lead to bless: "M1 Task 11: steering and separation")
1. **`system_order__sim_tick.snap.new`:** one line added, `Think/Behaviour fl_sim::steer::compute_steering`, right after `process_path_queue`.
2. **`xtask/snapshots/api/fl_sim.txt.new`:** `fl_sim::SteerPushes` and its derives, plus `clear`, `entries`, `get`, `is_empty` and `len`. Nothing removed. `step_clear`, `BLOCKED_REPLAN_TICKS` and `FLOW_LOOKAHEAD_CELLS` are crate-private.
3. **`testbed_wander.replay.new`:** the command stream is unchanged. The hash stream first departs at **tick 284** (expected 0x3bba…, actual 0x3960…), when wandering testbed units first come within pair distance and separation moves them. The header changes because of the build.
4. **`docs/perf/budgets.ron.new`**, recorded with `FL_RECORD_BUDGETS=1` from a fresh file. Budget = ceil(max × 1.25).
   - **New budgets:**
     - 500: `steer.units` 625, `steer.pairs` 1750, `move.los_cells` 1355.
     - 3000: 3750 / 26213 / 8070.
     - 5000: 6250 / 64292 / 13445.

     `steer.units` = every unit, since every scale unit wanders, so it is always moving.
   - **`path.*`:** budget_spent, los_cells and requests went down a little, for example at 3000: 1564→1560 and 29→27. Blocked steps no longer re-request every tick, and separation changes the wander trajectories.
   - **Spatial:** query_candidates and relocations moved by under 3%, because trajectories changed.
   - **Allocations:**
     - 3000: count 72→69, bytes 20155→19030.
     - 500: count 78→79, bytes 6435→6515.
     - 5000: count 85→87, bytes 29820→30770.

     Steering's scratch is kept capacity, so these are trajectory effects on path corners and requests, not per-tick steering allocations.
5. **`perf_counters__scale_{500,3000,5000}.snap.new`:** new totals `steer.units`, `steer.pairs` and `move.los_cells` (at 3000: 600000, 4075560 and 1211962). The `path.*` and `spatial.*` totals shift, with the same causes as in the budgets.

No codec changed. Backoff uses the existing `PathFollow.planned` field, so `formats__save_codecs` is unchanged.

## Perf (G7)
`cargo run -p fl_cli --release -- bench --units 500,3000,5000 --ticks 400 --repeat 3 --profile` (median of 3, profiler on). The machine was busy, so the numbers are noisy.

| units | mean ms | p95 ms | compute_steering ms | move_system ms |
|---:|---:|---:|---:|---:|
| 500 | 0.90 | 1.82 | 0.10 | 0.27 |
| 3000 | **9.17** | 15.7 | 1.87 | 2.50 |
| 5000 | 18.07 | 28.3 | 4.32 | 4.35 |

**3000 units fit G7** (gate check: mean ≤ 15 ms). My first version looked neighbours up through `SimIndex` and `Query::get`. It cost 2.38 ms at 3000 and 7.55 ms at 5000. The sorted `bodies` table with binary search brought that down to 1.87 and 4.32 ms. I did not bench the base commit, so the M0.json mean of 4.43 ms at 3000 is not a like-for-like baseline: Tasks 5–10 changed movement and pathing in between.

## Gate
`cargo xtask gate task --track sim-core --track perf` at `4a92672`, with the .new files present and untracked. Result: `gate task: FAIL (check-scope, test-headless, api) in 916s`. Every failure is a protected artifact pending bless:

```
PASS fmt (19.1s)
PASS clippy-drift (16.8s)
PASS claude-md (0.0s)
FAIL check-scope (0.9s)
    1 of 19 changed files are outside track(s) sim-core, perf:
      xtask/snapshots/api/fl_sim.txt.new → contract
PASS protected (0.7s)
WARN test-guard (5.7s): test-guard: 5 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/src/movement.rs:543 ToleranceChanged: let step_delta = step.map_or(FxVec2::ZERO, |(to, _)| to - from);
    TEST-GUARD crates/fl_sim/tests/movement.rs:164 RemovedAssert: assert_eq!( pos, map.clamp(v(5000, 5000)), "arrived at the clamped target" );
    TEST-GUARD crates/fl_sim/tests/observe.rs:411 RemovedAssert: assert!(orders.iter().all(|order| order.len() == 2), "{orders:?}");
    TEST-GUARD crates/fl_sim/tests/observe.rs:412 RemovedAssert: assert!( orders.iter().any(|order| order != &orders[0]), "seeds 1-3 must order the recording systems differently: {orders:?}" );
    TEST-GUARD crates/fl_sim/tests/steering.rs:701 NewAssume: prop_assume!(!placed.is_empty());
PASS clippy (327.7s)
PASS arch-check (16.7s)
FAIL test-headless (368.3s)   492 run: 488 passed, 4 failed, 3 skipped
    fl_test_utils::system_order smoke_sim_tick_system_order      (snapshot .new)
    fl_test_utils::golden golden_replays_verify                  (replay .new)
    fl_test_utils::perf_counters smoke_counters_scale_500        (steer.units/steer.pairs/move.los_cells have no budget yet)
    fl_test_utils::perf_counters counters_scale_3000             (same)
SKIP test-client: no client package touched since 49e8a94 (.gate-base)
FAIL api (86.1s)   fl_sim: changed (fl_sim.txt.new written)
PASS data-check (73.0s)
WARN gate task took 916s (budget 300s)
gate task: FAIL (check-scope, test-headless, api) in 916s
```

check-scope flags only the untracked `fl_sim.txt.new`, which is the API artifact the lead blesses. After the bless, all three should pass.

Test-guard findings, for the reviewer:
- `movement.rs:543` ToleranceChanged is a false positive. It flags non-test code: `step_delta` in move_system (see R109's park note).
- `tests/movement.rs:164` RemovedAssert: the same assert is kept with the target changed to (5000, −5000), for the separation reason given above.
- `tests/observe.rs:411–412` RemovedAssert: both asserts are kept in a stronger form. All orders have length 2, and some seed in 1–8 must order the systems differently, else the test panics. The differing seed is then compared as well.
- `tests/steering.rs:701` NewAssume: a random case whose every unit landed on a blocked cell has nothing to test. `max_global_rejects` is 4 × cases.

## Concerns
1. **Idle push amortised over `idle_every`.** This is my reading of "capped at max_push_frac × speed per tick", needed for the brief's own 40-tick test. Side effect: an overlapping idle unit jumps up to `idle_every × max_push_frac × speed` in one tick, for example 1.25 m for a tank or 2.5 m for a truck. Under the 20 Hz interpolation that will look like a hop. The alternative is to steer overlapping idle units every tick, which needs hashed state (a flag) or a different stagger rule. That is the lead's call.
2. **Extra candidate "step without push" before x/y sliding.** The brief lists whole → x → y → stay. I added the unpushed step second, so separation can never stall a step that is itself clear.
3. **A moving unit bulldozes idle units** whose push is weaker than its step: cap 0.5 × speed < speed. This is what the spec'd values give, and it is why `move_target_outside_map_is_clamped` needed a new target. It may surprise in play. Tuning, or yielding, is C7 or later.
4. **Stranded units** (terrain set to water under them) still cannot leave across a blocked ring. They now re-plan once per 10 ticks instead of every tick; `blocked_step_replans_with_backoff` uses exactly this case. Unsticking them, for example by allowing a step through blocked cells to the snapped point, is not done.
5. **`move.blocked`** shows up in the 5000 bench (0.085 per tick over 400 ticks), but not in the 200-tick perf-counter window, so it has no budget. If a future change makes it appear in that window, `check_budgets` fails with "has no budget". This was already true before this task.
6. **Out-of-list edits:** `orders.rs`, `path.rs` (docs), `nav/los.rs` and `nav/mod.rs` (step_clear), and the four existing test files above. `movement.rs` is now about 600 lines.

---

# Fix round 1 (commit `ea5acbf`, on top of `4a92672`)

## Changes
- **Minor 1, steering branch test.** `branch_mid_steering` in `tests/steering.rs` runs two branches:
  - `assert_branch(stacked_idle, 3, 120)`: 5 tanks stacked on one point and 3 on another, all idle and still pushing apart at tick 3. Tick 3 is not a multiple of `idle_every` (5).
  - `assert_branch(crowd, 7, 300)`.
- **Minor 2, swept-cell assertion.**
  - New helpers `mobile_positions` and `swept_blocked_cells`.
  - Every tick, for every unit with Mobility that started the tick on a passable cell, the public `nav::line_clear(prev, cur)` must hold.
  - The check runs in `never_blocked_case`, which covers the 64-case and 1024-case proptests, and in `run_on_passable_cells` (crowd, strip and bulldoze tests).
- **Minor 3, antisymmetry note.** The `compute_steering` doc now says a pair's two terms are opposite only to within 1 raw ulp, because `Fx` multiplication floors. Outcomes are deterministic but not exactly symmetric.
- **Minor 7, steering's own scratch.**
  - `steer::SteerScratch` (`pub(crate)`) is a Transient `"core.steer_scratch"` resource, `Class::TransientReset(SteerScratch::clear)`. It holds `bodies`, `steered` and `near`.
  - `OrderScratch` is back to its pre-Task-11 fields and docs. `orders.rs` no longer imports `steer`, and `Steered` is private to `steer.rs`.
- **Minor 8, `idle_every` range.**
  - `SteeringRules::link` rejects `idle_every` outside `1..=64` (`MAX_IDLE_EVERY`), with the message "<v> must lie in 1..=64".
  - Test: in `tests/terrain.rs` `path_rules_ranges_are_validated`, 0 and 65 give "Steering/idle_every … 1..=64", and 64 loads. This existing test now expects the new message instead of "at least 1".
- **Minor 9, stale-grid note.** The `compute_steering` doc notes that the spatial grid is one tick stale after a `debug.teleport` in `Input` (deterministic, debug-only).
- **Minor 10, bulldoze pin (R115).** `mover_pushes_an_idle_unit_aside`: a mover drives 120 m past an idle tank standing 1.5 m off its line. The test asserts:
  - the idle tank is pushed north by more than 1 m and stays `Idle`;
  - the mover reaches its goal within `arrive_m`;
  - no unit stands on or sweeps a blocked cell.
- **Spec §7 and plan amended.**
  - Spec: the "Local separation and steering" bullet now states the pair distance, the 1..=64 stagger, the idle push cap per tick the push stands for (R113), the slide order whole → unpushed → x → y → stay (R114), the swept-cell rule, and bulldozing (R115).
  - Spec: the follow bullet's stopped threshold is now `max(arrive_m, cell / 2)`.
  - Plan, Task 11 brief text: amended for R113 (with 1..=64) and R114.

## TDD
- **RED:** `cargo nextest run --profile agent --no-fail-fast -p fl_sim --test steering --test terrain` gave 44 run, 43 passed, 1 failed. The failure was `terrain path_rules_ranges_are_validated`: `idle: "1..=64" not in content failed to load: …`, which was expected because the link only checked ≥ 1.
- Minors 1, 2 and 10 passed at once. They pin behaviour the first commit already had: stateless steering, one swept-cell predicate for every candidate, bulldozing.
- Teeth of the Minor 2 check: with `step_clear` short-circuited, the earlier run already failed the end-cell assertion, and the swept assertion is strictly stronger.
- **GREEN:** the same command passed after the link change.

## Full run
`FL_RECORD_BUDGETS=1 FL_RECORD_GOLDEN=1 cargo nextest run --profile agent --no-fail-fast -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli` at `ea5acbf` gave 443 run, 438 passed, 5 failed. The 5 failures are the same bless-pending artifacts as before: system_order, golden, and perf_counters at 500, 3000 and 5000.

## Gate
`cargo xtask gate task --track contract,sim-core,perf,docs` at `ea5acbf`:
```
PASS fmt, clippy-drift, claude-md, check-scope, protected, clippy, arch-check, data-check
WARN test-guard: the same 5 findings as round 0, all accepted in the review (line numbers moved: movement.rs:551, steering.rs:786)
FAIL test-headless: 494 run, 490 passed, 4 failed (system_order, golden, perf_counters 500/3000), all bless-pending
SKIP test-client
FAIL api: fl_sim changed (fl_sim.txt.new)
WARN gate task took 376s (budget 300s)
gate task: FAIL (test-headless, api) in 376s
```

## `.new` regenerated at `ea5acbf`
Each new file was compared with the copy from `4a92672`:

| File | Compared with 4a92672 |
|---|---|
| `docs/perf/budgets.ron.new` | byte-identical |
| `perf_counters__scale_{500,3000,5000}.snap.new` | byte-identical |
| `system_order__sim_tick.snap.new` | byte-identical |
| `xtask/snapshots/api/fl_sim.txt.new` | byte-identical |
| `testbed_wander.replay.new` | **bytes differ**, hash stream does not |

`SteerScratch` is `pub(crate)`, so the API `.new` did not move.

On the replay: the header carries the build id. I checked the hash stream by placing the round-0 `.new` as `testbed_wander.replay` (the original was restored afterwards and is unchanged in git), with the round-1 `.new` beside it. Both verify against the `ea5acbf` build, so the command and hash streams are unchanged and the hashes still first depart from the blessed golden at tick 284.
