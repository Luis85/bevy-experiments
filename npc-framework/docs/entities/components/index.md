---
type: Component
---
# Components

One page per main ECS component: its crate and file, purpose, fields with units, who adds, reads and writes it,
whether it is part of the state hash and/or saved in world snapshots ([ADR 0013](../../adr/0013-world-snapshot.md)),
related components and an example. Systems: [systems index](../systems/index.md).

Conventions: positions are integer millimetres (`SimPos`, `i32`), needs are milli-units, time is ticks
(`SimClock.tick`). A component registered with `register_sim_component` is **hashed and saved**; one registered with
`register_snapshot_component_named` is **saved, not hashed**; view components are neither.

## Kernel (`npc_kernel`)

- [`SimId`](SimId.md) — stable id of a sim entity, handed out in spawn order; the key of every sorted iteration.
- [`SimPos`](SimPos.md) — ground-plane position in integer mm; the one sim position.
- [`PrevSimPos`](PrevSimPos.md) — `SimPos` at the start of the tick, for view interpolation; not hashed, not saved.
- [`HostPosition`](HostPosition.md) — the host's position for an entity in Relaxed mode (host-owned movement).
- [`CurrentOrder`](CurrentOrder.md) — a unit's standing kernel order (`GoTo`) from `SimCommand::Order`.
- [`GridBlocker`](GridBlocker.md) — footprint that blocks cells in the spatial grid's dynamic layer.
- [`GridOccluder`](GridOccluder.md) — footprint that blocks line of sight in the dynamic layer.

## Brain (`npc_ai`)

- [`Brain`](Brain.md) — which shared behaviour tree an agent runs; makes an entity an agent.
- [`BrainState`](BrainState.md) — per-node tree memory: the running path and where sequences resume.
- [`ActiveLeaf`](ActiveLeaf.md) — the running leaf and its status; the seam between tree and action system.
- [`Blackboard`](Blackboard.md) — fixed 8-slot store leaves use to pass data (targets, positions).
- [`BrainStats`](BrainStats.md) — debug counters of leaf starts and aborts; saved, not hashed.
- [`ThinkRequest`](ThinkRequest.md) — marker: think this tick even though the leaf is still running.

## Needs and jobs (`npc_needs`, `npc_jobs`)

- [`Needs`](Needs.md) — one `Need { value (milli-units), level }` per registered `NeedDef`.
- [`PauseDecay`](PauseDecay.md) — bit mask pausing the decay of selected needs (e.g. while asleep); saved, not hashed.
- [`Provides`](Provides.md) — smart-object advertisement: restores a need by an amount per tick; also covers `Reservations`.
- [`Approach`](Approach.md) — where an agent stands to use a place; saved, not hashed.

## Locomotion and nav (`npc_locomotion`, `npc_nav`)

- [`MoveIntent`](MoveIntent.md) — the locomotion seam's input: goal, speed (mm/tick), tolerance (mm).
- [`MoveStatus`](MoveStatus.md) — the seam's output: `Idle` / `Moving` / `Arrived` / `Blocked`.
- [`ExternalMover`](ExternalMover.md) — marker: another mover owns this agent, the straight-line mover skips it.
- [`NavAgent`](NavAgent.md) — opt-in to grid A* pathfinding.
- [`NavState`](NavState.md) — `Idle` / `Pending` / `Following` / `Failed` navigation state.
- [`Path`](Path.md) — smoothed grid path: waypoints, pass-cost markers, next index.

## Perception (`npc_perception`)

- [`Sensors`](Sensors.md) — sensor list (vision, hearing, proximity) plus interest mask.
- [`Facing`](Facing.md) — look direction as an integer mm vector; vision cones centre on it.
- [`FixedFacing`](FixedFacing.md) — marker: movement does not turn this agent.
- [`Perceivable`](Perceivable.md) — kind bits that let vision and proximity sense an entity.
- [`PerceptionMemory`](PerceptionMemory.md) — bounded map of percepts by `SimId` plus the last heard noise.
- [`PerceptionFacts`](PerceptionFacts.md) — opt-in mirror of the nearest sensed percept into the blackboard.

## Rules and progression (`npc_rules`, `npc_progression`)

- [`CharacterSheet`](CharacterSheet.md) — point-buy sheet: bought attributes, secondary deltas, traits, skills, unspent points.
- [`Experience`](Experience.md) — total XP and level.
- [`Learning`](Learning.md) — per-skill progress toward free skill points (uses, practice, study, teaching).
- [`SpendPolicy`](SpendPolicy.md) — `Ai` spends level points automatically, `Player` banks them for `SpendPoints`.

## Interaction (`npc_interaction`)

- [`Item`](Item.md) — an item entity: kind, stack, quality; in the world or carried.
- [`Inventory`](Inventory.md) — carry limits (slots, grams) and the sorted list of carried items.
- [`Equipment`](Equipment.md) — named equipment slots, each empty or holding a carried item.
- [`CarriedBy`](CarriedBy.md) — back-link from a carried item to its holder and slot.
- [`Interactor`](Interactor.md) — marker: the unit can use verbs, the `Interact` leaf and doors.
- [`Door`](Door.md) — open / locked state and key; mirrored into the grid's static layer.
- [`Interactable`](Interactable.md) — the verbs a target advertises.
- [`ToolTarget`](ToolTarget.md) — target of `use_tool_on`: required tool, yields, duration, skill check, uses left.
- [`Interacting`](Interacting.md) — a running interaction on its actor.
- [`InteractLeaf`](InteractLeaf.md) — action component of the generic `Interact` BT action.

