# ADR 0008: The host owns the schedule, clock, input and movement

- Status: Accepted (P3 implemented the driver, `SimControl`, `DeterminismMode` and the host-position adapter; actor-capabilities C1 implemented the locomotion seam: `npc_locomotion`, `NpcSet::Locomotion`)
- Date: 2026-10-23
- Source: baseline-hardening spec §2 and §4, `docs/research/framework-best-practices.md` §0.3

## Context

Iteration 1 drives `SimTick` from `FixedUpdate` with `Time::<Fixed>::from_hz(20.0)` and binds Space and 1–4
in the view. A host game has its own fixed timestep (e.g. physics at 64 Hz), its own input map and its own
movement (Avian, navmesh, flow fields). A library that rewrites `Time<Fixed>`, binds keys or moves units
itself cannot be dropped into such a game.

## Decision

- `NpcPlugins::new().tick_hz(n).run_in(<ScheduleLabel>)`: the kernel keeps its own accumulator (`SimTime`)
  and runs `NpcSimTick` from one exclusive driver system in the schedule the host chooses. It never writes
  `Time<Fixed>` (pinned by `host_fixed_timestep_untouched`).
- Pause and speed are a kernel resource (`SimControl`); keyboard bindings live in apps.
- Public labels are prefixed (`NpcSimTick`, `NpcSet::*`) and public sets are the ordering seams. Resources
  use `init_resource` and never overwrite host-owned ones. Ambiguity = Error applies only to our schedule.
- Movement is a seam: actions write `MoveIntent { goal, speed_mm_per_tick, tolerance_mm }` and read
  `MoveStatus { Idle, Moving, Arrived, Blocked }`. `npc_locomotion` ships a deterministic
  `StraightLineMover`; hosts plug their adapters into `NpcSet::Locomotion`.
- `DeterminismMode::{Strict, Relaxed}`: Strict (the default) keeps integer positions, ambiguity Error and
  hash checks; Relaxed lets a host provide positions (e.g. from physics) through an adapter.

## Consequences

- The framework coexists with host physics and input at any tick rate.
- In Relaxed mode replays are only as deterministic as the host's mover; Strict stays lockstep-ready.
- Actions such as `MoveToDest` change from moving `SimPos` themselves to waiting on `MoveStatus`.
- Placement (C1): `NpcSet::Actions → NpcSet::Locomotion → NpcSet::Effects`; movers run in `NpcMoverSet` inside
  `Locomotion`, and actions that wait on `MoveStatus` check it `.after(NpcMoverSet)` in the same set, so a walk
  still ends in the tick it arrives. `MoveIntent`/`MoveStatus` (and the spatial grid) are hashed only when a
  game or capability registers them (`register_locomotion_hash`, `register_spatial_hash`; `npc_nav` does):
  registering a name changes the content hash, and the village's hashed `move_to` already carries the goal, so
  the village keeps its hashes and replays.
