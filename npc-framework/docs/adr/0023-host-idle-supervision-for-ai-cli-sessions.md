# ADR 0023: Host idle supervision for resumable AI CLI sessions

- Status: Accepted
- Date: 2026-10-28

The user wants a connected game-playing AI to continue after it stops: observe a real-time idle period,
then nudge that same external AI. The chosen delivery is a configured resumable CLI session (Codex or
Claude), not a new chat, autonomous actor decisions, or a Codex app automation.

Implement generic host supervision in npc_agent_io, outside NpcSimTick and all sim registries/hashes.
Scene policy controls enablement, idle/cooldown/poll intervals, hourly/concurrency caps and owned-runner
timeout. Authorized agent request/observation/action counters, actor movement and busy predicates
distinguish inactivity from gathering, crafting, sleeping and queued work. Pausing, revoked control,
observer mode, death and a changed actor suppress dispatch. Host status queries are read-only.

An operator links a controller/actor to an exact previous CLI conversation UUID and trusted workspace.
Programs/session IDs live in an operator-owned bindings file, not a replay/world descriptor or notebook.
Do not infer a private conversation, use `--last`, or accept an executable from game/diary content.
Require the original one-shot client PID or the operator's declaration that its turn has finished.
The PID is only a liveness guard; it is never terminated. Interactive clients need an explicit idle
signal rather than assuming that a quiet actor proves an idle model.

Use structured argv and prompt stdin, preserve model/permission configuration, and hide background
console windows on Windows. Installed help verified Codex 0.145.0 `exec resume UUID -` and Claude Code
2.1.281 `--print --resume UUID`. Codex's repository check is skipped for the generated non-repository
game workspace. No sandbox/approval bypass is injected. Provider output is discarded, avoiding copied
credentials/transcripts in game logs/status. Only explicitly owned continuation processes are reaped,
timed out or cleaned up, with a target lock and cooldown/backoff/rate/concurrency limits.

The host polls independently of simulation ticks while the game continues its normal loop; default Bevy
game settings also update unfocused windows. Core timing/planning tests use injected elapsed values;
process tests use an owned fake CLI and verify prompt delivery, no overlap, timeout and lifecycle cleanup.
No real AI conversation is invoked during verification. Agent actions chosen by a resumed CLI still
enter through ordinary recorded game commands, so replay does not run a supervisor or AI process.
