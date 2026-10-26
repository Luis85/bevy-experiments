# Task 9 review: `core.rotate`, `core.follow`, Shift-queue, formation offsets

Commit 5294571 on m1/phase-a, base 9c732d7. I read the diff (`review-t9.diff`) in one pass. The API `.new` hunk is in that diff. I did not decode the golden replay `.new` (binary), so its hash claims come from the report.

### Spec Compliance

- ✅ The task meets the brief, with two interpretations that need rulings (R-a and R-b below).
  - `formation_targets` follows the brief's formula exactly, at orders.rs:318.
  - The move handler uses formation slots for groups larger than one, including queued moves, and takes positions at command time (commands.rs:621-632).
  - `core.rotate` requires Mobility. Unqueued, it gives `[Rotate]` and sets the path to Idle. Queued, it appends (commands.rs:653-697).
  - `core.follow` drops the leader from the units in `unit_can`. A leader that is missing or has no Pose is rejected with `core.invalid_target`, which goes through `INVALID_TARGET` (commands.rs:80) as R79 requires. The offset is capped at `follow_distance_m` (commands.rs:705-750).
  - All three kinds share the queue cap through `any_full`, checked before any change, as R77 requires (commands.rs:557).
  - `Scenario::world_mut` exists (builder.rs:503).
  - All 11 brief tests are present, plus 4 more.
- ⚠️ The golden `.new` is binary. Its "equal for ticks 0–9, diverges at 10" result and the "formation disabled → old golden verifies" control experiment come only from the report. The controller should keep the decoded comparison output with the bless commit.
- ⚠️ I could not check from this diff that formation slots get snapped to passable cells. My named-risk check of path.rs:69-72 found the following. `PathFollow.corners` ends at "the passable cell it snapped to", so snapping happens per profile at plan time (Tasks 7/8), and the formation slot only clamps to the map. `Order::Move.target` keeps the slot before snapping. This is acceptable, but see Minor 3.

### Strengths

- The Shift-queue logic is one shared copy, with no duplication. Moves, rotates and follows all go through `any_full` and `give_order` (commands.rs:557, :567). A rejected command changes nothing.
- Formation results do not depend on selection order:
  - `run_command` sorts and dedups the units, so they arrive in SimId order (commands.rs:523-524).
  - The centroid is an exact integer sum.
  - The radius is a `max`.
  - Slots pair with units by zip.
  - The math is Fx-only, with no float.
  - Every slot is within `max_spread` of a clamped target, and the map is at most 4096 m per side (fl_content builtin.rs:15). So `offset × spread` stays below about 3.4e7 and cannot overflow I32F32.
- The follow re-plan rule is bounded and documented (orders.rs:279-307):
  - Idle requests at once.
  - Waiting never requests.
  - Every other state requests at most once per `follow_repath_ticks`.
  - `PathFollow.goal` is the requested goal (path.rs:69), so a stationary leader gives zero drift and no requests. `follow_replans_ended_paths_on_a_cadence` pins this, including retrying Failed exactly on the cadence.
- Leader edge cases:
  - Leader is self: filtered in `unit_can`. Alone, the command is `no_valid_units`.
  - Missing leader: rejected.
  - Leader despawned or loses its Pose: the follow pops as `completed` and any queue behind it continues.
  - SimIds are never reused (ids.rs:61), so a stale leader id cannot resolve to a new unit.
  - Cycles (A→B→A): advance_unit does not recurse, and the goals converge (goal_A = B + capped(A−B)), so there is no loop or request storm.
- Hashing and saving: the Order variants were already in the hash (units.rs:388-405) and the save codec snapshot (`formats__save_codecs.snap:40`). The cadence state (`PathFollow.planned`) is hashed (path.rs:160). The task adds no new state.
- Request rate: rotating and idle units make no requests. `idle()` writes only when something changes (orders.rs:157). Followers make no allocations per tick.
- `follow_self_or_missing_leader_rejected` is a strong test: it compares the full state hash against a twin whose commands name no units, and asserts LastSeq = 2. `queued_orders_share_the_cap` checks that a unit that still has room is not changed when the whole command is rejected.
- The `MovementRules::link` range checks are added and tested (movement.rs:70-82, `group_order_rules_are_range_checked`).
- The path-timing documentation (Minor 9 carry) is pinned by a test, both with and without a schedule shuffle.

### Issues

#### Critical (Must Fix)

None.

#### Important (Should Fix)

1. **commands.rs:686: `core.rotate` subtracts an untrusted payload without clamping it.** The line is `(payload.toward - from).angle()`, and `toward` is any raw i64 from the wire.
   - Fx is I32F32. Its `-` (fx.rs:185-190) panics on overflow when debug assertions are on (dev/test/agent builds), so `toward.x = Fx::MIN` with any positive position panics inside `apply_commands`. That kills the threaded runner, the same failure class as the Task 2 carry "never panic, threaded runner dies".
   - In release builds the value wraps to a deterministic but wrong facing.
   - `core.move` clamps its target first (commands.rs:618). Rotate should do the same: `let toward = map.clamp(payload.toward)` when a map exists, or compute the difference with i128 or saturating arithmetic. Clamping changes facing only for off-map points.
   - Add a test with `RotateCmd { toward: FxVec2::new(Fx::MIN, Fx::MAX) }` that expects no panic and a valid rotate.
   - The golden and API artifacts are unaffected: the testbed issues no rotate, and no public API changes.

#### Minor (Nice to Have)

