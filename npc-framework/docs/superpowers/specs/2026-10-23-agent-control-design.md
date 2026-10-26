# External agent control ("agent-in-the-loop"): design

Date: 2026-10-23 · Status: **approved by user** (scheduled as wave D3)
Builds on the actor-capabilities spec (perception, interaction, command) and the existing tooling (BRP `npc/*`, `npcctl`, journal, replays).

## 1. Intent

The user wants to:
1. start a game;
2. set up the world and its actors;
3. read the game state from the CLI;
4. control actors from the CLI.

The headline experiment is to **prompt an AI agent with the current state, task it with controlling an actor, let it see what that actor sees, and let it do what that actor can do.** The framework provides the *interfaces* for such experiments. Any agent can use them: Claude Code through the CLI, an MCP client, or a script calling the API.

## 2. Goals

1. **Sessions.** A long-running game process can be started headless (`npcctl serve`) or windowed (the testbed/village with `--remote`), loaded from a **scenario file** (world layout + actors, JSON). It is driven over JSON-RPC (BRP) or a CLI client.
2. **Attach.** An external controller attaches to a unit in one of three modes:
   - `Direct`: the unit's BT is suspended and the agent chooses every action.
   - `Advisor`: the BT keeps running and the agent issues orders, like an RTS player.
   - `Observer`: read-only.
   - Several controllers can attach to different units, each identified by a `controller_id` with an owner/permission check.
3. **Observe**, from the actor's point of view: `agent/observe {unit}` returns an egocentric observation.
   - **Self:** position, needs, activity, character sheet, inventory and equipment, current order or action, mounted vehicle.
   - **Perceived world:** only what the actor's sensors currently perceive, plus its perception memory (last seen, where, how). When `npc_perception` is absent, an explicit `omniscient: true` flag is set instead.
   - **Local map:** walkable, blocked and door cells within the actor's known radius.
   - **Time:** tick, day and time of day.
   - **Recent events:** the journal filtered to what this actor experienced.
   - **Affordances:** the list of things this actor can do right now (§3.2).
   - **Formats:** `format: json`, or `text`, a compact prompt-ready narrative ("You are villager #5 … You see: …").
4. **Act.** `agent/act {unit, action}` takes one of the offered affordances, or a parameterized one such as a move to a position.
   - It is validated against the actor's capabilities, then submitted as a `SimCommand`.
   - Every agent-driven session is therefore journaled and **replayable bit for bit**.
   - The response says accepted or rejected (with a reason), the tick it applies at, and an action handle.
5. **Real-time (user decision: no turn-based agent play).** The world keeps running at its normal tick rate while the agent thinks. The actor is affected by whatever happens during that latency, which is part of the experiment. To make this workable:
   - **Action queue:** `agent/act` can enqueue several actions (`queue: true`), so an agent can plan ahead. `agent/cancel` and `agent/clear` exist too.
   - **Idle policy per controller:** `on_idle: Idle | Autopilot`. When the queue runs empty, the actor either idles or falls back to its own BT until the next action arrives. The default is `Idle`.
   - **Events without polling:** `agent/events {since_seq, kinds?, timeout_ms}` is a long-poll that returns as soon as a relevant event happens for this actor (action done or failed, new percept, need level change, attacked later) or when the timeout expires. A `+watch` streaming variant is offered where BRP supports it.
   - **Staleness:** every observation and response carries `tick` and a monotonic `seq`. An action may include `expect_tick` or `expect_seq`; if the world has moved past a configured staleness bound, it is rejected with `stale`, unless the agent opts in.
   - **Latency stats:** each controller records observe→act latency in ticks and ms, exposed via `agent/stats` and the journal. This helps when analysing experiments.
   - **Determinism:** each accepted action is stamped with the tick it applies at, so a real-time agent session still replays bit for bit without the agent.
   - **Debug only:** `npc/pause` and `npc/step` remain available for debugging, but agent experiments run in real time.
6. **World setup and control:**
   - `world/load_scenario`, `world/reset {seed}`, `world/spawn` (via content commands), and `world/state` (the existing snapshot).
   - `world/snapshot` and `world/restore` are out of scope this round (§4); reset is a replay of seed + scenario + commands.
7. **Interfaces:**
   - **JSON-RPC (BRP)** methods `agent/*` and `world/*` on the existing remote plugin.
   - **CLI client:** `npcctl serve` (starts a headless session), `npcctl session info|stop`, `npcctl world …` and `npcctl agent attach|observe|act|cancel|events|stats|detach --url http://127.0.0.1:15702`. It is stateless, so each call is one RPC, and the output is JSON (or `--format text`).
   - **MCP server:** `npcctl mcp --url …` is a stdio MCP server exposing `observe`, `act`, `cancel`, `events`, `attach` and `list_units` as tools, so agent harnesses can connect directly.
