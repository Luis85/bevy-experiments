# Final whole-branch review — M1 (control & movement)

Branch `m1/phase-a`, range `0ae615d` (tag `m0`) .. `ac7a864` (79 commits, 194 files, +32,955 / −910).
Reviewer: final whole-branch reviewer (read-only; no cargo run). Authority: spec (rev 2.4), ADR-0006/0007/0012/
0014/0015/0016 and refinements, M1 plan (Global Constraints, Review Focus, DoD from the M0 rev-2 plan), rulings
R69–R143 (M1 ledger) and R1–R80 (M0 ledger). Per-task reviews are taken as done; this review looks at what crosses
task boundaries and at what M2–M9 build on. Rulings are not re-litigated unless noted.

## Verdict: ready to build M2 on — yes, once the close-out item (I1) is done

No Critical findings. The determinism core of M1 is sound and I could not construct a desync. I checked:

- **Path queue and budget.** Every source enqueues in a fixed order. Charges are cold costs. Stale requests are
  dropped at charge 0. Group fallbacks re-enqueue in `SimId` order.
- **Derived caches.** HPA* is pure, and `invalidate_equals_full_rebuild` covers the in-place path. Flow reads never
  observe a miss. Eviction runs only at the tick boundary. `cold_cost` is `settled`, a pure function of the key.
- **Steering.** It reads start-of-tick positions. Bodies are sorted by `SimId` and `query_radius` sorts. The sums
  are exact integer additions, so order never matters.
- **Orders, follow and formations.** All walks are in `SimIndex` or `SimId` order, every tie-break is pinned, and
  the BTreeMaps are keyed by `MoveProfile`.
- **Wire `Fx`.** Every handler clamps or validates its input (R104). Map sides are capped at 4,096 m, so products
  of map-scale vectors stay inside I32F32.
- **Reload and load.** Both use the same `rebuild_derived` and `replan_if_changed` helper. A rebuild-load re-plans
  flow units by rules-hash difference.
- **Journal and debug-only code.** No `journal.enabled()` block and no `cfg!(debug_assertions)` block writes
  hashed state.

What M2 builds on is coherent:

- the closed `Order` enum, ready to be replaced under C7;
- remapping of command ids by name;
- `fill_missing` and `migrate_from`;
- the debug-query registry;
- the `#[non_exhaustive]` runner enums (apart from `FromRunner`, M4).

The Important items are about close-out, verification coverage and client state hygiene, not sim correctness.

| Severity | Count |
|---|---:|
| Critical | 0 |
| Important | 4 |
| Minor | 17 |

---

## Critical

None.

---

## Important

### I1. M1 close-out is incomplete: `gate milestone` is red, the perf baseline is unblessed, ADRs await ratification — **before M1 close**

- **Where:**
  - `docs/perf/M1.json.new` is tracked in git (`git ls-files`), and `docs/perf/M1.json` does not exist.
  - The `perf-file` row (`xtask/src/gate.rs:695`) therefore FAILs; Task 21 reports `gate milestone: FAIL (perf-file) in 910s`.
  - ADR-0014/0015/0016 and the 0007/0012 refinements say `status: "accepted"`, but they were accepted by the
    controller under R85's delegation ("listed in the final report for user ratification").
- **Failure scenario:** The DoD says "`cargo xtask gate milestone --milestone M1` is green", and today it isn't.
  - R143 makes `M1.json` the baseline for M2's 20 % rule. Without the bless, M2 Task 1's `bench --baseline
    docs/perf/M1.json` has nothing to compare against.
  - The stray tracked `.new` file can be mistaken for the baseline. It is not protected: `M*.json` does not match
    `M1.json.new`, so anyone can edit it freely.
  - The plan says only the user accepts ADRs (Global Constraints, "New ADRs … Only the user accepts them"). An M2
    plan built on unratified ADRs inherits rework risk.
- **Fix:**
  - The lead runs `cargo xtask bless docs/perf/M1.json.new --reason "M1 Task 21: idle-machine baseline (R128, R143)"`.
    The bless should leave no `.new` in the tree.
  - Re-run `gate milestone --milestone M1`; only `perf-file` should change.
  - List ADR-0014/15/16 and the 0007/0012 refinements for the user's ratification (R85) in the M1 final report.

