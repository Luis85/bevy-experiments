# M1 final fix wave — report (lane 1, `C:\fl\a`, branch `m1/phase-a`)

Status: **DONE_WITH_CONCERNS** (concerns are about process and wording, not correctness; see the end).
Gate base: `ac7a864` (`.gate-base`). Commits `61cc357..e932f56` (7 commits):

| commit | item | subject |
|---|---|---|
| `61cc357` | 1 (I4), 2 (M9) | fix(fl_client): forget in-flight polls on a match swap, reset selection on load |
| `454a791` | 3 (M2) | fix(fl_sim): range-check the Steering rules and cap group distances |
| `c969dfd` | 4 (I3) | feat(xtask): gate task runs the catalog and golden tests on content changes (`Refs: ADR-0009`) |
| `0a1b2d4` | 5 (M15) | chore(owners): protect proptest regression files (`Refs: ADR-0009, ADR-0011`), **own commit** |
| `777b33c` | 7 | test(fl_game): round-trip the M1 payloads through fl.issue_command |
| `6e01933` | 6 | docs: M1 fl.entity and fl.journal_since output, perf wording, README tools |
| `e932f56` | 1 | docs(fl_client): reflow the replace_match doc comment |

`docs/perf/M1.json.new` was not touched. No protected file changed. No `.new` file was produced (no
snapshot, API, golden or budget change: `api` and `protected` rows PASS).

## What changed

### 1. I4 — overlay polls after a match swap (`crates/fl_client/src/{link.rs,overlays.rs,debug/mod.rs}`)
- `PendingReplies::forget_runner()` clears `replies` and `abandoned` but keeps `next_id`: query ids are
  monotonic across runners, so a post-swap query can never share an id with a stale holder.
  `replace_match` calls it instead of `insert_resource(PendingReplies::default())`.
- `OverlayData::forget_runner()` drops every poll (in-flight id included) and the drawn answers; a fresh
  `Poll` starts due, so an active overlay polls the new runner on the next frame. `replace_match` calls it
  (next to the existing `InspectorState.pending = None`).
- Other pending-id holders checked: only `InspectorState.pending` (already reset) and `OverlayData` keep an id
  across frames; `remote.rs` (`query`), `fl_game::reload` and `wait_for_reply` wait synchronously and abandon
  on failure.

### 2. M9 — selection and control groups across a load (`selection.rs`, `debug/mod.rs`)
- `replace_match` resets `Selection` and `ControlGroups` for `LoadRequest::Scenario` and `LoadRequest::Save`;
  `Rebuild` keeps both (it continues the match).
- `prune_selection` keeps only units that are the local player's (`is_own`) in the selection. Groups are still
  pruned by liveness only: recalling a group already filters to own units (`Context::live`), and dropping
  non-own ids from groups would wipe every group on a console player switch. Documented on the function and
  pinned by the test below.

### 3. M2 — `SteeringRules::link` (`crates/fl_sim/src/movement.rs`)
`separation_gain` in [0, 4], `separation_margin_m` in [0, 50] m, `max_push_frac` in (0, 1];
`MovementRules::link` caps `formation_max_spread_m` and `follow_distance_m` at 4096 m (R89) besides ≥ 0. Private
consts only; no API, codec or hash change (base rules are inside every range).

