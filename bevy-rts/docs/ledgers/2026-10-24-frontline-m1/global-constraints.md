## Global Constraints

Every task's requirements implicitly include this section. Spec section numbers are in brackets.

**Versions and toolchain**
- `bevy`, `bevy_app` and `bevy_ecs` are pinned at `=0.20.0`, with `default-features = false` and an explicit feature list [§3, §5.2].
- Rust is pinned to `1.99.1` in `rust-toolchain.toml` (MSRV 1.97.1), edition 2024.
- One dated nightly exists only for `cargo-public-api` rustdoc JSON. Its name is in `xtask/api-toolchain.txt`, and nothing builds with it [§3, §12.7].

**Where and how to build**
- `main` lives in `C:\Projects\bevy-experiments`, and a guard blocks Write/Edit under `C:\Projects\`. All editing happens in git worktrees under `C:\fl\`:
  - `C:\fl\a` (branch `m0/phase-a`) is the controller's integration worktree and lane 1.
  - Parallel tasks use `C:\fl\tNN` on branch `m0/tNN-<slug>`.
  - The controller merges with `git -C …`.
- Build output (ruling R19): the **repo** `.cargo/config.toml` sets `build.build-dir = "C:/t/fl/{workspace-path-hash}"`, and there is no shared `target-dir`. Task 1 probes it; the fallback is a per-worktree `CARGO_BUILD_BUILD_DIR` set by the controller.
- At most **two** cargo builders run at once across all agents [§13.1, §13.3]. A gate run counts as a builder.
- A cold Bevy build takes 15+ minutes per build dir. Run long cargo commands in the background or with a 600000 ms timeout.
- After Task 1, no task edits `Cargo.toml` or `Cargo.lock` unless its **Files** list says so. If a dependency is missing, stop and report.

**Dependency rules** [§5.1, ADR-0001, ADR-0002]
- `fl_math` depends on nothing in the workspace; `fl_protocol` → `fl_math`; `fl_view` → `fl_math`, `fl_protocol`. These three have no Bevy at all.
- `fl_content` → `fl_math`, `fl_protocol`, `bevy_ecs`.
- `fl_sim` and every `fl_feat_x` use `bevy_app` + `bevy_ecs` only, plus `fl_math`, `fl_protocol`, `fl_view` and `fl_content`. They never use `bevy_render`, `bevy_window`, `bevy_input`, `bevy_time`, `bevy_winit`, `wgpu`, the `bevy` umbrella or `Time`.
- Features are two crates (`fl_feat_x` + `fl_feat_x_client`). Feature-to-feature edges must appear in `deps()`, and in `[package.metadata.fl] sim_deps` for `arch-check`.
- `fl_client` and `fl_feat_x_client` use full `bevy` plus `fl_view`, `fl_protocol`, `fl_math` and `fl_content`. **No client crate depends on `fl_sim`, not even as a dev-dependency.**
- `fl_game` is the only crate that depends on both client crates and `fl_sim`.
- `fl_catalog`, `fl_cli`, `fl_test_utils` and `fl_observe` are headless.
- `cargo xtask arch-check` enforces all of this from Task 2 on.

**Determinism** [§5.4, ADR-0003/0004/0005/0006]
- **Numbers:** sim state is `Fx` and integers only.
- **Randomness:** only `fl_math::Rolls` over `(MatchSeed, tick, sim_id, stream_key(name))`, with every stream name registered via `SimBuilder::register_stream`. World-level draws use `SimId(0)`. RNG state in a resource, component or `Local` is banned; `Pcg32` is for tools and the client only.
- **Banned in sim crates** (generated `clippy.toml` per crate): `f32`/`f64` types; std and `bevy_platform` `HashMap`/`HashSet`; `Instant`, `SystemTime`, `ParallelCommands`, `Time` and `Pcg32`; `thread::spawn`, `rand::rng`/`random` and `env::var`. Every sim crate also denies `clippy::float_arithmetic`. The one allowed exception is the runner thread spawn in `fl_sim::runner` (Task 12), behind a scoped `#[expect]` with a reason.
- **Ordering (narrowed):** order-sensitive operations follow `SimIndex` (ascending `SimId`). These are shared writes, spawns, despawns and `SimId` allocation, budgets and queues, and cross-entity reads of same-tick writes. Entity-local updates may iterate in any order, including `par_iter_mut`.
- **Executor:** single-threaded, set explicitly. `SimTick` ambiguity detection is at `LogLevel::Error`, with `auto_insert_apply_deferred: false`.
- **Hidden state:**
  - no `Local<T>` in sim systems
  - no gameplay logic on `Changed`/`Added`/`RemovedComponents` (`Changed<T>` is allowed only in `Phase::Publish`)
  - no observers in sim crates (hooks maintain indices only)
  - no `Entity` in hashed or saved state, commands, journal or replays
