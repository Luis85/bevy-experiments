---
type: System
---
# `update_panel` (perf panel, F4)

**Crate:** `npc_view_helpers` (feature `perf`) · **File:** [`crates/npc_view_helpers/src/perf_panel.rs`](../../../crates/npc_view_helpers/src/perf_panel.rs) · **Plugin:** `NpcViewHelpersPlugin` (`perf_panel::plugin`, only with feature `perf`)

Fills the perf panel: frame time (EMA of `Time<Real>` delta), sim time per tick (`SimTickTiming.ema_us`,
measured around the driver), the speed, and the top `TOP` (10) mechanics and systems by total time with their
rolling p50/p95/max from the `npc_perf` profiler. Mechanics over their p95 budget, and systems of such
mechanics, are red. The report is recomputed at most every `REFRESH_SECS` (0.5 s) while the panel is
visible (and at once when it opens), with `PerfCollector::report_limited(TOP, usize::MAX)`, so the cost does
not grow with the number of systems.

## Schedule

- **Schedule:** `Update` (panel spans spawned by `spawn_panel` in `Startup`)
- **Set:** `ViewSystems::Present`
- **Ordering:** none beyond the set.

## Reads / writes

| Access | Data |
|---|---|
| reads | `Time<Real>`, `DebugPanels.perf`, `PerfHandle` (optional), `SimTickTiming` (optional), `SimControl` (optional) |
| writes | the panel's `Visibility`, row `TextSpan` / `TextColor` (view entities); `Local<FrameMs>` (frame EMA, cached report) |
| messages / events | none |

## Determinism

View-only: reads profiler data and sim speed, never writes sim state. Profiling data lives in non-hashed
resources; budget breaches never go in the journal.

## Performance

- **Mechanic tag:** `view.perf_panel` (via `default_npc` prefix `npc_view_helpers::perf_panel`) · **Budget:**
  none · no span.

## Related

- [`npc_view_helpers.journal_panel.update_panel`](npc_view_helpers.journal_panel.update_panel.md) — F3.
- `SimTickTiming` (`timing.rs`): `before_driver` / `after_driver` around `NpcDriverSet` in `FixedUpdate`
  and `Update`.
- `docs/debugging.md` §7.2 — mechanics and budgets.
