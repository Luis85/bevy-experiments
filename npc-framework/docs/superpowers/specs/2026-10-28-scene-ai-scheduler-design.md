# Scene AI scheduler

The user starts a game and connects an external AI. When the AI stops issuing actions and its character
stays idle, the scheduler nudges that AI to continue playing. The user chose **resuming a configured AI
CLI session**, rather than creating a new chat or sending an app automation reminder.

## Contract

- Scene policy configures enablement, real-time idle threshold, polling, cooldown, hourly nudge cap and
  runner timeout. These are host policy, never simulation clocks or authoritative decisions.
- Operator-owned bindings map the game agent/controller identity to a resumable CLI provider/session and
  working directory. The game cannot infer which private AI conversation belongs to a game connection;
  link it once. Do not guess a session, use global `--last`, or let game/diary text choose an executable.
- Watch meaningful agent request/action progress and actual actor work, including queues, movement,
  gathering, crafting and sleep. Pause, lost/revoked control, permanent death, changed actor identity,
  missing binding and stale host state suppress nudges or require a fresh link as appropriate.
- A nudge resumes the same configured conversation with a brief continuation prompt: observe the actor,
  check needs/work, continue the prior goal, and report blockers. Do not inject credentials into prompts,
  process arguments, status or logs. Existing game connection instructions remain in the agent workspace.
- Resume turns run as bounded noninteractive background CLI processes. Track owned child handles and
  prevent concurrent nudges to one agent; apply global concurrency/rate caps and failure backoff. An
  optional known client-process guard avoids starting over an existing CLI run. Interactive clients
  need an explicit idle signal/finished turn; actor idleness alone cannot prove a model is not thinking.
- Use structured argv and prompt stdin. Verify provider flags from installed CLI help/official docs.
  Show useful unlinked/idle/busy/running/cooldown/error states. Disabling prevents new nudges; host shutdown
  cleans up only scheduler-owned children. Never terminate an original client or another chat's process.
- Shared generic host implementation belongs in npc_agent_io, with scene policy and host integration in
  content/apps. Keep content names out of core. No scheduler types join sim hashes/registry manifests.
- Keep the current village work in scope. Test scheduler timing with an injected monotonic time value,
  fake runner processes, concurrency, failure/timeout, pause/busy/death/control changes, target isolation,
  and unchanged simulation hashes. Do not invoke real AI sessions during implementation/verification.