### I2. No gate level runs any release-profile test, so the debug-vs-release split (the R104 class) is unguarded — **M2 prelude (first item)**

- **Where:** `xtask/src/gate.rs:62–145` (rows) and `:369–376` (`nextest_env`). Every `nextest` row uses
  `--profile agent` with the dev profile at task, pr, nightly and milestone level.
  - `Cargo.toml [profile.release]` has no `overflow-checks`. So `Fx` (`fixed::I32F32`) and integer overflow panic
    in debug but wrap in release, which the spec accepts ("Arithmetic is checked in debug builds", §5.4).
  - The journal defaults on in debug and off in release (`build.rs:181`).
  - The goldens (`testbed_wander.replay`, `testbed_paths.replay`) are recorded and verified only in debug builds.
- **Failure scenario:** A later feature computes something like `range * range` on a value a debug test never
  drives to overflow.
  - Debug and release then compute different hashed values. Peers on different build profiles, or a
    debug-recorded replay checked against a release `fl_cli replay-verify`, diverge.
  - Or a state write slips into an `if journal.enabled()` block. The gate can catch neither.
  - M1's only evidence that release equals debug is Task 21's manual run (54/55 passing, the one failure being the
    R139 journal test).
  - Combat (M2), with ballistics, penetration and dispersion, is exactly where this class of bug appears.
- **Fix:** Add a `release-determinism` row at nightly and milestone level:
  - `cargo nextest run --release --profile agent -p fl_sim -p fl_test_utils -E 'binary(golden) | binary(movement_determinism) | binary(determinism) | test(/twin|branch|round_trip|shuffle/)'`.
  - It needs R139's migration first (journal-reading tests set `RunConfig.journal = Some(true)`), or an exclusion
    list until then.
  - Also add one cross-profile check: record the testbed hash stream in debug and verify it with the release
    `fl_cli replay-verify`. That turns "debug ≡ release" into a gated property.
  - Consider `overflow-checks = true` for a `release-check` profile used only by this row: it turns silent wraps
    into failures.

### I3. Task-level gate runs no golden or catalog test for content-only changes — **M2 prelude**

- **Where:** `xtask/src/changed.rs:95–126`. `touched_packages` maps a file to the package whose directory holds it,
  and `assets/**` lies under no package.
  - `xtask/src/gate.rs:545–580`: at task level, `test-headless` runs the touched packages plus the `smoke_` tests of
    `fl_sim` and `fl_test_utils`.
  - `golden_replays_verify`, `testbed_paths_replay_verifies`, `base_content` and the `fl_catalog` tests are not
    `smoke_`.
- **Failure scenario:** This already happened once (ledger line 36: T20's `testbed_ai` scenario broke the
  `fl_catalog` count and the golden header, and only `gate pr` caught it).
  - The fix then was a dispatch note (Lesson 37), not a gate change.
  - M2 is content-heavy: weapon and shell kinds, `rules.ron` sections for combat. Every `rules.ron` edit changes
    the rules hash, the golden headers and often the hash stream.
  - A lane-2 task that edits only `assets/base/**` passes `gate task` green and breaks `gate pr` later, after merge
    and possibly after other tasks were stacked on it.
- **Fix:** In `touched_packages` (or a gate-only wrapper), treat `assets/base/**` and `scenarios/**` as touching
  `fl_catalog`, `fl_content` and `fl_test_utils`. Alternatively, rename the two golden-verify tests and the
  base-content count test `smoke_*` so the task level always runs them; both are cheap.

### I4. Client: overlay polls go stale after a match swap, and query ids are reused — **M2 prelude**

- **Where:**
  - `crates/fl_client/src/debug/mod.rs:369`: `replace_match` does `insert_resource(PendingReplies::default())`,
    which resets `next_id` to 0 (`link.rs:192–196`). It also resets `InspectorState.pending` (`:385`), but not
    `OverlayData`.
  - `crates/fl_client/src/overlays.rs:378` skips polling while `poll.pending.is_some()`.
  - `:419–424` takes the reply by id alone.
