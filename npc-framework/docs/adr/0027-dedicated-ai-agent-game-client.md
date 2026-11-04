# ADR 0027: Dedicated AI-Agent-Game-Client

- Status: Accepted for the dedicated client implementation
- Date: 2026-10-29

An operator starts a game and a separate AI-Agent-Game-Client. The client owns
provider processes, initial prompts, exact conversation continuity, bounded
continuations, cancellation and private operational state. Provider CLIs retain
their own subscription authentication and global permission settings. The game
does not need to launch a provider to run an AI scenario.

Keep the existing registered MCP interface as the actor's game interface and the
generated explorer folder as its workspace. The client must not register a
second monitoring session: doing so can revoke the provider's session. Its
operator monitoring bridge is separate from `agent/*`.

`client/claim` obtains an exclusive, renewable 30-second lease for a controller.
`client/poll` renews that lease and returns host-only activity, work and eligibility
state, world policy and bounded closed event summaries. `client/turn` reserves or
releases a turn slot shared with all other clients and legacy runners, enforcing
the world's concurrency limit. `client/release` yields ownership. Registered-world
claims must validate the agent's private token without creating a game session.
Later requests carry an opaque private owner identity; responses never echo
it, agent tokens, registration session keys or provider conversation IDs. These
are local operator endpoints, not MCP tools available to actors. The headless
host retains HTTP Origin/Host/content-type checks and optional bearer protection;
native Bevy BRP has its documented header-inspection limitation. Token-protected
claims and private lease identities prevent browser callers from seizing control.

Explorer controller identities resolve their assigned actor through existing
host credentials, including before the actor first attaches. Initial bootstrap
can therefore launch a provider without copying a prompt or obtaining an old
conversation UUID. Monitoring creates no game session, increments no agent
activity counter, acknowledges no actor event receipt and advances no tick.

Claims reject an active legacy host runner and another live client owner. While
a lease exists, the legacy host supervisor cannot dispatch for that controller.
Expired leases restore legacy eligibility; clients must renew during provider
turns and stop their owned process when they lose ownership. Existing scheduler
bindings and manual interfaces remain backward compatible.

The bridge distinguishes initial registration/control from lost authorization.
Once authorization has been observed, its loss and changed credentials or actor
assignment latch a revoked state for that lease. Explicit release and a new
claim are required; monitoring never repairs revocation or replays an action.

Notifications reuse the host's trusted closed classifier. Raw journal payloads,
speech, notebooks, secrets and arbitrary game text cannot enter a continuation
through this channel. Each page scans at most 4096 ring entries, contains at most
16 coalesced summary groups and
returns the exact scanned cursor, ring-gap count, actor observation receipt and
world epoch. The client prunes notifications already consumed by its actor's
authorized observation; monitoring itself advances no receipt. A missing or
changed epoch starts at the current cursor rather than replaying a previous
world's history. Reads are repeatable and do not consume provider observations.

All leases, deadlines, activity, provider state and delivery bookkeeping remain
outside simulation registries, hashes, snapshots and replay inputs. Resulting
actor actions continue through ordinary recorded commands. Regression tests use
injected host state and owned fake provider processes; successful compilation or
fake providers do not establish authenticated gameplay or native visual quality.

## 2026-10-30: Process-local Codex model resolution

A real bounded rehearsal failed before gameplay because the CLI inherited a
coding model absent from its advertised catalog. Requiring a global configuration
edit would conflict with the intended native flow: supplying a logged-in CLI
should be sufficient for the client to prepare a turn.

Resolve effective configuration with `config/read` for the exact workspace and
query `model/list` on the same executable, working directory and subscription
environment. These metadata requests create no thread, turn or game session and
never write configuration. Preserve a visible configured model; otherwise require
exactly one visible advertised default. Preserve configured effort when supported,
otherwise use only a reported supported default. Reject ambiguous, incomplete,
paginated or invalid metadata and custom/unknown providers. Model identifiers are
never hardcoded. Pass the resulting options only to the owned Codex initial or
resume process. No paid trial or automatic retry selects a model.

Catalog metadata is not proof of inference entitlement or successful gameplay.
Keep selection intent separately attributed in metrics; provider-reported model,
effort, token and cost fields retain their existing meaning and unknown values.
Do not rewrite historical exports from subsequently observed configuration.
Global CLI settings, authentication and sandbox policy remain unchanged.

