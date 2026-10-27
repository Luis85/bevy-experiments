# ADR 0018: Host-side landmarks, remembered positions and file identity

- Status: Accepted (plan Tasks 21–23)
- Date: 2026-10-27
- Source: actor-map design M2–M8

## Context

Actors need places they can remember beyond perception memory. Agents and players must edit the same map
without putting personal files in simulation snapshots, leaking unseen world positions or changing replay
inputs into filesystem references. The shared notes storage also has ID/revision exhaustion and external
edit/rename edge cases.

## Decision

- Landmarks live in owned Obsidian-compatible `map/<name>.md` files. IDs remain stable through rename; there
  is no delete operation. MapStore shares NotesStores' folder mapping, owner and optimistic stamps. Names
  are one line/up to 200 characters, notes up to 8192 UTF-8 bytes; coordinates are validated kernel positions.
- `Here` resolves the actor's current SimPos; `Thing` uses a named generic position-provider hook. Facade
  perception glue supplies only the thing's last stored PerceptionMemory position, including remembered things
  that moved or disappeared. Agent I/O gains no perception dependency or world-truth fallback. Stored positions
  remain fixed. An explicit-null kind patch clears it; an omitted kind leaves it unchanged.
- All six agent/player map methods use the existing notes authorization and writer conventions. Registered
  agents can use their own map before taking control. MCP hides unit/controller/session; the developer CLI
  offers both credentialed agent calls and explicit player operations.
- Observe refreshes the shared host cache and adds up to five nearest landmarks, with stable ID tie-breaking.
  The same path serves regular, delta and act-and-wait observations. Empty/disabled maps add no map field/line.
- A Move order's landmark reference is expanded before any step is submitted. Recorded commands and queued
  action data contain the resolved position only. Unknown/disabled maps reject atomically, and replay works
  after the map folder is removed.
- `SystemsSpec.map` defaults true, including old saved world copies. Disabled maps reject before filesystem
  access and disappear from the handbook, F7 and markers. F7 uses the same store and expected stamps as tools.
  Separate noninteractive UI markers show the actor's saved knowledge through every fog state.
- Shared storage checks ID/revision increments and reports exhaustion rather than wrapping or reusing an ID.
  Raw-byte digests detect equal-length edits with preserved/coarse mtimes. Source-file removal during rename,
  migration and cleanup retries checks the original file identity, retaining a source changed by another editor.

## Consequences

Map writes leave simulation state unchanged. Registration, notes, map, windowed and detached clients share a
single editing model. Navigation and replay remain position-based, while the agent keeps lasting knowledge
in ordinary files. External edits remain visible and stale game saves cannot silently replace newer content.
