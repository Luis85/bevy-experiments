# Task 11 review: steering and separation (4a92672, base 49e8a94)

### Spec Compliance

- ✅ Spec compliant. Every brief item and all four carried fixes are present and tested. The deviations are the six Concerns, which the lead rules on below.
  - `steer.rs` was created (220 lines), and `tests/steering.rs` was created with 15 tests. `movement.rs` and `lib.rs` were modified (`lib.rs:84`, `:142`).
  - `SteerPushes` is a Resource classified `Class::TransientReset(SteerPushes::clear)` as `"core.steer"` (`steer.rs:213-219`). It has `clear`, `get` (binary search, ZERO when the unit is absent), `len` and `is_empty`, plus an extra `entries()`. There is no per-tick component (R76), and the archetype check is at `tests/steering.rs:229`.
  - `compute_steering` runs in the core slot of `ThinkSet::Behaviour`, `.after(process_path_queue)`. It is the only writer, and the system-order `.new` confirms the placement.
  - Who is steered: moving units (`Following` or `Flow`) plus idle units where `id % every == tick % every` (`steer.rs:157-161`).
  - Neighbours come from `query_radius`, with `r = radius(u) + radius(v) + margin` (`steer.rs:174-186`).
  - Push: `(u−v)/d·(r−d)·gain`, with the ±x split by SimId for a coincident pair (`steer.rs:191-196`). The cap is at `:166` and `:199`. The per-`idle_every` cap is Concern 1.
  - Move order: whole → (extra: unpushed) → x → y → stay, with `move.blocked` counted (`movement.rs:330-356`, `:567`). The unpushed step is Concern 2.
  - Counters: `steer.units` (`steer.rs:204`). `steer.pairs` (`:205`) goes beyond the M1 counter list (constraints line 203) and is accepted like R100.
- Carried fixes, each checked against code and test:
  - **Corner cut / wall hop (Task 5).** `nav::step_clear` (`nav/los.rs:84-98`) runs the supercover `line_clear` from a passable start. I checked `los.rs:24-75`: an exact corner pass tests both side cells, and the boundary order uses exact i128 cross-multiplication. It applies to every candidate, pushes included (`movement.rs:546-551`). Tests: `step_never_cuts_a_blocked_corner` (`tests/steering.rs:341`) and `fast_unit_never_hops_a_one_cell_wall` (`:370`). Both are RED-proven ("tick 6: … cut the corner", "tick 0: … crossed the wall").
  - **Blocked-step backoff (Task 8 Minor 8).** A unit re-plans only once `tick − planned ≥ BLOCKED_REPLAN_TICKS` (`movement.rs:567-573`). `planned` is hashed (`path.rs:90`, `:180`) and is set by both the single and group apply paths (`path.rs:658`, `:678`). Test: `blocked_step_replans_with_backoff` (`:399`). RED: 88 requests in 110 ticks.
  - **Follower hysteresis (Task 9 Minor 5).** A `Done` follower re-plans only when the drift exceeds `max(arrive_m, cell/2)` (`orders.rs:325`). Test: `stopped_follower_ignores_small_leader_nudges` (`:443`), with a positive control at the end.
  - **LOS lookahead (R111).** `look_ahead` (`movement.rs:369-395`) walks `FlowCache::direction` for up to 4 cells, gated by `line_clear`. It is pure per R84. A field candidate must also end in a cell that has a direction (`movement.rs:553-560`). Tests: `flow_units_look_ahead_along_the_field` (RED: 133 turn-stops) and `flow_units_stay_on_the_field`.
- ⚠️ **Cannot verify from the diff.** The golden replay is binary. The report's claim that the command stream is unchanged and the hashes first depart at tick 284 is unverified. The controller should check it with `cargo nextest run -p fl_test_utils -E 'test(golden_replays_verify)'` after the bless.
- ⚠️ **Cannot verify the like-for-like perf delta.** The 9.17 ms at 3000 units is measured against M0's 4.43 ms, which is not the same base.

### Strengths

- **Determinism.** The pushes never depend on Entity order:
  - `bodies` and `steered` are sorted by SimId (`steer.rs:169-170`).
  - `query_radius` clears and sorts its output (checked at `spatial.rs:139-157`).
  - The neighbour lookup is a binary search, and there is no float anywhere.
  - Integer addition commutes, so the sum order does not matter.