- **Failure scenario:**
  - F5 (paths) is on with a selection. A poll is in flight (`pending = Some(37)`) when `fl.load`, a console `load`
    or a hot-reload rebuild runs `replace_match`.
  - The old runner's reply never arrives. If the selection is unchanged (loading a save of the same match keeps
    the same `SimId`s), nothing abandons the poll. The overlay freezes on the previous match's paths and never
    polls again until it is toggled off and on.
  - Worse, ids restart at 0. The 38th query after the swap gets id 37, and its reply is stolen by the stale poll:
    an inspector query, another overlay's query, or a BRP `fl.debug_query` if `receive_overlay_replies` runs
    first.
  - The stolen reply fails to decode, which clears and warns. Its real owner then waits forever: the inspector and
    the overlays have no deadline, only BRP waits are bounded.
  - Focus item 5 asks exactly about "stale state after load/reload, unbounded waits".
- **Fix:**
  - Never reset the id counter: keep `next_id` monotonic across swaps (clear `replies` and `abandoned` only, or
    add a link generation to the id).
  - In `replace_match`, reset `OverlayData` (or abandon every pending poll) the way `InspectorState.pending` is
    reset.
  - Add a test: overlay active, a query in flight, `replace_match`, then the overlay re-polls within
    `POLL_FRAMES` and no other waiter loses its reply.

---

## Minor

Each item has a location, the failure, a fix and when to do it.

**M1. Value hot reload does not invalidate paths that the new nav data blocks** (M2 prelude or C7).
- **Where:** `crates/fl_sim/src/reload.rs:145–148`. Step 5 re-plans only units whose profile changed (and flow
  units when `cluster_cells` changed). `debug.set_terrain` runs `invalidate_paths` over the rebuilt clusters
  (`nav/mod.rs:219`).
- **Failure:** A reload that makes a `TerrainDef` block a class (forest blocks Wheeled, say) leaves `Following`
  units driving corners through now-blocked cells. They stall against `step_clear` for up to
  `BLOCKED_REPLAN_TICKS` (10) and then re-plan. This is deterministic but inconsistent with `set_terrain`.
- **Fix:** After `rebuild_derived`, when the nav content changed (terrain defs or `Movement` rules), run
  `invalidate_paths` over the whole map, or re-plan every `Following` unit.

**M2. `SteeringRules::link` validates only `idle_every`** (M2 prelude).
- **Where:** `crates/fl_sim/src/movement.rs:154–161`.
- **Failure:** A negative `max_push_frac` makes `cap` negative. Then `push.length() > cap` always holds and the push
  is reversed (`normalize × negative`). A negative `separation_gain` attracts units. An extreme
  `separation_margin_m` overflows `r * r`: a debug panic that kills the threaded runner, or a release wrap, so the
  R104 class comes back through content. `reload.rs:5` claims content is "range-checked in its link pass (R104)".
- **Fix:** Range-check `separation_gain` in [0, 4], `separation_margin_m` in [0, 50] m and `max_push_frac` in
  (0, 1]. Also cap `follow_distance_m` and `formation_max_spread_m` at the 4,096 m map side.

**M3. `FromRunner` is not `#[non_exhaustive]`** (M2 prelude, contract ADR).
- **Where:** `crates/fl_view/src/runner.rs:134–143`. `ToRunner`, `SimQuery`, `SimReply` and `RunnerControl` all are.
- **Failure:** The first new runner message (an M8 desync report, a late-join progress message) is a breaking
  change for every client `match`.
- **Fix:** Add `#[non_exhaustive]` with the C7/C8 contract batch, and park `ToRunner::reply_id()` (R123) with it.

**M4. Flow members fall off the field on the first blocked step once the group is 10 ticks old** (C7).
- **Where:** `crates/fl_sim/src/movement.rs:575–582` together with `path.rs:658`: `planned` is set when the group
  request is processed.
- **Failure:** Ten or more ticks after a group move, any member that is fully blocked for one tick goes `Idle` and
  asks for a single path, and it never rejoins the field (R130). In a choke, which is exactly where flow fields pay
  off, the group decays into N single requests through the same choke. `choke_crowd`'s `path.nodes_expanded` shows
  this fallback.
- **Fix:** For `Flow` units, apply the backoff from the last tick the unit moved, not from `planned`. Or let a
  blocked flow unit stay on the field (its next read gives the same direction) and fall back only after K
  consecutive blocked ticks. Fold it into the C7 crowd work with R115, R127 and R130.

