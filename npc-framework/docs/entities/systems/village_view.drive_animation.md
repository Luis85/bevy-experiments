---
type: System
---
# `drive_animation` (village view)

**Crate:** `village_view` · **File:** [`content/village_view/src/anim.rs`](../../../content/village_view/src/anim.rs) · **Plugin:** `VillageViewPlugin` (`anim::plugin`)

Picks each villager's animation clip from its sim `Activity` and cross-fades (200 ms) when it changes:
`Idle` → `Idle`, `Walk` → `Walking_A`, `Work` → `Interact`, `Eat` and `Drink` → `Use_Item`, `Sleep` →
`Lie_Idle`. A switch to `Idle` must hold for `IDLE_DEBOUNCE_SECS` (0.15 s real time) first, because the sim is
`Idle` for one tick between two branches. The first time it sees a player it inserts the shared
`AnimationGraphHandle` and `AnimationTransitions`. Its partner `build_graph` builds the shared graph once the
villager glb is loaded (missing clips fall back to `Idle`; no `Idle` clip disables animation).

## Schedule

- **Schedule:** `Update`
- **Set:** `ViewSystems::Animate`
- **Ordering:** `(build_graph, drive_animation).chain()`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `Time`, `VillagerAnimations` (graph); `VisualOf` on the root; sim `Activity` |
| writes | `AnimationPlayer`, `AnimationTransitions`, `VillagerAnimTarget` (`current`, `idle_since`) on view entities; inserts `AnimationGraphHandle` |
| messages / events | none (the `AnimationPlayer` entity is tagged by the `on_villager_ready` observer) |

## Determinism

View-only: the clip comes from sim state only; no gameplay logic, nothing written back. The debounce uses
real time, which is fine in the view.

## Performance

- **Mechanic tag:** `view.anim` (prefix `village_view::anim`) · **Budget:** 200 µs + 10 µs/agent
  (`view.anim`) · no span.

## Related

- [`village_view.spawn_visuals`](village_view.spawn_visuals.md) — spawns the model and its observer.
- [`testbed_view.drive_animation`](testbed_view.drive_animation.md) — the testbed's pose-based version.
- Components: [`Activity`](../components/Activity.md), [`VisualOf`](../components/VisualOf.md)
