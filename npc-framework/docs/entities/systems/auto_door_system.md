---
type: System
---
# `auto_door_system`

**Crate:** `npc_interaction` · **File:** [`crates/npc_interaction/src/door.rs`](../../../crates/npc_interaction/src/door.rs) · **Plugin:** `InteractionPlugin`

The door-on-path behaviour. The nav pass-cost hook `door_pass_cost` lets an [`Interactor`](../components/Interactor.md)
path through a closed door it may open, at `InteractionConfig::door_pass_cost` extra (doubled when locked)
and with a path **marker** on the door cell. This system makes an idle `NavAgent` + `Interactor` whose next
waypoint is such a marker, standing within one cell of the door, start `unlock` (locked door, with the key)
or `open` on it. It runs the normal verb path (`run::begin`, `Source::Auto`), so it is journaled
(`Interacted`) and replays like any interaction. Off when `InteractionConfig::auto_open_doors` is false.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcInteractionSet::Begin` (configured `.after(NpcSet::Think).before(NpcSet::Actions)`); exclusive
  (`auto_door_system(world: &mut World, walkers: &mut DoorWalkerQuery)`, a cached `QueryState` of the idle
  path-following interactors)
- **Ordering:** no other constraint inside the set.

## Reads / writes

| Access | Data |
|---|---|
| reads | `InteractionConfig`, `SimId`, `SimPos`, `npc_nav::Path` (`markers`, `next`), `With<NavAgent>`, `With<Interactor>`, `Without<Interacting>`, `SpatialGrid`, `DoorCells`, `Door`; verb checks via `can_interact` |
| writes | through `run::begin`: inserts `Interacting`, may reserve the door in `Reservations` |
| messages / events | journal `Interacted { outcome: "started" }` (or `rejected`), `Reserve` |

## Determinism

Agents with non-empty path markers are collected as `(SimId, Entity, SimPos, Path)` and sorted by `SimId`;
each one is handled in that order, so of two agents at one door the lower `SimId` claims it first. Cell
distance is integer (`|dx| <= 1 && |dz| <= 1`).

## Performance

- **Mechanic tag:** `interaction` (via `tags.rs` prefix `npc_interaction`) · **Budget:** 25 µs + 2 µs/agent
  (`interaction` row) · no span of its own.

## Related

- [`door_sync_system`](door_sync_system.md) — keeps `DoorCells` and the grid current.
- [`complete_interactions_system`](complete_interactions_system.md) — applies `open` / `unlock` when due.
- [`key_replan_system`](key_replan_system.md) — retries a path once the key is picked up.
- Components: [`Door`](../components/Door.md), [`Interacting`](../components/Interacting.md), [`Interactor`](../components/Interactor.md)
