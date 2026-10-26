# Debug, Trace & Perf Tooling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** AI agents and humans can trace, debug, replay and profile the game, both headless and live.

**Architecture:**
- `npc_ai` emits think events into an optional `AiTrace` buffer.
- `npc_sim` gains a journal, a command input layer with orders, replays and divergence detection, and JSON inspection.
- A new `npc_perf` crate holds the tracing layer that times every system and mechanic.
- A new `npc_remote` crate adds the BRP methods.
- A new `npcctl` binary is the headless JSON CLI.
- `npc_view` and `village` add the F3/F4 panels, stepping, and remote and profiler wiring.

**Tech Stack:** Bevy 0.20 (`bevy_remote`, `trace`), tracing 0.1, tracing-subscriber 0.3, serde/serde_json, clap 4 (derive).

**Spec:** `docs/superpowers/specs/2026-10-23-debug-perf-tooling-design.md`. API facts are in `docs/research/bevy-0.20-brp-tracing.md`. The base spec and plan are the `2026-10-23-villager-npc*` files.

## Global Constraints

- Determinism rules from the base spec still hold. **Tooling must never change sim results**, and the state hash must be equal with tooling on or off.
- The journal is deterministic: no wall-clock times, no `Entity` ids, only `SimId`s and ticks. Perf data lives only in `npc_perf` and never enters journal, sim state or hash.
- The `debug-tools` feature (default on) gates BRP and the `bevy/trace` profiler in `village`. `tracy` and `chrome-trace` are opt-in features.
- All machine output is JSON or JSON Lines via serde. CLI JSON goes to stdout and logs to stderr. Exit codes: 0 ok, 1 check failed, 2 usage.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` must pass at the end of each task.
- Do not `git commit`. Only edit files your task owns.

## Review Focus

1. **Tooling perturbing determinism:** the journal or profiler is enabled and the state hash changes. Pinned by `tooling_does_not_change_hash` (Task G).
2. **Replay with commands at tick 0, or several on one tick:** the order is preserved by `(tick, seq)`. Pinned by `replay_same_tick_commands_ordered` (Task G).
3. **Stale or invalid unit ids in commands, RPC or CLI:** a JSON error comes back, with no panic. Pinned by `command_unknown_unit_is_rejected` (Task G) and `rpc_unknown_unit_error` (Task I).
4. **Profiler under the multi-threaded executor:** spans enter on worker threads and the collector stays consistent. Pinned by `layer_aggregates_across_threads` (Task H).
5. **Journal growth during long runs:** the ring buffer is capped, and the file sink streams rather than buffering. Pinned by `journal_ring_buffer_caps` (Task G).

---

### Task G: `npc_ai` trace and `npc_sim` journal, commands, orders, replay, inspect (wave 1)

**Files:**
- `crates/npc_ai/src/{runtime.rs,lib.rs}` and a new `trace.rs`.
- `crates/npc_sim/src/*`: new modules `journal.rs`, `commands.rs`, `orders.rs`, `replay.rs`, `inspect.rs`, plus edits to `schedule.rs`, `villager.rs`, `actions.rs`, `lib.rs`, `ids.rs` and `hash.rs`.
- Tests: `crates/npc_ai/tests/runtime.rs` and `crates/npc_sim/tests/{journal,commands,replay,inspect}.rs`.
- Deps: `tracing = "0.1"` in npc_ai and npc_sim. `serde` with derive and `serde_json` in npc_sim.

**Interfaces it produces** (wave 2 reads the code; these names are fixed):

- **`npc_ai::trace`**
  - `#[derive(Resource, Default)] pub struct AiTrace { pub enabled: bool, pub verbose: bool, pub events: Vec<AiTraceEvent> }`
  - `pub struct AiTraceEvent { pub agent: Entity, pub kind: AiTraceKind }`
  - `pub enum AiTraceKind`:
    - `ThinkStart { reason: ThinkReason }`
    - `LeafStart { node: NodeIdx, action: ActionId, status: Status }`
    - `LeafStop { node: NodeIdx, action: ActionId, reason: StopReason }`
    - `Abort { guard: NodeIdx, aborted: Vec<NodeIdx> }`
    - `CondEval { node: NodeIdx, cond: CondId, result: bool }` (only when `verbose`)
    - `RootDone { status: Status }`
  - `pub enum ThinkReason { Request, LeafDone, NoLeaf }`
  - `think` appends events only when the resource exists and has `enabled`.
  - `think` also wraps work in `tracing::info_span!("mechanic", name = "think.agent")`, `"think.cond"`, `"think.start"` and `"think.stop"`.
- **Runtime fix (review cleanup 1):** when a selector's running child fails, the selector re-evaluates from child 0, skipping only the already-failed child for this call. The test `selector_reconsiders_higher_siblings_after_running_child_fails` proves NightSleep is chosen rather than Wander.
- **`npc_sim::journal`**
  - `JournalEntry { tick: u64, seq: u32, unit: Option<SimId>, kind: JournalKind }` (Serialize/Deserialize; `SimId` gets serde as a plain u32).
  - `JournalKind` covers every AI event, with node names resolved to strings (the path for Abort), plus:
    - `NeedLevel { need, from, to }`
    - `Reserve { target, ok }`, `Release { target }`
    - `Effect { .. }`
    - `Command { cmd: SimCommand }`
    - `PhaseChange { night: bool }`
    - `Spawn { kind: String }`, `Despawn`
  - `#[derive(Resource)] Journal { enabled, verbose, capacity (default 50_000), entries: VecDeque<JournalEntry>, sink: Option<JournalSink> }`. The JSON Lines file writer flushes per tick.
  - `JournalFilter { unit, kinds: Vec<String>, from, to, grep, limit }` with `fn matches` and `fn query(&Journal, &JournalFilter) -> Vec<JournalEntry>`.
  - `SimPlugin` gets `journal: bool`, or add a `JournalPlugin`. The builder chooses, but enabling must be one call.
- **`npc_sim::commands`**
  - `SimCommand` (serde, tagged `"type"`):
    - `SpawnVillager { pos: (i32,i32), home: Option<u32> }`
    - `Despawn { unit: u32 }`
    - `SetNeed { unit, need: NeedKind, value: i32 }`
    - `SetFood { stockpile, food: u32 }`
    - `Order { unit, order: Order }`
  - `#[derive(Resource)] CommandQueue { pending: BTreeMap<(u64 tick, u32 seq), SimCommand> }`.
  - `pub fn submit(world, at_tick: Option<u64>, cmd) -> Result<(u64,u32), CommandError>` validates ids and defaults to the next tick.
  - `apply_commands_system` runs in `SimSet::Inputs` and journals each applied command.
  - Applied commands are appended to `CommandLog` for recording.
- **`npc_sim::orders`**
  - `Order { GoTo { pos: (i32,i32) }, Clear }` and a `#[derive(Component)] CurrentOrder(Option<OrderKind>)`.
  - Conditions and actions: `has_order`, `FollowOrder` = `MoveToDest` toward the order position, then clear.
  - Tree: `Guard "FollowOrder" [has_order, LowerPriority]`, between Forage and NightSleep.
- **`npc_sim::replay`**
  - `Replay { version: u32, seed: u64, scenario: String, ticks: u64, commands: Vec<(u64,u32,SimCommand)>, checkpoints: Vec<(u64,u64)> }`.
  - `fn record(app, ticks, every) -> Replay`, `fn verify(&Replay) -> VerifyReport { ok, checked, first_mismatch: Option<(u64, u64, u64)> }`, and `fn diverge(a: &Replay, b: &Replay) -> Option<Divergence { tick, entities: Vec<EntityDiff { sim_id, components: Vec<String>, a: serde_json::Value, b: serde_json::Value }> }>`.
  - `fn entity_hashes(world) -> BTreeMap<SimId, u64>` (per-entity hashes for the diff) goes in `hash.rs`.
  - `pub fn new_sim_app(seed, scenario: &str, villagers: u32) -> App` is the shared headless constructor.
    - `scenario` is `"village"` (iteration-1 layout) or `"village_n"`, which uses `spawn_village_n(world, n)`: n houses, beds, fields and stockpiles on a grid, for benchmarks.
- **`npc_sim::inspect`**
  - `WorldSnapshot { tick, day, time_permille, night, entities: Vec<UnitSnapshot>, reservations }`.
  - `UnitSnapshot` contains:
    - `sim_id`, `kind`, `pos`, `needs`, `activity`, `stockpile`
    - `tree_path: Vec<String>`, `active_leaf`, `blackboard`, `order`
    - `reservations_held`, and `recent_journal: Vec<JournalEntry>` (last 20)
  - `fn snapshot(world) -> WorldSnapshot` and `fn unit(world, SimId) -> Option<UnitSnapshot>`.
- **Reflect:** derive `Reflect` with `#[reflect(Component)]` / `#[reflect(Resource)]` on the sim components and resources BRP should show (`SimId`, `SimPos`, `Needs`, `Activity`, `Stockpile`, `Villager`, `Place`, `CurrentOrder`, `SimClock`). Register them in `SimPlugin`. Add `bevy_reflect = "0.20"`, plus `bevy_ecs` features if needed.

- [ ] **Step 1: Write failing tests**
  - `npc_ai/tests/runtime.rs`:
    - `trace_records_leaf_start_stop_abort`: run the abort scenario with `AiTrace{enabled:true}`. Expect events `ThinkStart`, `LeafStart(b)`, then on preemption `Abort{guard:1}`, `LeafStop(b, Aborted)`, `LeafStart(a)`.
    - `no_trace_resource_no_events`: no panic and no events.
    - `selector_reconsiders_higher_siblings_after_running_child_fails`.
  - `npc_sim/tests/journal.rs`:
    - `journal_records_abort_with_guard_name`: urgent hunger interrupts wander. The journal holds `Abort` with guard `"UrgentEat"` and an aborted path containing `"Wander"`.
    - `journal_filter_by_unit_kind_range`.
    - `journal_ring_buffer_caps`: capacity 100 over 24 000 ticks gives `len == 100`.
    - `journal_deterministic`: two seed-1 runs give identical serialized journals.
    - `tooling_does_not_change_hash`: hash at 10 000 ticks is equal with journal plus verbose trace on vs off.
  - `npc_sim/tests/commands.rs`:
    - `order_goto_interrupts_wander_not_urgent`: GoTo while wandering means the path starts with `"Villager > FollowOrder"` and he arrives within the expected ticks. With hunger urgent and food available, UrgentEat wins.
    - `order_clear`.
    - `spawn_villager_mid_run_acts`: a new SimId thinks within 2 ticks.
    - `despawn_command_releases_reservations`.
    - `set_need_and_set_food_apply_at_tick`.
    - `command_unknown_unit_is_rejected`: `submit` returns `Err`.
  - `npc_sim/tests/replay.rs`:
    - `record_then_verify_ok`, with commands.
    - `tampered_replay_reports_mismatch_tick`.
    - `replay_same_tick_commands_ordered`.
    - `diverge_seeds_reports_first_tick_and_entity`: seeds 1 vs 2 give `Some`, with `tick` > 0 and a non-empty `entities` list.
    - `diverge_identical_is_none`.
  - `npc_sim/tests/inspect.rs`:
    - `unit_snapshot_has_path_names_and_reservations`.
    - `world_snapshot_serializes_roundtrip`.
    - `spawn_village_n_layout`: 100 villagers, unique SimIds, no overlapping places.
- [ ] **Step 2:** Run the tests and confirm they fail.
- [ ] **Step 3:** Implement.
  - Journal capture happens inside the existing exclusive `think_system`: drain `AiTrace` into the `Journal`, mapping `Entity → SimId` and node idx → name.
  - Other sim journal events are pushed where they happen. `NeedLevel` comes from `decay_needs_system` and reservations from the actions. Use a helper that no-ops when the journal is disabled.
- [ ] **Step 4:** Pass the gate: `cargo test --workspace` and clippy. Then re-run `print_three_day_timeline` and confirm the rhythm is unchanged.

### Task H: `npc_perf` crate (wave 1, parallel with G)

**Files:** a new crate `crates/npc_perf/` (`lib.rs`, `layer.rs`, `stats.rs`, `tags.rs`, `budget.rs`, `report.rs`) and `tests/perf.rs`. Add it to the workspace members and `[workspace.dependencies]`.
- Deps: `tracing`, `tracing-subscriber = { version = "0.3", features = ["registry"] }`, `serde`, `serde_json`, `bevy_ecs = "0.20"` (for `Resource` wrappers only).

**Interfaces it produces:**
- **Collector**
  - `#[derive(Clone)] pub struct PerfCollector(Arc<...>)`
  - `impl PerfCollector`:
    - `fn new(window: usize) -> Self` (default 600)
    - `fn layer(&self) -> PerfLayer`
    - `fn end_frame(&self)`, which closes a sample for every mechanic
    - `fn report(&self) -> PerfReport`
    - `fn reset(&self)`
    - `fn set_tags(&self, MechanicTags)`
    - `fn set_budgets(&self, PerfBudgets)`
    - `fn take_breaches(&self) -> Vec<BudgetBreach>`
- **`PerfLayer`** implements `tracing_subscriber::Layer<S>` for any `S: Subscriber + for<'a> LookupSpan<'a>`.
  - It times the spans `"system"` (field `name`, mapped to a mechanic via the tags) and `"mechanic"` (field `name` used as the mechanic id directly), using `on_enter`/`on_exit` keyed by span id.
  - It records the `name` field in `on_new_span` via span extensions.
  - It must be `Send + Sync` and handle concurrent entries on many threads; re-entrant spans are not expected.
- **`MechanicTags`**: an ordered list of `(prefix, mechanic)`, longest prefix wins. Unmatched names go under `"untagged"`. `MechanicTags::default_npc()` covers the npc_sim, npc_ai and npc_view module paths plus `bevy_` → `"engine"`.
- **Stats**
  - `SystemStats { name, mechanic, calls, total_us, p50_us, p95_us, max_us }` over a rolling window plus whole-run totals.
  - `MechanicStats { mechanic, samples, p50_us, p95_us, max_us, total_us, budget_us: Option<u64>, over_budget: bool }`. A sample is the sum of the mechanic's time between two `end_frame` calls.
  - `PerfReport { systems: Vec<SystemStats>, mechanics: Vec<MechanicStats>, frames: u64 }`, sorted by `total_us` descending (serde).
- **`PerfBudgets`**: `BTreeMap<String, u64>` holding p95 µs per mechanic, plus `default_npc()`.
- **Breaches:** checked in `end_frame`. A breach is pushed and a `tracing::warn!(target: "npc_perf", ...)` is logged at most once per mechanic per 5 s of wall time.
- **`pub fn install_global(collector: &PerfCollector) -> bool`** sets `registry().with(layer)` as the global default and returns false if one already exists. It is used headless.
- **`#[derive(Resource, Clone)] pub struct PerfHandle(pub PerfCollector)`**.

- [ ] **Step 1: Write failing tests** in `tests/perf.rs`. Use a local subscriber via `tracing::subscriber::with_default` for single-thread tests, and a global one only in a dedicated test binary if needed.
  - `system_spans_timed_and_tagged`: synthetic `info_span!("system", name = "npc_sim::needs::decay_needs_system")` entered 10× gives `calls == 10` and mechanic `"needs"`.
  - `mechanic_spans_use_name_directly`.
  - `percentiles_correct`: inject known durations through a test hook `PerfCollector::record_for_test(name, us)`. Values 1..=100 give p50 50, p95 95, max 100.
  - `end_frame_sums_per_mechanic`.
  - `budget_breach_rate_limited`: budget 0 for 3 frames within 5 s gives exactly 1 breach.
  - `layer_aggregates_across_threads`: 8 threads × 1000 spans gives `calls == 8000`.
  - `bevy_schedule_systems_are_captured`: a bare `bevy_ecs` `Schedule` with 2 systems, with `bevy_ecs/trace` enabled as a dev-dependency feature, run 5× under the layer, reports both system names with `calls == 5`.
- [ ] **Steps 2–4:** fail, implement, pass, clippy.

### Task I: `npc_remote` crate, view panels and `village` wiring (wave 2, after G and H)

**Files:**
- New crate `crates/npc_remote/` (`lib.rs`, `methods.rs`) and `tests/rpc.rs`.
- `crates/npc_view/src/{overlay.rs, new journal_panel.rs, perf_panel.rs, select.rs, driver.rs, lib.rs}`.
- `apps/village/{Cargo.toml,src/main.rs}`.

**Interfaces:**
- **`npc_remote::NpcRemotePlugin { port: u16 }`** adds `RemotePlugin::default().with_method_main(...)` for:
  - Control: `npc/pause`, `npc/resume`, `npc/step {ticks}`, `npc/speed {multiplier}`
  - State: `npc/state`, `npc/unit {id}`
  - Input: `npc/command {cmd, at_tick?}`
  - Data: `npc/journal {filter}`, `npc/perf`, `npc/perf/reset`
  - Recording: `npc/record/start`, `npc/record/stop {path}`
  - It also adds `RemoteHttpPlugin::default().with_port(port)`.
  - All results are the serde structs from Task G and H. Errors use `BrpError` with a clear message, never a panic.
  - `npc/step` runs `SimTick` n times synchronously inside the handler, works while paused, and returns the new tick and hash.
  - Pause and speed use `npc_view::SimSpeed` when present. Otherwise, headless, `npc_remote` owns a `RemoteSimControl` resource that `npc_remote::HeadlessDriverPlugin` uses to drive `SimTick` each `Update`.
  - `npc/screenshot {path}` is registered by `village`, not by npc_remote, using the Task C screenshot code.
- **Profiler wiring in `village` (`debug-tools`):**
  - `LogPlugin { custom_layer: npc_perf_layer, filter: "wgpu=error,naga=warn,npc=debug".into(), .. }`.
  - `npc_perf_layer` creates or fetches a static `OnceLock<PerfCollector>`, inserts `PerfHandle`, and returns the boxed layer.
  - A system in `Last` calls `end_frame`.
  - The headless driver in npc_remote calls `end_frame` after each SimTick, and so does npcctl.
- **Features:**
  - `village`: `default = ["debug-tools"]`, `debug-tools = ["npc_view/debug-tools", "dep:npc_remote", "bevy/trace", "bevy/bevy_remote"]`, `tracy = ["debug-tools", "bevy/trace_tracy"]`, `chrome-trace = ["debug-tools", "bevy/trace_chrome"]`.
  - New flags: `--no-remote`, `--remote-port N`, `--journal FILE`, `--record FILE`.
- **View:**
  - F3 journal panel: the last 30 entries, filtered to the selected unit, else all.
  - F4 perf panel: frame ms, SimTick µs, and the top 10 mechanics and systems with p50/p95/max, with over-budget rows in red.
  - `.` steps one tick while paused.
  - Left-clicking a villager selects it (view-only `Selected` resource holding a SimId). The overlay shows the selected unit.

- [ ] **Step 1: Write failing tests** in `tests/rpc.rs`. A headless app with `MinimalPlugins`, `SimPlugin`, `VillagerPlugin`, journal, `HeadlessDriverPlugin` and `NpcRemotePlugin { port: free_port() }` runs on a thread or with `app.update()` loops. Use a blocking HTTP client dev-dependency such as `ureq`.
  - `rpc_pause_step_state`: pause, then step 10, and the tick advances by exactly 10. `npc/state` returns entities.
  - `rpc_command_order_and_unit`: a GoTo order is followed, and the next `npc/unit` shows `FollowOrder`.
  - `rpc_unknown_unit_error`.
  - `rpc_journal_filter`.
  - `rpc_perf_has_mechanics`.
  - `rpc_builtin_world_query_sees_simid`.
- [ ] **Step 2:** Implement until the tests pass.
- [ ] **Step 3:** Visual check.
  - Run `village --screenshot` with F3/F4 toggled on by flags. Add `--panels journal,perf` to the village args for screenshots.
  - Read the PNG and confirm the panels are readable.
  - Run `curl` against the live game (`npc/state`, `npc/command` GoTo, then `npc/screenshot`) and confirm it works.
- [ ] **Step 4:** Pass the gate.

### Task J: `npcctl` headless CLI and benchmarks (wave 2, parallel with I)

**Files:** a new crate `apps/npcctl/` (`main.rs` plus modules) and `tests/cli.rs`. Deps: npc_sim, npc_ai, npc_perf, clap 4 derive, serde_json, and bevy_ecs/bevy_app with the `trace` feature.

**Subcommands** (spec §3.5, exact flags): `run`, `inspect`, `journal`, `replay verify`, `diverge`, `bench`.
- **`bench`** creates `new_sim_app(seed, "village_n", n)` for each n, installs the perf layer globally once, warms up for 200 ticks, then measures `--ticks`. The output is `{ git_rev?, results: [{ villagers, ticks, wall_ms, ticks_per_sec, mechanics: [...], systems: [top 15] }] }`. With `--baseline FILE`, it compares against the baseline, prints regressions over 25% on mechanic p95, and exits 1.
- `bench/baseline.json` is produced by running `npcctl bench --villagers 1,100,500 --ticks 2000 --out bench/baseline.json` (release profile recommended, documented).
- `--help` text for every subcommand includes a one-line example.

- [ ] **Step 1: Write failing tests** in `tests/cli.rs`. Use `std::process::Command` with `env!("CARGO_BIN_EXE_npcctl")`.
  - `run_outputs_summary_json`: it parses, and includes `final_hash` and `days`.
  - `inspect_unit_at_tick`.
  - `journal_filter_kinds`: every line parses and has kind ∈ filter.
  - `replay_record_verify_roundtrip`, which writes to a temp dir.
  - `diverge_two_seeds`: exit 1, JSON with `first_divergent_tick`.
  - `bench_small`: villagers `1,10`, ticks 100, so it has 2 results with non-empty mechanics including `"think"` and `"needs"`.
  - `usage_error_exit_2`.
- [ ] **Steps 2–4:** fail, implement, pass, clippy.

### Task K: Docs and agent-eye verification (wave 3, coordinator plus one agent)

- [ ] **Write the docs.**
  - `docs/debugging.md` is the full guide: every CLI subcommand and RPC method with a copy-paste example, JSON schemas (field lists), journal kinds, mechanic list and budgets, and the Tracy/Chrome builds.
  - It includes recipes: why did unit X do Y; reproduce and bisect a desync; find the slowest mechanic; drive the live game with curl; screenshot at tick N.
  - `npc-framework/CLAUDE.md` is a 30-line agent entry point that links to it, plus the README section.
- [ ] **Fresh-agent test:** a fresh agent, given only `CLAUDE.md`, must answer three debugging questions using the tools: why the villager went to bed at tick X; the slowest mechanic at 500 villagers; and the first divergent tick between seeds 1 and 2. Its friction notes feed doc fixes.
