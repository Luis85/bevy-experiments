# Performance explorer evidence — 2026-10-28

Scope: F4 ownership domains, execution-path drill-down, effective budgets, actual frame/simulation rates,
and rolling charts over time. Decision: [ADR 0021](../adr/0021-performance-domains-and-trace-explorer.md).
This feature follows the completed explorer/editor plan; historical plan tasks are not reopened.

## Behavior and review

- Costs opens with five domains, plus Unclassified when observed. Domains expand into mechanic families,
  real systems, deferred commands, conditions and nested scopes. Cross-owner calls link to their owner.
- Actual execution ancestry handles Bevy's cached parentless spans. Self excludes measured children;
  inclusive call timings and existing inclusive mechanic-budget accounting remain separate.
- Frame FPS/interval, achieved ticks per real second, configured Hz/speed, host pacing, per-tick p95 and
  diagnostic frame target are shown separately. No target, budget, pacing or authoritative state is changed.
- History retains at most 240 samples/120 real seconds at 2 Hz, including while F4 is hidden. Charts show
  frame mean/peak/target, achieved/target ticks/s, selected cost/budget and tick p95. Missing/inactive budget
  values are gaps. Freeze includes every retained path's history and call summaries; simulation continues.
- Reset clears diagnostics. Tab navigation, native button roles/names, focus restoration, input shielding,
  host modal gates, bounded visible rows, 4,096 paths and depth 64 are covered by tests.
- Review corrected deep omitted descendants reappearing as duplicate roots, selected focus being lost on
  refresh, zero-width native captions, clipped detail text, and reset racing execution-path creation.
  The latter uses the same recording epoch for parent lookup and slot creation, released before user work.
- Normal flat `npc/perf` replies remain compatible. Explicit tree, selected-row, complete-capture and
  projected-history requests expose the additional data. Quiet logging still enables measurement spans.

## Focused verification

- `cargo test --locked -p npc_perf --lib --test tree`: 7 unit tests and 11 trace integration tests passed,
  including reset/path creation concurrency, shared/reentrant spans, depth/cap limits, raw parent p95 sums,
  actual tick schedule samples, history retention, inactive budgets and frozen captures.
- `cargo test --locked -p npc_perf -p npc_view_helpers --features npc_view_helpers/perf,npc_view_helpers/command --lib --test tree`:
  initial history integration passed, including all 34 view tests (chart gaps/scales, selection, freeze,
  reset, bounded image reuse and actual widget/input behavior).
- `cargo test --locked -p village --test rpc rpc_perf_has_mechanics`: passed, including compatible flat,
  tree/selected, configured budget, history and complete capture replies.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- No dependency versions, Cargo.lock, protected policy, replay/snapshot oracle or budget baseline changed.

## Native captures

All paths below are ignored local evidence, not blessed snapshot fixtures. Processes use a temporary output
root and free BRP port, ordinary testbed scenario, semantic widget events/reflected view state and ordinary
window close. No physical keyboard/mouse input or user explorer data was used. Probe-owned hosts exited;
the user's pre-existing MCP process was preserved.

- Incumbent comparison: `target/shots/perf-before.png`.
- Tree confirmation: `target/perf-explorer-native-2145e535bd88/summary.json`, `ok:true`; six PNGs inspected:
  collapsed, expanded, frozen selected detail, budgets, narrow and paused. 2,824 paths with zero dropped
  entries; real system names under quiet logging; 12 caption geometry checks. Expand/drill, frozen detail,
  background recording, complete export, effective perception limit (55 us/render frame), reset, distinct
  pause/frame rate, Tab and Escape all passed.
- History first pass: `target/perf-explorer-native-834bcbd12030/summary.json`, `ok:true`; four PNGs inspected:
  live, frozen budget selection, narrow and paused. All four graphs rendered; pause dropped achieved/target
  ticks/s to zero while frame FPS continued; frozen text stayed fixed while live history grew. Hidden-panel
  collection continued. Follow-up improves chart color space and shortens the horizontal time caption.
- Final chart confirmation: `target/perf-explorer-native-ceecbc21de06/summary.json`, `ok:true`; all five PNGs
  inspected (live, frozen budget, narrow top, narrow scrolled to budget/tick graphs, paused). Dark plots,
  short time captions, units/current values and dashed targets are readable. All interaction assertions passed.
  Two earlier confirmation harness runs failed during refresh: text/hierarchy were queried across different
  frames and compared in entity iteration order. A single optional-component query gives one world snapshot;
  state waits and sorted text comparisons removed those harness races without changing production code.
- Initial full gate: 1,278/1,279 tests passed, one failed (`npcctl::session_host::a_long_step_does_not_block_other_requests`),
  12 skipped, 4 slow and 2 leaky. The unchanged exact gate binary passed in isolation (0.13 s); a focused locked
  Cargo recheck also passed (0.12 s). A full uncontended gate rerun follows; no test or timeout is weakened.
- Final full workspace gate: `cargo xtask verify` → **`"ok":true`**. Log:
  `target/performance-explorer-verify-final.log`. Formatting and workspace Clippy passed; 1,279 nextest
  tests passed, 12 skipped and 3 reported leaky by the runner; doc tests: 1 passed, 10 ignored. All 7
  architecture checks, all 8 replay files and all 6 snapshot fixtures passed. Dependency advisories, bans,
  licenses and sources passed; no verification tool was skipped. The earlier HTTP-test failure did not
  recur in the complete uncontended run, and no assertion or timeout changed.
- All five release binaries built with `cargo build --locked --release -p actor_testbed -p village
  -p rts_units_demo -p game_menu -p npcctl --bins`; final native evidence uses these binaries.
  Log: `target/perf-final-release.log`.
- Feature commit `7407720`, local merge `069b83f` on `main`. The merge tree matches the verified feature
  exactly. Rebuilt all five locked release binaries on `main` successfully in 3m 57s; log:
  `target/perf-main-release.log`. The pre-existing MCP process (57336) remained alive.

## Diagnostic cost (release, same machine)

Commands: `cargo run --release --locked -p npc_perf --example overhead` and
`cargo run --release --locked -p npc_perf --example tree_report`.
Logs: `target/perf-history-overhead.log`, `target/perf-history-report-cost.log`.
These are local best-of-seven synthetic timings, not a workload performance promise or a budget waiver.

The layer added **327.0 ns/system run** and **613.8 ns/fresh mechanic span** over the bare registry.
The report benchmark retains 600 interval samples and 240 history points:

| Paths | Live selected snapshot | History sample | History projection | Full frozen capture | Idle interval close |
|---:|---:|---:|---:|---:|---:|
| 300 | 170.6 us | 167.1 us | 8.1 us | 3.8074 ms | 13.1 us |
| 1,000 | 335.6 us | 345.0 us | 8.5 us | 10.5458 ms | 62.0 us |
| 2,500 | 774.7 us | 786.9 us | 8.3 us | 30.2120 ms | 155.5 us |

Live collection/UI refresh is throttled to 2 Hz. Full freeze/export computes every retained path's call
percentiles and copies the bounded history, so it can cause a one-off hitch. Four chart images are reused.
The collector itself also contributes to measured work. Native app runs are functional/visual checks,
not controlled before/after game benchmarks; observed budget breaches are shown, not suppressed.

## Limits

These are CPU-side elapsed spans including waits, with overlapping parallel execution; their sum can exceed
frame wall duration. GPU timing is absent. History samples rolling statistics rather than individual call
timestamps; Tracy/Chrome tracing remains the route to a full chronological timeline. Headless hosts must
explicitly sample history. Physical input and OS screen-reader integration remain manual checks.
