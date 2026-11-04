# ADR 0031: Bounded AI client control and observability

Status: accepted, 2026-11-01.

The user confirmed same-PC server connections first, including headless hosting.
The game owns actor assignment, authorized observations/actions and simulation
time. The dedicated AI client owns provider conversations, prompts, supervision
and private observability. A provider app-server is a possible adapter, not a
replacement game server. See the [primary-source research](../research/2026-11-01-ai-game-server-client.md).

Use a separate loopback-only, deadline/response-bounded JSON-RPC path for client
claim/poll/turn/release. Connection, writes and all reads share one deadline;
literal loopback/localhost avoids blocking DNS. Preserve existing action/event
long-poll behavior. Validate correlated JSON-RPC envelopes, bound response heads,
chunk framing and bodies, classify protocol/authentication failures permanently,
and never automatically replay a potentially accepted game action.

Provider output is evidence only through narrowly recognized structured frames.
Revoked game access must remain fatal even if a CLI subsequently reports turn
success. Preserve already accepted identity/usage during independent cleanup.
Expose actor attachment separately from game connection and provider identity;
spawning a CLI is not proof of actor control.

Transcript storage must not hold supervision behind a disk lock. Use bounded
queued persistence and explicit bounded durability barriers before paid dispatch
and at shutdown. Bound encoded capture separately from raw provider streams,
retain a clear partial/omission indication on failure, and keep metrics independent.
Capture requested preferences, provider-reported metadata and version-qualified
accounting provenance separately. Preserve unknown costs and legacy evidence.

Reserve native history capacity per record kind. Continue reading only owned
indexed paths or compatible workspace-local outputs; no provider-history scans.
All observability remains host data, outside simulation hashes, snapshots and
replay records. Same-user loopback isolation does not promise an OS security
sandbox against malicious local processes.

Native shortcut dispatch must preserve the order of individual keyboard events,
including modifier transitions and focus changes, using installed Bevy APIs.
Clipboard edits must respect pending selection changes and discard late results
when their target text, caret or focus changed. No global dependency/configuration
edit or artificial typing delay is an acceptable input fix.
