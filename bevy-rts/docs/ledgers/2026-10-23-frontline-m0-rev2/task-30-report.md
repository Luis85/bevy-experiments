# Task 30 report: M0 integration (demo, determinism, perf baseline, render stress, debugging guide)

Worktree `C:\fl\a`, branch `m0/phase-a`, base `3500b33` (the controller added `4fb22bd docs(plan): M1 …` on
the branch while I worked; untouched).

## Commits

| SHA | Subject |
|---|---|
| `5609c06` | fix(fl_game): load debug.ron before the sim starts and turn on phase markers with the profiler |
| `ac68ebe` | docs(probes): probes are historical evidence pinned to their commits |
| `a06803c` | docs(spec): revision 2.1, M0 implementation refinements |
| `eac6e61` | perf: M0 baseline (sim bench at 500/3,000/5,000 units, render stress) |
| `56e3425` | docs: M0 debugging guide, demo checklist and perf baseline |

## What I did

1. **T22 carry (fl_game, `crates/fl_game/src/builder.rs`).** `GameBuilder::build` now loads `debug.ron`
   (`observe_setup` → `load_debug_config`) before `start_sim`, and builds the sim with
   `RunConfig { phase_markers: cfg.profiler, .. }`; `set_up_sim` and the `ScenarioLoader` (used by
   `fl.load_scenario`/`fl.load`) take the same `RunConfig`, so reloaded matches keep the markers. Headless
   tests keep `RunConfig::default()`. Live evidence: the F2 perf panel and `fl.perf` now show all eight phases
   with real timings (`docs/demo/m0-31-perf-panel.png`).
2. **`probes/README.md`** (R43): probes are historical evidence; `dynlink` built against `a28e79b`, `remote`
   against `80bbd57`; may not compile now.
3. **Spec "Revision 2.1"** section (after Revision 2): one line per contract-refining ruling with its ADR
   (R5, R15, R19/R31/R59, R20, R23, R26, R33/R34, R36/R37, R41, R42, R43, R44, R45/R49, R46, R47, R51, R52, R53,
   R54, R56, R60, R61, R62, R65, R66, R68); status line updated.
4. **README.md**: run the testbed (controls), the render stress test, `fl_cli`, pointer to `docs/debugging.md`
   (the gate section already existed).
5. **`docs/debugging.md`** (spec §15 C): debug launch flags and features (`--remote`, `--remote-port`,
   `--agent`, `trace`/`trace_chrome`/`trace_tracy`, `dynamic`), `debug.ron`, `RUST_LOG`, in-game keys; BRP port
   per agent; all 11 `fl_cli` commands with real text and `--json` outputs (run with the release binary, pasted);
   all 17 `fl.*` methods with request/response JSON from the live session, plus the useful `world.*` /
   `brp_extras/*` calls; file locations; the reproduce-a-bug recipe (seed + scenario → `run-scenario --trace
   --dump-state-every` → `replay-verify` → `bisect-desync` → `inspect --entity` → `journal`), every command of
   which I ran; the Task 26 dynamic-linking result.
6. **Perf baseline** `docs/perf/M0.md` + `docs/perf/M0.json` (bench output plus machine/rustc/commit/power-plan
   metadata, load conditions, analysis, render-stress section and runs; JSON `meta.machine`, `meta.rustc`,
   `meta.commit`, `meta.render_stress`).
7. **Windowed demo over BRP** (`docs/m0-demo-checklist.md`, screenshots in `docs/demo/`, quantized PNGs).
8. Gates, determinism, data-check (below).

## Results

### Gates

```
$ cargo xtask gate pr --track docs --track perf --track client --track probes --track contract
PASS fmt (17.6s)
PASS clippy-drift (20.8s)
PASS claude-md (0.0s)
PASS check-scope (0.7s)
PASS protected (0.8s)
PASS test-guard (2.0s)
PASS clippy (266.0s)
PASS arch-check (30.3s)
PASS test-headless (292.8s)
PASS test-client (152.2s)
PASS api (108.7s)
PASS deny (51.1s)
PASS data-check (84.1s)
gate pr: PASS in 1029s

$ cargo xtask gate milestone --milestone M0
PASS fmt (11.4s)
PASS clippy-drift (24.0s)
PASS claude-md (0.1s)
PASS clippy (89.9s)
PASS arch-check (12.2s)
PASS test-headless (108.0s)
PASS test-client (70.0s)
PASS api (58.9s)
PASS deny (11.2s)
PASS data-check (10.4s)
PASS gpu (502.3s)
PASS brp (249.1s)
PASS perf-file (0.0s)
gate milestone: PASS in 1149s
```

