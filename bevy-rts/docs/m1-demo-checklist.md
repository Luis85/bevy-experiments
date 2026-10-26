# M1 demo checklist

Walked on 2026-10-25 by the M1 Task 21 implementer (an agent, no human at the keyboard) on `m1/phase-a` at
`3d78b45` plus the Task 21 commits. Windows 11 Pro 10.0.26300, Intel Core Ultra 9 185H, 63.5 GB RAM, NVIDIA RTX
1000 Ada Generation Laptop GPU (Vulkan, driver 596.71). The game ran as `fl_game --features debug_remote --
--remote --remote-port 15721 --agent` (dev build: BRP and the hot-reload watcher exist only in debug builds) and
was driven over BRP only: `fl.*` methods for the sim, and `brp_extras/move_mouse`, `click_mouse`,
`double_click_mouse`, `drag_mouse`, `send_keys` and `scroll_mouse` for real mouse and keyboard input on the client
(screen positions computed by projecting the unit visuals through the camera; `docs/debugging.md` §4
`fl.select`). Screenshots are in `docs/demo/` (`fl.screenshot`, the game camera rendered offscreen, scaled to
960×540; gizmo overlays and selection rings are in them; the console shot is a window capture with
`brp_extras/screenshot`). Requests and responses are in `docs/debugging.md` §4, §8–§10.

Result legend: **PASS** seen in the live session; **PASS (test)** covered by the named test, not walked live;
**HUMAN** needs a person (stated why); **NOT DONE** with the reason.

## Terrain and units

