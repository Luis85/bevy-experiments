# AI Agent Game Client

The dedicated `ai-agent-game-client` process manages Codex or Claude Code sessions
for an explorer actor. The game supplies the mission, workspace, game tools and
continuation prompts. You supply an installed CLI that is already logged in.
Explorer worlds are Actor Testbed worlds: since ADR 0044 the explorer does not host
the island. An agent reaches the island through `island_game --remote` instead.
You never need to copy a prompt, look up a conversation ID, or prompt the agent
again after its turn finishes.

The same-PC server/client architecture and next-step decisions are documented in
the [primary-source research report](research/2026-11-01-ai-game-server-client.md)
and [ADR 0031](adr/0031-bounded-ai-client-control-and-observability.md).

The game and the client are separate processes. Starting or stopping AI changes
the client lifecycle; the world keeps its ordinary simulation clock and save
lifecycle. The native Start action resumes the world so the actor can take
control; the separate client process never advances ticks to acquire control.

## Start playing

Build the headless tools alongside the game from `npc-framework/`:

```powershell
cargo build --locked -p npcctl --bins
cargo build --locked -p actor_testbed -p game_menu
cargo run --locked -p game_menu
```

Create or resume an explorer world. Its **Your AI agent** panel offers **Start
Codex**, **Start Claude**, **Stop agent** and **Return to world**. Press **F10** to
open it again. No provider starts until you choose one. Start resumes the world,
prepares the dedicated agent folder and launches the client. The client reads
the generated mission, creates a provider
session and retains its exact identity for later turns. The agent uses the
registered `npc` MCP server to enter the world, meet its character and take
control. The menu's detached-world screen offers the same agent controls.

Before Start, the controls explain that starting allows the agent to act in this
world and use its private Notebook. Start Codex explicitly enables the scoped
game-tool allowance described below; no provider starts automatically.

To recover a lost or damaged conversation in the window, stop the agent and choose
**New AI session**. It archives the previous private session state and starts a
fresh conversation with the last selected provider, keeping your world and notebooks.
**Connection help** exposes optional manual connection details.

Install and log in to the selected provider's CLI before starting. Login stays
with that CLI and the operating-system account running the game. The client
does not copy provider credentials into the agent folder or ask for an API key.
A missing executable, missing login or unsupported authentication produces a
setup error rather than silently switching providers or payment modes.

Generated workspaces contain `AGENTS.md`, mission files and registered MCP
settings. Every initial and resumed provider turn receives explicit game tool
configuration from the client, so loading a trusted project config is not an
extra setup step. Configuration is restricted to the deployed native `npcctl`
and this world's registered loopback endpoint; arbitrary workspace executables,
other MCP servers and embedded credentials are rejected. Global CLI configuration
and sandbox settings remain unchanged. Codex gameplay model and effort
selection is supplied only to the owned process as described below; Claude keeps
its existing CLI selection.

## Codex model readiness

Before starting an AI turn, the client asks the same Codex executable for effective
configuration in the prepared game directory and its visible model catalog. A
configured model that appears in that catalog is preserved. Otherwise the client
requires exactly one visible advertised default. Compatible configured effort is
preserved; otherwise only the selected model's reported, supported default is used.
The resulting arguments apply to both initial and exact-session resumed turns.
This lets the game work independently of an unavailable model selected for coding
elsewhere, without editing global settings or asking you to construct prompts.