## Crafting (`npc_crafting`)

- [`Workstation`](Workstation.md) — smart object that hosts recipes of its workstation kinds.
- [`CraftingOrders`](CraftingOrders.md) — work posted on a workstation, in posting order.
- [`Durability`](Durability.md) — tool wear (`current` of `max`); a broken tool despawns.
- [`Crafting`](Crafting.md) — a running craft with its claimed tools and inputs.
- [`CraftLeaf`](CraftLeaf.md) — action component of the generic `Craft` BT action.

## Command (`npc_command`)

- [`Owner`](Owner.md) — which player may order a unit or squad.
- [`Commandable`](Commandable.md) — opts a unit into RTS orders: order speed and arrival radius.
- [`OrderQueue`](OrderQueue.md) — the running order and the shift-queue behind it.
- [`ExecutingOrder`](ExecutingOrder.md) — marker: the tree runs the `ExecuteOrder` leaf, so the engine steps this unit.
- [`Squad`](Squad.md) — a squad entity: sorted members and formation.
- [`MemberOf`](MemberOf.md) — the squad a unit belongs to (authoritative side).

## Vehicles (`npc_vehicles`)

- [`Vehicle`](Vehicle.md) — seats, speed and control mode (`ByCrew` / `Direct`).
- [`Occupants`](Occupants.md) — the vehicle's seat slots (free / claimed / taken).
- [`MountedIn`](MountedIn.md) — rider side: which vehicle and seat an actor sits in.
- [`SeatPreference`](SeatPreference.md) — preferred seat role when a mount names none.
- [`DriveGoal`](DriveGoal.md) — destination for the vehicle a unit drives.
- [`Driving`](Driving.md) — action component of the `DriveVehicle` BT action.

## Agent control (`npc_agent_io`)

- [`ExternalController`](ExternalController.md) — the external agent attached to a unit: id, control mode, idle policy.
- [`ExternalQueue`](ExternalQueue.md) — a `Direct` unit's queue of external actions.

## Facade glue (`npc_framework::glue`)

- [`InteractOrder`](InteractOrder.md) — command × interaction: a running `Interact` / `Mount` order's verb, target and phase.
- [`Stealthy`](Stealthy.md) — perception × rules: penalty to the observer's Per roll in the `per_roll` check.
- [`Teaches`](Teaches.md) — progression × interaction: something to study (target of `study`).
- [`Teacher`](Teacher.md) — progression × interaction: a unit that teaches (actor of `teach`).

## Village content (`village_content`)

- [`Villager`](Villager.md) — the villager marker: home and walking speed; saved, not hashed.
- [`Place`](Place.md) — marks a village smart object and its kind; saved, not hashed.
- [`Home`](Home.md) — a house villagers live in; saved, not hashed.
- [`Bed`](Bed.md) — a bed of one home; saved, not hashed.
- [`WorkSite`](WorkSite.md) — a field: shift length, food yield, target stockpile; saved, not hashed.
- [`Well`](Well.md) — drinking water with a number of slots; saved, not hashed.
- [`Stockpile`](Stockpile.md) — the food store (food, capacity).
- [`Activity`](Activity.md) — what a villager is visibly doing; drives the view's animation.
- [`OrderMove`](OrderMove.md) — marker: the running walk belongs to an order.
- [`MoveTo`](MoveTo.md) — action component: a running walk.
- [`Work`](Work.md) — action component: a running work shift.
- [`Eat`](Eat.md) — action component: a running meal at a stockpile.
- [`Drink`](Drink.md) — action component: a running drink at a well.
- [`Sleep`](Sleep.md) — action component: asleep in a bed.
- [`Idle`](Idle.md) — action component: standing still for a drawn number of ticks.

## Actor Testbed content (`testbed_content`)

- [`TestbedUnit`](TestbedUnit.md) — a testbed actor: sheet template, behaviour (tree), speed, home, wander radius.
- [`Prop`](Prop.md) — a fixed thing of the compound (walls, doorways, trees, ore, workstations, vehicles).
- [`TbIdling`](TbIdling.md) — action component of `TbIdle`.
- [`TbWalking`](TbWalking.md) — action component of `TbWalk`.

## View (never hashed, never saved)

- [`VisualOf`](VisualOf.md) — `npc_view_helpers`: link from a visual root to its sim entity (+ `VisualIndex`).
- [`Selectable`](Selectable.md) — `npc_view_helpers`: a visual root that can be click-picked.
- [`VillagerVisual`](VillagerVisual.md) — `village_view`: smoothed facing and interpolation endpoints.
- [`PlaceVisual`](PlaceVisual.md) — `village_view`: marker on a place's visual root.
- [`ActorVisual`](ActorVisual.md) — `testbed_view`: model, interpolation, facing and hand bones of an actor.
- [`VehicleVisual`](VehicleVisual.md) — `testbed_view`: wheel spin state of a cart or wagon.
- [`DoorVisual`](DoorVisual.md) — `testbed_view`: door leaf swing angle and hinge.