### 4. I3 — content-only diffs at task level (`xtask/src/{changed.rs,gate.rs}`)
`changed::touches_content(files)` (anything under `assets/`), new `Ctx.content`, and at task level
`test-headless` adds `CONTENT_TESTS = package(=fl_catalog) | (package(=fl_test_utils) & binary(=golden))`.
The filter is built by the new pure `task_headless_filter`; `task_filter` takes the optional content set.
Checked with `cargo nextest list … -E '<CONTENT_TESTS>'`: it selects the 3 `fl_catalog::base_content` tests and
the 4 `fl_test_utils::golden` tests (`golden_replays_verify`, `testbed_paths_replay_verifies`,
`testbed_paths_semantics`, `testbed_wander_replay_verifies`). Root `scenarios/` does not exist, so only
`assets/**` is treated as content (the brief's scope).

### 5. M15 — `**/*.proptest-regressions` protected (`OWNERS.toml`, `xtask/src/owners.rs` test)
Own commit `0a1b2d4`. `crates/fl_sim/tests/hpa.proptest-regressions` is unchanged (it is now listed in
`owners::tests::protected_globs`). The commit carries **no `Blessed:` trailer** (implementers never bless).
Since no protected file changed since the gate base, the `protected` row passes at `gate pr` without one; the
lead may still add the trailer if wanted.

### 6. Task 21 review docs
- `docs/debugging.md`: `fl.entity` and `fl.journal_since` re-run live on M1 (`fl_game --features debug_remote --
  --remote --remote-port 15730 --agent`, `base.testbed`): `orders`, `core.path_follow`, `core.order_changed`,
  `core.path_request`/`core.path_result`, and the new `brp_issue_command_m1_payloads_apply` test is cited.
- `docs/perf/M1.md`: cost is superlinear (+0.34 / +0.55 / +0.75 ms per 1,000 units), tracking `steer.pairs`;
  per-system medians sum to +3.83 ms vs the tick's +3.87 ms (medians of parts); the terrain-mesh claim is now
  limited to the one loaded pair run.
- `docs/m1-demo-checklist.md`: the shuffle row reads "PASS (filtered release subset, not the brief's full debug
  run): 55 tests, 54 passed" (R139).
- `README.md`: headless tools list `query` and `map-gen` (the `query` line was run: exit 0).

### 7. Task 21 review test (`crates/fl_game/tests/remote.rs`)
`brp_issue_command_m1_payloads_apply` (headless game + real threaded runner over BRP): `core.rotate
{toward}` turns tiger #3 from 90° to east; `core.follow {leader: 3}` puts `Follow` in #4's orders and #4
drives north after #3 (y > 400 from 350); `debug.set_terrain {min_cell, max_cell, terrain}` turns
`core.terrain_at [71, 124]` into `base.water`. `brp_` prefix: runs at nightly/milestone with `debug_remote`.

## TDD evidence

RED (before the implementation, tests written first):

```
cargo nextest run --profile agent -p fl_client --test overlays --test selection
  FAIL fl_client::overlays a_query_after_a_match_swap_gets_its_own_reply
    assertion `left == right` failed: query 0 kept its reply (the stale poll was 0)
      left: None   right: Some(Entity(None))
  FAIL fl_client::overlays overlay_polls_again_after_a_match_swap
    panicked at crates\fl_client\tests\overlays.rs:100:5  ("no debug query within 15 frames")
  FAIL fl_client::selection a_selected_unit_given_away_leaves_the_selection
    left: [1, 2]  right: [1]
  FAIL fl_client::selection loading_another_match_clears_selection_and_groups
    left: [1, 2]  right: []
  Summary 47 tests run: 43 passed, 4 failed
```
Expected: ids restarted at 0 so the stale overlay poll (id 0) stole the new query's reply, the frozen poll never
re-polled, `prune_selection` had no ownership check, `replace_match` kept the selection.
(`a_rebuild_keeps_selection_and_groups` passed before and after: it pins the behaviour that must stay.)

```
cargo nextest run --profile agent -p fl_sim --test terrain -E 'test(steering_rules_ranges) | test(group_distances_are_capped)'
  FAIL fl_sim::terrain group_distances_are_capped_at_the_map_side   (terrain.rs:150 "<name>: the mod loads")
  FAIL fl_sim::terrain steering_rules_ranges_are_validated          (terrain.rs:150 "<name>: the mod loads")
  Summary 2 tests run: 0 passed, 2 failed
```
Expected: the out-of-range rules loaded.

