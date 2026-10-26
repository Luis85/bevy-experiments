# Agent registration: id, token, entering the world: design

Date: 2026-10-26. Status: approved by the dev in chat ("proceed"). Changes the agent interface of
`2026-10-25-agent-notes-and-workspace-design.md` (G5/G6/G13: AGENTS.md, prompts, MCP tools) and is
implemented as part of its plan (Phase I).

## 1. Intent

Today an AI agent learns too much up front: AGENTS.md names its unit number, controller, URL and the
`npcctl` command lines, and its actor is already attached to it. The dev wants the agent to start as
an outsider that must **register at the server** with an id and a token, then **enter the world**,
learn which character it has been assigned, and **take control** of that actor itself. The token is a
real lock: without it the game answers no agent tool, and no agent can take over another agent's
actor. The start and resume prompts become a **dungeon master's briefing**; from there the agent
writes its own story and does what the environment provides.

## 2. Goals

- R1. **Identity.** Creating an agent generates an `agent_id` (stable, e.g. `wren-ashdown-7f3k`) and a
  random secret `token` (≥ 128 bits, from the OS RNG — host-side, never sim). Both are stored in
  `agent.json` (owned) and written into the generated `AGENTS.md`. They persist across resumes and
  mode switches. The menu (and `npcctl agent-token rotate`) can issue a new token, which revokes the
  old one at once (a live session using it is closed).
- R2. **The lock.** Every agent method — `agent/observe`, `act`, `act_and_wait`, `events`, `cancel`,
  `attach`, notes, the map tools, `list_units` — is refused with reason `not_registered` unless the call
  carries a valid **session key**. `agent/register {agent_id, token}` checks the token (constant-time
  compare) and returns `{session, agent_id, world}`; a wrong id/token is refused `bad_credentials`, and
  repeated failures from the same client are slowed (≥ 1 s between failed attempts per agent id).
  One live session per agent: registering again replaces the old session (a crash or restart never
  locks the agent out). Sessions live in host memory only (not saved; a resumed or moved world
  requires registering again — the MCP server does it transparently, R5).
- R3. **Entering the world.** `agent/enter_world {session}` returns the assigned character: its name,
  body, senses and what it can do (from the actor preset), plus a short scene line from the world —
  but not its unit number until it takes control. The actor stays idle and **unattached** until
  `agent/take_control {session}` attaches it (Direct mode) to this agent's session. An agent can only
  ever take control of the actor assigned to it in `agent.json`; another agent's or an unassigned
  unit is refused `not_yours`. `agent/release {session}` hands it back (idle).
- R4. **Session-bound tools.** After taking control, tools act on "my actor": the MCP tools drop the
  `unit` and `controller` arguments (the session implies them). Observations still name things by
  `#ID`, so the agent can refer to what it senses.
- R5. **MCP is the only door.** The MCP server (`npcctl mcp`) exposes `register`, `enter_world`,
  `take_control`, `release` and the session-bound tools. It keeps the session key after `register` and
  attaches it to every later call; when the game restarts or the world moves (window ↔ detached) and a
  call is refused `not_registered`/`unknown_session`, it re-registers with the credentials the agent
  gave and retries once.
- R6. **What the agent is told.** `AGENTS.md` names the agent id and token, how to register, enter and
  take control, its notebooks and the rules — and only the MCP tools. No `npcctl` command lines, no
  unit number, no controller name, no URL (it lives in `.mcp.json`). The start and resume prompts are
  a **dungeon master's briefing** in the spirit of the dev's Epoch 1 mission brief: who you are
  arriving as, that you must register and enter, take control, survive, learn, explore, write your
  story in your notebooks.
- R7. **The dev's tools still work.** The dev's methods (`npc/*`, `world/*`, player notes, the F7
  editor, `app/quit`) need no token; the agent cannot reach them (its Claude Code settings are MCP-only
  with no shell). `npcctl agent …` gains `--agent-id`/`--token` for the dev's own testing (it registers
  first); it is documented in `docs/debugging.md`, never in AGENTS.md.

