# Debugging, tracing & performance tooling: design

Date: 2026-10-23 · Status: scope approved by user (pasted requirements, quoted below). Builds on `2026-10-23-villager-npc-design.md`.

## 1. Intent

Humans and **AI agents** must be able to trace, debug and profile the game easily in debug mode. An agent cannot watch a window, so everything an agent needs must be reachable as **JSON**: through a headless CLI, or through a localhost JSON-RPC interface on the running game. Performance must be measurable for every system and every mechanic, because the framework targets hundreds of NPCs and RTS scale.

User requirements (verbatim scope):

- **For AI agents:**
  - A headless CLI with JSON output to run scenarios, inspect any unit at any tick, filter the journal, verify replays, and find the first tick where two runs diverge.
  - In debug builds, a JSON-RPC interface on localhost using Bevy Remote Protocol. Agents can pause, step ticks, query state, issue orders, spawn units, read the journal, read performance data and take screenshots.
  - A `docs/debugging.md` guide covering all of this.
- **Performance tracing:**
  - A built-in profiler times every system, each one tagged with the mechanic it belongs to, with rolling p50, p95 and max per mechanic.
  - Each mechanic can have a time budget; going over it logs a warning.
  - Results show in a live debug panel, over the remote interface, and in benchmark JSON files.
  - For deeper digging, optional builds record every system in the Tracy profiler or as Chrome trace files.

## 2. Non-goals / anti-goals

- **Non-goals:** networking or multiplayer, a GUI debugger, and save/load of arbitrary game state. Replays are seed plus command log, not snapshots.
- **Anti-goal:** debug tooling must never change simulation results. With journal and profiler on or off, the same seed and commands give the same state hash. A test enforces this.
- **Anti-goal:** release builds must not pay for, or expose, the remote interface or the profiler.
- **Anti-goal:** no agent-only formats. All output is plain JSON / JSON Lines with documented schemas.

## 3. Decisions