- **Classification:** every sim component and resource is registered as exactly one of `Hashed` (with a versioned codec), `Derived` (with a rebuild fn), `Transient` or `External` (resources only). An unclassified type fails `build_sim_app` in debug builds and every test.
- **Hashing:** the state hash writes registry-backed IDs as their stable name key, never the dense ID. Every runtime ID comes from a stable-string registry that is sorted and frozen.
- **Content IDs inside command payloads are names** (`"base.de.pz3"`), never dense IDs, so replays survive content renumbering.

**Schedule and commands** [§5.5, §5.6]
- **Phases:** `Input → Think → Act → Resolve → Economy → Perceive → Lifecycle → Publish`, with the 18 hook sub-sets of §5.6. Each phase ends with one explicit `ApplyDeferred`. Features add systems only into hook sub-sets.
- **Spawns and Messages:**
  - Every in-tick spawn is a `SpawnRequest` Message handled in `Lifecycle::Spawn`.
  - Setup and load use `setup_spawn`, which runs the same code.
  - Messages rotate in `Input::Housekeeping`.
- **`step_tick` is the only way to run `SimTick`.** Its order is: inbox → `SimTick` → hash if due → take outputs → `clear_trackers` → advance clock.
- **Envelope:** `PlayerCommand { exec_tick, player, seq, units, kind, queue }`. Commands apply in `(player, seq)` order, and `seq` is strictly increasing per player over the match.
- **Delivery:** drivers deliver each command in the `TickInput` of exactly its `exec_tick`. The sim rejects `Late`, `Early` and `DuplicateSeq`.
- **Input delay:** `LocalDriver` 1, `TestDriver` 0, `LockstepDriver` 2 in tests.
- **Formats:** postcard on the wire and in replays and saves. bincode is banned.
- **Debug commands:** `debug.*` kinds are accepted only when `MatchHeader.cheats_allowed` is true.
- `SIM_TICK_HZ = 20`. The state hash runs every `StateHash.interval_ticks` (20) ticks, and every tick in tests.

**Content and world** (unchanged from rev 1)
- Content is strict: every def and kind uses `#[serde(deny_unknown_fields)]`, and unknown component names, rules sections and `*.X.ron` kinds are errors that carry file and line. There is no `#[serde(flatten)]` in rules-hashed content.
- Never copy code or data from nival/Blitzkrieg. M0 uses primitive placeholders, and nothing is downloaded.
- Coordinates:
  - Sim positions are metres with `x` east and `y` north. Heading is an `Angle`: 0 is +x, increasing counter-clockwise.
  - The client maps sim `(x, y)` → Bevy `Vec3::new(x, height, y)`, with yaw = `-heading_radians`.
  - Placeholder meshes are built with their length along local +X.

**Observability** [§15]
- The journal, `SimCounters`, profiler timings and phase markers are observe-only. Sim logic never reads them, and they are never hashed.
- The journal is `Transient`. Its record kinds come from a stable-string registry, and `step_tick` sorts each tick's entries by `(phase, kind, sim_id, emission order)`.
- Log with `tracing` on module-path targets. Sim code lives in module paths that name its owner (`fl_sim::<module>`, `fl_feat_<name>::…`), so the profiler attributes time from system names.
- **The sim never reads a clock.**
  - `step_tick` wraps each tick in a `sim_tick` span with field `tick`, so every sim log record carries the tick.
  - Phase timing comes from `fl_sim::phase` marker *events*, which `fl_observe` timestamps.
  - Wall-clock reads (`Instant`) are allowed only in `fl_observe`, `fl_cli` bench code and `fl_client`, each with a scoped `#[expect(clippy::disallowed_types, reason = "…")]` where a ban applies.
- Every mechanic ships with its journal records, counters and spans (definition of done, §9).

**Gate and evidence** [§12, ADR-0009]
- **Interim gate** (Task 1, and Task 2 until its last step). Run it from the worktree root; all must pass:
  ```bash
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  bash scripts/check_deps.sh
  ```