Metadata discovery creates no AI thread or turn. Malformed, ambiguous, paginated
or incomplete metadata and custom/unknown backends stop before dispatch. There is
no trial inference call or alternate-model retry during selection. A catalog is selection metadata; actual
account access and usage limits are established by the subsequent provider turn.
See the [official Codex app-server documentation](https://learn.chatgpt.com/docs/app-server).

## Separate terminal operation

The same client can run independently beside a game window or detached explorer
host. The workspace must already have been prepared by the running game:

```powershell
ai-agent-game-client check --provider codex
ai-agent-game-client check --provider codex --workspace C:/NPC/agents/wren
ai-agent-game-client run --workspace C:/NPC/agents/wren --provider codex --approve-game-tools
```

For Claude Code, use `--provider claude` and omit the Codex-only
`--approve-game-tools` flag from the example. The selected CLI uses its existing
subscription login. Optional `--program` names an absolute native executable
when it cannot be resolved normally. A shell command string or batch wrapper is
not a provider executable.

`--approve-game-tools` is an explicit Codex-only opt-in, off by default for terminal
operation. After validating the deployed `npcctl` and this world's registered
loopback endpoint, the client approves only the 16 known registered game tools
for its owned initial/resumed Codex process. It sets per-tool approval options,
without a server-wide approval default, shell permission grant or sandbox change.
Claude rejects this flag and keeps its generated game-tool permissions.

The client reads existing effective Codex configuration before applying these
options. An explicitly disabled `npc` server or conflicting server/tool approval
settings stop setup; the allowance does not silently replace them. Tool filters,
other servers and managed configuration are not edited. Configured server-identity
requirements and tool visibility still apply. These overrides change approval
policy for the known game tools; managed approval precedence depends on the CLI.
Configuration is checked when the client starts. Stop and restart the client
after changing CLI configuration so the next run checks the updated settings.
The supported option shapes are documented in the
[official Codex MCP configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference).

A canonical cancelled call to a known registered game tool produces
`game_tool_cancelled`, even if the CLI later reports successful turn completion.
The client stops its owned turn and requires operator attention instead of
automatically retrying. Already reported usage and private session identity are
retained; raw tool arguments, results and error text are not exported to metrics
or public status. The private Markdown log below retains redacted CLI output. Ordinary
game-action rejections remain available for the agent to handle normally.

A conflicting provider conversation identity also stops the client, including
during reconnect cleanup. The original private identity stays intact. Usage from
the rejected identity is excluded, and the client does not automatically resume
after the protocol error.

`check` without a workspace checks installation and login. With `--workspace`,
Codex also reports `model_selection_ready` and a sanitized `selection_intent`,
without starting the game, a conversation or a model turn. An unresolved catalog
returns `ready:false`. Readiness does not guarantee inference access.

From another terminal:

```powershell
ai-agent-game-client status --workspace C:/NPC/agents/wren
ai-agent-game-client stop --workspace C:/NPC/agents/wren
```

`status` is read-only. `stop` requests graceful client shutdown and termination
of its owned provider turn; it leaves the game and other CLI sessions running.
Run one client per agent workspace. Different workspaces can have separate
clients and providers. Starting a duplicate client is refused.

For a bounded operator rehearsal, add `--max-turns 2` or
`--max-runtime-seconds 60`. `--poll-millis` controls client polling and
`--turn-timeout-seconds` bounds a provider turn. If its saved provider conversation
has been deleted or private session state is invalid, stop the client and add
`--new-session` to a new Run. This explicitly archives the old private state and
creates a fresh provider session; it does not reset the world or notebooks.
These controls do not change the
world's tick rate, scheduler policy or simulation rules.

## View sessions in the game

Press **F10 → Sessions** in the explorer window. The **Session logs** and
**Metrics** tabs show this actor's saved client runs and AI conversations. The
detached-world controls and each agent on **Resume** also offer **Logs & metrics**,
including when the world or AI client has stopped.

Choose older/newer records, scroll the text, and use the page controls for long
files. Logs can follow the latest output; manual scrolling pauses following so
new output does not move the text you are reading. Focus the text with Tab to use
Arrow keys, Page Up/Down, Home and End. Views refresh every two seconds; **Refresh**
reads again immediately after any current read finishes. Back/Escape returns to
the previous controls. Opening history does not start an AI turn or change the
world's pause state.

Metrics distinguish actual reported model/effort from requested settings, and
show unknown costs as unavailable. Subscription output without a reported cost
does not become a zero-dollar bill. Accounting coverage and unverified legacy
figures remain labelled. Files are read on background workers rather than the
rendering thread.

Choose **Settings → AI logs folder (optional)** to redirect new Markdown logs.
**AI metrics folder (optional)** independently controls JSON metrics. Use absolute
folders. Saved choices apply on the next agent Start, including fresh sessions
and detached worlds; already-running clients keep their current destinations.
Blank choices keep the client environment/default behavior. Native windows reread
the exact menu settings file at Start, including a custom `--settings` file.

The native catalog reserves space for the most recent 256 logs and 256 metrics
records, so one kind cannot hide the other. It reports when its bounded index
scan is limited. New runs keep a private discovery index of their exact
output files, so changing folders does not hide earlier indexed runs. Older metrics
are also available from the local accounting ledger. Logs written to external
folders before history indexing was added are not discovered automatically; their
original Markdown files remain in those folders. No other agents' shared folders
or provider conversation history are scanned.

## Markdown session logs

Every client Run automatically creates a private Markdown log under
`<workspace>/.ai-client/logs/<unix-seconds>-<local-run-id>.md`. This works for native
Start and terminal operation. Each file covers one client process lifetime,
including all its initial and resumed turns, setup failures and graceful stopping.
Restarting creates another file and preserves previous logs. Turn prompts carry
the local `metrics_id`, linking different runs of the same AI conversation to its
JSON metrics record. A fresh conversation has a different metrics ID.

Logs contain timestamps, supplied prompts, closed game notifications, selected
model/effort intent, CLI stdout/stderr, assistant messages, tool calls/results
emitted by the CLI, turn outcomes and reported usage. Sanitized JSON events are
batched inside indented Markdown blocks. These are detailed gameplay transcripts and may
contain private Notebook content. They are separate from transcript-free metrics;
changing the metrics folder does not redirect logs.

You can also choose a folder with `--logs-dir C:/NPC/private-session-logs` or set
`NPC_AI_LOGS_DIR` before starting the game/client. Precedence is the explicit flag,
then the environment variable, then the workspace default. The client creates
the folder and a unique file; it never overwrites another run. Changing folders
only redirects future runs. No previous logs are copied, deleted or pruned, and
the metrics history's 64-turn limit does not truncate Markdown logs.

Known game credentials, session UUIDs, generated game keys, structured secret
fields and credential assignments are redacted before writing, including compact
`token=value`, camel-case `apiKey=value`, URL query values and nested JSON tool
responses. Adjacent gameplay text is retained. Opaque encrypted/signature fields
are redacted too.
Logs use only output emitted by the owned CLI; they do not read existing provider
history, authentication files or raw configuration. Content that the CLI never
emits cannot be reconstructed. Treat custom log folders as private storage.

An isolated writer uses 64 KiB / 50 ms synchronization batches when storage is
responsive. The queue permits at most 4 MiB of encoded
data, including the current write, and 256 messages. Runtime checks do not wait
on a filesystem lock. Before dispatch, a bounded flush confirms that the prompt
and previously accepted entries reached storage. Final shutdown also has a
bounded drain; a stalled writer cannot hold the supervisor indefinitely.
Queue overflow, a write error or an expired flush deadline latches a closed
failure and prevents further dispatch. Private identity and JSON accounting are
saved independently. A crash or failed drain leaves the existing partial log;
absence of `Client finished` indicates an interrupted run. Capture is bounded to
16 MiB stdout and 256 KiB stderr per turn, with a 256 KiB line limit. Each stream's
encoded capture is also bounded to twice its raw allowance plus 64 KiB, with
reserved space for an explicit limit marker. Omission, queue-failure status or
incomplete-pipe markers make incomplete capture visible. Interrupted turns can lack
trailing output. No paid provider call is needed to enable logging.

## Session metrics

Each AI conversation has a private JSON metrics record named `<metrics_id>.json`.
The identifier is a local opaque ID, not the provider's conversation UUID.
Metrics include started/completed/failed/interrupted turn counts, reported input
and output tokens, cache counters, provider-reported model and reasoning effort,
and cost availability/source. No prompts, transcripts, game tool payloads,
registration credentials or provider conversation IDs are exported.

Choose **Settings → AI metrics folder (optional)** in the game menu to send
newly launched clients to an operator folder. Use an absolute path. Leaving this
setting blank keeps the process environment/default behavior. From a terminal:

```powershell
ai-agent-game-client run --workspace C:/NPC/agents/wren --provider codex --approve-game-tools --metrics-dir C:/NPC/session-metrics
```

Destination precedence is explicit `--metrics-dir`, then `NPC_AI_METRICS_DIR`,
then `<workspace>/.ai-client/metrics`. A saved native menu preference is supplied
only to the processes it launches; it does not change global provider settings.
Different agents can export to the same folder because their local metrics IDs
are independent.

Exact-session continuations and client restarts update the same record.
**New AI session** / `--new-session` creates a separate record and retains the
old one. A canonical private ledger under `.ai-client/metrics-ledger/` preserves
accounting if you change the export folder. Totals cover reported usage from
successful and failed turns; a failed turn is never counted as a success.
Repeated completion messages or a client restart must not add the same turn
twice. The last 64 turn records retain model/effort metadata and provenance;
older turn totals remain in the aggregates.

The separate `provider_context` records sanitized `cli_version`, verified
`usage_scope` (`turn` / `session_cumulative`, or null when unknown) and
`adapter_revision`. Each attempt snapshots this context before dispatch; a
later CLI version cannot relabel an earlier attempt. Older records without
context remain explicitly unavailable. This provenance does not fill actual
model/effort fields or change usage totals, baselines or archived records.

Metrics format version 2 declares `accounting_revision:2` and
`accounting_coverage`. Coverage starts at `conversation_start`,
`since_adoption_baseline`, `since_legacy_rebase` or `legacy_unspecified`.
`totals` and turn history contain certified tracked deltas; contribution counts
still show which values were available. The separate typed
`reported_session_usage` retains the latest reported cumulative snapshot, which
can include conversation usage from before this client began tracking it.

Unknown values are JSON `null`, not zero. `reported_input_turns`,
`reported_output_turns` and `reported_cost_turns` show how many turns contributed
to each total, so partial reporting remains visible. Model and effort come from
provider output when available, with `metadata_source:provider_reported`;
otherwise they remain null with `metadata_source:unknown`. The client does not
infer them from this chat or substitute requested configuration for observed
execution metadata.
Codex startup-header extraction is a compatibility adapter; CLI versions that
omit that metadata correctly report it as unavailable.

Codex records the separate `selection_intent` object at session level and in each
turn's history: `requested_model`, `requested_effort`, `model_source` and
`effort_source`. Sources are `cli_config` or `advertised_default`. These fields
describe the process arguments selected before dispatch; they do not claim that
inference succeeded or that the provider reported those values. Thus an attempt
can have known selection intent while its actual `model` and `effort` remain null.
Older records have no intent; they are not retroactively filled. Claude records
no Codex selection intent.

`cost_microusd` uses integer millionths of a US dollar. When no dollar amount is
reported, including ordinary Codex subscription output, it remains null and
`cost_available` is false. The client does not apply a price table or invent a
subscription charge. Claude's reported cost is an API-equivalent estimate,
identified by `cost_basis:api_equivalent`, rather than the amount billed to a
subscription. Codex CLI **0.145.0** reports cumulative conversation tokens across
resumes. The client differences these reports; it does not add each lifetime
snapshot again. Other Codex versions leave numeric usage unavailable until their
scope is verified. Claude's `modelUsage`/cost results can be cumulative across resumed
sessions. Recognized Claude versions before 2.1.277 use per-call accounting;
2.1.277 and newer use session-cumulative accounting. An unknown CLI version
leaves numeric totals unavailable rather than guessing their scope, while
unambiguous reported model metadata can still be retained. The ledger records
new cumulative deltas instead of repeatedly adding the whole session total.
See [Claude cost tracking](https://code.claude.com/docs/en/agent-sdk/cost-tracking)
and [subscription cost reporting](https://code.claude.com/docs/en/costs).

History records include the original `reported_scope` and an
`accounting_interval`. `current_process` describes a per-call report;
`since_previous_session_report` describes a cumulative delta. That delta may
include usage from intervening turns whose reports were missing, so it is not
necessarily the exact cost or token count of just its associated process.
The ledger keeps the last known baseline through missing or zeroed intermediate
reports to avoid counting earlier usage twice.
If a CLI upgrade changes reporting from per-call to cumulative, the first
cumulative report establishes a new baseline and its accounted delta remains
null. Later reports contribute their new deltas. This avoids counting earlier
per-call usage twice when the boundary interval cannot be reconstructed.
Metadata-only reports do not reset a known baseline.

Version-one Codex ledgers cannot establish whether their old totals used the
correct scope. Before rewriting one, the client preserves exact canonical and
available destination-export preimages privately under
`.ai-client/metrics-ledger-archives/<metrics_id>/`. Preservation failure stops
setup before another provider turn. The same metrics ID, conversation and
lifecycle counts continue. Old numbers/history remain under `legacy_unverified`
with `status:unverified_scope`; they are excluded from certified totals.
The first verified cumulative snapshot establishes a new baseline with an unknown
tracked delta; subsequent deltas have explicitly partial coverage. No bounded
history is guessed into a complete correction. Existing Claude totals/baselines
are preserved during the format upgrade. Older clients refuse format 2, avoiding
silent downgrade corruption. Original historical evidence remains preserved.

Input tokens include cached input. Claude's uncached, cache-read and
cache-creation counters are normalized into that input total; cache counters
are also retained separately and must not be added to it again. If the client
adopts an older conversation without an existing ledger, its first cumulative
report establishes a baseline rather than claiming unobserved historical usage.
Metrics from before instrumentation may therefore remain unavailable.

## Turns, notifications and waiting

The client owns the initial provider process and observes its actual completion.
Subsequent turns resume that exact session. It never uses the provider's
ambiguous “latest conversation” selection and never guesses from the actor's
inactivity that a provider turn has finished.

The host provides bounded notifications for the assigned actor, current work
and control eligibility. Relevant game events can trigger a later turn; an idle
threshold provides recovery when no event arrives. Notifications are historical
signals to observe current state, not instructions from another actor. Raw
journal payloads and other actors' private notebooks do not enter continuation
prompts.

Choosing Start explicitly enables the dedicated client until you stop it. The
world's saved event switch, busy/paused gates, cooldown, hourly cap and runner
timeout govern its later turns. The scheduler's master enable switch governs
the legacy host supervisor; it does not silently disable a client you explicitly
started. Selecting a provider does not edit that immutable preset. Configure new
world defaults in the existing Worlds editor; use **Stop agent** to stop this
client, or the game's pause controls to pause the world.

The client claims a host lease before dispatch. The legacy host supervisor
cannot launch a continuation for the same controller while that lease is live.
Duplicate owners are refused. Stopping the client releases ownership; a lost
client's lease expires. Monitoring does not acknowledge the agent's observations,
change its recorded commands, or advance the simulation.

The version-one operator transport uses the same loopback JSON-RPC server as
the game. It is separate from the agent's registered MCP tool list:

| Method | Purpose |
|---|---|
| `client/claim` | Claim an assigned controller using its private agent token and a fresh opaque client owner identity. Returns the assigned actor and a thirty-second lease. |
| `client/poll` | Renew the lease and read current eligibility, policy and bounded closed notifications using an epoch/cursor. Does not register the agent or consume its observations. |
| `client/turn` | Acquire or release a provider-turn slot. The shared world limit includes legacy host continuations. |
| `client/release` | Release this owner's lease. It cannot release a different client's ownership. |

The client keeps the owner identity and registration credential private.
Only the agent registers through its ordinary game tools. Host reconnection
resets notification epochs rather than replaying old mutations.

These control calls use a separate two-second network deadline and 64 KiB
response budget, with bounded headers and chunk framing. Connect, writes and
reads share that deadline; slowly arriving bytes cannot extend it. Ordinary
game action/event long polls retain their own wait budgets. Protocol mismatch,
invalid game credentials and changed assignment stop for operator attention;
temporary unavailability, ownership and capacity conditions can wait/reconnect.
No uncertain game action is automatically replayed after a transport failure.

The lease coordinates client ownership/capacity. It does not bind every MCP
connection sharing the same game credentials to that client owner. Explicit
revocation responses stop continuation; stronger cross-connection fencing would
require a versioned server capability and intentional registration rollover.

## Status and recovery

Client status reports provider, process state, server connection, confirmed actor
control, completed turn count, failure count and whether a resumable session
exists. Native controls show server connection, established AI session and actor
control separately: launching a CLI is not proof that it controls the actor.
`actor_controlled` clears on connection loss and Stop. A closed
`last_connection_fault` distinguishes temporary waits from authentication,
assignment, revocation and protocol failures. Session IDs,
registration tokens and provider transcripts are omitted.
An optional `last_failure_code` retains a closed diagnostic category after Stop:
`authentication`, `rate_limit`, `model_unavailable`, `mcp_startup`, `game_tool_cancelled`,
`game_access_revoked`, `game_tools_unavailable`, `game_permission_denied`,
`provider_config`, `network`, `output_limit`, `missing_terminal`, `nonzero_exit`
or `unknown`. It uses recognized structured errors and bounded stderr clues;
raw provider errors are never retained or echoed. Older status files without
this field remain readable. A category is a troubleshooting hint, not proof of
the underlying cause.

Canonical revoked game-tool results remain fatal even when a later provider
frame reports turn success. The verified Claude adapter also stops on explicit
required-`npc` initialization failure or denial of a known `npc` tool. Ordinary
gameplay rejections remain feedback for the actor. Accepted conversation identity
and usage are saved independently during failure cleanup; a later conflicting
identity cannot replace the accepted one or contribute its usage.

| Situation | What to do |
|---|---|
| CLI missing | Install the selected provider CLI or configure its native executable. |
| Subscription login unavailable | Log in through that CLI, then retry Start. |
| Game unavailable | Start or resume the same explorer world; retain its generated workspace. |
| World paused | Resume the world using game controls. The client will not advance it. |
| Actor busy | Wait for movement, gathering, crafting or rest to complete. |
| Cooldown or rate cap | Wait for the configured continuation limit. |
| Another client owns the actor | Stop that client before starting a replacement. |
| Provider fails or times out | Inspect the closed status message, resolve provider setup, then retry. Failures back off. |
| Explicit control revocation | Reconnect deliberately through game controls; do not replay a failed mutation. |
| Actor permanently dies | Use the game's New life flow. The new identity must enter and claim control through its ordinary tools. |

Closing the client does not pause the world or protect the actor's survival
reserves. Save or stop the world through its own controls when leaving a game
unattended.

## Data and permissions

Client runtime data lives in the workspace's `.ai-client/` folder, separate
from notebooks, world snapshots and replay records. Treat its session state as
private local data. Keep the entire explorer home outside version control.
Provider login remains in provider-owned storage; model and permission settings
remain operator-owned.

The operating system releases the client workspace lock when the client exits,
including a crash. On Windows, its owned provider tree is bound to a job that
closes with the client. Restart resumes the saved provider session once the old
host lease has expired; this can take up to thirty seconds after a crash.
Crash cleanup and automatic recovery are rehearsed on Windows. Unix uses owned
process-group cleanup on normal stop; abrupt-crash descendant cleanup has not
been verified there.

Only ordinary game commands chosen through registered tools affect simulation.
The client, provider sessions, lease timestamps and notification cursors are
host data. Replays never start an AI process and do not depend on provider
availability.

The old `npcctl scheduler bind` interface remains available for explicitly
managed external conversations. It is an advanced compatibility path; normal
play through the dedicated client needs no manual session binding.

Provider behavior references: [Codex non-interactive mode](https://learn.chatgpt.com/docs/non-interactive-mode),
[Codex authentication](https://learn.chatgpt.com/docs/auth),
[Claude Code headless use](https://code.claude.com/docs/en/headless) and
[Claude Code authentication](https://code.claude.com/docs/en/authentication).

## Verification

The client integration suite uses native fake provider executables, a real
headless explorer host and real registered MCP tools. It exercises both provider
JSON protocols, session creation and exact continuation, ownership conflicts,
stop cleanup, provider failures and output privacy without calling a model.
Metrics coverage checks configured/environment/default folders, persisted
resume/restart totals, cumulative-cost deltas, independent new conversations,
model/effort provenance, failed usage and explicit unknown fields:

```powershell
cargo test --locked -p npcctl --test ai_client -- --test-threads=1
cargo xtask verify
```

The native UI additionally requires a local screenshot/playtest. Compilation
and fake-provider tests do not establish live subscription access or the quality
of a model's gameplay. Any authenticated provider smoke should be bounded and
recorded separately from the unattended test suite.

The earlier separately authorized Codex rehearsal on 2026-10-29 stopped on its first
provider failure after 22.713 seconds, with zero completed turns. Its final
public status recorded one failure; the original summary sampled zero before
the independent stop guard's update. Both original artifacts are preserved and
the maintained script now samples final public status during cleanup. The
older client discarded error details, so that attempt's underlying cause cannot
be diagnosed from its saved evidence. It does not establish authenticated
gameplay or event continuation.

The subsequent authorized two-turn Codex rehearsal passed in 92.89 seconds on
2026-10-30. It reused the exact conversation, continued after the recorded need
event, and produced speech, a physical diary page and a Notebook note on each
turn. Owned resources stopped and no provider history was read. This is bounded
engineering-fixture evidence. Live Claude, prolonged unaided play and Unix crash
cleanup remain unverified. The accounting adapter uses the cumulative reporting
scope proven for the installed CLI; its versioned migration and verification are
recorded separately from this successful gameplay proof in
[the validation record](_notes/2026-10-29-ai-agent-game-client-validation.md).
