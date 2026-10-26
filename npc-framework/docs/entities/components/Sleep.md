---
type: Component
---
# `Sleep`

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Capability:** village content (actions)

A villager asleep in a bed (leaf `SleepInBed`). Each tick adds the bed's energy `Provides` to energy until the
wake-up condition holds. While it runs, energy and thirst do not decay (a `PauseDecay` inserted with it).
`SparseSet` storage.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `bed` | `SimId` | the claimed bed (`BB_TARGET`) |
| `until` | `SleepUntil` | `Full`: until energy is `NEED_MAX` (urgent sleep); `Morning`: until it is day and energy > the energy `low_exit` threshold (night sleep) |

## Lifecycle

- **Added by:** `sleep_in_bed_start` (param `"Full"` or `"Morning"`; anything else means `Full`) in
  [`actions.rs`](../../../content/village_content/src/actions.rs), with `PauseDecay::of(&[energy, thirst])`
  and `Activity::Sleep`.
- **Read by:** [`sleep_system`](../systems/sleep_system.md); the view's
  [`villager_transforms`](../systems/villager_transforms.md) (lies the villager in `bed`).
- **Removed by:** `stop_sleep` (removes `PauseDecay`, takes `Sleep`; on `Finished` releases the bed).

## Hashing and snapshots

- **State hash:** yes, as `sleep` (`bed.0` as `u32`, then `until` as `u8`: `Full` 0, `Morning` 1;
  `register_sim_component` in `register_components`).
- **World snapshot (ADR 0013):** saved as `sleep` (automatic with hashing).

## Related

- [`Bed`](Bed.md) — where it sleeps. [`PauseDecay`](PauseDecay.md) — no energy/thirst decay meanwhile.
- [`Activity`](Activity.md) — `Sleep` while it runs.

## Example

```rust
// Tree (villager.rs): urgent sleep and night sleep differ only by the param.
b.leaf("SleepInBed", "SleepInBed", SLEEP_FULL);    // "Full"
b.leaf("SleepInBed", "SleepInBed", SLEEP_MORNING); // "Morning"
```
