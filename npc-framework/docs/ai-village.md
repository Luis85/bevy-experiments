# Island Village: playing and supervising an AI actor

Normal play uses the dedicated [AI Agent Game Client](ai-agent-game-client.md).
Create or resume an explorer world, choose **Start Codex** or **Start Claude**
in **Your AI agent**, and use your already logged-in CLI. The game supplies all
prompts, launches the first turn and continues the exact session for relevant
events or idle recovery. **F10** reopens the controls; **Stop agent** leaves the
world running. No prompt copy/paste or conversation UUID lookup is required.

Choose **Island Village** when creating an AI explorer world in the game menu. It is a small island with
nine named residents, working food/material/tool chains, finite shop stock and currency, days/nights,
four seasons and permanent actor death. Existing agents/world saves retain their own immutable presets.
The NPC life-tree increment changes the island content hash: create a fresh island
for this build and preserve older island saves for their matching binary. Residents
and clients now share rest/recovery and production actions; see
[NPC life and the environment API](npc-life-and-environment.md).
For cold setup, registered-client rehearsals, recovery and the observed-session run sheet, see
[the first-playtest guide](island-village-playtest.md).
The new preset is installed additively in usable preset folders; a user-owned `island_village.json` is
preserved. Folders containing only invalid presets keep their errors and disabled Start action.

For a quick standalone view, run from `npc-framework/`:

```powershell
cargo run --locked --release -p actor_testbed -- --scenario ai_village
```

For a persistent headless AI world, use the normal explorer host:

```powershell
cargo run --locked --release -p npcctl -- serve --explorer-home C:/NPC/agents --agent Wren --world Island --preset human --world-preset island_village
```

Connect with the generated agent workspace/instructions. Registered control stays scoped to the assigned
actor. The village observation section reports the actor's health, reserves, coins, calendar, nearby
perceived traders and recovered diaries. Affordances explain stock, price, funds, reach and capacity.
The island opens with a wider scene camera and its needs/economy HUD. **F1** toggles the diagnostic
controls overlay; in an explorer world, **F7** opens the actor's shared notebooks.
World labels avoid one another and the village panels. The current actor and selected unit have priority;
crowded captions may be suppressed, and old corpse captions appear when selected.

## Earn, buy, survive

The newcomer starts with zero coins. Gather fallen wood or grain, sell to a resident who needs it, and
buy necessities. Crafting at the village workstations adds more saleable goods. Shops reject unwanted
goods, have inventory limits and need real money to purchase from you. Prices are fixed in this scene.

| Need | How to satisfy it | Price |
|---|---|---:|
| Hunger | Buy bread from Mira, then `eat` from your pack | 6 coins/bread |
| Thirst | Buy clean water from Nell, then `drink` | 1 coin/flask |
| Energy | `rest` near Tess at the inn; 300 simulation ticks | 3 coins |

`buy` and `sell` take a trader target and `args: {kind, quantity?}` (1..20, default 1). `gather` takes a
resource patch target and time; it has a finite daily stock. `talk` gives a resident's current offer.
Use the scene's `stop_work` affordance to stop gathering/crafting/resting. Movement and ordinary
interaction/crafting continue to use the framework's `order`/`execute_orders`, `interact` and `craft`
tools. Inventory weight/slots, existing reservations and tool identity/wear are respected.

| Resident | Work |
|---|---|
| Ada, farmer | Collects grain at the field |
| Rowan, miller | Grain → flour |
| Mira, baker | Flour + clean water + wood fuel → bread |
| Nell, well keeper | Supplies clean water |
| Oak, woodcutter | Collects fallen logs |
| Jun, carpenter | Logs → planks → bowls |
| Bram, miner | Collects ore |
| Ivo, blacksmith | Ore + fuel → ingots; ingots + planks → hammers |
| Tess, innkeeper | Paid lodging; buys fuel, bowls and maintenance tools |

Scene item keys include `village_grain`, `village_flour`, `village_water`, `village_bowl`, plus the
existing `log`, `plank`, `ore`, `ingot`, `bread` and `hammer`. Shared crafting recipes are
`village_mill`, `village_bake`, `village_saw`, `village_carve`, `village_smelt` and `village_tool`.

## Talking and audience bubbles

