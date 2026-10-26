---
type: Component
---
# `Sensors`

**Crate:** `npc_perception` · **File:** [`crates/npc_perception/src/sensors.rs`](../../../crates/npc_perception/src/sensors.rs) · **Capability:** perception (opt-in, `PerceptionPlugin`)

What an agent senses with: a list of sensor definitions plus an interest mask. Put it on a sim entity that has
`SimId` and `SimPos`. It is the component that makes an entity an observer for
[`perception_system`](../systems/perception_system.md). It requires [`Facing`](Facing.md) and an empty
[`PerceptionMemory`](PerceptionMemory.md) (`#[require(Facing, PerceptionMemory)]`).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `sensors` | `Vec<Sensor>` | sensor definitions, in insertion order |
| `interest` | `u32` | kind bit mask the default conditions `sees_kind` / `remembers_kind` test (default `ALL_KINDS` = `u32::MAX`) |

`Sensor` is an enum:

| Variant | Payload | Meaning |
|---|---|---|
| `Vision(Vision)` | `range_mm: i32`, `half_fov_deg: u16` (0..=180), `cos2_ppm: u32` | cone around `Facing`; needs line of sight over occluding grid cells. `cos2_ppm` is cos² of the half angle in parts per million (offline `COS2_PPM` table). Build it with `Vision::new`, not by hand. |
| `Hearing(Hearing)` | `radius_mm: i32` | hears `Noise` messages within this radius and within the noise's own radius; no line of sight |
| `Proximity(Proximity)` | `radius_mm: i32` | any `Perceivable` within the radius, any direction, no line of sight |

Builders: `Sensors::new().vision(range_mm, half_fov_deg).hearing(radius_mm).proximity(radius_mm).interest(mask)`.
Helpers the system uses: `hearing_radius()` and `proximity_radius()` (the largest of each), `scan_radius()`
(the largest vision range or proximity radius).

## Lifecycle

- **Added by:** content spawns. The testbed's `spawn_unit` gives every actor
  `Sensors::new().vision(VISION_RANGE_MM, VISION_HALF_FOV_DEG).hearing(HEARING_MM)`
  ([`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** [`perception_system`](../systems/perception_system.md) (hearing radius every tick, the full
  sensors on the agent's stagger tick); conditions `sees_kind` and `remembers_kind` read `interest`
  ([`conds.rs`](../../../crates/npc_perception/src/conds.rs)); `perception_json` (inspect); the perception ×
  agent-io glue (`perceived_or_remembered`, `perception_section`: no `Sensors` means no point of view, so
  `omniscient: true`); the testbed explorer walls sense (`content/testbed_content/src/walls.rs`). View only:
  `npc_view_helpers::perception_gizmos`, the `testbed_view` F6 panel.
- **Written / removed by:** nothing in the framework after spawn.

## Hashing and snapshots

- **State hash:** yes, as `sensors` (`register_sim_component` in `register_perception_hash`,
  [`crates/npc_perception/src/lib.rs`](../../../crates/npc_perception/src/lib.rs)). Encoding: the length, then
  per sensor a `u8` tag (0 vision, 1 hearing, 2 proximity) and its fields, then `interest`.
- **World snapshot (ADR 0013):** saved as `sensors` (automatic with hashing).

## Related

- [`Facing`](Facing.md) — the direction vision cones point in.
- [`Perceivable`](Perceivable.md) — what vision and proximity can sense.
- [`PerceptionMemory`](PerceptionMemory.md) — where the results go.
- [`PerceptionFacts`](PerceptionFacts.md) — optional blackboard mirror of the nearest sensed thing.

## Example

```rust
use npc_framework::perception::{Perceivable, Sensors};

const UNIT: u32 = 1 << 0;
const FOOD: u32 = 1 << 1;

// A 12 m cone of 120 degrees, 8 m hearing, 1.5 m "touch". The default conditions only care about food.
spawn_sim(world, (
    SimPos::new(0, 0),
    Sensors::new().vision(12_000, 60).hearing(8_000).proximity(1_500).interest(FOOD),
    Perceivable::new(UNIT),
));
```
