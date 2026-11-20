# Unported agent-supervision work from the first-playtest branch

Recorded on 2026-11-20 while auditing the leftover branch
`audit/island-first-playtest-verified` before its deletion. Its first commit
(`63417af`, playtest tools and feedback) reached main in `a3d6fdb`. Two later
commits never did. Both change generic agent tooling (`npc_agent_io`, `npcctl`),
not the island simulation, and both were verified on their own base
(`cargo xtask verify` `ok:true`, 1,348 and 1,352 nextest passes; see the
[followup](2026-10-29-island-first-playtest-followup.md) and
[CLI handoff](2026-10-29-cli-registration-handoff.md) notes). They were not
ported because main's scheduler has since diverged (about 600 changed lines in
`scheduler.rs` and a separate `npcctl` AI client), so each is a re-implementation,
not a patch. The patches are kept outside the repository with the audit report
(`npc-out-island-archive/leftover-branch-patches/`).

Main's behaviour today: `npc_agent_io::scheduler` stops a timed-out or unlinked
continuation with `taskkill /PID <root> /T /F` on Windows (and `Child::kill`
elsewhere), and checks client liveness through `tasklist`/`ps`. An operator links
a conversation only by typing its exact UUID into `npcctl scheduler bind
--session-id`. `npcctl`'s separate AI game client (`ai_client/job.rs`) already
places its own provider children in a kill-on-close Job Object.

## 1. Owned continuation containment (`dc303f7`)

The problem it fixed (P1 in the followup review): cleanup depended on the root
process, so descendants of a finished CLI could outlive it and its session lock;
a failing `tasklist` could be read as a dead client and let a lock be cleared.

- New `npc_agent_io/src/scheduler/process.rs` (469 lines). On Windows a
  continuation starts suspended, joins a non-inheritable kill-on-close Job Object,
  then resumes. Exit is accepted only after job membership, retained handles and
  zero active job processes are verified. Unix uses a new owned process group;
  uncertain cleanup keeps the target lock.
- Liveness becomes tri-state from native read-only handles; "unknown" means wait,
  never dispatch or clear a lock. No existing process is adopted.
- Python counterparts `scripts/playtest/owned_process.py`, `harness_runtime.py`
  and their `test_*.py` contracts served the removed `live_smoke.py`.

To port: move main's continuation spawn/stop (`scheduler.rs`, the `taskkill`
stop and `process_alive`) behind a process module modelled on the branch file,
reusing the Job Object FFI already in `npcctl/src/ai_client/job.rs` if it moves
into `npc_agent_io`. Port the fake root/descendant/sentinel tests and the
unknown-liveness and retained-lock regressions. Cost: one focused change to
`npc_agent_io` plus its tests and a full verify; no oracle is involved.

## 2. Private CLI identity at registration (`bf48345`)

- `agent/register` accepts paired optional `cli_provider` (`codex`/`claude`) and
  `cli_session_id` (an exact UUID). The pair is private host memory on the live
  registration: never hashed, saved, journalled, logged, echoed or shown in Debug,
  and dropped on revocation or re-registration. With supervision enabled, the
  reply asks for the pair when it is missing; old clients keep working.
- New operator RPC `scheduler/bind_registered` and
  `npcctl scheduler bind --from-registration` link that offer without the operator
  copying the UUID. It requires settled Direct control of the actor, an explicit
  workspace and an original client PID or `--turn-finished`. A changed offered
  identity pauses the old link ("CLI session changed; rebind from registration").
- `RegisteredClient::register_with_cli_session` keeps the pair for a transparent
  reconnect and redacts it from results and errors; `npcctl --cli-provider
  --cli-session-id` pass it. The generated explorer handbook asks for it.

To port: add `cli` to main's `LiveSession` (`registration.rs`), the parse and
lookup helpers, `bind_registered` in `scheduler.rs` (extract `valid_session_id`),
the method in `api::METHODS` (now 32 entries, checked by the registration and
notes tests), the MCP client and `npcctl` flags, the handbook line in
`testbed_content/prompts/agents.md` and the explorer workspace, plus the 116-line
registration test and two `npcctl` scheduler tests. Then document it in ADR 0023
and `docs/debugging.md` (the branch's ADR 0023 "2026-10-29 lifecycle hardening"
section is the text to adapt). About 450 lines of code; no oracle is involved.