- **From Task 2:** run `cargo xtask gate task --track <track>…` and paste its output into the task report. The expected last line is `gate task: PASS in <n>s`.
- From Task 4 the Stop/SubagentStop hook enforces the gate.
- `cargo xtask gate pr` runs before any merge into `main`.
- Test-name conventions the gate filters on:
  - `smoke_*`: the task-level subset of `fl_sim` and `fl_test_utils`
  - `gpu_*`: needs a GPU; nightly and milestone only, run serially
  - `milestone_*`: milestone only
  - `brp_*`: needs the `debug_remote` feature; nightly and milestone only

**Protection and governance** [§12.5, §13.2, ADR-0011]
- **Protected paths** are listed in `OWNERS.toml` `[protected]`, from Task 3:
  - `**/tests/{acceptance,holdout,replays}/**`
  - `**/snapshots/**`
  - root `scenarios/**`
  - `docs/design/mechanics/**`
  - `docs/perf/baselines/**`
  - `docs/perf/budgets.ron`
- **Producing protected artifacts:** tests and tools write them as `*.new` files (`insta` writes `*.snap.new`). Only the lead promotes them, with `cargo xtask bless <paths> --reason "…"`, and the reason goes into the commit as `Blessed: <reason>`.
- **Escape hatch:** an implementer that believes a protected test is wrong writes `BLOCKED: test X contradicts spec Y because …` and stops.
- **Contract paths** change only with an ADR. They are the `contract` track in `OWNERS.toml`: `fl_math`, `fl_protocol`, `fl_view`, the `fl_sim` extension API (builder, registries, classification, phases, `step_tick`, runner, save format), the `fl_content` loader and registry, `xtask`, `.claude/`, `deny.toml`, `OWNERS.toml`, the clippy template and the spec.
  - Rework tasks cite their ADR in the commit with a `Refs: ADR-NNNN` trailer.
  - Each task names its **Track**, and `check-scope` enforces it.
- **Commits:** commit only the files your task owns. Use conventional messages (`feat(fl_x): …`, `!` for contract breaks) ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.


## Environment notes for agents (controller)
- Edit files ONLY with absolute paths under your worktree (C:\fl\...). The Write/Edit tools are blocked under C:\Projects\ by a guard hook; never write there.
- The shell may reset its cwd; always `cd <worktree> && …` in each Bash call.
- Plan (read only your brief; consult other sections only if your brief points there): C:\fl\a\docs\superpowers\plans\2026-10-24-frontline-m1.md. Spec: docs/superpowers/specs/2026-10-23-frontline-rts-design.md; ADRs: docs/adr/. Architect's per-crate rework list: C:\fl\a\.superpowers\sdd\2026-10-23-frontline-m0-foundation\architect-rev2-report.md §2.
- Never copy code/data from nival/Blitzkrieg. Never download assets.
- Do not dispatch subagents.

# M1 additions (verbatim from the M1 plan's Global Constraints and Review Focus; these override the M0 text above where they differ)
## Global Constraints

**The M0 rev-2 Global Constraints** (`docs/superpowers/plans/2026-10-23-frontline-m0-rev2.md`, section "Global Constraints") apply unchanged to every task, except where this section changes them. Below are only the additions and changes.

