---
type: System
---
# `drive_sim`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/driver.rs`](../../../crates/npc_kernel/src/driver.rs) · **Plugin:** `NpcDriverPlugin` (added by `NpcPlugins` unless `.without_driver()`)

The host-safe driver (ADR 0008). Turns host time into a whole number of ticks and runs `NpcSimTick` that many
times, then the registered `TickObservers`. It is the only sim-side code that reads host time, and it never
writes `Time<Fixed>` or any other host resource. Without a `SimClock` it does nothing; it freezes the registries
(`ensure_frozen`) before the first tick.

## Schedule

- **Schedule:** the **host's** schedule chosen in `NpcDriverPlugin { schedule, .. }`: `NpcPlugins` default is
  `FixedUpdate`, changed with `.run_in(Update)` etc. (`apps/village` uses `Update`, `apps/rts_units_demo`
  `FixedUpdate`). Headless tools, tests and replays use `.without_driver()` and call `run_ticks(world, n)`.
- **Set:** `NpcDriverSet` (public, for hosts that order against it), exclusive (`fn(&mut World)`)
- **Ordering:** none of its own

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimClock` (presence), `SimRate` (`hz`), `DriverSettings` (`max_ticks_per_run`, default 100; `pacing`), `SimControl`, `TickObservers`, `bevy_time::Time` delta of the running schedule |
| writes | `SimTime` (`accumulated`, `ticks_last_run`, `ticks_driven`), `SimControl.pending_steps` (reset to 0); everything via `NpcSimTick` |
| messages / events | none |

Pacing:
- **Stopped** (`paused` or `multiplier == 0`): drops the accumulator; runs `pending_steps` ticks (capped).
- **`Pacing::EveryRun`:** exactly `multiplier` ticks per schedule run (capped), ignoring time.
- **`Pacing::HostTime`** (default): `accumulated += delta * multiplier`; runs ticks of `1 / hz` while enough time
  is accumulated and under the cap; if the cap is hit with time left, the backlog is dropped (no spiral of death).

`SimTime::overstep_ppm(&rate)` (0..=1 000 000) is the integer interpolation factor for the view.

## Determinism

Host time only decides *how many* ticks run, never what a tick does (`host_delta` is the one sanctioned
`Time` reader, with a `// determinism:` comment). Same seed and commands give the same state per tick count.
Tick observers are tooling (profiler frames) and must not change sim state. The `NpcSimTick` it runs uses a
`SingleThreadedExecutor` (`NpcKernelPlugin`, `schedule.rs`; a host can `set_executor` its own), which keeps
the system order and so the state hash.

## Performance

- **Mechanic tag:** `driver` (via `tags.rs` prefix `npc_kernel::driver::drive_sim`), **inclusive**: its time
  contains the whole tick (Bevy's `Main::run_main`, tag `frame` via prefix `bevy_app::main_schedule`, is
  inclusive too and contains it) · **Budget:** none (`driver` is not in `NPC_BUDGET_TABLE`) · no spans of its own.

## Related

- [`advance_clock_system`](advance_clock_system.md) — the tick counter each run advances.
- [`apply_host_positions_system`](apply_host_positions_system.md) — the Relaxed-mode input seam.
- Components: [`HostPosition`](../components/HostPosition.md)
