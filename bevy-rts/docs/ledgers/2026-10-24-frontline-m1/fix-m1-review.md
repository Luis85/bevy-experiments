# M1 final fix wave: scoped re-review (61cc357..e932f56)

Read-only; no cargo run. Evidence from review-fixm1.diff, fix-m1-report.md and a grep of crates/fl_client/src.

## Finding verdicts

- **I4 stale overlay poll after a match swap** — ADDRESSED. `replace_match` (debug/mod.rs, hunk at ~l.372) calls `PendingReplies::forget_runner()` (link.rs: clears `replies`/`abandoned`, keeps `next_id`, so ids stay monotonic) and `OverlayData::forget_runner()` (overlays.rs: `polls.clear()` plus `clear(kind)` for all kinds). `poll_overlays` recreates entries with `polls.entry(kind).or_default()` and `Poll` starts due, so polling resumes on the next frame. The only other cross-frame id holder, `InspectorState.pending`, was already reset. The remaining `send_query` callers (remote.rs, reload, wait_for_reply) wait synchronously. Tests: `overlay_polls_again_after_a_match_swap` (the new runner gets `core.passability`, the reply is drawn) and `a_query_after_a_match_swap_gets_its_own_reply` (a non-overlay query after the swap has a different id and keeps its reply). RED was captured in the report.
- **M9 selection/groups across a load** — ADDRESSED. `replace_match` resets `Selection` and `ControlGroups` inside the `if let Some(scenario)` branch. That branch covers `Scenario` and `Save` (`Some(None)`, debug/mod.rs l.347-349) and not `Rebuild` (`None`). Tests: `loading_another_match_clears_selection_and_groups` (both Scenario and Save), `a_rebuild_keeps_selection_and_groups`. `prune_selection` now keeps only the local player's units in the selection (selection.rs, `is_own`), with `a_selected_unit_given_away_leaves_the_selection`. Groups are pruned by liveness only, which is the lead's accepted behaviour. The test pins that a group keeps the given-away unit and that a recall takes only own units.
- **M2 SteeringRules range checks** — ADDRESSED. `separation_gain` in [0, 4], `separation_margin_m` in [0, 50], `max_push_frac` in (0, 1] (movement.rs `SteeringRules::link`). `formation_max_spread_m` and `follow_distance_m` are capped at 4096 in `MovementRules::link`. Test `steering_rules_ranges_are_validated` rejects -0.1 and 4.5 for gain, -0.5 and 50.5 for margin, and 0.0, -0.5 and 1.5 for push. It accepts 0.0 and 4.0 for gain, 0.0 and 50.0 for margin, and 1.0 for push. Test `group_distances_are_capped_at_the_map_side` rejects 4096.5 and accepts 4096.0 for both fields. Both go through content loading (R104 class). RED was captured.
- **I3 content-only diff runs catalog and golden tests** — ADDRESSED. `touches_content` (changed.rs) is true only for paths under `assets` (component-wise `starts_with`, so `docs/assets.md` does not match). `Ctx.content` feeds `task_headless_filter`, which adds `CONTENT_TESTS` only when `content` is true and `fl_catalog` is headless. Fixture test `content_change_runs_catalog_and_golden_tests_at_task_level` uses `assets/base/rules.ron` with no touched package. It asserts the exact filter with the catalog and golden terms. It asserts the smoke-only filter when `content = false`. It asserts that code and `docs/assets.md` do not count as content. The report shows `cargo nextest list` selecting 3 `fl_catalog` and 4 `fl_test_utils::golden` tests.
- **M15 proptest regressions protected** — ADDRESSED. `OWNERS.toml` `[protected]` has `**/*.proptest-regressions`. `owners::tests::protected_globs` asserts `crates/fl_sim/tests/hpa.proptest-regressions` is protected. That file is not in the diff. The change is in its own commit `0a1b2d4`. It has no `Blessed:` trailer, which is the lead's call (see below).
- **test-guard: movement.rs:161 `MAX_SEPARATION_MARGIN_M`** — FALSE POSITIVE. It is a new content range constant, not a loosened test tolerance.
- **test-guard: gate.rs:847 `RemovedAssert`** — FALSE POSITIVE. Both asserts in `task_filter_selects_touched_and_smoke` remain; only the call gained the `None` content argument.
- **test-guard: gate.rs:854 `RemovedAssert`** — FALSE POSITIVE. Same test, same reason.
- **Task 21 review, docs (item 6)** — ADDRESSED. `fl.entity` shows `orders` and `core.path_follow`. `fl.journal_since` shows `core.order_changed`, `path_request` and `path_result`. M1.md says the cost is superlinear and tracks `steer.pairs`, gives per-system sum +3.83 against +3.87 ms, and limits the terrain-mesh claim to the one loaded pair. The checklist shuffle row is relabelled as a filtered release subset with 54/55 and R139. README lists `query` and `map-gen`.
- **Task 21 review, test (item 7)** — ADDRESSED. `brp_issue_command_m1_payloads_apply` issues `core.rotate {toward}`, `core.follow {leader}` and `debug.set_terrain {min_cell, max_cell, terrain}` through `fl.issue_command`. It checks the heading turns east, `Follow` appears in #4's orders and #4 moves north, and `core.terrain_at` becomes `base.water`. A mutation check (`toward` renamed to `towards`) is in the report.

## New breakage in the fix diff

- Minor, debug/mod.rs ~l.375-378: a `Rebuild` also calls `OverlayData::forget_runner()`, so active overlays drop their drawn data and re-poll. The data is blank for about one poll round trip after a rebuild. That is correct, since the data belongs to the old runner, but it is a visible one-frame flicker and no test covers it.
- Minor, docs/perf/M1.md: the edited line is not reflowed (the `M0's own docs/perf/M0.md runs…` line is over-long). No functional impact.
- No Critical or Important breakage.

## Out-of-scope observations

- None.

## Verdict

**Fix round:** All findings addressed, no new Critical/Important breakage. The lead decides whether `0a1b2d4` needs a `Blessed:` trailer. The `protected` row passes at `gate pr` without one because no protected file changed since the gate base.
