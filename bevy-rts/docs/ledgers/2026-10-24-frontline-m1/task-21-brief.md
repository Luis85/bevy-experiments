### Task 21: Integration — demo, milestone gate, perf, debugging guide

**Track:** docs + perf · **Refs:** ADR-0009, ADR-0010, ADR-0012, ADR-0014–0016 · **Lane 1** (after every task is merged)

**Files:**
- Create: `docs/m1-demo-checklist.md`, `docs/perf/M1.md`, `docs/perf/M1.json.new` (protected: the lead blesses it into `docs/perf/M1.json`)
- Modify: `docs/debugging.md`, `README.md` (M1 status line)

- [ ] **Step 1: Merge and gate.** With every branch merged into `m1/phase-a`, run `cargo xtask gate pr`. Expected: `gate pr: PASS`.
- [ ] **Step 2: Update `docs/debugging.md`.** Add:
  - every new journal kind, counter and span
  - the reject codes M1 uses (`core.invalid_target`, `core.queue_full` for every queued order kind), as `SimEvent::CommandRejected` and the `core.command_rejected` journal record report them
  - the five debug queries with request and response JSON
  - `fl_cli query` and `fl_cli map-gen`, each with a `--json` example
  - BRP `fl.debug_query`, `fl.select` and `fl.selection`, with example JSON
  - the console commands `reload` and `replay save`
  - the `debug.ron` keys `hot_reload` and `hot_reload_poll_ms`
  - the overlay keys F5/F6/F7 and the order and testbed hotkeys
  - a "why didn't my unit move" recipe: `core.paths` → `journal --kind core.path_result --sim-id N` → `core.passability {unit}` → `core.terrain_at`

  Run every documented command and check its output matches the document.
- [ ] **Step 3: Content and determinism.**
  - `cargo run -p fl_cli --release -- data-check assets/base` → exit 0, `10 units`, 8 terrain kinds.
  - `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_sim -p fl_test_utils` → PASS.
  - `replay-verify` on both golden replays → exit 0.
- [ ] **Step 4: Perf (tier 3).** Ask the user to confirm the fixed power plan first.
  - Run `cargo run -p fl_cli --release -- bench --units 500,3000,5000 --ticks 400 --warmup 40 --repeat 5 --out docs/perf/M1.md --json docs/perf/M1.json.new` (on terrain).
  - Run the same with `--flat`, writing `--out <scratch>/m1-flat.md --json <scratch>/m1-flat.json`. Paste its table into `M1.md` as a second table. Never write it into `M1.json`: the gate's `perf-file` row reads the first `rows` entry with `units` 3000, which must be the terrain run.
  - Expected: the 3000-unit mean is **≤ 15 ms** with pathing on terrain (G7). `gate milestone`'s `perf-file` row checks this from `docs/perf/M1.json`.
  - Expected: the `--flat` row is within 20% of `docs/perf/M0.json` at every size. If it isn't, write the justification into `M1.md` naming the systems responsible (from `--profile`), or fix it.
  - Add the machine, rustc, commit and power plan to `M1.md`. `M1.json.new`'s `meta` names the measured commit as `"head"`, as `M0.json`'s does. If `bench` doesn't write it, add it to the `.new` file before the bless.
  - The lead blesses `docs/perf/M1.json` (`cargo xtask bless docs/perf/M1.json --reason "M1 Task 21: M1 perf baseline"`) before Step 8, because the milestone gate fails while it is missing.
- [ ] **Step 5: Render stress.** `cargo run -p fl_game --release -- --stress 3000 --exit-after 25 --agent` (terrain mesh on). Expected: `avg_fps` ≥ 60. Append the line to `docs/perf/M1.md`.
- [ ] **Step 6: BRP smoke through `bevy_brp_mcp`.** Launch `cargo run -p fl_game --features debug_remote -- --remote --remote-port 15702 --agent`, then:
  - poll `fl.sim_state` until tick ≥ 1
  - `fl.select {sim_ids: [1, 2]}`
  - `fl.issue_command` `core.move` to (720, 760)
  - `fl.step {n: 200}`
  - `fl.debug_query core.paths {"units": [1, 2]}`
  - `fl.toggle_overlay paths`, then `fl.screenshot`

  Record the results in the checklist.
