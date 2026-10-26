# Planner report: M1 (Control & Movement)

Plan: `docs/superpowers/plans/2026-10-24-frontline-m1.md` (1475 lines, 21 tasks).

## Tasks and topology

21 tasks. At most two builders: lane 1 is `C:\fl\a` on `m1/phase-a`; lane 2 is one worktree at a time, `C:\fl\m1tNN`, building in the shared dir `C:/t/fl/28/2710d09957a748` (R31).

- **Phase I.**
  - Lane 1: 1 (ADRs, docs only, no cargo) → 2 (payloads, debug queries, AI seam types) → 3 (terrain content format, PNG layers).
  - Lane 2: 15 (selection; needs no contracts) → 4 (base terrain kinds, `fl_cli map-gen`, testbed and scale layers; after 3).
- **Phase II.**
  - Lane 1: 5 (TerrainGrid, NavGrid, speed formula, `debug.set_terrain`) → 6 (A*, LOS) → 7 (HPA*) → 8 (path queue, MoveOrder v2, following) → 9 (rotate, follow, formation) → 10 (flow fields) → 11 (steering).
  - Lane 2: 16 (orders input, testbed keys) → 17 (terrain mesh, picking) → 20 (AI host) → 18 (overlays, `fl.debug_query`; after 8).
- **Phase III.**
  - Lane 1: 12 (hot reload, sim side) → 14 (determinism suite, `testbed_paths` golden).
  - Lane 2: 13 (scale mix on terrain, budgets, bench) → 19 (hot reload, game side; replay capture).
- **Phase IV.** Lane 1: 21 (integration, demo checklist, `docs/perf/M1.*`, `docs/debugging.md`).

The plan's topology table lists the files each lane owns for every parallel pair. The only expected conflicts are re-export lines in `lib.rs`. The lead regenerates protected snapshots that both lanes moved, after the second merge.

## New contracts (ADR task 1)

- **ADR-0014: terrain layers and movement classes.** `TerrainDef` kind; `MapDef.terrain` and `MapDef.layers` through a `ContentKind::load_sidecars` hook; `MoveClass` moved to `fl_content`; profile `(class, size)`; clearance; slope blocking; Hashed `core.terrain`.
- **ADR-0015: path requests, derived caches and debug queries.** A Hashed FIFO queue charged by cold cost, so caches (`NavGrid`, `HpaGraphs`, `FlowCache`) are Derived and outcome-neutral. Lazy flow tiles in `SimIndex` order. Invalidation by cluster. A `SimQuery::Debug` registry with JSON replies carrying raw `Fx`.
- **ADR-0016: AI seam (stub scope).** `PlayerView`, `AiController`, `NullAi`, and `AiHost` in the client.
- **ADR-0007 refinement:** the `core.rotate`, `core.follow` and `debug.set_terrain` payloads.
- **ADR-0012 refinement:** the polling watcher, `ToRunner::Control` / `RunnerControl::ReloadContent`, `load_world_with` (`drop_unknown_units`), `DefComponent::reload`, client replay capture.

## Spec gaps and how the plan resolves them

1. **§6.4 names Bevy `file_watcher`,** but content is loaded with `std::fs`, not the AssetServer. The plan polls mtimes (ADR-0012 refinement).
2. **§5.7 says nothing about caches versus budgets and saves.** The plan rules that every request is charged its cold cost, so caches are Derived and never change outcomes (ADR-0015).
3. **§7 speed formula and unit factors.** `MobilityDef` limits `terrain_factor` to [0, 1], while the research shows negative values (trucks). The plan widens it to [−1, 1] with a `min_speed_frac` clamp.
4. **§7 gives no per-class path cost,** but unit factors differ within a class. The plan adds `Movement.path_terrain_factor` per class (a guess).
5. **§7 "bridges are destructible objects".** No destruction exists before M2/M7. In M1 bridges are terrain cells, changed by `debug.set_terrain`, which exercises cluster invalidation.
6. **§11 overlays "read … debug queries",** but no generic debug-query mechanism existed. The plan adds the `SimQuery::Debug` registry.
7. **§8 builds `PlayerView` in `Publish`,** but there is no vision yet. The plan builds a client-side stub from the snapshot until M3.
8. **Shift-queue of mixed orders** needs an order queue, which C7 delivers only in the M2 prelude. The plan uses an interim closed `Order` enum in `MoveOrder` v2.
9. **ADR-0012 says reloads are replay events,** but the client recorded no replays in M0. The plan adds `ReplayCapture` and `replay save`.
10. **No layer format was specified:** pixel size, orientation, legend, bit depth. ADR-0014 fixes them.
11. **Height had no M1 rule.** The plan uses it for per-class slope blocking (a guess) and rendering.
12. **§10 gives no key bindings** for stop, rotate, follow, pause, speed or player switch. The plan picks H, R, F, P, `=`, `-`, F4, F5–F7, as provisional until M6.
13. **§7 suggests the `pathfinding` crate.** The plan writes A* in-house, to count expansions, respect the budget, and avoid a download.

## Assumptions

- M1 starts after M0 Task 30 is merged and `main` is fast-forwarded; `docs/perf/M0.json` exists.
- Cells are 4 m; clusters 16×16 (`Path.cluster_cells`); clearance size = ceil(footprint width / cell), capped at 3. All base units are size 1 at 4 m; the ASCII fixture `t.wide_tank` exercises size 2.
- No new base unit types. HalfTrack has no iteration-1 roster unit; tests use the ASCII fixture units.
- `png` 0.18 is already in `Cargo.lock` through `bevy_image`, so adding it downloads nothing. The lead should confirm it passes `cargo deny`.
- Wander now goes through the path queue, so the scale bench includes pathing. A `--flat` row keeps an M0-comparable workload for the 20% rule.
- Tests write terrain fixtures at runtime (an ASCII → PNG helper in `fl_test_utils`). The only committed PNGs are the generated base maps, which a byte-equality test checks against the generator.
- Golden replays and budgets move in several tasks (3, 4, 5, 8, 9, 11, 13, 14). Each re-records them and the lead blesses, so every commit's gate passes.
- `SIM_VERSION` becomes 2 in Task 5 and doesn't change again in M1.
- ADRs 0014–0016 need the user's acceptance (Task 1 Step 5) before Task 2 starts.

## Rulings encoded in the plan's Global Constraints

R31 (shared lane-2 build dir), R33 (`.gate-base` per dispatch), R34 and the bless rule (protected artifacts as `.new`, blessed by the lead), R40 (start dependents during review), R42 (receive-timeline visuals), R44 (debug commands as `LocalPlayer`), R45 (bad-command assertions), R46 (core and feature hook slots), R47 (named messages), R49, R50 (lock delta), R52/R54 (names in hashed state), R53 (derived state consistent at mutation boundaries), R59 (no `SCCACHE_BASEDIRS`), R60 (gameplay-driving state is Hashed), R62 (bump the replay format version when `ReplayStart` changes), plus the deterministic fake-runner timing lesson for client tests.