The explicit MCP `talk` tool accepts `{"text":"Hello, neighbor!","target":123}`
using an actor ID your character has perceived. Omit `target` to speak aloud.
The same recorded action is available through `act` with
`{"verb":"talk","target":123,"args":{"text":"Hello, neighbor!"}}`.
Text is one public line, at most 240 characters / 1024 bytes, without control
characters. An addressed living actor must be within 1.5m; aloud speech reaches
living listeners within 6m. Nearby visitors can converse too. Asking a resident
about offers with the old textless `talk` remains supported and produces one
bounded reply; replies never recursively trigger another reply.

`village.heard_speech` contains only the actor's own last eight heard lines from
the last 1200 ticks. Distant/unheard speech and private notebooks are excluded.
Speech is ephemeral journal information and is not a saved conversation archive.
Incoming speech can notify the connected AI through the event scheduler, which
sends only a closed signal; the AI reads actual dialogue through game tools.

The native view shows transient speech bubbles and separate action/thought
bubbles near visible actors. Action bubbles describe submitted public actions,
including command actions such as trade and rest. They do not expose private
reasoning or command arguments, and acceptance is not proof of completion.
Fog, expiry and caption/HUD collision rules apply; their layout never affects
the simulation.

Residents buy inputs, work at actual places, consume food/water, and rest at night. A producer continues
working when the supply it needs is empty, allowing a stalled chain to restart. Merchant surplus flows
into a finite community fund that supports low-cash residents. These transfers conserve money; visitors
receive no stipend. Initial money and reserve goods are explicit scene endowments, not recurring refills.

The shipped day is 24,000 ticks (20 minutes at 20 Hz); each season lasts seven days. Winter reduces the
field's daily replenishment to one quarter. Low reserves slow movement. Depleted hunger/thirst have a
600-tick grace period before health loss. Exhaustion can force slow ground recovery while hunger and
thirst continue. A rented bed restores energy faster and helps health recover when fed and hydrated.
Navigation blocks the surrounding water.

## Death, diaries and a successor

Death is permanent for that SimId. Work stops, possessions/coins drop, and one physical diary appears.
The village keeps running. Other living actors can pick up and read the diary; residents can recover
nearby diaries and hand one over through `ask_diary`. The native **Found diaries** reader shows books
the current actor carries.
The reader keeps a stable book selection, disables unavailable page/book navigation and returns to the
top when changing pages or books.
Agent observations list diary metadata; `read_diary` accepts `args: {page: 1}` to read one page at a
time, keeping large recovered books out of routine observations.

Your F7/agent **Diary** becomes readable by others after death. **Memory, Proposals, map knowledge,
credentials and workspace files stay private.** Physical diary pages also accept the recorded
`write_diary` action. After death the host publishes only Diary title/body/tick pages through a recorded
command, then seals that copy. Replay never reads notebook files. Late entries are excluded; unreadable
or oversized books leave an export error and preserve the originals. A physical diary is bounded to
256 pages / 1 MiB (titles ≤200 characters, bodies ≤8192 bytes).

Choose **New life**, or use the `new_life` meta-action after death, to create a different actor in the
same world. It gets a new SimId, empty pockets, fresh senses and fresh notebooks. Previous bodies,
diaries, NPCs, production and the calendar remain. Re-enter the world after the host revokes the old
control session. A saved dead actor remains dead and can still be claimed for this meta-action.

Life one uses the original agent folder's books. Later lives use `lives/2`, `lives/3`, etc.; earlier
files remain preserved. F7 keeps an unfinished developer draft attached to its original folder until
Save/Cancel, then switches to the successor's fresh books.

## Event-aware AI continuation

The normal native flow now uses **Your AI agent** (**F10**): Start Codex/Claude
launches the [dedicated game client](ai-agent-game-client.md), which supplies the
mission and tools and manages the provider session automatically. A logged-in
CLI is the operator prerequisite. The manual prompt/session-link commands below
remain the advanced compatibility path; never run both dispatchers for one actor.

Supervision also watches meaningful events for each connected actor: needs
thresholds, action/order/crafting outcomes, failures, public day/night changes,
and the island's gathering, trading, consumption and lodging outcomes. The
Worlds editor's **Dispatch relevant game events to connected AI** switch controls
this independently of the master enable switch. Older presets default to event
delivery; the stored preset still governs each world's own configuration.

