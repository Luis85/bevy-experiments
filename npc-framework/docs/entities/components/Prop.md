---
type: Component
---
# `Prop`

**Crate:** `testbed_content` · **File:** [`content/testbed_content/src/world.rs`](../../../content/testbed_content/src/world.rs) · **Capability:** testbed content

A fixed thing of the testbed compound: a wall run, a doorway, a tree, the ore rock, a workstation, a chest
or a vehicle. The behaviour lives in capability components on the same entity (`GridBlocker`, `Door`,
`ToolTarget`, `Workstation`, `Vehicle`); `Prop` says what it is so the sim can name it and the view can
pick a model.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `kind` | `PropKind` | `Wall` (its cells are the `GridBlocker` footprint), `Doorway` (holds a `Door`), `Tree`, `OreRock`, `Workbench`, `Anvil`, `Forge`, `Chest`, `Cart` (`ByCrew`), `Wagon` (`Direct`) |
| `yaw_deg` | `i32` | facing for the view, whole degrees about +Y; the sim never reads it (`Prop::new` sets 0) |

## Lifecycle

- **Added by:** the compound builder in [`layout.rs`](../../../content/testbed_content/src/layout.rs) (walls,
  doorways, trees, ore, stations, chests, cart and wagon).
- **Read by:** `kind_name` (`PropKind::name`: `"wall"`, `"doorway"`, `"tree"`, `"ore_rock"`, ..; sorts before
  the capability namers, so a cart is a `"cart"`) and `ascii_map` in `layout.rs`; the `walls` agent section
  ([`walls.rs`](../../../content/testbed_content/src/walls.rs): `Wall` runs and `Doorway` gaps); the view:
  [`testbed_view.spawn_visuals`](../systems/testbed_view.spawn_visuals.md) (model per kind, `yaw_deg`),
  [`actor_transforms`](../systems/actor_transforms.md) (seat offsets per vehicle kind), the F1 overlay's
  item markers (`Without<Prop>`).
- **Written / removed by:** nothing changes it after spawn.

## Hashing and snapshots

- **State hash:** yes, as `testbed_prop` (`kind` tag `u8`, `yaw_deg` `i32`; `register_sim_component` in
  `TestbedPlugin`, [`lib.rs`](../../../content/testbed_content/src/lib.rs)).
- **World snapshot (ADR 0013):** saved as `testbed_prop` (automatic with hashing).

## Related

- [`GridBlocker`](GridBlocker.md), [`Door`](Door.md), [`ToolTarget`](ToolTarget.md) — the capability data
  props carry.
- [`DoorVisual`](DoorVisual.md), [`VehicleVisual`](VehicleVisual.md) — their visuals.

## Example

```rust
// layout.rs: a chest, then look it up by kind.
let (_id, e) = spawn_sim(world, (cell(9, 8), Prop::new(PropKind::Chest)));
assert_eq!(world.get::<Prop>(e).map(|p| p.kind.name()), Some("chest"));
```
