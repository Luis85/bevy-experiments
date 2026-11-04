# Herp's Island Village stream

The next NPC/environment increment adds resident life trees, shared rest/recovery
actions and resource/production inspection. Use a **fresh Island Village** with this
build because its island content hash changes. See [the shared action guide](npc-life-and-environment.md).

Herp is the visiting AI hero. Nine residents gather resources, make goods, trade,
eat, drink and rest while Herp tries to make a living among them. The stream
follows those decisions and their consequences. Food, stock, money and lives are
finite; the outcome is allowed to be surprising.

This is the stream operator's companion to
[the playtest runbook](island-village-playtest.md). Read that guide for provider
trust/setup, exact-session linking, rehearsals and the readiness decision. This
increment passed its bounded authenticated Codex smoke and full verification
gate; see [the execution evidence](_notes/2026-10-29-herp-stream-closeout.md).
The two-turn engineering fixture is distinct from an unaided full-length stream.

## Prepare once, before going live

Build inside `npc-framework/` with the repository's pinned toolchain:

```powershell
cargo build --locked --release -p actor_testbed -p npcctl --bins
```

Use a new agents folder reserved for this stream. The following creates a native
Island Village with a human-bodied actor named **Herp**:

```powershell
target/release/actor_testbed.exe --explorer-home C:/NPC/herp-stream --agent Herp --world Island --preset human --world-preset island_village --world-state-out ../target/world-state/herp.json --world-state-interval-ms 500
```

Explorer New starts paused with **Your AI agent**. With a logged-in CLI installed,
choose **Start Codex** or **Start Claude**. The game resumes the world and starts
the dedicated client, which supplies the generated mission/tools and manages
later turns. The Start notice explains that the agent may act in this world and
use its private Notebook. Start Codex grants only this registered game server's
16 tools in the owned CLI process; conflicting explicit NPC approval preferences
stop setup. Global provider, shell and sandbox settings stay unchanged. A
cancelled game tool stops the client for operator attention instead of retrying.
**F10** opens the panel again; **Stop agent** stops that client while
the village remains a separate process. **Return to world** preserves the current
pause state. Keep the game at **1x** for the stream. Herp registers, enters and
requests control through the generated `npc` MCP server; a pending claim needs a
running host. Herp must never resume, step or change the world's speed.

For the pixel-art view of this **same** running world, run from the sibling
`npc-world-viewer/` directory:

```powershell
npm ci
npm run dev -- --state ../target/world-state/herp.json
```

Open the loopback URL printed by the viewer. The native window remains the game
host; the browser is a read-only second camera. Do not start a second headless
host for the same save or output file. Both views show public speech and action
bubbles; their cameras and selection are independent. The exported JSON omits
private notebooks and credentials. A missing/stale source retains the last valid
browser frame and reports its status.

The operator chooses and signs in to the provider and agrees its usage/time
limit before launch. The [dedicated client](ai-agent-game-client.md) manages the
exact provider session privately; normal setup needs no copied prompt or manual
conversation ID. Its bounded terminal mode supports `--max-turns` and
`--max-runtime-seconds`. Manual prompts and `scheduler bind/check` remain advanced
compatibility diagnostics, using the actual Herp folder and host URL.

Keep setup off the broadcast: the generated handbook, credentials, provider
conversation ID, MCP configuration and desktop notifications are private. Capture
the game window. Review the full broadcast composition before starting.

## Brief Herp

The generated mission supplies the survival briefing automatically. This is the
public creative direction for the playtest and operator's introduction:

> You are Herp, a newcomer to Island Village. Earn a living, look after your
> hunger, thirst and energy, and learn how your neighbors live. Choose your own
> useful work and relationships through the game tools. Keep brief, factual Diary
> entries about what you tried, what happened and your next intention. Let the
> village's responses change your plans. Your life is permanent; a later life
> continues in the same world with fresh private books. Observe and act only
> through the game tools. Your turn ending does not pause the village.

