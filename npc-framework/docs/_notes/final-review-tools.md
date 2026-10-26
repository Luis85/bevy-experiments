# Final review: tooling, agent interface, apps, CI, docs (2026-10-24)

## Security
**S1 (High): localhost RPC servers have no Origin/Host/Content-Type checks.**
- **Where:** the agent_io std http server and bevy_remote.
- **Attack:** a CSRF request from a browser page (no-cors) or DNS rebinding.
- **Risky methods:** some take arbitrary paths:
  - `npc/record/stop {path}` (writes and creates directories)
  - `npc/screenshot {path}`
  - `world/load_scenario {path}` (reads, and so works as an existence oracle)
- **Fix:**
  - Reject requests that carry an Origin header.
  - Require `Content-Type: application/json`.
  - Require Host to be `127.0.0.1:<port>` or `localhost:<port>`.
  - Optionally require a token.
- **Path rules:** relative paths under one output root only, no `..`, absolute, `\\?\`, `:` or trailing dot/space, a forced extension, and no overwrite by default.
- **Also:** remove `path` from `world/load_scenario` (the CLI sends the document inline).

**S2 (Med): hooks only load when Claude Code is launched in `npc-framework/`.** Document this prominently. Don't add hooks to the repo root: the root is the QMS vault, and other projects share it.

**S3 (Med): `protect_goldens.sh` bypasses.**
- **Path forms that get through:**
  - a trailing dot or space on a path component
  - 8.3 short names
  - the `\\?\` prefix
- **Commands that get through:**
  - `git checkout|restore -- tests/replays`
  - PowerShell `ri`/`cpi`/`mi`/`rni`
  - `[IO.File]::Write*`
  - `python -c open(..,'w')`
  - `cd bench; rm baseline.json`
- **False positive:** copying *from* a protected path is blocked. Match only the destination.
- **Also protect:** `xtask/src/main.rs` and the CI workflow (the workflow is outside the hook root, so it needs a pattern on the `../.github` path).

**S4 (Low): HTTP server resource limits.**
- Header lines are unbounded and can be dripped slowly.
- One thread per connection, uncapped.
- Up to a 16 MiB body allocation before any bytes arrive.
- Batch size is unlimited, and an empty batch returns `[]`.

**S5 (Low): release builds expose BRP by default.** `debug-tools` is a default feature and the testbed has `remote:true`. Gate it on `debug_assertions`, or require opt-in in release builds.

## Correctness
**C1 (Med-High): `agent/act` can be accepted, then dropped at apply time.**
- **Cause:** the queue-cap check ignores AgentActs already queued for the same tick, and force-attach/detach can land in the same tick.
- **Symptom:** the handle never emits an event.
- **Fix:** journal `ExternalAction{rejected}` or `ActionRejected` on apply-time skips, and count same-tick queued acts in `validate`.

**C2 (Med): seq and handles restart after `world/reset` / `world/load_scenario`.**
- `since_seq > last` returns nothing, silently.
- **Fix:** an `epoch` field, plus an error on `since_seq > last`, or carry `next_seq`/`next_handle` across the reset.
- **Also:** no gap signal when the buffer or ring evicts entries. Add `oldest_seq`/`dropped`.

**C3 (Med): `npc/step` on the session host blocks the loop** (up to 1M ticks). Run it in chunks across loop iterations, or cap it (e.g. 20k).

**C4 (Low): MCP `events` sends no nonce**, and `+watch` streams are keyed by params, so subscribers with identical params share state.

**C5 (Low): `ActionPlan::Commands` can be applied partially.** Validate every command first, then submit.

**C6 (Low): the session loop rescans the event buffer about every 1 ms.** Use `partition_point`.

## CI
The workflow should run as written. Gaps:
- **Duplicate work:** the verify job duplicates lint, test and deny.
- **Clippy coverage:** no clippy on non-default feature combinations.
- **No MSRV job.**
- **Unprotected:** the workflow file itself is not protected.

## Docs mismatches
1. **agent-experiments §3 village example:** the pre-attached controller controls unit 6, not 5. The `--unit 5 --controller me` examples need an attach first.
2. **agent-experiments §6:** the transport error message doesn't name the URL.
3. **`remote.rs:12` comment:** claims MCP sends a nonce; only the CLI does.
4. **agent-experiments §4:** the stale example's quoted message tick doesn't match the command.
5. **agent-control spec §2.7:** says `session start`, but the real commands are `serve` and `session info|stop`.
6. **tooling spec:** says npcctl `debug-tools` is on by default, but it's an empty feature.
7. **play-actor skill journal recipe:** assumes `testbed:SCENARIO`; file and village scenarios break.
8. **xtask skip text:** still says "comes with Task P2".

## Cleanups
1. **Duplicate screenshot and remote setup** in the village and actor_testbed apps. Move it to `npc_view_helpers`, which is also where the path rules go.
2. **actor_testbed `attach_controllers`** duplicates the agent_io scenario code. Expose the agent_io version and use it.
3. **Scenario lookup order differs** between npcctl and actor_testbed. Unify it.
4. **xtask:** fix the stale "Task P2" text, and treat an empty corpus as a failure in full verify.
5. **Dead flags:** remove npcctl's `debug-tools = []` feature and actor_testbed's no-op `--remote` flag.
6. **Leftovers:**
   - `build.log`
   - the `_notes` stubs
   - `scripts/p3/capture.sh`, which is village-only and breaks on the testbed corpus
   - `scripts/c2`
   - `corpus_equivalence.rs`, which only runs when `replays.next` exists
7. **Performance:**
   - `observe` clones the whole AgentRegistry on every call.
   - The stats ring uses `Vec::remove(0)`; use a VecDeque.
8. **Wrongly typed args are silently defaulted** (e.g. `"ticks":"50"`). Return `invalid_args`.
