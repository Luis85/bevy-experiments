# Agent experiments: an AI controls an actor

The headline experiment of the agent-control spec (`docs/superpowers/specs/2026-10-23-agent-control-design.md`):
prompt an AI agent with what one actor sees, task it with a goal, and let it act as that actor while the world
keeps running in real time. The framework provides the interfaces (`npc_agent_io`); any agent can use them:
Claude Code through `npcctl` (the `/play-actor` skill), an MCP client, or a script. Every accepted action is a
recorded `SimCommand`, so a session replays bit for bit **without** the agent.

- [1. Start a world](#1-start-a-world)
- [2. Attach](#2-attach)
- [3. The real-time loop](#3-the-real-time-loop)
- [4. Staleness and the idle policy](#4-staleness-and-the-idle-policy)
- [5. A prompt template for an LLM agent](#5-a-prompt-template-for-an-llm-agent)
- [6. MCP](#6-mcp)
- [7. Analysing a run](#7-analysing-a-run)
- [8. A worked example: key, store, pickaxe, ore](#8-a-worked-example-key-store-pickaxe-ore)
- [9. The AI explorer](#9-the-ai-explorer)

Every method, parameter and result shape is in [debugging.md §4](debugging.md#4-agent-world-and-session-methods);
`npcctl` client flags are in [debugging.md §2.9](debugging.md#29-agent-subcommands-serve-agent-world-session-mcp).

## 1. Start a world

**Headless, real time** (no window, the fastest to start; the agent's only senses are the observations):

```sh
cargo build -p npcctl                                               # once; then use target/debug/npcctl
npcctl serve --scenario smithy --record target/session.json         # an embedded testbed scenario by name
npcctl serve --scenario testbed --port 15702 --tick-hz 20           # the full showcase (8 actors, 2 factions)
npcctl serve --scenario content/village_content/scenarios/agent_village.json   # a village scenario file
```

It prints `{"listening":"http://127.0.0.1:15702","port":15702,"scenario":"smithy","tick_hz":20,"record":..}` once
ready and keeps running until `npcctl session stop` (which writes `--record`). `--port 0` picks a free port
(read it from that line); `--seed N` overrides the scenario's seed. Embedded testbed scenarios: `testbed`,
`doors`, `locked_store`, `chop`, `craft`, `perception`, `vehicles`, `formation`, `agent`, `smithy`
(`content/testbed_content/README.md` describes each).

**Windowed** (a human watches while the agent plays): the Actor Testbed serves the same agent methods on BRP:

```sh
cargo run -p actor_testbed -- --scenario smithy --remote             # BRP on 127.0.0.1:15702
cargo run -p actor_testbed -- --scenario smithy --record target/tb.json --remote-port 15712
```

BRP is on by default in dev builds and off in release builds (`--remote` turns it on, `--no-remote` off). It has
no browser/DNS-rebinding protection (a `bevy_remote` 0.20 limit, [debugging.md §3.2](debugging.md#32-calling-brp)),
so use it only on a development machine; the headless `npcctl serve` refuses such requests and takes `--token`.

The village serves `agent/*` read-only (it has no `AgentIoPlugin`: list, observe and events work; attaching is
rejected). The session-only methods (`world/load_scenario`, `world/reset`, `session/*`) return `-23601` on BRP.

Check it is up and find a unit:

```sh
npcctl session info                       # headless only: scenario, content, seed, tick, tick_hz, methods
npcctl agent list-units --kind actor      # testbed actors: {id, kind, pos, controllable, controller}
npcctl run --scenario testbed:smithy --ticks 1 --pretty   # names → SimIds (content_summary.units)
```

Ids in the testbed. The world (doors, items, stations, vehicles) spawns before the actors, so its ids are the
same in **every** compound scenario, `testbed` and `testbed:smithy` alike: the workshop door **11**, the store
door **12** (locked, `key:store`), the workbench **13**, the anvil **14**, the forge **15**, the book **16**, the
pickaxe **17** and hammer **18** (inside the store), the ore rock **24**, the axe **25**, the store key **26**, the
bread **27**, the cart **29**, the wagon **30**. The actors differ: `smithy` has the smith **33**, joiner **35**,
apprentice **37**; the default `testbed` the carpenter **33**, smith **35**, woodcutter **36**, scout **38** (player
1) and the skeletons **39–42** (player 2). `npcctl agent list-units` shows each unit's `owner` player.

In `agent_village.json` the scenario pre-attaches controller `agent` (Direct) to its actor, unit **6**: act on it
with `--controller agent`, or `agent attach --unit 6 --controller me --force` first. The village's own villager
(unit 5 in `npcctl run`) has no controller in that scenario; attach before acting on any other unit.

## 2. Attach

```sh
npcctl agent attach --unit 33 --controller claude --mode direct [--on-idle idle|autopilot] [--force]
# {"applies_at":359,"controller":"claude","mode":"Direct","on_idle":"Idle","seq":0,"tick":359,"unit":33}
```

| Mode | The unit's own tree | The agent | Typical use |
|---|---|---|---|
| `Direct` | suspended by the injected top-priority `ExternalControl` branch | chooses every action; queued actions run the same registered actions the tree would | "you are this actor" |
| `Advisor` | keeps running | gives orders (`order`, `walk_to` becomes a `GoTo`), like an RTS player | commanding a squad |
| `Observer` | keeps running | read-only (`act` → `read_only`) | watching |

Attaching is a recorded command (`AttachController`) applied at the next tick (`applies_at`). One controller per
unit; `--force` takes a unit from another controller (a scenario's pre-attached controller, e.g. `agent` in
`agent.json`, can also simply be used by passing `--controller agent`). `npcctl agent detach --unit 33
--controller claude` hands the unit back to its tree. Several agents may control different units at once.

## 3. The real-time loop

The world does **not** wait for the agent. One loop iteration:

1. **Observe:** `npcctl agent observe --unit 33 --controller claude --format text --brief` (or JSON without
   `--format`; `--brief` hides affordances that are structurally impossible, like "door has no lock").
2. **Think** (the world keeps ticking meanwhile; an LLM turn takes seconds = tens to hundreds of ticks).
3. **Act**, optionally several steps at once with `--queue`.
4. **Wait for events** with a long poll instead of polling: `npcctl agent events --unit 33 --since-seq S
   --until-idle --dedupe` returns once the unit has finished everything it was given (action queue, orders,
   interaction, craft; 30 s default timeout), or `--kinds OrderDone,OrderFailed,Interacted --timeout-ms 20000`
   on the first matching event (`--min-events N` for more). Pass the returned `next_seq` next time.
5. Repeat from 1 (re-observe after anything surprising).

A text observation (`testbed:smithy`, unit 33, right after attaching, `--brief`):

```text
You are actor #33 (controller `claude`, Direct mode, on idle: Idle).
Tick 121 (day 0, 5.0% of the day, day time), seq 8.
You command for player 1: 5 unit(s).
No orders.
You are not crafting.
0 workstation(s) known.
You carry: the plank #34.
Load 800 of 30000 g, 1 of 8 slots used.
You perceive: locked door #12 at (6.50 m, 3.50 m), 4.00 m away (vision).
Level 0 (0 XP; next level at 100 XP). Level-up points are spent automatically (5 per level, with the rest of the bank, on your most-used skills; policy Ai).
Unspent character points on your sheet are a bank: at each level-up the whole bank is spent on the skills you used most (none used: it stays banked); until then you may spend it yourself with spend_points.
Position: (6.50 m, -0.50 m).
Sheet: ST 13, DX 10, IQ 11, HT 12; HP 13, Will 11, Per 11, FP 12, Speed 5.50, Move 5.
Skills: Mining 13 (ST), Smithing 13 (IQ+2).
Unspent character points: 0.
You are a smith of player 1; you walk 70 mm per tick.
Doing: nothing (waiting for your next action).
Recent events:
- t17 ControllerAttached {"controller":"claude","mode":"Direct","on_idle":"Idle"}
You can:
- [wait] Wait in place (~100 ticks) args {ticks: ticks to wait, 1..=1000000 (default 100)}
- [order] Give an RTS order to your units (player 1) args {..., order: {"type":"Move","pos":[x,z]} | ... | {"type":"Interact","verb":V,"target":ID} | ..., queue: .., units: ..}
- [interact] Walk into reach of a target and do a verb to it, in one act (an Interact order you carry out) args {queue: .., target: .., verb: ..}
- [execute_orders] Carry out your own orders -- unavailable: no orders
- [craft:ingot] Craft ingot (~80 ticks) -- unavailable: needs a workstation: forge
- [drop@34] drop the plank #34 (~5 ticks)
- [open@12] open the locked door #12 (4.0 m away) (~10 ticks) -- unavailable: door is locked
- [unlock@12] unlock the locked door #12 (4.0 m away) (~20 ticks) ... -- unavailable: door is locked: needs key:store
- ...
(2 structurally impossible action(s) hidden: brief view.)
```

(abridged). Lines come from the registered sections: `self`, `sheet`, `inventory`, `perception`, `command`,
`crafting`, `progression`, `testbed`. The JSON form has the same data as `sections.<name>`, plus
`affordances:[{id, verb, target?, pos?, label, args, est_ticks, requires, ok, why_not?, check?}]`,
`queue:{current, pending}`, `recent_events`, `tick`, `seq` and `epoch`.

**What the unit knows.** With perception on (`omniscient: false`) the observation is the unit's point of view:
`You perceive` lists what its senses report now, by name (`store_key #26`, `locked door #12`, `ore_rock #24 (needs
tool:pickaxe; yields 1 × ore; 6 uses left)`) with position and distance; `Within reach` (units with a
`ReachSense`, e.g. explorer actors: 1.5 m all round, never through walls) what it could touch, `(behind you)` for
what only reach senses; `You remember` everything it saw before, most recent first, with the last-known position
and age (what it carries or rides is never "remembered"). **Affordances only target known
entities** (perceived, remembered, carried or ridden): in the observation above the store key 2.2 m behind the
smith is not offered until it turns towards it, the pickaxe in the locked store not until the store is open and
the smith looks in, and a target whose memory expires (`Lost {forgotten: true}`) drops out. There is no distance
cut-off for known targets: a far one is offered with `why_not: "too far: 12.0 m (reach 1.5 m)"`. Things the
perception model cannot see at all (no `Perceivable`, such as the ore a mined rock drops at the unit's feet) are
known by touch within 1.5 m. Without perception (the village), or for a unit without senses (`omniscient: true`),
affordances cover everything within 10 m. Acts and orders by id are not gated, so a
plan made from map knowledge (the ids in §1) still works. `world/state` is debug tooling and shows everything.
At night a line says whether darkness reduces vision (it does not in the testbed: no light model).

**Odds.** A skill-checked affordance says its odds: `[Mining 13 vs 3d6: ~83%]` in the label and `check:
{skill, target_number, dice, odds_permille, default?}` in JSON; under 30% the label adds `low odds`, and a unit
that rolls against a default skill level shows it (`Mining 5 (default ST-5)`). A failed check spends the time and
does nothing (`Interacted {outcome:"failed"}`), so check the odds first.

**Recent events** are the unit's significant events, oldest first, with the agent's own outcomes (its actions,
rejections, interactions and their rolls, items, orders, crafts) kept in preference; perception rolls
(`Roll {purpose:"perception.detect"}`) are left out (ask `agent events --kinds Roll` for them).

**Acting.** Take an offered affordance by id, or name a verb:

```sh
npcctl agent act --unit 33 --controller claude --affordance pick_up@26
npcctl agent act --unit 33 --controller claude --verb order --args '{"order":{"type":"Move","pos":[4500,-1500]}}'
npcctl agent act --unit 33 --controller claude --verb execute_orders --queue
npcctl agent act --unit 6  --controller agent  --verb walk_to --pos 4000,2000          # village (agent_village.json)
```

`{"accepted":true,"handle":1,"verb":"execute_orders","label":"Carry out your orders","queued":true,"applies_at":528,..}`,
or `{"accepted":false,"reason":"unavailable","message":"door is locked"}` (exit code 1). Reasons:
`not_attached`, `not_controller`, `read_only`, `stale`, `unknown_affordance`, `unavailable` (the `why_not`),
`unknown_verb`, `invalid_args` (also a wrongly typed argument such as `"ticks":"50"`), `direct_only`, `invalid`.

**Walk and act in one call:** `npcctl agent act --unit 33 --controller claude --interact pick_up@26` (the
`interact` verb, args `{"verb": V}` plus `--target ID`): an `Interact` order for the unit that walks into the
verb's reach and does it, plus `execute_orders` for a `Direct` unit, as one atomic act. The outcome is `OrderDone
{order:"Interact"}` or `OrderFailed {reason}`, then `ExternalAction {status:"done"|"failed"}`.

**Several steps at once:** `--steps '[{"verb":"order","args":{"order":{"type":"Move","pos":[4500,6500]}}},
{"verb":"execute_orders"}]'` plans and validates every step before anything is submitted (all or nothing, so a
plan never half-applies); a rejection names the failing `step`. The first step replaces the action queue (unless
`--queue`), the rest append. A later step may rely on an earlier one (`execute_orders` after an `order`).

Things worth knowing about the testbed's verbs:

- An affordance with `why_not` "too far: …" must be **walked to** first: use `--interact VERB@ID`, or the two
  steps it stands for: `order` (any `UnitOrder`; an `Interact` order walks into the verb's reach by itself, then
  does it) followed by `execute_orders` (a `Direct` unit carries out its own order queue). The outcome arrives as
  `OrderDone {order:"Interact"}` (or `OrderFailed {reason}`), and the `execute_orders` action ends `done` when the
  order queue ran dry (`failed` when the last order failed).
- A failed order does **not** cancel the orders queued behind it: the unit goes on with the next one (it may walk
  off towards the next target right after an `OrderFailed`). Give `order {"type":"Stop"}` to drop them all.
- Verbs in reach (`pick_up@26` once adjacent, `equip@17`, `use_tool_on@24`, `craft:ingot` at the forge) run
  directly and report `ExternalAction {status:"done"|"failed"}` plus their own events (`Interacted`,
  `ItemMoved`, `CraftDone`, `Roll`).
- The village offers `walk_to`, `eat`, `sleep`, `work` and `wait` directly (Direct: the same actions the
  villager tree uses; Advisor: `walk_to` becomes a `GoTo` order).
- Two different queues: `--queue` appends to the unit's **action** queue (agent actions: `execute_orders`,
  `wait`, verbs in reach); without it the new action replaces that queue. An RTS order's own shift-queue is
  `"queue":true` *inside* `--args` of the `order` verb (it queues the order behind the unit's other orders); the
  CLI prints a note when `--queue` is given with `--verb order`. `npcctl agent cancel [--handle H]` and `agent
  clear` exist.

**Events.** `agent/events` returns as soon as a matching event happens for the unit (or at the timeout, with
`timed_out: true`):

```json
{"events":[{"event":{"order":"Move","type":"OrderDone"},"kind":"OrderDone","seq":31,"tick":561,"unit":33}],
 "journal":true,"next_seq":31,"seq":35,"tick":562,"timed_out":false}
```

Without `--kinds` it returns the significant kinds (action states, interactions, items, doors, need levels,
paths, perception, rolls except perception rolls, orders, mounting, XP, crafting); `--kinds all` returns
everything, and an unknown kind name is an error that lists the valid ones. `--dedupe` collapses repeated
identical events (`"repeats": n`). Every result has `epoch`: after `world reset` / `world load-scenario` it
changes and seqs start over (a `since_seq` beyond the latest seq is then an error: poll again from 0);
`oldest_seq` / `dropped` say whether the 20 000-event ring evicted events you had not read yet. An act that was
accepted but could no longer apply when its tick came (another controller took the unit first) ends as
`ExternalAction {status:"rejected"}` with an `ActionRejected {reason:"skipped at tick T: .."}`, so a waiting poll
wakes up.

## 4. Staleness and the idle policy

Every result carries `tick` and `seq`. An act may say what it is based on:

```sh
npcctl agent act --unit 33 --controller claude --affordance pick_up@26 --expect-tick 640 --expect-seq 31
# {"accepted":false,"reason":"stale","message":"observed at tick 640, now 1290: more than 600 ticks ago (re-observe, or allow_stale)"}
```

- `expect_tick` older than `AgentIoConfig::stale_ticks` (default **600** ticks = 30 s at 20 Hz, room for an
  LLM turn; a host may insert its own `AgentIoConfig`) → `stale`.
- `expect_seq`: more than `stale_events` (default **0**) *relevant* events for the unit since that seq →
  `stale`. Relevant: its own actions, orders, interactions or crafts ending (done, failed, cancelled, rejected),
  a need level change, mounting or dismounting, the controller detached, a despawn, a failed path, and a new or
  lost percept **of an entity the act refers to** (its target, `args.item`, an order's target or vehicle).
  Percepts of anything else no longer invalidate an observation. The message lists the events (`t640 OrderDone
  Move done; ..`) and `interrupting` holds them.
- `--allow-stale` opts out. Without `expect_*` there is no check.

The idle policy (`--on-idle`, per controller) decides what a `Direct` unit does when its queue runs empty:
`idle` (default) stands still until the next action; `autopilot` lets its own tree run until the next action
arrives (the agent then only steers). Each accepted act records `latency_ticks` (from the controller's last
`observe` to the act; `null` when it acted without observing first) in its `ExternalAction {status:"queued"}`
entry; `npcctl agent stats` summarises per controller (`observes`, `acts`,
`accepted`, `rejected`, `stale`, `latency_ticks` / `latency_ms` `{n,min,p50,p95,max,mean}`). Pause/step
(`npc/pause`, `npc/step`) exist for debugging, but experiments run in real time.

## 5. A prompt template for an LLM agent

The loop prompt the `/play-actor` skill follows; adapt it for your own harness:

```text
You control actor #{UNIT} in a running simulation, as controller `{CONTROLLER}` in {MODE} mode.
The world keeps running while you think: about {TICK_HZ} ticks per second. Positions are millimetres [x, z];
+z is north; 1000 = 1 m.

Your goal: {GOAL}

Here is your current observation (tick {TICK}, seq {SEQ}):
---
{OBSERVATION_TEXT}
---
Available actions are the lines under "You can:". Use one by its [id] (e.g. pick_up@26), or name a verb
with arguments (e.g. order with {"order":{"type":"Move","pos":[x,z]}}). An action marked "unavailable"
says why; fix the reason first (walk there, get the key, equip the tool). To walk to a target and use it,
use interact with {"verb":V} and the target (one action); to walk somewhere, give yourself an order and then
execute_orders. You only see what your senses report: look around (move) to find things.

Reply with your next 1-3 actions as JSON lines, one per line, then one line `WAIT <kinds>` naming the
events that would make you re-plan:
{"affordance":"<id>"}  or  {"verb":"<verb>","target":<id>,"pos":[x,z],"args":{..},"queue":true}
WAIT OrderDone,OrderFailed,Interacted,CraftDone,CraftFailed
If the goal is reached, reply DONE and why. If it is impossible, reply GIVE UP and why.
```

The harness turns each JSON line into `npcctl agent act ... --expect-tick {TICK} --expect-seq {SEQ}` (the first
without `--queue`, the rest with it), long-polls `npcctl agent events --since-seq {SEQ} --kinds <kinds>
--timeout-ms 20000`, re-observes and repeats. Feed rejections (`reason`, `message`) back into the next prompt.

## 6. MCP

`npcctl mcp --url URL` is a stdio MCP server in front of a session or a windowed game, with the tools
`observe` (default format `text`), `act`, `cancel`, `events`, `attach`, `list_units` (the same parameters as the
methods). Build npcctl first so the server starts instantly, then register it with Claude Code, in the project's
`.mcp.json`:

```json
{
  "mcpServers": {
    "npc": {
      "type": "stdio",
      "command": "target/debug/npcctl",
      "args": ["mcp", "--url", "http://127.0.0.1:15702"]
    }
  }
}
```

or with `claude mcp add npc -- target/debug/npcctl mcp --url http://127.0.0.1:15702` (on Windows the command is
`target\debug\npcctl.exe`; any MCP client takes the same command and arguments). Start `npcctl serve` (or the
windowed testbed) before the client calls a tool; a tool call against a server that is not running returns an
error result naming the URL (`http://127.0.0.1:15702/ (connection refused ..)`). A session started with
`npcctl serve --token T` needs `"args": ["mcp", "--url", URL, "--token", "T"]`; the server answers a missing or
wrong token with `401` and a message naming `--token`. Tool results carry the JSON as `structuredContent` and a text rendering as
`content`.

## 7. Analysing a run

**Record.** `npcctl serve --record target/session.json` writes a session record on `session stop`:
`{version:1, scenario:{the scenario document}, seed, replay:{version, content_hash, seed, scenario, ticks,
commands:[[tick,seq,cmd]], checkpoints:[[tick,hash]]}}`. The commands are exactly what the agent did
(`AttachController`, `IssueOrder`, `AgentAct` with its steps and `latency_ticks`, `AgentCancel`, ..), stamped with
the ticks they applied at. Rejected acts are not in it (they never became commands). The windowed testbed's
`--record FILE` writes a replay on exit.

**Verify** that the run is reproducible without the agent:

```sh
npcctl session stop
npcctl replay verify-session target/session.json     # {"ok":true,"checked":13,"first_mismatch":null}
npcctl replay verify target/session.json             # the same: verify dispatches session records
```

**Read the journal of the run.** Extract the replay and re-run the session's scenario with its commands (use the
scenario name the session was served with, as `testbed:<name>`, or the same scenario file):

```sh
python -c "import json,sys; json.dump(json.load(open(sys.argv[1]))['replay'], open(sys.argv[2],'w'))" \
  target/session.json target/session_replay.json
npcctl journal --scenario testbed:smithy --commands target/session_replay.json --ticks 1200 --unit 33 \
  --kind ExternalAction,OrderDone,OrderFailed,Interacted,ItemMoved,CraftDone,CraftFailed,Roll
npcctl inspect --scenario testbed:smithy --commands target/session_replay.json --tick 1000 --unit 33 --pretty
npcctl run --scenario testbed:smithy --commands target/session_replay.json --ticks 1200 --pretty   # content_summary
```

```json
{"tick":930,"seq":1,"unit":33,"kind":{"type":"ExternalAction","handle":2,"verb":"execute_orders","status":"queued","latency_ticks":410}}
{"tick":976,"seq":0,"unit":33,"kind":{"type":"ItemMoved","item":26,"kind":"store_key","stack":1,"from":null,"to":33,"slot":null}}
{"tick":977,"seq":0,"unit":33,"kind":{"type":"OrderDone","order":"Interact"}}
```

Questions to ask of a run: how long did each decision take (`latency_ticks` on `queued`, `agent stats` before
stopping)? Which acts were rejected and why (`ActionRejected` in the live session's `npc/journal` or
`agent events --kinds ActionRejected`)? Did the world change under the agent (interrupting events between
observe and act)? Where did it waste time (gaps between `OrderDone` and the next `queued`)? To compare two
attempts at the same task, record both and `npcctl diverge --a run1_replay.json --b run2_replay.json` (the first
tick and the components where they part ways), or diff their journals.

## 8. A worked example: key, store, pickaxe, ore

The task the testbed's scripted agent test solves (`content/testbed_content/tests/glue.rs`, "Agent io, fetch and
mine"), done by hand with the CLI (`testbed:smithy`, the smith 33):

```sh
npcctl serve --scenario smithy --record target/session.json &
npcctl agent attach --unit 33 --controller me --mode direct
npcctl agent observe --unit 33 --controller me --format text          # the key is 2.2 m away; the store door is locked
npcctl agent act --unit 33 --controller me --interact pick_up@26        # the key (by id: not seen yet)
npcctl agent events --unit 33 --since-seq S --until-idle              # OrderDone {order:"Interact"}
npcctl agent act --unit 33 --controller me --interact unlock@12        # then the same wait after each act
npcctl agent act --unit 33 --controller me --interact open@12
npcctl agent act --unit 33 --controller me --interact pick_up@17       # the pickaxe, now in view
npcctl agent act --unit 33 --controller me --affordance equip@17
npcctl agent act --unit 33 --controller me --steps \
  '[{"verb":"order","args":{"order":{"type":"Move","pos":[8500,-5000]}}},{"verb":"execute_orders"}]'
npcctl agent observe --unit 33 --controller me --format text --brief   # [use_tool_on@24] .. [Mining 13 vs 3d6: ~83%]
npcctl agent act --unit 33 --controller me --interact use_tool_on@24
npcctl agent events --unit 33 --since-seq S --until-idle --kinds OrderDone,OrderFailed,Roll,Interacted
npcctl agent stats
npcctl session stop && npcctl replay verify-session target/session.json
```

Along the way the observation's `why_not`s explain every refusal: "door is locked: needs key:store", "too far:
2.2 m (reach 1.5 m)", "needs tool:pickaxe", "equip tool:pickaxe first". The four `interact` steps can also be
shift-queued orders (`--verb order --args '{"order":{..},"queue":true}'`) and one `execute_orders`; remember that a
failed order does not stop the ones behind it. The `/play-actor` skill
(`.claude/skills/play-actor/SKILL.md`) runs this loop for any unit and goal.

## 9. The AI explorer

The longer-running version of this experiment (game menu spec, `docs/superpowers/specs/2026-10-24-game-menu-and-ai-explorer-design.md`):
an AI agent of the dev's choice wakes in a world of its own with an Epoch 1 mission (survive, learn, explore, map),
keeps notebooks as its only memory, and writes improvement proposals for the game. The world is saved and resumed exactly.

**The flow.**

1. `cargo run -p game_menu` → **Settings**: the agents folder (default `<home>/npc-agents`, kept in
   `%APPDATA%\npc-framework\settings.json`, `$XDG_CONFIG_HOME` or `~/.config`; `--settings FILE` for another
   file). **Explore with an AI agent** stays disabled until the folder is valid.
2. **New**: the agent and world names come prefilled with random names (**Random** rolls again; Ctrl+C, Ctrl+X and
   Ctrl+V use the OS clipboard). **Start** stays disabled, with the reason inline, while a name is blank, has no
   letters or digits a folder can use, or names an agent that exists. Under the names the screen says "After
   Start, open your AI tool in <agents folder>/<slug(agent)> and paste the prompt": the folder the game prepares.
   **Start in window** runs `actor_testbed --explorer-home DIR --agent NAME --world NAME`: the `explorer` scenario
   (`content/testbed_content/README.md`, the `frontier` layout) with one actor named after the agent under the
   `Direct` controller `explorer`, a wall-clock seed, saved at tick 0. On every start and resume the game writes
   the agent's **prepared folder** (`testbed_content::explorer::workspace::write_workspace`, each file atomically):
   `AGENTS.md` (the actor, the world, the tools with this run's URL, the notes, the rules), `CLAUDE.md`
   (`@AGENTS.md`), `prompts/<kind>.md`, `.claude/settings.json` (only the game's MCP tools and reading the folder;
   no shell, edits or web), `.claude/settings.local.json` (enables the `npc` server) and `.mcp.json`
   (`npcctl mcp --url ...`). A folder that cannot be written is logged and named in the modal's notes; the world
   still starts. **Start detached** runs the same world without a window (`npcctl serve`, see **Detached** below).
3. The game window opens **paused** behind a modal with the prompt: **Copy** puts it on the OS clipboard (a
   note names `prompts/<kind>.md` when the copy tool fails), **OK** closes the modal and starts the sim. Open the
   agent tool (Claude Code, Codex, a chat with MCP or a shell) **in the agent's folder**, paste the prompt into a
   fresh session and press OK. Claude Code asks once to trust the folder; until then its settings (and so the
   game's tools) are not applied.
4. The agent plays through `npcctl agent observe | act | events` (or `npcctl mcp`) on the port the game picked
   (the first free one in 15702..=15721, or from `--remote-port N` on, whose next port is free too for the render
   server; named in the prompt and the modal's notes). **F7** is the notes editor: tabs Diary,
   Memory and Proposals (Tab cycles them), newest first with ids and stamps, re-read within ~1 s of an agent's
   write; **Add** / **Edit** (click an entry's title) open a title and a body field, **Save** writes as the player
   and never silently replaces an entry the agent changed meanwhile: the save is refused, the form keeps your
   text and shows the entry's current title and body above it ("Now in the file:"), and only a second Save
   replaces it (if it changed again, you are shown the newer version first). A proposal's status buttons set its
   status (a stale one says "changed meanwhile - reloaded"). While a field has focus the game's keys do nothing (Escape leaves the field);
   `--panels explorer` opens it at start.
5. Closing the game window asks **"Keep it running detached?"**: **Yes** keeps the world running without a window
   on the same port (the agent keeps playing; the menu shows its Detached screen), **No** saves and stops it, Escape
   goes back to the game. **F9** moves it to detached without asking. The world is autosaved every 60 s of wall
   clock and when the game exits.
   **Fog of war** (spec G8): the window shows the world as the agent's actor knows it, like Age of Empires: black
   where it has never been, greyed and **as last seen** where it is not looking now (walls, doors open or closed as
   they were, trees standing or felled, stations, chests, items lying on the ground; never a unit), and clear where
   its senses reach. Units (and vehicles) are drawn only while the actor senses them (its "You perceive" list), the
   actor itself and what it rides always. **F8** toggles it while watching; it is on by default for explorer windows
   (Settings has a **Fog of war** checkbox, saved at once and passed as `actor_testbed --fog on|off`; the flag also
   works by hand, default off outside explorer mode). Units the actor does not sense get no crew, path, selection
   or order gizmos and cannot be selected (a selected one is deselected when it leaves the actor's senses). The F1
   item markers follow the fog (none where the item is hidden, at the last-seen spot where it is drawn as last
   seen); the rest of F1 and the F5, F6 and F7 panels are not fogged. A brand-new
   world is black but for the actor until its first perception scan (a few ticks after OK).
6. **Resume** lists the agents with their world and last save date ("no save yet" ones are disabled); a row starts
   the world in the window, its **Start detached** button without one. A world that runs detached (started by this
   menu, or found through its live `run.json`) shows "running detached on port N - Open / Stop" with those buttons
   (the row opens its Detached screen); one running in a window cannot be started twice. Resume first
   runs `actor_testbed ... --check-save` ("Checking ..."); a save that fits starts the game with
   `--resume`, which rebuilds the world from the save header's scenario and seed and loads the snapshot (ADR
   0013) before the first frame: the actor stands where it was, mid-order, with its inventory, doors, items,
   the clock and the controller attachment. A check that crashes (an exit code other than 0 or 1) is reported on
   the Resume screen with its last stderr line; one still running after 30 s, or when you go Back, is killed.

**The folder layout** (`testbed_content::explorer::home` and `::workspace`; folder and file names are slugs, the real names stay in
the files). The generated files are rewritten on every start and resume (hand edits there are lost; an older
folder gets them on its next resume, and its old `start_prompt.md` / `resume_prompt.md` are deleted); the notes,
`agent.json` and the saves are never touched by it:

```
<agents folder>/
  <slug(agent)>/
    agent.json                 {agent, world, created}
    AGENTS.md                  generated: the actor, the world, the tools (this run's URL), the notes, the rules
    CLAUDE.md                  generated: `@AGENTS.md`
    prompts/start.md           generated: the prompt the modal showed on New
    prompts/resume.md          generated: the prompt of the last Resume (also resume_changed.md, resume_rebuilt.md)
    .claude/settings.json      generated: MCP-only permissions for Claude Code
    .claude/settings.local.json  generated: enables the `npc` server of .mcp.json
    .mcp.json                  generated: the game's MCP server (`npcctl mcp --url URL --controller explorer`)
    diary/YYYY-MM-DD <title>.md   one file per diary entry (frontmatter id, type, title, aliases, tags, agent, world,
                               author, created, created_tick, updated, updated_tick, revision; then the body)
    memory/<title>.md          one file per memory, same frontmatter
    proposals/<title>.md       one file per proposal, plus status and prompted_by
    diary.md.bak, memory.md.bak   an old one-file diary / memory after it moved into its folder (kept, never read)
    <slug(world)>.world.json   the save: {version, content_hash, tick, header {agent, world, scenario, seed, saved}, entities, resources}
    <slug(world)>.world.<date>.json   a backup made by "Fresh world, keep the diary"
```

The notes are written only through the game (the notes tools, `npcctl agent notes`, `npcctl notes`, F7); the
agent never writes files. Every entry is a markdown file with YAML frontmatter named after its title, so the
agent folder (or the agents folder) opens as an Obsidian vault: the game finds entries by their `id` property, so
renaming a file or editing it there is fine; a file it cannot read is listed as `unreadable` (with the reason, in
`notes_list` and F7) and never overwritten. Only the dev changes a proposal's `status` (`new` → `liked`,
`pursuing` or `rejected`); the agent reads every proposal before writing one and never re-proposes an idea.

**The prompt variants** (`content/testbed_content/prompts/`, editable without code; `cargo test -p
testbed_content explorer` checks every placeholder is filled): the prompts are short mission briefs, as a game
master would hand them out. Epoch 1's mission is **survive, learn, explore and map the world**; nobody explains the
world, the tools or the mechanics: the agent finds out by trying and keeps what it learns only in its notebooks.
`start.md` (you wake in the world, the mission, read `AGENTS.md`, then observe) and three resume openings, each
followed by `common.md` (your unit and controller, act only through the game's MCP tools, notebooks only through the
`notes_*` tools, never in files). `agents.md`, the template of the folder's `AGENTS.md`, is a character sheet, not a
manual: the mission, the actor's body, senses and actions, how it reaches the world (unit, controller, the `npc` MCP
server, whose tools describe themselves), its three notebooks (memory doubles as its map and its manual of how
things work), the rules (only `agent/*`, never `world.mutate_*`, `npc/command`, `npc/pause|step` or the speed; no
end) and, for tools without MCP, the `npcctl` path with a pointer to `npcctl agent --help`. It no longer teaches the
one-call loop, the tool fields or the world's geometry:

| Variant | When | The opening says |
|---|---|---|
| `resume.md` | the save fits this build | list memory, diary and proposals (with the dev's statuses) with `notes_list` first, then observe and continue; the save may be up to a minute older than the diary's last lines |
| `resume_changed.md` | **Load anyway** | the world was saved by another version and may have changed: check memory against the senses, write down what differs |
| `resume_rebuilt.md` | **Fresh world, keep the diary** | the world was rebuilt from scratch: trust the senses over the diary |

**A save from another build.** The format version or the content hash (any new registered name) differs: the
menu shows "This world was saved by another version of the game: <error>" with two buttons. **Load anyway**
(`--resume --load-anyway`) loads leniently: what this build does not know is skipped and resources the save
lacks keep their built value; the modal's notes list both. **Fresh world, keep the diary**
(`--resume --fresh-world`) renames the save to `<world>.world.<date>.json` and starts a new world (new seed) in
the same folder, so the diary and proposals stay. An unreadable save offers only Fresh world. Without the menu,
`actor_testbed --resume` on such a save exits with code 3.

**By hand** (what the menu runs; the sim starts paused, so either press OK or, from a script, `npc/resume` over
BRP, [debugging.md §3.3](debugging.md#33-npc-methods)):

```sh
cargo build -p actor_testbed -p npcctl
target/debug/actor_testbed --explorer-home ~/npc-agents --agent "Wren" --world "Hollowmere" [--panels explorer]
target/debug/actor_testbed --explorer-home ~/npc-agents --agent "Wren" --check-save   # {"ok":true} or {"ok":false,"kind":..}
target/debug/actor_testbed --explorer-home ~/npc-agents --agent "Wren" --resume [--load-anyway | --fresh-world]
target/debug/npcctl snapshot check --scenario testbed:explorer ~/npc-agents/wren/hollowmere.world.json
```

(`npcctl snapshot check` without `--scenario` checks against the village and reports a content-hash mismatch.)
`--record` and `--commands` are refused with `--resume`.

**Detached** (spec G7): the same world without a window, hosted by `npcctl serve` (it never renders):

```sh
target/debug/npcctl serve --explorer-home ~/npc-agents --agent "Wren" --world "Hollowmere"
# {"listening":"http://127.0.0.1:15702","port":15702,"agent":"Wren","world":"Hollowmere","prompt":"..\\prompts\\start.md"}
target/debug/npcctl serve --explorer-home ~/npc-agents --agent "Wren" --resume [--load-anyway | --fresh-world]
target/debug/npcctl session stop --url http://127.0.0.1:15702      # or Ctrl+C: save, remove run.json, exit 0
# (on Windows, closing the console window instead loses up to the last 60 s since the autosave)
target/debug/actor_testbed --explorer-home ~/npc-agents --agent "Wren" --resume --exact-port 15702
```

The detached world runs at once (no modal; `--paused` keeps it paused): give the agent `prompts/<kind>.md` from
the listening line. It writes the workspace for its URL and `run.json` (`{pid, port, mode, started, agent,
world}`; the window writes one too, mode `window`) so another menu can find it; a stale one (dead pid) is
ignored. It saves every 60 s and on stop. **Moving it into the window**: stop it (`session stop`: it saves; a
failed save refuses the stop and the world keeps running), then start `actor_testbed ... --resume --exact-port
N` on its port: the window binds exactly `N`, retrying for up to 10 s, and never moves to another port (the
agent's settings pin the URL); still taken, it exits **4** with "port N is still in use". Within the same 10 s
it also waits for the detached process to be gone (its `run.json` removed or its pid exited); one that stays
exits 2, "agent X is already running (pid P, mode M, port N)", as any explorer start does while another live
process runs the world (one owner per world: both would autosave the same save). On this path there is no
prompt modal (the agent is already playing): the sim runs and a notice "moved here from detached (port N)" shows
for 5 s. The agent sees connection errors for the seconds the switch takes, and a new event epoch after it.

**Detached from the menu** (plan Task 8): **Start detached** (New or a Resume row) runs `npcctl serve
--explorer-home DIR --agent NAME [--world W | --resume ..]` in its own console and process group (closing the
menu's terminal or Ctrl+C there does not reach it; the menu's **Quit** stops it, see **Quitting the game** below) and
opens the **Detached** screen: "<agent> in <world> running detached on port N,
tick T" (`session/info` every 2 s, off the frame), the URL, the prompt with **Copy**, **Open in window**, **Stop**
and Back (it keeps running). **Stop** sends `session/stop` and waits up to 15 s for the exit, then Resume says
"<agent> stopped at tick T and was saved."; a refused stop (the save failed) says why and the world keeps running.
**Open in window** does the same stop and wait, then starts `actor_testbed ... --resume --exact-port N`: a refused
stop, or a process still running after 15 s, aborts and keeps it detached; a process that exited with an error, or
wrote no newer save, is restarted detached from its last save on `N`; a window that exits 4 ("port N is still in
use") or 2 (the old host stayed) restarts it detached on `N` and the Detached screen says so. `game_menu --detached
NAME` opens the Detached screen of a running agent directly.

**From the window back to detached**: closing an explorer window asks "Keep it running detached?"; Yes (or F9, no
question) saves, starts `npcctl serve --explorer-home DIR --agent NAME --resume --port N --wait-port` and exits.
`--wait-port` is the handover: npcctl waits up to 10 s for the window to release `N` and exit (its `run.json`), never
another port (exit 4). The window prints `{"detached":{"pid","port","agent","world"}}` on stdout, so a menu that
started it shows the Detached screen next. While the dialog is open the game's keys do nothing. If that `npcctl serve`
cannot take the port within its 10 s (exit 4), nothing reports it: the window has already exited and saved, so the
world simply is not running; resume it from the menu. A detached world the menu started writes its log to the
menu's terminal (there is no log file yet), so the lines of a world left running (Back on a refused stop, below) are
lost once the menu quits. An end-to-end run with its findings: `docs/_notes/explorer-e2e.md`.

**Quitting the game** (plan Task 29, the dev's "quitting the game stops all processes"; it replaces G7's "a detached
world outlives the menu" for the menu's Quit). **Quit** on Main, the menu window's close button, or Ctrl+C on the
menu's console (Bevy's Ctrl+C handler; the menu turns that exit into the same quit) stops every process the game
started, in parallel and off the frame, then exits; a "Quitting..." screen lists each one and its state:

- every detached world the menu tracks (started by it, moved to detached from a window, or adopted from a live
  `run.json` on the Resume screen or with `--detached`): `session/stop` (the world saves first; a world that does not
  answer yet, e.g. one just moved here from a window and still taking its port over, is asked again within 15 s), then
  up to 15 s for its exit; one that saved but still runs then is killed. A world still starting is stopped once it is
  up, or killed (and its `run.json` removed) when it is not up within 15 s;
- the explorer window it started: BRP `app/quit {pid}` ([debugging.md §3.3](debugging.md#33-npc-methods)) on the
  port of its `run.json`: it saves, removes its `run.json` and exits 0, exactly as when it is closed with "No". Once
  it answered it gets up to 10 s more to save and exit; one that does not answer within 15 s, or has not exited by
  then, is killed (and its `run.json` removed);
- the Village, a testbed scenario window or the RTS units demo: nothing to save, so it is killed at once (the menu
  starts them without `--remote`);
- a build or a save check: killed at once.

The menu kills its own children through their process handles. A world it adopted from a `run.json` (a pid, not a
child) is killed by that pid only after checking that the pid still is that world's process: its program is
`npcctl` or `actor_testbed`, and it did not start after the `run.json` was written. A pid the OS has reused for
another process is never killed: the world is forgotten, its stale `run.json` removed, and the Quitting screen says so.

An Open in window or Stop in progress is waited for first (no window opens and nothing restarts after it). A world
that cannot be stopped keeps the menu on the Quitting screen with the world's name and the reason, which says whether
the world refused the stop (its save failed) or does not answer at all: **Quit anyway** kills it (the save stays at
its last autosave, its `run.json` is removed) and quits; **Back** returns to Main and leaves it running (not offered
after Ctrl+C). A second Ctrl+C exits the menu at once and leaves running whatever was not stopped yet, worlds still
starting included. Closing one game window still asks "Keep it running detached?"; that does not quit the game.

The menu never touches a process it did not start or adopt: a world whose `run.json` it never adopted keeps running,
and **`npcctl mcp` servers are left alone**: they belong to the AI tool's session (Claude Code, Codex, ..) that
started them from the agent's `.mcp.json`, and exit when that session closes. Once the world is gone their tools
answer with connection errors until the world runs again.