Events are deduplicated and coalesced into at most sixteen pending groups per
controller/actor. They can trigger the linked conversation before the idle
threshold, while pause, actor work, active turns, cooldown/backoff, locks and
rate/concurrency caps still apply. There is no provider interruption. Prompt
stdin contains only bounded event identities and closed summaries, never raw
payloads, other actors' private state or notebook text. The AI observes current
state before deciding; notifications describe things that already happened.

Successful observations and ordinary complete own-event reads acknowledge their
returned cursors. Filtered event reads do not discard unseen event kinds. New
links, replacement actors, revocation and reset start fresh; failed launches
retain pending notifications. Disabling event delivery clears its backlog;
pausing the master supervisor retains it. Status reports `pending_events`,
`event_overflow`, `trigger` and `last_delivery_seq` without launching anything.
Delivery records prompt handoff to the CLI, not proof the model acted on it.

Every world preset has an **AI scheduler** page in the Worlds editor. It controls enablement, real-time
idle threshold, polling, cooldown, hourly/concurrency caps and runner timeout. The shipped Island Village
enables supervision with a 120-second idle threshold and a 180-second cooldown; it cannot launch anything
until an operator links a CLI session. The in-game **AI scheduler** panel shows status and can pause/resume
nudges for the current run. Preset edits apply to new worlds; saved worlds keep their original policy.
The control names its next action (**Pause nudges** or **Resume nudges**). Read-only status reflects current
pause/control/death eligibility while retaining cached timing data; querying it never launches a turn.

### Advanced: separately managed CLI conversations

The dedicated client above automatically owns and continues its session. The
commands below support an external conversation that an operator manages
separately. They are not required for the native Start Codex / Start Claude flow.

Start the game, let that external AI connect and take control, then link its exact CLI conversation once:

```powershell
npcctl scheduler bind --home C:/NPC/agents --agent Wren --provider codex --session-id SESSION_UUID --turn-finished
npcctl scheduler status --url http://127.0.0.1:15702
npcctl scheduler check --home C:/NPC/agents --agent Wren
```

Use `--provider claude` for Claude Code. While the original **one-shot** CLI is still running, supply
`--client-pid PID` instead of `--turn-finished`; that PID is a liveness guard and is never killed.
Interactive clients need an explicit idle/finished-turn signal rather than assuming a quiet actor
means an idle model. Use a new link after changing actor or CLI conversation.

Links live under the agents folder's `.ai-scheduler/bindings.json`, separate from scene descriptors and
notebooks. For a standalone/non-explorer scene, configure `NPC_AI_SCHEDULER_BINDINGS` (default
`.npc-ai-scheduler/bindings.json` under the host working directory), then bind explicitly:

```powershell
npcctl scheduler bind --bindings C:/NPC/supervision/bindings.json --controller visitor --unit ACTOR_ID --cwd C:/NPC/ai-workspace --provider codex --session-id SESSION_UUID --turn-finished
```

Use separate bindings/unique controller identities for separate standalone game instances. `--program`
overrides an executable. Custom runners accept structured `--runner-arg` entries, with `{session_id}`
substitution and the prompt on stdin. Host status is read-only and excludes conversation UUIDs,
credentials and provider transcripts. An HTTP bearer can be supplied to the operator CLI through
`NPC_SCHEDULER_BEARER_TOKEN` when the host requires one.

The supervisor watches authorized agent activity and actual actor work. It waits through gathering,
crafting, movement, sleep and a paused game. It resumes the **same named conversation**, with no global
`--last`, no model override and no injected sandbox/approval bypass. A resumed turn must use the normal
game tools; the supervisor itself never advances a tick or chooses actor actions. Death, lost/revoked
control or a changed actor suppresses dispatch. Provider output is discarded. Only scheduler-owned
continuation processes are cleaned up/timed out. Concurrency, target locks, cooldown, failure backoff
and hourly caps bound repeated launches.

After a host crash a runner lock stays conservative: confirm the old turn has finished before rebinding
with `--turn-finished` to clear a verified stale lock. A malformed lock/edit lock needs operator inspection.
Stopping the host cleans up its own continuation processes, without terminating an original client.

Provider argv was checked against installed Codex CLI 0.145.0 and Claude Code 2.1.281. Verification uses
owned fake CLI processes; an authenticated live-provider run remains an operator integration check.
