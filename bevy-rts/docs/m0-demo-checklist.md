# M0 demo checklist

Walked on 2026-10-24 by the Task 30 implementer (an agent, no human at the keyboard) on `m0/phase-a` at
`3500b33` plus the Task 30 commits. Windows 11 Pro, Intel Core Ultra 9 185H, NVIDIA RTX 1000 Ada Laptop GPU
(Vulkan). The game ran as `fl_game --features debug_remote -- --remote --remote-port 15730 --agent` (dev
build; BRP needs a debug build) and was driven only over BRP: `fl.*` methods for the sim, `world.query` and
`brp_extras/*` input methods on the client World. Screenshots are in `docs/demo/` (`fl.screenshot`, an
offscreen copy of the game camera, 1280×720; the two UI shots are window captures with
`brp_extras/screenshot`, scaled down). Every request and response quoted here is in `docs/debugging.md` §4.

Result legend: **PASS** seen in the live session; **PASS (test)** covered by the named test, not walked
live; **NOT RUN** needs a human (stated why).

## Rev-1 Task 17 items

| # | Item | How it was checked | Result |
|---|---|---|---|
| 1 | Iso view of the 1 km ground, 10 placeholders in faction colours | `fl.sim_state`: 10 entities, map 1000×1000; screenshots [01](demo/m0-01-iso-testbed.png), [14](demo/m0-14-zoom-out.png): German units olive, Soviet units blue-grey, tanks with turret and barrel, trucks, riflemen as capsules | PASS |
| 2 | Camera pan (WASD/arrows) | `brp_extras/send_keys KeyD 1 s`: camera translation (724.1, 230.9, 724.4) → (844.4, 230.9, 604.1); [11](demo/m0-11-zoom-in.png) → [12](demo/m0-12-pan.png) | PASS |
| 3 | Camera edge pan | needs a real cursor at the window edge; `fl_client::camera` unit test `edge_pan_left_edge` | PASS (test) |
| 4 | Wheel zoom | `brp_extras/scroll_mouse y=+5`: `FixedVertical` height 335.4 → 167.7; `y=-10`: 167.7 → 335.4; [11](demo/m0-11-zoom-in.png), [14](demo/m0-14-zoom-out.png); limits: `zoom_is_clamped` | PASS |
| 5 | Q/E rotate, snap (R in M0, Home since M1 Task 16) | `send_keys KeyE 0.5 s`: translation (844.4, 604.1) → (940.0, 369.9) at the same height ([13](demo/m0-13-rotate.png)); `KeyR`: (940.0, 369.9) → (940.0, 373.2) (snapped to 45°) | PASS |
| 6 | Camera clamped at the map edge | `send_keys KeyA+KeyS` 8 s: translation stops at (1326.6, 230.9, 1000.0) and stays there after 3 s more; [15](demo/m0-15-edge-clamp.png) shows the map corner at the focus | PASS |
| 7 | Console `spawn` | `fl.spawn` (the same `debug.spawn` command the console issues): `base.de.tiger` at (500, 480) → `{exec_tick: 163, seq: 0}`; sim id 11 exists with that pose, `core.unit_spawned` journaled; [02](demo/m0-02-after-spawn.png). Unknown unit → `-32602`. Typing into the console: see row 9 | PASS |
| 8 | Spawn-panel click-to-place | needs mouse picking on a visible, focused window and a human to aim | NOT RUN (needs a human) |
| 9 | `wander 0 100` with smooth interpolation | console text typed with `brp_extras/type_text` does not reach the egui text field (the console opens: [30](demo/m0-30-console.png)), so `debug.wander {radius: 100}` was issued with `fl.issue_command` (the console's command path is `console_wander_targets_the_players_units`). Sampling one unit's visual with `world.query` 42 times in 1 s gave 42 distinct positions over 20 sim ticks (≈2 frames per tick, moving smoothly, e.g. (407.52, 390.60) → (398.35, 402.06)); [03](demo/m0-03-wander.png) | PASS (wander, interpolation); console typing NOT RUN |
| 10 | Stats panel: tick, FPS, hash | window capture [30](demo/m0-30-console.png): fps 50, tick 489, last hash, rules hash, units 10, player 0, runner running ×1 | PASS |

## Rev-2 additions

| # | Item | How it was checked | Result |
|---|---|---|---|
| 11 | `setowner`/`invuln`/`sethp`/`teleport` cheats work | `fl.issue_command` `debug.set_owner {owner: 0}` on #6 (player 1's T-70) → `core.owner: 0`; `debug.invulnerable {on: true}` on #2 → `core.invulnerable` present; `debug.set_hp {hp: 5}` on #3 → `hp: 5/210`; `debug.teleport {pos: [100, 900]}` on #4 → pose (109.3, 888.2) a second later (it kept wandering). Console mapping: `console_debug_cheats_map_to_kinds`, `console_debug_commands_issued_as_local_player` | PASS |
| 12 | …and are rejected with `cheats_allowed = false` | `fl_game` always allows cheats; `fl_sim/tests/commands.rs::debug_requires_cheats`, `fl_client/tests/debug.rs::console_cheats_need_cheats_allowed` | PASS (test) |
| 13 | Orders given while paused apply on resume | `fl.pause` at tick 1943; `core.move` to (600, 600) for #1 → `exec_tick 1944`; the tick stayed 1943 for 1 s; `fl.step 20` → #1 has the waypoint and is heading for it; journal: `core.command_applied` + `core.move_order` at 1944. `fl_client/tests/view.rs::paused_orders_apply_on_resume` | PASS |
| 14 | Speed ×0.25–×8 | `fl.set_speed`: ×0.25 → 10 ticks in 2 s; ×8 → 322 ticks in 2 s; ×1 → 41 ticks in 2 s; 9.0 → `-32602` | PASS |
| 15 | Single step | `fl.step {n: 20}` → `{tick: 1963, completed: 20, finished: true}`; `n: 601` → `-32602`; console `step`: `console_step_while_paused` | PASS |
| 16 | `reveal` toggles | client render override; console typing does not work over BRP (row 9); `console_reveal_toggles_render_override`, `reveal_all_draws_unseen_records` | PASS (test) |
| 17 | `player 1` switches the controlled player | `console_player_switch`; the stats panel shows `player 0` | PASS (test) |
| 18 | `inspect` window shows components | the Inspector window is present ([30](demo/m0-30-console.png)); its query is `console_inspect_sends_query`; the same components come back from `fl.entity` | PASS (test) |
| 19 | F2 perf panel lists phases and features | window capture [31](demo/m0-31-perf-panel.png): 8 phases with p50/p95/max (real phase timings now that `debug.ron`'s `profiler` turns on the sim's phase markers), features below, unit visuals 10 | PASS |
| 20 | `fl.save` then `fl.load` resumes at the same tick and hash | paused at tick 2584, hash 15586365802526815550; `fl.save` (4053 bytes); 40 steps → tick 2624, hash 11263170666591694002; `fl.load` → tick 2584, same hash; 40 steps → tick 2624, same hash 11263170666591694002 | PASS |
| 21 | BRP smoke (`fl.sim_state` until tick ≥ 1, `fl.spawn`, `fl.step 20`, `fl.entity`, `fl.screenshot`) | all done in the session above; `fl_game/tests/remote.rs::brp_step_state_command` in the gate's `brp` row | PASS |
| 22 | Client and headless sims agree | the game's tick-0 and tick-20 hashes (`4c4acd04ce1d032a`, `8b3418f4fcb9d9af`) equal `fl_cli hash-dump --scenario base.testbed` | PASS |
| 23 | Render stress, 3,000 units ≥ 60 fps | `docs/perf/M0.md` "Render stress": median 98.8 fps average (best 104.0) over 7 runs; 2 runs below 60 under the heaviest foreign build load | PASS (idle-machine rerun owed) |

## Content and determinism

| Check | Command | Result |
|---|---|---|
| Content | `cargo run -p fl_cli --release -- data-check assets/base` | PASS: exit 0, 10 units, rules hash `0xbc8d91076ccb4b38` |
| Twin run, shuffle seeds, save/branch, golden replay (release) | `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent --release -p fl_sim -p fl_test_utils` | 212 of 213 PASS, including `scale_twin_run_500_units`, `smoke_shuffle_invariant`, `shuffled_schedule_gives_the_same_hashes`, `smoke_save_round_trip`, `smoke_branch`, `scale_mix_branch`, `branch_test_hashes_match_every_tick`, `golden_replays_verify`, `testbed_wander_replay_verifies`. The one failure, `fl_sim::classify::tests::unclassified_sim_type_fails_build`, is not a determinism test: the classification check runs only in debug builds by design (`build.rs`, `cfg!(debug_assertions)`), and the test is not gated to debug builds (reported to the controller) |
| The same suite in debug builds | `cargo xtask gate pr` / `gate milestone` row `test-headless` (`FL_SHUFFLE_SEEDS` set by the gate) | PASS |
| Golden replay | `cargo run -p fl_cli --release -- replay-verify crates/fl_test_utils/tests/replays/testbed_wander.replay` | PASS: `verified: 400 ticks` |
| Save/load by hand | `fl_cli save … --tick 100` then `load … --ticks 100` | PASS: final hash `0x28c67f8b5038110a` equals the uninterrupted run |
| Gates | `cargo xtask gate pr`, `cargo xtask gate milestone --milestone M0` | PASS (outputs in the Task 30 report) |

## ADR confirmations

One row per Confirmation bullet of ADR-0001…0013. "gate row X" means the row of `cargo xtask gate pr` /
`gate milestone --milestone M0` (outputs in the Task 30 report); test names are `crate/file::test`.

| ADR | Confirmation bullet | Evidence | Result |
|---|---|---|---|
| 0001 | `xtask arch-check`: allowed edges and per-package `cargo tree` | gate row `arch-check` | PASS |
| 0001 | Per-package headless test runs in `gate task` and `gate pr` | gate row `test-headless` (pr: each headless package built on its own) | PASS |
| 0001 | `GameBuilder` tests for cycle, missing-dependency, duplicate and orphan-client errors | `fl_sim/tests/feature.rs::{feature_cycle_is_error, missing_dependency_is_error, duplicate_feature_id_is_error}`, `fl_game/tests/builder.rs::orphan_client_feature_is_error` | PASS |
| 0001 | Public-API snapshot of `SimFeature`, `ClientFeature`, `SimBuilder` | gate row `api` | PASS |
| 0002 | `fl_client` has no edge to `fl_sim` | gate row `arch-check` | PASS |
| 0002 | Threaded and inline runners give identical hash streams | `fl_sim/tests/runner.rs::inline_and_threaded_hash_streams_equal` | PASS |
| 0002 | Loopback `LockstepDriver`, input delay 2, two sims hash-identical | `fl_sim/tests/runner.rs::loopback_lockstep_two_sims_identical` | PASS |
| 0002 | Render stress test, 3,000 placeholders at ≥ 60 fps, threaded runner | `fl_game --release -- --stress 3000 --exit-after 25 --agent` (`docs/perf/M0.md`): median 98.8 fps average over 7 runs, 5 of 7 ≥ 60, on a machine kept at 96–100 % CPU by other builds | PASS (idle-machine rerun owed) |
| 0003 | Clippy bans in every sim crate (no `Pcg32`/`rand`) | gate rows `clippy-drift`, `clippy` (generated per-crate `clippy.toml`) | PASS |
| 0003 | `fl_math` tests: reference vectors, every input changes the output, tick/sim_id not interchangeable, range bounds and coverage | `fl_math/tests/rolls.rs::{roll_reference_vectors, stream_key_reference_vectors, each_input_changes_roll, tick_and_sim_id_not_interchangeable, unit_fx_and_range_fx_bounds, range_u32_covers_all_values, lemire_rejection_consumes_extra_draws}` | PASS |
| 0003 | Golden stability: a dummy stream and drawing system leave a scenario's hashes unchanged | `fl_sim/tests/movement.rs::adding_a_stream_does_not_move_wander`, `fl_sim/tests/golden.rs::golden_stability_extra_feature` | PASS |
| 0003 | Review checklist: every draw site names a registered stream | per-task reviews (determinism checklist, `.claude/agents/reviewer.md`) | PASS (review) |
| 0004 | IDs sorted by name, duplicate and post-freeze registration panic | `fl_sim/tests/commands.rs::command_type_ids_sorted_by_name`, `fl_sim/tests/feature.rs::{registry_ids_sorted_by_name, register_after_freeze_panics, duplicate_name_panics_naming_both}` | PASS |
| 0004 | String-table round trip through the header | `fl_protocol/tests/protocol.rs::{match_header_postcard_roundtrip, string_tables_id_of_and_remap}` | PASS |
| 0004 | Loading a replay recorded with an extra registered name | remapping is tested for saves (`fl_sim/tests/save.rs::{load_remaps_unit_type_ids, load_remaps_faction_ids, faction_renumbering_keeps_the_hash}`); no replay-specific test | PARTIAL: deferred to C8 (golden-replay versioning ADR, M2 entry) |
| 0004 | Golden stability with an early-sorting command, component and stream | `fl_sim/tests/golden.rs::golden_stability_extra_feature` | PASS |
| 0004 | Public-API snapshot of the registry types | gate row `api` | PASS |
| 0005 | Schedule build fails on ambiguity | `fl_sim/tests/schedule.rs::ambiguous_systems_fail_schedule_build` | PASS |
| 0005 | Shuffle seeds: hashes equal the unshuffled run | `FL_SHUFFLE_SEEDS=3` run of `fl_sim` + `fl_test_utils` (`smoke_shuffle_invariant`, `shuffled_schedule_gives_the_same_hashes`, `journal_identical_across_shuffle_seeds`) | PASS |
| 0005 | Insta snapshot of the `SimTick` system order | `fl_test_utils/tests/system_order.rs::sim_tick_system_order` | PASS |
| 0005 | `clear_trackers` and message rotation once per `step_tick` | `fl_sim/tests/schedule.rs::{clear_trackers_runs_each_step, messages_rotate_once_per_tick}` | PASS |
| 0005 | The branch test catches iteration-order dependence | `fl_sim/tests/save.rs::branch_test_hashes_match_every_tick`, `fl_test_utils/tests/determinism.rs::{smoke_branch, scale_mix_branch}` | PASS |
| 0006 | Startup classification check in every test binary | `fl_sim/src/classify.rs::unclassified_sim_type_fails_build`, `fl_sim/tests/classify.rs::core_app_is_fully_classified` | PASS |
| 0006 | Round-trip and branch tests (smoke in task, several N in pr) | `fl_sim/tests/save.rs::{round_trip_is_byte_identical, save_at_tick_zero_round_trips, branch_test_hashes_match_every_tick, branch_after_despawns}`, `fl_test_utils/tests/determinism.rs::{smoke_save_round_trip, smoke_branch, scale_mix_branch}` | PASS |
| 0006 | `serde-reflection` snapshots of every save codec; unbumped change fails | `fl_sim/tests/formats.rs::{save_codec_formats, format_guard_flags_unbumped_codec_change}` | PASS |
| 0007 | `(player, seq)` order independent of submit order; duplicate-seq and late rejection | `fl_sim/tests/commands.rs::{commands_apply_in_player_seq_order, duplicate_seq_rejected, late_and_early_rejected}` | PASS |
| 0007 | Postcard round trip of `PlayerCommand` and `MatchHeader` | `fl_protocol/tests/protocol.rs::{player_command_postcard_roundtrip, match_header_postcard_roundtrip}` | PASS |
| 0007 | Rules hash sees a one-ULP `Fx` change at large magnitude; ignores field order and whitespace | `fl_content/src/hash.rs::rules_hash_sees_low_bits_at_large_magnitude`, `fl_content/tests/load.rs::{rules_hash_ignores_formatting, rules_hash_stable_across_loads}` | PASS |
| 0007 | `serde-reflection` snapshots of `PlayerCommand`, payloads, `MatchHeader` | `fl_protocol/tests/formats.rs::{protocol_formats, format_guard_flags_unbumped_change}` | PASS |
| 0007 | `cargo deny` bans bincode | gate row `deny` (`deny.toml` `[bans]`) | PASS |
| 0008 | `debug.*` rejected without `cheats_allowed`, accepted with it; a replay with debug commands verifies | `fl_sim/tests/commands.rs::debug_requires_cheats`, `fl_test_utils/tests/golden.rs::testbed_wander_replay_verifies` (`debug.wander`), `fl_cli replay-verify` of the golden | PASS |
| 0008 | BRP integration test drives `fl.step`, `fl.sim_state`, `fl.issue_command` over HTTP | `fl_game/tests/remote.rs::{brp_step_state_command, brp_save_load_and_scenarios}` (gate row `brp`), plus the live session in this checklist | PASS |
| 0008 | No client path into the sim World except the runner protocol | gate row `arch-check` (no `fl_client` → `fl_sim` edge) | PASS |
| 0009 | Hooks in `.claude/settings.json` and role agents; an implementer's protected-path edit is denied | `xtask/src/hooks.rs::{guard_denies_protected_for_implementer, guard_denies_bless_variants, shell_writes_onto_protected_paths}` (hook logic) | PARTIAL: a live hook denial from a role subagent is deferred (R65: M0 sessions run outside the repo's project dir, so role-agent frontmatter hooks were never exercised live) |
| 0009 | `gate task` timing is reported; over 5 minutes is a defect | gate footer (`WARN gate task took …`), `xtask/src/gate.rs::fail_tail_is_capped_and_budget_warned` | PASS |
| 0009 | `gate pr` before every merge into `main` | SDD ledger (every `main` fast-forward follows a `gate pr: PASS`); this task's `gate pr` output in the report | PASS |
| 0010 | `docs/perf/budgets.ron` exists and is protected; counter tests on the scale scenario | `OWNERS.toml` `[protected]`, `fl_test_utils/tests/perf_counters.rs::{smoke_counters_scale_500, counters_scale_3000, milestone_counters_scale_5000}` | PASS |
| 0010 | Milestone gate requires `docs/perf/M<n>.json` with median-of-5 numbers | gate row `perf-file`; `docs/perf/M0.json` (`repeat: 5`) | PASS |
| 0011 | `gate pr` runs API and format snapshots and `check-scope` | gate rows `api`, `test-headless` (format tests), `check-scope` | PASS |
| 0011 | Review rejects a contract-path diff without an ADR reference | per-task reviews; `Refs: ADR-NNNN` trailers | PASS (review) |
| 0012 | M1 tests: value reload, ID-set rebuild, replay with a reload event is non-reproducible | `fl_sim/tests/replay.rs::replay_with_reload_event_is_non_reproducible` exists; reload itself is M1 | deferred: hot reload is M1 |
| 0013 | `gate task` timings; the build-dir layout under `C:/t/fl/` | gate footers; `.cargo/config.toml` `build.build-dir` | PASS |
| 0013 | M0 probes recorded in `docs/research/bevy020-api-reference.md` | "Probe: dynamic_linking", BRP sections; `probes/README.md` | PASS |
| 0013 | `CLAUDE.md` at most 200 lines, checked by xtask | gate row `claude-md`, `xtask/src/hooks.rs::claude_md_limit` | PASS |
| — | D1 cross-OS determinism | Windows only | deferred: D1 (user decision, no cross-OS CI) |
| — | D2 instruction-count perf tier | not built | deferred: D2 (user decision) |
