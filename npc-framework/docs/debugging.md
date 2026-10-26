# Debugging, tracing and profiling

Everything a human or an AI agent needs to understand the sim is reachable as **JSON**: a headless CLI
(`npcctl`), a localhost JSON-RPC interface on the running games (Bevy Remote Protocol, BRP), and a headless
real-time session host for external agents. Debug tooling never changes sim results (ADR 0006): with the
journal and the profiler on or off, the same seed and commands give the same state hash.

Sources: the tooling spec (`docs/superpowers/specs/2026-10-23-debug-perf-tooling-design.md`), the agent-control
spec (`…/2026-10-23-agent-control-design.md`), ADRs 0006 and 0010. Agent experiments have their own guide,
[`agent-experiments.md`](agent-experiments.md); integrating the framework into a game is
[`using-npc-framework.md`](using-npc-framework.md).

- [1. Which tool for which question](#1-which-tool-for-which-question)
- [2. npcctl, the headless CLI](#2-npcctl-the-headless-cli)
- [3. The live games and BRP](#3-the-live-games-and-brp)
- [4. Agent, world and session methods](#4-agent-world-and-session-methods)
- [5. The journal](#5-the-journal)
- [6. Inspection, state hash and replays](#6-inspection-state-hash-and-replays)
- [7. Performance: profiler, budgets, bench, Tracy, Chrome traces](#7-performance)
- [8. Gates and guardrails](#8-gates-and-guardrails)
- [9. Recipes](#9-recipes)

Re-baselining replays and snapshots after an intended change: [`rebaselining.md`](rebaselining.md).

## 1. Which tool for which question

| Question | Tool |
|---|---|
| What happens over N ticks with seed S? | `npcctl run` |
| What did unit X decide, and why? | `npcctl journal`, then `npcctl inspect` (or `/why`) |
| What is unit X's state at tick T? | `npcctl inspect --tick T --unit X` |
| Does this replay still produce the same hashes? | `npcctl replay verify` |
| Where do two runs first differ? | `npcctl diverge` (or `/debug-desync`) |
| Which mechanic is slow? | `npcctl bench` (release), F4 panel, BRP `npc/perf` |
| What does the running window show? | BRP `npc/state`, `npc/unit`, `npc/screenshot` (or `/verify-visual`) |
| Control a unit as an external agent | `npcctl serve` + `npcctl agent …` / `npcctl mcp` ([agent-experiments.md](agent-experiments.md)) |
| Is the repo green? | `cargo xtask verify` |

Ids: every sim entity has a `SimId` (`u32`), assigned in spawn order and never reused. The default village
villager is **5**; the testbed's units are **33, 35, 36, 38** (player 1: carpenter, smith, woodcutter, scout)
and **39–42** (player 2: skeletons), the cart **29** and the wagon **30**; in `testbed:smithy` the smith is
**33**, the joiner **35** and the apprentice **37**. `npcctl run --scenario testbed --ticks 1` prints the
testbed's names and ids (`content_summary`); the windowed apps log them at startup.

Positions are integer millimetres `[x, z]` (`SimPos`), +z is north; needs are milli-units `0..=100000`. The
village day is 2400 ticks at 20 ticks/s (2 minutes); night is its last 30 %.

## 2. npcctl, the headless CLI

`cargo run -p npcctl -- <command>` (add `-q` to silence cargo; use `--release` for `bench`). It never opens a
window and never links the renderer (`arch_fitness` pins it).

### 2.1 Global behaviour

- Flags on every command: `--pretty` (indent JSON), `--out FILE` (write the JSON there; created on first write),
  `--log off|error|warn|info|debug|trace` (default `warn`; logs go to **stderr**).
- **Exit codes:** `0` ok; `1` a check failed (replay mismatch, divergence, bench regression, a rejected agent
  action); `2` usage or input error (clap, unreadable or invalid file, rejected command, unknown unit, unknown
  journal kind, RPC/transport error).
- **Errors:** non-clap failures print `{"error": "...", ...}` on stdout as well as a message on stderr, e.g.

  ```json
  {"error":"unknown journal kind `Bogus` (expected one of ThinkStart, ...)","kinds":["ThinkStart", "..."]}
  ```

### 2.2 World selection (`run`, `inspect`, `journal`, `replay record`, `diverge` in seed mode)

| Flag | Meaning |
|---|---|
| `--seed N` | world seed (default 1; a scenario file: its own `seed`, else 1) |
| `--villagers K` | village size (default 1; village scenarios only) |
| `--scenario S` | default `village` for K = 1, else `village_n` |
| `--commands FILE` | commands applied at their ticks |

Scenario names are resolved by npcctl's factory registry (`apps/npcctl/src/sim.rs`, `CONTENTS`; the village is
last and takes every other name, so village output and errors never changed):

- `village`, `village_n` (one plot per villager), `village_nav` (a wall per plot, `NavAgent` villagers);
- `testbed` (the Actor Testbed showcase) or `testbed:<name>`, an embedded testbed scenario: `doors`,
  `locked_store`, `chop`, `craft`, `perception`, `vehicles`, `formation`, `agent`, `smithy` (a bare embedded
  name such as `smithy` is qualified to `testbed:smithy`);
- a scenario **file** (ends in `.json` or contains a path separator): `{"version":1,"content":..}`, built by its
  content's `npc_agent_io` scenario loader (village or testbed), exactly as `serve` builds it.

`--commands FILE` takes a JSON array of `{"tick":T,"cmd":{SimCommand}}` items, of `[tick, seq, {SimCommand}]`
tuples (the two may be mixed), or a whole replay file (its commands are used). Commands are sorted by
`(tick, seq)` and queued just before their tick with `submit_deferred`: each is validated when it applies,
after the earlier commands of its tick, so a command may target a unit an earlier command of the same tick
spawned. A rejected command exits 2 with `{"error","index","tick","cmd","reason":<CommandError>}`.

```json
[
  {"tick": 100, "cmd": {"type": "Order", "unit": 5, "order": {"type": "GoTo", "pos": [8000, 8000]}}},
  {"tick": 200, "cmd": {"type": "SetNeed", "unit": 5, "need": "Hunger", "value": 10000}},
  {"tick": 300, "cmd": {"type": "SetFood", "stockpile": 3, "food": 0}},
  {"tick": 400, "cmd": {"type": "SpawnVillager", "pos": [1000, 1000], "home": null}},
  {"tick": 500, "cmd": {"type": "Despawn", "unit": 5}}
]
```

Testbed commands, from the corpus (`testbed_orders_craft`, scenario `testbed:smithy`: the smith 33 fetches the
store key 26, unlocks and opens the store door 12, takes the pickaxe 17, mines the ore rock 24, then forges at
the forge 15; `testbed_vehicles`: units mount the cart 29, the cart is ordered, then unloaded):

```json
[
  {"tick": 10, "cmd": {"type": "IssueOrder", "player": 1, "units": [33], "order": {"type": "Interact", "verb": "pick_up", "target": 26}, "queue": false}},
  {"tick": 10, "cmd": {"type": "IssueOrder", "player": 1, "units": [33], "order": {"type": "Interact", "verb": "unlock", "target": 12}, "queue": true}},
  {"tick": 10, "cmd": {"type": "IssueOrder", "player": 1, "units": [33], "order": {"type": "Move", "pos": [5500, 6500]}, "queue": true}},
  {"tick": 1050, "cmd": {"type": "Craft", "unit": 33, "recipe": "ingot", "workstation": 15}}
]
```

```json
[
  {"tick": 10, "cmd": {"type": "IssueOrder", "player": 1, "units": [33], "order": {"type": "Mount", "vehicle": 29}, "queue": false}},
  {"tick": 80, "cmd": {"type": "IssueOrder", "player": 1, "units": [29], "order": {"type": "Move", "pos": [4500, -1500]}, "queue": false}},
  {"tick": 80, "cmd": {"type": "IssueOrder", "player": 1, "units": [29], "order": {"type": "Unload"}, "queue": true}}
]
```

Every registered content command is accepted as `{"type": NAME, ..fields}` with its Rust struct's field names:
`IssueOrder {player, units, order, queue, formation?}`, `CreateSquad`, `DisbandSquad` (`npc_command`);
`Interact {unit, verb, target, item?}` (`npc_interaction`); `Craft {unit, recipe, workstation?}`,
`PostCraftOrder` (`npc_crafting`); `Mount {unit, vehicle, role?}`, `Dismount {unit}`, `Unload {vehicle}`
(`npc_vehicles`); `SpendPoints {unit, purchase: {"type":"Skill","skill":..,"points":N} | {"type":"Stat",..} |
{"type":"Trait",..}}` (`npc_rules`, registered by the rules × agent glue); `TestbedDrive {unit, pos}`. Direct
commands such as `Interact` and `Craft` are validated when they apply (reach, workstation, tools), and a refusal
stops npcctl with exit 2 (`"too far: 4.2 m (reach 1.5 m)"`); orders (`IssueOrder`) walk there first. The corpus
files `tests/replays/testbed_*.json` hold the full, verified command lists (`commands: [[tick, seq, cmd], ..]`).

### 2.3 `run`

```sh
npcctl run --ticks T [world] [--journal out.jsonl] [--verbose] [--record replay.json] [--checkpoint-every N=100]
npcctl run --seed 1 --ticks 7200 --pretty
npcctl run --scenario testbed --ticks 4800 --pretty
```

```text
{seed, scenario, villagers, ticks, day_length, final_tick, final_hash, days:[...],
 needs:{energy:{min,max}, hunger:{min,max}}, aborts, leaf_starts,
 commands_applied, commands_not_reached, journal, record, content_summary?}
```

- `days[]`: `{day, ticks, meals, work_shifts, ticks_asleep, aborts, leaf_starts, min_needs, max_needs}`, summed
  over villagers; needs in milli-units.
- Other contents (the testbed): the same shape; `villagers` is the scenario's actor count, `aborts` /
  `leaf_starts` count every brain, the village fields stay 0 (per-day `min_needs`/`max_needs` keep their
  `i32::MAX`/`i32::MIN` sentinels) and `content_summary` is added:
  `{units:[{sim_id, name, template, owner, pos, level, xp, unspent, skills, carries, riding, orders}],
  doors:{name: open|closed|locked}, vehicles:{name:{sim_id,pos}}, ground_items:{kind: count}}`.
- `final_hash` is a `u64`: compare it as a string (jq 1.6 rounds numbers above 2^53).
- Goldens: `apps/npcctl/tests/snapshots/golden__run_seed1_7200.snap`, `testbed__run_testbed_4800.snap`.

### 2.4 `inspect`

```sh
npcctl inspect --tick T [world] [--unit ID | --all] [--journal-from-build BOOL=false]
npcctl inspect --seed 1 --tick 1800 --unit 5 --pretty
npcctl inspect --scenario testbed:smithy --tick 100 --unit 33 --pretty
```

- Village: a `UnitSnapshot` `{sim_id, kind, pos, needs:{hunger:{value,level},..}, activity, stockpile,
  tree_path:[node names], active_leaf:{node,action,status}, blackboard, order, brain_stats:{leaf_starts,aborts},
  reservations_held, recent_journal:[..20]}` for `--unit`, a `WorldSnapshot` for `--all` (or no flag).
- Other contents: the kernel's generic inspector (`kernel::inspect::unit_json` / `world_json`):
  `{sim_id, kind, tree_path, active_leaf, blackboard, brain_stats, components:{<registered name>: value, ..},
  recent_journal}` — every registered sim component by its stable name (`pos`, `character_sheet`,
  `inventory`, `order_queue`, `perception_memory`, `experience`, `nav_state`, ..).
- An unknown id exits 2 with reason `{"error":"UnknownUnit","unit":ID}`.
- `--journal-from-build true` starts the journal before the scenario spawns (adds its tick-0 `Spawn` entries).

### 2.5 `journal`

```sh
npcctl journal --ticks T [world] [--unit ID] [--kind K,..] [--from T1] [--to T2] [--grep TEXT] [--limit N]
               [--verbose] [--journal-from-build BOOL=true]
npcctl journal --seed 1 --ticks 4000 --unit 5 --kind LeafStart,Abort --from 3000 --to 4000 --limit 50
npcctl journal --scenario testbed:smithy --ticks 600 --kind OrderDone,Interacted
```

JSON Lines, streamed, one `{"tick","seq","unit","kind":{"type":..,..}}` per line:

```json
{"tick":287,"seq":1,"unit":33,"kind":{"type":"LeafStart","node":"Idle","path":"ExternalControlRoot > Root > Idle","action":"TbIdle","status":"Running"}}
```

`--kind` is case-insensitive (§5 lists every kind), `--grep` a case-sensitive substring of the JSON line,
`--limit` keeps the last N matches, `--verbose` also records `CondEval`. The run stops early after `--to`. By
default the journal starts before the scenario spawns (`npc_tools::new_app_journaled`), so it begins with the
tick-0 `Spawn` entries; `--journal-from-build false` starts it after the build.

### 2.6 `replay record | verify | verify-session`

```sh
npcctl replay record --ticks T [world] [--checkpoint-every N=100] --out r.json
npcctl replay record --scenario testbed:vehicles --ticks 1200 --commands cmds.json --out r.json
npcctl replay verify tests/replays/village_seed1_3days.json
npcctl replay verify-session session.json
```

- A replay: `{version, content_hash, seed, scenario, villagers, ticks, commands:[[tick,seq,cmd]],
  checkpoints:[[tick,hash]]}`. The header names what the content factory needs to rebuild the world:
  `village`/`village_n` (with `villagers`), `testbed`/`testbed:<name>`. With a scenario **file**, `record`
  prints a session record instead (`{version:1, scenario:{doc}, seed, replay}`, the format of
  `serve --record`), since a name cannot rebuild a file.
- `verify` → `{ok, checked, first_mismatch:[tick,expected,actual]|null, error?}`; exit 0 ok, 1 mismatch or not
  runnable, 2 unreadable. It dispatches: a session record (`{"scenario":{..},"replay":{..}}`) is re-run from its
  document (as `verify-session`), a replay by the content claiming its `scenario` name. A replay whose
  `content_hash` differs from the build (a registered name was added or renamed) is refused: that needs an
  approved re-record.
- `verify-session FILE` re-runs an agent session record **without the agent**: `{ok, checked, first_mismatch}`.
- The corpus `tests/replays/` holds village replays (`village_seed1_3days`, `village_orders`,
  `village_spawn_despawn`, `village_n_25_1day`), testbed replays (`testbed_default_2days`,
  `testbed_orders_craft`, `testbed_vehicles`) and an agent session (`testbed_agent_session`). They are oracles:
  never edit them (§8).

### 2.6b `snapshot save | check`

```sh
npcctl snapshot save --scenario testbed --seed 1 --ticks 600 --out target/save.json
npcctl snapshot check --scenario testbed target/save.json
```

- `save` runs the scenario (world selection as in 2.2, `--commands` too) for `--ticks` and writes the world
  snapshot (ADR 0013) to `--out` atomically (`save.json.tmp`, then rename). Here `--out` is the snapshot file;
  the JSON result `{ok, file, scenario, seed, tick, entities}` goes to stdout. A bare `--out` is a usage error (2).
- `check` builds the scenario and compares the file's format version, then content hash, with this build
  without loading it: `{ok:true}` (exit 0) or `{ok:false, error}` (exit 1, e.g. "snapshot content hash .. differs");
  an unreadable or truncated file is exit 2 (`unreadable save <path>: ..`).

### 2.7 `diverge`

```sh
npcctl diverge (--a FILE --b FILE | --seed-a N --seed-b M) [--ticks T] [--villagers K] [--scenario S]
               [--commands FILE] [--max-entities N=20]
npcctl diverge --seed-a 1 --seed-b 2 --ticks 2400 --pretty
npcctl diverge --scenario testbed:vehicles --seed-a 1 --seed-b 1 --ticks 1200   # must NOT diverge
```

```json
{"first_divergent_tick":666,"ticks_compared":2400,"entities_differing":1,"truncated":false,
 "entities":[{"sim_id":5,"components_differing":["blackboard","move_to","pos"],
              "a":{"pos":{"x":2955,"z":2161},"move_to":{"goal":{"x":-2614,"z":-2652}},"blackboard":{..}},
              "b":{"pos":{"x":2941,"z":2198},"move_to":{"goal":{"x":-10226,"z":1674}},"blackboard":{..}}}]}
```

Exit 1 when they diverge, 0 when identical. `--ticks` defaults to 2400 with seeds and to the shorter replay with
files. Both files must be of the same content; seed mode takes any named scenario, not a file (record session
records instead). The per-entity diff covers every registered sim component, by name.

### 2.8 `bench`

```sh
cargo run --release -p npcctl -- bench [--villagers 1,100,500] [--ticks 2000] [--warmup 200] [--seed 1]
      [--scenario village_n] [--top 15] [--baseline FILE] [--metric p95|p50|mean] [--threshold-pct 25]
      [--min-us 5] [--min-samples 100]
cargo run --release -p npcctl -- bench --villagers 1,100,500 --ticks 2000 --baseline bench/baseline.json --pretty
cargo run --release -p npcctl -- bench --scenario testbed --ticks 2000
```

```text
{git_rev?, profile, seed, warmup,
 results:[{villagers, ticks, wall_ms, ticks_per_sec, ms_per_tick, budget_breaches,
           mechanics:[MechanicStats], systems:[top N SystemStats]}],
 comparison?:{regressions:[..], throughput:[..]}}
```

A regression is `value > base·(1 + pct/100)` **and** `value − base ≥ min_us`; mechanics with fewer than
`--min-samples` samples in either run are skipped. Exit 1 on a regression. A fixed-size scenario (`testbed`,
`testbed:<name>`) ignores `--villagers` and is measured once. Compare release against release on an idle
machine; `bench/README.md` covers re-running, `--metric` and regenerating the baseline (only after an intended
performance change; `bench/baseline.json` is protected).

Reference numbers (`bench/baseline.json`, release, idle, 2000 ticks):

| villagers | ticks/s | ms/tick | top p95 µs |
|---|---|---|---|
| 1 | 18 009 | 0.056 | think.agent 36, think 6 |
| 100 | 10 631 | 0.094 | think 91, think.cond 16, actions.move 9 |
| 500 | 3 628 | 0.276 | think 464, think.cond 207, actions.move 50, actions.work 45 |

### 2.9 Agent subcommands: `serve`, `agent`, `notes`, `world`, `session`, `mcp`

```sh
npcctl serve --scenario FILE|NAME [--tick-hz 20] [--record session.json] [--port 15702] [--seed N] [--token T]
npcctl serve --scenario testbed --port 15702          # embedded testbed scenario by name
npcctl serve --scenario content/village_content/scenarios/agent_village.json --record session.json
```

`serve` runs the scenario headless **in real time** behind a JSON-RPC server on `127.0.0.1:PORT` (`--port 0`:
any free port). It prints `{"listening": url, "port", "scenario", "tick_hz", "record", "token_required"}` once
ready, and `{"stopped": true, "record"}` after `session stop` (which writes `--record`). `--scenario` takes a file
when one exists at that path, else an embedded testbed scenario name (the windowed testbed looks up the same way).
The server refuses browser and DNS-rebinding requests and, with `--token T`, every request without
`Authorization: Bearer T` (§4, *Transport security*).

```sh
npcctl serve --explorer-home DIR --agent NAME --world W [--preset ID] [--actors-dir DIR] [--port N] [--paused] [--tick-hz 20]
npcctl serve --explorer-home DIR --agent NAME --resume [--load-anyway | --fresh-world] [--port N] [--paused]
```

With `--explorer-home`, `serve` hosts a **detached explorer world** (no window, no renderer; the same world as
`actor_testbed --explorer-home`, [agent-experiments.md §9](agent-experiments.md#9-the-ai-explorer)): New
(`--world`, a wall-clock seed, saved at tick 0) or `--resume` from the agent's save. It binds `--port N` exactly
(`0`: any free port) or, without it, the first port in 15702..=15721 whose next port is free too (so the window
can later take the world over on the same port); a taken port fails before anything is written. It prints one
line, `{"listening": url, "port", "agent", "world", "prompt": <prompts/<kind>.md, or null when the folder could
not be written>}`, writes the agent folder's workspace for this URL and `run.json` (`{pid, port, mode:
"detached", started, agent, world}`), and runs the sim (`--paused`: paused until `npc/resume`). It saves every 60 s
of wall time (when the tick moved) and on stop: `session stop` (answer `{stopped, record, tick, saved}`) or Ctrl+C
(on Unix also SIGTERM / SIGHUP, so closing the terminal; on Windows closing the console window is **not**
graceful: ctrlc's handler returns at once and Windows ends the process, losing up to the last 60 s autosave
interval, so use `session stop` or Ctrl+C there); then it removes `run.json`, prints
`{"stopped": true, "tick", "saved"}` and exits 0. Once a stop is accepted the world is frozen: calls queued
behind it, parked long-polls and steps are answered "the session is stopping", so the final save is the last
state. A save that fails on `session stop` refuses the stop (RPC error, the world keeps running); after Ctrl+C it
exits 2 with the error. `world reset [--seed N]` rebuilds the explorer world (tick 0, same actor, notes kept:
they live in the folder): the old save is kept as `<world>.world.<date>.json` (as `--fresh-world` keeps it) and
the reset world is saved at once; `world load-scenario` is refused. A second `serve --resume` for an agent whose
`run.json` names a live process exits 2 ("agent X is already running (pid P, mode M, port N)"); so does
`actor_testbed --resume`. `--scenario`, `--record`, `--seed` and `--token` cannot be used with
`--explorer-home` (the agent folder's settings carry no token). `--preset ID` (New only, default `human`) picks
the actor preset: a valid `<ID>.json` in `--actors-dir` (default `npc-actors` beside the agents folder, the built-in
`human`, `rover` and `sentinel` copied into it when it has no preset; `testbed_content::actors`); an unknown or
invalid one exits 2 with its reasons (the robots until the robot sensors exist); a resume keeps the agent's preset. A save from another build exits **3** (as `actor_testbed`) with `"needs_choice": true`.

Clients make one RPC call per invocation (`--url http://127.0.0.1:15702` default, `--format json|text`,
`--token T` for a session served with one); they
exit 0 ok, 1 when an action was rejected (`accepted:false`), 2 on an RPC/transport/usage error with
`{"error","code","data","method"}`. They work against `serve` **and** against a windowed game's BRP port.

```sh
npcctl agent attach --unit 33 --controller me --mode direct|advisor|observer [--on-idle idle|autopilot] [--force]
npcctl agent observe --unit 33 --controller me [--events 10] [--format text] [--brief] [--since-seq S]
npcctl agent act --unit 33 --controller me --affordance pick_up@26 [--queue] [--expect-tick T] [--expect-seq S] [--allow-stale]
npcctl agent act --unit 33 --controller me --verb order --args '{"order":{"type":"Move","pos":[4500,-1500]}}'
npcctl agent act --unit 33 --controller me --interact pick_up@26          # walk into reach + do it: one act
npcctl agent act --unit 33 --controller me --steps '[{"verb":"order","args":{..}},{"verb":"execute_orders"}]'
npcctl agent act --unit 5 --controller me --verb walk_to --pos 4000,2000 --queue
npcctl agent act --unit 33 --controller me --interact pick_up@26 --wait [--kinds Perceived] [--timeout-ms 30000] [--format text]
npcctl agent cancel --unit 33 --controller me [--handle H]
npcctl agent clear --unit 33 --controller me
npcctl agent events [--unit 33] [--since-seq 0] [--kinds OrderDone,Interacted|all] [--timeout-ms 20000] [--limit 100]
                   [--until-idle] [--min-events N] [--dedupe]
npcctl agent stats [--controller me]
npcctl agent list-units [--kind actor]
npcctl agent detach --unit 33 --controller me
npcctl agent notes list --unit 33 --controller me --book diary|memory|proposals
npcctl agent notes add --unit 33 --controller me --book diary --title T [--body B] [--prompted-by P]
npcctl agent notes update --unit 33 --controller me --book diary --id N --expected-tick T [--expected-date D] [--title T] [--body B]
npcctl notes list|add|update --unit 33 --book proposals .. [--status new|liked|pursuing|rejected]   # the player: no controller
npcctl world state | world reset [--seed N] | world load-scenario FILE [--seed N]     # reset/load: session only
npcctl session info | session stop                                                 # session only
npcctl mcp --url http://127.0.0.1:15702 [--token T] [--controller explorer]
    # stdio MCP server: tools observe, act, act_and_wait, cancel, events, attach, list_units, notes_list, notes_add, notes_update
```

`agent notes` and `notes` wrap the notes methods (§4): `update` passes `--expected-tick`/`--expected-date`, the
entry's `updated` stamp exactly as last listed or returned (a stale one exits 1 with `reason: "stale"` and
`current`). Only `npcctl notes update` has `--status`. The MCP notes tools use `--controller` (default
`explorer`) when a call names no `controller`; there is no MCP player tool. MCP tool errors carry the method's
`data.reason` (`too_long`, `not_found`, `forbidden`, ..) in their text and in `structuredContent.error.reason`; a
rejection (`stale`, `not_controller`, ..) is the method's result with `isError: true`.

`--queue` appends to the unit's **action** queue (`agent/act`); an RTS order's own shift-queue is `"queue":true`
inside `--args` (the CLI prints a note when `--queue` is given with `--verb order`). `--steps` and `--interact`
exclude `--affordance`/`--verb`. `events --until-idle` (needs `--unit`; default timeout 30 s) returns once the
unit's action queue, orders, interaction, craft and vehicle drive are all done. `act --wait` is `agent/act_and_wait` (§4): act,
wait until idle (or the first `--kinds` event, or `--timeout-ms`, default 30000, at most 120000) and observe, in one
call; `--kinds`/`--timeout-ms` need `--wait`; a rejection exits 1 at once. `observe --since-seq S` (S: the `seq` of
your last observation) adds what changed since then.

Result shapes are in §4; the workflow is in [agent-experiments.md](agent-experiments.md).

## 3. The live games and BRP

### 3.1 Apps, flags, keys

| App | Run | BRP |
|---|---|---|
| `apps/village` | `cargo run -p village -- [--seed N] [--speed N] [--screenshot PATH --after-secs S] [--panels journal,perf] [--journal FILE] [--record FILE] [--remote \| --no-remote] [--remote-port N] [--render-remote-port N \| --no-render-remote] [--out-dir DIR]` | `npc/*` (no `npc/sheet`), `npc/screenshot`, `app/quit`, `agent/*` read-only (no `AgentIoPlugin`: list, observe, events; attach is rejected) |
| `apps/actor_testbed` | `cargo run -p actor_testbed -- [--scenario NAME\|FILE] [--seed N] [--speed N] [--paused] [--player N] [--select ID,..] [--camera X,Z,DIST[,YAW,PITCH]] [--commands FILE] [--screenshot PATH --after-secs S] [--panels journal,perf,sheet,perception] [--journal FILE] [--record FILE] [--remote \| --no-remote] [--remote-port N] [--render-remote-port N \| --no-render-remote] [--out-dir DIR]` | `npc/*` incl. `npc/sheet`, `npc/screenshot`, `app/quit`, `agent/*`, `world/state` |
| `apps/rts_units_demo` | `cargo run -p rts_units_demo -- [--units N] [--seed N] [--idle] [--screenshot PATH --after-secs S] [--panels journal,perf]`; `--headless-ticks N` prints a JSON summary without a window | none (it is the portability proof: facade + `bevy` only) |

The BRP server listens on `127.0.0.1:15702` (Bevy's render-app server on the next port, 15703) with the default
feature `debug-tools`, **on by default in dev builds and off in release builds**: `--remote` turns it on,
`--no-remote` off (`npc_view_helpers::remote`). A build without `debug-tools` (`--no-default-features`) has
neither the remote interface nor the profiler (`--remote` is then a usage error). RPC file paths are relative to
the output root `--out-dir` (default `target/npc-out`). The names and `SimId`s of what a testbed scenario spawned
are logged at startup. The testbed's `--scenario` takes a file when one exists at that path, else an embedded
name.

| Key | village | actor_testbed | rts_units_demo |
|---|---|---|---|
| Space | pause | pause | pause |
| `.` | one tick (while paused) | one tick | one tick |
| speed | 1 / 2 / 3 / 4 = 1×, 3×, 10×, 0× | `-` / `=` slower / faster (1×, 3×, 10×) | – |
| F1 | overlay (needs, tree path, target, reservations, state hash) | overlay (pose, interaction, crafting, ride, needs, tree path, order queue, equipment, inventory) | – |
| F2 | – | – | path gizmos |
| F3 / F4 | journal / perf panel | journal / perf panel | journal / perf panel |
| F5 / F6 | – | character sheet / perception (cone, LOS, memory) | – |
| mouse | left click selects a villager | select / box (Shift adds), right click context order (Shift queues), Ctrl+1–9 / 1–9 groups, Z/X/C/V formations, H/B/U hold/stop/unload | select / box, right click `Move` (A: `AttackMove`, P: `Patrol`, Shift: queue), Ctrl+1–9 / 1–9, Z/X/C/V, S/H |
| camera | WASD/arrows pan, Q/E rotate, wheel zoom | WASD, Q/E orbit, PgUp/PgDn tilt, wheel, F focus | arrows, Q/E, wheel |

### 3.2 Calling BRP

JSON-RPC 2.0 via `POST http://127.0.0.1:15702/`. Bevy's built-in methods (`world.query`, `rpc.discover`,
`registry.schema`, `schedule.list`, ..) stay available; the reflected sim types keep their pre-split type paths
(`npc_sim::ids::SimId`, ADR 0009).

```sh
rpc() { curl -s -X POST http://127.0.0.1:15702/ -H 'content-type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-null}}"; }
rpc npc/methods
rpc npc/pause
rpc npc/step '{"ticks":100}'
rpc npc/unit '{"id":5}'
```

**Security (residual risk).** Any web page in a browser on the same machine can send requests to a localhost
port (a "simple" cross-site `POST`, or DNS rebinding, which also lets it read the answers). `bevy_remote` 0.20
gives no hook to inspect a request before it is dispatched (`RemoteHttpPlugin::with_headers` only adds
*response* headers), so the games' BRP server cannot refuse `Origin`, wrong-`Host` or non-JSON requests the way
`npcctl serve` does. A page could therefore drive a running game (pause it, submit commands, call Bevy's
`world.*` methods). What is limited: the server listens on `127.0.0.1` only, and every client file path
(`npc/record/stop {path}`, `npc/screenshot {path}`) is **relative and confined under the output root**
(`npc_tools::out_path`: no absolute paths, drives, `\\?\`, `..`, `:` streams, trailing dots or spaces,
device names; the extension is forced, `.json` / `.png`; an existing file is only replaced with
`"overwrite": true`). BRP is off by default in release builds; outside development run with `--no-remote`, and
do not browse untrusted sites while a game with BRP is running.

### 3.3 `npc/*` methods

| Method | Params | Result |
|---|---|---|
| `npc/pause`, `npc/resume` | – | `ControlState {tick, paused, multiplier, driver: "view"\|"headless"\|"session"}` |
| `npc/step` | `{ticks=1}` (≤ 1 000 000) | `{tick, ticks, hash: u64, hash_hex}`; works while paused |
| `npc/speed` | `{multiplier}` (0..=1000) | `ControlState` |
| `npc/state` | – | the installed inspector's world snapshot (village: `WorldSnapshot`) |
| `npc/unit` | `{id}` | the unit's snapshot incl. `recent_journal` (as `npcctl inspect --unit`) |
| `npc/command` | `{cmd: SimCommand, at_tick?}` | `{tick, seq}`: queued, validated, recorded for replay |
| `npc/journal` | `{filter: JournalFilter}` or the filter fields directly: `{unit?, kinds?:[..], from?, to?, grep?, limit?}` | `[JournalEntry]`; an error while the journal is off |
| `npc/perf` | `{top?}` | `PerfReport {systems, mechanics, frames}` (with `top`, only those rows: `report_top`) |
| `npc/perf/reset` | – | `{reset: true}` |
| `npc/record/start` | `{every=100}` | `{recording, started_at, every}` |
| `npc/record/stop` | `{path?, overwrite?=false}` | the `Replay` (also written to `path`, relative to the output root, `.json`; a refused path is a `-32602` error before the recording stops; on a write error the replay is in the error's `data`) |
| `npc/screenshot` (village, testbed) | `{path, overwrite?=false}` | `{path (absolute), pending: true}`; `path` is relative to the output root (`.png` forced); the PNG appears 1–2 frames later |
| `app/quit` (village, testbed) | `{pid?}` | `{quitting: true, pid}`; the app exits 0 about 200 ms later through its ordinary exit (an explorer window saves and removes `run.json`, as when closed with "No"); a `pid` that is not the app's is a `-32602` error and nothing happens. The game menu's Quit sends it to the explorer windows it started (`apps/game_menu/src/quit.rs`); the Village and testbed scenarios have nothing to save and are killed instead |
| `npc/sheet` (feature `rules`: the testbed) | `{unit}` | `{unit, tick, sheet: SheetView}` (attributes, secondary, traits, skills with effective levels, points) |
| `npc/methods` | – | the `npc/*` method names |

The session host (`npcctl serve`) serves the subset `npc/pause|resume|speed|step|state|unit|command|journal|methods`
(its `driver` is `"session"`); profiler, recording and screenshots are BRP-only. On the session host a long
`npc/step` runs in slices of 50 ticks (about 5 ms per loop iteration), so other requests and long-polls are
answered meanwhile; the call returns when the last slice is done (the server and `RpcClient` wait up to 10
minutes for an `npc/step` reply, `http::STEP_WAIT`), frames wait, and `world/reset`,
`world/load_scenario` or `session/stop` end a running step with an error.

**Errors** (`error.code`): `-32602` bad params or a rejected command (the `CommandError` JSON is in `data`),
`-23401` unknown unit, `-23501` the journal is off, `-23502` a sim resource or the profiler is missing,
`-23601` the method needs the session host (`world/load_scenario`, `world/reset` on BRP), `-32601` unknown
method, `-32603` internal error.

**Gotchas**

- Change the sim only through `npc/command`. Bevy's `world.mutate_*` / `world.insert_*` on sim entities bypass
  the command log, so the run no longer replays.
- Use `hash_hex` (or treat `hash` as a string): u64 values above 2^53 get rounded by JavaScript and jq 1.6.
- The journal is on in the windowed apps (they add `JournalPlugin`); `--journal FILE` also streams it to disk.
- **Ports:** both ports are probed when the plugin builds; a taken port is logged as an error naming it
  (target `npc_tools`) and that server is not started (`RemoteStatus` records the outcome). Two games at once:
  `--remote-port 15712` (its render server goes to 15713) or `--no-render-remote`.

## 4. Agent, world and session methods

Served by `npcctl serve` (a std JSON-RPC host) and, in games, by `AgentRemotePlugin` on BRP (ADR 0010). Every
result carries `tick` (the next tick to run), `seq` (the latest agent event) and `epoch` (the world instance: a new,
larger number after `world/reset` or `world/load_scenario`, when seqs and handles restart; poll again from
`since_seq: 0` when it changes).

| Method | Params | Result |
|---|---|---|
| `agent/attach` | `{unit, controller?, mode?=Direct\|Advisor\|Observer, on_idle?=Idle\|Autopilot, force?}` | `{controller, unit, mode, on_idle, applies_at}` |
| `agent/detach` | `{unit, controller}` | `{unit, applies_at}` |
| `agent/observe` | `{unit, controller?, format?=json\|text, events?=10, brief?=false, since_seq?}` | `{unit, kind, day, time_permille, night, omniscient, controller, queue:{current,pending}, sections:{self, ..}, recent_events, affordances, hidden_affordances?, changes?}` or `{format:"text", text, changes?}` |
| `agent/act` | `{unit, controller, affordance? \| verb + target?/pos?/args? \| steps:[{affordance? \| verb, target?, pos?, args?}], queue?=false, expect_tick?, expect_seq?, allow_stale?}` | `{accepted:true, handle, verb, label, est_ticks, queued, applies_at, mode}` (several parts: `{accepted, steps:[{verb,label,handle}], handles, handle, ..}`) or `{accepted:false, reason, message, step?, interrupting?}` |
| `agent/act_and_wait` | every `agent/act` field + `until_idle?=true, kinds?, timeout_ms?=30000 (≤ 120000), observe?:{format?, brief?}, nonce?` (BRP: `nonce` required) | `{act: <agent/act result>, events: <agent/events result after the act>, observation: <agent/observe result with changes since the act> \| null, observation_error?}` |
| `agent/cancel` | `{unit, controller, handle?}` | `{unit, handle, applies_at}` |
| `agent/clear` | `{unit, controller}` | `{unit, applies_at}` |
| `agent/events` | `{unit?, since_seq?=0, kinds?(list or "a,b"; "all"), timeout_ms?=0 (≤ 60000), limit?=100, until_idle?, min_events?=1, dedupe?, nonce?}` | `{events:[{seq,tick,unit,kind,event,repeats?}], next_seq, timed_out, oldest_seq, dropped, journal, idle?, busy?}` |
| `agent/events+watch` (BRP only) | as above | server-sent events, one result per new batch |
| `agent/stats` | `{controller?}` | `{controllers:[{controller, unit, observes, acts, accepted, rejected, stale, latency_ticks, latency_ms, latency_us}]}`, each latency `{n,min,p50,p95,max,mean}` |
| `agent/list_units` | `{kind?}` | `{units:[{id, kind, pos, controllable, controller, owner?, commandable?}]}` (`owner`: the owning player with the command glue; testbed actors have kind `actor`) |
| `agent/notes/list` | `{unit, controller, book: diary\|memory\|proposals}` | `{unit, book, notes:[{id, title, body, created:{tick,date}, updated:{tick,date}, status, prompted_by}]}` (oldest first; `status`/`prompted_by` proposals only, else `null`) or a rejection (an Observer may list) |
| `agent/notes/add` | `{unit, controller, book, title, body, prompted_by?}` | `{accepted:true, unit, book, note}` or `{accepted:false, reason, message}` |
| `agent/notes/update` | `{unit, controller, book, id, expected:{tick,date}, title?, body?}` (no `status`: invalid params) | `{accepted:true, unit, book, note}` or `{accepted:false, reason, message, current?}` |
| `notes/list` (player) | `{unit, book}` | as `agent/notes/list` |
| `notes/add` (player) | `{unit, book, title, body, prompted_by?}` | as `agent/notes/add` |
| `notes/update` (player) | `{unit, book, id, expected:{tick,date}, title?, body?, status?: new\|liked\|pursuing\|rejected}` | as `agent/notes/update` |
| `world/state` | – | the inspector's world JSON (everything, not limited by perception) |
| `world/load_scenario` (session) | `{scenario: {..}, seed?}` (the document inline; `npcctl world load-scenario FILE` reads the file client-side; there is no `path`) | session info |
| `world/reset` (session) | `{seed?}` | session info |
| `session/info` (session) | – | `{scenario, content, seed, tick, units, tick_hz, url, record, methods}` |
| `session/stop` (session) | – | `{stopped, record}` |

`agent/act` rejection reasons: `not_attached`, `not_controller`, `read_only` (Observer), `stale`,
`unknown_affordance`, `unavailable` (the affordance's `why_not`, e.g. `"door is locked"`), `unknown_verb`,
`invalid_args` (also a wrongly typed arg such as `"ticks":"50"`, or `args` that is not an object), `direct_only`,
`invalid`. With `steps`, every step is planned and validated before anything is submitted (all or nothing; a
rejection names its `step`), and so are all commands of one plan. `stale`: `expect_tick` older than
`AgentIoConfig::stale_ticks` (default **600** = 30 s at 20 Hz), or more than `stale_events` (default 0)
*relevant* events of the unit since `expect_seq`: its own actions, orders, interactions and crafts ending, need
level changes, mounting, detach, despawn, failed paths, and percepts (`Perceived`/`Lost`) of the entities the act
refers to; the message lists them and `interrupting` holds them. Rejections are journaled as `ActionRejected` for
the unit at request time (tooling-originated, so a replay of the session does not contain them). An act accepted
but **skipped when its tick came** (a detach or forced attach landed first) is journaled `ExternalAction
{status:"rejected"}` plus `ActionRejected {reason:"skipped at tick T: .."}`, so a waiting poll wakes up; the queue
limit counts acts already submitted for the same tick.

**Notes** (`npc_agent_io::notes`, spec `2026-10-25-agent-notes-and-workspace-design.md`, storage
`2026-10-25-notes-as-obsidian-files-design.md`): a diary, a memory and proposals per agent folder, one markdown
file per entry with YAML frontmatter in `diary/` (`YYYY-MM-DD <title>.md`), `memory/` and `proposals/`
(`<title>.md`), kept outside the world save; never sim state (a note write leaves the state hash unchanged).
Entries are found by their `id` property; `list` also returns `unreadable: [{file, reason}]` for files the store
cannot read (no or broken frontmatter, a duplicate id), which it never writes. An old `diary.md` / `memory.md`
moves into its folder once on first open (kept as `.bak`). The host maps units to folders in the `NotesDirs` resource (a unit not
in it: invalid params `"unit N keeps no notes"`); one `NotesStores` resource holds one store per folder per
process, shared with the in-game editor. Stamps are `{tick: SimClock.tick, date}` with the date from the host's
`NotesClock` (wall-clock RFC 3339 in the games; empty without one). The `agent/notes/*` methods need the unit's
controller (rejections `not_attached`, `not_controller`); `add` and `update` use the full `agent/act` check, so an
Observer controller gets `read_only` for them, while `list` is a read and works in Observer mode. They write as
the agent; the `notes/*` methods take no controller and write as the player (the F7
editor, `npcctl notes`), and only they set a proposal's `status` (new proposals are `new`; an agent call with a
`status` field is invalid params, so an agent cannot pose as the player). `updated` is a version: pass it back
exactly as `expected` (the date may carry a `#rN` revision); when the entry changed since, the update is the
rejection `{accepted:false, reason:"stale", message:"note N changed since you read it ..", current}` and nothing
is written. Other errors are invalid params with `data.reason`: `too_long` (title or `prompted_by` over 200
characters, body over 8192 bytes; the message names the limit), `not_found`, `forbidden` (`prompted_by` outside
proposals, a status outside proposals); a file error is `-32603` with `data.reason: "io"`. Book and status
names are case-insensitive.

With a perception provider (`omniscient: false`), affordances only target entities the unit perceives, remembers
(last-known position), holds or rides, plus imperceptible things (no `Perceivable`) within 1.5 m (touch); there
is no distance cut-off for them (a far one says `why_not: "too far: 12.0 m (reach 1.5 m)"`). Without perception,
or for a unit without `Sensors` (`omniscient: true`), they are those within
`InteractionConfig::affordance_radius_mm` (10 m). `world/state` always shows everything. Skill-checked affordances carry `check: {skill, target_number,
dice, odds_permille, default?}` and a label suffix `[Mining 13 vs 3d6: ~83%]` (`low odds` under 30%).

**Robot sensors** (plan Task 12, spec G10; `npc_perception::{Lidar, Camera, Radar}`, the `rover` and `sentinel`
presets): a unit with a LIDAR, camera or radar gets the observation sections `lidar`, `camera` and `radar` (absent
without that sensor), the ROS-shaped `SensorReadings` of its latest scan (snapshot-only `sensor_readings`, never
hashed; `npc/inspect`'s perception block shows them as `readings`). Angles are millidegrees relative to the facing,
**+ = left**; distances mm.

- `lidar` (`sensor_msgs/LaserScan`): `{angle_min_mdeg, angle_increment_mdeg, range_min_mm, range_max_mm, ranges_mm,
  tick}` (`tick`: the LIDAR's own scan, half a stagger after the camera/radar pass); beam `i` points
  `angle_min + i · increment`; a range is where the beam enters the first blocking or occluding cell or cell
  holding an obstacle thing (`LidarObstacles`, set by content: in the testbed units, vehicles, trees, ore,
  workstations and chests; items never, a door only through its cell, so a closed door stops the beam and an open
  one does not), with `±noise_ppm` deterministic noise; a miss is `range_max_mm + 1`. `range_min_mm` is 50: a hit
  closer than that (a robot pressed against a wall) reads as 50, never as a miss. It names nothing and feeds no
  memory. An idle robot reuses its traced ranges (same position, facing, grid cells and obstacle cells) and only
  draws its noise again. Text: `LIDAR (20.00 m, 360 beams): nearest obstacle N 9.41 m, NE 10.32 m, E 4.00 m, ..`
  (8 compass sectors, `+z` north, `+x` east).
- `camera` (`vision_msgs` style): `{tick, detections: [{class, track_id, bearing_mdeg, distance_mm, size_mm,
  confidence_ppm}]}`, nearest first: perceivable things in its cone and range with line of sight; `class` is the
  kind name, `confidence_ppm` falls from 990000 at 0 to 600000 at the range, `track_id` stays while the thing stays
  in view. Text: one `Camera: item (track 1) 3.35 m north-east, 27 degrees right (90% sure, 0.50 m wide).` line per
  detection (8, then `Camera: and N more (4 tree, 1 item).`).
- `radar` (`radar_msgs` style): `{tick, tracks: [{bearing_mdeg, range_mm, radial_velocity_mm_per_tick,
  amplitude_ppm}]}`: bearing and range rounded to the radar's resolution, radial velocity + = moving away (the
  range's change since the radar's previous scan), amplitude higher for a thing that moved; walls block it. Text:
  one `Radar: echo 10.00 m north-east, 60 degrees right, no radial motion (weak echo).` line per track (8, then
  `Radar: and N more (2 moving, 9 still).`).

The three sections carry the JSON; their text lines come together in the perception text, right after `You
perceive`. Camera and radar are **automatic** (no detection check, no `per_roll` dice) and feed the memory (`via: camera`,
`radar`), so `You perceive`, `You remember` and the affordances follow them; a robot without eyes or ears knows
only what its camera, radar and reach report. Camera and radar scan on the unit's stagger ticks, the LIDAR half a
stagger later (`is_lidar_due`), so one robot never runs both in one tick (perf mechanic `perception`).

**Act and wait** (spec G13: an LLM agent works in turns, so one call per step instead of act, poll, observe):
`agent/act_and_wait` validates every param first (nothing is acted on bad params), acts exactly like `agent/act`
(same commands, same state hash) and then waits: with `until_idle` (default) until the unit is idle, or until the
first event of the given `kinds`; with `until_idle: false` until the first matching event; at most `timeout_ms`
(default 30000, clamped to 120000; `0`: answer at once). `events` are the `agent/events` result from the act's `seq`
(filtered by `kinds` when given; `timed_out: true` at the timeout, with `idle`/`busy`), `observation` the unit's
observation in `observe.format` with `changes` since the act (null with `observation_error` if the unit is gone). The
`events` list keeps `agent/events`' limit of 100 (then `next_seq` stops short of the end): for the full picture of a
long wait read the observation's `changes`, or `agent/observe {since_seq}`. `kinds` names what ends the wait, so
`kinds: ["ExternalAction"]` ends it at once on the act's own `queued` event; name outcome kinds (`OrderDone`,
`Interacted`, `Perceived`, ..) instead. A rejected act answers at once with `act.accepted: false` (events and
observation included, no wait). In the session it is a parked poll. On BRP it is a watching method that **needs a
`nonce`** (invalid params without one; the CLI and the MCP server send a unique one per call): its state is keyed by
the params, nonce included; the first run acts, later frames only check, and the answer stays stored as a tombstone
for 60 s after the last run, because `bevy_remote` keeps re-running an answered watcher until the HTTP side has
dropped it: every re-run gets the stored answer and never acts again (a second call with the same params and nonce
within that time gets the first call's answer, so never reuse a nonce). Neither host blocks a frame. Over MCP the
tool simply waits for the answer.

**Since your last look**: `agent/observe {since_seq: S}` (S: the `seq` of your last observation) adds `changes`,
built from the agent event buffer after S (never from sim state):
`{since_seq, truncated, sensed_new:[{id,name,via}], lost:[{id,name,forgotten}], inventory_gained:[{item,kind,stack,name,from}],
inventory_lost:[{item,kind,stack,name,to}], orders:[{order, result:"done"|"failed", reason?}],
interactions:[{verb,target,name,outcome,item?,reason?}], actions:[{handle,verb,status}]}`. Percepts are net (a
target's last `Perceived`/`Lost` decides; what the unit now holds or rides is left out), items too (held before the first move in the window vs after the last,
whoever moved them; moving between the unit's own slots is no change), actions once per handle with the latest
status; orders and ended interactions oldest first. `truncated: true` when the ring buffer already dropped events
after S; a `since_seq` beyond the latest seq is invalid params (the world was rebuilt). The text form starts with
one line, e.g. `Since your last look: action #3 interact done; order Interact done; pick_up book #26: success;
gained book #26.` (at most 8 names per kind, then "and N more"), or `Nothing changed since your last look.`

`kinds` must be journal kind names (case-insensitive) or `all`; an unknown name is an error listing the valid ones.
A poll answers once `min_events` (default 1) matching events are buffered, or with `until_idle` once the unit is
idle (`busy` says what still keeps it busy), or at the timeout. `next_seq` skips past non-matching events.
`dedupe` collapses consecutive identical events into one with `repeats`. The buffer is a ring of 20 000 events:
`oldest_seq` is the first still held and `dropped` counts events after `since_seq` that were evicted before this
call; a `since_seq` beyond the latest seq is an error (the world was rebuilt: see `epoch`).

`agent/events` without `kinds` returns the significant kinds (`npc_agent_io::events::SIGNIFICANT_KINDS`, minus
`Roll` with purpose `perception.detect`, which is noise for agents; name `Roll` to get those):
`ExternalAction`, `ActionRejected`, `ControllerAttached`, `ControllerDetached`, `Interacted`, `ItemMoved`,
`DoorChanged`, `NeedLevel`, `Effect`, `Reserve`, `Release`, `PathFound`, `PathFailed`, `Despawn`, `PhaseChange`,
`Perceived`, `Lost`, `Roll`, the `Order*` kinds, `Mounted`, `Dismounted`, `VehicleStalled`, `XpGained`, `LevelUp`,
`SkillImproved` and the `Craft*` kinds. On BRP a long poll is a *watching* method: the HTTP request stays open
while the game checks once per frame. `bevy_remote` does not pass the request id to the handler, so per-request
state is keyed by the params plus the request's position among identical ones in that frame (an order
`bevy_remote` does not keep stable when one of them ends); send a unique `nonce` (the CLI and the MCP server send
one per call) so identical concurrent polls or `+watch` streams never share state.

**Transport security** (`npc_agent_io::http`, the `npcctl serve` server). Before reading the body it refuses: any
request with an `Origin` header (`403`: browsers send one, CLI/MCP/script clients do not), a `Host` other than
`127.0.0.1:PORT`, `localhost:PORT` or `[::1]:PORT` (`421`, DNS rebinding), a `Content-Type` other than
`application/json` (`415`), a missing token when served with `--token` (`401`, `Authorization: Bearer T`), no
`Content-Length` (`411`), `Transfer-Encoding` (`501`) and bodies over 1 MiB (`413`). Limits: headers 64 KiB
(`431`), 10 s for the whole request, 64 connections at once (`503`), 64 calls per batch; an empty batch is a
`-32600` error. Each refusal is a JSON-RPC error whose message names the rule (`curl` needs `-H 'content-type:
application/json'`). The server never reads or writes a file a client names.

## 5. The journal

`npc_kernel::journal`: a ring buffer (default 50 000 entries) plus an optional JSON Lines file sink. Each entry is
`{tick, seq, unit, kind}`; `(tick, seq)` is a total order and the content is deterministic. Off by default in the
library: `JournalPlugin::default()` (or `{verbose: true}` for `CondEval`), `npc_kernel::journal::set_journal`,
`AgentIoPlugin` (on by default), `npcctl journal`, `--journal FILE` in the games. One filter function serves the
CLI, BRP and the F3 panel. Breaches, timings and other tooling data never go in the journal.

`JournalKind::NAMES` (39 kinds, the JSON `"type"`; `npcctl journal --kind` and BRP filters accept them
case-insensitively):

**Behaviour trees (`npc_ai` through the kernel)**

| Kind | Fields | Meaning |
|---|---|---|
| `ThinkStart` | `reason: Request \| LeafDone \| NoLeaf` | the agent re-thinks: a `ThinkRequest`, its leaf finished, or it had none |
| `LeafStart` | `node, path, action, status` | a leaf started its action (`path` = node names root → leaf) |
| `LeafStop` | `node, path, action, reason: Finished \| Aborted` | an action was stopped |
| `Abort` | `guard, aborted:[nodes]` | a `Guard { abort: LowerPriority }` interrupted lower branches |
| `CondEval` | `node, cond, result` | a guard condition's value (verbose journal only) |
| `RootDone` | `status` | the tree's root finished (the next think starts over) |

**Kernel, needs, jobs, commands**

| Kind | Fields | Meaning |
|---|---|---|
| `NeedLevel` | `need, from, to` (`Ok`, `Low`, `Urgent`) | a need crossed a threshold (with hysteresis) |
| `Reserve` | `target, ok` | a reservation of a smart object (`ok:false`: taken) |
| `Release` | `target, reason` | a reservation was released (`Finished`, `Aborted`, `Failed`, `Reclaim`, ..) |
| `Effect` | `effect:{type,..}, applied` | a content effect applied in `NpcSet::Effects` (e.g. `{"type":"AddFood","stockpile":3,"amount":3}`) |
| `Command` | `cmd` | an applied `SimCommand` (journaled just before it takes effect) |
| `PhaseChange` | `night` | day ↔ night |
| `Spawn` | `kind` | a sim entity appeared (`"villager"`, `"actor"`, `"wall"`, ..) |
| `Despawn` | – | a sim entity left |

**Navigation (`npc_nav`)**

| Kind | Fields | Meaning |
|---|---|---|
| `PathFound` | `len, nodes` | a path request was served: waypoints after smoothing, A* expansions over every tick it took |
| `PathFailed` | `reason: StartBlocked \| GoalBlocked \| OutOfBounds \| Unreachable \| SearchLimit` | no path (`SearchLimit`: restarted by grid changes until its work cap, see `npc_nav`) |

**Rules (`npc_rules`)**

| Kind | Fields | Meaning |
|---|---|---|
| `Roll` | `skill, target_number, roll, margin, success, crit: None \| Success \| Failure, purpose` | a 3d6 roll-under against `target_number` (the effective level plus modifiers; the Rust field is `target`, and `target` is still read); `purpose` names the caller (`craft.planks`, `interaction.use_tool_on`, `perception.detect`, ..) |

**Perception (`npc_perception`)**

| Kind | Fields | Meaning |
|---|---|---|
| `Perceived` | `target, via: vision \| hearing \| proximity \| reach \| camera \| radar` | the unit started sensing `target` (`camera` / `radar`: a robot's sensors, never rolled) |
| `Lost` | `target, forgotten` | `false`: no longer sensed but remembered; `true`: the memory expired or was evicted |

**Agent control (`npc_agent_io`)**

| Kind | Fields | Meaning |
|---|---|---|
| `ControllerAttached` | `controller, mode, on_idle` | an external controller attached |
| `ControllerDetached` | `controller` | it detached (the unit's own tree resumes) |
| `ExternalAction` | `handle, verb, status: queued \| started \| done \| failed \| cancelled \| rejected, latency_ticks` | an agent action changed state; `latency_ticks` (queued) = how stale the agent's observation was; `rejected`: accepted, then skipped when its tick came |
| `ActionRejected` | `controller, verb, reason` | an act was refused before it became a command, or skipped at apply time (`reason: "skipped at tick T: .."`; not replayed) |

**Interaction (`npc_interaction`)**

| Kind | Fields | Meaning |
|---|---|---|
| `Interacted` | `verb, target, item, outcome: started \| success \| failed \| invalid \| cancelled \| rejected, reason, dropped?` | a verb ran; `failed` = the skill check failed (time spent, no effect); a success that put new items on the ground (a tool yield at the actor's feet) lists them in `dropped: [{item, kind, stack, pos: [x, z]}]`, with `reason` saying why the pack could not take them (`"pack would carry 30950 of 30000 g"`); a full-pack refusal names what to drop (`"inventory is full (8 slots): drop 1 of [plank #66, ingot #89] to make room"`) |
| `ItemMoved` | `item, kind, stack, from, to, slot` | an item changed holder (`null` = the ground) or equipment slot |
| `DoorChanged` | `door, open, locked` | a door opened, closed, locked or unlocked |

**Vehicles (`npc_vehicles`)**

| Kind | Fields | Meaning |
|---|---|---|
| `Mounted` | `vehicle, seat, role` | the unit took a seat (`Driver`, `Passenger`, `Gunner`) |
| `Dismounted` | `vehicle, seat, at` | it left the seat and now stands at `at` |
| `VehicleStalled` | `reason` | the vehicle (the entry's `unit`) cannot move: `"no driver"`, a required seat is empty |

**Progression (`npc_progression`)**

| Kind | Fields | Meaning |
|---|---|---|
| `XpGained` | `source, amount, xp` | XP from a registered source (`roll`, `craft`, `job`, `discovery`, ..) |
| `LevelUp` | `level, points` | a new level and the banked points it granted |
| `SkillImproved` | `skill, points, level, via: practice \| study \| teaching \| spend` | a skill gained points |

**Crafting (`npc_crafting`)**

| Kind | Fields | Meaning |
|---|---|---|
| `CraftStarted` | `recipe, workstation, ends` | a craft started; it completes at tick `ends` |
| `CraftDone` | `recipe, workstation, margin, quality, outputs` | it succeeded; quality from the roll's margin |
| `CraftFailed` | `recipe, workstation, outcome: rejected \| failed \| critical \| invalid \| cancelled, reason, consumed` | it did not succeed (`critical`: inputs consumed, tool damaged) |

**RTS command (`npc_command`)**

| Kind | Fields | Meaning |
|---|---|---|
| `OrderIssued` | `order, queued, player` | an order was given to the unit (`queued`: shift-queued) |
| `OrderStarted` | `order` | the unit started executing it |
| `OrderDone` | `order` | it completed |
| `OrderFailed` | `order, reason`, `nearest_reachable?`, `blocker?` | it ended without completing (`blocked`, `target gone`, `replaced`, `stopped`, a verb's reason, ..); a `blocked` walk adds the reachable cell centre closest to the goal (mm) and what is in the way (`{"kind":"Wall","cell","pos"}`, `{"kind":"Door","id","locked"}`, `{"kind":"OutOfBounds"}`; `npc_nav::explain_blocked`) |

## 6. Inspection, state hash and replays

- **Inspect** (`npc_kernel::inspect`): `unit_json` / `world_json` cover every component registered with
  `register_sim_component::<T>()`, by its stable `SimHash::NAME`; content may install its own inspector
  (`set_inspector`, the village does). `npcctl inspect`, BRP `npc/unit`/`npc/state` and agent `world/state` all
  return it.
- **State hash** (ADR 0009): `SimClock.tick`, the entity count, then per entity in `SimId` order every registered
  component in name order (presence tag + fixed encoding), then every registered resource. The **content hash**
  hashes the sorted manifest of registered names plus each tree's content hash; replays carry it.
- **Replays** are seed + scenario + commands + `(tick, hash)` checkpoints, never snapshots. Record from the CLI
  (`run --record`, `replay record`), from a game (`--record FILE`, written on exit; BRP `npc/record/start|stop`)
  or from an agent session (`serve --record`). One recorder (`npc_kernel::replay::Recorder`) serves them all.
- **Diffs**: `diverge` re-runs both sides tick by tick, compares state hashes and, at the first different tick,
  per-entity hashes and then per-component JSON.

## 7. Performance

### 7.1 The profiler

`npc_perf` is a `tracing` layer that times every Bevy system span (with `bevy/trace`) and every custom
`mechanic` span, tags each system with a mechanic (`MechanicTags`, by module-path prefix), keeps rolling
p50/p95/max (window 600 samples) per system and per mechanic, and checks per-mechanic p95 budgets
(`PerfBudgets`). A breach is recorded in the profiler's own list (never in the journal) at most once per mechanic
per 5 s and logged on target `npc_perf`: `BreachLog::Warn` (the collector's default, `npcctl bench`) warns every
time, `BreachLog::WarnOnce` (`NpcPerfPlugin`'s default, the windowed games) warns on a mechanic's first breach and
logs the rest at `debug` (`perf_log_filter` keeps `npc_perf=info`, so they stay off the console; the F4 panel still
marks the mechanic red), and the testbed's `--perf-warn` switches back to `Warn`. A `mechanic` span inside a timed
span of the same mechanic (the `vehicles` span inside an `npc_vehicles` system) is not timed again, so a mechanic's
per-frame sample is its systems' time, counted once. `frame` (Bevy's `Main::run_main`) and `driver` are inclusive
and have no budget. Budgets are checked only after `DEFAULT_MIN_SAMPLES` (60) samples
since the last reset; `PerfBudgets::scaled_for_profile()` multiplies them by `DEBUG_BUDGET_SCALE` (10) in dev
builds (the workspace builds the sim at `opt-level = 1` in dev).

Where it shows: the **F4** panel (village, testbed, RTS demo; cached, refreshed at most every 0.5 s), BRP
`npc/perf {top?}` / `npc/perf/reset`, and `npcctl bench` JSON. Wiring it into your own app: see
[using-npc-framework.md §9](using-npc-framework.md#9-view-helpers-and-debug-tooling-in-your-app).

### 7.2 Mechanics and budgets

The core rows live in `NPC_BUDGET_TABLE` (`crates/npc_perf/src/budget.rs`). Content adds its own rows with
`PerfAppExt::register_budget(mechanic, fixed_us, per_agent_us)` or `NpcPerfPlugin::budget(..)`. For example, the
village registers `actions.eat/drink/work/sleep/idle` from `village_content::VILLAGE_BUDGETS`.

p95 budget = `fixed + per_agent × agents` µs (`PerfBudgets::default_npc_for(agents)`, or
`default_npc_for_world` to include the content rows; `default_npc()` is the fixed part). To see every registered
need over a headless run, use `npcctl run --all-needs`, which adds a `needs_all` field.

| Mechanic | Fixed µs | Per agent µs | Systems |
|---|---|---|---|
| `inputs` | 20 | 1 | command application, orders, host positions |
| `needs` | 15 | 2 | need decay and levels |
| `clock` | 10 | 0 | the sim clock, day/night |
| `think` | 40 | 20 | the BT think pass (spans `think.agent`, `think.cond`, `think.start`, `think.stop`) |
| `actions.move` | 10 | 2 | movement (core) |
| `actions.eat` / `.drink` / `.work` / `.sleep` / `.idle` | 10 | 2 | the village's action systems (registered by `village_content`) |
| `spatial` | 10 | 1 | the spatial grid index |
| `locomotion` | 10 | 2 | movers (straight line, nav path following) |
| `nav` | 30 | 2 | path requests, replanning |
| `nav.astar` | 2500 | 0 | the A* searches of one tick (span) |
| `perception` | 50 | 5 | sensors, LOS, memory, `per_roll` |
| `fog` | 55 | 0 | fog of war: explored/visible cells and last-seen statics of `RevealsFog` actors |
| `rules` | 10 | 1 | rolls |
| `agent_io` | 15 | 2 | controllers, event harvest |
| `interaction` | 25 | 2 | verbs, doors |
| `progression` | 10 | 2 | XP, levels, learning |
| `crafting` | 15 | 2 | crafts |
| `command` | 15 | 2 | order execution, formations |
| `vehicles` | 30 | 2 | seats, crew follow, driving |
| `effects` | 15 | 2 | effect resolution |
| `journal` | 20 | 2 | journal stamping |
| `view.visuals` / `view.anim` | 200 | 10 | visual sync, animation |
| `view.overlay` / `view.camera` / `view.scene` | 500 / 100 / 100 | 0 | presentation |

`vehicles`, `perception`, `nav` and `interaction` have a higher fixed part than their headless cost needs: in a
windowed game each sim tick starts with cold caches between GPU-bound frames, and these mechanics run several
systems every tick (measurements: `docs/_notes/explorer-perf-budgets.md`).

Unbudgeted tags: `driver` (the kernel driver), `debug` (replay, inspect, hash), `remote` (`npc_tools`), `perf`,
`engine` (`bevy_*`), `render` (Bevy's render crates), `view` / `view.*` (`npc_view_helpers` modules), content
tags (`village_content::perf_tags`, `testbed_content::perf_tags`, the RTS demo's `rts.sim` / `rts.view` / `rts.host`) and `untagged`. Content
tags its own systems with `MechanicTags::add(prefix, mechanic)` and sets budgets with `PerfBudgets::set`.

### 7.3 Tracy and Chrome traces

For a deeper look at one frame or one system, the village has two optional features on top of `debug-tools`:

```sh
cargo run --release -p village --features tracy          # stream spans to a running Tracy profiler (bevy/trace_tracy)
cargo run --release -p village --features chrome-trace   # writes trace-<timestamp>.json; open it in ui.perfetto.dev
```

Every system span (and the `mechanic` spans) shows up with its real name. Tracy needs the Tracy GUI with a
protocol version matching Bevy 0.20's `tracing-tracy`; the Chrome trace grows fast, so run it for seconds, not
minutes. The testbed and the RTS demo have no such feature; add `bevy/trace_tracy` the same way in your app.

## 8. Gates and guardrails

### 8.1 `cargo xtask verify`

```sh
cargo xtask verify [--quick] [--pretty]
```

One JSON verdict on stdout, progress on stderr, exit 0 when `ok`:

```json
{"ok":true,"steps":[{"name":"fmt","ok":true,"ms":812,"summary":"all files formatted","log_tail":""}, ...]}
```

Steps (all run even when one fails): `fmt` (`cargo fmt --check`), `clippy` (`--workspace --all-targets -D
warnings`), `test` (`cargo nextest run --workspace` + doc tests, or `cargo test --workspace` without nextest),
`arch_fitness` (`xtask/tests/arch_fitness.rs`: the crate graph, facade-only content, headless npcctl, the RTS
demo's facade-only closure), `replays` (`npcctl replay verify` on every `tests/replays/*.json`, counted per kind),
`goldens` (no pending insta snapshots) and `deny` (`cargo deny check`). `--quick` skips `deny`, `replays` (the
corpus tests still run inside `test`) and the slow test binaries (`determinism`, `properties`, `shuffle`). A
missing cargo-nextest or cargo-deny is reported as skipped with an install hint, never as a failure.

A full run (not `--quick`) fails when `tests/replays/` has no `*.json`: the corpus is an oracle.

CI (`.github/workflows/npc-framework.yml` at the repository root): `cargo xtask verify` on Linux and Windows
(the only place fmt, clippy, nextest and `cargo deny` run), a features job (`cargo hack check --each-feature -p
npc_framework`, the glue pairs, and `clippy -D warnings` on the feature combinations `verify` never builds alone:
the facade with no and all features, `npc_tools`/`npc_agent_io`/`npc_view_helpers` with `remote`, the apps
without `debug-tools`), an MSRV job (`cargo check --workspace --all-targets` on Rust 1.97.1), and a determinism
job that verifies the corpus and fixed runs on both OSes and fails if their hashes differ.

### 8.2 Claude Code hooks (`.claude/settings.json`)

- **Launch Claude Code in `npc-framework/`.** The hooks, `.claude/rules/` and the skills load only when this
  directory is the project root. Started from the repository root (a shared vault for other projects, which
  deliberately has no hooks of its own), none of the guardrails below are active.
- **PreToolUse** (`.claude/hooks/protect_goldens.sh`) blocks Edit/Write on the oracles: `tests/replays/**`,
  `tests/golden/**`, `**/snapshots/**`, `*.snap`, `bench/baseline.json`, `**/clippy.toml`, `deny.toml`,
  `xtask/tests/arch_fitness.rs`, `xtask/src/main.rs` (the verify definition), `.claude/hooks/**`,
  `.claude/settings*.json` and the CI workflow `../.github/workflows/npc-framework.yml`. Paths are compared after
  folding case, `\`, `..`, trailing dots/spaces, `\\?\` prefixes, NTFS streams and 8.3 short names. Shell commands
  are checked for what they *write* (best effort, not a sandbox): delete/move verbs on an oracle or a directory
  holding one, write verbs and redirects, copy destinations (copying *from* an oracle is fine), `git
  checkout|restore|reset|stash|apply ..` naming one (and the files a patch touches), PowerShell aliases
  (`ri`, `cpi`, `mi`, `rni`, ..) and `[IO.File]::` writes, `python -c` / `node -e` writes, relative paths after
  `cd`, `--out`, `cargo insta accept`, `INSTA_UPDATE=always`.
- **Re-baseline:** one procedure, [`rebaselining.md`](rebaselining.md): the agent **stages** the new oracles next
  to the old ones (`tests/replays.next/`, `*.snap.new`), writes the before/after **evidence** note and asks; the
  **user approves** and **swaps** them in (in their own shell, or by creating `.claude/rebaseline-approved` and
  telling the agent in chat to run the swap commands), then deletes the marker. An agent never overwrites an
  oracle on its own and never creates or deletes the marker. The hook also blocks `--out tests/replays.next/..`
  (record into `target/` and copy, or use `scripts/rebaseline/stage_replays.sh`).
- **Stop** (`.claude/hooks/verify_on_stop.sh`) runs `cargo xtask verify --quick` when a `.rs`, `.toml` or `.json`
  changed since the last run and refuses to end the turn while it reports `ok:false`; state and last verdict in
  `target/claude-hooks/`.
- The hooks are POSIX `sh` (Git Bash on Windows); `xtask/tests/hooks.rs` tests them.

## 9. Recipes

### 9.1 Why did unit X do Y at tick T?

(The `/why` skill automates this.)

```sh
# 1. The decisions around T (add --commands/--villagers/--scenario as in the run you are explaining)
cargo run -q -p npcctl -- journal --seed 1 --ticks 3101 --unit 5 --from 2900 --to 3100 \
  --kind ThinkStart,LeafStart,LeafStop,Abort,NeedLevel,Reserve,Release,Effect,Command,PhaseChange --limit 200
# 2. Which guards were true or false (CondEval exists only in a verbose journal)
cargo run -q -p npcctl -- journal --seed 1 --ticks 3101 --unit 5 --from 2900 --to 3100 --kind CondEval --verbose
# 3. The state at T: needs and levels, tree path, active leaf, blackboard, reservations, order
cargo run -q -p npcctl -- inspect --seed 1 --tick 3000 --unit 5 --pretty
```

Read it as: which branch ran (`LeafStart.path`), what triggered the think (`ThinkStart.reason`), which
higher-priority guards were false (`CondEval`) and why (need value vs `NeedDef.thresholds`, night, `Reserve
ok:false`, empty stockpile), what aborted (`Abort.guard`). For capability units add their kinds:
`--kind OrderIssued,OrderStarted,OrderDone,OrderFailed,Interacted,Roll,PathFailed,Perceived,Lost,CraftFailed`.
A failed check is a `Roll` with `success:false` and its `purpose`; a refused verb is `Interacted
{outcome:"rejected", reason}`. In a live game use BRP `npc/journal {"unit":5,"from":2900,"to":3100}` and
`npc/unit {"id":5}`.

### 9.2 Reproduce and bisect a desync

(The `/debug-desync` skill automates this.)

```sh
cargo run -p npcctl -- replay verify tests/replays/village_orders.json         # {ok:false, first_mismatch:[tick,expected,actual]}
cargo run -p npcctl -- diverge --seed-a 1 --seed-b 1 --ticks 4000 --pretty     # the same seed twice must not diverge
cargo run -p npcctl -- diverge --a old.json --b new.json --pretty              # two replays of the same content
```

1. `first_divergent_tick` and `components_differing` name the entity and the first state that differs; the first
   component usually points at one system (`pos` → a mover, `blackboard` → a decision, `needs`, `order_queue`).
2. Journal both sides just before that tick for that unit (`journal --unit ID --from T-50 --to T --verbose`) and
   diff the files: the first different entry is the decision that split.
3. Same inputs, different code: build the old revision in its own worktree (`git worktree add ../npc-old <rev>`,
   never `git stash`), run the same command there and diff. Bisect with
   `git bisect run sh -c 'cargo run -q -p npcctl -- replay verify tests/replays/<name>.json'`.
4. Cross-OS: CI's determinism job uploads `hashes-Linux.json` / `hashes-Windows.json`; record a replay on each OS
   and `diverge --a linux.json --b windows.json`.
5. Usual causes: iteration in `HashMap`/query/`Entity` order, wall clocks or floats in sim state, a reused
   `sim_rand` purpose, an unordered system pair, tooling that writes sim state, a component missing from the hash
   or hashed with a platform-dependent encoding. An intended behaviour change makes replays fail rightly: ask the
   user for a re-baseline ([`rebaselining.md`](rebaselining.md)).

### 9.3 Find the slowest mechanic

```sh
cargo run --release -p npcctl -- bench --villagers 1,100,500 --ticks 2000 --pretty --out target/bench.json
cargo run --release -p npcctl -- bench --villagers 500 --ticks 2000 --baseline bench/baseline.json   # exit 1 on a regression
cargo run --release -p npcctl -- bench --scenario testbed --ticks 2000 --top 20
```

`results[].mechanics` is sorted by whole-run total time; `systems` lists the top N systems with their mechanic.
`think` breaks down into the spans `think.agent` (per agent), `think.cond` (conditions), `think.start` /
`think.stop` (action start/stop). Live: press **F4** (over-budget mechanics are marked) or

```sh
rpc npc/perf/reset; sleep 10; rpc npc/perf '{"top":10}'
```

For one system in detail, run the village with `--features tracy` (§7.3).

### 9.4 Drive the live game with curl

```sh
cargo run -p village -- --seed 1 &                                   # or: cargo run -p actor_testbed
until rpc npc/methods >/dev/null 2>&1; do sleep 2; done
rpc npc/pause                                                        # {tick, paused:true, ..}
rpc npc/step '{"ticks":600}'                                         # {tick, ticks, hash, hash_hex}
rpc npc/unit '{"id":5}'
rpc npc/command '{"cmd":{"type":"Order","unit":5,"order":{"type":"GoTo","pos":[8000,8000]}}}'
rpc npc/command '{"cmd":{"type":"SetNeed","unit":5,"need":"Hunger","value":10000}}'
rpc npc/journal '{"unit":5,"kinds":["LeafStart","Abort"],"limit":20}'
rpc npc/record/start '{"every":100}'; rpc npc/resume; sleep 30; rpc npc/record/stop '{"path":"live"}'
cargo run -p npcctl -- replay verify target/npc-out/live.json        # a live recording replays headless
```

Testbed orders: `rpc npc/command '{"cmd":{"type":"IssueOrder","player":1,"units":[33,35],"order":{"type":"Move","pos":[0,-5000]},"queue":false,"formation":{"kind":"Line","spacing_mm":1500}}}'`.

### 9.5 Screenshot at tick N

(The `/verify-visual` skill automates this.) Time-based, then exit:

```sh
cargo run -p village -- --seed 1 --screenshot "$PWD/target/shots/quick.png" --after-secs 8
cargo run -p actor_testbed -- --select 38 --panels sheet,perception --screenshot "$PWD/target/shots/tb.png" --after-secs 8
cargo run -p rts_units_demo -- --screenshot "$PWD/target/shots/rts.png" --after-secs 18
```

At an exact tick (the sim is deterministic, the frame is not):

```sh
cargo run -p village -- --seed 1 &        # or actor_testbed -- --paused
until rpc npc/methods >/dev/null 2>&1; do sleep 2; done
T=$(rpc npc/pause | python -c "import json,sys; print(json.load(sys.stdin)['result']['tick'])")
rpc npc/step "{\"ticks\":$((1200 - T))}"
rpc npc/screenshot '{"path":"shots/tick_1200"}'                    # -> target/npc-out/shots/tick_1200.png
until [ -s target/npc-out/shots/tick_1200.png ]; do sleep 1; done    # it lands 1-2 frames later
cargo run -q -p npcctl -- inspect --seed 1 --tick 1200 --unit 5       # must match npc/unit at that tick
```

Read the PNG (an agent: with the Read tool). Write shots under `target/shots/` (git-ignored); never commit them as
goldens without approval.

### 9.6 Inspect a recorded agent session

```sh
cargo run -p npcctl -- serve --scenario smithy --record target/session.json &   # ... agent plays ... then:
cargo run -p npcctl -- session stop
cargo run -p npcctl -- replay verify-session target/session.json                # {ok, checked, first_mismatch}
```

The record is `{version, scenario:{doc}, seed, replay:{.., commands, checkpoints}}`; its commands are what the
agent did, stamped with their apply ticks:

```json
[359, 0, {"type":"AttachController","unit":33,"controller":"claude","mode":"Direct","on_idle":"Idle","force":false}]
[524, 0, {"type":"IssueOrder","player":1,"units":[33],"order":{"type":"Move","pos":[4500,-1500]},"queue":false}]
[528, 0, {"type":"AgentAct","unit":33,"controller":"claude","handle":1,"verb":"execute_orders","steps":[..],"latency_ticks":8}]
```

To see what the unit did, re-run the session's scenario with the record's commands and the journal on.
`--commands` takes a replay object, so extract `replay` from the record first, and pass the scenario the session
was served with (an embedded name as `testbed:<name>`, or the same scenario file) and the record's `seed`:

```sh
python -c "import json,sys; json.dump(json.load(open(sys.argv[1]))['replay'], open(sys.argv[2],'w'))" \
  target/session.json target/session_replay.json
cargo run -q -p npcctl -- journal --scenario testbed:smithy --commands target/session_replay.json --ticks 1200 \
  --unit 33 --kind ExternalAction,OrderDone,OrderFailed,Interacted,ItemMoved
cargo run -q -p npcctl -- inspect --scenario testbed:smithy --commands target/session_replay.json --tick 1000 --unit 33
```

```json
{"tick":528,"seq":2,"unit":33,"kind":{"type":"ExternalAction","handle":1,"verb":"execute_orders","status":"started","latency_ticks":null}}
{"tick":976,"seq":0,"unit":33,"kind":{"type":"ItemMoved","item":26,"kind":"store_key","stack":1,"from":null,"to":33,"slot":null}}
{"tick":977,"seq":0,"unit":33,"kind":{"type":"OrderDone","order":"Interact"}}
```

The re-run reaches the record's checkpoint hashes (`run --ticks <last checkpoint tick>` prints the same
`final_hash`). Latency and rejection counts (`agent stats`) live only in the running session: read them before
`session stop`. See [agent-experiments.md §7](agent-experiments.md#7-analysing-a-run) for the full analysis
workflow.