**M5. The path queue is a single FIFO with no priority** (C7 design note).
- **Where:** `path.rs:469–553`.
- **Failure:** Player commands, follower re-plans, blocked re-plans and wander all share it. With M2 chase,
  attack-move and re-target re-plans at scale, a sustained backlog (`scale_5000` already averages `path.queue_len`
  16.8) delays a human's order by whole ticks behind background work.
- **Fix:** C7's order queue can add deterministic lanes, such as command-issued requests before re-plans, each
  FIFO. The charge model is unchanged.

**M6. A group request's cold cost is a whole-graph Dijkstra** (park with R98).
- **Where:** `nav/flow.rs:128–154`. `integrate` settles every reachable node of the profile's graph whatever the
  members' positions.
- **Failure:** At the spec's 2 km cap (500² cells, about 10k nodes) this is the `group_move` 10,555 per request,
  above the 8,000 budget. That is fine at G7's scale, but the cost grows with map area, not route length.
- **Fix:** If maps ever exceed 2 km, a resumable integration (stop once every member's start cluster is settled,
  continue on demand) keeps purity and bounds the cost per group. Same trigger as R98.

**M7. `core.paths` fails the whole query when any listed unit cannot move** (M4).
- **Where:** `path.rs:826–831`.
- **Failure:** The F5 overlay sends the whole selection. From M4 (guns, buildings) one immobile unit in the
  selection blanks the overlay, which logs "unit #N cannot move".
- **Fix:** Answer `{"state": "immobile"}` per unit instead of `Err`, or let the overlay filter.

**M8. `core.passability` dumps the whole map every 15 frames** (M2 prelude).
- **Where:** `nav/mod.rs:283–307` and `overlays.rs:25` (`POLL_FRAMES = 15`).
- **Failure:** While F6 is on, the full map is dumped on the sim thread every 15 frames, though the grid carries a
  `version`. On the 500² scale map that is 250k cells per poll, which delays ticks; on larger maps the hitch would
  be visible.
- **Fix:** Add an optional `"since_version"` arg answering `{"unchanged": true}`, or a bounded rect arg.

**M9. Selection and control groups survive a load into a different match** (M2 prelude).
- **Where:** `selection.rs:262–291`. `prune_selection` keeps every id still alive, without an ownership check;
  `replace_match` (`debug/mod.rs:338–390`) does not reset `Selection` or `ControlGroups`.
- **Failure:** After loading another scenario or save, `SimId`s name unrelated units. Rings may then draw on enemy
  units, orders are rejected `core.no_valid_units`, and Ctrl+digit recalls arbitrary own units. The same happens
  after `debug.set_owner` takes a selected unit away.
- **Fix:** In `replace_match`, clear both for `LoadRequest::Scenario` and `LoadRequest::Save`; a rebuild keeps them,
  as it keeps the match. Make `prune_selection` drop non-own ids.

**M10. Replay format coupled to the bench struct** (C8).
- **Where:** `replay.rs:26–37`. `ReplayStart::Scale(ScaleScenario)` embeds the bench scenario struct, so every mix
  field bumps `REPLAY_FORMAT_VERSION` (2 → 3 in M1).
- **Failure:** M2 adding combat to the mix will bump it again and invalidate every replay.
- **Fix:** Store the scale scenario as its content-side id plus a versioned params blob, or accept it under C8's
  versioning policy explicitly.

**M11. M1 old-save fixture for M2** (M2 prelude).
- **Where:** R83 says each milestone's integration task commits a fixture from the previous milestone tag.
- **Failure:** C7 changes `core.move_order` (v2 → v3) in the M2 prelude, before M2's integration task.
- **Fix:** Tag `m1`, generate `m1_testbed.save` (mid-path, with flow units and queued requests) from it before C7
  lands, and copy the v2 `Order` and `MoveOrder` types into a frozen `mod v2` for `migrate_from::<v2::MoveOrder>(2)`.

**M12. Path reset logic is duplicated five times** (M2 prelude cleanup).
- **Where:** The "path back to `Idle`, clear corners, `next = 0`" code exists in five copies: `orders.rs:167`
  (`idle`), `movement.rs:402` (`to_idle`), `commands.rs:845` (stop), `debug_cmds.rs:192` (teleport) and
  `path.rs:772` (`invalidate_paths`).
- **Failure:** C7 adds an activity stack. A copy that forgets a new field gives inconsistent state between paths.
- **Fix:** One `PathFollow::reset()`.

