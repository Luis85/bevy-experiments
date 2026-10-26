---
type: System
---
# `drive_animation` (testbed view)

**Crate:** `testbed_view` · **File:** [`content/testbed_view/src/anim.rs`](../../../content/testbed_view/src/anim.rs) · **Plugin:** `TestbedViewPlugin` (`anim::plugin`)

Picks each actor's animation from a `Pose` read from sim state, and cross-fades (200 ms) when it changes.
`pose_of(mounted, interacting, crafting, moving)`: `MountedIn` → `Ride`; an `Interacting` verb →
`use_tool_on` = `Work`, `pick_up` = `PickUp`, others `Interact`; `Crafting` → `Craft`; moved during the
latest tick (`ActorVisual.motion.moving`) → `Walk`; else `Idle`. `Ride`, `Walk` and `Work` play at once;
any other new pose must hold `DEBOUNCE_SECS` (0.15 s real time) before it plays. Clips per model kind (skeletons walk with `Walking_D_Skeletons`). Its
partner `build_graphs` builds one graph per `ModelKind` once that glb is loaded.

## Schedule

- **Schedule:** `Update`
- **Set:** `ViewSystems::Animate`
- **Ordering:** `(build_graphs, drive_animation).chain()`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `Time`, `ActorAnimations`; root `VisualOf` + `ActorVisual`; sim `MountedIn`, `Interacting`, `Crafting` |
| writes | `AnimationPlayer`, `AnimationTransitions`, `ActorAnimTarget` (`current`, `pending_since`) on view entities; inserts `AnimationGraphHandle` |
| messages / events | none (the player entity is tagged by the `on_actor_ready` observer) |

## Determinism

View-only: the pose comes from sim state only; nothing is written back.

## Performance

- **Mechanic tag:** `view.anim` (prefix `testbed_view::anim`) · **Budget:** 200 µs + 10 µs/agent
  (`view.anim`) · no span.

## Related

- [`actor_transforms`](actor_transforms.md) — sets `motion.moving`.
- [`village_view.drive_animation`](village_view.drive_animation.md) — the village's `Activity`-based version.
- Components: [`ActorVisual`](../components/ActorVisual.md), [`MountedIn`](../components/MountedIn.md)
