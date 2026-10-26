---
type: Component
---
# `Stealthy`

**Crate:** `npc_framework` (glue `perception_rules`) · **File:** [`crates/npc_framework/src/glue/perception_rules.rs`](../../../crates/npc_framework/src/glue/perception_rules.rs) · **Capability:** perception × rules glue (`PerceptionRulesPlugin`, features `perception` + `rules`)

Makes a target harder to detect with the `per_roll` detection check. The built-in target modifier
`"stealthy"` (`stealthy_modifier`) subtracts `penalty` from the observer's Per for each detection of this
target. A negative penalty makes it easier to spot (a bright banner). It has no effect while the active
detection check is `"automatic"`.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `penalty` | `i32` | Per modifier against this target, subtracted (`-penalty`, saturating) |

## Lifecycle

- **Added by:** content. The testbed's `spawn_unit` inserts `Stealthy { penalty }` when the unit spec has
  `stealthy` ([`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** `stealthy_modifier`, summed with the other registered `PerRollModifiers` (name order) by
  `per_roll_modifier`, called from `per_roll_check`. That check runs inside
  [`perception_system`](../systems/perception_system.md) (the perception crate calls the active
  `DetectionCheckFn` with `&World`).
- **Written / removed by:** nothing in the framework.

## Hashing and snapshots

- **State hash:** yes, as `stealthy` (`register_sim_component` in `PerceptionRulesPlugin::build`). Encoding:
  `penalty` as `i32`. Only registered when the glue plugin is added.
- **World snapshot (ADR 0013):** saved as `stealthy` (automatic with hashing).

## Activation note

`NpcPlugins` adds `PerceptionRulesPlugin` whenever perception and rules are both on, but with
`activate: self.per_roll`. So `per_roll` is only the active check after `with_per_roll_detection()`
(or `use_detection_check("per_roll")`). `PerceptionRulesPlugin::default()` added by hand activates it.

## Related

- [`Perceivable`](Perceivable.md) — the target must be perceivable to be sensed at all.
- [`CharacterSheet`](CharacterSheet.md) — the observer's Per comes from it.
- [`flush_per_roll_log`](../systems/flush_per_roll_log.md) — journals the rolls the check makes.

## Example

```rust
use npc_framework::prelude::Stealthy; // features perception + rules

world.entity_mut(scout).insert(Stealthy { penalty: 4 }); // observers roll at Per - 4
```