**M13. `enqueue_group_in` panics without a `PathQueue`; `enqueue_path_in` inserts one** (cleanup).
- **Where:** `path.rs:359–360` vs `:287–289`.
- **Failure:** Unreachable today (the resource is always initialized or filled), but the asymmetry invites a panic
  on the sim thread.
- **Fix:** Use the same helper in both.

**M14. Handlers reject "no map" as `core.no_valid_units`** (C7, codes are contract).
- **Where:** `commands.rs:626, 739`; `debug_cmds.rs:65, 168, 219`.
- **Failure:** "No map is set up" is reported as `core.no_valid_units`, which misleads a reader or an AI that keys
  on codes.
- **Fix:** Use `RejectReason::Feature("core.no_map")`, which needs no new variant (R71), with C7's codes.

**M15. Proptest regression files are not protected** (M2 prelude, with R140).
- **Where:** `crates/fl_sim/tests/hpa.proptest-regressions` (R136's committed seed) is outside `OWNERS.toml
  [protected]`, and `test-guard` doesn't look at it.
- **Failure:** An implementer can delete the regression seed silently.
- **Fix:** Add `**/*.proptest-regressions` to `[protected]`, or a test-guard rule for removed lines.

**M16. `test-guard` re-reports every accepted finding at `gate pr`** (M2 prelude).
- **Where:** 26 accepted findings at `gate pr` (ledger line 79). The row is Warn-only.
- **Failure:** The list grows each milestone, and a new weakened test drowns in already-accepted noise.
- **Fix:** A committed allowlist of accepted findings (file, test, kind, ruling id), so the row shows only new ones.

**M17. Gate time is over budget and the budget only warns** (M2 prelude; ledger line 76 "watch").
- **Where:** `gate task` reached 916 s against a 300 s budget, and `gate milestone` took 910 s. `TASK_BUDGET`
  (`gate.rs:150, 182`) only prints a WARN.
- **Failure:** The SubagentStop hook makes every task wait for the gate. At about 15 minutes per run, two builders
  and the R142 caps, M2's throughput is bound by the gate rather than by the work.
- **Fix:** Before M2:
  - Split `test-headless` at task level into touched-crate tests plus a fixed smoke set capped near 120 s.
  - Move the `scale_*` counter tests and `group_move` to the pr level.
  - Make the budget overrun a FAIL at nightly for trend tracking.

---

## Focus-area notes (no further findings)

### 1. Determinism across the M1 stack

- **Path queue.** Sources are in a fixed order: commands in `(player, seq)` order, groups before singles per
  command, then `advance_orders`, then `wander_system` in `SimIndex` order. Group fallbacks go to the back in
  `SimId` order. Charges are cold costs, and `spent >= budget` is checked after at least one request.
  - Group charge = `Entry::settled`, a pure function of `(graph, dest)`.
  - Tile builds and `direction` read-miss rebuilds are uncharged (R112) and observe-only (R86).
- **HPA\* and flow caches.** Both are pure, with no `RefCell`, statics or hidden caches (checked `nav/*`). In-place
  invalidation equals a full rebuild (proptest).
  - `FlowCache::begin_tick` is idempotent per tick. A fresh cache (`now = 0`) after a load or reload is correct.
  - A `debug.set_terrain` in `Input` clears per profile, before any read of the tick.
- **Steering (R76).** `SteerPushes` and `SteerScratch` are `TransientReset`, sorted by `SimId`.
  `SpatialGrid::query_radius` sorts. Each push is a sum of exact integer terms.
- **`move_system` iterates in archetype order, which is safe.** Every write is entity-local, flow reads are pure,
  and the pushes come from start-of-tick positions.
- **Orders, follow and formations.** `formation_targets` and `space_slots` use exact `Fx` and a BTreeMap bucket
  index, with ring order pinned. Follow re-plan is on cadence with hysteresis.
- **Reload and load.** Both run `rebuild_derived`. `replan_loaded_flow_units` keys on a rules-hash difference;
  the header's `rules_hash` is updated by the value reload, so a save after a reload loads without re-planning.
  The rebuild path's `drop_unknown_units` leaves only references that are filtered later:
  - group members (filtered in `process_group`),
  - stale requests (charge 0),
  - `Follow` leaders (the order completes).
- **Debug-only code.** `debug_assert!` and `cfg!(debug_assertions)` blocks only check (`check_tick`,
  `assert_requests_drained`, `assert_tick`, `plan_for`'s missing-graph assert). Every `journal.enabled()` block only
  records. The one remaining split is overflow semantics, which is I2.
- **Client paths.** The client-side journal dependency is R134's terrain view only (ruled). AI timing is
  frame-driven, but its commands are recorded, so replays are exact (ADR-0016).

### 2. Contract stability for M2+

- **Order enum (C7).** `Order` (R69) is replaced under C7 with codec v3 and `migrate_from` v2 (see M11). New
  payloads (`core.attack`, …) shift dense ids, which `remap_command_id` absorbs (R73). Client codecs are now pinned
  by `every_order_and_debug_command_has_a_codec`.
- **Reject codes and runner messages.** `RejectReason` covers M2 through `Feature` codes. `SimReply::Error(String)`
  has no structured code; acceptable for debug use, revisit for M8.
- **Hashed state that names a derived layout.** `PathState::Flow { key: FlowKey { ClusterId, MoveProfile } }` is
  Hashed state naming a layout that depends on `Path.cluster_cells` and `cell_m`. It is handled (the R129 re-plan),
  but every future cache key that leaks into Hashed state needs the same treatment. Note it in the C7 ADR.
- **Content kinds.** `MoveClass` is a closed 4-variant enum baked into `NavGrid`'s `[_; 4]` arrays and hash tags. A
  fifth class (amphibious, boats) is a format change; `trace_enum` catches it.
- **Bridges.** M7 needs the always-on terrain-change event (R134) and the unsticking fix (R116).

### 3. Performance architecture

- **The +55–63 % against `m0` (R143) is new work, not a design flaw.** Steering is +2.56 ms and movement +1.04 ms
  at 5,000 units; everything else is within 0.1 ms. Per-unit cost scales linearly.
- **Cheap steering wins later.** `compute_steering` gathers and sorts every footprint body (16.7k at 5,000) each tick
  and binary-searches per pair. Storing `(pos, radius)` in the spatial cells would remove both.
- **Serial core systems.** `move_system` holds `ResMut<FlowCache>` because reads build lazily, which keeps it
  serial. `process_path_queue` is exclusive. For M2's "executor parity before any multi-threaded executor", make
  `prepare_flow_tiles` build everything `move_system` will read (the look-ahead's next clusters included), so the
  mover can take `Res<FlowCache>`.
- **Headroom.**
  - At 3,000 units (G7): 4.6 ms mean, 3.2× headroom.
  - At 5,000 units: p95 is already 14.8 ms. Combat at 5,000 units will need that parallelism.
- **Allocations.** Per-request buffers (R98/R125) stay parked; `flat_3000` gates non-path allocations at M0 levels.
- **Hashing and saves.** Terrain hashing (xxh64 over types and heights) and 190 KB testbed saves are fine at the
  2 km cap.

### 4. Verification integrity

- **Proptests.** The fixed-seed policy (R140, R142) is applied in `movement_determinism`, `hpa`'s tripwire and
  `never_blocked`. The `proptest!`-macro tests in astar, flow, terrain and fl_content, and `steering.rs:873`'s
  `TestRunner::new`, still draw random seeds (R140 migrates them in the M2 prelude).
- **Assertions.** Tests without visible asserts all delegate to asserting helpers (`run_on_passable_cells`,
  `never_blocked`, `assert_branch`, insta snapshots); none is vacuous.
- **Shuffle runs.** They prove only that the setting changes nothing (R138, documented).
- **Gaps:** I2 (release) and I3 (content-only changes).

### 5. Client and game

- **Bounded waits.** `wait_for_reply` and `read_runner` waits are bounded. The lockstep refusal (`NetDriven`, R133)
  and the cheats checks sit on both the client and the sim side.
- **Hot reload.** It is debug-only and debounced, and a timed-out value reload is recorded as non-reproducible
  (a safe choice).
- **AI host.** It is carried across loads (R91).
- **Input.** `InputGate` covers the console, egui and the spawn panel for camera, selection, orders and overlay
  keys.
- **Gaps:** I4, M9.

### 6. Dead or duplicated code

- `MAX_COMMAND_UNITS` is duplicated in `fl_client` (parked to `fl_protocol` with C7).
- The path-reset logic is duplicated (M12).
- No dead public items of note.