8. **Example experiment kit:**
   - A Claude Code skill `/play-actor <unit> <goal>`: an attach → observe → think → act (queue) → events loop, in real time through `npcctl`.
   - A `docs/agent-experiments.md` guide (setup, prompt template, how to replay and analyze an agent's run with the journal and `diverge`).
   - A scripted, non-LLM reference controller in Rust (`examples/greedy_controller`) used by tests.

## 3. Design

### 3.1 Crate `npc_agent_io` (opt-in, generic, headless)
- **Plugin:** `AgentIoPlugin`, adding `ExternalController { id, mode }` on units.
- **Suspending the tree:** a reserved top-priority guard, `"ExternalControl"`, is injected into every tree at freeze time when the plugin is present. In `Direct` mode it runs the external action queue (`ExternalAction` leaf) and suspends lower branches. In `Advisor` mode nothing is injected, because orders flow through the existing `FollowOrder`/`npc_command`.
- **Registries, so capability crates plug in without `npc_agent_io` knowing about them:**
  - `ObservationProvider`: a fn adding a named section to the observation (self, needs, sheet, inventory, perception, map).
  - `AffordanceProvider`: a fn listing affordances. Each one is a verb, a target `SimId` and/or a position, an argument schema, satisfied preconditions, an estimated duration, and a human label.
  - `ActionHandler`: maps an affordance to a `SimCommand`.
- **Village content** registers affordances for walk to, eat, sleep, work and wait. Waves D–F register perception, interaction verbs, orders, mount/dismount and crafting.
- **Text renderer:** deterministic and template-based, with provider-supplied lines.
- **Determinism:** observations are read-only; actions are `SimCommand`s. The controller id and mode are recorded in the replay via an `AttachController` command, so a replay reproduces the same behaviour without the agent.

### 3.2 Affordance contract (JSON)
```json
{"id":"a7","verb":"pick_up","target":12,"label":"Pick up the axe (2 m away)","args":{},"est_ticks":40,"requires":["hands_free"],"ok":true}
```
- Non-`ok` affordances may be listed with the reason they are unavailable (`"why_not":"door is locked"`), which helps agent reasoning.
- `act` accepts `{"affordance":"a7"}`, or an explicit `{"verb":"move_to","pos":[4000,2000]}` validated the same way.

### 3.3 Sessions and scenarios
- **Scenario file:** JSON with a schema `version`, a `content` (village, testbed, …), `world` (grid size, places, blockers, doors, items), `actors` (kind, pos, owner, sheet overrides), and `controllers` to pre-attach.
- **Loading:** a registry of scenario loaders per content crate; the testbed and village provide theirs.
- **`npcctl serve --scenario s.json [--tick-hz N] [--record session.json] [--port N]`** runs headless with the remote plugin and `HeadlessDriverPlugin`.
- **Windowed:** testbed/village `--scenario s.json --remote` runs the same session with visuals.

## 4. Non-goals / anti-goals
- **Non-goals:**
  - shipping an LLM client or API keys in the framework (agents bring their own harness);
  - snapshot/restore of arbitrary state;
  - networking beyond localhost;
  - user accounts or authentication beyond localhost (localhost-only and debug-tools gated). *(Updated 2026-10-24: the session server refuses browser `Origin`, wrong-`Host` and non-JSON requests and takes an optional `--token`; BRP is off in release builds unless `--remote`; see `docs/debugging.md` §3.2 and §4, "Transport security".)*
- **Anti-goal: observations leaking information** the actor could not perceive when perception is on. A test enforces it: an item behind a closed door is absent from `observe` and present in `world/state`.
- **Anti-goal: agent actions bypassing validation, or applying outside tick boundaries.**
- **Anti-goal: an agent-driven session that cannot be replayed.**

## 5. Testing
- **Attach and suspend:**
  - Direct attach suspends the BT: the unit idles until acted on.
  - An act is applied at the next tick.
  - Detaching resumes the BT.
- **Advisor:** an order is followed and the BT resumes after it.
- **Observation correctness:**
  - the perception gate (closed-door test);
  - affordance listing per capability, including `why_not` reasons.
- **Real-time:**
  - Queued actions run back to back while the world keeps ticking.
  - `agent/events` long-poll returns on the action-done event within one frame, and on timeout otherwise.
  - The stale-action rejection works.
  - The `on_idle` policies (Idle vs Autopilot) behave as specified.
- **Replays:**
  - a recorded agent session replays bit for bit without the agent;
  - the `greedy_controller` reaches a goal in a scenario test.
- **Interfaces:**
  - **CLI:** integration tests for `npcctl session`/`world`/`agent` subcommands against an in-process server.
  - **MCP:** a stdio round-trip test (initialize, tools/list, a call to observe).
  - **Fresh-agent test** (wave H): a fresh Claude agent, given only `docs/agent-experiments.md` and the `/play-actor` skill, controls a testbed actor to "get the axe from behind the locked door and chop a tree".

## 6. Sequencing (proposed)
- **Wave D3** (parallel with D1 perception and D2 rules): the `npc_agent_io` foundation.
  - Sessions, scenario loading, attach/observe/act/cancel/events (real time), the registries, text rendering, village affordances.
  - CLI client, MCP server, `greedy_controller`.
- **Waves D–F:** each capability crate registers its observation sections and affordances as part of its own task.
- **Wave G:** the testbed scenario files plus pre-attached-controller scenarios.
- **Wave H:** `docs/agent-experiments.md`, the `/play-actor` skill and the fresh-agent experiment.

## 7. Calls made by the designer (flag if wrong)
- **Pacing:** real time only, by user decision. The action queue, the `on_idle` policy (default `Idle`), long-poll events and staleness guards make it workable.
- **Modes:** three (Direct, Advisor, Observer). Direct suspends the BT through an injected top-priority guard.
- **MCP:** an MCP server is included, as `npcctl mcp`, alongside the CLI.
- **Scenario format:** JSON for now; RON can come later.
