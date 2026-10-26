---
status: "accepted"
date: 2026-10-23
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: architecture-best-practices.md N19/C12; workflow-and-verification.md §4.10; agent-dev-environment.md #5; baseline-recommendations.md A10, B10"
---

# Debug actions are commands; BRP reaches the sim only through `fl.*`

## Context and Problem Statement

Revision 1's testbed (§11) had a spawn panel, "invulnerable" and "reveal all" toggles, and BRP methods such as
`fl/spawn`. As specified, they mutate the sim from the client. Testbed sessions then can't be replayed, and it breaks
the anti-goal "input code changes unit state directly instead of issuing commands". BRP's built-in `world.*` methods
act on the client's main World; wiring `world.mutate_*` to sim state would bypass the command stream. Window
screenshots come back black when the game window is hidden behind another app, which is the usual case when an agent
drives it from the Claude desktop app.

## Decision Drivers

* Every sim state change is a recorded command, so testbed sessions replay exactly.
* Agents can drive and inspect the live game without a human.
* Cheats must be impossible in a normal or multiplayer match unless every peer agreed.
* Agent screenshots must work regardless of window focus.

## Considered Options

* Client-side debug mutation of the sim World (revision 1)
* Debug actions as `debug.*` command kinds gated by a header flag; driver controls for time; client render overrides
  for view-only toggles
* A separate privileged debug channel into the runner

## Decision Outcome

Chosen option: "Debug actions as `debug.*` command kinds, driver controls for time, render overrides for view toggles".

- **Debug commands:** `debug.spawn`, `debug.set_owner`, `debug.invulnerable`, `debug.set_hp`, `debug.teleport`,
  `debug.wander`, and later ones in the same namespace. Ordinary command kinds, marked debug-only:
  - accepted only when `MatchHeader.cheats_allowed` is true, otherwise rejected and logged
  - may target any player's units
  - recorded in replays like any command; lockstep peers agree on the flag because it is in the header
- **Driver controls, not commands:** pause, resume, single step, speed. They change *when* ticks run, never what a tick
  does, so replays are unaffected. Orders given while paused are stamped for the next tick. A lockstep pause later is a
  session-level message.
- **Client-only:** the local-player switcher; "reveal all", a render override that draws every snapshot entity
  regardless of the local team's visibility bits.
- **Reads:** inspector, overlays and BRP reads use `SimQuery` against the runner (ADR-0002).
- **BRP:**
  - Built-in `world.*` methods act on the client World only; `world.mutate_*` is never wired to sim state.
  - The sim is reachable only through custom `fl.*` methods (dot names, matching Bevy 0.20's `world.*` and `rpc.*`).
  - Writes go through `fl.issue_command`, `fl.spawn` (which issues `debug.spawn`) or the driver.
  - The methods are published as agent tools through `bevy_brp_extras`.
  - Each agent uses its own BRP port.
- **`fl.screenshot`** renders from a dedicated offscreen camera into an `Image` target and captures it with
  `Screenshot::image`. Agent and bench launch modes set the window's unfocused update mode to continuous.

### Consequences

* Good, because testbed sessions, including cheats, are replayable and bisectable.
* Good, because agents use the same command path as players, and their sessions are recorded.
* Bad, because a debug spawn appears at the end of the tick (the spawn funnel), not instantly.
* Bad, because every new testbed action that changes state needs a command kind and a handler.
* Neutral, because the offscreen camera costs one extra render target, only in debug builds.

### Confirmation

* A test: a `debug.*` command is rejected without `cheats_allowed` and accepted with it; a replay of a session with
  debug commands verifies.
* The BRP integration test drives `fl.step`, `fl.sim_state` and `fl.issue_command` over HTTP.
* Review: no client code path calls into the sim World except through the runner protocol (`fl_client` has no
  `fl_sim` dependency, ADR-0002).

## Pros and Cons of the Options

### Client-side mutation

* Good, because it is quick to build.
* Bad, because it is unreplayable, breaks an anti-goal, and is impossible anyway with a threaded runner.

### Commands, driver controls and render overrides

* Good, because it is replayable, uniform and safe in multiplayer.
* Bad, because of a command kind per action.

### Privileged debug channel

* Good, because it is flexible.
* Bad, because it is unrecorded state changes again, just moved.

## More Information

* Spec §5.5 (debug commands, testbed), §11, §15 C.
* Research: `architecture-best-practices.md` N19, C12; `workflow-and-verification.md` §4.10;
  `agent-dev-environment.md` #5 (port collisions, black screenshots, unfocused throttling).
* Related: ADR-0002, ADR-0007, ADR-0013.
