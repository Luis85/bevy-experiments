# M1 plan pre-flight (aligned with the M0 final fix waves)

Plan: `docs/superpowers/plans/2026-10-24-frontline-m1.md`. It was written against M0 at `56e3425` and is now aligned with
`56e3425..f247a85` (fix waves A and B, rulings R69–R77, spec revision 2.2, ADR-0005/0006/0007 refinements,
`.claude/rules/sim.md`). Each fact was checked against the code: `fl_sim` API snapshot, `commands.rs`, `classify.rs`,
`feature.rs`, `spatial.rs`, `scale.rs`, `save/load.rs`, `fl_protocol/header.rs`, `xtask/src/{arch,gate,api}.rs`,
`xtask/clippy-template.toml`, `OWNERS.toml`, the crate manifests and the `fl_client` test fake runners.

## Scan: pairs of tasks sharing a file or interface

| # | Producer → consumer | Shared file or interface | Finding | Status |
|---|---|---|---|---|
| P1 | T1 → T2–T21 | ADR-0007 `## Refinements (2026-10-24)` | Wave A already added a section with that heading. | Fixed: T1 writes `## Refinements (2026-10-24, M1)`. |
| P2 | T1 → spec | Spec revision notes | T1 Step 3 added "Revision 2.1", which already exists (M0); 2.2 is the final M0 review. | Fixed: `## Revision 2.3 (2026-10-24, M1 contracts)` above 2.2, plus the status line. |
| P3 | T1 → T12 | ADR-0012 refinement ↔ `ToRunner` | `ToRunner` isn't `#[non_exhaustive]`, and `fl_client/tests/{debug,view}.rs` match it exhaustively. | Fixed: the refinement and T12 make it `#[non_exhaustive]`; see P20. |
| P4 | T1 → T5, T9, T21 | ADR-0007 M1 refinement ↔ R71/R77 | The plan added a `RejectReason::InvalidTarget` variant, and the caps text said nothing about `MoveOrder.orders`. | Fixed: `RejectReason::Feature("core.invalid_target")`; `MAX_WAYPOINTS` bounds `orders` (every kind). |
| P5 | T2 → T5, T7, T8 | `register_debug_query`, `DebugQueries` | Consistent. | — |
| P6 | T2 → T9, T16 | `RotateCmd`, `FollowCmd` | Consistent. New command names shift dense ids; saves and replays remap them by name (R73). | Noted in Global Constraints. |
| P7 | T2 → T18, T20 | `SimQuery::Debug`/`SimReply::Debug`, `PlayerView`, `AiController` | Consistent (`SimQuery`/`SimReply` are `#[non_exhaustive]`). | — |
| P8 | M0 wave A → T2 | `fl_cli query` replay source | It used `replay_app` alone; recorded command ids must go through `replay_inputs` (I1). | Fixed: T2 Step 3. |
| P9 | M0 wave B → T2, T3, T12 | `xtask/snapshots/api/*.txt` | `fl_content` is now an API crate: "api: 4 crates unchanged" was wrong, and T3/T12 did not bless `fl_content.txt`. | Fixed: 5 crates; `fl_content.txt` in T3 and T12 Blessed. |
| P10 | T2 ↔ T12 | `fl_view/src/runner.rs`, `fl_sim/src/runner/mod.rs` | Both lane 1, sequential. | OK |
| P11 | T3 → T4, T5, T17 | `MapTerrain`, `MapLayers`, `write_layers`, `MapDef.layers`, `TerrainDef`, `MoveClass` | Consistent. Only `fl_content` decodes PNGs; the client reads `MapDef.layers`. | Made explicit (Global Constraints, ADR-0014). |
| P12 | T3 ↔ arch-check | `png` in `fl_content` and `fl_sim`'s tree | Allowed: the `Content` denylist and the `Sim` allowlist constrain only `bevy*` crates, and `png` and its dependencies are pure Rust. No sim crate depends on `png` directly. | Decision recorded; T3 Step 6 expects arch-check OK. |
| P13 | T4 ∥ T5 | `testbed_wander.replay`, `assets/base/**` | Both re-record the golden in parallel lanes. The lead regenerates it after the second merge (existing rule); T4 owns maps and terrain, T5 owns `rules.ron`. | OK |
| P14 | M0 R74 → T5, T7, T10, T12 | `register_rebuild` (one per feature; a second call panics) | Core already registers `rebuild_spatial`, and the plan gave `NavGrid` a separate `rebuild_nav` plus "its own rebuild" wording for `HpaGraphs`/`FlowCache`. | Fixed: core's one `rebuild_core` (spatial, then nav) in `feature.rs`; T5 Files += `feature.rs`, `spatial.rs`. |
| P15 | T5 → T6 | `crates/fl_sim/src/lib.rs` `pub mod nav` | T5 creates `nav/{mod,grid}.rs`, but T6 claimed to add `pub mod nav`. | Fixed: T5 declares it; T6 adds re-exports. |
| P16 | T5 → T6, T7, T8, T10, T11, T13 | `NavGrid`, `MoveProfile`, `CellRect`, `MovementRules`/`PathRules`/`SteeringRules` fields | Every field consumed later is produced in T5. | OK |
| P17 | T5–T12 tests → `fl_sim` manifest | `fl_test_utils` helpers in `crates/fl_sim/tests/*.rs` | `fl_sim` has no dev-dependency on `fl_test_utils`, and no task's Files added one. | Fixed: T5 adds it (arch-check allows dev edges between non-client crates); integration tests only. |
| P18 | T5 ↔ M0 `debug.spawn` | Handler doc: "an applied command always spawns" | A blocked cell was checked only in the funnel. | Fixed: the handler rejects `core.invalid_target` first; the funnel keeps `SpawnError::BlockedCell`. |
| P19 | T5 ↔ T13 | `crates/fl_sim/src/scale.rs` (`init_terrain`) | Ownership handed over after T5 (table). | OK |
| P20 | T12 → `fl_client` tests | `ToRunner::Control` | `fl_client/tests/{debug,view}.rs` match `ToRunner` exhaustively, so T12 Step 4's claim that `fl_client` compiles unchanged was false. | Fixed: T12 Files/track += client, wildcard arms; Step 4 reworded. |
| P21 | T6 → T7 | `astar`, `dijkstra_from`, `line_clear`, `smooth`, `nearest_passable` | Consistent. | — |
| P22 | T7 → T8, T10 | `plan_path`, `PathWork`, `PlanStatus`, `HpaGraphs`, `ClusterId` | Consistent. | — |
| P23 | T7 → T18 | `core.clusters` args | T18 sends `{"unit": id}`, but T7 accepted only `{"class","size"}`. | Fixed: T7 accepts both; test extended. |
| P24 | T8 → T9, T10, T18 | `Order`, `enqueue_path`, `PathFollow`, `FlowKey`, `RequestKind::Group`, `PathState::Flow`, `core.paths` JSON | Consistent (every variant is defined in T8, so later tasks don't change codecs). | — |
| P25 | M0 wave A → T8, T9 | `MAX_WAYPOINTS` (`MoveOrder.waypoints`) | The cap must move to `orders`, and queued rotate/follow must obey it. | Fixed: T8 handler and ported test; T9 rule and test `queued_orders_share_the_cap`. |
| P26 | M0 wave A → T8 | `"debug.wander"` `Stream` via `with_input` | The plan moved wander into `advance_orders` without the token, and its system placement was ambiguous. | Fixed: `wander_system` keeps its `Stream` and its draws, chained after `advance_orders` in `ThinkSet::Orders`. |
| P27 | M0 tests → T8 | `core.move_order` journal kind, `MoveOrder.waypoints` | `observe.rs`, `runner.rs` and `debug_cmds.rs` use them but weren't in T8's Files. | Fixed: T8 Files. |
| P28 | M0 wave A → T5, T8, Global Constraints | Codec snapshot | Codec blocks moved to `fl_test_utils/tests/snapshots/formats__save_codecs.snap`; `fl_sim`'s snapshot keeps only the layouts. | Fixed: Blessed lists and the protected list. |
| P29 | M0 wave A → T8 | `core.move_order@2` | `migrate_from(1)` satisfies the format guard (R70). | OK |
| P30 | M0 R72/R76 → T11 | `Steer` Transient component | An archetype move per unit per tick. | Fixed: `SteerPushes`, a `TransientReset` resource sorted by `SimId`; test added. |
| P31 | T8, T11, T13 → budgets | `docs/perf/budgets.ron` | `check_budgets` fails a budgeted counter a run doesn't measure; T13 claimed `flowfield.*` budgets, which the scale mix never measures (no group moves). | Fixed: T13 records measured counters only. |
| P32 | T12 → T19 | `apply_content_values`, `load_world_with`, `LoadOptions`, `same_id_sets`, `RunnerControl` | Consistent. The save leg can answer `SimReply::Error` (`SaveError`). | Fixed: T19 maps it to `Failed`. |
| P33 | T12 ∥ T13 | Ownership table | No overlap after the additions (T12: `fl_client` tests; T13: `fl_game` builder). | OK |
| P34 | T13 ↔ M0 tests | `ScaleScenario::default()` on fixture mods (movement, replay, runner, spatial, determinism, benches) | A default of `map: Some("base.scale_2km")` would make them fail with `UnknownMap` or a size mismatch. | Fixed: the default stays open; `ScaleScenario::on_terrain` opts in; bench, perf and stress use it. |
| P35 | T13 → T21, via T17 | Stress run "(terrain mesh on)" | No task gave the stress mode a `MapView.terrain`. | Fixed: T13 sets it in `fl_game/src/builder.rs`; test `stress_map_view_has_scale_terrain`. |
| P36 | T13 → T14 | Scale shuffle scenario | It used `with_units(500)`, which is now the open map. | Fixed: `on_terrain(500)`. |
| P37 | T13 ↔ T19 | `fl_game/src/builder.rs` | Lane 2, sequential (13 → 19). | OK |
| P38 | T15 → T16, T18 | `Selection`, `ClickedUnit`, `debug/remote.rs`, `tests/remote.rs` | Consistent; lane 2, sequential. | OK |
| P39 | T16 ↔ T19 | `debug/console.rs` | Lane 2, sequential. | OK |
| P40 | T17 → T13, T19 | `TerrainView`, `fl_client/src/view.rs` | T19's `InputLog` is appended by `drive_runner` in `view.rs`, which wasn't in T19's Files. | Fixed: T19 Files and table. |
| P41 | T17, T20, T13, T19 | `fl_game/src/builder.rs` | Lane 2, sequential. | OK |
| P42 | client clippy → T19 | `ContentWatcher` mtimes | The client profile bans the `std::time::SystemTime` type. | Fixed: keep a `Duration` since `UNIX_EPOCH`. |
| P43 | M0 wave B → T19 | `debug.ron` log filter | It must keep `fl_sim::spawn=info`; `repo_debug_ron_parses` pins it. | Noted in T19 Files. |
| P44 | T14 ∥ T19 | Ownership table | No overlap. | OK |
| P45 | M0 wave B → T21 | `docs/perf/M*.json` protected; `perf-file` checks G7 on the first 3000-unit row | T21 wrote `M1.json` directly and appended the `--flat` rows to it. | Fixed: `M1.json.new` with the terrain rows only, lead bless before Step 8; the flat table goes to `M1.md`. |
| P46 | M0 wave B → T21 | Milestone and device gate rows fail on zero matches | The `brp_` tests are in T15 and T18, the `milestone_` tests in T11 and T13. | Noted in T21 Step 8. |
| P47 | M0 R70 → T5, T8 | `check_save` lets an M0 save load; `load_world` removes `Hashed` resources the save lacks | M1 systems that take `Res<TerrainGrid>`/`ResMut<PathQueue>` would panic on an M0 save. | **Open: controller ruling** (Global Constraints, Preconditions). |
| P48 | M0 build.rs → T14 | `build_id` = git revision | Golden replays recorded at another commit verify with a warning (R70). | Noted in T14 Step 4. |

## Scan: each task's internal consistency

| Task | Check | Finding | Status |
|---|---|---|---|
| T1 | Files vs steps; refinements | Heading collision and revision number (P1, P2). | Fixed |
| T2 | Tests vs interfaces; Step 6 expected output | `api: 4` → `5` (P9); `PlayerView` visibility test matches the bit rule. | Fixed |
| T3 | Blessed vs API changes; tests vs pixel mapping | `fl_content.txt` missing (P9); `layers_are_north_up` (1, 0) → (1, 2) matches `cells_y − 1 − py`. | Fixed |
| T4 | Layout vs `testbed_layout_matches_plan` | Overlapping roads, river and bridges had no paint order; (71, 124) as a bridge needs bridges painted last. | Fixed: paint order and bridge height stated |
| T5 | Tests vs `NavGrid` | `PartialEq` derived over the observe-only `version` broke `save_right_after_set_terrain` and `incremental_nav_equals_full_rebuild`. | Fixed: hand `PartialEq` ignoring `version` |
| T5 | Files created vs touched | `feature.rs`, `spatial.rs`, the `fl_sim` manifest and `Cargo.lock` missing (P14, P17); `fl_test_utils` "dev → normal tempfile", though it has no `tempfile` at all. | Fixed |
| T5 | Speed formula values | (0.8, −0.18) → 191/250, (0.7, 0.2) → 19/25 and the clamp to 1/20 check out. | OK |
| T6 | Pinned test | `astar_open_grid_pinned` traced by hand with the (f, h, y, x) tie-break: path and cost 341 hold. Its Files duplicated `pub mod nav` (P15). | Fixed (Files) |
| T7 | Pinned test; Blessed | 32×32 open map → 4 borders × 2 transitions × 2 nodes = 16 ✓. "save formats if any" → none (`Derived`). | Fixed (wording) |
| T8 | Files vs ported tests | P27. `MoveOrder` cap (P25) and wander (P26). | Fixed |
| T9 | `formation_targets_pinned` | (0,0),(10,0) → (95,50),(105,50) ✓. Cap test added. | Fixed |
| T10 | Flow tests vs group threshold 6 | 8 → group, 5 → single, 6 + 6 → 2 groups ✓. `FlowCache` wording (P14), `BTreeMap`. | Fixed (wording) |
| T11 | Interface vs R72/R76 | P30. | Fixed |
| T12 | Step 4 claim vs code | P20; `rebuild_derived` was private (made `pub(crate)`). | Fixed |
| T13 | Default vs existing tests; gate tracks | P34, P35; tracks += client. | Fixed |
| T14 | Scenario list | P36; `build_id` note (P48). | Fixed |
| T15 | Threshold and double-click tests | 5 px < 6 → Click, 6 px → Box; 200 ms < 350, 400 ms > 350 ✓. | OK |
| T16 | Speed steps | 1 → 2 → 4 → 8 → 8 and down to 0.25 match `SPEED_STEPS`. | OK |
| T17 | `terrain_mesh_counts` | A 4×3 view: 12 vertices, 6 quads → 36 indices ✓. | OK |
| T18 | `grid_lines_count` | Multiples of 4 in [40, 60] are 6 lines per axis, not 5. | Fixed |
| T19 | Files vs `InputLog`; clippy | P40, P42; `SaveError` reply (P32). | Fixed |
| T20 | Tests vs `Controller::Ai` | `Controller` is `#[non_exhaustive]` with `Ai(String)`; matching with `if let` works. | OK |
| T21 | Protected output; gate rows | P45, P46; reject codes added to the `debugging.md` list. | Fixed |

## Change list (plan edits)

1. Inputs: spec revisions 2/2.1/2.2, ADR refinements, rulings R20–R77 (R69, R70–R74, R76, R77), `.claude/rules/sim.md`, fix-wave reports.
2. Lane 1 is cut after the final fix waves (`f247a85`+); the Preconditions say the same, plus a ruling on M0 saves before T5.
3. Dependencies: PNG decoding only in `fl_content` (arch-check reasoning); `fl_sim` dev-dependency on `fl_test_utils` (T5); `tempfile` as a normal dependency of `fl_test_utils`; `build_id` rebuild note.
4. Determinism constraints: R72 Transient rules, no per-tick Transient components (R76), scratch buffers as `TransientReset`, messages cleared at the end of a tick, one Derived rebuild per feature (`rebuild_core`), `Stream` tokens, hash-collection bans, spawn funnel/`SimEntity`, reject codes (`core.invalid_target` as `Feature`), caps (R77), remap by name, R70 save compatibility and `SaveError`, `SIM_VERSION` location, the open question on M0 saves.
5. Protected artifacts: `formats__save_codecs.snap`, 5 API crates, `docs/perf/M*.json`, the measured-budget rule.
6. Client: clippy bans on `Instant`/`SystemTime`/std hash collections. Observability: per-request logs at `trace` (wave B's spawn-log lesson).
7. Ownership table: `Cargo.lock` (T5), `fl_view/src/lib.rs` and `fl_client/tests/{debug,view}.rs` (T12), `fl_sim/tests/replay.rs` and `fl_game` builder and tests (T13), `fl_client/src/view.rs` (T19).
8. T1: new refinement headings, ADR-0007 M1 bullets (remap, caps over `orders`, `core.invalid_target`), ADR-0012 bullets (`#[non_exhaustive]` `ToRunner`, rebuilds after a reload), ADR-0014 PNG location, ADR-0015 rebuild and scratch resource, spec Revision 2.3.
9. T2: `replay_inputs` in `fl_cli query`; `api: 5 crates unchanged`.
10. T3: bless `fl_content.txt`; arch-check expectation.
11. T4: paint order and bridge height.
12. T5: Files (`feature`, `spatial`, manifest, lock, `lib` `pub mod nav`); `rebuild_core`; `NavGrid` `PartialEq` without `version`; `core.invalid_target`; `debug.spawn` check; test codes; new test `core_registers_one_rebuild`; classification.
13. T6: Files (no duplicate `pub mod nav`).
14. T7: `HpaGraphs` rebuilt inside `rebuild_nav`; no save formats; `core.clusters` takes `{"unit"}`; test extended.
15. T8: Refs += ADR-0007; Files += `debug_cmds.rs`, `observe.rs`, `runner.rs`; codec snapshot path; `MoveOrder` cap over `orders`; wander system placement and `Stream`; ported cap test.
16. T9: `core.invalid_target` code; queued rotate and follow obey the cap; new test.
17. T10: `FlowCache` reset in `rebuild_nav`, `BTreeMap`.
18. T11: `Steer` component → `SteerPushes` Transient resource; new test.
19. T12: track += client; Files += `fl_client` tests; `ToRunner` `#[non_exhaustive]`; `rebuild_derived` `pub(crate)`; `check_save` ignores the rules hash; `save_world` `Result`; Step 4 and the gate fixed.
20. T13: `ScaleScenario` default open; `on_terrain`/`SCALE_MAP`; placement redraws from setup `Rolls`; no `flowfield.*` budgets; `fl_game` stress terrain plus test; Refs += ADR-0006; tracks += client.
21. T14: `on_terrain(500)`; `build_id` warning note.
22. T18: `grid_lines_count` 6 + 6.
23. T19: Files += `view.rs`, the `debug.ron` filter; watcher mtime without the `SystemTime` type; save `SimReply::Error` → `Failed`.
24. T21: `M1.json.new` plus lead bless; flat rows only in `M1.md`; `meta.head`; reject codes in `debugging.md`; zero-match gate rows.
25. Builder's calls and self-review: the decisions above and the new type names.

## Needs a controller ruling

1. **M0 saves under M1 (P47).** R70 lets an M0 save load, and `load_world` removes the `Hashed` resources the save lacks (`core.terrain`, `core.path_queue`), so M1 systems would panic. Options:
   - (a) M1 systems tolerate their absence (`Option<Res>`), and `rebuild_nav` treats a missing grid as open;
   - (b) a framework rule that inserts `Default` for a `Hashed` resource the save lacks;
   - (c) declare saves older than M1 unsupported, with a header rule.
2. **`core.invalid_target` as `RejectReason::Feature`.** It is a core reason under the `Feature` variant, chosen per "codes, not variants" (R71). Confirm, or allow one core variant.
3. **`ToRunner` `#[non_exhaustive]`** (contract change in T12 / ADR-0012 refinement). Confirm.