```
cargo nextest run --profile agent -p xtask -E 'test(content_change_runs)'
  error[E0425]: cannot find function `touches_content` in module `changed`
  error[E0425]: cannot find function `task_headless_filter` in this scope
```
Expected: compile-RED, the API did not exist.

```
(OWNERS.toml change set aside as a patch, test edited) cargo nextest run --profile agent -p xtask -E 'test(protected_globs)'
  FAIL xtask::bin/xtask owners::tests::protected_globs   1 test run: 0 passed, 1 failed
```

Item 7 is test-only (the codecs landed in `ebc1242`), so there is no RED against the code. Mutation check
instead: with the test's payload renamed `toward` → `towards` it fails with
`command 'core.rotate' payload: unknown field `towards`, expected `toward``; restored, it passes.

GREEN (focused, after each implementation):
```
-p fl_client --test overlays --test selection     47 tests run: 47 passed
-p fl_sim --test terrain --test orders            48 tests run: 48 passed
-p xtask                                          96 tests run: 96 passed
-p fl_game --features debug_remote --test remote -E 'test(brp_issue_command_m1)'   1 passed
```

## Full verification (after the last code commit)

```
cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli -p xtask --no-fail-fast
  Summary [ 189.926s] 599 tests run: 599 passed (1 slow), 0 skipped
cargo nextest run --profile agent -p fl_client -p fl_game --features fl_client/debug_remote,fl_game/debug_remote --no-fail-fast
  Summary [ 125.395s] 214 tests run: 214 passed, 0 skipped        (gpu_ and brp_ included)
cargo xtask gate pr --track contract,sim-core,client,tools,docs,perf
  PASS fmt, clippy-drift, claude-md, check-scope, protected, clippy, arch-check, test-headless,
       test-client, api, deny, data-check
  WARN test-guard: 3 findings (reviewer must accept each)
  gate pr: PASS in 560s
```
`gate pr` has no `perf-file` row (that row is milestone-only), so it is fully PASS; `gate milestone` still needs
the lead's bless of `M1.json.new` (I1).

test-guard findings (all false positives, for the reviewer to accept):
- `crates/fl_sim/src/movement.rs:161 ToleranceChanged: const MAX_SEPARATION_MARGIN_M: i32 = 50;` — a new
  content range bound (M2), not a test tolerance.
- `xtask/src/gate.rs:847` and `:854 RemovedAssert` (`task_filter_selects_touched_and_smoke`) — both asserts are
  still there; only the call gained the `None` content argument (`task_filter(&touched, &smoke, None, excl)`).

## Files changed
`crates/fl_client/src/{link.rs,overlays.rs,selection.rs,debug/mod.rs}`, `crates/fl_client/tests/{overlays.rs,
selection.rs}`, `crates/fl_sim/src/movement.rs`, `crates/fl_sim/tests/terrain.rs`, `xtask/src/{changed.rs,
gate.rs,owners.rs}`, `OWNERS.toml`, `crates/fl_game/tests/remote.rs`, `docs/debugging.md`, `docs/perf/M1.md`,
`docs/m1-demo-checklist.md`, `README.md`.

## Self-review / concerns
1. **M15 trailer:** `0a1b2d4` has no `Blessed:` trailer. The brief expected the commit to need one; the
   `protected` row passes at `gate pr` without it because no protected file changed since the base. Lead's
   call whether to add it.
2. **M9 groups:** non-own ids are dropped from the selection only, not from control groups (recall already
   filters to own units; pruning groups would empty them on a player switch). If the lead reads the brief as
   "groups too", it is a one-line change in `prune_selection`.
3. **Live docs run:** the `debugging.md` re-run used a fresh session, so its ticks (137, 142) differ from the
   neighbouring M0-era examples (`fl.spawn` 163, `fl.pause` 1943); each re-run block says which run it is from.
4. During item 7 I lost the uncommitted test once with `git checkout <file>` (reverting a mutation check) and
   rewrote it; the committed version is the one verified above.