### 3.1 Feature gating
- Cargo feature `debug-tools`, enabled by default in `village` and `actor_testbed`. A release build runs `--no-default-features` (documented). `npcctl` has no such feature: it has no window, BRP or render profiler, and its tooling (profiler, journal) is always built in. *(Updated 2026-10-24: npcctl's empty `debug-tools` feature was removed; BRP is off by default in release builds of the apps, see `docs/debugging.md` §3.2.)*
- Optional features `tracy` (`bevy/trace_tracy`) and `chrome-trace` (`bevy/trace_chrome`) record every system for deep dives.
- The sim crates keep their journal, commands, inspect and replay code always compiled. These are cheap, and recording is off unless enabled at runtime.

### 3.2 Journal (decision and event log)
- `npc_ai` gets an optional per-call **think observer**: `think` records events into an `AiJournal` resource buffer when it exists, and records nothing otherwise. Events:
  - `ThinkStart { agent, reason }`, where reason is Request, LeafDone or NoLeaf;
  - `LeafStart { node, action, status }`;
  - `LeafStop { node, action, reason }`;
  - `Abort { by_guard, aborted_path }`;
  - `CondEval { cond, result }`, recorded only at verbose level;
  - `RootDone { status }`.
- `npc_sim` stamps those events and its own events into a `Journal` resource. Each `JournalEntry { tick, seq, sim_id, kind }` is serde-serializable JSON. Sim event kinds:
  - `NeedLevel { need, from, to }`
  - `Reserve { target, ok }`, `Release { target, reason }`
  - `Effect { .. }`
  - `Command { .. }` (applied commands)
  - `PhaseChange { night }`
  - `Spawn`, `Despawn`
- Storage: a ring buffer (default 50 000 entries) plus an optional JSON Lines file sink.
- `seq` is a per-tick counter, so `(tick, seq)` is a total order. Journal content is deterministic.
- Filtering by unit, kind, tick range or text is a shared function used by the CLI, the remote interface and the F3 panel.

### 3.3 Commands, orders and replays (the input layer)
- `SimCommand` (serde enum) is the only way external input changes the sim. Commands are applied in `SimSet::Inputs` in `(tick, seq)` order. Variants:
  - `SpawnVillager { pos, home }`, `Despawn { unit }`
  - `SetNeed { unit, need, value }`, `SetFood { stockpile, food }`
  - `Order { unit, order }`
- **Orders (new, minimal):** `Order::GoTo { pos }` and `Order::Clear`. The villager tree gets a `FollowOrder` branch in the existing RTS-orders slot, between Forage and NightSleep, with `Abort::LowerPriority`. It walks to the position, then clears the order.
- **Replay** = `{ version, seed, scenario, commands: [(tick, cmd)], checkpoints: [(tick, state_hash)] }`. It is recorded by the CLI and by the running game (`--record replay.json`, or over RPC).
  - **Verify** re-runs the replay and compares every checkpoint.
  - **Diverge** runs two replays (or two seeds) side by side and reports the first tick where hashes differ, then a per-entity, per-component diff at that tick. The per-entity diff uses per-entity hashes.

### 3.4 Inspection
- `npc_sim::inspect` exposes:
  - `snapshot(world) -> WorldSnapshot`, serde over all sim entities and resources, ordered by SimId;
  - `unit(world, SimId) -> UnitSnapshot`, which includes needs, activity, tree path with node names, blackboard, active leaf, held reservations, and that unit's recent journal.
- The CLI and the remote interface return exactly these structs.

### 3.5 Headless CLI: `apps/npcctl`
- A binary that depends on `npc_sim` only (no rendering), so it builds fast. Output is JSON to stdout, with `--pretty` and `--out FILE`; logs go to stderr.
- Subcommands:
  - `run --seed N --ticks T [--villagers K] [--commands cmds.json] [--journal out.jsonl] [--record replay.json] [--checkpoint-every N]` → a summary: per-day timeline, min/max needs, aborts, final hash.
  - `inspect --seed N --tick T [--unit ID | --all]` → a unit or world snapshot at tick T.
  - `journal --seed N --ticks T [--unit ID] [--kind K,..] [--from T1 --to T2] [--grep TEXT] [--limit N]` → JSON Lines.
  - `replay verify FILE` → `{ ok, checked, first_mismatch? }`.
  - `diverge (--a FILE --b FILE | --seed-a N --seed-b M) [--ticks T]` → `{ first_divergent_tick, entities: [{ sim_id, components_differing, a, b }] }`.
  - `bench --villagers 1,100,500,1000 --ticks T [--out bench.json]` → ticks/s and per-mechanic p50/p95/max at each scale.
- Exit codes: 0 ok, 1 verification or divergence failure, 2 usage error.

### 3.6 Live remote interface (BRP) in `village`
- Uses Bevy Remote Protocol on `127.0.0.1` with the default BRP port. It is compiled only with `debug-tools` and can be turned off with `--no-remote`. *(Updated 2026-10-24: on by default only in dev builds; a release build needs `--remote`.)*
- Bevy's built-in `world.*` methods stay available. The sim components that BRP needs get `Reflect`.
- Custom methods, all returning the same JSON structs as the CLI:
  - Control: `npc/pause`, `npc/resume`, `npc/step { ticks }`, `npc/speed { multiplier }`
  - State: `npc/state` (world snapshot), `npc/unit { id }`
  - Input: `npc/command { cmd }` (issue orders, spawn units, set needs and food; goes through the command queue, so it is recorded for replay)
  - Data: `npc/journal { filter }`, `npc/perf`, `npc/perf/reset`
  - Capture: `npc/screenshot { path }`, `npc/record/start`, `npc/record/stop { path }`

### 3.7 Profiler
- **Tracing layer:** with `debug-tools`, Bevy's per-system tracing spans are on. A custom tracing layer measures each span's wall time and aggregates it per system. Headless, `npcctl` installs the layer itself; in the game it goes in through `LogPlugin`'s custom layer.
- **Mechanic tags:**
  - Each system is tagged with a mechanic through a `MechanicTags` registry (system name → mechanic). The tags are `needs`, `think`, `actions.move`, `actions.eat`, `actions.work`, `actions.sleep`, `actions.idle`, `effects`, `clock`, `inputs`, `journal`, `view.visuals`, `view.anim`, `view.camera`, `view.overlay`, `view.scene` and `render`.
  - Untagged systems fall under `untagged`, or under their crate.
  - Fine-grained mechanic spans inside `think` cover per-agent think, condition evaluation and action start/stop, so `think` can be broken down.
- **Statistics:** rolling window (default the last 600 samples per system): p50, p95, max, calls and total. Mechanic statistics aggregate per frame or tick, summed over the mechanic's systems.
- **Budgets:**
  - The `PerfBudgets` resource holds a p95 budget in µs per mechanic.
  - Sim defaults are set at 1 villager and documented; they scale with agent count via `per_agent_us` where relevant.
  - Exceeding a budget logs a rate-limited `warn!` (at most once per mechanic per 5 s) and records a breach in the profiler's own breach list.
- Breaches never go into the journal, because journal content must stay deterministic.
- **Where it shows:** an F4 panel in the game, `npc/perf` over the remote interface, and `npcctl bench` JSON. Benchmark baselines live in `bench/baseline.json`.
- **Isolation:** profiling data lives in non-hashed resources. Timers never feed sim decisions.

### 3.8 In-game debug UI additions
- F3: the journal panel (last 30 entries, filtered to the selected unit).
- F4: the performance panel.
- `.`: step one tick while paused.
- Clicking a unit selects it (selection is view-only).

### 3.9 Docs
- `docs/debugging.md` is the full guide, with recipes:
  - "why did unit X do Y at tick T"
  - "reproduce and bisect a desync"
  - "find the slowest mechanic"
  - "drive the live game with curl"
- `npc-framework/CLAUDE.md` is a short agent entry point that points to it.

## 4. Testing
- **Determinism guard:** for the same seed and commands, the state hash is equal with journal and profiler on vs off.
- **Replay:** record, verify and get ok. Tamper with one command and get a mismatch at the right tick. Diverge on seeds 1 and 2 reports a tick and a differing entity.
- **Journal:** an abort scenario produces an `Abort` entry naming the guard. Filters work.
- **Orders:** GoTo makes the villager walk there and interrupts wander and work, but not urgent needs. Clear works.
- **Commands:** spawning a villager mid-run thinks and acts. Despawn releases reservations.
- **CLI:** integration tests run the `npcctl` binary for each subcommand and parse the JSON.
- **Remote:** an integration test starts a headless BRP app (no window) on a free port, then calls `npc/pause`, `npc/step`, `npc/unit`, `npc/command` and `npc/perf` over HTTP.
- **Profiler:** after N ticks every SimTick system shows up under a non-`untagged` mechanic. A budget of 0 µs triggers exactly one rate-limited warning.

## 5. Calls made by the designer (flag if wrong)
- The CLI is a separate binary, `npcctl`, so agents get fast builds and no window.
- Orders are minimal (`GoTo`/`Clear`), so "issue orders" has something real behind it. Richer orders come with the RTS iteration.
- Replays are seed plus commands plus hash checkpoints, not full snapshots.
- A selected-unit concept exists only in the view, by click.
