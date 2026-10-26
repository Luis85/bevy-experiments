# npc-framework: agent guide

> **Launch Claude Code in this directory (`npc-framework/`).** The hooks in `.claude/settings.json` (protected
> oracles, verify on stop), `.claude/rules/` and the skills load only when `npc-framework/` is the project root.
> Started from the repository root (a shared vault; it deliberately has no hooks), none of the guardrails below
> run: if `/play-actor` or `/verify-visual` are not offered, you are in the wrong directory.

A deterministic 3D NPC/actor framework in Rust on **Bevy 0.20** (rustc ≥ 1.97.1), for colony sims and RTS
games. The sim is integer-only, runs headless, and is replayable bit for bit. Opt-in capabilities (perception,
rules, interaction, RTS command, vehicles, progression, crafting, agent control, grid A*) plug into a facade.
Example content: the village (villagers with needs, a food economy) and the Actor Testbed (every capability).

## Crate map

| Crate | Role | May depend on |
|---|---|---|
| `crates/npc_ai` | generic BT runtime (flat `TreeDef` by name, guards, actions as components, `ParamValue`) | `bevy_ecs` only |
| `crates/npc_kernel` | ids, clock, rng, `SimPos`, `NpcSimTick`/`NpcSet`, commands, orders, journal, replay, hash registry, inspect, freeze, `SpatialGrid`, host-safe driver (`SimControl`, `SimTime`), world snapshots (`snapshot`: save/load, ADR 0013) | `npc_ai`, `bevy_*` sub-crates, no rendering |
| `crates/npc_needs` / `npc_jobs` | data-driven `NeedDef`s + decay / `Provides`, `Approach`, `Reservations` | kernel |
| `crates/npc_locomotion` / `npc_nav` | `MoveIntent`/`MoveStatus` seam + `StraightLineMover` / opt-in grid A* (`NavAgent`, budget, replan, pass costs) | kernel |
| `crates/npc_perception` | sensors (integer FOV, grid LOS, hearing), `PerceptionMemory`, detection checks by name, conditions | kernel, `npc_ai` |
| `crates/npc_rules` | `CharacterSheet` (point-buy), skills/traits by name, pluggable `RuleSet`, 3d6 rolls (`Roll`), `SpendPoints` | kernel |
| `crates/npc_agent_io` | external agent control: controllers, observation/affordance/handler registries, session host, CLI/MCP clients | kernel, `npc_ai`, tools |
| `crates/npc_interaction` | items, inventory/equipment, doors, tools, verbs by name (`Interact` leaf/command), busy/finished hooks | kernel, jobs, locomotion, nav |
| `crates/npc_command` | `Owner`/`PlayerId`, recorded `IssueOrder`, `OrderQueue`, formations, squads, `ExecuteOrder`, executor/redirect hooks | kernel, ai, locomotion, nav |
| `crates/npc_vehicles` | `Vehicle` seats, `MountedIn`/`Occupants`, `mount`/`dismount`, `ByCrew` (`DriveVehicle`) / `Direct` | foundations, `npc_interaction` (ADR 0012) |
| `crates/npc_progression` | `Experience`, XP sources by name, level curve, `SpendPolicy`, practice/study/teaching | foundations, `npc_rules` (ADR 0012) |
| `crates/npc_crafting` | `RecipeDef` by name, `Workstation`, `CraftingOrders`, `Craft` leaf/command, quality, `Durability`, craft hooks | foundations, `npc_interaction`, `npc_rules` |
| `crates/npc_perf` | per-system/per-mechanic profiler, budgets | no rendering |
| `crates/npc_tools` | CLI helpers, bench harness, replay recorder; BRP `npc/*` behind `remote` (`npc/sheet` with `rules`) | kernel, perf, (rules) |
| `crates/npc_framework` | **the facade**: `NpcPlugins` (+ `with_<capability>()`), `NpcAppExt`, `prelude`; `glue/<a>_<b>.rs` pairwise capability glue, each behind both features (ADR 0011) | the generic crates |
| `crates/npc_view_helpers` | generic view: `VisualOf`, interpolation, panels, selection, screenshots, gizmos, `command_ui`, perf setup | `bevy`, kernel |
| `content/village_content` / `village_view` | the villager: needs, places, actions, tree, commands, scenarios / models, animation, camera, F1 overlay | facade (+ `bevy` for the view) |
| `content/testbed_content` / `testbed_view` | Actor Testbed: compound, scenarios, every capability on, `explorer` (AI explorer prompts, agent folders, clipboard, settings) / models, doors, vehicles, F5 sheet, F6 perception, F7 notes editor, prompt modal, RTS UI | facade (+ `bevy` for the view) |
| `apps/village` / `apps/actor_testbed` | windowed demos (speed keys, BRP `npc/*` + `agent/*`; the testbed's explorer mode `--explorer-home`) | facade + content + view |
| `apps/game_menu` | the launcher (`cargo run -p game_menu`): starts the games as child processes, settings, explorer New/Resume, detached worlds (Detached screen, Open in window, Stop) | `bevy` + `testbed_content` (no sim) |
| `apps/npcctl` | headless JSON CLI (never a renderer; village + testbed factories) | facade + content |
| `apps/rts_units_demo` | portability proof: own units/map/physics, 10 Hz in host `FixedUpdate`, `--headless-ticks N` | facade + `bevy` only |
| `xtask` | `cargo xtask verify`, architecture fitness and hook tests | tooling |

Capability crates depend on each other only per ADR 0012's whitelist; everything else is facade glue. ADRs:
`docs/adr/` (0008 host-owned schedule, 0009 registration by stable name, 0010 agent transport, 0011 glue, 0012 tiers, 0013 world snapshot).

## Golden rules

1. **Determinism.** Sim code never reads `Time`, wall clocks, frame delta, `HashMap`/`HashSet` iteration
   order or `Entity` order, and has no float arithmetic in sim state. Use `SimClock`, `SimPos` (mm, `i32`),
   milli-unit needs, `BTreeMap`/sorted `Vec`, iteration sorted by `SimId`, and `rng::sim_rand` (counter-based).
   Same seed + same commands = same state hash. Clippy enforces part of this (`clippy.toml`).
2. **Core/content boundary.** Generic crates never name content, never depend on `content/*` or (headless ones)
   `bevy_render`/`wgpu`. Content uses only the facade and registers by stable name (`NpcAppExt`).
   `xtask/tests/arch_fitness.rs` fails the build otherwise.
3. **No clock or host hijack.** The library never writes `Time<Fixed>`, binds input, or overwrites
   resources the host owns. Keyboard bindings live in apps. Public labels are prefixed `Npc*`.
4. **Oracles are protected.** Do not edit goldens, snapshots, the replay corpus, `bench/baseline.json`,
   `clippy.toml`, `deny.toml`, `arch_fitness.rs`, `xtask/src/main.rs`, the CI workflow or `.claude/hooks/` to
   make a check pass. A hook blocks it.
   If a test or golden looks wrong, **say so and ask the user**; that is a valid outcome, not a failure.
5. **Run verify, show evidence.** Done means `cargo xtask verify` prints `"ok":true`. Quote the command and
   its result instead of claiming success. Debug tooling must never change sim results.

Path-scoped detail loads automatically from `.claude/rules/` (sim, view, content).

## Commands

```sh
cargo xtask verify            # full gate: fmt, clippy -D warnings, nextest/test, arch, replays, goldens, deny
cargo xtask verify --quick    # what the Stop hook runs (no deny, no long tests)
cargo test -p <crate> [--test <file>]    # the inner loop while you work; verify once at the end
```

Timings (busy machine): fresh build ~15 min, `verify --quick` ~5 min, full `verify` 40–50 min (run it once, last).

Headless CLI (`cargo run -p npcctl -- <cmd>`; JSON out, exit 0 ok / 1 check failed / 2 usage error):

```sh
npcctl run --seed 1 --ticks 7200 [--villagers 25] [--commands cmds.json] [--journal j.jsonl]
npcctl inspect --seed 1 --tick 1800 --unit 5            # or --all
npcctl journal --seed 1 --ticks 4000 --unit 5 --kind LeafStart,Abort --from 3000 --to 4000 --limit 50
npcctl replay record --ticks 2400 --out r.json  /  npcctl replay verify tests/replays/<name>.json
npcctl diverge --seed-a 1 --seed-b 2 --ticks 2400       # or --a r1.json --b r2.json
npcctl bench --villagers 1,100,500 --ticks 2000 --baseline bench/baseline.json   # release build
npcctl run --scenario testbed[:<name>] --ticks 4800    # any subcommand above; or --scenario file.json
npcctl serve --scenario smithy --record s.json  /  npcctl agent observe --unit 33 --format text  /  npcctl mcp
```

Live games serve BRP on `127.0.0.1:15702` (`npc/*`, `agent/*`, `world/state`; dev builds, or `--remote`). Change
the sim only through `npc/command`. RPC file paths are relative to `target/npc-out/`. `--screenshot shot.png
--after-secs 5` saves a PNG and exits (Read it). Tokens, methods, security: `docs/debugging.md` §3.

## Workflow

1. Read the spec/plan for the task (`docs/superpowers/specs/`, `docs/superpowers/plans/`) and the relevant ADR.
2. Write the failing test first: a headless scenario test, a replay, or an invariant.
3. Implement in the right crate (rules 1–3). Check Bevy 0.20 APIs in `docs/research/bevy-0.20-cheatsheet.md`
   or the registry source; do not trust memory for Bevy APIs.
4. `cargo xtask verify`. For visible changes also `/verify-visual`.
5. Record a decision that later agents must not relitigate as an ADR; add a `CHANGELOG.md` line.

Skills: `/add-mechanic`, `/add-unit-type`, `/verify-visual`, `/debug-desync`, `/why`, `/play-actor`.

## Guardrails (hooks in `.claude/settings.json`)

- **PreToolUse** (`.claude/hooks/protect_goldens.sh`): blocks Edit/Write on the oracles of golden rule 4 (incl.
  `tests/golden/**`, `**/snapshots/**`, `.claude/settings*.json`) and shell commands that write them (redirects,
  delete/move/copy verbs, `git checkout|restore|apply ..`, `--out`, `cargo insta accept`; `docs/debugging.md` §8.2).
- **Re-baseline** (one procedure, `docs/rebaselining.md`): the agent **stages** new oracles next to the old ones
  (`tests/replays.next/`, `*.snap.new`), writes before/after **evidence** in `docs/_notes/` and asks; the **user**
  approves and **swaps** (or creates `.claude/rebaseline-approved` and asks the agent to). Agents never overwrite
  an oracle on their own and never create or delete the marker.
- **Stop** (`.claude/hooks/verify_on_stop.sh`): runs `cargo xtask verify --quick` when a `.rs`, `.toml` or `.json`
  changed, and refuses to end the turn while it reports `ok:false` (state: `target/claude-hooks/`). If the failure
  is not yours (or is a staged re-baseline), say so. Hooks are POSIX `sh` (Git Bash on Windows).

## Docs

- Guides: `docs/using-npc-framework.md` (integrating into a game), `docs/debugging.md` (every CLI subcommand,
  RPC method, journal kind, budget, recipes), `docs/rebaselining.md`, `docs/agent-experiments.md` (AI agents
  playing actors), `docs/adding-a-host-app.md` (a new windowed app in this workspace: the `deny.toml` step).
- Specs: `docs/superpowers/specs/`. Research: `docs/research/` (best practices, agent-driven gamedev, Bevy 0.20).
- Decisions: `docs/adr/`. History: `CHANGELOG.md`. Benchmarks: `bench/README.md`. Evidence notes: `docs/_notes/`.