Both ran after the code commit (`5609c06`) with the docs/perf files present in the working tree. After the last
two (docs-only) commits:

```
$ cargo xtask gate task --track docs --track perf --track client --track probes --track contract
PASS fmt (12.0s)
PASS clippy-drift (7.9s)
PASS claude-md (0.0s)
PASS check-scope (1.2s)
PASS protected (0.9s)
PASS test-guard (2.0s)
PASS clippy (81.9s)
PASS arch-check (15.4s)
PASS test-headless (153.7s)
PASS test-client (47.7s)
SKIP api: no API crate touched since 3500b33 (.gate-base)
PASS data-check (16.0s)
WARN gate task took 341s (budget 300s)
gate task: PASS in 341s
```

(The budget warning is machine load: other projects' builds ran throughout.)

### Content and determinism

- `fl_cli --release data-check assets/base`: exit 0, `units: 10`, `rules hash: 0xbc8d91076ccb4b38`.
- `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent --release -p fl_sim -p fl_test_utils`:
  `213 tests run: 212 passed, 1 failed`. The failure is
  `fl_sim::classify::tests::unclassified_sim_type_fails_build` ("expected Unclassified, the sim built"): the
  classification check runs only under `cfg!(debug_assertions)` (`fl_sim/src/build.rs:183`) by design, but
  the test is not gated to debug builds. Not a determinism failure; every twin-run, shuffle, save/branch and
  golden-replay test passed in release. In debug builds the same suite passes in the gate rows.
- `fl_cli replay-verify crates/fl_test_utils/tests/replays/testbed_wander.replay`: `verified: 400 ticks`, exit 0.
- Save/load by hand: `save --tick 100` + `load --ticks 100` reach the uninterrupted run's final hash
  `0x28c67f8b5038110a`; live `fl.save`/`fl.load` resumed at tick 2584 with the same hash and matched 40 ticks
  later.

### Bench (release, median of 5, 400 ticks + 40 warm-up)

| units | entities | mean ms | p95 ms | max ms |
|---:|---:|---:|---:|---:|
| 500 | 1680 | 0.537 | 1.314 | 3.299 |
| 3000 | 10000 | 4.431 | 7.955 | 12.850 |
| 5000 | 16680 | 9.335 | 16.329 | 27.443 |

G7 met (4.43 ms ≤ 15 ms). `Perceive` dominates at scale (2.38 / 6.09 ms at 3k / 5k). A second run gave
0.570 / 4.383 / 9.400 ms. Budgets (`budgets.ron`) untouched.

### Render stress (`fl_game --release -- --stress 3000 --exit-after 25 --agent`, visible window)

avg/p1-low fps over 7 runs: 101.0/57.7, 56.8/12.7, 104.0/73.2, 98.8/45.5, 67.1/21.5, 43.0/7.0 (RUST_LOG=warn),
102.7/85.7 (RUST_LOG=warn). Median average 98.8, best 104.0; 5 of 7 ≥ 60. The machine was **never idle**:
other projects' cargo builds (7–31 foreign `cargo`/`rustc` processes, CPU 96–100 %) ran through every
measurement and I could not stop them (not my processes). The dips are not reproducible with the same binary,
so I attribute them to CPU starvation. A dev-build `fl.perf` profile under stress: 95 fps, 10.5 ms frames,
3,000 visuals but only 175–190 visible meshes after culling, client systems < 1.1 ms in total; no rendering
change was justified by the data, so none was made.

### Demo checklist summary (`docs/m0-demo-checklist.md`)

PASS live over BRP: iso view + 10 placeholders in faction colours, pan, wheel zoom, Q/E rotate + R snap, edge
clamp (camera stops at the map corner), `fl.spawn` (+ unknown unit rejected), `debug.wander` with smooth
interpolation (42 distinct visual positions in 1 s over 20 ticks), stats panel, the four cheats, orders while
paused apply on resume, speed ×0.25/×8 (+ 9 rejected), single step (+ 601 rejected), F2 perf panel with
phases and features, `fl.save`/`fl.load` same tick and hash, BRP smoke, game hashes = headless hashes, render
stress (with the load caveat). PASS (test): edge pan, cheats rejected without `cheats_allowed`, `reveal`,
`player 1`, `inspect` window. NOT RUN (needs a human): spawn-panel click-to-place, typing into the console
(`brp_extras/type_text` does not reach the egui text field; the console opens and the same commands were
issued with `fl.issue_command`). ADR confirmations: 46 rows (44 bullets + D1/D2), all PASS except 0004 "replay with an extra
registered name" (PARTIAL: remap tested for saves only), 0009 live hook denial (PARTIAL, R65), 0012 (M1), and
D1/D2 (deferred by user decision).

## Findings for the controller

1. **Spawn logging costs ~30 ms per sim tick in debug runs.** With the default `debug.ron`
   (`info,fl_sim=debug`), `fl_sim::spawn` logs every spawn at DEBUG; at 3,000-unit stress (50 projectile
   spawns per tick) `Lifecycle` took 29.7 ms and the tick 38.0 ms in the dev build; with `RUST_LOG=info`,
   0.56 ms and 10.5 ms. Release still prints them to stderr. One-line fix (TRACE, or `debug.ron` default
   `info`) is outside my tracks (sim-core / tools). Recorded in `docs/perf/M0.md`.
2. **`unclassified_sim_type_fails_build` fails in release** (above): gate it with `#[cfg(debug_assertions)]`
   (contract path `fl_sim/src/classify.rs`; not changed).
3. **`BRP_EXTRAS_PORT` is not read by `fl_game`**: it builds `BrpExtrasPlugin::with_http_plugin(..)`, which
   ignores the env var (bevy_brp_extras 0.23 `plugin.rs`). `CLAUDE.md`, `.claude/rules/client.md`, the
   client-engineer agent and spec §15 C say otherwise; `docs/debugging.md` documents the real behaviour.
   Contract-path text, not changed.
4. **`fl.save_scenario` after `fl.load`** fails with "the match did not start from a scenario" (by design:
   `CurrentScenario` is cleared); documented.
5. **The release and the combined builds differ in features**: `cargo build -p fl_cli -p fl_game` and
   `cargo run -p fl_game` alone unify different Bevy feature sets, so each rebuilds Bevy (~15–24 min cold).
6. The fl_game window launched from a background shell starts minimized (size 0); `fl.screenshot` (offscreen)
   still works, window captures need `Start-Process -WindowStyle Normal`.

## Not done / deviations

- Power plan: the brief asks to confirm a fixed power plan with the user first. I am a subagent and may not
  change system settings; the plan was **Balanced**, recorded in both files.
- The brief's Step 9 asks for a whole-branch review via `requesting-code-review`; the implementer
  instructions forbid spawning reviewers, so I self-reviewed and leave the review to the controller.
- Bench command: the brief's `--json docs/perf/M0.json` is `--json-out` in the CLI (`--json` is the global
  output flag); I added `--profile` for the per-phase breakdown.

## `.new` files to bless

None. No protected path was touched (`budgets.ron`, baselines, snapshots unchanged).

## Files changed

`crates/fl_game/src/builder.rs`, `probes/README.md`, `docs/superpowers/specs/2026-10-23-frontline-rts-design.md`,
`README.md`, `docs/debugging.md`, `docs/m0-demo-checklist.md`, `docs/demo/*.png` (10),
`docs/perf/M0.md`, `docs/perf/M0.json`.

## Self-review

- builder.rs: `cfg` is `Some` exactly for the windowed shell (commented); the run directory is now created
  before content loading, so a content error in a windowed debug run leaves an empty run dir (minor).
- No new test for the phase-marker wiring; the live perf panel/`fl.perf` show real phase rows.
- Docs: every command and method shown was run; Windows paths and run ids are the real ones.
