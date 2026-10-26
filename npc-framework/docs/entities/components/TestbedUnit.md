---
type: Component
---
# `TestbedUnit`

**Crate:** `testbed_content` · **File:** [`content/testbed_content/src/world.rs`](../../../content/testbed_content/src/world.rs) · **Capability:** testbed content

A testbed actor: the sheet template it was made from, its idle behaviour, its speed and the home spot it
wanders around. Everything else on an actor is capability data (`CharacterSheet`, `Sensors`, `Inventory`,
`Commandable`, `NavAgent`, ..).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `template` | `String` | sheet template name (`content::TEMPLATES`); the view picks the model from it |
| `behaviour` | `Behaviour` | `Idle` (default), `Wander`, `Worker`; chooses the tree (`testbed_idle` / `testbed_wander` / `testbed_worker`) at spawn |
| `speed_mm_per_tick` | `i32` | walking speed, mm per tick (scenario default `UNIT_SPEED`) |
| `home` | `SimPos` | spawn position, mm; the wander centre |
| `wander_radius_mm` | `i32` | wander spots lie within this many mm of `home` per axis |

## Lifecycle

- **Added by:** the actor spawner in [`layout.rs`](../../../content/testbed_content/src/layout.rs) (scenario
  `actors`), with `Brain { tree: behaviour.tree_name() }`, sheet, sensors, inventory, `Commandable`, `Owner`,
  `NavAgent`.
- **Read by:** `walk_start` (`speed_of`, at least 1) and `wander_spot` in
  [`actions.rs`](../../../content/testbed_content/src/actions.rs); the `testbed` agent section in
  [`agent.rs`](../../../content/testbed_content/src/agent.rs); `kind_name` (`"actor"`) and `ascii_map`; the
  view: [`testbed_view.spawn_visuals`](../systems/testbed_view.spawn_visuals.md) (`ModelKind::for_actor`),
  [`testbed_view.update_text`](../systems/testbed_view.update_text.md), `inspect_target` in
  [`command.rs`](../../../content/testbed_view/src/command.rs).
- **Written / removed by:** nothing changes it after spawn.

## Hashing and snapshots

- **State hash:** yes, as `testbed_unit` (`template` as str, `behaviour` tag `u8`, speed `i32`, `home`,
  radius `i32`; `register_sim_component` in `TestbedPlugin`,
  [`lib.rs`](../../../content/testbed_content/src/lib.rs)).
- **World snapshot (ADR 0013):** saved as `testbed_unit` (automatic with hashing).

## Related

- [`TbWalking`](TbWalking.md), [`TbIdling`](TbIdling.md) — its own actions.
- [`ActorVisual`](ActorVisual.md) — its visual root.
- [`Commandable`](Commandable.md), [`NavAgent`](NavAgent.md) — capability data spawned with it.

## Example

```rust
// content/testbed_content/scenarios/testbed.json, an actor entry:
// { "kind": "unit", "name": "smith", "template": "smith", "owner": 1, "pos": [-1500, 500], "behaviour": "wander" }
if let Some(unit) = world.get::<TestbedUnit>(e) {
    println!("{} walks {} mm/tick", unit.template, unit.speed_mm_per_tick);
}
```
