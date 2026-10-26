---
type: Component
---
# `Activity`

**Crate:** `village_content` · **File:** [`content/village_content/src/activity.rs`](../../../content/village_content/src/activity.rs) · **Capability:** village content

What a villager is visibly doing. It is sim state, written only by action start and stop functions. The view
reads it to pick animation clips. It is never used for decisions.

## Fields

Enum (default `Idle`):

| Variant | Hash tag | Set by |
|---|---|---|
| `Idle` | 0 | every action's stop (`end`), `idle_for_start`, the spawn bundle |
| `Walk` | 1 | `move_to_dest_start`, `follow_order_start` |
| `Work` | 2 | `do_work_start` |
| `Eat` | 3 | `eat_food_start` |
| `Sleep` | 4 | `sleep_in_bed_start` |
| `Drink` | 5 | `drink_water_start` |

## Lifecycle

- **Added by:** `villager_bundle` (`Activity::Idle`) in
  [`scenario.rs`](../../../content/village_content/src/scenario.rs).
- **Read by:** the inspector (`UnitSnapshot.activity`), the `village` agent section, `npcctl`'s day summary
  (meals, work shifts, sleep ticks); the view:
  [`village_view.drive_animation`](../systems/village_view.drive_animation.md) (clip per activity; `Drink`
  reuses the eating clip), [`villager_transforms`](../systems/villager_transforms.md) (lying in bed while
  `Sleep`), [`village_view.update_text`](../systems/village_view.update_text.md).
- **Written by:** `set_activity` (re-inserts the component) from the start/stop functions in
  [`actions.rs`](../../../content/village_content/src/actions.rs) and
  [`orders.rs`](../../../content/village_content/src/orders.rs). No system writes it.

## Hashing and snapshots

- **State hash:** yes, as `activity` (one `u8` tag; `register_sim_component` in `register_components`,
  [`lib.rs`](../../../content/village_content/src/lib.rs)).
- **World snapshot (ADR 0013):** saved as `activity` (automatic with hashing). BRP type path
  `npc_sim::activity` (kept from before the crate split, ADR 0009).

## Related

- [`MoveTo`](MoveTo.md), [`Eat`](Eat.md), [`Drink`](Drink.md), [`Work`](Work.md), [`Sleep`](Sleep.md),
  [`Idle`](Idle.md) — the action components behind each value.

## Example

```rust
// View side: choose an animation from sim state only.
let activity = activities.get(visual_of.0).copied().unwrap_or_default();
let clip = match activity {
    Activity::Walk => "Walking_A",
    Activity::Sleep => "Lie_Idle",
    _ => "Idle",
};
```