- [ ] **Step 7: Demo checklist.** Write `docs/m1-demo-checklist.md` and walk it with `cargo run -p fl_game --release`:
  - The terrain shows the river, two bridges, roads, forest, the hill and the ridge.
  - Units stand on the ground.
  - Click, box, Shift-add, double-click on a Pz III, Ctrl+1 / 1 / Shift+1 / Alt+1.
  - Right-click move across the river goes over a bridge; the tanks turn before driving; the group keeps its shape; Shift-queue works.
  - H stops, R+click rotates, a right-click on an own unit follows.
  - `debug.set_terrain` (console `terrain 70 122 72 127 base.water`) on the bridge in use makes the units repath.
  - F5/F6/F7 overlays show paths, blocked cells for the selected class, cells and clusters.
  - Pause (P), give orders, resume: the orders apply; `=`/`-` change the speed; F4 switches the player.
  - Editing `pz3.unit.ron` HP reloads in place; adding a unit file rebuilds; a broken file shows an error and the match continues.
  - `testbed_ai` runs with player 1 idle.
  - ADR confirmations: one row per Confirmation bullet of ADR-0012 and ADR-0014–0016, naming the passing test or gate step, plus any M0 rows M1 changed.
- [ ] **Step 8: Milestone gate.** Run `cargo xtask gate milestone --milestone M1`. Expected: `gate milestone: PASS`. At milestone level every nextest row, and the `gpu` and `brp` rows, fail when their filter matches no test. So a renamed or `cfg`'d-out `milestone_`, `gpu_` or `brp_` test fails the gate. M1's `brp_` tests are in Tasks 15 and 18, and its `milestone_` tests in Tasks 11 and 13.
- [ ] **Step 9: Review and commit.** Run the final whole-branch review (`superpowers:requesting-code-review`), fix the findings, and re-run `cargo xtask gate pr`. Commit `docs: M1 debugging guide, demo checklist and perf results`. The controller fast-forwards `main`.

---

## Builder's calls and deviations recorded here

- **Hot reload watcher.** Content is loaded with `std::fs`, so the testbed polls mtimes instead of Bevy's `file_watcher` (spec §6.4; ADR-0012 refinement).
- **In-house pathfinding.** No `pathfinding` crate: budget and expansion counting need our own loop, and nothing is downloaded (spec §7 says "can", not "must").
- **Path cost per class.** It uses `Movement.path_terrain_factor`, because unit types in one class differ and the graphs are shared per class. The unit's own `terrain_factor` drives its actual speed.
- **`terrain_factor` range** widens to [−1, 1], clamped by `min_speed_frac` (research §1.8: trucks are negative).
- **Height** drives per-class slope blocking (a guess in `rules.ron`) and rendering. Vision height bonuses come in M3.
- **Bridges** are terrain cells in M1, changed by `debug.set_terrain`. That exercises the dynamic-obstacle path (cluster invalidation). Destructible bridge objects with HP arrive with combat and engineering.
- **Order queue.** `MoveOrder` v2 holds a closed `Order` enum (`Move`, `Rotate`, `Follow`) as an interim. C7's registry-based order queue and activity stack replace it in the M2 prelude.
- **No path-result cache.** Only flow fields and HPA*'s intra-cluster paths are cached. The cold-cost charge keeps both outcome-neutral.
- **`PlayerView`** is built client-side from the snapshot until vision (M3) builds it in `Publish`.
- **The client records replays** (`replay save`), so ADR-0012's reload events have a home.
- **Key bindings** (H, R, F, P, `=`, `-`, F4, F5–F7) are provisional until M6's command grid.
- **No new unit types.** The base roster already covers Foot, Wheeled and Tracked. HalfTrack has no iteration-1 unit (spec §9.1), so the ASCII fixtures exercise it and wide clearance.
- **Golden replays and budgets move in several tasks.** Each re-records and the lead blesses, rather than one rebaseline at the end, so every commit's gate passes.
- **PNG decoding in `fl_content`.** It is the content loader, already does file I/O at load, and `arch-check` gives it the `Content` role (`bevy_ecs` family only). `png` is pure Rust with no Bevy crate. Sim crates receive integer layers, never images.
- **`core.invalid_target` is a code, not a variant.** It is `RejectReason::Feature("core.invalid_target")` (R71: add no variants; ruled R79), with the same stable code a variant would report.
- **M0 saves load under M1 (R78).** `Codec::fill_missing` (Task 5) fills `core.terrain`, `core.path_queue` and `core.path_follow` for older core saves. A protected M0 testbed save fixture, generated from `main`'s M0 tip, proves it. Saves carry no map id, so the terrain fill picks the content map whose size and cell match the saved `MapInfo`. Without a fill, the code's current behaviour stays: a missing Hashed resource is removed. R78 says "LoadError", but saves without a map rely on the removal.
- **Steering output is a scratch resource** (`SteerPushes`, Transient, sorted by `SimId`), not a Transient component (R76).
- **Core's one Derived rebuild** is `rebuild_core` (spatial, then nav), because a feature may register only one (R74).
- **`ScaleScenario` stays on the open map by default.** `ScaleScenario::on_terrain` opts in, so the M0 tests that build `ScaleScenario::default()` on fixture mods keep working. Bench, perf counters and the `fl_game` stress mode use the terrain.
- **`ToRunner` becomes `#[non_exhaustive]`** when it gains `Control` (Task 12), like `SimQuery`/`SimReply` (ruled R80; ADR-0012 refinement).
- **Cache reads never observe a miss (R84).** `FlowCache::direction` builds an evicted integration or tile before answering, so the eviction policy can't change outcomes; `flow_cache_overflow_does_not_change_outcomes` (Task 10) checks it with more concurrent keys than `flow_cache_max`. Read-miss rebuilds are never charged to the path budget, so their cost is bounded by `flow_cache_max` (how many keys stay warm), not by the budget; Task 13 measures a thrash with more live keys than `flow_cache_max` (R112).
- **Task 1 review fixes.** ADR-0014 pins the PNG formats, the unit anchor cell, the speed and cost evaluation and the rules ranges; the matching tests are in Tasks 3, 5, 12 and 19, and the `png` dependency rule is an `arch-check` rule from Task 3.