- **No cross-tick state.** `SteerPushes` and the scratch buffers are TransientReset (`orders.rs:72-81`). The stagger depends only on the hashed `SimClock` tick and the SimId, and the backoff uses the hashed `planned`. So a save/load cannot change the outcome.
- **Spatial grid.** The grid is Derived, but query output is sorted, so its internal cell order after a rebuild on load does not matter.
- **One fit predicate for drive and push.** Pushes, slides and drive steps all go through the same `fits` check. A push can therefore never enter a blocked or off-map cell (`map.clamp`, which is `size − ε`, was checked at `map.rs:48-51`). It also cannot cross a diagonal whose side cell is blocked.
- **Allocation-free steering.** The steering scratch keeps its capacity. The bench shows the sorted `bodies` table took 2.38 → 1.87 ms at 3000 units.
- **Proptest teeth.** The proptests were shown to have teeth by short-circuiting `step_clear`. The content load was moved out of the per-case setup, which keeps the 1024-case run at 16 s.
- **Reached rule.** It is careful: a corner advances only when the drive's own step was taken (`movement.rs:578-580`), so a blocked push never stalls corner advancement.

### Issues

#### Critical (Must Fix)
None.

#### Important (Should Fix)
None.

#### Minor (Nice to Have)
1. **No steering branch test** (`tests/steering.rs`: no `assert_branch`). The design is stateless, and the existing branch tests now run with steering (`flow.rs:603/646`, `orders.rs:571`, `pathing.rs:618`, `terrain.rs:748`). But none of them branches with overlapping idle units at a tick that is not a multiple of `idle_every`. Add `assert_branch(crowd, 7, 300)` and an overlapping-idle scenario branched at tick 3.
2. **The property tests check only the end-of-tick cell** (`tests/steering.rs:136-165`). A corner cut or hop by a *push* is covered by the shared code path, but no test targets it. Strengthen `never_blocked_case`: each tick, assert that the public `line_clear(prev, cur)` holds for every unit that started on a passable cell.
3. **The pair term is antisymmetric only up to 1 raw ulp** (`steer.rs:195`). I32F32 multiplication floors, so `(−x)·s ≠ −(x·s)` in general. This does not affect determinism, and the caps differ per unit anyway. Document it rather than claiming exact symmetry. Exact symmetry would need the term computed once per pair and negated.
4. **The neighbour reach grows with the widest footprint present** (`steer.rs:151`, `:173`). That widest footprint includes units with no Mobility, so one large footprint (M4 buildings) widens every unit's query map-wide, and `steer.pairs` rises with it. Park this until structures exist: bucket by size, or query with `u.radius + margin` and file large bodies in every cell they cover.
5. **`move.blocked` now also counts idle units whose push alone is blocked** (`movement.rs:567`), for example a unit leaning on a wall in a crowd. That dilutes the counter as a path-blocking signal. Consider counting only `step_delta != 0`, or splitting out a `steer.blocked`.
6. **Slide oscillation has no progress check** (`movement.rs:567`). The backoff fires only on `Stayed`, so a unit that keeps sliding under a steady push without nearing its corner never re-plans. Not observed in the tests, and plausible only in cramped crowds. Park it for C7 crowd tuning: count a Slid step that brings the unit no nearer its target toward the backoff.
7. **Scratch coupling.** The steering scratch lives in `orders::OrderScratch`, and `orders.rs:39` imports `steer::Steered` (orders ↔ steer). A `SteerScratch` TransientReset resource owned by `steer.rs` would keep the module self-contained.
8. **`idle_every` has no upper bound** (`movement.rs:150`). An extreme value from content overflows `mul_int(ticks)` at `steer.rs:166` in debug builds. Bound it, for example to ≤ 600, or saturate.
9. **The SpatialGrid is one tick stale for debug teleports.** It is refiled in `Perceive/Spatial`, after `Act`. A unit teleported in `Input` is therefore missed by its new neighbours' queries for that one tick. This is deterministic and debug-only. Note it in the `compute_steering` doc.
10. **Bulldoze not pinned.** The rewrite of `tests/movement.rs:164` dodges the bulldozing instead of pinning it. Add a small test that documents a mover pushing an idle unit, so that a future change to it is deliberate.
11. **Pre-existing noise.** The report's test and clippy output has two pre-existing `clippy.toml` rand warnings, and `gate task` took 916 s against a 300 s budget. Neither comes from this task. Record both for gate hygiene.