**Where and how to build** (changes)
- **Lane 1** is `C:\fl\a` on branch `m1/phase-a`. The controller cuts it from `main` once the M0 final fix waves (A and B, through `f247a85` or later) have fast-forwarded `main` (`git -C C:\fl\a switch -c m1/phase-a main`).
- **Lane 2** worktrees are `C:\fl\m1tNN`, on branch `m1/tNN-<slug>`, cut from `m1/phase-a` at the commit their dependencies need.
- **Shared lane-2 build dir (R31).** Every cargo command in a lane-2 worktree runs with `CARGO_BUILD_BUILD_DIR=C:/t/fl/28/2710d09957a748`. Only one lane-2 worktree builds at a time. Lane 1 keeps the per-worktree dir from the repo config.
- **At most two builders.** Those are the lane-1 task and the lane-2 task. The controller's gate and merge runs use the slot of the lane they gate. A docs-only task that never runs cargo (Task 1) doesn't count as a builder.
- **`.gate-base` (R33).** At every dispatch the controller writes the task's base commit to `<worktree>/.gate-base`. Implementers never edit it. `check-scope` and `test-guard` judge only the task's own diff.
- `SCCACHE_BASEDIRS` stays unset (R59).
- **Dependencies.**
  - Task 3 adds `png = "0.18"` to `[workspace.dependencies]` and to `fl_content`. It is already in the lock, so nothing is downloaded. Task 3 may commit the resulting `Cargo.lock` delta (R50).
  - **PNG decoding lives in `fl_content` only** (role `Content` in `xtask/src/arch.rs`). `fl_content` is the only crate with a direct `png` dependency; it decodes the layers once, at content load, into integer `MapLayers`. `png` and its dependencies (`crc32fast`, `fdeflate`, `flate2`, `miniz_oxide`, `simd-adler32`, `bitflags`) are pure Rust with no Bevy crate, so the `Content` denylist and the `Sim`/`Feature` allowlist (`bevy_app` + the `bevy_ecs` family; it constrains only `bevy*` crates) both accept them in `fl_sim`'s tree. No sim crate, no `fl_test_utils` and no `fl_cli` code calls `png`; they use `fl_content::{decode_layers, write_layers}`. The client never decodes the layers either: it reads `MapDef.layers` from `ContentDb`.
  - Task 5 adds `fl_test_utils.workspace = true` to `fl_sim`'s `[dev-dependencies]`: the `crates/fl_sim/tests/*.rs` files of Tasks 5–12 use `fl_test_utils` (`ascii_terrain_content`, `ScenarioBuilder`, `assert_deterministic`, …). `arch-check` allows dev edges between non-client crates. Only integration tests (`tests/`) may use it; a `#[cfg(test)]` unit test inside `fl_sim/src` would see a second copy of `fl_sim`.
  - No other task edits `Cargo.toml` or `Cargo.lock` unless its **Files** list says so.
  - `tempfile` (already a workspace dependency) may be added as a dev-dependency where a task's Files list says so, and as a normal dependency of `fl_test_utils` (Task 5).
  - `fl_sim`'s `build.rs` sets `build_id` from `git rev-parse --short HEAD`, so `fl_sim` and its dependents rebuild after every commit. That's expected; it isn't a cache miss to investigate.

