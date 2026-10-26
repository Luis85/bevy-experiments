# ADR 0010: Agent control runs over a std JSON-RPC host and BRP; events long-poll never blocks the loop

- Status: Accepted (implemented in actor capabilities wave D3, `npc_agent_io`)
- Date: 2026-10-23
- Source: `docs/superpowers/specs/2026-10-23-agent-control-design.md` §2.5, §2.7, §3.1; ADRs 0006, 0008, 0009

## Context

External agents control units in real time: the world keeps ticking while the agent thinks, and an agent
waits for "my action is done" without polling (`agent/events {since_seq, timeout_ms}`). Two hosts need it:
the headless `npcctl serve`, and windowed games that already run Bevy's BRP (`npc_tools` feature `remote`).

Three constraints shaped the design:

1. `xtask/tests/arch_fitness.rs` forbids `bevy`, `bevy_remote` and the renderer anywhere in `npcctl`'s
   build ("headless CLI: must build and run without a GPU or window"), and `deny.toml` lets only the
   presentation crates, `npc_tools` and `village` depend on the `bevy` umbrella. So `npcctl serve`
   cannot host Bevy's BRP server.
2. A BRP method handler runs inside the game's frame (`RemoteLast`, exclusive world access). A handler
   that waited for an event would stall the game loop, and with it the very tick that produces the event.
3. Agent sessions must replay bit for bit without the agent (ADR 0006: only `SimCommand`s change the sim).

## Decision

- **Methods are transport-agnostic** (`npc_agent_io::api`): plain functions over `&mut World` with JSON
  params, shared by both hosts. Every state change is a registered content command (`AttachController`,
  `DetachController`, `AgentAct`, `AgentCancel`, `AgentClear`) submitted with `submit`, so it is
  validated, journaled, stamped with its apply tick and recorded in the `CommandLog`.
- **Headless host: a std-only JSON-RPC 2.0 over HTTP/1.1 server** (`npc_agent_io::http`, `session`), using
  the BRP wire format (`POST /`, single or batch requests), so the same clients (CLI, MCP, scripts) talk to
  both hosts. A listener thread spawns one thread per connection; each call is handed to the session loop
  over a channel, and the *connection thread* blocks until the loop replies. The loop drains the channel
  with `try_recv` (never blocking), answers instant methods at once, and parks `agent/events` long-polls
  in a list that is re-checked after every frame and every ~1 ms; a poll is answered when a matching event
  is buffered or its deadline passes. A frame runs every `1 / tick_hz` (headless driver, `multiplier`
  ticks), so a done event reaches its poll in the same loop iteration as the tick that produced it.
- **BRP host: watching methods** (`npc_agent_io::remote`, feature `remote`). `bevy_remote` 0.20 decides
  streaming by the method *name* (`+watch` → server-sent events) and runs *watching* handlers once per
  frame until they return `Some`. So `agent/events` is registered as a watching method under a plain
  name: the HTTP request stays open (a long-poll) while the handler does one cheap, non-blocking check
  per frame; the deadline is kept server-side, keyed by the request params (`nonce` distinguishes
  identical concurrent polls; the CLI adds one). `agent/events+watch` streams. `world/load_scenario` and
  `world/reset` rebuild the app, which only the session host can do; on BRP they return `-23601`.
- **Events come from the journal** (`AgentIoPlugin` turns it on by default): harvested into a non-hashed
  buffer with a monotonic `seq`, used by observations, staleness checks and `agent/events`.
- **Direct control is a tree branch, not a special case in `npc_ai`**: a kernel tree transform
  (`TreeTransforms`, applied before the freeze) wraps every tree's root in `ExternalControlRoot` with a
  top-priority `Guard "ExternalControl"` → `Leaf "ExternalAction"`. The leaf runs queued steps that name
  registered actions, so an agent-driven unit uses the same actions as an autonomous one.

## Consequences

- `npcctl` stays headless and links no Bevy umbrella; the game keeps one BRP server for everything.
- Neither host's game loop ever waits for a client; a slow or vanished client costs one parked
  connection thread (session) or one per-frame check (BRP) until its timeout.
- The session host serves only a subset of `npc/*` (pause, resume, speed, step, state, unit, command,
  journal); profiler and recording methods stay BRP-only.
- `ActionRejected` journal entries are written at request time (tooling-originated), so a replay of a
  session does not contain them; everything that changes the sim does replay.
- Adding `AgentIoPlugin` adds names and a branch to every tree, so it changes the content hash: replays
  recorded with it verify only with it. Without it, nothing changes (the village oracles are untouched).

## Amendment (2026-10-24, final review)

- **The session server checks every request** before reading its body: no `Origin` header, a `Host` of
  `127.0.0.1|localhost|[::1]:<port>`, `Content-Type: application/json`, an optional bearer token (`npcctl serve
  --token`), and resource limits (64 KiB headers, 10 s per request, 64 connections, 1 MiB bodies, 64 calls per
  batch). A browser page on the same machine can reach any localhost port (CSRF, DNS rebinding); these checks
  make the session unusable from one. `world/load_scenario` takes the document inline only: the server never
  opens a file a client names.
- **BRP cannot do the same**: `bevy_remote` 0.20 has no request hook (`with_headers` only sets response
  headers). Accepted residual risk for a development tool: BRP is off by default in release builds (`--remote`),
  listens on `127.0.0.1` only, and every client file path (`npc/record/stop`, `npc/screenshot`) is confined under
  an output root (`npc_tools::out_path`). Replacing `RemoteHttpPlugin` with a checked transport feeding
  `BrpSender` is possible (it is public) and the way to close this if BRP ever leaves development use.
- **Long `npc/step` on the session host runs in slices** (50 ticks, ~5 ms per loop iteration) so the loop keeps
  serving requests and long-polls; the call is answered when done.
- **Watching-method state is keyed by params plus position in the frame** (bevy_remote passes no request id);
  clients still send a unique `nonce` (CLI per process, MCP per call).
