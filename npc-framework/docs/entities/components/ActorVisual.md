---
type: Component
---
# `ActorVisual`

**Crate:** `testbed_view` · **File:** [`content/testbed_view/src/visuals.rs`](../../../content/testbed_view/src/visuals.rs) · **Capability:** testbed view

`pub(crate)` state on a testbed actor's visual root: which model it uses, its interpolation and facing
state, and the hand bones items attach to.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `kind` | `ModelKind` | model (Knight, Rogue, skeleton minion/warrior), from `ModelKind::for_actor(template, owner)` |
| `motion` | `Motion` | `yaw` (radians), private `from`/`to` (metres) and `tick`, and `moving` (the sim entity moved during the latest tick) |
| `hand_r`, `hand_l` | `Option<Entity>` | hand bone entities, found when the model's scene is ready; `None` before |

## Lifecycle

- **Added by:** [`testbed_view.spawn_visuals`](../systems/testbed_view.spawn_visuals.md) for every sim entity
  with `TestbedUnit` + `SimPos` (with `VisualOf`, `Selectable`). Children: the model (`ActorModel { root }`,
  observer `on_actor_ready`, which adds `ActorAnimTarget` to the `AnimationPlayer` entity) and a faction ring
  coloured by `Owner`.
- **Written by:** [`actor_transforms`](../systems/actor_transforms.md) (`motion`, `Transform`); `on_actor_ready`
  (hand bones).
- **Read by:** [`testbed_view.drive_animation`](../systems/testbed_view.drive_animation.md) (`motion.moving`
  feeds `pose_of`), `item_transforms` (hand bones),
  [`testbed_view.update_text`](../systems/testbed_view.update_text.md) (pose line).
- **Removed by:** `despawn_visuals` when the actor leaves the sim.

## Hashing and snapshots

- **State hash:** no. View components are never hashed.
- **World snapshot (ADR 0013):** not saved (view-only; rebuilt from sim state).

## Related

- [`TestbedUnit`](TestbedUnit.md) — the sim component it draws.
- [`VisualOf`](VisualOf.md), [`Selectable`](Selectable.md) — on the same root.
- [`VehicleVisual`](VehicleVisual.md) — riders are placed on their vehicle's visual.

## Example

```rust
// testbed_view: a view system reading the pose of every actor.
for (of, actor) in &roots {
    let (m, i, c) = sims.get(of.0)?; // MountedIn, Interacting, Crafting
    let pose = pose_of(m, i, c, actor.motion.moving);
}
```