### Test-guard findings (5)
- `movement.rs:543` ToleranceChanged: a false positive on non-test code (the R109 park). Accept.
- `tests/movement.rs:164` RemovedAssert: the same assert is kept with the target moved to `(5000, −5000)`, which still clamps on both axes. Justified by the bulldozing (Concern 3). Accept, with Minor 10.
- `tests/observe.rs:411-412` RemovedAssert: both asserts are kept in a stronger form. All orders have length 2, the test searches seeds 1–8 for a differing order (it panics if none differs), and it compares that seed too. Accept.
- `tests/steering.rs:701` NewAssume: a case that placed no unit has nothing to test. With `max_global_rejects = 4 × cases` and a placement rate of about 20% per unit, the rejection rate is small. Accept.
- Not flagged by test-guard, but reviewed:
  - `tests/orders.rs:727-734`: 5 → 8 ticks, then 95 → 92, keeping the 100-tick total. This follows from the hysteresis.
  - `tests/runner.rs:655-668`: the teleport targets are spread 12 m apart, which keeps the units of `some` untouched so that the record-copy path is still exercised.

  Both are justified.

### Perf
- **Counters are plausible.** At 3000 units, `steer.pairs` is 4,075,560 / 200 ticks ≈ 20.4k per tick, about 6.8 candidates per steered unit. Its budget is 26,213 (`ceil(max × 1.25)`). `steer.units` equals every unit, because every scale unit wanders. `move.los_cells` is about 6k per tick, roughly 2 cells per unit.
- **Small `path.*` and `spatial.*` drift.** Fewer requests come from the backoff and from the changed trajectories.
- **Allocation changes are within noise** (+2 counts at 5000, −3 at 3000). This is nothing like the +50 that R102 was about.
- **Bench.** At 3000 units the mean is 9.17 ms (compute_steering 1.87 ms, move_system 2.50 ms). That fits the G7 mean ≤ 15 ms, but p95 is 15.7 ms on a busy machine. Recommend a like-for-like release bench of 49e8a94 against 4a92672 (perf-analyst) before `gate pr`, so the jump from M0's 4.43 ms is attributed between Tasks 5–10 and Task 11.

### Recommended rulings
1. **Idle push cap per `idle_every`: accept for M1.**
   - The cap is stateless and deterministic, and it is the only reading under which the brief's own 40-tick test can pass. A per-tick cap gives at most 4 m against the 6.4 m needed.
   - The 1.25–2.5 m hops occur only on deep overlaps (spawn stacking, teleports). A light overlap's raw push `(r−d)·gain` is small.
   - Amend spec §7 to read "per tick the push stands for".
   - Park smoothing with a carry to C7 tuning. The options are a per-tick cap with hashed overlap state, or client-side interpolation of the hops.
2. **Extra unpushed attempt: accept.** It is strictly safer: separation can never stall a step that is clear on its own. Amend the plan and spec text to read whole → unpushed → x → y → stay.
3. **Movers bulldozing idle units: accept**, as the consequence of the spec'd values (a cap of 0.5 × speed is less than speed). Accept the four test rewrites. Park yielding and tuning to C7. Add Minor 10 (a pinning test) in the fix round.
4. **Stranded units: accept the backoff** (one request per 10 ticks). Park unsticking with a carry: it must be fixed before terrain can be destroyed in play (bridges in M7; today only `debug.set_terrain` strands units). The fix: allow the step from a blocked cell toward `nearest_passable`, or plan from the stranded cell itself.
5. **`move.blocked` unbudgeted: accept.** The counter is deterministic, and any change that makes it appear in the 200-tick window also regenerates the budgets. Carry to Task 13: one budgeted perf scenario where `move.blocked` and `steer.pairs` are measured, such as a choke-point crowd.
6. **Out-of-list edits: accept (R108 precedent).** `orders.rs`, `nav/los.rs` and `nav/mod.rs` are the carried fixes, and `path.rs` changes only docs. Registering steering through `movement::build`, which calls `steer::build` (`movement.rs:605`), is also accepted, because `feature.rs` is a contract path and the ordering is identical.

### Bless per `.new`
- `system_order__sim_tick.snap.new`: **bless**. One line, `compute_steering` after `process_path_queue`.
- `xtask/snapshots/api/fl_sim.txt.new`: **bless**. Adds `SteerPushes` and its derives plus `clear`, `entries`, `get`, `is_empty` and `len`; nothing is removed.
- `testbed_wander.replay.new`: **bless**. The hashes depart at tick 284, as expected from separation. Verify after the bless (⚠ above).
- `docs/perf/budgets.ron.new`: **bless**. It was recorded with `FL_RECORD_BUDGETS`, the new counters are plausible and the allocation changes are within noise.
- `perf_counters__scale_{500,3000,5000}.snap.new`: **bless**. The totals agree with the budgets.

### Assessment

**Task quality:** Approved

**Reasoning:** Steering is deterministic, as the Strengths section shows. Every candidate move goes through one swept-cell check, so neither a push nor a drive step can enter or cut past a blocked cell. All four carried fixes are present and have tests that fail without them. The remaining findings are tests to strengthen and items to park, and the six concerns need rulings, not code changes.