## 3. Non-goals

- No network exposure: the server stays on 127.0.0.1. The token protects against other local tools
  and agents, not a remote attacker.
- No encryption, no user accounts, no token expiry by time.
- No change to sim state or hashes: registration and sessions are host-side; `take_control` is the
  existing attach command (recorded like today).

## 4. Anti-goals

- An agent controls an actor without a valid token, or another agent's actor.
- A restart, a resume or "Open in window" locks the agent out or makes it re-learn its unit number.
- AGENTS.md or a prompt leaks `npcctl`, a unit number or a controller name.
- A token appears in logs, journals, the save or observation text.

## 5. Calls I made (check while reading)

- Id and token stay the same across resumes; the menu can rotate the token.
- Sessions live in memory; the MCP server re-registers transparently after a restart or a move.
- The actor is unattached until `take_control`; `enter_world` gives the character without its unit number.
- Tools become session-bound (no `unit`/`controller` arguments in MCP).
- Dev methods stay token-free; `npcctl agent` gets `--agent-id/--token` for the dev only.
- Failed registrations are slowed to ≥ 1 s apart per agent id.

## 6. Builder answers (from the builder check; my calls)

1. **Opt-in lock.** The lock is generic in `npc_agent_io` but switched on per world: a
   `RegistrationRequired` resource (absent = today's behaviour). The explorer host turns it on for
   explorer worlds (window and detached); the village, the plain testbed scenarios and
   `npcctl serve --scenario` stay open, so their tests and docs don't change.
2. **Registry.** A generic `AgentCredentials` resource in `npc_agent_io` maps `agent_id → { token
   digest, assigned SimId, controller name }`; the explorer host fills it from `agent.json` after the
   world is built or loaded (the assigned unit is the explorer actor, resolved by name — `agent.json`
   stores no unit number, so it survives save/load and moves between processes). One agent per explorer
   world today; the registry allows several.
3. **Controller.** The controller name is the `agent_id`. `take_control` is the existing attach command
   (recorded) under that controller, Direct mode. On build and on load the explorer host detaches the
   actor, so it is always unattached until `take_control`; the agent never needs its unit number.
4. **Session key** travels as a `session` field in the params of every agent call (BRP and the
   session server alike; long-poll and nonce keys include it unchanged). Closing a session (rotation,
   re-register) answers its parked long-polls with `unknown_session`.
5. **`enter_world`** returns the same character text the workspace renders today from the actor preset
   (`ActorSummary`: name, body, description, senses, actions) — which therefore moves out of
   `AGENTS.md` into the world — plus a host-composed scene line ("You arrive in <world>."). The agent's
   own `#ID` may appear in observations after `take_control`; only `enter_world` withholds it.
6. **Gating.** Before `register`: everything refused. After `register`: notes (keyed by the session's
   agent folder) and `enter_world`/`take_control` are allowed. After `take_control`: observe, act,
   act_and_wait, events, cancel, release. `list_units` is dropped from MCP; `agent/list_units` under the
   lock returns only the agent's own actor.
7. **Credentials in MCP.** The agent passes `agent_id` and `token` to the `register` tool (read from
   AGENTS.md); the MCP server keeps them in memory for its process lifetime to re-register, and never
   logs or echoes them.
8. **Rotation.** The menu rewrites `agent.json` and `AGENTS.md`. The host re-reads `agent.json` when
   its mtime changed, on every agent call, and closes sessions whose token no longer matches — so a
   rotated token stops working on the next call, without restarting.
9. **Throttle.** A failed `register` is refused with `retry_after_ms` (≥ 1000 per agent id); nothing
   blocks a frame.
10. **Tests and texts.** The prompt/AGENTS.md tests are rewritten for the new texts. The dungeon-master
    briefing builds on the dev's Epoch 1 brief already in `content/testbed_content/prompts/`
    (`agents.md`, `start.md`, `resume*.md`).