## 2026-10-30: Explicit registered game-tool allowance

The compatible-model rehearsal completed a provider turn but failed gameplay
acceptance. A separately authorized, narrowly scoped inspection found a cancelled
`register` call. The pinned Codex noninteractive approval path can cancel MCP
elicitation; successful CLI completion alone does not establish gameplay success.

Add an explicit Codex-only `--approve-game-tools` flag, default false. Native Start
shows permission scope before the button and passes this opt-in for Codex only.
After existing trusted-executable and current registered-loopback validation,
apply per-tool `approval_mode="approve"` to the closed 16-tool allowlist on the
owned initial/resumed process. Never set a server-wide approval default, grant
shell access, change sandbox settings or write global/provider configuration.
Inspect effective configuration first and refuse explicit server disable or
conflicting approval settings. Preserve filters, unrelated tools and configured
server-identity requirements. This deliberately changes approval policy for the
known game tools. The installed CLI determines managed approval precedence.

Keep tool annotations conservative: registration replaces an authentication
session and is neither read-only nor strictly additive. An annotation change
must describe behavior truthfully, rather than remove a prompt by mislabelling it.

Recognize cancellation only from the pinned structured Codex MCP event for the
`npc` server, a known registered tool and the canonical cancellation error.
Latch the closed `game_tool_cancelled` diagnostic across later terminal events,
stop without automatic retry and retain already reported usage/session identity.
Do not export tool payloads or classify arbitrary action errors as cancellations.

Cleanup preserves a known cancellation even if a subsequent identity error makes
polling fail. All polling errors propagate after owned shutdown and independent
accounting, rather than becoming an interrupted turn eligible for reconnect.
Identity rejection latches before accepting later progress or metadata; validate
terminal identity before merging usage. Keep accepted usage for the original
session and exclude reports belonging to a conflicting identity.

## 2026-10-30: Cumulative Codex usage and versioned accounting

The successful bounded Codex rehearsal exposed a separate accounting defect:
CLI 0.145.0 emits cumulative conversation usage, restoring prior totals on
resume. Adding its successive snapshots as per-turn amounts counted earlier
usage twice. Recognize this proven version as cumulative; leave numeric usage
unavailable for unverified Codex versions. Keep raw reported session snapshots
separate from certified tracked deltas and preserve cost availability semantics.

Use metrics format/revision 2 with explicit coverage. Fresh conversations start
at zero; adopted conversations establish a baseline without inventing earlier
tracked usage. Version-one Codex records lack enough provenance to reconstruct
their scope or original adoption boundary. Preserve exact canonical and available
export preimages privately before migrating, fail closed if preservation fails,
and quarantine old figures as unverified. Preserve metrics identity, provider
session identity and lifecycle counts. The next verified cumulative snapshot
establishes a baseline; subsequent deltas have partial coverage. Preserve existing
Claude accounting during the format upgrade. Older clients must refuse format 2
rather than silently drop these fields and restore incorrect aggregation.

The original live rehearsal evidence remains immutable. Validate the corrected
arithmetic from its already exported snapshots without another provider call;
publish any corrected calculation as a separate audit artifact.

## 2026-11-01: Private Markdown client-session logs

Automatically record each client process lifetime in a unique Markdown file,
including setup failure, all prompts/turns, CLI stream evidence and shutdown.
Use `.ai-client/logs` by default, independently of telemetry exports; allow
`--logs-dir` and `NPC_AI_LOGS_DIR`. Preserve earlier run files across restarts and
link each turn to its stable local conversation metrics ID. This captures failed
starts even before a provider conversation exists, without reading provider history.

Redact known credentials, structured secret fields, UUIDs and generated keys
before disk writes, including JSON object keys and nested textual JSON. Keep
arbitrary content inside indented blocks. Explicitly mark per-turn capture caps,
oversized lines and incomplete output. The original per-entry flush requirement
is superseded by [ADR 0031](0031-bounded-ai-client-control-and-observability.md):
bounded queued persistence, synchronization batches and explicit durable barriers
before provider probes/dispatch and at shutdown. A missing final entry
is crash evidence, never an invented successful termination. Logging failure is
fatal for dispatch but must still drain accepted identity/accounting and stop
owned processes independently. Metrics and public status remain transcript-free.
