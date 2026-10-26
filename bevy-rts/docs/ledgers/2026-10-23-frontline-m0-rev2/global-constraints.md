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
- Plan (read only your brief; consult other sections only if your brief points there): C:\fl\a\docs\superpowers\plans\2026-10-23-frontline-m0-rev2.md. Spec: docs/superpowers/specs/2026-10-23-frontline-rts-design.md; ADRs: docs/adr/. Architect's per-crate rework list: C:\fl\a\.superpowers\sdd\2026-10-23-frontline-m0-foundation\architect-rev2-report.md §2.
- Never copy code/data from nival/Blitzkrieg. Never download assets.
- Do not dispatch subagents.