| # | Item | How it was checked | Result |
|---|---|---|---|
| 1 | The terrain shows the river, two bridges, roads, forest, the hill and the ridge | [01](demo/m1-01-terrain-overview.png): river (blue) with a bridge at each road crossing, two roads each way, forest blocks (dark green), field (brown), swamp (grey-green), the ridge; `core.terrain_at` finds `base.bridge` at cells x 70–72 and 180–182, y 122–127. The ridge is clearly modelled ([03](demo/m1-03-ridge.png)); the 12 m hill at (240, 800) m is only a faint shading change ([02](demo/m1-02-hill-units-on-ground.png)), see "Visual notes" | PASS (hill: HUMAN to judge) |
| 2 | Units stand on the ground | Spawned two Pz IIIs on the hill and a rifleman on the ridge: visual `Transform.y` vs `core.terrain_at` `height_raw`: 11.952 vs 12.000 m at the hill top (cell centre vs the unit's position), 9.056 vs 9.056 on the hill's flank, 6.000 vs 6.000 on the ridge | PASS |
| 3 | Client and headless sims agree | the game's `state hash` log records at ticks 0, 20 and 40 equal `fl_cli hash-dump --mods assets/base --scenario base.testbed --ticks 41` line for line | PASS |

## Selection

| # | Item | How it was checked | Result |
|---|---|---|---|
| 4 | Click selects an own unit | `click_mouse Left` on Pz III #12's screen position → `fl.selection` `[12]` | PASS |
| 5 | Shift + click adds | Shift held (`send_keys ShiftLeft`, 1.2 s) + click on #14 → `[12, 14]` | PASS |
| 6 | A click on empty ground clears, unless Shift is held | click 60 px off any unit → `[]`; with Shift, the selection `[13]` stayed `[13]` in 4 of 5 tries (once cleared: the synthetic Shift and the click landed in an order the game did not see as held; `click_empty_ground_clears_unless_shift` covers the rule) | PASS (live 4/5; test) |
| 7 | Box select; a drag under 6 px is a click | `drag_mouse` over the six Pz IIIs → `[3, 11, 12, 13, 14, 15, 16]` (the Tiger #3 was inside the box); a 3 px drag on #16 → `[16]` (`tiny_drag_is_a_click`). The drag rectangle is not drawn in M1 (Task 15 concern) | PASS |
| 8 | Double-click on a Pz III selects every own Pz III on screen; on an enemy, nothing | `double_click_mouse` on #15 → `[2, 11, 12, 13, 14, 15, 16]` (#2 is the testbed's Pz III); on the T-70 #6 → `[]` | PASS |
| 9 | Ctrl+1 / 1 / Shift+1 / Alt+1 | `fl.select [1,2,3]`, `send_keys ControlLeft+Digit1`; `fl.select [4]`, `Digit1` → `[1, 2, 3]`; `fl.select [5]`, `ShiftLeft+Digit1` → `[1, 2, 3, 5]`; `AltLeft+Digit1` → `[1, 2, 3]` and the camera moved from (730.9, 230.9, 730.9) to (671.4, 230.9, 604.0); Ctrl+1 with an empty selection, then 1 → `[]` | PASS |
| 10 | Control groups lose dead units; keys typed into the console don't select | no unit dies in M1 (no combat): `control_group_drops_dead_units`; `input_ignored_while_console_open` | PASS (test) |
| 11 | Selection rings | every screenshot with a selection shows the green rings ([04](demo/m1-04-group-on-bridge.png)); on slopes they stay flat (see "Visual notes") | PASS |

## Orders

| # | Item | How it was checked | Result |
|---|---|---|---|
| 12 | Right-click move | #11 selected, right click on the ground 30 m east → `core.paths` orders `[{move: [306.1, 702.6]}]` (the clicked point) | PASS |
| 13 | Shift-queue | Shift + right click on the ground → a third queued `move`; Shift + right click on an own Tiger → a queued `follow: 3`; 34 Shift + right clicks on one unit filled its 32 orders and the next commands were rejected `core.queue_full` | PASS |
| 14 | H stops | `send_keys KeyH` → `state: idle`, `orders: []`, journal `core.order_changed` `reason: stopped` | PASS |
| 15 | R + click rotates | `KeyR`, left click 28 m south-west of #11 → `orders: [{rotate: …}]`; 40 ticks later heading 225.0° and the order popped | PASS |
| 16 | A right click on an own unit follows; F + click follows; Alt + right click moves | right click on #14 → `[{follow: 14}]`; `KeyF` + left click on #13 → `[{follow: 13}]`; Alt + right click on #14 → `[{move: [299.3, 706.6]}]` (its position). A follower kept 14.7–15.5 m behind its moving leader and closed to 8.0 m (`follow_distance_m`) when it stopped | PASS |
| 17 | Right-click move across the river goes over a bridge | 11 units (the 5 testbed units + 6 spawned Pz IIIs, south of the river) moved to (300, 720): every unit crossed between x 284 and 290 m, the west bridge (cells 70–72), none ever stood on water (sampled every 10 ticks), [04](demo/m1-04-group-on-bridge.png) | PASS |
| 18 | The tanks turn before driving | the Tiger #3 (facing 90°) turned 90° → 103.8° over 5 ticks without moving, then drove while finishing the turn (`turn_in_place_deg` 30); the Pz IIIs' goal was within 30°, so they drove at once | PASS |
| 19 | The group keeps its shape; group moves use flow fields | the move gave formation slots in two rows (y 733.5 and 708.8 m, 10–15 m apart); the 9 tracked units drove on one flow field (`core.paths` `state: flow`, journal `core.path_request kind: group` and `core.path_result status: flow`), the truck and the rifleman on single paths; arrivals stopped near their slots | PASS |
| 20 | Units steer around each other | `steer.pairs` / `steer.units` counted every tick; the closest two Pz IIIs came was 3.2 m centre to centre (hull 2.9 × 5.6 m) while 6 tanks crossed one bridge; `units_never_enter_blocked_cells` | PASS |
| 21 | `debug.set_terrain` on the bridge in use makes the units repath | six Pz IIIs heading for the west bridge; at tick 148 `debug.set_terrain {min_cell: [70, 122], max_cell: [72, 127], terrain: base.water}` (what the console's `terrain 70 122 72 127 base.water` issues): `core.terrain_changed` journaled, `core.terrain_at` → `base.water`, the group turned east along the bank, crossed on the east bridge (x 720–723 m) and reached (260…340, 720); no unit on water. The terrain view and F6 show the flooded bridge ([08](demo/m1-08-bridge-removed-terrain-view.png), [10](demo/m1-10-bridge-removed-passability.png)). Console typing over BRP does not reach egui (M0 finding), so the command was issued with `fl.issue_command`, which needed the Task 21 fix `ebc1242`; `console_terrain_issues_debug_set_terrain` covers the console | PASS |
| 22 | Orders while paused apply on resume | `P` → paused; right-click move for #11; 1 s later the tick had not moved and the order was not applied; `P` again → `core.command_applied core.move` at the next tick and the new target in `core.paths` | PASS |
| 23 | `=` / `-` change the speed; F4 switches the player | `Equal` ×2 → 2.0, 4.0; `Minus` ×3 → 2.0, 1.0, 0.5; `F4` → `fl.select [6, 11]` keeps `[6]` (player 1's T-70); `F4` again → `[11]` | PASS |
| 24 | More than 256 units in one order | the client splits them (`big_selection_is_split_into_commands_of_at_most_256`) | PASS (test) |

## Overlays

| # | Item | How it was checked | Result |
|---|---|---|---|
| 25 | F5 paths | `paths` on: the selected units' remaining corners drawn from their visuals ([04](demo/m1-04-group-on-bridge.png): the truck's and the rifleman's single paths; flow units have no corners) | PASS |
| 26 | F6 blocked cells for the selected class | Pz IIIs selected: the river's water cells ([05](demo/m1-05-overlay-passability-tracked.png)); the truck selected: a wider band, its slope limit also blocks the banks ([07](demo/m1-07-overlay-passability-wheeled.png)); after the bridge was flooded the band is continuous ([10](demo/m1-10-bridge-removed-passability.png)) | PASS |
| 27 | F7 cells and clusters | the 4 m cell grid around the cursor, the 16-cell HPA* cluster borders and the entrance marks ([06](demo/m1-06-overlay-cells-clusters.png)) | PASS |
| 28 | The overlay keys are gated by `InputGate` | `fl_client/tests/overlays.rs::overlay_keys_ignored_while_console_open` | PASS (test) |

## Hot reload, save and load, AI

| # | Item | How it was checked | Result |
|---|---|---|---|
| 29 | Editing `pz3.unit.ron` HP reloads in place (while paused) | `hp: 110` → `150`: log `content reloaded in place tick=3213`, journal `core.content_reload`, every Pz III 150/150, same entities; a Pz III set to 75/150 became 100/200 after `150` → `200` (ratio kept) | PASS |
| 30 | Editing a terrain kind | road `pass: 1.0` → `0.5`: reloaded in place; `core.terrain_at` road cost 100 → 167 (Wheeled), 134 (Tracked) | PASS |
| 31 | A broken file shows an error and the match continues | a missing comma: console and log `reload failed; the match keeps running:` with `pz3.unit.ron:6:26: base.de.pz3/components.Hull: Expected comma` ([09](demo/m1-09-reload-error-console.png)); rules hash unchanged; 20 more ticks ran | PASS |
| 32 | Adding a unit file rebuilds | `t21_probe.unit.ron` added: `hot reload rebuilt the match tick=3238 dropped=0`, same tick, entities and poses; a spawned probe tank was dropped (`dropped=1`, logged with its id) when the file was deleted, the other 16 units kept their ids and the rules hash returned to the original | PASS |
| 33 | `reload` and `replay save` console commands | console typing does not reach egui over BRP; the watcher drove every reload above. `reload_records_replay_event`, `replay_without_reload_verifies`, `replay_with_reload_event_is_non_reproducible` | PASS (test) |
| 34 | Save and load mid-path | 10 units moving (9 on a flow field, 1 on a single path), `fl.save` at tick 3112 (194,820 bytes), 100 ticks → hash H; `fl.load` → tick 3112, the saved hash, the same path states; 100 ticks → H again | PASS |
| 35 | `testbed_ai` runs with player 1 idle | `--scenario base.testbed_ai`: player 1 is `{"Ai": "null"}`; over 200 ticks player 1 issued no command and its units did not move. The scenario was started at launch only: reloading it with `fl.load_scenario` or `fl.load` re-inserts the AI host since Task 19, but that path was not walked live | PASS (startup) |

## BRP smoke (brief Step 6)

| Step | Result |
|---|---|
| `fl.sim_state` until tick ≥ 1 | tick 24 on the first poll after the window opened |
| `fl.select {sim_ids: [1, 2]}` | `{"selected": [1, 2]}`; with `[1, 2, 6, 999]` also `[1, 2]` |
| `fl.issue_command core.move {target: [720, 760]}` for 1 and 2 | `{"exec_tick": 293, "seq": 0, "player": 0}` |
| `fl.step {n: 200}` | `{"tick": 492, "completed": 200, "requested": 200, "finished": true}` |
| `fl.debug_query core.paths {"units": [1, 2]}` | both `following`, 3 corners through the east bridge to their formation slots (705, 760) and (735, 760) |
| `fl.toggle_overlay paths`, `fl.screenshot` | `{"name": "paths", "on": true}`; the overlay is in the offscreen screenshots ([04](demo/m1-04-group-on-bridge.png)) |
| gate | `fl_game/tests/remote.rs::{brp_step_state_command, brp_save_load_and_scenarios}` and `fl_client/tests/remote.rs::brp_*` in the milestone gate's `brp` row |

## Needs a human

- **Edge pan and the mouse feel** (a real cursor at the window edge; `edge_pan_left_edge`).
- **Typing into the console** (`terrain`, `reload`, `replay save`): `brp_extras/type_text` does not reach the egui
  text field; the commands' behaviour is covered by the tests named above.
- **Whether the terrain reads well to a player**, especially the hill (row 1) and the visual notes below.
- **Window-focus behaviour** of mouse input: every click above went to a focused window started with `--agent`.

## Visual notes (parked, not M1 defects)

- The camera's focus sits at height 0, so on the hill and the ridge the view centres slightly off the units (Task 17).
- The terrain mesh ends half a cell inside the map border (Task 17).
- The mesh is triangulated, while units sample the height bilinearly: on slopes a unit can float or sink by a few
  centimetres (row 2: 11.952 vs 12.000 m at the hill top; Task 17).
- Selection rings are flat discs, so on slopes part of a ring dips into the ground (Task 17).
- The 12 m hill has no strong shading at the default light angle, so it is hard to see (row 1).

## Content and determinism

| Check | Command | Result |
|---|---|---|
| Content | `cargo run -p fl_cli --release -- data-check assets/base` | PASS: exit 0, 10 units, 8 terrain kinds, 2 maps, 2 scenarios, no warnings |
| Map layers | `fl_cli map-gen testbed\|scale --out <scratch>` | PASS: all four PNGs byte-identical to `assets/base/maps` |
| Determinism suite, release, three shuffle seeds | `FL_SHUFFLE_SEEDS=3 cargo nextest run --release --profile agent -p fl_sim -p fl_test_utils -E 'binary(movement_determinism) \| binary(determinism) \| binary(golden) \| binary(m0_save) \| test(/shuffle\|twin\|branch\|round_trip\|deterministic/)'` | PASS (filtered release subset, not the brief's full debug run): 55 tests, 54 passed; the one failure, `fl_test_utils::m0_save::m0_testbed_save_loads_under_m1`, asserts on the journal ("a path request within 50 ticks"), which release builds do not record (R139: determinism tests never read the journal; the save itself loads). The full release run: 450 tests, 410 passed, the 40 failures are the known journal-reading tests (Task 14 report) |
| Golden replays | `fl_cli replay-verify` on `testbed_wander.replay` and `testbed_paths.replay` | PASS: `verified: 400 ticks`, `verified: 900 ticks` (exit 0; a build-id warning each) |
| Save/load by hand | `fl_cli save base.testbed --wander 100 --tick 100`, then `load … --ticks 100` | PASS: the final hash equals the uninterrupted `run-scenario` |
| Gates | `cargo xtask gate pr`, `cargo xtask gate milestone --milestone M1` | `gate pr --track docs --track perf --track client --track contract`: PASS in 267 s. `gate milestone --milestone M1`: every row PASS (fmt, clippy-drift, claude-md, clippy, arch-check, test-headless 206 s, test-client 178 s, api, deny, data-check, **gpu** 247 s, **brp** 146 s) except `perf-file`: `docs/perf/M1.json does not exist`, FAIL in 910 s total. The file is `docs/perf/M1.json.new` (3,000-unit mean 4.613 ms ≤ 15) until the lead blesses it; then only the `perf-file` row needs a rerun |

## ADR confirmations

One row per Confirmation bullet of ADR-0012 and ADR-0014…0016. "gate row X" is a row of `cargo xtask gate
milestone --milestone M1`; every named test runs in it (its nextest rows fail on a filter that matches nothing).
Test names are `crate/file::test`.

| ADR | Confirmation bullet | Evidence | Result |
|---|---|---|---|
| 0012 | Value reload keeps ID sets and HP ratios; adding a unit rebuilds with identical `SimId`s; a replay with a reload event is non-reproducible | `fl_sim/tests/reload.rs::{value_reload_keeps_ids_and_hp_ratio, rebuild_drops_vanished_unit_types}`, `fl_game/tests/reload.rs::{value_edit_reloads_in_place, adding_a_unit_rebuilds_with_same_sim_ids, reload_while_paused_applies, malformed_ron_keeps_match_running, removing_a_unit_type_drops_its_units, reload_refused_in_lockstep_match}`, `fl_sim/tests/replay.rs::replay_with_reload_event_is_non_reproducible`; live rows 29–32 | PASS |
| 0014 | Content format | `fl_content/tests/terrain.rs` (the 17 tests the ADR names) | PASS |
| 0014 | Base content | `testbed_layout_matches_plan`, `testbed_scenario_units_on_passable_terrain`, `base_content_loads_terrain`; `data-check` | PASS |
| 0014 | Speed, blocking and clearance | `fl_sim/tests/terrain.rs` (the 12 tests the ADR names) | PASS |
| 0014 | Rules ranges | `movement_rules_ranges_are_validated`, `path_rules_ranges_are_validated` | PASS |
| 0014 | Terrain as Hashed state | `set_terrain_blocks_a_bridge`, `incremental_nav_equals_full_rebuild`, `save_right_after_set_terrain`, `smoke_terrain_twin_and_shuffle`, `terrain_debug_queries`; live row 21 | PASS |
| 0014 | Profiles end to end | `wide_profile_avoids_narrow_gap`, `units_never_enter_blocked_cells`, `milestone_units_never_enter_blocked_cells_long`, `scale_units_start_on_passable_cells` | PASS |
| 0014 | Reload | `terrain_kind_added_changes_id_set`, `value_reload_updates_passability`; live row 30 | PASS |
| 0014 | The client reads `MapLayers`, not PNGs | `terrain_view_from_base_testbed` | PASS |
| 0014 | `arch-check` `png` rule; `cargo deny` accepts `png` | gate rows `arch-check` (`png_direct_dep_only_in_content`) and `deny` (`gate pr`) | PASS |
| 0015 | Search | the 7 Task 6 tests the ADR names | PASS |
| 0015 | HPA* and the R136 cost bound | the Task 7 and 7b tests the ADR names | PASS |
| 0015 | Queue, budget and order | the Task 8 tests the ADR names; `path_budget_holds_at_scale` | PASS |
| 0015 | Flow fields, outcome-neutral caches, tick-boundary eviction | the Task 10 and 10b tests the ADR names; perf scenario `flow_thrash` (`docs/perf/M1.md`) | PASS |
| 0015 | Derived rebuild; mid-pathing saves | `core_registers_one_rebuild`, `incremental_nav_equals_full_rebuild`, `save_right_after_set_terrain`, `m1_save_round_trip_mid_pathing`, `m1_branch_mid_pathing`, `m1_scenarios_shuffle_invariant`; live row 34 | PASS |
| 0015 | Per-tick scratch | `steer_pushes_are_per_tick_and_sorted`, `idle_steering_is_staggered`, `units_never_enter_blocked_cells` | PASS |
| 0015 | Debug queries and their consumers | the Task 2, 5, 7, 8 and 18 tests the ADR names, `brp_debug_query_round_trip` (gate row `brp`); live: `fl.debug_query`, `fl_cli query`, the F5–F7 overlays | PASS |
| 0015 | Each new core system in its hook | `fl_test_utils/tests/system_order.rs::sim_tick_system_order` (blessed snapshot) | PASS |
| 0016 | Types | `fl_view/tests/view.rs::{player_view_splits_own_and_visible, null_ai_issues_nothing, ai_controller_by_id}` | PASS |
| 0016 | Hosting | `fl_client/tests/ai.rs::{null_ai_issues_nothing, ai_thinks_once_per_new_tick, draft_for_another_player_is_dropped}`, `fl_game/tests/ai.rs::{ai_scenario_builds_host, unknown_ai_is_error}` | PASS |
| 0016 | `arch-check`: `fl_view` has no Bevy, no client crate depends on `fl_sim` | gate row `arch-check` | PASS |
| 0016 | Demo row "`testbed_ai` runs with player 1 idle" | row 35 | PASS (startup) |

M0 rows that M1 changed:

| ADR | M0 row | M1 evidence | Result |
|---|---|---|---|
| 0002 | Render stress, 3,000 placeholders ≥ 60 fps (M0: owed an idle rerun) | M1 render stress on the terrain mesh, `docs/perf/M1.md` | PASS: avg_fps 88.0 / 70.6 / 71.1 / 74.1 (≥ 60) on the terrain map; 1 % lows ≈ 2 fps in M1 and in the `m0`-tag game alike; interleaved with the `m0`-tag game at equal load, M1 is level (59.9 / 61.2 vs 58.5 / 60.3 fps; `docs/perf/M1.md`); a fully idle pair is still owed |
| 0004 | Loading a replay recorded with an extra registered name (M0: PARTIAL, deferred to C8) | replays and saves map command ids through their recorded string table (R73/I1): `fl_sim/tests/replay.rs::replay_remaps_command_ids_by_name`, `fl_sim/tests/save.rs::pending_command_ids_remap_on_load`; M1's new `core.follow` sorts before `core.move` | PASS (C8 still owns the replay-versioning ADR) |
| 0010 | Milestone gate requires `docs/perf/M<n>.json` | gate row `perf-file` on `docs/perf/M1.json` (the bless of `M1.json.new` is the lead's) | PENDING the bless of `docs/perf/M1.json.new` (G7 holds in it: 4.613 ms at 3,000 units) |
| 0012 | M0: "deferred: hot reload is M1" | the ADR-0012 row above | PASS |