## Self-review

**Spec coverage** (M1 scope → task):

| Spec item | Task(s) |
|---|---|
| §7 terrain grid: height and terrain-type PNG layers, road, forest, river, bridge | 3, 4, 5 |
| §7 speed formula; movement-class blocking; clearance | 5 |
| §5.7 / §7 HPA* (16×16, per class + clearance); A* refinement | 6, 7 |
| §5.7 budgeted request queue, results in request order | 8 |
| §5.7 flow fields cached per (destination cluster, class) | 10 |
| §5.7 dynamic obstacles invalidate only touched clusters | 5, 7, 8, 10 |
| §7 steering and separation; hull rotation; formation offsets | 11, 8, 9 |
| §9.1 / §10 selection: click, box (`world_to_viewport`), Shift-add, double-click, control groups | 15 |
| §9.1 / §5.5 commands Move, Stop, Rotate, Follow, Shift-queue | 2, 8, 9, 16 |
| §11 testbed: pause with orders, speed, player switch | 16 (verified and extended) |
| §11 overlays: paths, passability, cells | 18 (queries in 5, 7, 8) |
| §6.4 / ADR-0012 hot reload | 12, 19 |
| §8 AI seam: `PlayerView` stub, do-nothing `Controller::Ai` | 2, 20 |
| §15 journal kinds, counters, spans; `docs/debugging.md` | 5, 7, 8, 10, 11, 12; 21 |
| §12.6 work-counter budgets; M1 bench; G7 | 8, 11, 13, 21 |
| §12.3 twin and shuffle, save and branch on pathing, golden replay | every sim task, 14 |
| §13.2 contracts through ADRs | 1, 2, 3, 12 |

- **Type consistency.** These names are checked across tasks: `MoveProfile { class, size }`, `CellRect`, `CellXY`, `NavGrid::{passable, speed_factor, cost, cell_of}`, `HpaGraphs::{build, invalidate}`, `plan_path` → `PathPlan`, `PathWork::charge`, `PathFollow`, `PathState::{Idle, Waiting, Following, Flow, Done, Failed}`, `FlowKey { cluster: ClusterId, profile }`, `PathQueue`, `RequestKind::{Single, Group}`, `MoveOrder { orders }`, `Order::{Move, Rotate, Follow}`, `SteerPushes`, `rebuild_core` / `rebuild_nav`, `ScaleScenario::on_terrain`, `RejectReason::Feature("core.invalid_target")`, `SimQuery::Debug` / `SimReply::Debug`, `RunnerControl::ReloadContent(OpaqueContent)`, `SimReply::Reloaded`, `LoadOptions`, `LoadRequest::Rebuild`, `Selection`, `ClickedUnit`, `TerrainView`, `MapView.terrain`.
- **Proportion.** The plan carries signatures, rules values, layouts and test assertions. Algorithm bodies are left to the implementer, apart from the tie-break, entrance and charge rules, which are contracts (ADR-0015).
