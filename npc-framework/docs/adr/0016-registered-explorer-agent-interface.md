# ADR 0016: Registered explorer sessions and compatible transports

- Status: Accepted (agent registration plan, Tasks 30–31)
- Date: 2026-10-27
- Source: `docs/superpowers/specs/2026-10-26-agent-registration-design.md`

## Context

Explorer agents need a real credential boundary and control only of their assigned actor, while plain
scenario clients, recorded commands and existing unlocked MCP integrations must keep working. BRP and the
headless session have polling paths outside the ordinary synchronous dispatcher, so a single call guard is
insufficient. Replaying a mutating call after explicit session replacement can duplicate accepted work.

## Decision

- Registration is opt-in through `RegistrationRequired`. Credentials, secret digests, sessions, rate limits
  and hooks are host resources, excluded from hashing and snapshots. Entropy comes from the OS; token checks
  use the existing cryptographic library's constant-time digest comparison, not custom cryptography.
- A registered session binds the assigned SimId and public controller identity. Authorization runs before
  dispatch and before parked/cached polling results, strips the transport session from legacy argument DTOs,
  and rejects conflicting supplied identities. Notes can be accessed after registering, before taking control.
- Explicit replacement/rotation revokes the old session. Revoked-session errors carry `revoked:true`, which
  prevents transparent MCP reconnect from stealing control back or replaying an accepted mutation. Unknown
  sessions on a restarted/moved host can reconnect once, restoring previously requested control.
- Explorer MCP configuration selects `npcctl mcp --registered`, exposing session-bound tools without unit or
  controller arguments. The legacy default MCP mode stays compatible with unlocked scenarios. The CLI keeps
  old HTTP-bearer `--token` behavior without `--agent-id`; credentialed agent calls distinguish that transport
  lock from the registration credential.
- Explorer host lifecycle starts the assigned actor unattached and control is granted through the existing
  recorded attach command. Generic kernel snapshot loading is unchanged. A fresh registered scenario's
  descriptor omits its old automatic played-actor attachment so its recorded control sequence replays correctly.
- Host-generated scenario data carries an optional pure `world.explorer_context`: played actor, preset ID/full
  immutable preset copy, complete perception configuration, fog and chest-sensing flags (reach comes from the
  preset). The ordinary loader honors this explicit data without authentication resources or filesystem access.
  This makes fresh session records and raw snapshot restoration reproduce host setup, including custom presets
  whose source files were later removed. Existing descriptors with no context retain their original behavior.
- Changing the explorer's initial/control-input sequence naturally changes its state; unchanged inputs and
  unlocked scenarios retain existing hashes. Historical human/scout hash fixtures explicitly restore their
  legacy controller input, preserving every original hash constant and assertion rather than rebaselining.
- Credential creation/migration/rotation and metadata corrections serialize file updates and preserve current
  credentials. Read-only listing/check-save do not migrate files. Tokens appear only in private agent metadata
  and the generated handbook, never prompts, saves, journals, observations, CLI output or Debug representations.

## Consequences

Authentication covers synchronous calls and every asynchronous bypass. Restart recovery and explicit revocation
have different semantics. Generated explorer clients use the new workflow; ordinary scenario integrations and
the authoritative encoding/recording boundary remain compatible.
