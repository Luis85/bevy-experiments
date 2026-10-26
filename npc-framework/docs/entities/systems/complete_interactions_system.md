---
type: System
---
# `complete_interactions_system`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/run.rs`](../../../crates/npc_interaction/src/run.rs) · **Plugin:** `InteractionPlugin`

Finishes every interaction whose duration is over (`Interacting::ends <= SimClock.tick`) by calling
`run::complete` on its actor. `complete` removes [`Interacting`](../components/Interacting.md), checks the
verb again (`can_interact`), rolls the skill check through the skill-check hook (no hook or no check:
pass; the facade's `interaction_rules` glue rolls with `npc_rules`, purpose `interaction.<verb>`), applies
the verb's effect, makes its noise through the noise hook (the `interaction_perception` glue emits a
perception `Noise`), releases the target claim, journals the outcome and calls the finished hooks. Outcomes:
`success`; `failed` (check failed, time spent, no effect); `invalid` (the world changed, with the reason).
For `Source::Tree` it writes `Success`/`Failure` into the actor's `ActiveLeaf` when [`InteractLeaf`](../components/InteractLeaf.md) is present.

This is where every verb's effect lands: item moves (`pick_up`, `drop`, `give`, `equip`, `unequip`), door
changes, `use_tool_on` yields, and `npc_vehicles`' `mount` / `dismount` (seat the actor, unseat it).

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcInteractionSet::Complete` (configured `.after(NpcSet::Locomotion).before(NpcSet::Effects)`);
  exclusive (`complete_interactions_system(world: &mut World, actors: &mut InteractingQuery)`, a cached
  `QueryState<(&SimId, Entity, &Interacting)>`)
- **Ordering:** `(complete_interactions_system, key_replan_system).chain()`. Also after `NpcVehicleSet::Crew`
  (that set is `.before(NpcInteractionSet::Complete)`) and before `NpcCraftingSet::Complete`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimClock`, `SimId`, `Interacting`, `InteractionRegistry`, `InteractionHooks`, `InteractionListeners`, `InteractionConfig`, `Reservations`, plus whatever each verb reads |
| writes | removes `Interacting`; `ActiveLeaf.status`; `Reservations` (release); each verb's effect (`Item`, `Inventory`, `Equipment`, `CarriedBy`, `SimPos`, `Door` + grid, `ToolTarget.remaining`, spawned items, `MountedIn`/`Occupants`) |
| messages / events | journal `Interacted { verb, target, item, outcome, reason }`, `ItemMoved`, `DoorChanged`, `Release`, `Mounted`/`Dismounted`, `Roll` (rules glue), `Spawn`; perception `Noise` (glue) |

## Determinism

Due actors are collected as `(SimId, Entity)` and sorted by `SimId`, then completed one by one, so effects
on shared targets apply in id order. Reach uses integer `dist2`; checks use the counter-based rules roll.

## Performance

- **Mechanic tag:** `interaction` (prefix `npc_interaction`) · **Budget:** 25 µs + 2 µs/agent (`interaction`)
  · opens `info_span!("mechanic", name = "interaction")` (nested in the system's own `interaction`-tagged span, so `PerfLayer` does not time it a second time).

## Related

- [`key_replan_system`](key_replan_system.md) (runs right after), [`auto_door_system`](auto_door_system.md),
  [`claim_seats_system`](claim_seats_system.md), [`crafting_complete_system`](crafting_complete_system.md).
- Components: [`Interacting`](../components/Interacting.md), [`InteractLeaf`](../components/InteractLeaf.md), [`Item`](../components/Item.md), [`Door`](../components/Door.md), [`ToolTarget`](../components/ToolTarget.md)
