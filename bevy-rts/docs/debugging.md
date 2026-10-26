# Debugging Frontline

How to see what the sim and the client are doing, for people and for agents (spec §15). Every command and
response below was run and pasted from its real output: the M0 parts on `m0/phase-a` (M0, Task 30), the M1
parts on `m1/phase-a` (M1, Task 21); long outputs are cut where marked `…`.

**Hashes are examples, not constants.** A state hash or rules hash in this guide is what that run printed. It
changes whenever the content, the seed, the commands or the sim's behaviour change (M1 changed every M0 hash).
Check determinism by comparing two runs with each other, never with a value remembered from here: the game
against `fl_cli hash-dump` (§4 `fl.sim_state`), a loaded save against the uninterrupted run (§3 `save` and
`load`), a replay against itself (`replay-verify`).

Contents: [debug mode](#1-launching-in-debug-mode) · [BRP ports](#2-one-brp-port-per-agent) ·
[`fl_cli`](#3-fl_cli-headless-tools) · [`fl.*` methods](#4-the-remote-debug-endpoint-fl-methods) ·
[files](#5-where-things-are-written) · [reproduce a bug](#6-reproduce-a-bug) ·
[dynamic linking](#7-dynamic-linking-task-26) · [pathing, terrain and orders](#8-pathing-terrain-and-orders-m1) ·
[why didn't my unit move](#9-why-didnt-my-unit-move) · [hot reload](#10-hot-reload)

## 1. Launching in debug mode

```bash
cargo run -p fl_game                                            # testbed, debug build, debug UI
cargo run -p fl_game --features debug_remote -- --remote        # + the remote debug endpoint on 15702
cargo run -p fl_game --features debug_remote -- --remote --remote-port 15731 --agent
```

| Flag / feature | Effect |
|---|---|
| `--remote` | Serve BRP (JSON-RPC over HTTP on `127.0.0.1`) with the `fl.*` methods. Needs `--features debug_remote` **and** a debug build; a release build or a build without the feature logs a warning and serves nothing (spec §15 C). |
| `--remote-port <P>` | The BRP port (default 15702). Requires `--remote`. |
| `--agent` | Keep updating while the window is unfocused or hidden (`WinitSettings::continuous`): use it whenever no human watches. |
| `--scenario <id>` / `--stress <N>` | Start a scenario (default `base.testbed`) or the scale entity mix scaled to N units. |
| `--exit-after <secs>` | Exit after that long and print `stress: units=… avg_fps=… p1_low_fps=… frames=…`. |
| `--mods <dir>…` | Mod directories in load order (default: the base mod). |
| `--features dynamic` | `bevy/dynamic_linking` for faster dev relinks (see §7). |
| `--features trace` | Bevy's tracing spans (`bevy/trace`). |
| `--features trace_chrome` | Writes a Chrome trace (`trace-<timestamp>.json` in the working directory); open it in `chrome://tracing` or Perfetto. |
| `--features trace_tracy` | Streams to a running Tracy profiler. |

**`debug.ron`** (repo root, read from the working directory by `fl_observe::load_debug_config` in `fl_game`
before the sim starts; a missing or broken file falls back to the defaults below with a message on stderr):

```ron
(
    log_filter: "info,fl_sim=debug,fl_sim::spawn=info",   // EnvFilter directives for stderr and log.jsonl
    json_log: true,                    // write log.jsonl into the run directory
    journal_file: true,                // write journal.jsonl into the run directory
    profiler: true,                    // the profiler layer, and the sim's phase markers (fl_sim::phase)
    budgets: (tick_ms: 15.0, per_phase_ms: { "Input": 3.0, … }, per_feature_ms: {}),
    hot_reload: true,                  // dev builds: watch the mod files and hot-reload on a change (§10)
    hot_reload_poll_ms: 500,           // how often the watcher scans the mod files
)
```

Budget overruns only warn, on target `fl_observe::budget`. With `profiler: true` the sim runs with
`RunConfig::phase_markers`, so the F2 perf panel and `fl.perf` show per-phase times.

**`RUST_LOG`** replaces `log_filter` when set, for example `RUST_LOG=info,fl_sim::movement=trace`. Targets are
module paths: `fl_sim::<module>`, `fl_client::<module>`, `fl_observe::budget`. Every record in `log.jsonl`
emitted inside a sim tick carries `"tick"` and its `sim_tick` span; human stderr output may not (R56).

In the game: `` ` `` or F1 opens the console (`help` lists every command: `spawn`, `wander`, `setowner`,
`invuln`, `sethp`, `teleport`, `terrain`, `player`, `reveal`, `pause`, `resume`, `step`, `speed`, `hash`,
`tick`, `inspect`, `reload`, `replay`), F2 the perf panel. Every console command that changes the sim is a
`debug.*` command issued through the driver (ADR-0008); `player` and `reveal` are client-only render overrides.
The M1 console commands:

| Command | Effect |
|---|---|
| `terrain <x0> <y0> <x1> <y1> <id>` | `debug.set_terrain`: the cells `x0..=x1` × `y0..=y1` become terrain `<id>`; units repath (`terrain 70 122 72 127 base.water` floods the west bridge of the testbed) |
| `reload` | hot-reload the content now, as the watcher would on a file change (§10) |
| `replay save <path>` | write the running match as a replay: its start, every input sent and every hash received, plus its `ContentReload` events (a replay with one is non-reproducible: `replay-verify` exits 3) |

**Keys** (M1; the order and testbed keys are provisional until M6's command grid). Every key below is ignored
while egui wants the keyboard (the console's text field has focus) or while the spawn panel places a unit
(`InputGate`); the mouse is ignored while egui wants the pointer.

| Key | Effect |
|---|---|
| WASD / arrows / screen edge, wheel, Q/E, **Home** | pan, zoom, rotate, snap the rotation (Home since M1, R87; R was the M0 snap key) |
| F3 / F5 / F6 / F7 | overlays `grid` (100 m) / `paths` (the selected units' paths) / `passability` (blocked cells of the first selected unit's profile, else Tracked size 1, within 120 m of the camera focus) / `cells` (cell grid within 60 m of the cursor, HPA* cluster borders and entrances of the same profile) |
| left click, drag, Shift, double-click | select a unit, box-select (a drag under 6 px is a click), add to the selection, select every own unit of that type on screen; a click on empty ground clears the selection unless Shift is held |
| Ctrl / plain / Shift / Alt + digit | assign the control group (an empty selection empties it) / recall it / add it / recall it and centre the camera on it |
| right click | move to the ground point (a group moves in formation); on an own unit: follow it; Alt+right click on an own unit: move to it |
| Shift + an order | queue the order (at most 32 orders per unit, then `core.queue_full`) |
| H | stop (`core.stop`) |
| R, then left click | rotate toward the point (`core.rotate`) |
| F, then left click on a unit | follow it (`core.follow`) |
| P / `=` / `-` | pause and resume / faster / slower (×0.25, 0.5, 1, 2, 4, 8) |
| F4 | view and command as the next player |

A selection of more than 256 units is sent as several commands of at most 256 units, in `SimId` order (R88).

## 2. One BRP port per agent

The default port 15702 belongs to the lead. Every other agent that runs the game picks its own port and passes
it to the game **and** to every `brp` MCP tool call:

```bash
cargo run -p fl_game --features debug_remote -- --remote --remote-port 15731 --agent
```

`--remote-port` sets the port. When the flag is absent, `fl_game` takes it from the `BRP_EXTRAS_PORT`
environment variable (the flag wins; a value that is not a port number is ignored with a message), else 15702.
`--remote` is still needed to serve the endpoint. A free port per worktree, such as `15700 + <task number>`, avoids
collisions. Readiness: poll `fl.sim_state` until `tick` reaches what you need; never sleep a fixed time.

Raw JSON-RPC works too, without the MCP:

```bash
curl -s -X POST http://127.0.0.1:15731 -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"fl.sim_state"}'
```

The built-in `world.*` and `brp_extras/*` methods see only the **client** World (visual entities, the camera,
input); the sim World is reachable only through `fl.*`.

## 3. `fl_cli`: headless tools

`cargo run -p fl_cli [--release] -- [--json] <command> …`. With `--json`, stdout holds exactly one JSON
document (errors too: `{"error": "…"}`); logs go to stderr. Exit codes: 0 success; 1 failure (an error, a
failed check, a divergence); 2 bad usage; 3 a non-reproducible replay. Commands that re-simulate take
`--mods <dir>…` (default: the base mod). A *source* is a scenario id (`base.testbed`), a `*.scenario.ron`, a
`*.replay`, a run directory (its `run.replay`) or a `*.save`.

| Command | What it does |
|---|---|
| `data-check <dir>…` | Validate mods; prints every error and exits 1 on failure |
| `query <source> --tick T --name <query> [--args <json>]` | Re-simulate to after tick T and print a debug query's answer (§8) |
| `map-gen testbed\|scale --out <dir>` | Write a base map's procedural height and terrain PNGs |
| `hash-dump --mods <dir>… --scenario <id> --ticks N [--interval 20]` | Run a scenario headless and print `tick hash` lines |
| `bench [--units 500,3000,5000] [--ticks 400] [--warmup 40] [--repeat 5] [--seed 1] [--flat] [--out md] [--json-out json] [--profile] [--baseline json [--force]] [--max-regression 0.20]` | Wall-clock bench of the scale entity mix (tier 3, informational; median of `--repeat` runs), on the 2 km terrain map, or with `--flat` on M0's open map |
| `run-scenario <scenario> --ticks N [--seed S] [--wander R] [--trace] [--dump-state-every N] [--out logs]` | Run headless into a new run directory: `log.jsonl`, `journal.jsonl`, `run.replay`, `states.jsonl` |
| `inspect <source> --tick T [--entity SIMID]` | Re-simulate to after tick T; print the world summary or one entity's components |
| `journal <run dir or journal.jsonl> [--kind K] [--sim-id N] [--player P] [--from T] [--to T] [--limit N]` | Journal entries matching every filter, in tick order |
| `replay-verify <file.replay>` | Play a replay back, compare its hashes: exit 0 verified, 1 diverged, 3 non-reproducible |
| `replay dump <file.replay>` | A summary, or with `--json` the whole replay |
| `save <source> --tick T --out <file.save>` | Re-simulate to after tick T and write a save |
| `load <file.save> --ticks N` | Load a save, run N more ticks, print the final hash |
| `bisect-desync <a> <b>` | Re-simulate two replays or run directories hashing every tick; report the first tick that differs and the entities that differ there: exit 0 identical, 1 diverged |

`--seed` and `--wander R` (before tick 0, every player orders all its units to wander R metres with
`debug.wander`; the match then allows cheats) apply to scenario sources.

### `data-check`

```console
$ cargo run -p fl_cli --release -- data-check assets/base
mods: base
units: 10
faction: 2
map: 2
scenario: 2
terrain: 8
unit: 10
visual: 10
rules hash: 0x0f29a5a56c793aa7
$ cargo run -p fl_cli --release -- --json data-check assets/base
{
  "kinds": { "faction": 2, "map": 2, "scenario": 2, "terrain": 8, "unit": 10, "visual": 10 },
  "mods": [ "base" ],
  "rules_hash": "0x0f29a5a56c793aa7",
  "units": 10,
  "warnings": []
}
$ cargo run -p fl_cli --release -- --json data-check crates/fl_cli/tests/fixtures/bad_mod   # exit 1
{
  "errors": [
    "crates/fl_cli/tests/fixtures/bad_mod\\mod.ron: depends: mod 'bad_mod' depends on mod 'no_such_mod', which is not loaded",
    "crates/fl_cli/tests/fixtures/bad_mod\\units\\bogus_component.unit.ron:5:10: bad_mod.bogus/components.NoSuchComponent: unknown component 'NoSuchComponent' (registered: Hull, Mobility, Selectable)",
    "crates/fl_cli/tests/fixtures/bad_mod\\units\\thing.widget.ron: unknown content file type (expected mod.ron, rules.ron, credits.ron or *.<kind>.ron for a registered kind: faction, map, scenario, unit, visual)"
  ]
}
```

### `hash-dump`

```console
$ cargo run -p fl_cli --release -- hash-dump --mods assets/base --scenario base.testbed --ticks 60 --interval 20
0 <hash at tick 0>
20 <hash at tick 20>
40 <hash at tick 40>
$ cargo run -p fl_cli --release -- --json hash-dump --mods assets/base --scenario base.testbed --ticks 60 --interval 20
[
  { "hash": "0x<hash>", "tick": 0 },
  { "hash": "0x<hash>", "tick": 20 },
  { "hash": "0x<hash>", "tick": 40 }
]
```

### `run-scenario`

```console
$ cargo run -p fl_cli --release -- --json run-scenario base.testbed --ticks 200 --wander 100 --out logs/doc
{
  "final_hash": "0x5b3cf196af410851",
  "journal_entries": 43,
  "rejected_commands": 0,
  "rules_hash": "0x0f29a5a56c793aa7",
  "run_dir": "logs/doc\\20261025-055108-3724",
  "ticks": 200
}
$ cargo run -p fl_cli --release -- run-scenario base.testbed --ticks 200 --wander 100 --seed 7 --out logs/doc
run dir: logs/doc\20261025-055109-49696
ticks: 200
final hash: 0xa8a92e4547098e70
rules hash: 0x0f29a5a56c793aa7
journal entries: 40
rejected commands: 0
```

Since M1 a wander goal is a path request, so the journal also holds `core.path_request`, `core.path_result`
and `core.order_changed` records (§8).

The run directory holds `log.jsonl`, `journal.jsonl`, `run.replay` and `states.jsonl` (empty unless
`--dump-state-every N`). The log also goes to stderr (`--trace` logs everything at `trace`). A `log.jsonl`
record:

```json
{"fields":{"hash":"b2b8a96fe20c0dd0","message":"state hash"},"level":"DEBUG","spans":[{"fields":{"tick":0},"name":"sim_tick"}],"target":"fl_sim::step","tick":0,"timestamp":"2026-10-24T06:37:21.823807Z"}
```

### `inspect`

```console
$ cargo run -p fl_cli --release -- inspect logs/doc/20261025-055108-3724 --tick 100
{
  "entities": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
  "header": { "features": [["core", 2]], "mods": [["base", "0.1.0"]], "rules_hash": 1092586514522127015 },
  "last_seq": { "0": 0, "1": 0 },
  "map": { "cell": 4.0, "cells_x": 250, "cells_y": 250, "size": [1000.0, 1000.0] },
  "players": [
    { "controller": "Human", "faction": 0, "id": 0, "team": 0 },
    { "controller": "Human", "faction": 1, "id": 1, "team": 1 }
  ],
  "seed": 1,
  "state_hash": 15669243218292312646,
  "tick": 100
}
$ cargo run -p fl_cli --release -- --json inspect logs/doc/20261025-055108-3724 --tick 100 --entity 3
{
  "components": {
    "core.footprint": { "hitbox_shrink": 0.5, "length": 6.300000000046566, "width": 3.6000000000931323 },
    "core.health": { "hp": 210, "max": 210 },
    "core.mobility": { "class": "Tracked", "speed": 0.5277777777519077, "terrain_factor": 0.5, "turn_rate": 2.7520751953125 },
    "core.move_order": { "orders": [{ "Move": { "target": [467.6937564648688, 283.627812191844] } }] },
    "core.owner": 0,
    "core.path_follow": { "corners": [[467.6937564648688, 283.627812191844]], "goal": [467.6937564648688, 283.627812191844],
                          "next": 0, "planned": 0, "state": "Following" },
    "core.pose": { "heading": 311.0009765625, "pos": [429.0088024912402, 328.13184845703654] },
    "core.selectable": null,
    "core.unit_type": 4,
    "core.wander": { "home": [410.0, 350.0], "radius": 100.0 }
  },
  "sim_id": 3
}
```

`inspect` prints JSON with or without `--json`. `state_hash` is decimal here; in hex it is the hash `save`
prints for the same source and tick (`0xd97455b015830a46` in this run): compare the two.

### `journal`

```console
$ cargo run -p fl_cli --release -- journal logs/doc/20261025-055108-3724 --limit 3
{"tick":0,"phase":0,"kind":"core.command_applied","sim_id":null,"player":0,"emitted":0,"data":{"command":"debug.wander","queue":false,"seq":0,"units":[1,2,3,4,5]}}
{"tick":0,"phase":0,"kind":"core.command_applied","sim_id":null,"player":1,"emitted":1,"data":{"command":"debug.wander","queue":false,"seq":0,"units":[6,7,8,9,10]}}
{"tick":0,"phase":1,"kind":"core.path_request","sim_id":1,"player":null,"emitted":0,"data":{"goal":[362.7614972181618,331.69829715043306],"kind":"single","profile":{"class":"Tracked","size":1},"request":0}}
$ cargo run -p fl_cli --release -- --json journal logs/doc/20261025-055108-3724 --kind core.command_applied --player 1 --limit 5
{
  "entries": [
    {
      "data": { "command": "debug.wander", "queue": false, "seq": 0, "units": [6, 7, 8, 9, 10] },
      "emitted": 1, "kind": "core.command_applied", "phase": 0, "player": 1, "sim_id": null, "tick": 0
    }
  ],
  "matched": 1
}
```

### `replay-verify` and `replay dump`

```console
$ cargo run -p fl_cli --release -- replay-verify crates/fl_test_utils/tests/replays/testbed_wander.replay
verified: 400 ticks
warning: replay recorded with build "0.0.0+87407dc", playing back with build "0.0.0+3d78b45"
$ cargo run -p fl_cli --release -- replay-verify crates/fl_test_utils/tests/replays/testbed_paths.replay
verified: 900 ticks
warning: replay recorded with build "0.0.0+9799309", playing back with build "0.0.0+3d78b45"
$ cargo run -p fl_cli --release -- --json replay-verify logs/doc/20261025-055108-3724/run.replay
{ "status": "verified", "ticks": 200, "warnings": [] }
$ cargo run -p fl_cli --release -- --json replay-verify nope.replay      # exit 1
{ "error": "cannot read nope.replay: … (os error 2)" }
$ cargo run -p fl_cli --release -- replay dump logs/doc/20261025-055108-3724/run.replay
format version: 3
seed: 1
cheats allowed: true
rules hash: 0x0f29a5a56c793aa7
start: scenario base.testbed
ticks: 200
hash interval: 20
inputs: 1 (2 commands)
hashes: 10
events: 0
```

`replay dump --json` prints the whole replay: `version`, `header` (`build_id`, `cheats_allowed`, `features`,
`format_version`, `mods`, `rules_hash`, the string tables, …), `start`, `ticks`, `hash_interval`, `hashes`
(`[tick, u64]` pairs), `events` and `inputs` (each `{tick, commands: [{exec_tick, player, seq, units, queue,
kind: {type_id, payload: [postcard bytes]}}]}`).

### `save` and `load`

```console
$ cargo run -p fl_cli --release -- --json save logs/doc/20261025-055108-3724 --tick 100 --out logs/doc/t100.save
{ "entities": 10, "hash": "0xd97455b015830a46", "out": "logs/doc/t100.save", "pending_commands": 0, "tick": 100 }
$ cargo run -p fl_cli --release -- --json load logs/doc/t100.save --ticks 100
{ "final_hash": "0x5b3cf196af410851", "start_tick": 100, "tick": 200, "ticks": 100 }
$ cargo run -p fl_cli --release -- save base.testbed --wander 100 --tick 100 --out logs/doc/t100b.save
saved tick 100 to logs/doc/t100b.save (193909 bytes)
hash: 0xd97455b015830a46
entities: 10
pending commands: 0
$ cargo run -p fl_cli --release -- load logs/doc/t100b.save --ticks 100
loaded tick 100, ran 100 ticks to tick 200
final hash: 0x5b3cf196af410851
```

The check: the loaded match's `final_hash` equals the uninterrupted `run-scenario`'s `final_hash` (the
save/branch property, by hand), and both saves print the same hash. Since M1 a save holds the terrain grid
(`core.terrain`, Hashed), so it is about 190 KB on the testbed map.

### `bisect-desync`

```console
$ cargo run -p fl_cli --release -- --json bisect-desync logs/doc/20261024-063510-27684 logs/doc/20261024-063655-35332   # exit 1
{
  "diverged": true,
  "entities": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
  "hash_a": "0xb2b8a96fe20c0dd0",
  "hash_b": "0x<hash>",
  "setup": false,
  "tick": 0,
  "ticks_a": 200,
  "ticks_b": 200,
  "ticks_compared": 1
}
$ cargo run -p fl_cli --release -- bisect-desync logs/doc/20261024-063510-27684 logs/doc/20261024-063630-14472   # exit 1
diverged at before the first tick: 0x4c4acd04ce1d032a vs 0xcf0ecde0d6cd4064
entities that differ: []
$ cargo run -p fl_cli --release -- --json bisect-desync logs/doc/20261024-063510-27684 logs/doc/20261024-063510-27684/run.replay
{ "diverged": false, "entities": [], "setup": false, "tick": null, "ticks_a": 200, "ticks_b": 200, "ticks_compared": 200 }
```

The first pair differs in its commands (`--wander 100` against none), so tick 0 differs and every wandering
unit is listed; the second pair differs only in the seed, so the worlds differ before the first tick
(`"setup": true`) without any entity differing.

### `bench`

```console
$ cargo run -p fl_cli --release -- bench --units 500,3000,5000 --ticks 400 --warmup 40 --repeat 5     --out docs/perf/M0.md --json-out docs/perf/M0.json --profile
# Sim bench

ticks 400 (+40 warmup), seed 1, median of 5, release build; profiler window: last 240 ticks; wall times include profiler and phase-marker overhead

| units | entities | mean ms | p95 ms | max ms |
|---:|---:|---:|---:|---:|
| 500 | 1680 | 0.537 | 1.314 | 3.299 |
| 3000 | 10000 | 4.431 | 7.955 | 12.850 |
| 5000 | 16680 | 9.335 | 16.329 | 27.443 |
…
```

```console
$ cargo run -p fl_cli --release -- --json bench --units 200 --ticks 40 --warmup 5 --repeat 1
{
  "meta": { "build": "release", "profiler_window_ticks": 40, "repeat": 1, "seed": 1, "ticks": 40, "units": [200], "warmup": 5, "note": "…" },
  "rows": [
    {
      "units": 200, "entities": 680, "mean_ms": 0.3170924999999999, "p95_ms": 0.7033999999999999, "max_ms": 4.143,
      "per_phase_mean_ms": [["Input", 0.008320000000000001], ["Think", 0.00346], ["Act", 0.13614], …],
      "counters_per_tick": [["despawn.entities", 2.5], ["publish.rebuilt", 200.0], …, ["spawn.entities", 4.0]]
    }
  ]
}
```

The text report is the markdown file (headline table, mean ms per phase, work counters per tick). `--json`
prints the JSON document instead: `meta`, `rows` (per size `units`, `entities`, `mean_ms`, `p95_ms`,
`max_ms`, `per_phase_mean_ms`, `counters_per_tick`) and, with `--profile`, `sizes` (per size the profiler's
`tick`, `phases`, `features` and `systems`). `--baseline <json> [--max-regression 0.2]` exits 1 when a size's
mean is more than 20 % slower than in that file. Wall-clock numbers are informational; the gates are the work
counters in `docs/perf/budgets.ron` (ADR-0010).

Since M1 the bench runs the scale mix on the 2 km terrain map `base.scale_2km` with pathing (every row and
`meta` carry `"map"`); `--flat` runs M0's flat, open map (`"map": "open"`), the workload `docs/perf/M0.json`
measured. A baseline from the other map is refused before anything runs, unless `--force`, which compares with
a warning (M1 Task 21, `docs/perf/M1.md` has the numbers):

```console
$ cargo run -p fl_cli --release -- bench --units 500 --ticks 20 --warmup 2 --repeat 1 --baseline docs/perf/M0.json   # exit 1
error: bench baseline: the baseline ran on map open, this run on map base.scale_2km: the comparison is not like for like (--flat runs the open map); pass --force to compare anyway
$ cargo run -p fl_cli --release -- bench --flat --units 500 --ticks 20 --warmup 2 --repeat 1 --baseline docs/perf/M0.json   # exit 1 on a regression
bench regression:
500 units: 0.835 ms vs baseline 0.537 ms (+55.6%, limit 20%)
# Sim bench

map open, ticks 20 (+2 warmup), seed 1, median of 1, release build; …
$ cargo run -p fl_cli --release -- --json bench --units 500 --ticks 20 --warmup 2 --repeat 1 --baseline docs/perf/M0.json --force
warning: the baseline ran on map open, this run on map base.scale_2km: the comparison is not like for like (--flat runs the open map)
…                                    (stdout: the JSON document, with "baseline": {"ok": false, "message": "bench regression:\n500 units: …"})
```

A baseline is only as good as the machine it ran on: `docs/perf/M0.json` was taken under foreign build load.
For a like-for-like comparison, build the older commit's `fl_cli` in its own worktree and interleave the two
binaries on an idle machine (`docs/perf/M1.md` "Method").

### `query`

`query` re-simulates a source to after tick `T`, then answers one debug query (§8) as JSON, with or without
`--json` (a failing query prints `{"error": …}` and exits 1; `--args` that is not JSON exits 2). Positions and
heights are raw `Fx` (`i64`; divide by 2^32 for metres).

```console
$ cargo run -p fl_cli --release -- --json query base.testbed --tick 0 --name core.debug_queries
{ "queries": ["core.clusters", "core.debug_queries", "core.passability", "core.paths", "core.terrain_at"] }
$ cargo run -p fl_cli --release -- --json query crates/fl_test_utils/tests/replays/testbed_paths.replay --tick 100 \
    --name core.paths --args '{"units": [1, 2]}'
{
  "1": {
    "corners": [[1245540515840, 2104533975040], [1245540515840, 2207613190144], [2894807957504, 3272765079552],
                [2920577761280, 3264175144960]],
    "goal": [2920577761280, 3264175144960],
    "next": 0,
    "orders": [{ "move": [2920577761280, 3264175144960] }],
    "profile": { "class": "Tracked", "size": 1 },
    "state": "following"
  },
  "2": { "corners": [[3100966387712, 2087354105856], …], "goal": [3035110222507, 3264175144960], … }
}
$ cargo run -p fl_cli --release -- --json query crates/fl_test_utils/tests/replays/testbed_paths.replay --tick 100 \
    --name core.terrain_at --args '{"cell": [900, 1]}'          # exit 1
{ "error": "cell [900, 1] is off the 250x250 map" }
$ cargo run -p fl_cli --release -- --json query base.testbed --tick 0 --name core.passability --args 'nope'   # exit 2
{ "error": "--args is not JSON: expected ident at line 1 column 2" }
```

Unit 1's goal `[2920577761280, 3264175144960]` is (680, 760) m. The source is anything `inspect` takes; a
golden replay or a run directory gives units with orders.

### `map-gen`

The base maps' PNG layers are generated, never drawn by hand: `map-gen` writes `<map>.height.png` and
`<map>.terrain.png` (one pixel per cell, row 0 north; ADR-0014) from integer code in `fl_cli::map_gen`, the same
bytes on every machine. After changing the generator, regenerate both maps in place and run `data-check` and
the golden replays (the testbed's terrain is in the golden hash stream):

```console
$ cargo run -p fl_cli --release -- map-gen testbed --out assets/base/maps
250x250 cells
assets/base/maps\testbed_1km.height.png
assets/base/maps\testbed_1km.terrain.png
$ cargo run -p fl_cli --release -- --json map-gen scale --out assets/base/maps
{ "cells": [500, 500], "files": ["assets/base/maps\\scale_2km.height.png", "assets/base/maps\\scale_2km.terrain.png"] }
```

`testbed` is `base.testbed_1km` (250 × 250 cells of 4 m: every terrain kind, the river with two bridges at cells
x 70–72 and 180–182, y 122–127, two roads each way, forest blocks, a 12 m hill at cell (60, 200) and a 6 m ridge
at x 140–143, y 140–170); `scale` is `base.scale_2km` (500 × 500 cells, two rivers with bridges every 62 cells,
forest blocks). Written into a scratch directory, both files are byte-identical to the committed ones (checked
in M1 Task 21 with `cmp`).

## 4. The remote debug endpoint: `fl.*` methods

Every method is JSON-RPC 2.0 over `POST http://127.0.0.1:<port>`; each is also published as an agent tool
(`brp_extras/agent_tools` lists them). Errors use the JSON-RPC error object: `-32602` for bad params
(unknown fields are rejected), `-32603` for a failure (`"no match is running"`, a runner failure, a 2 s
timeout). Methods that need the sim send a query to the runner thread and wait at most 2 s. Requests below are
the `method` and `params` members; responses are the `result` (or `error`) member, from a live session on
port 15730 (`fl_game --features debug_remote -- --remote --remote-port 15730 --agent`, `base.testbed`).

| Method | Params | Result |
|---|---|---|
| `fl.sim_state` | none | sim summary: `tick`, `seed`, `header`, `state_hash`, `entities`, `players`, `map`, `last_seq`, plus `last_hash`, `paused`, `speed` |
| `fl.entity` | `{sim_id}` | `{sim_id, components}` (saved sim components by name) or `null` |
| `fl.hash` | none | `{hash, last_tick}`: the newest hash received (the sim hashes every 20 ticks) |
| `fl.issue_command` | `{player, units, command, payload}` | `{exec_tick, seq, player}` |
| `fl.pause` / `fl.resume` | none | `{paused, speed, next_tick}` |
| `fl.step` | `{n}` (default 1, at most 600) | `{tick, completed, requested, finished}` |
| `fl.set_speed` | `{x}` in 0.25..=8 | `{paused, speed, next_tick}` |
| `fl.spawn` | `{unit, player, x, y, heading_deg}` | `{exec_tick, seq, player}` |
| `fl.journal_since` | `{tick, limit}` (limit default 1000) | `[JournalEntry]` |
| `fl.perf` | none | `{profiler, sim, frame}` |
| `fl.screenshot` | `{path}` | `{path, width, height}`; the PNG appears a few frames later |
| `fl.debug_query` | `{name, args}` | the query's JSON (§8) |
| `fl.select` | `{sim_ids}` | `{selected}`: the local player's units among the ids |
| `fl.selection` | none | `{selected}` |
| `fl.toggle_overlay` | `{name}` | `{name, on}` |
| `fl.load_scenario` | `{id}` | `{scenario, tick}` |
| `fl.load` | `{path}` | `{path, tick}` |
| `fl.save_scenario` | `{path, id?}` | `{path, id, units}` |
| `fl.save` | `{path}` | `{path, bytes}` |

### `fl.sim_state`

```json
{"method": "fl.sim_state"}
→ {"tick": 23, "seed": 1,
   "header": {"features": [["core", 1]], "mods": [["base", "0.1.0"]], "rules_hash": 13586675111922715448},
   "state_hash": 16088387657000427651,
   "entities": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
   "players": [{"id": 0, "faction": 0, "team": 0, "controller": "Human"},
               {"id": 1, "faction": 1, "team": 1, "controller": "Human"}],
   "map": {"size": [1000.0, 1000.0], "cell": 4.0, "cells_x": 250, "cells_y": 250},
   "last_seq": {}, "last_hash": "<hash>", "paused": false, "speed": 1.0}
```

Poll it for readiness (`tick >= T`). The game's hash at tick 20 must equal the tick-20 line of `fl_cli hash-dump --mods assets/base --scenario
base.testbed --ticks 20 --interval 20` (the windowed and headless sims agree). The value changes whenever sim state
changes, so compare the two, never a remembered constant. In M1 Task 21 the game's `state hash` log records
for ticks 0, 20 and 40 (`fl_sim::step` at `debug`, in the run directory's `log.jsonl` and on stderr) equalled
`hash-dump --ticks 41`'s three lines. The M1 summary has `"features": [["core", 2]]`, and a player run by the AI
seam shows `"controller": {"Ai": "null"}` (`--scenario base.testbed_ai`).

### `fl.entity`

```json
{"method": "fl.entity", "params": {"sim_id": 11}}
→ {"sim_id": 11, "components": {
     "core.footprint": {"width": 3.6000000000931323, "length": 6.300000000046566, "hitbox_shrink": 0.5},
     "core.health": {"hp": 210, "max": 210},
     "core.mobility": {"class": "Tracked", "speed": 0.5277777777519077, "terrain_factor": 0.5, "turn_rate": 2.7520751953125},
     "core.move_order": {"orders": []}, "core.owner": 0,
     "core.path_follow": {"state": "Idle", "goal": [0.0, 0.0], "corners": [], "next": 0, "planned": 0},
     "core.pose": {"pos": [500.0, 480.0], "heading": 45.0},
     "core.selectable": null, "core.unit_type": 4}}
{"method": "fl.entity", "params": {"sim_id": 1}}
→ {"sim_id": 1, "components": {…,
     "core.move_order": {"orders": [{"Move": {"target": [600.0, 600.0]}}]}, "core.owner": 0,
     "core.path_follow": {"state": "Following", "goal": [600.0, 600.0],
                          "corners": [[290.0, 490.0], [290.0, 514.0], [578.0, 554.0], [582.0, 562.0], [600.0, 600.0]],
                          "next": 0, "planned": 142},
     "core.pose": {"pos": [349.5973269394599, 350.93957047443837], "heading": 99.99755859375}, …}}
{"method": "fl.entity", "params": {"sim_id": 999}}
→ null
```

(Re-run in the M1 final fix wave: #11 is the tiger of the `fl.spawn` request below, applied at tick 137 in
that run; #1 is shown two ticks after the `core.move` of the `fl.journal_since` example.)

### `fl.hash`

```json
{"method": "fl.hash"}
→ {"hash": "<hash>", "last_tick": 22}
```

### `fl.issue_command`

```json
{"method": "fl.issue_command",
 "params": {"player": 0, "units": [1], "command": "core.move", "payload": {"target": [600.0, 600.0]}}}
→ {"exec_tick": 1944, "seq": 3, "player": 0}
{"method": "fl.issue_command",
 "params": {"player": 0, "units": [6], "command": "debug.set_owner", "payload": {"owner": 0}}}
→ {"exec_tick": 2364, "seq": 4, "player": 0}
{"method": "fl.issue_command", "params": {"player": 0, "units": [3], "command": "core.nope"}}
→ error -32603 "unknown command 'core.nope'; known commands: core.follow, core.move, core.rotate, core.stop,
  debug.invulnerable, debug.set_hp, debug.set_owner, debug.set_terrain, debug.spawn, debug.teleport, debug.wander"
{"method": "fl.issue_command",
 "params": {"player": 0, "units": [3], "command": "core.rotate", "payload": {"toward": [410.0, 300.0]}}}
→ {"exec_tick": 10, "seq": 0, "player": 0}
{"method": "fl.issue_command", "params": {"player": 0, "units": [4], "command": "core.follow", "payload": {"leader": 3}}}
→ {"exec_tick": 10, "seq": 1, "player": 0}
{"method": "fl.issue_command", "params": {"player": 0, "units": [], "command": "debug.set_terrain",
 "payload": {"min_cell": [70, 122], "max_cell": [72, 127], "terrain": "base.water"}}}
→ {"exec_tick": 137, "seq": 12, "player": 0}
```

Payloads are the command's JSON form: `core.move {target: [x, y]}`, `core.stop {}`, `core.rotate {toward: [x,
y]}`, `core.follow {leader}`, `debug.wander {radius}`, `debug.set_owner {owner}`, `debug.invulnerable {on}`,
`debug.set_hp {hp}`, `debug.teleport {pos: [x, y]}`, `debug.spawn {unit, owner, pos, heading}`,
`debug.set_terrain {min_cell: [x, y], max_cell: [x, y], terrain}` (cells, inclusive; no units). `debug.*`
commands are issued as the local player and may target any unit (R44); the sim rejects them unless the match
allows cheats (the testbed does). `fl.issue_command` always sends `queue: false`: a Shift-queued order comes only
from the mouse (Shift + right click) or a test. The reply means the command was queued in the driver; whether
the sim applied or rejected it is in the journal (`core.command_applied` / `core.command_rejected`, §8).
(`core.rotate`, `core.follow` and `debug.set_terrain` were missing from `fl.issue_command` until M1 Task 21;
`fl_game`'s `brp_issue_command_m1_payloads_apply` issues each one over BRP and checks its effect.)

### `fl.pause`, `fl.resume`, `fl.step`, `fl.set_speed`

```json
{"method": "fl.pause"}                    → {"paused": true, "speed": 1.0, "next_tick": 1943}
{"method": "fl.step", "params": {"n": 20}} → {"tick": 1963, "completed": 20, "requested": 20, "finished": true}
{"method": "fl.resume"}                   → {"paused": false, "speed": 1.0, "next_tick": 1963}
{"method": "fl.set_speed", "params": {"x": 8.0}} → {"paused": false, "speed": 8.0, "next_tick": 1994}
{"method": "fl.set_speed", "params": {"x": 9.0}} → error -32602 "invalid params: the speed must be in 0.25..=8"
{"method": "fl.step", "params": {"n": 601}}  → error -32602 "invalid params: n = 601 is more than 600 ticks; step in parts"
```

`fl.step` pauses first and returns `tick`, the next tick the sim runs. When 2 s pass first, `finished` is
`false` and the remaining steps stay queued in the driver. A command issued while paused executes at its
`exec_tick`, which the next step or `fl.resume` reaches.

### `fl.spawn`

```json
{"method": "fl.spawn", "params": {"unit": "base.de.tiger", "player": 0, "x": 500.0, "y": 480.0, "heading_deg": 45.0}}
→ {"exec_tick": 163, "seq": 0, "player": 0}
{"method": "fl.spawn", "params": {"unit": "base.nope", "player": 0, "x": 1.0, "y": 1.0}}
→ error -32602 "invalid params: no unit named 'base.nope'"
```

`player` is the new unit's owner; the command itself is issued as the local player (the result's `player`).

### `fl.journal_since`

```json
{"method": "fl.journal_since", "params": {"tick": 137, "limit": 10}}
→ [{"tick": 137, "phase": 0, "kind": "core.command_applied", "sim_id": null, "player": 0, "emitted": 0,
    "data": {"command": "debug.spawn", "seq": 0, "units": [], "queue": false}},
   {"tick": 137, "phase": 6, "kind": "core.unit_spawned", "sim_id": 11, "player": 0, "emitted": 0,
    "data": {"unit": "base.de.tiger", "owner": 0, "pos": [500.0, 480.0]}}]
{"method": "fl.issue_command",
 "params": {"player": 0, "units": [1], "command": "core.move", "payload": {"target": [600.0, 600.0]}}}
→ {"exec_tick": 142, "seq": 1, "player": 0}
{"method": "fl.journal_since", "params": {"tick": 141, "limit": 10}}
→ [{"tick": 142, "phase": 0, "kind": "core.command_applied", "sim_id": null, "player": 0, "emitted": 0,
    "data": {"command": "core.move", "seq": 1, "units": [1], "queue": false}},
   {"tick": 142, "phase": 0, "kind": "core.order_changed", "sim_id": 1, "player": 0, "emitted": 0,
    "data": {"orders": 1, "current": "move", "reason": "command"}},
   {"tick": 142, "phase": 0, "kind": "core.path_request", "sim_id": 1, "player": null, "emitted": 0,
    "data": {"request": 0, "goal": [600.0, 600.0], "profile": {"class": "Tracked", "size": 1}, "kind": "single"}},
   {"tick": 142, "phase": 1, "kind": "core.path_result", "sim_id": 1, "player": null, "emitted": 0,
    "data": {"request": 0, "status": "found", "corners": 5, "cost": 15481, "charge": 1844, "waited": 0}}]
```

(Re-run in the M1 final fix wave, the same run as the `fl.entity` example.)

### `fl.perf`

```json
{"method": "fl.perf"}
→ {"profiler": {"ticks": 2398,
                "tick": {"mean_ms": 0.423, "p50_ms": 0.380, "p95_ms": 0.516, "max_ms": 12.336, "samples": 2398},
                "phases": [["Input", {"mean_ms": 0.125, "p50_ms": 0.0745, "p95_ms": 0.144, "max_ms": 12.004, "samples": 2398}],
                           ["Think", {…}], ["Act", {…}], ["Resolve", {…}], ["Economy", {…}],
                           ["Perceive", {…}], ["Lifecycle", {…}], ["Publish", {…}]],
                "features": [["bevy", {…}], ["client.camera", {…}], ["client.debug", {…}], …],
                "systems": [{"name": "…", "feature": "bevy", "stat": {…}, "history_ms": […]}, …],
                "history_ms": […]},
   "sim": {"counters": {"publish.rebuilt": 6, "publish.records": 12, "spatial.inserts": 0, "spatial.relocations": 0}},
   "frame": {"fps": 58.08, "frame_time_ms": 17.22, "entities": 594, "unit_visuals": 12,
             "visible_unit_visuals": 12, "visible_meshes": 12}}
```

(Numbers rounded here.) `profiler` covers the last 240 ticks/frames; `sim.counters` are the work counters of
the last tick; `frame` is the client. The F2 panel shows the same data.

### `fl.screenshot`

```json
{"method": "fl.screenshot", "params": {"path": "C:/fl/a/docs/demo/m0-01-iso-testbed.png"}}
→ {"path": "C:/fl/a/docs/demo/m0-01-iso-testbed.png", "width": 1280, "height": 720}
```

An offscreen camera copies the game camera's transform and projection and renders 1280×720, so it works with
the window minimized or behind other windows (the debug UI is not in it). Poll for the file. For the whole
window, UI included, `brp_extras/screenshot {path}` captures the window (black when the window is hidden).

### `fl.toggle_overlay`

```json
{"method": "fl.toggle_overlay", "params": {"name": "grid"}} → {"name": "grid", "on": true}
{"method": "fl.toggle_overlay", "params": {"name": "paths"}} → {"name": "paths", "on": true}
{"method": "fl.toggle_overlay", "params": {"name": "nope"}}
→ error -32602 "invalid params: unknown overlay 'nope'; known overlays: cells, grid, passability, paths"
```

The overlay names match the keys: `grid` F3, `paths` F5, `passability` F6, `cells` F7 (§1). The M1 overlays
poll their debug query (`core.paths`, `core.passability`, `core.clusters`) every 15 frames and draw it with
gizmos, which `fl.screenshot` captures (`docs/demo/m1-04-group-on-bridge.png`, `m1-05`…`m1-07`).

### `fl.debug_query`

```json
{"method": "fl.debug_query", "params": {"name": "core.debug_queries"}}
→ {"queries": ["core.clusters", "core.debug_queries", "core.passability", "core.paths", "core.terrain_at"]}
{"method": "fl.debug_query", "params": {"name": "core.paths", "args": {"units": [1, 2]}}}
→ {"1": {"state": "following", "goal": [3027951943680, 3264175144960],
         "corners": [[3100966387712, 2087354105856], [3083786518528, 2413771620352], [3027951943680, 3264175144960]],
         "next": 0, "orders": [{"move": [3027951943680, 3264175144960]}], "profile": {"class": "Tracked", "size": 1}},
   "2": {"state": "following", "goal": [3156800962560, 3264175144960], …}}
{"method": "fl.debug_query", "params": {"name": "core.nope"}}  → error -32603 "unknown debug query 'core.nope'"
{"method": "fl.debug_query", "params": {"name": "core.paths", "args": {"units": [99]}}}  → error -32603 "no unit #99"
```

That is the brief's BRP smoke: `fl.select {sim_ids: [1, 2]}`, `fl.issue_command core.move {target: [720, 760]}`
for units 1 and 2, `fl.step {n: 200}`, then `core.paths`: both units cross the river on the east bridge (the
corner (718, 562) m lies north of the bridge cells x 180–182) toward their formation slots at y = 760.
`args` defaults to `{}`; the queries and their arguments are in §8.

### `fl.select`, `fl.selection`

```json
{"method": "fl.select", "params": {"sim_ids": [1, 2]}}            → {"selected": [1, 2]}
{"method": "fl.select", "params": {"sim_ids": [1, 2, 6, 999]}}    → {"selected": [1, 2]}
{"method": "fl.selection"}                                        → {"selected": [1, 2]}
```

`fl.select` replaces the selection with the local player's units among the ids (unit 6 belongs to player 1,
999 does not exist); the selection feeds the overlays, the order keys and the control groups exactly as a mouse
selection does. To drive real input instead, `brp_extras/move_mouse {position: [x, y]}` (logical pixels),
`brp_extras/click_mouse {button: "Left" | "Right"}`, `double_click_mouse`, `drag_mouse {button, start, end,
frames}` and `send_keys {keys: ["ShiftLeft", "Digit1"], duration_ms}` all reach the game's selection and
order input (M1 Task 21 walked the demo checklist with them). To aim at a unit, project its visual's
`Transform` (`world.query` with `bevy_ecs::hierarchy::Children`, without `Camera3d`) through the camera's
`Transform` and `Projection` (orthographic, `FixedVertical`).

### `fl.save`, `fl.load`, `fl.save_scenario`, `fl.load_scenario`

```json
{"method": "fl.pause"}
{"method": "fl.sim_state"}                       → {"tick": 2584, "state_hash": 15586365802526815550, …}
{"method": "fl.save", "params": {"path": "C:/fl/a/docs/demo/m0-demo.save"}}
→ {"path": "C:/fl/a/docs/demo/m0-demo.save", "bytes": 4053}
{"method": "fl.step", "params": {"n": 40}}       → {"tick": 2624, …}; fl.sim_state → state_hash 11263170666591694002
{"method": "fl.load", "params": {"path": "C:/fl/a/docs/demo/m0-demo.save"}}
→ {"path": "C:/fl/a/docs/demo/m0-demo.save", "tick": 2584}
{"method": "fl.sim_state"}                       → {"tick": 2584, "state_hash": 15586365802526815550, …}
{"method": "fl.step", "params": {"n": 40}}       → {"tick": 2624, …}; fl.sim_state → state_hash 11263170666591694002
{"method": "fl.save_scenario", "params": {"path": "C:/fl/a/docs/demo/m0-demo.scenario.ron"}}
→ error -32603 "the match did not start from a scenario; nothing to base it on"
{"method": "fl.load_scenario", "params": {"id": "base.testbed"}} → {"scenario": "base.testbed", "tick": 0}
{"method": "fl.save_scenario", "params": {"path": "C:/fl/a/docs/demo/m0-demo.scenario.ron"}}
→ {"path": "C:/fl/a/docs/demo/m0-demo.scenario.ron", "id": "m0-demo", "units": 10}
```

The loaded match resumes at the saved tick with the saved hash, and 40 ticks later matches the original run.
`fl.save_scenario` needs a match started from a scenario (after `fl.load` there is none); the file is a
`ScenarioDef` that `fl_cli run-scenario <path>` and `--scenario`-style tools take.

### Client-world methods

`world.query`, `world.list_components`, `world.get_components` and the `brp_extras/*` input methods work on the
**client** World only. Useful ones in M0:

```json
{"method": "world.query", "params": {"data": {"components": ["bevy_transform::components::transform::Transform",
  "bevy_camera::projection::Projection"]}, "filter": {"with": ["bevy_camera::components::Camera3d"]}}}
→ [{"entity": …, "components": {"bevy_transform::components::transform::Transform":
     {"translation": [724.1, 230.9, 724.4], …}, "bevy_camera::projection::Projection":
     {"Orthographic": {"scaling_mode": {"FixedVertical": {"viewport_height": 335.45}}, …}}}}]
{"method": "brp_extras/send_keys", "params": {"keys": ["KeyD"], "duration_ms": 1000}}   (pan; Q/E rotate, Home snap)
{"method": "brp_extras/scroll_mouse", "params": {"x": 0.0, "y": 5.0, "unit": "Line"}}   (zoom in; negative y zooms out)
{"method": "brp_extras/shutdown"}
```

`fl_client::camera::RtsCamera` is not reflected, so filter on `Camera3d`. Typed text
(`brp_extras/type_text`) does not reach the egui console's text field; drive the sim with `fl.*` instead
(typed letters also reach the camera keys).

## 5. Where things are written

| What | Where |
|---|---|
| `fl_game` run directory (debug builds) | `logs/<run-id>/` in the working directory; `<run-id>` = `<UTC yyyymmdd-hhmmss>-<pid>`; `logs/latest.txt` names the newest |
| `fl_cli run-scenario` run directory | `<--out, default logs>/<run-id>/`, also named in `<out>/latest.txt` |
| Structured log | `<run>/log.jsonl`: one JSON record per line (`timestamp`, `level`, `target`, `fields`, `spans`, `tick`) |
| Sim journal | `<run>/journal.jsonl`: one `JournalEntry` per line (`tick`, `phase`, `kind`, `sim_id`, `player`, `emitted`, `data`) |
| Replay | `<run>/run.replay` (`fl_cli run-scenario`); golden replays in `crates/fl_test_utils/tests/replays/` (protected) |
| State dumps | `<run>/states.jsonl` with `run-scenario --dump-state-every N` (the world and every entity's components) |
| Saves | wherever `fl.save {path}` or `fl_cli save --out` puts them (`*.save`, postcard) |
| Screenshots | wherever `fl.screenshot {path}` puts them (PNG, 1280×720) |
| Chrome traces | `trace-<timestamp>.json` in the working directory (`--features trace_chrome`) |
| Bench results | `docs/perf/M<n>.md` and `docs/perf/M<n>.json` (`fl_cli bench --out … --json-out …`) |
| Gate logs | `target/xtask/<step>.log` |

Journal record kinds: `core.command_applied`, `core.command_rejected` (with the reason code),
`core.unit_spawned`, `core.unit_despawned`, and since M1 `core.order_changed` (replacing M0's
`core.move_order`), `core.path_request`, `core.path_result`, `core.terrain_changed` and `core.content_reload`
(§8). Setup spawns (scenario units) are not journaled; in-tick spawns (`debug.spawn`) are. The sim records
the journal when `RunConfig.journal` says so, by default only in debug builds (`debug_assertions`); `fl_cli
run-scenario` turns it on, so a release run writes `journal.jsonl` too, while a release game records none.

## 6. Reproduce a bug

A bug report is **seed + scenario (+ the commands)**, which is exactly a replay. The recipe:

1. **Pin the inputs.** Note the seed and scenario (and mods). From the live game, `fl.save {path}` captures
   the exact state and pending commands; `fl.save_scenario {path}` turns the current units into a
   `*.scenario.ron` that `run-scenario` takes as a path.
2. **Record it headless with full logs:**
   `cargo run -p fl_cli -- run-scenario base.testbed --seed 1 --wander 100 --ticks 400 --trace --dump-state-every 20`
   → `logs/<run-id>/` with `log.jsonl` (every record at `trace`, each with its tick), `journal.jsonl`,
   `states.jsonl` and `run.replay`.
3. **Check it reproduces:** `cargo run -p fl_cli -- replay-verify logs/<run-id>/run.replay`. Exit 0: the
   replay is deterministic and you can bisect it. Exit 1 (`diverged at tick T`): the sim is nondeterministic on
   this machine; compare against a shuffled run (`FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_sim`)
   to find an order dependence. Exit 3: the replay holds an event (such as a hot reload) that cannot be
   re-simulated.
4. **Find the first bad tick:** record the good behaviour too (an older commit, another seed, or the command
   set without the suspect command) and run
   `cargo run -p fl_cli -- --json bisect-desync logs/<good> logs/<bad>`: `tick` is the first tick whose hashes
   differ (`setup: true` if the worlds differ before tick 0) and `entities` the `SimId`s whose state differs
   there.
5. **Look at the entity:** `cargo run -p fl_cli -- --json inspect logs/<bad> --tick <T> --entity <id>` (and
   `--tick <T-1>` for the state before), or the same in the running game with `fl.entity {sim_id}` after
   `fl.step`.
6. **Read why:** `cargo run -p fl_cli -- --json journal logs/<bad> --from <T-20> --to <T>` lists the commands
   applied and rejected (with the reason; their units are in `data.units`), move orders, spawns and despawns;
   `--sim-id <id>` keeps the records of one entity (spawn, despawn, move order), `--kind` one kind. Then read the
   `log.jsonl` records with `"tick": T` on the target of the system involved (`fl_sim::movement`, …).
7. **Pin it:** turn the run into a test (a `fl_test_utils` scenario test or, once blessed by the lead, a golden
   replay under `crates/fl_test_utils/tests/replays/`, written as `*.replay.new`).

## 7. Dynamic linking (Task 26)

`cargo run -p fl_game --features dynamic` enables `bevy/dynamic_linking` for faster relinks in dev. The Task 26
probe (`probes/dynlink`, built against `a28e79b`; `docs/research/bevy020-api-reference.md` "Probe:
dynamic_linking") showed that it links and runs with the sim crates, which depend only on `bevy_app` and
`bevy_ecs`: no duplicate-crate, link or type-mismatch error, and a sim `App` crossed the dylib boundary three
ways. `fl_game --features dynamic` builds; a windowed run was not tested. Never use it for gate, bench or
release builds. Probes are historical evidence and may no longer compile (`probes/README.md`).

## 8. Pathing, terrain and orders (M1)

M1 adds terrain, budgeted path requests, flow fields, steering and an order queue (spec §5.7, §7; ADR-0014,
ADR-0015). Everything below is observe-only: the sim never reads the journal, counters or spans, and debug
queries never change state. The examples come from the M1 Task 21 sessions (`fl_cli` and the live game).

### Journal kinds

Each record is a `JournalEntry` (`tick`, `phase`, `kind`, `sim_id`, `player`, `emitted`, `data`), in
`journal.jsonl`, `fl.journal_since` and `fl_cli journal`. In `data`, positions are metres.

| Kind | When | `data` |
|---|---|---|
| `core.command_applied` | a command was applied (phase `Input`) | `command`, `seq`, `units`, `queue` |
| `core.command_rejected` | a command was rejected | `command`, `seq`, `reason` (a stable code, below) |
| `core.order_changed` (replaces M0's `core.move_order`) | a unit's order queue changed | `orders` (how many), `current` (`move`, `rotate`, `follow` or `none`), `reason`: `command`, `completed`, `failed`, `stopped` or `replanned` |
| `core.path_request` | a path request entered the `PathQueue` | `request` (its id), `goal`, `profile` `{class, size}`, `kind` `single`, or `group` with `units` |
| `core.path_result` | a request was processed (phase `Think`) | `request`, `status` (`found`, `partial`, `unreachable`, `stale`; `flow` for a group), `corners`, `cost`, `charge` (the budget charged: its cold cost, 0 when stale), `waited` (ticks in the queue) |
| `core.terrain_changed` | `debug.set_terrain` changed cells | `min`, `max` (cells, inclusive), `terrain` |
| `core.content_reload` | a value hot reload was applied in place | `old_rules_hash`, `rules_hash`, `units` |
| `core.unit_spawned`, `core.unit_despawned` | in-tick spawn and despawn (as in M0) | `unit`, `owner`, `pos` / the despawn |

```json
{"tick": 28, "phase": 0, "kind": "core.path_request", "sim_id": 11, "player": null, "emitted": 0,
 "data": {"request": 0, "goal": [300.0, 720.0], "profile": {"class": "Tracked", "size": 1}, "kind": "group",
          "units": [11, 12, 13, 14, 15, 16]}}
{"tick": 28, "phase": 1, "kind": "core.path_result", "sim_id": 11, "player": null, "emitted": 0,
 "data": {"request": 0, "status": "flow", "corners": 0, "cost": 0, "charge": 1024, "waited": 0}}
{"tick": 293, "phase": 1, "kind": "core.path_result", "sim_id": 1, "player": null, "emitted": 0,
 "data": {"request": 0, "status": "found", "corners": 3, "cost": 19810, "charge": 2085, "waited": 0}}
{"tick": 148, "phase": 0, "kind": "core.terrain_changed", "sim_id": null, "player": 0, "emitted": 0,
 "data": {"min": [70, 122], "max": [72, 127], "terrain": "base.water"}}
{"tick": 3213, "phase": 0, "kind": "core.content_reload", "sim_id": null, "player": null, "emitted": 0,
 "data": {"old_rules_hash": 1092586514522127015, "rules_hash": 11959931894920095309, "units": 16}}
{"tick": 10, "phase": 0, "kind": "core.order_changed", "sim_id": 3, "player": 0, "emitted": 0,
 "data": {"orders": 1, "current": "rotate", "reason": "command"}}
{"tick": 3238, "phase": 0, "kind": "core.order_changed", "sim_id": 1, "player": null, "emitted": 0,
 "data": {"orders": 1, "current": "move", "reason": "replanned"}}
```

A unit following a flow field has no corners of its own: its group request answers `flow`, and a terrain
change rebuilds the touched flow tiles without a new request (only `core.terrain_changed` and, on arrival,
`core.order_changed` `completed`).

### Reject codes

Every rejection is reported by its stable code, never a `Debug` name (R71): in `SimEvent::CommandRejected
{player, seq, reason}` (the client receives it with the tick output; `fl_cli` counts them as
`rejected_commands`), in the `core.command_rejected` journal record and in the counter `commands.rejected`.
M1 adds no variant; it uses:

| Code | Meaning |
|---|---|
| `core.invalid_target` | `RejectReason::Feature("core.invalid_target")` (R79): `core.follow` of a leader that does not exist; `debug.spawn` or `debug.teleport` onto a cell the unit cannot stand on; `debug.set_terrain` with a rect off the map |
| `core.queue_full` | a queued (`queue: true`) `core.move`, `core.rotate` or `core.follow` onto a unit that already has `MAX_WAYPOINTS` = 32 orders: the whole command is rejected and nothing changes |
| `core.too_many_units` | more than `MAX_COMMAND_UNITS` = 256 units in one command (the client splits larger selections, R88) |
| `core.unknown_content` | `debug.set_terrain` naming a terrain id the content lacks (as M0's `debug.spawn` with an unknown unit) |

The M0 codes stay: `core.late`, `core.early`, `core.duplicate_seq`, `core.unknown_type`, `core.bad_payload`,
`core.unknown_player`, `core.debug_disabled`, `core.no_valid_units`. A move into water is **not** rejected: the
goal snaps to the nearest cell the unit can stand on (`Path.snap_radius_cells`), and an unreachable goal ends
as `partial` (the closest reachable point) or `unreachable` (the unit stops).

```json
{"tick": 10, "phase": 0, "kind": "core.command_rejected", "sim_id": null, "player": 0, "emitted": 0,
 "data": {"command": "core.follow", "seq": 2, "reason": "core.invalid_target"}}
{"tick": 3242, "phase": 0, "kind": "core.command_rejected", "sim_id": null, "player": 0, "emitted": 0,
 "data": {"command": "debug.spawn", "seq": 24, "reason": "core.invalid_target"}}
{"tick": 3245, "phase": 0, "kind": "core.command_rejected", "sim_id": null, "player": 0, "emitted": 0,
 "data": {"command": "core.move", "seq": 57, "reason": "core.queue_full"}}
{"tick": 13, "phase": 0, "kind": "core.command_rejected", "sim_id": null, "player": 0, "emitted": 1,
 "data": {"command": "debug.set_terrain", "seq": 4, "reason": "core.unknown_content"}}
```

(`core.queue_full` came from 34 Shift + right clicks on one Pz III: queued moves on top of its current order
filled the 32 slots, and the next clicks were rejected.)

### Counters

Work counters per tick (`SimCounters`; `fl.perf` `sim.counters` shows the last tick's, `bench` the mean per
tick; budgets in `docs/perf/budgets.ron`, ADR-0010). The M1 ones:

| Counter | Counts |
|---|---|
| `path.requests` | path requests processed this tick |
| `path.queue_len` | requests left in the `PathQueue` after processing |
| `path.nodes_expanded` | A*/HPA* nodes expanded |
| `path.los_cells` | cells tested by path smoothing's line-of-sight checks |
| `path.budget_spent` | budget charged (every charge, group requests' included; cold cost, ADR-0015) |
| `path.clusters_invalidated` | HPA* clusters rebuilt after a terrain change (over all profiles) |
| `flowfield.cells` | flow-tile cells settled (group builds and read-miss rebuilds; depends on the cache, R86) |
| `flowfield.abstract_nodes` | abstract-graph nodes settled for flow integrations |
| `nav.cells_updated` | `NavGrid` cells recomputed after a terrain change |
| `steer.units`, `steer.pairs` | units steered, neighbour candidates examined by separation |
| `move.blocked` | units that moved nowhere though they had a step or a push |
| `move.los_cells` | line-of-sight cells movement tested (sweeps, look-ahead, arrival; not path budget) |

### Spans

`tracing::info_span!` spans, timed by the profiler (Chrome/Tracy traces with `--features trace_chrome` /
`trace_tracy`): `hpa_search` per path search (fields `class`, `size`), `hpa_cluster_build` per rebuilt HPA*
cluster (`cx`, `cy`, `size`), `flowfield_build` per abstract integration and per tile (`kind`, `cluster`),
inside the per-tick `sim_tick` span (field `tick`). Per-request and
per-unit log records (path requests, steering, flow tiles) are at `trace` level: turn them on for one module
with `RUST_LOG=info,fl_sim::path=trace` rather than globally (at 3,000 units, `debug` per spawn cost about
30 ms per tick in M0).

### Debug queries

`SimQuery::Debug { name, args }` answers JSON; `fl.debug_query {name, args}` (§4), `fl_cli query <source>
--tick T --name N --args JSON` (§3) and the F5–F7 overlays all use it. `Fx` values are raw `i64` (metres ×
2^32); cells are `[x, y]` with `x` east, `y` north, index `y × cells_x + x`. A profile is `{class, size}`
(`class` one of `Foot`, `Wheeled`, `HalfTrack`, `Tracked`; `size` the clearance in cells, default 1) or
`{unit: id}` for that unit's profile. Bad or missing arguments are an error, never a panic.

| Name | Args | Answer |
|---|---|---|
| `core.debug_queries` | none | `{"queries": [every name, sorted]}` |
| `core.paths` | `{"units": [id, …]}` | per unit id: `state` (`idle`, `waiting`, `following`, `flow`, `done`, `failed`), `goal`, `corners`, `next` (the next corner's index), `orders` (`{"move": [x, y]}`, `{"rotate": raw angle}`, `{"follow": id}`), `profile` |
| `core.passability` | a profile | `{class, size, cells: [w, h], version, blocked: [[start, len], …]}`: row-major runs of cells the profile cannot enter |
| `core.clusters` | a profile | `{cluster_cells, clusters: [cx, cy], entrances: [[x, y], …], nodes}`: the profile's HPA* graph |
| `core.terrain_at` | `{"cell": [x, y]}` | `{cell, terrain, height_raw, blocked: {class: bool}, clearance: {class: n}, cost: {class: n}}` |

```json
{"method": "fl.debug_query", "params": {"name": "core.terrain_at", "args": {"cell": [71, 124]}}}
→ {"blocked": {"Foot": false, "HalfTrack": false, "Tracked": false, "Wheeled": false}, "cell": [71, 124],
   "clearance": {"Foot": 2, "HalfTrack": 2, "Tracked": 2, "Wheeled": 1},
   "cost": {"Foot": 100, "HalfTrack": 100, "Tracked": 100, "Wheeled": 100}, "height_raw": 0, "terrain": "base.bridge"}
{"method": "fl.debug_query", "params": {"name": "core.passability", "args": {"unit": 1}}}
→ {"blocked": [[7600, 31], [7850, 31], [8100, 31], …], "cells": [250, 250], "class": "Tracked", "size": 1,
   "version": 0}                                                  (119 runs)
{"method": "fl.debug_query", "params": {"name": "core.passability", "args": {"class": "Wheeled", "size": 2}}}
→ {"blocked": [[249, 1], [499, 1], [749, 1], …], "cells": [250, 250], "class": "Wheeled", "size": 2, "version": 0}
{"method": "fl.debug_query", "params": {"name": "core.clusters", "args": {"class": "Tracked"}}}
→ {"cluster_cells": 16, "clusters": [16, 16], "entrances": [[15, 0], [0, 15], [15, 15], …], "nodes": 1028}
{"method": "fl.debug_query", "params": {"name": "core.terrain_at", "args": {"cell": [900, 1]}}}
→ error -32603 "cell [900, 1] is off the 250x250 map"
{"method": "fl.debug_query", "params": {"name": "core.clusters", "args": {"class": "HalfTrack", "size": 3}}}
→ error -32603 "no navigation graph for HalfTrack size 3: no unit of the content moves with it"
```

(`core.paths`' example is in §4 `fl.debug_query`.) After `terrain 70 122 72 127 base.water`, `core.terrain_at
{cell: [71, 124]}` answers `"terrain": "base.water"`.

## 9. Why didn't my unit move

Work from the order to the ground, with the unit's `SimId` N (the inspector, `fl.selection`, or `fl_cli
inspect … --tick T` lists them):

1. **Its order and path:** `fl.debug_query {name: "core.paths", args: {units: [N]}}` (or `fl_cli query <run>
   --tick T --name core.paths --args '{"units": [N]}'`).
   - `orders` empty and `state: "idle"`: it has no order. Check the command reached the sim: `fl_cli journal
     <run> --kind core.command_rejected` (the `reason` code, §8) or `--kind core.command_applied` (its
     `units` must list N; units of another player and units without `Mobility` are filtered out).
   - `state: "waiting"`: its request is still in the queue (`path.queue_len`, budget `Path.budget_per_tick`).
   - `state: "failed"` or `done` with the unit short of the goal: read the result (step 2).
   - `state: "flow"`: it follows its group's flow field and stops once inside the destination cluster, near its
     formation slot.
   - `orders` starting with `{"follow": id}` or `{"rotate": …}`: the queue's head is not a move (a queued follow
     blocks the queue behind it in M1, R107).
2. **The path result:** `fl_cli journal <run> --kind core.path_result --sim-id N` (or `fl.journal_since` and
   filter on `sim_id`). `status`: `found` (a path; `corners` and `cost`), `partial` (the goal is unreachable:
   it drives to the closest reachable point), `unreachable` (nothing reachable: it stops), `stale` (the order
   changed before the result; the newer request counts), `flow` (group). The matching `core.path_request` has
   the `goal` after snapping and the `profile` it was planned for.
3. **Where it may go:** `fl.debug_query {name: "core.passability", args: {unit: N}}`: the blocked cells of its
   profile. Toggle F6 with the unit selected to see them around the camera. A goal inside a blocked region, or
   on an island the profile cannot reach, gives `partial` or `unreachable`.
4. **Why a cell is blocked:** `fl.debug_query {name: "core.terrain_at", args: {cell: [x, y]}}` with the cell
   (`floor(metres / 4)` on the testbed): `terrain`, `height_raw` (a slope over `Movement.max_slope_m` of the
   class blocks), `blocked`, `clearance` and `cost` per class. A cell whose clearance is below the profile's
   `size` is too narrow for it.

If the path is fine but the unit still stands: `move.blocked` in `fl.perf` counts units that moved nowhere
though they had a step (a crowd jam, R115, R117), and a unit stranded on a blocked cell (after
`debug.set_terrain` under it) re-plans every 10 ticks (R116).

## 10. Hot reload

In a dev build with `debug.ron` `hot_reload: true`, `fl_game` scans every file under the mod directories every
`hot_reload_poll_ms` (500 ms) and reloads once a change has settled; `reload` in the console forces one. The
outcome goes to the console (window capture `docs/demo/m1-09-reload-error-console.png`) and the log
(`fl_game::reload`):

| Change | Outcome |
|---|---|
| only visuals (`*.visual.ron`, colours) | `visuals reloaded; the rules are unchanged`: the client swaps its content |
| values, same ids (unit stats, terrain `pass`, rules) | `content reloaded in place at tick T (rules hash …)`: the runner applies it between ticks (`RunnerControl::ReloadContent`), every unit keeps its `SimId`, `Health` keeps its ratio, derived nav data is rebuilt; journal `core.content_reload` |
| ids added or removed (a new or deleted `*.unit.ron`) | `hot reload rebuilt the match tick=T dropped=n` (log): the match is saved and loaded under the new content (`LoadRequest::Rebuild`); units whose type vanished are dropped and logged, the others keep their `SimId`s |
| a file that does not load | `reload failed; the match keeps running:` and every error with file, line and column |

Seen live in M1 Task 21 (paused at tick 3212): Pz III `hp: 110` → `150` → `200` reloaded in place; a Pz III at
75/150 became 100/200. Road `pass: 1.0` → `0.5` changed `core.terrain_at`'s road cost from 100 to 167
(Wheeled) and 134 (Tracked). A missing comma gave `pz3.unit.ron:6:26: base.de.pz3/components.Hull: Expected
comma`, and the match kept its rules hash and ticked on. Adding `t21_probe.unit.ron` rebuilt the match
(`dropped=0`, same entities and poses); a spawned probe tank was dropped when the file was deleted
(`dropped=1`, survivors kept their ids).

Limits:
- **Dev builds only.** The watcher is not built into release builds (`cfg!(debug_assertions)`), and only a
  match with `cheats_allowed` (the testbed's) reloads.
- **Never under lockstep (R133).** A match driven by the network (`NetDriven`) or without a `LocalDriver`
  refuses every reload.
- **Same-length, same-mtime edits are missed.** The watcher rereads a file only when its modification time or
  length changed; a tool that rewrites a file with the same length within the file system's mtime resolution
  goes unnoticed. Save it again or type `reload`.
- **The terrain view follows `debug.set_terrain` only in debug builds** (R134): it reads the journal's
  `core.terrain_changed`, which release builds do not record, so there the sim changes but the drawn terrain
  stays as it was.
- `HashInterval` is not refreshed by a reload (R120), and a component removed from a unit file stays on live
  units (R121).
- A replay with a reload event (`replay save` after a reload) is non-reproducible: `replay-verify` exits 3.
