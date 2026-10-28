# ADR 0021: Performance domains and trace explorer

- Status: Accepted
- Date: 2026-10-28

The F4 panel must explain cost, budgets and actual simulation throughput, with a compact default view.
Keep the existing flat PerfReport and mechanic-budget semantics compatible; never change a budget or
an authoritative simulation result to implement diagnostics.

- Group measured work into Simulation, World presentation, User interface, Rendering (CPU), Engine/host,
  and Unclassified only when needed. Longest-prefix domain rules are configurable; unknown work stays visible.
- Default to collapsed domain rows, then mechanics, systems and their measured nested scopes. Cached Bevy
  system spans have no static parent: record execution-time, per-thread ancestry. Keep per-path identities,
  inclusive/self timings, same-mechanic children and concurrent/reentrant entries correctly separated.
- Domain/mechanic CPU attribution uses self elapsed time, excluding frame-wrapper self time. These are CPU-side
  span durations, including waits; parallel spans can overlap. They are not GPU measurements or thread CPU cycles.
- Frame wall FPS, achieved simulation ticks/real second, configured tick rate/speed and per-tick wall duration
  are distinct. Pausing, stepping, resets and absent drivers must not display fabricated simulation FPS.
- Show configured effective mechanic limits separately, including warmup and unobserved budgets. Limits apply
  to the collector interval (render frame in windowed hosts; tick in configured headless hosts), not every call.
  Inclusive budget measurements remain unchanged and are not a sum of child p95 values.
- Bounded rolling trace storage and cached, throttled UI snapshots prevent the profiler becoming the workload.
  Freeze freezes the displayed snapshot, not the simulation or recording. Reset clears diagnostics only.
- Retain 240 samples at 2 Hz of existing rolling statistics, with real elapsed timestamps. Native History
  charts separate frame mean/peak/target, achieved/target ticks per second, selected self cost or inclusive
  budget p95, and per-tick p95. Missing values remain gaps. Collect while the panel is hidden; freeze/export
  retains the same history for subsequent drill-down. Reuse four image assets rather than accumulating plots.
- Preserve the flat RPC by default; an explicit tree request exposes the same hierarchy and selected detail.
  The UI follows the existing native dark/FiraMono/yellow identity, keyboard focus and scoped input shielding.

Verify nested/self accounting, dynamic parents, concurrency, reset/retagging, bounded storage, interval and
rate semantics, compatibility, UI interactions and native captures. Run the full workspace gate and measure
collector/report overhead. No replay/snapshot oracle, dependency version or existing budget is rebaselined.