**Determinism** (additions)
- **Gameplay-driving state is `Hashed` (R60):** `TerrainGrid`, `PathQueue`, `PathFollow` and the `MoveOrder` queue.
- **Navigation data and caches are `Derived`:** `NavGrid`, `HpaGraphs` and `FlowCache`.
- **A Derived cache is outcome-neutral (ADR-0015).** It may change wall-clock time and observe-only counters. It never changes state, the budget a request is charged, or the tick on which a request completes. Every path request is charged its cold cost, the work a from-scratch computation does, whether or not a cache answered it.
- **Names, never dense ids, in hashed and saved state (R52, R54).** Terrain types are stored as a palette of terrain ids (strings). `MoveClass` is an enum.
- **Derived state stays consistent at mutation boundaries (R53).** A spawn, a `debug.set_terrain` or a reload updates every derived index it affects before the next system runs, so a save taken right after it loads to equal derived state.
- **Integer costs.** Path, flow and clearance costs are `u32`/`u16`/`u8` integers with explicit tie-breaks. Positions stay `Fx`, and nothing in `nav` uses floats.
- **Core systems** go into the core slot of their hook (R46). Order among core systems in one hook is explicit (`.chain()`, `.before`, `.after`). New messages are named `"core.msg.<x>"` (R47) and are added only with `SimBuilder::add_message`.
- **Transient is per-tick and framework-enforced (R72, ADR-0006 refinement).**
  - `Input::Housekeeping` removes every `Class::Transient` component from every entity and resets every Transient resource at the start of each tick. A Transient resource is `Class::transient()` (reset to `Default`), `Class::TransientReset(fn)` (for example a `clear` that keeps capacity), or `Class::TransientKept("reason")` when its owner empties it elsewhere. Plain `Class::Transient` on a resource panics.
  - **No per-tick Transient components on units (R76).** Removing one is an archetype move per entity per tick. Per-tick per-unit results go into a Transient scratch resource keyed and sorted by `SimId` (Task 11's `SteerPushes`).
  - Scratch buffers kept between ticks to save allocations (search heaps, `g`/parent arrays) are resources classified `Class::TransientReset(<clear that keeps capacity>)`, or plain locals inside one system run. Never `Local<T>`.
  - `step_tick` clears every `add_message` buffer at the end of the tick, so a message is read in the tick it was written. Anything that must reach a later tick (a path request, a replan flag) is `Hashed` state, as `PathQueue` and `PathFollow` are.
- **One `Derived` rebuild per feature (R74).** A feature registers at most one rebuild with `SimBuilder::register_rebuild`, and a second call panics. Core already registers `rebuild_spatial` (`crate::spatial::build`). From Task 5, core's one rebuild is `rebuild_core`, registered in `CoreFeature::build`: `rebuild_spatial`, then `nav::rebuild_nav` (`NavGrid`, then `HpaGraphs` from Task 7, then an empty `FlowCache` from Task 10). `load_world` and Task 12's reload run the rebuilds in feature load order. `Class::Derived` names no rebuild.
- **Randomness (`Stream` tokens).** Draws come only from `fl_sim::rolls(seed, tick, sim_id, stream)`, where `stream` is the `Stream` that `SimBuilder::register_stream(name)` returned. It is captured in a closure or passed with `IntoSystem::with_input`, as `wander_system` and `scale_fire` do. `fl_math::Rolls::new`/`roll` are clippy-banned in sim crates. Setup code that draws from a scenario's own seed (`ScaleScenario::populate`) keeps its existing scoped `#[expect(clippy::disallowed_methods)]`. M1 adds no stream: `"debug.wander"` keeps its draws.
- **Hash collections.** In sim crates, clippy bans std, `bevy_platform`, `hashbrown` and `indexmap` hash maps and sets, `RandomState`, and the `Entity`-keyed maps and sets. `fl_content` (edge profile) and the client crates ban the std ones. Nav caches use `Vec`s indexed by cell or node, and `BTreeMap`s keyed by `FlowKey`/`MoveProfile`. Nothing is keyed or ordered by `Entity`.
- **Spawn funnel and `SimEntity` (R73).** `SimEntity`'s field is private. Every unit is spawned through `SpawnRequest` (in a tick) or `setup_spawn` (setup and tests between ticks), and despawned through `DespawnRequest` or `setup_despawn`. In debug builds `step_tick` panics on an entity that holds a classified component but no `SimEntity`, and on a type first registered mid-match outside the builder.
- **`debug.*` commands** are issued as `LocalPlayer`, and the sim skips the ownership filter for `DEBUG_ONLY` kinds (R44).
- **Reject codes (R71).** `RejectReason` is `#[non_exhaustive]`, and every rejection is reported by its stable code (`RejectReason::code()`, in `SimEvent::CommandRejected` and the `core.command_rejected` journal record). M1 adds no `RejectReason` variant. Its one new reason is `RejectReason::Feature("core.invalid_target")`, code `"core.invalid_target"` (Tasks 5, 9). Tests assert codes, never `Debug` names.
- **Command input caps (R77).** `fl_sim::MAX_COMMAND_UNITS` (256 units per command, `core.too_many_units`) and `fl_sim::MAX_WAYPOINTS` (32) stay code constants until C7. From Task 8, `MAX_WAYPOINTS` bounds `MoveOrder.orders`, every kind counted. A queued move, rotate or follow onto a unit whose orders are full rejects the whole command with `core.queue_full`, changing nothing.
- **Saved and recorded command ids are remapped by name (R73/I1).** New command names (`core.follow` sorts before `core.move`) shift the dense command ids. `load_world` and `replay_app`/`replay_inputs` map them through the recorded `commands` table, so a new command name never by itself needs a golden re-record.
- **Save compatibility (R70).** `check_save` needs the same feature ids, each saved at a version no newer than the build's, and equal mods. It ignores the rules hash, which Task 12's rebuild path relies on. Per-codec versions and migrations decide what decodes, and the format guard fails a codec whose version rose without a `migrate_from` the committed version. `save_world` returns `Result<_, SaveError>` (`#[non_exhaustive]`; `PendingRequests` while spawn or despawn requests wait), and `SimQuery::Save` can answer `SimReply::Error`.
- **Bad-command tests** assert that gameplay state is unchanged and that `LastSeq` advanced (R45). Late and early commands don't advance `LastSeq` (R49).
- **`SIM_VERSION`** (`crates/fl_sim/src/build.rs`) becomes 2 in Task 5, the first task that changes the hash stream of existing matches. It changes no further in M1; the bump policy is C8, in M2.
- **M0 saves load under M1 (R78).**
  - Task 5 adds `Codec::fill_missing(fn(&mut World) -> Result<(), String>)`. When an older-feature-version save lacks a state name, `load_world` fills it, and otherwise behaves as before. Task 5 also bumps `CoreFeature::version()` to 2 and registers the fill for `core.terrain`.
  - Task 8 registers fills for `core.path_queue` (empty) and `core.path_follow` (`Idle` on every unit with `Mobility`, so a loaded unit equals a live one).
  - The evidence is a protected M0 testbed save fixture generated from the M0 tip of `main` before Task 5's changes (Task 5 Step 0), and `crates/fl_test_utils/tests/m0_save.rs` loading it.

**Protected artifacts in M1** (additions to the M0 bless rule)
- **Which artifacts.** A task that adds sim systems, Hashed codecs, payloads or public API items changes protected snapshots:
  - `crates/fl_test_utils/tests/snapshots/system_order__sim_tick.snap`
  - `crates/fl_test_utils/tests/snapshots/formats__save_codecs.snap`: the codec block of every catalog feature (`<name>@<v> reads …`), traced by `fl_test_utils/tests/formats.rs` since the final M0 fix wave. A new or bumped Hashed codec changes this file.
  - `crates/{fl_protocol,fl_view,fl_sim}/tests/snapshots/formats__*.snap`. `fl_sim`'s `formats__save_codec_formats.snap` now holds only the save and replay layouts, such as `ReplayStart`.
  - `xtask/snapshots/api/*.txt`, for the five API crates `fl_math`, `fl_protocol`, `fl_view`, `fl_content` and `fl_sim`. With all five unchanged, `cargo xtask api --check` prints `api: 5 crates unchanged`.
  - `docs/perf/M*.json` (protected since the final M0 fix wave). Task 21 writes `docs/perf/M1.json.new`, and the lead blesses it.

  A task that changes the testbed hash stream or the scale counters also changes `crates/fl_test_utils/tests/replays/*.replay`, `docs/perf/budgets.ron` and `crates/fl_test_utils/tests/snapshots/perf_counters__*.snap`. `check_budgets` fails a budgeted counter that a run didn't measure, so budgets come only from `FL_RECORD_BUDGETS=1` runs and are never hand-added.
- **The implementer** regenerates them as `.new` files:
  - insta writes `*.snap.new`
  - `cargo xtask api` writes `*.txt.new`
  - `FL_RECORD_GOLDEN=1` writes `*.replay.new`
  - `FL_RECORD_BUDGETS=1` writes `budgets.ron.new`

  The report pastes their diffs (or, for binary replays, the hash-stream change). The implementer never edits a protected file.
- **The lead** blesses with `cargo xtask bless <paths> --reason "M1 Task N: <why>"` before the task's commit. The commit carries `Blessed: <reason>`. When two branches both moved a protected file, the lead regenerates it after the second merge and never hand-merges it.
- **New ADRs** (Task 1) start `proposed`. Only the user accepts them; the lead asks. Tasks that change contract paths cite them in a `Refs:` trailer.

**Client** (additions)
- **Client tests** use `MinimalPlugins`, `TimeUpdateStrategy::ManualDuration` and a fake runner on `fl_view::runner_channel` that acknowledges each tick. Tests wait on those acknowledgements and never sleep; this lesson comes from the flaky despawn test fixed after M0 Task 21.
- **Picking and `Camera::world_to_viewport`** need a window and GPU. Put their logic behind pure functions, test those headless, and cover the wiring with the demo checklist and `brp_` tests.
- **Visuals follow the receive-timeline interpolation (R42).** Rings, paths and markers read the unit visuals' `Transform`, never a separately interpolated position.
- **Input is ignored** while egui wants the pointer or keyboard (bevy_egui 0.43's wants-input state; read its source in `~/.cargo/registry`) and while `SpawnPanel.placing` is true.
- **Every order** is a `PlayerCommand` issued as `LocalPlayer` through `LocalDriver`. The client never mutates sim state.
- **Clippy in client crates** (the root `clippy.toml`, profile `client`) bans the `std::time::Instant` and `std::time::SystemTime` types and std `HashMap`/`HashSet`. Use Bevy's `Time`, `Duration` and `BTreeMap`/`BTreeSet`.

**Observability** (M1 additions; the definition of done requires all of them)
- **Journal kinds:**
  - `core.path_request`
  - `core.path_result`
  - `core.order_changed` (replaces M0's `core.move_order`)
  - `core.terrain_changed`
  - `core.content_reload`
- **Counters:**
  - `path.requests`, `path.queue_len`, `path.nodes_expanded`, `path.los_cells`, `path.budget_spent`, `path.clusters_invalidated`
  - `flowfield.cells`, `flowfield.abstract_nodes`
  - `nav.cells_updated`, `steer.units`, `move.blocked`
- **Spans** (`tracing::info_span!`): `hpa_search` per request, `hpa_cluster_build` per rebuilt cluster, `flowfield_build` per abstract field and per tile.
- **Debug queries** (ADR-0015): `core.debug_queries`, `core.passability`, `core.terrain_at`, `core.clusters`, `core.paths`. Their JSON carries `Fx` as raw `i64` (`Fx::raw`), so no float enters the sim.
- **Log cost.** Per-request and per-unit log records (path requests, steering, flow tiles) are at `trace` level. In M0, `DEBUG` records per spawn cost about 30 ms per tick at 3,000 units, until `debug.ron`'s default filter (`info,fl_sim=debug,fl_sim::spawn=info`) kept them out. The journal and counters carry the per-request facts.

## Review Focus

These are the five failure modes most likely to bite a person using M1 that the spec implies but no roadmap item names, most likely first. Each one's tests are added to the owning task.

1. **Unreachable or blocked goals.** A target in water, off the map or on an island; a tank ordered across a river with no bridge; a bridge removed mid-route. The unit drives to the closest reachable point (`Partial`) or stops (`Unreachable`), with a journal record. It never oscillates, never enters a blocked cell and never panics. Tests: Task 7 `island_goal_is_partial`, `goal_in_water_snaps_to_nearest_shore`; Task 8 `unreachable_goal_goes_to_closest_point`, `bridge_destroyed_mid_route_repaths`; Task 11 `units_never_enter_blocked_cells`.
2. **Queue pressure.** Forty units ordered in one tick, an order replaced before its result arrives, a unit stopped or despawned while queued. The budget holds, results apply in request order, stale requests are discarded, and no request starves. Tests: Task 8 `queue_respects_budget_and_order`, `stale_request_is_discarded`, `despawned_unit_request_discarded`, `no_request_starves`.
3. **Save and load mid-pathing.** A save while requests are queued, while a group follows a flow field, or right after `debug.set_terrain`. Branch hashes are equal, and derived caches rebuild from scratch. Tests: Task 5 `save_right_after_set_terrain`; Task 8 `branch_with_queued_requests`; Task 10 `branch_mid_flow_follow`, `cache_does_not_change_outcomes`; Task 14 suite.
4. **Selection input edges.** Behaviour a player expects in each case:
   - A click on empty ground clears the selection (unless Shift is held).
   - A drag below the threshold is a click.
   - Double-clicking an enemy selects nothing.
   - Control groups lose dead units.
   - Ctrl+digit with an empty selection clears the group.
   - Keys typed into the console don't select.
   - Orders given while paused apply on resume.

   Tests: Task 15 `click_empty_ground_clears_unless_shift`, `tiny_drag_is_a_click`, `control_group_drops_dead_units`, `ctrl_digit_with_empty_selection_clears_group`, `input_ignored_while_console_open`; Task 16 `orders_while_paused_apply_on_resume`.
5. **Hot reload at awkward moments.**
   - A malformed RON file saved mid-edit: the match keeps running and the error is shown.
   - A reload in a match without cheats is refused.
   - A unit type removed while instances live: those instances are dropped and logged, and survivors keep their `SimId`s.
   - A reload while paused works.

   Tests: Task 12 `reload_refused_without_cheats`, `rebuild_drops_vanished_unit_types`; Task 19 `malformed_ron_keeps_match_running`, `removing_a_unit_type_drops_its_units`, `reload_while_paused_applies`.

---


- Gate base: the controller writes it to .gate-base per dispatch (R33). Lane-2 builds use CARGO_BUILD_BUILD_DIR=C:/t/fl/28/2710d09957a748 (R31).
- Never use `git stash` (the stash stack is shared across worktrees and sessions). To set work aside (e.g. to capture a RED run), use a temporary WIP commit and reset it afterwards.
