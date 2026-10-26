---
type: System
---
# `interp_alpha`

**Crate:** `npc_view_helpers` · **File:** [`crates/npc_view_helpers/src/interp.rs`](../../../crates/npc_view_helpers/src/interp.rs) · **Plugin:** none (a helper function, not a scheduled system)

The interpolation factor between ticks: how far the kernel driver's own accumulator (`SimTime`) is into the
next tick, as `0.0..=1.0`. View systems lerp a visual from `PrevSimPos` to `SimPos` by it. It returns 1.0
when the sim is stopped (`SimControl::is_stopped`: paused or 0×, so the latest state shows without jitter) or when
`SimTime`, `SimRate` or `SimControl` is missing. `alpha(..)` is the same from `Option<Res<..>>` parameters.

## Schedule

- **Schedule:** none. It is called from view systems in `Update` / `ViewSystems::Animate`:
  [`villager_transforms`](villager_transforms.md) and the testbed's `frame_params` (used by
  [`actor_transforms`](actor_transforms.md) and `vehicle_transforms`); also `apps/rts_units_demo/src/view.rs`.
- **Set / ordering:** n/a.

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimTime` (`overstep_ppm(rate)`), `SimRate` (`hz`), `SimControl` |
| writes | nothing (pure function) |
| messages / events | none |

Formula: `t.overstep_ppm(r) as f32 / 1_000_000.0`. Example from its test: at 20 Hz with 25 ms accumulated,
alpha is 0.5.

## Determinism

View-only: never writes sim state. It reads the driver's `SimTime`, never `Time<Fixed>`. The float result
must never flow back into the sim (`.claude/rules/view.md`).

## Performance

- **Mechanic tag:** n/a (not a system; its cost lands in the caller's tag, e.g. `view.visuals`).

## Related

- [`villager_transforms`](villager_transforms.md), [`actor_transforms`](actor_transforms.md) — the callers.
- [`copy_prev_pos_system`](copy_prev_pos_system.md) — fills `PrevSimPos` each tick.
- Components: [`PrevSimPos`](../components/PrevSimPos.md), [`SimPos`](../components/SimPos.md)

```rust
use npc_framework::view_helpers::interp::interp_alpha;
let alpha = interp_alpha(sim_time.as_deref(), rate.as_deref(), control.as_deref());
transform.translation = sim_to_world(prev.0).lerp(sim_to_world(*pos), alpha);
```