1. **Missing test (tests/orders.rs:543): no save/load branch with follow orders active.** The controller's brief asks for save/load round-trips mid-follow. No new state was added, but there is no test that a follower's cadence (`planned`, `goal`) and a mixed queue survive a branch. Add `assert_branch(mixed_orders, 70, 300)`; tick 70 is after the queued follow at tick 60. Also add a 2-unit A→B→A follow cycle to the twin/shuffle builder.
2. **Formation slots collapse for large spread-out groups (orders.rs:318, a consequence of the brief's formula).** The offsets are bounded: |slot − target| ≤ 40 m for any n. But scaling by `max_spread / R` squeezes 256 widely spread tanks into a 40 m disc, about 19.6 m² each, which is near or below a tank footprint. Clumps that are far apart also collapse onto nearly identical slots. This is bounded and deterministic, but arrival will crowd. Carry to Task 10/11: either a minimum slot spacing, or a cap that scales with √n × footprint.
3. **Queued rotate uses the slot before snapping (commands.rs:672-682).** The facing for a queued rotate comes from the last `Order::Move` target. When that target is in water or partial, the unit stops elsewhere (the shore or the closest reachable point), so the facing can be noticeably off. It also skips any `Follow` between the move and the rotate. See ruling R-a.
4. **Follower with offset ZERO (commands.rs:733).** A follower standing exactly at the leader's position gets offset ZERO, so its goal is the leader's centre and it drives into the leader. Task 11 separation may hide this, but a fallback offset would be more robust, for example `follow_distance_m` behind the leader's heading.
5. **Asymmetric re-plan thresholds can cause steady re-planning (orders.rs:300).** `Done` re-plans on drift > `arrive_m` (0.5 m), while `Following` needs drift > 1 cell. Once Task 11 separation nudges idle leaders, every stopped follower may re-plan every 20 ticks indefinitely: 256 followers would make about 12.8 requests per tick. This is bounded, but Task 11 should add hysteresis, for example a threshold of max(arrive_m, cell/2).
6. **Stop with `queue = true` is untested.** `core.stop` ignores `queue` (commands.rs:759-778), which is correct because stop is never queued, but no test pins it, and the module docs (commands.rs:18-22) list only move/rotate/follow under the Shift rules. Add one line to the docs and one assert.
7. **Rotate skips units with no Pose silently (commands.rs:670).** Such a unit gets no journal entry and no order, though it counts as applied. `move` falls back with `unwrap_or(target)` and `follow` with `map_or(ZERO)`. This is unreachable today (Mobility implies Pose), but the three handlers should behave the same way.
8. **The follow leader can be any owner.** Following an enemy is allowed and is the expected RTS behaviour. Noting it for M3 PlayerView: a client should only offer leaders it can see.
9. **Test-guard `NewSeedOverride` (orders.rs:592).** This is the deliberate shuffle seed in `path_request_timing_follows_the_hook`. Accept it.

### Recommended rulings

- **R-a (Concern 1, facing of a queued rotate): accept the implementer's reading.** For a queued rotate, `from` is the target of the last queued `Move`, falling back to the current position.
  - The brief's own test, "arrives, then faces the point", only holds under this reading.
  - Per-unit formation slots are already the stored targets, so groups work.
  - Carry to C7 (M2 order queue): `Order::Rotate` should store `toward` and resolve the facing when the order becomes current, from the actual position. That fixes Minor 3 and the skipped Follow.
  - Amend the plan text to match.
- **R-b (Concern 2, follow re-plan rule): accept the cadence version.** The rule:
  - Idle requests at once.
  - Waiting never requests.
  - Every other state waits until `tick − planned ≥ follow_repath_ticks`, and then: Following/Flow re-plan if drift > 1 cell, Done if drift > `arrive_m`, Failed always.

  Read literally, the brief would re-plan a Done follower almost every tick, which breaks its own `ticks/20 + 2` bound. Amend the plan and spec §7. Carry the Done-threshold hysteresis (Minor 5) to Task 11.
- **R-c (Concern 3, a queued follow never ends while its leader lives): accept for M1.** This matches common RTS follow semantics. A non-Shift order still replaces the queue, and the cap prevents unbounded growth. Carry to Task 16 (UX), which should show that orders queued behind a follow are blocked. Carry to C7 an optional "follow until" or ending condition.
- **R-d (out-of-list edits): accept all three.**
  - `lib.rs` `pub use orders::formation_targets`: the brief requires a public function.
  - `tests/feature.rs`: two command names, a mechanical consequence.
  - `ScenarioBuilder::queued_command`: needed to put Shift-queued orders into the twin/shuffle builder; small, and `command` is unchanged.

### Bless

- `xtask/snapshots/api/fl_sim.txt.new`: **bless.** It adds one line, `formation_targets`, exactly as the brief specifies.
- `crates/fl_test_utils/tests/replays/testbed_wander.replay.new`: **bless.** The divergence starts at tick 10, which is the testbed's two-unit move where formation now applies. Inputs and payloads are unchanged, command ids are remapped by name (R73/I1), and the control experiment with formation disabled reportedly verifies the old golden.
- The Important fix touches neither file. After the fix round, re-run the regeneration to confirm both `.new` files are byte-identical, then bless both with reason "M1 Task 9: rotate, follow, formation offsets".

### Assessment

**Task quality:** Needs fixes

**Reasoning:** The design is sound: queue semantics are shared, formation is deterministic and selection-order invariant, and the follow request rate is bounded. The one Important is that `core.rotate` does arithmetic on an unclamped wire value, which panics on overflow in dev builds and takes down the runner. It is a one-line clamp plus a test.
