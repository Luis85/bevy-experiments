---
type: System
---
# Systems

One page per main system: its crate and file, schedule and set (with ordering constraints), what it reads and
writes, determinism notes, the perf mechanic it is tagged with (`crates/npc_perf/src/tags.rs`, budgets in
`NPC_BUDGET_TABLE`, [debugging.md §7.2](../../debugging.md#72-mechanics-and-budgets)) and related systems.
Components: [components index](../components/index.md).

## A tick in order

`NpcSimTick` runs the chained sets `Inputs → Needs → Sense → Think → Actions → Locomotion → Effects → Cleanup`
(`crates/npc_kernel/src/schedule.rs`). Capability sub-sets sit between them as shown. The host runs the schedule
through [`drive_sim`](drive_sim.md).

| Phase | Systems |
|---|---|
| `NpcSet::Inputs` | [`apply_commands_system`](apply_commands_system.md) → [`apply_host_positions_system`](apply_host_positions_system.md) → [`copy_prev_pos_system`](copy_prev_pos_system.md) → [`spatial_update_system`](spatial_update_system.md) → [`door_sync_system`](door_sync_system.md) |
| `AgentIoSet::Dispatch` (after Inputs, before Needs) | [`external_dispatch_system`](external_dispatch_system.md) |
| `NpcSet::Needs` | [`decay_needs_system`](decay_needs_system.md) → [`day_night_system`](day_night_system.md) → [`rethink_timer_system`](rethink_timer_system.md) |
| `NpcSet::Sense` (`NpcPerceptionSet`) | [`perception_system`](perception_system.md) → [`flush_per_roll_log`](flush_per_roll_log.md) |
| `NpcCommandSet::Execute` (after Sense, before Think) | [`command_engine_system`](command_engine_system.md) |
| `NpcSet::Think` | [`think_system`](think_system.md) |
| `NpcInteractionSet::Begin` (after Think, before Actions) | [`auto_door_system`](auto_door_system.md) |
| `NpcSet::Actions` | village: [`order_system`](order_system.md) → [`move_to_system`](move_to_system.md) → [`eat_system`](eat_system.md) → [`drink_system`](drink_system.md) → [`work_system`](work_system.md) → [`sleep_system`](sleep_system.md) → [`idle_system`](village_content.idle_system.md); testbed: [`walk_cancel_system`](walk_cancel_system.md) → [`idle_system`](testbed_content.idle_system.md) |
| `NpcVehicleSet::Drive` → `Gate` (after Actions, before Locomotion) | [`drive_vehicle_system`](drive_vehicle_system.md) → [`vehicle_gate_system`](vehicle_gate_system.md) |
| `NpcSet::Locomotion` (`NpcMoverSet`) | [`straight_line_mover_system`](straight_line_mover_system.md) → [`nav_plan_system`](nav_plan_system.md) → [`grid_path_mover_system`](grid_path_mover_system.md) |
| `NpcSet::Locomotion`, after the movers | [`move_arrival_system`](move_arrival_system.md) (village), [`walk_arrival_system`](walk_arrival_system.md) (testbed) |
| `NpcVehicleSet::Crew` (after Locomotion) | [`seat_follow_system`](seat_follow_system.md) → [`claim_seats_system`](claim_seats_system.md) |
| `NpcInteractionSet::Complete` (after Locomotion, before Effects) | [`complete_interactions_system`](complete_interactions_system.md) → [`key_replan_system`](key_replan_system.md) |
| `NpcCraftingSet::Complete` (after interaction Complete, before Effects) | [`crafting_complete_system`](crafting_complete_system.md) |
| `NpcSet::Effects` | [`resolve_effects_system`](resolve_effects_system.md) (village) |
| `AgentIoSet::Advance` (after Effects, before Cleanup) | [`external_advance_system`](external_advance_system.md) |
| `NpcSet::Cleanup` | [`facing_from_movement_system`](facing_from_movement_system.md), [`progression_system`](progression_system.md) → [`flush_journal_system`](flush_journal_system.md) → [`advance_clock_system`](advance_clock_system.md) |

Arrows show ordering constraints that exist in code; systems listed with a comma are unordered against each other.
Each page states the exact constraints.

## Kernel (`npc_kernel`)

- [`drive_sim`](drive_sim.md) — host-safe driver (ADR 0008): turns host time into whole ticks and runs `NpcSimTick`.
- [`apply_commands_system`](apply_commands_system.md) — applies due `SimCommand`s in `(tick, seq)` order; first in `Inputs`.
- [`apply_host_positions_system`](apply_host_positions_system.md) — Relaxed mode only: `SimPos = HostPosition`.
- [`copy_prev_pos_system`](copy_prev_pos_system.md) — `PrevSimPos = SimPos` before anything moves.
- [`spatial_update_system`](spatial_update_system.md) — keeps the `SpatialGrid` index and dynamic layer current; writes `GridChanged`.
- [`day_night_system`](day_night_system.md) — on day/night transitions, every brain gets a `ThinkRequest`.
- [`rethink_timer_system`](rethink_timer_system.md) — staggered periodic `ThinkRequest` per brain.
- [`think_system`](think_system.md) — runs the `npc_ai::think` BT runtime over every brain in `SimId` order.
- [`flush_journal_system`](flush_journal_system.md) — flushes the journal sink once per tick.
- [`advance_clock_system`](advance_clock_system.md) — `SimClock.tick += 1`, last in every tick.

## Needs (`npc_needs`)

- [`decay_needs_system`](decay_needs_system.md) — decays needs, updates levels with hysteresis, requests a re-think on level change.

## Locomotion and nav (`npc_locomotion`, `npc_nav`)

- [`straight_line_mover_system`](straight_line_mover_system.md) — default mover: steps `SimPos` toward the `MoveIntent` goal.
- [`nav_plan_system`](nav_plan_system.md) — exclusive A* planner: requests, replans, serves under `NavBudget`.
- [`grid_path_mover_system`](grid_path_mover_system.md) — moves nav agents along their `Path`.

## Perception (`npc_perception`)

- [`perception_system`](perception_system.md) — drains noises, hearing, staggered vision/proximity scans, memory and facts.
- [`facing_from_movement_system`](facing_from_movement_system.md) — an agent that moved faces its movement direction.

## Progression (`npc_progression`)

- [`progression_system`](progression_system.md) — drains the rules roll feed and applies uses, practice and roll XP.

## Interaction (`npc_interaction`)

- [`door_sync_system`](door_sync_system.md) — mirrors every `Door` into the grid's static layer.
- [`auto_door_system`](auto_door_system.md) — idle nav agents open or unlock the door ahead on their path.
- [`complete_interactions_system`](complete_interactions_system.md) — finishes due interactions in `SimId` order.
- [`key_replan_system`](key_replan_system.md) — retries a failed path once the agent carries a locked door's key.

## Crafting (`npc_crafting`)

- [`crafting_complete_system`](crafting_complete_system.md) — completes due crafts: roll, consume, wear, outputs, hooks.

## Command (`npc_command`)

- [`command_engine_system`](command_engine_system.md) — the RTS order engine: prunes squads, steps units' orders in `SimId` order.

## Vehicles (`npc_vehicles`)

- [`drive_vehicle_system`](drive_vehicle_system.md) — drivers steer their vehicles; arrival, failure, stall.
- [`vehicle_gate_system`](vehicle_gate_system.md) — drops lost riders, frees stale seats, halts riders before the movers.
- [`seat_follow_system`](seat_follow_system.md) — after the movers, puts each rider on its seat.
- [`claim_seats_system`](claim_seats_system.md) — claims seats for mounts in progress (lower `SimId` wins).

## Agent control (`npc_agent_io`)

- [`external_action_system`](external_action_system.md) — the shared body of the `ExternalAction` leaf (not scheduled itself).
- [`external_dispatch_system`](external_dispatch_system.md) — `Pass::Dispatch`, right after `Inputs`.
- [`external_advance_system`](external_advance_system.md) — `Pass::Advance`, after `Effects`; counts `Wait` steps down.

## Facade glue (`npc_framework::glue`)

- [`flush_per_roll_log`](flush_per_roll_log.md) — perception × rules: journals the Per rolls of the `per_roll` check.

## Village content (`village_content`)

- [`order_system`](order_system.md) — keeps an order walk in sync with the standing kernel order.
- [`move_to_system`](move_to_system.md) — stops a walk whose leaf is no longer running.
- [`eat_system`](eat_system.md) — runs meals at a stockpile.
- [`drink_system`](drink_system.md) — runs drinks at a well.
- [`work_system`](work_system.md) — runs work shifts; yields food via an effect.
- [`sleep_system`](sleep_system.md) — runs sleepers: adds energy per tick until rested.
- [`village_content.idle_system`](village_content.idle_system.md) — counts `Idle` down to `Success`.
- [`move_arrival_system`](move_arrival_system.md) — ends a walk from `MoveStatus` after the movers.
- [`resolve_effects_system`](resolve_effects_system.md) — applies cross-entity `SimEffect`s sorted by `(source, kind)`.

## Actor Testbed content (`testbed_content`)

- [`walk_cancel_system`](walk_cancel_system.md) — stops a walk whose leaf is no longer running.
- [`testbed_content.idle_system`](testbed_content.idle_system.md) — counts `TbIdling` down to `Success`.
- [`walk_arrival_system`](walk_arrival_system.md) — ends a walk from `MoveStatus` after the movers.

## View helpers (`npc_view_helpers`, `Update`)

- [`interp_alpha`](interp_alpha.md) — the interpolation factor between ticks (helper fn, not a system).
- [`click_select`](click_select.md) — single-unit click selection.
- [`command_input`](command_input.md) — RTS command UI: selection, control groups, recorded `IssueOrder`s.
- [`journal_panel::update_panel`](npc_view_helpers.journal_panel.update_panel.md) — F3 journal panel.
- [`perf_panel::update_panel`](npc_view_helpers.perf_panel.update_panel.md) — F4 perf panel.

## Village view (`village_view`, `Update`)

- [`spawn_visuals`](village_view.spawn_visuals.md) — spawns and despawns a visual root per village sim entity.
- [`villager_transforms`](villager_transforms.md) — interpolates villager (and place, sack) transforms between ticks.
- [`drive_animation`](village_view.drive_animation.md) — picks the animation clip from `Activity`, cross-fades.
- [`rts_camera`](village_view.rts_camera.md) — the village orbit camera.
- [`update_text`](village_view.update_text.md) — F1 debug overlay text and gizmos.

## Testbed view (`testbed_view`, `Update`)

- [`spawn_visuals`](testbed_view.spawn_visuals.md) — spawns a visual root per prop, item and actor.
- [`actor_transforms`](actor_transforms.md) — interpolates actors, vehicles, items and door swings.
- [`drive_animation`](testbed_view.drive_animation.md) — picks the animation from a `Pose` read from sim state.
- [`rts_camera`](testbed_view.rts_camera.md) — the testbed orbit camera (tilt, focus).
- [`update_text`](testbed_view.update_text.md) — F1 overlay text.
- [`sheet_text`](sheet_text.md) — F5 character sheet panel.
- [`perception_text`](perception_text.md) — F6 perception panel and gizmos.
- [`explorer_panel::refresh`](testbed_view.explorer_panel.refresh.md) — F7 notes editor (explorer mode): diary, memory and proposals, listed and edited as the player.
- [`order_keys`](order_keys.md) — RTS hotkeys: formations and orders on top of the command UI.
