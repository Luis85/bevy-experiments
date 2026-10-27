# ADR 0017: Immutable world presets and optional explorer mechanics

- Status: Accepted (plan Tasks 15–17)
- Date: 2026-10-27
- Source: world-preset goal G12 and the implementation plan

## Context

Explorer worlds need editable needs, perception, day/night, systems and inhabitants without changing
ordinary testbed scenarios or existing replay/hash fixtures. Editing a preset must not mutate a running or
saved world. Existing installed actor presets also predate survival/recovery verbs.

## Decision

- World presets are strict, validated JSON files. New selects both actor and world; actor validation uses the
  chosen world's detection rules. Saves carry the world ID and a complete immutable copy. Resume and reset
  use saved construction data, independent of later source edits or deletion.
- Explicit explorer scenario context is the shared construction path. The ordinary loader prepares optional
  definitions before content freezes and applies runtime setup after actors spawn. Hosts do not repeat those
  hooks. Snapshot loading restores live mechanics state without reinitializing needs or inhabitants.
- Initial inhabitant patrols are validated construction state, applied before the world starts. Session records
  contain actual external inputs rather than a second copy of factory-created patrol commands.
- Plain explorer preserves every existing registration, entity and hash fixture. Its existing 2,400-tick,
  30%-night clock remains enabled with 100% night vision; that clock already exists in the explorer. Optional
  needs definitions and additional entities appear only when selected settings require them. All additional
  runtime state is registered for snapshots only.
- Need configuration describes rising deprivation (0 to 100,000); generic need satisfaction is its complement.
  Humans use hunger/thirst/energy; robots use battery. Survival enables both sets for the corresponding body.
  Slowing and collapse affect ongoing movement, not only newly accepted orders. Night sensing derives from
  stored base ranges on each transition; vision/camera scale, while LIDAR/radar retain their ranges.
- Active-world eat/drink/rest/charge verbs are intrinsic for the matching body and source, so installed actor
  files and saved copies need no whitelist migration. The world still gates them. Exhausted actors may only
  rest/charge at a suitable source until recovery; hunger/thirst exhaustion requires external rescue rather
  than an undocumented automatic refill.
- World rules are described in the handbook using MCP tools. Assigned character details remain in
  `enter_world`, as required by registration. Regenerating the handbook after token rotation retains the saved
  world rules.

## Consequences

The menu, detached host, native host and generic replay loader share construction semantics. Plain and
unrelated scenarios remain compatible; optional mechanics remain inspectable and resumable. Runtime changes
are explicit saved data, and editing a preset affects new worlds only.
