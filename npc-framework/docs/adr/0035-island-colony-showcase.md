# 0035 — One island showcase, resident policy and recorded colony editing

Date: 2026-11-08. Status: accepted.

Superseded in part by [ADR 0044](0044-island-crates-split.md): `island_colony` is removed; the island lives in `island_content`, `island_view` and `island_game`.

## Context

The user wants Island Village to be the playable showcase, with autonomous production,
housing and social life, inspectable/editable NPC decisions, a playable character and
terrain editing. Existing compact and expanded scenarios have protected replay contracts.

## Decision

Add the opt-in `island_colony` scenario and preset. `WorldPreset.colony` and its optional
`island_ai` policy default to absent/false, preserving old construction and registrations.
Generic capability crates remain unaware of island content. The launcher presents modes
of this one world; historical scenarios remain available for development and replay.

Resident routine priorities and thresholds are validated JSON data. Critical survival and
recorded external orders remain ahead of the authored routine. A bounded deterministic
forward planner searches actual inventory, cash, finite sources and registered recipes for
funded production goals. Every chosen step uses existing physical actions and validates
again. The policy is hashed and saved; planning explanations are read-only.

Households have finite beds and a real rest destination. Social state changes through
physical proximity; civic policy cannot mint goods or currency. Housing commissions feed
the existing funded production/procurement model and shared cottage construction actions.

The initial visitor is a stable identity awaiting arrival, with no `Villager`, `Needs` or
`Vitality`. The recorded `JoinIsland` command starts that life once. Returning to inspector
mode releases native control without resetting or freezing a living participant. Existing
permanent death and successor rules remain in force.

Terrain is a registered integer resource with a matching static navigation layer. Recorded
absolute brush commands validate map bounds, surfaces, occupancy, protected worksites,
home entrances and connected passable land, then apply atomically. View meshes, ray picking
and camera modes never write terrain or actor state directly.

The local launcher shares one save across inspector, character and editor modes. The host
uses atomic snapshots, immutable world/actor construction data, strict compatibility and an
OS file lock. It saves periodically, on F9 and on graceful close. Unreadable/incompatible
saves are preserved. AI explorer sessions retain their existing separate host lifecycle.

## Evidence

The [island architecture guide](../architecture-island.md) records the context owners,
dependency direction and extension contracts. Resident interruption uses one shared
work teardown and cancel/replan semantics. Native control has one coordinator shared
by the ledger and character panel. Save integrity belongs to content; file ownership
and graceful shutdown belong to the host. The launcher requires a saved acknowledgement
before treating a local island as closed.

Focused tests cover authored priority/emergency ordering, real planner preconditions,
observer purity, currency conservation, late production, household capacity, physical
social contact, safe terrain rejection, deterministic command execution and save/resume.
Native verification and the complete gate are recorded in the iteration validation note.