Do not give Herp entity IDs, undiscovered coordinates, private merchant balances
or a prescribed optimal route. Record any later operational hint as an
intervention. Use F7 to show suitable Diary entries; review what is on screen
before opening other notebooks. Only Diary pages become public in the world
after death; Memory and Proposals stay private.

## Follow the story

Island Village's native camera follows the current hero by default. Use the HUD's
**Stop following** / **Follow hero** toggle to switch tracking off or on. Manual
panning with WASD/arrows, or **F** to focus a selected unit, releases tracking;
wheel zoom and Q/E orbit preserve it. An explicit `--camera` launch option starts
with tracking off. These controls affect only the native view; the browser keeps
its own camera.

Begin with a wide view of the village and introduce the zero-coin newcomer.
Follow a few concrete questions rather than forcing a script:

- What does Herp notice, and what useful work can Herp offer?
- Does a buyer need those goods and have enough money to pay?
- How does Herp's work support a resident's next production step?
- Will Herp buy provisions and rest before reserves become dangerous?
- What does Herp do when a trader moves, stock runs out, or a plan fails?

The HUD explains current activity, reserves and coins. Hunger, thirst and energy
are **remaining reserves**; higher is healthier. An accepted action may still be
queued or in progress. Use completed events and changed inventory/coins to
explain outcomes. A resident walking away can mean work, provisioning or rest.
Action/thought bubbles name Herp's public issued actions, not private reasoning
or an invented thought transcript. Speech bubbles show
actual spoken lines. Herp can use MCP `talk` with `{text, target?}` to address a
nearby actor, or omit the target to speak aloud. Nearby residents can answer;
another visiting actor can participate too. These are public game words, so
keep private notes, credentials and private reasoning out of dialogue.
The standard day lasts 20 minutes at 1x; seasons take seven days, so winter is a
separate accelerated rehearsal rather than a promised 30-minute stream event.

Use the first ten minutes for arrival and first income, the next twenty for
production/trade/needs, then allow time for a Diary recap and safe save. This is
an observation schedule, not a gameplay script. Record first sale, purchases,
rejections, stuck time, operator hints, deaths and continuation count separately
from simulation ticks and provider latency.

## Recover and close

If Herp is busy, wait for completion or let Herp cancel work through its tools.
The event-aware scheduler queues relevant work, trade, needs and failure events
for Herp and can resume the linked conversation before the idle timer expires.
It delivers a bounded historical batch; Herp observes again before acting.
Events wait through busy actions, paused worlds, active turns and cooldown/rate
limits. The scheduler panel shows pending groups, the last trigger and overflow;
`scheduler status` exposes the same safe counters. Worlds > AI scheduler controls
event delivery separately from the master supervisor switch.
If Herp is idle, inspect the scheduler state: unlinked requires operator setup;
running/cooldown/backoff means respect its timer; provider errors need operator
attention. Do not repeatedly nudge an already active provider turn.

If technical intervention is necessary, pause as operator and say why. Preserve
the failed action and its visible reason in the issue record. Resume only after
the operational problem is understood; do not quietly replace stock, money or
needs to improve the broadcast. Permanent death is a valid story outcome. Use
**New life** only when choosing to continue that story, then enter/control the
successor and explicitly review its scheduler link.

Close through the native save/stop flow. A failed save keeps the host alive;
resolve it before closing. To continue the same village later, stop its old
owned host first and run:

```powershell
target/release/actor_testbed.exe --explorer-home C:/NPC/herp-stream --agent Herp --resume --world-state-out ../target/world-state/herp.json --world-state-interval-ms 500
```

Resume uses the saved immutable world and actor presets. Register and reclaim
control through the generated tools; confirm the actor, inventory, coins, Diary
and calendar before returning on air. Keep any raw evidence in the disposable
folder or ignored `target/`; share only sanitized summaries and game captures.
