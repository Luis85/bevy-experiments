# Frontline

Frontline is a WWII real-time strategy game written in Rust on Bevy 0.20, built as a Cargo workspace of `fl_*` crates with a deterministic, headless-capable simulation core.

Status: **M1 (control and movement)**: a 1 km testbed map with height and terrain layers (river, two bridges,
roads, forest, a hill and a ridge), selection, move/stop/rotate/follow orders with Shift-queues, group moves in
formation over flow fields, HPA* pathfinding under a per-tick budget, separation steering, pathing overlays and hot
reload. M0 (the foundation) is in `docs/m0-demo-checklist.md`, M1 in `docs/m1-demo-checklist.md`.

## Gate

One command defines "verified" for agents, hooks and people (spec §12, ADR-0009):

```bash
cargo xtask gate task --track <track>   # every task: one workspace build; clippy on everything, tests of the
                                        # touched crates plus the fl_sim smoke set (budget 300 s)
cargo xtask gate pr                     # before a merge into main: every test, headless crates built on their
                                        # own, plus cargo deny
cargo xtask gate nightly                # long proptests, GPU and BRP tests
cargo xtask gate milestone --milestone M1
```

Each step prints `PASS`, `WARN`, `SKIP` or `FAIL` (with the tail of the failing output; full logs are in `target/xtask/`), and the last line is `gate <level>: PASS in <n>s` or `gate <level>: FAIL (<steps>) in <n>s`. "Touched" means changed since the merge base with `--base` (default `main`), including uncommitted and untracked files.

The gate's building blocks also run on their own:

```bash
cargo xtask arch-check          # allowed crate edges and banned external crates
cargo xtask gen-clippy          # regenerate the per-crate clippy.toml files from xtask/clippy-template.toml
cargo xtask gen-clippy --check  # fail if a generated clippy.toml drifted
cargo deny check                # licences, advisories, bans, sources
```

Tests run through `cargo nextest run --profile agent` (`.config/nextest.toml`). Never edit a generated `clippy.toml`; edit the template and run `cargo xtask gen-clippy`.

## Run the testbed

```bash
cargo run -p fl_game                       # the testbed: 1 km map, 10 placeholder units, debug UI
cargo run -p fl_game --release             # the same, optimised
cargo run -p fl_game --features dynamic    # faster dev relinks (bevy/dynamic_linking; never for release)
```

Controls (M1; the order and testbed keys are provisional until M6's command grid):

| Input | Effect |
|---|---|
| WASD, arrow keys, screen edge / mouse wheel / Q, E / Home | pan / zoom / rotate / snap the rotation |
| Left click, left drag | select an own unit / box-select; a click on empty ground clears the selection |
| Shift + click or drag | add to the selection |
| Double-click an own unit | select every own unit of that type on screen |
| Ctrl+digit / digit / Shift+digit / Alt+digit | assign a control group / recall it / add it to the selection / recall and centre the camera on it |
| Right click on the ground / on an own unit | move there (in formation for a group) / follow that unit; Alt+right click on an own unit moves to it |
| Shift + any order | queue it after the current orders (at most 32) |
| H | stop |
| R, then left click | turn toward the clicked point |
| F, then left click on a unit | follow it |
| P / `=` / `-` | pause and resume / faster / slower (×0.25 to ×8) |
| F4 | switch the player you see and command |
| F3 / F5 / F6 / F7 | overlays: 100 m grid / paths of the selected units / blocked cells for the first selected unit's class / cells and HPA* clusters |
| `` ` `` or F1, F2 | debug console (`help` lists its commands, such as `spawn`, `terrain`, `reload`, `replay save`, `pause`, `step`), perf panel |

Keys are ignored while the console or another egui field has focus, and while the spawn panel is placing a unit.
Flags: `--scenario <id>` (such as `base.testbed_ai`, where player 1 is an idle AI), `--mods <dir>…`,
`--exit-after <secs>`, `--agent` (keep updating while unfocused). In dev builds, editing a file under the mod
directory hot-reloads the content (`debug.ron` `hot_reload`).

## Render stress test

```bash
cargo run -p fl_game --release -- --stress 3000 --exit-after 25
```

It runs the scale entity mix scaled to 3,000 wandering units, with vsync off, and after 25 s prints
`stress: units=… avg_fps=… p1_low_fps=… frames=…` (the first 3 s do not count). The numbers are in
`docs/perf/M0.md` and `docs/perf/M1.md`.

## Headless tools (`fl_cli`)

```bash
cargo run -p fl_cli -- data-check assets/base                  # validate content
cargo run -p fl_cli -- run-scenario base.testbed --ticks 200   # a run directory under logs/
cargo run -p fl_cli -- replay-verify <file.replay>             # play a replay back and compare hashes
cargo run -p fl_cli --release -- bench --units 500,3000,5000   # the sim bench (tier 3)
cargo run -p fl_cli -- query base.testbed --tick 0 --name core.debug_queries   # a debug query at tick T
cargo run -p fl_cli --release -- map-gen testbed --out assets/base/maps        # regenerate a base map's PNG layers
```

Every command takes `--json` and prints exactly one JSON document. The full list, the remote debug endpoint
(`--features debug_remote` with `--remote`), log and journal locations and the reproduce-a-bug recipe are in
`docs/debugging.md`.
