# Frontline M0 (Foundation) — Revision 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish M0 under spec revision 2: install the Group B verification baseline, rework the completed contract crates to the Group A contracts, then build everything M0 still lacks (runner and drivers, save/load, spatial grid, test utilities, observability, client view, debug tools, game assembly, agent CLI, live debug, probes, contract snapshots, work-counter perf gates) and close with the integration and demo.

**Architecture:** Verification infrastructure lands first (`cargo xtask gate`, generated lints, arch-check, protected tests, hooks), so every later task is checked mechanically. The contract crates (`fl_math`, `fl_protocol`, the new `fl_view`, `fl_content`, the `fl_sim` extension API) are then reworked one slice per task, sequentially in `C:\fl\a`. After that the sim runs behind `SimRunner` (inline or on its own thread) and is driven by `TickDriver`s, and the client sees it only through `fl_view` snapshots, so headless tracks (`fl_sim`, `fl_test_utils`, `fl_cli`, `fl_observe`) and client tracks (`fl_client`, `fl_game`) proceed in two parallel lanes.

**Tech Stack:** Rust 1.99.1 (edition 2024; a dated nightly only for rustdoc JSON), Bevy `=0.20.0` (`bevy_app` + `bevy_ecs` only in sim crates), bevy_egui 0.43, `fixed` (I32F32), serde + ron 0.12 + postcard 1, erased-serde 0.4, xxhash-rust (xxh64), crossbeam-channel 0.5, tracing + tracing-subscriber, clap 4, criterion, proptest, insta, serde-reflection, stats_alloc, cargo_metadata. Tools (installed globally): cargo-nextest, cargo-insta, cargo-deny, cargo-public-api, cargo-mutants, sccache, bevy_brp_mcp / bevy_brp_extras 0.23.

**Spec:** `docs/superpowers/specs/2026-10-23-frontline-rts-design.md` (revision 2) is the source of truth, with the ADRs in `docs/adr/`: 0001 two-crate features · 0002 threaded runner and `fl_view` · 0003 keyed RNG · 0004 stable-string registries · 0005 phases and spawn funnel · 0006 classification and saves · 0007 envelope, `MatchHeader`, canonical rules hash · 0008 debug actions as commands · 0009 gates and protected tests · 0010 work-counter perf gates · 0011 contract governance · 0012 hot reload · 0013 agent environment. Other inputs:
- `docs/superpowers/plans/2026-10-23-frontline-m0-foundation.md`: the revision-1 plan, with the detail of completed tasks and still-valid content this plan refers to as "rev-1 Tn".
- `.superpowers/sdd/2026-10-23-frontline-m0-foundation/architect-rev2-report.md` §2: the per-crate rework list. It is git-ignored, so read it in `C:\fl\a`.
- Bevy API facts: the `bevy-api` skill (from Task 4), then `docs/research/bevy020-api-reference.md`. Never from memory.

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

## Review Focus

These are the five failure modes most likely to bite a user that the spec implies but no rev-1 task exercised, most likely first. Each one's tests are added to the owning task.

1. **Commands at the edges of the envelope.** A duplicate `(player, seq)`, a `seq` that goes backwards across ticks, an `exec_tick` that is late or early, an order given while paused, or a command naming a unit despawned that tick: each is rejected or filtered deterministically, and the state hash equals a run without it. Tests: Task 11 `bad_commands_do_not_change_state`, `seq_must_increase_across_ticks`, `despawned_unit_command_same_tick_filtered`; Task 20 `paused_orders_apply_on_resume`.
2. **Saves at awkward moments.** A save at tick 0, one with driver-held (pending) commands, one after despawns leave `SimId` gaps, and loading against content with an extra earlier-sorting unit: these round-trip byte-identically and keep hashes equal. A save naming an unknown component is a `LoadError`, not a panic. Tests: Task 13 `save_at_tick_zero_round_trips`, `pending_commands_survive_save`, `branch_after_despawns`, `load_remaps_unit_type_ids`, `load_rejects_unknown_component_name`.
3. **Runner failure and shutdown.** A sim panic on the runner thread reaches the client as `Failed` with the panic text, and the client pauses instead of hanging. Dropping the port stops the thread. Tests: Task 12 `threaded_panic_is_reported`, `drop_port_stops_thread`; Task 20 `runner_failure_is_surfaced`.
4. **Feature and registry misconfiguration.** A dependency cycle, a missing dependency, a duplicate or reserved feature id, two features registering the same name, registering after freeze, a `ClientFeature` with no `SimFeature`, or an unclassified component: each is a readable error naming the culprits. Tests: Task 9 `feature_cycle_is_error`, `duplicate_name_panics_naming_both`, `unclassified_sim_type_fails_build`; Task 22 `orphan_client_feature_is_error`.
5. **Client frames that don't line up with ticks.** The first frame before any snapshot, several `TickOutput`s arriving in one frame, and an entity that appears and disappears between frames: none panics, and no event is lost. Tests: Task 20 `first_frame_without_snapshot_draws_nothing`, `multiple_outputs_per_frame_keep_all_events`, `despawned_record_removes_visual`.

Rev-1's Review Focus items (hostile content, map edges, broken mods, camera edge cases) stay covered by the completed tests and by Tasks 11 and 16.

---

## Completed work

| rev-1 task | Status | Commits | Rework in this plan |
|---|---|---|---|
| T1 workspace scaffold | done, reviewed | `a655efc..0090f47` | Task 1 (config, manifests), Task 2 (`check_deps.sh` → `xtask arch-check`) |
| T2 `fl_math` | done, reviewed | `03c3f58..da0fa1c` | Task 5 (keyed RNG) |
| T3 `fl_protocol` | done, reviewed | `0090f47..03c3f58` | Task 6 (envelope, payloads, header) |
| T4–T5 `fl_content` | done, reviewed | `da0fa1c..24d0421` | Task 8 (canonical hash, freeze, tables) |
| T6 `fl_sim` skeleton | done, reviewed | `24d0421..ccae65d`, `2eed139` | Tasks 9, 10 |
| T7 commands, spawn, units, movement, scenario, scale | in progress on `m0/phase-a` in `C:\fl\a`, finishing against the rev-1 contracts | — | Tasks 9–11 port it; its behaviour survives |
| T12 client camera and ground | committed `99e42d6` on `m0/t12-camera` (`C:\fl\t12`); the full gate has not been confirmed | — | none (contract-neutral). After Task 4, the controller rebases it onto `m0/phase-a`, runs `cargo xtask gate task --track client` in `C:\fl\t12`, and merges. This first build in a second worktree also verifies sccache reuse (Task 1 Step 2). |

**Preconditions.** T7 is committed and reviewed on `m0/phase-a` before Task 1 starts, because Tasks 1–4 run in the same worktree. The detail of every completed task is in the rev-1 plan.

**Rev-1 → rev-2 numbering for remaining work:** T8→14, T9→15, T10→16, T11→17, T13→20, T14→21, T15→22, T16→23, T17→30, T18→18, T19→19, T20→24, T21→25.

## Execution topology

At most two builders run at once. **Lane 1** is `C:\fl\a` (sequential; the controller's own gate runs use this lane's slot, between tasks). **Lane 2** is one parallel worktree at a time, `C:\fl\tNN`. A lane-2 branch is cut from `m0/phase-a` at the commit its dependencies need. The controller merges it back into `m0/phase-a` (`--no-ff`) and re-runs `gate task`. `main` is fast-forwarded to `m0/phase-a` with `gate pr` green after Tasks 4, 13 and 30.

```
Phase I   lane 1: [T7 rev-1 committed] → 1 → 2 → 3 → 4                       (infrastructure)
          lane 2:                         26 (after 1)
Phase II  lane 1: 5 → 6 → 7 → 9 (after 8 merged) → 10 → 11 → 12 → 13        (contract rework, new modules)
          lane 2: 27 (after 4) → T12 rebase/gate/merge → 8 (after 6) → 20 (after 7, 8) → 21
Phase III lane 1: 14 → 15 → 17 → 18 → 29 → 28
          lane 2: 19 → 16 (after 14) → 22 (after 14, 19, 21) → 23 (after 18) → 24 → 25 (after 22, 27)
Phase IV  lane 1: 30
```

| Parallel pair | Lane-1 task owns | Lane-2 task owns |
|---|---|---|
| 2–4 ∥ 26 | `xtask/**`, `deny.toml`, `.config/**`, `OWNERS.toml`, `CLAUDE.md`, `.claude/**`, `.mcp.json`, `rust-analyzer.toml`, `crates/*/clippy.toml`, `README.md` | `probes/dynlink/**`; the dynamic-linking section of `docs/research/bevy020-api-reference.md`; the `dynamic` feature line in `crates/fl_game/Cargo.toml` |
| 5–6 ∥ 27 | `crates/fl_math/**`, `crates/fl_protocol/**`, mechanical `crates/fl_sim/**` call sites | `probes/remote/**`; the `bevy_remote` section of `docs/research/bevy020-api-reference.md` |
| 7 ∥ 8 | `crates/fl_view/**` | `crates/fl_content/**` |
| 9–13 ∥ 20–21 | `crates/fl_sim/**` | `crates/fl_client/**` (except T12's `camera.rs`, `map_view.rs`) |
| 14 ∥ 19 | `assets/base/**`, `crates/fl_catalog/**` | `crates/fl_observe/**`, `debug.ron` |
| 15 ∥ 16 | `crates/fl_sim/src/spatial.rs`, `crates/fl_sim/tests/spatial.rs` | `crates/fl_cli/**` (lib, main, data_check, hash_dump, tests) |
| 17–18 ∥ 22 | `crates/fl_test_utils/**`, `crates/fl_sim/src/{observe,replay,step,commands,spawn,builder,build,lib}.rs` | `crates/fl_game/**`, `crates/fl_client/src/lib.rs` |
| 29 ∥ 23–24 | `crates/fl_sim/src/{scale,step,spawn,lib}.rs`, `crates/fl_sim/tests/scale_mix.rs`, `crates/fl_test_utils/src/{perf,lib}.rs`, `crates/fl_test_utils/tests/perf_counters.rs`, `docs/perf/budgets.ron(.new)` | `crates/fl_cli/**`, `crates/fl_sim/benches/sim_tick.rs` |
| 28 ∥ 25 | `xtask/src/{api,gate}.rs`, `xtask/snapshots/**`, `crates/{fl_protocol,fl_view,fl_sim}/tests/formats.rs`, `crates/fl_sim/src/classify.rs`, `crates/fl_test_utils/tests/system_order.rs` | `crates/fl_client/src/debug/**`, `crates/fl_client/tests/remote.rs`, `crates/fl_game/src/builder.rs`, `crates/fl_game/tests/remote.rs` |

## File map (new or substantially changed in this plan)

```
.cargo/config.toml  Cargo.toml  .gitignore  deny.toml  OWNERS.toml  CLAUDE.md  .mcp.json  rust-analyzer.toml  debug.ron
.config/nextest.toml   .claude/{settings.json,rules/*.md,agents/*.md,skills/bevy-api/SKILL.md}
xtask/src/{main,gate,changed,arch,clippy_gen,owners,scope,test_guard,bless,hooks,api}.rs  xtask/clippy-template.toml  xtask/snapshots/api/*
probes/{dynlink,remote}/
crates/fl_math/src/rng.rs                                  crates/fl_protocol/src/{ids,command,payloads,header,tables}.rs
crates/fl_view/src/{snapshot,event,journal,runner,driver}.rs
crates/fl_content/src/{hash,registry,db}.rs
crates/fl_sim/src/{registry,builder,classify,schedule,step,counters,rng,hash,commands,debug_cmds,spawn,publish,
                   spatial,observe,replay,scale}.rs  src/runner/{mod,threaded,test_driver,replay_driver,lockstep}.rs
                   src/save/{mod,format,load}.rs     tests/{feature,classify,schedule,hash,commands,funnel,movement,runner,save,
                   spatial,observe,replay,formats}.rs
crates/fl_catalog/src/lib.rs   assets/base/**
crates/fl_test_utils/src/{lib,builder,determinism,fitness}.rs  tests/{determinism,golden,perf_counters,system_order}.rs
crates/fl_observe/src/{lib,config,logging,profiler,sink}.rs
crates/fl_cli/src/{lib,main,data_check,hash_dump,bench,agent,inspect,journal_cmd,save_cmd}.rs
crates/fl_client/src/{feature,driver,link,view,placeholder}.rs  src/debug/{mod,console,spawn_panel,stats,inspector,remote,perf,screenshot}.rs
crates/fl_game/src/{lib,builder,main}.rs
docs/{debugging.md,m0-demo-checklist.md}  docs/perf/{M0.json,M0.md,budgets.ron}
```

---

## Phase I — verification infrastructure (lane 1, `C:\fl\a`)

### Task 1: Per-worktree build dir, repo Cargo config, workspace manifest

**Track:** contract · **Refs:** ADR-0013, ADR-0001, ADR-0002 · **Gate:** interim

**Files:**
- Modify: `.cargo/config.toml`, `Cargo.toml`, `.gitignore`, every `crates/*/Cargo.toml`, `crates/fl_client/src/lib.rs` (drop `pub mod game`), `scripts/check_deps.sh` (add the new crates; Task 2 deletes the script)
- Delete: `crates/fl_client/src/game.rs` (rev-1 stub; `GameBuilder` moves to `fl_game`, which is the only crate that may see both halves)
- Create compiling stubs:
  - `crates/fl_view/{Cargo.toml,src/lib.rs}`
  - `crates/fl_observe/{Cargo.toml,src/lib.rs}`
  - `crates/fl_game/src/lib.rs`
  - `xtask/{Cargo.toml,src/main.rs}`: prints `usage: cargo xtask <command>` and exits 2
  - `probes/dynlink/{Cargo.toml,src/main.rs}` and `probes/remote/{Cargo.toml,src/main.rs}`, with package names `probe_dynlink` and `probe_remote`

**Interfaces:**
- Produces:
  - the final M0 dependency graph (table in Step 4)
  - the `cargo xtask` alias
  - the stub crates `fl_view`, `fl_observe`, `fl_game` (lib + bin) and `xtask`

- [ ] **Step 1: Probe the build-dir template (ruling R19).** Replace `.cargo/config.toml` with:
  ```toml
  [build]
  build-dir = "C:/t/fl/{workspace-path-hash}"
  [target.x86_64-pc-windows-msvc]
  linker = "rust-lld.exe"
  [alias]
  xtask = "run -q -p xtask --"
  ```
  - Run `cargo build -p fl_math`. Expected: a new `C:/t/fl/<hash>/debug/.fingerprint/` exists, and `C:\fl\a\target\debug\` holds only final artifacts (no `.fingerprint`).
  - Run `cargo metadata --format-version 1 --no-deps` and record whether it reports a `build_directory` field. Task 4's `WorktreeRemove` hook relies on it.
  - **Fallback**, if a literal `{workspace-path-hash}` directory appears: delete the `build-dir` line. The controller then exports `CARGO_BUILD_BUILD_DIR=C:/t/fl/<worktree-dir-name>` for every agent it dispatches, and Task 4 writes that rule into `CLAUDE.md`.
  - Record the outcome in the task report.
- [ ] **Step 2: sccache.** Add `rustc-wrapper = "sccache"` under `[build]` and `[env] SCCACHE_BASEDIRS = "C:/fl"`. Verify `sccache --show-stats` counts compile requests after Step 6. Reuse across worktrees is checked at the T12 rebase. If it shows no hits for dependencies, remove both lines and report; the build stays correct either way.
- [ ] **Step 3: Root manifest.**
  - **Members:** `members = ["crates/*", "xtask", "probes/*"]` and `default-members = ["crates/*", "xtask"]`. If cargo rejects globs in `default-members`, list the crates.
  - **New `[workspace.dependencies]`:** `insta = { version = "1", features = ["ron", "json", "filters"] }`, `serde-reflection = "0.5"`, `crossbeam-channel = "0.5"`, `tracing-subscriber = { version = "0.3", features = ["env-filter", "json", "fmt", "registry"] }`, `stats_alloc = "0.1"`, `bevy_brp_extras = "=0.23.0"`, `cargo_metadata`, `toml`, `globset`, `anyhow`, `tempfile`, and path deps `fl_view`, `fl_observe`, `fl_game`. Versions are the latest compatible at implementation time; Bevy and `bevy_brp_extras` stay exact.
  - **Bevy:**
    - `bevy = { version = "=0.20.0", default-features = false }`
    - `bevy_app = { version = "=0.20.0", default-features = false, features = ["std"] }`
    - `bevy_ecs = { version = "=0.20.0", default-features = false, features = ["std", "trace", "debug"] }`
    - Confirm feature names in `~/.cargo/registry/src/*/bevy_ecs-0.20.0/Cargo.toml`, and add a feature only when a compile error proves it is needed. `debug` stays: `shuffle_seed` needs it (R14).
  - **Profiles:** `[profile.dev] opt-level = 1, debug = "line-tables-only"`; `[profile.dev.package."*"] opt-level = 3, debug = false`; `[profile.release] debug = "line-tables-only"`.
- [ ] **Step 4: Crate manifests.** Every crate keeps `[lints] workspace = true`.

  | crate | dependencies | dev-dependencies | features |
  |---|---|---|---|
  | fl_math | fixed, serde | proptest, ron, postcard, insta | — |
  | fl_protocol | fl_math, serde | postcard, ron, insta, serde-reflection | — |
  | fl_view | fl_math, fl_protocol, serde, serde_json, crossbeam-channel | postcard, insta, serde-reflection | — |
  | fl_content | (as now) + postcard | insta | — |
  | fl_sim | (as now) + fl_view, crossbeam-channel | criterion, proptest, insta, serde-reflection, tracing-subscriber | — |
  | fl_catalog | fl_sim | — | — |
  | fl_observe | fl_math, fl_protocol, fl_view, tracing, tracing-subscriber, serde, serde_json, ron, thiserror | fl_sim, fl_content, bevy_app, bevy_ecs, tempfile | — |
  | fl_test_utils | fl_math, fl_protocol, fl_view, fl_content, fl_sim, fl_catalog, bevy_app, bevy_ecs, insta, proptest, stats_alloc, serde_json, postcard | tracing-subscriber | — |
  | fl_cli (lib+bin) | fl_math, fl_protocol, fl_view, fl_content, fl_sim, fl_catalog, fl_test_utils, fl_observe, bevy_app, bevy_ecs, clap, thiserror, tracing, ron, serde, serde_json, postcard | tempfile | — |
  | fl_client | bevy, bevy_egui, fl_math, fl_protocol, fl_view, fl_content, tracing, crossbeam-channel, serde, serde_json, postcard, ron; optional bevy_brp_extras | — | `debug_remote = ["bevy/bevy_remote", "dep:bevy_brp_extras"]` |
  | fl_game (lib+bin) | fl_client, fl_sim, fl_view, fl_catalog, fl_content, fl_protocol, fl_math, fl_observe, bevy, clap, tracing | tempfile | `debug_remote = ["fl_client/debug_remote"]`, `trace = ["bevy/trace"]`, `trace_tracy = ["bevy/trace_tracy"]`, `trace_chrome = ["bevy/trace_chrome"]` |
  | xtask | cargo_metadata, clap, serde, serde_json, toml, globset, anyhow | tempfile | — |
  | probes/dynlink | bevy, fl_sim, fl_content | — | `dynamic = ["bevy/dynamic_linking"]` |
  | probes/remote | bevy, bevy_brp_extras, crossbeam-channel | — | — |

  The `bevy` feature list for `fl_client`, `fl_game` and the probes is Bevy's `default` list copied explicitly from `bevy-0.20.0/Cargo.toml`, minus `bevy_audio` and `vorbis` (M2 adds them), so the list is visible and auditable. T12's `map_view.rs` needs gizmos, which `default` includes. `probes/remote` adds `bevy_remote`.
- [ ] **Step 5: `.gitignore`.** It lists `/target`, `/logs/`, `/.superpowers/`, `*.log`, `.claude/settings.local.json` and `.claude/worktrees/`. Remove `/.claude/`.
- [ ] **Step 6: Cold build** (background, 15+ min): `cargo build --workspace --all-targets`. Expected: success with no warnings.
- [ ] **Step 7: Interim gate.** Expected: all green, and `check_deps.sh` prints `deps OK`. Add `fl_view` to its no-Bevy list and `fl_observe` to its headless list.
- [ ] **Step 8: Commit** `build: per-worktree build dir, rust-lld, sccache, workspace manifest for M0 rev 2` with trailer `Refs: ADR-0013`.

### Task 2: `xtask` gate levels, arch-check, generated clippy bans, cargo-deny, nextest profile

**Track:** contract · **Refs:** ADR-0009, ADR-0001 · **Gate:** interim, then `cargo xtask gate task` from Step 6

**Files:**
- Create:
  - `xtask/src/{gate,changed,arch,clippy_gen,cmd}.rs`, `xtask/clippy-template.toml`
  - `xtask/tests/fixtures/{metadata_ok,metadata_client_sim,metadata_features}.json`, made by running `cargo metadata --format-version 1` on the workspace and then hand-editing the copies
  - `deny.toml`, `.config/nextest.toml`
  - generated `crates/{fl_math,fl_protocol,fl_view,fl_content,fl_sim}/clippy.toml`
- Modify: `xtask/src/main.rs`, root `clippy.toml` (now generated, client profile), `README.md` (gate and run sections)
- Delete: `scripts/check_deps.sh`

**Interfaces:**
- Produces:
```rust
// gate.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)] pub enum Level { Task, Pr, Nightly, Milestone }
#[derive(Debug, Clone, PartialEq)] pub enum Outcome { Pass, Warn(String), Skip(String), Fail(String) }
pub struct Ctx { pub root: PathBuf, pub level: Level, pub base: String, pub touched: Vec<String>, pub tracks: Vec<String>, pub milestone: Option<String> }
pub struct Step { pub name: &'static str, pub levels: &'static [Level], pub run: fn(&Ctx) -> Outcome }
pub fn steps() -> Vec<Step>;                     // ordered table; Tasks 3, 4 and 28 append rows
pub struct GateReport { pub level: Level, pub rows: Vec<(&'static str, Outcome, Duration)>, pub total: Duration }
impl GateReport { pub fn passed(&self) -> bool; pub fn render(&self) -> String; }
// changed.rs
pub fn merge_base(root: &Path, base: &str) -> anyhow::Result<String>;
pub fn changed_files(root: &Path, base_rev: &str) -> anyhow::Result<Vec<PathBuf>>;   // committed since base ∪ staged ∪ unstaged ∪ untracked; repo-relative, sorted
pub fn touched_packages(meta: &cargo_metadata::Metadata, files: &[PathBuf]) -> Vec<String>;   // root Cargo.toml/Cargo.lock/.cargo/** → every package
// arch.rs
pub struct Violation { pub krate: String, pub detail: String }
pub fn check_edges(meta: &cargo_metadata::Metadata) -> Vec<Violation>;
pub fn check_tree(krate: &str, tree: &str) -> Vec<Violation>;   // `cargo tree -p <krate> -e normal,build --prefix none` output
// clippy_gen.rs
pub struct Template { /* parsed clippy-template.toml */ }
pub fn render(t: &Template, krate: &str, resolvable: &dyn Fn(&str) -> bool) -> Option<String>;
pub fn drift(root: &Path) -> anyhow::Result<Vec<PathBuf>>;
```
- CLI: `cargo xtask gate <task|pr|nightly|milestone> [--base main] [--track T]... [--milestone M0]`, `cargo xtask arch-check`, `cargo xtask gen-clippy [--check]`.

**Allowed workspace edges** (`check_edges`; normal and build edges, plus dev edges for client crates):

| crate | may depend on |
|---|---|
| fl_math | — |
| fl_protocol | fl_math |
| fl_view | fl_math, fl_protocol |
| fl_content | fl_math, fl_protocol |
| fl_sim | fl_math, fl_protocol, fl_view, fl_content |
| fl_feat_x | fl_math, fl_protocol, fl_view, fl_content, fl_sim, plus `fl_feat_y` for each y in `[package.metadata.fl] sim_deps` |
| fl_feat_x_client | fl_math, fl_protocol, fl_view, fl_content, fl_client, fl_feat_x |
| fl_catalog | fl_sim, fl_feat_* (not `_client`) |
| fl_observe | fl_math, fl_protocol, fl_view |
| fl_test_utils | fl_math, fl_protocol, fl_view, fl_content, fl_sim, fl_catalog |
| fl_cli | the above headless crates plus fl_test_utils, fl_observe |
| fl_client | fl_math, fl_protocol, fl_view, fl_content (dev edges checked too: never fl_sim) |
| fl_game | anything except xtask and probes |
| xtask | — |
| probes/* | unchecked |

**Banned external crates** (`check_tree`):
- `fl_math`, `fl_protocol`, `fl_view`: any crate whose name starts with `bevy`.
- `fl_content`: any `bevy*` except the `bevy_ecs` family: `bevy_ecs`, `bevy_ecs_macros`, `bevy_ecs_macro_logic`, `bevy_ptr`, `bevy_utils`, `bevy_platform`, `bevy_tasks`, `bevy_reflect`, `bevy_reflect_derive`, `bevy_macro_utils`.
- Headless crates (`fl_sim`, `fl_feat_*` not `_client`, `fl_catalog`, `fl_observe`, `fl_test_utils`, `fl_cli`): `bevy`, `bevy_internal`, `bevy_render`, `bevy_window`, `bevy_input`, `bevy_time`, `bevy_winit` and `wgpu`.

**Clippy template profiles** (`xtask/clippy-template.toml`):
- **`sim`** (`fl_protocol`, `fl_view`, `fl_sim`, `fl_feat_*` not `_client`):
  - `disallowed-types`: `f32`, `f64`, `std::collections::{HashMap,HashSet}`, `bevy_platform::collections::{HashMap,HashSet}`, `std::time::{Instant,SystemTime}`, `bevy_ecs::system::ParallelCommands`, `bevy_time::Time`, `fl_math::Pcg32`
  - `disallowed-methods`: `std::time::SystemTime::now`, `std::time::Instant::now`, `std::thread::spawn`, `rand::rng`, `rand::random`, `std::env::var`
- **`edge`** (`fl_math`, `fl_content`): the collection, time, thread and env bans only.
- **`client`** (the root `clippy.toml`, which every other crate reads): `std::collections::{HashMap,HashSet}`, `std::time::{Instant,SystemTime}`.
- **`[[pending]]`** `{ crate, path, until }` entries are omitted from the generated file, with a comment `# pending: <path> until <until>`. Seed it with `{ crate = "fl_sim", path = "fl_math::Pcg32", until = "Task 10" }`, plus any other ban that existing code violates, each naming the task that removes it. Never drop a ban silently.
- Only paths that resolve in the crate's dependency graph are emitted.

**Gate rows added by this task:**

| step | levels | command and rule |
|---|---|---|
| `fmt` | all | `cargo fmt --all --check` |
| `clippy-drift` | all | `gen-clippy --check` |
| `clippy` | task (touched), pr/milestone (workspace minus probes) | `cargo clippy -p … --all-targets -- -D warnings` |
| `arch-check` | task, pr, milestone | `check_edges` plus `check_tree` for every headless or pure crate |
| `test-headless` | all | headless packages only, so the sim proves it builds without client code: `cargo nextest run --profile agent -p <pkgs> -E '<filter>'`. **Task:** touched headless packages, then `-p fl_sim -p fl_test_utils -E 'test(/smoke_/)'`. **Pr:** all headless packages. The filter excludes `gpu_` and `brp_` always, and `milestone_` below milestone. Env: `FL_SHUFFLE_SEEDS=1` (task) or `3` (pr, milestone); `PROPTEST_CASES=4096` (nightly). |
| `test-client` | task (if `fl_client`/`fl_game` touched), pr, milestone | `cargo nextest run --profile agent -p fl_client -p fl_game` with the same filter |
| `deny` | pr, milestone | `cargo deny check` |
| `data-check` | task, pr, milestone | `cargo run -q -p fl_cli -- data-check assets/base`. Skip with a reason until both `crates/fl_cli/src/data_check.rs` and `assets/base/mod.ron` exist. |
| `gpu` | nightly, milestone | `cargo nextest run --profile agent --workspace -E 'test(/gpu_/)'` |
| `brp` | nightly, milestone | `cargo nextest run --profile agent -p fl_game --features debug_remote -E 'test(/brp_/)'` |
| `perf-file` | milestone | `docs/perf/<milestone>.json` exists |

**Output format.**
- One line per step: `PASS fmt (2.1s)`, `WARN …`, `SKIP data-check: <reason>` or `FAIL clippy (34.0s)`.
- Each FAIL line is followed by at most 40 lines of the failing command's tail.
- Last line: `gate <level>: PASS in <n>s` or `gate <level>: FAIL (<step>, <step>) in <n>s`.
- Exit code 0 or 1. Above 300 s at task level, print `WARN gate task took <n>s (budget 300s)`.

**`deny.toml`:**
- Licences: allow the permissive set the tree needs (MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2/3-Clause, ISC, Zlib, Unicode-3.0, CC0-1.0, MPL-2.0); never anything NC, ND, SA or GPL-family.
- `[bans] deny = [{ name = "bincode" }, { name = "bevy_dynamic_plugin" }]`.
- Multiple versions of any `bevy_*` crate fail. Use the per-crate key that the installed cargo-deny supports.
- Advisories from the default database; sources from crates.io only.

**`.config/nextest.toml`:**
- `[profile.agent]`: `failure-output = "final"`, `success-output = "never"`, `status-level = "fail"`, `final-status-level = "fail"`, `fail-fast = { max-fail = 5 }`, `slow-timeout = { period = "60s", terminate-after = 3 }`, and `junit.path = "junit.xml"`.
- `[test-groups] gpu = { max-threads = 1 }`, with an override that puts `test(/gpu_/)` in it.

- [ ] **Step 1: Write the failing tests** (`xtask/src/*` unit tests; no cargo invocations):
```rust
#[test] fn touched_maps_files_to_packages()        // ["crates/fl_sim/src/x.rs", "docs/a.md"] → ["fl_sim"]
#[test] fn root_manifest_touches_everything()      // ["Cargo.toml"] → every package
#[test] fn edges_flag_client_to_sim()              // metadata_client_sim.json → [Violation { krate: "fl_client", detail: "fl_client -> fl_sim not allowed" }]
#[test] fn edges_accept_fixture_workspace()        // metadata_ok.json → []
#[test] fn feature_edge_needs_declared_dep()       // metadata_features.json: fl_feat_b -> fl_feat_a without sim_deps = ["a"] → 1 violation
#[test] fn tree_flags_render_in_headless_crate()   // check_tree("fl_sim", "bevy_render v0.20.0") → 1; check_tree("fl_client", same) → 0
#[test] fn tree_flags_any_bevy_in_pure_crate()     // check_tree("fl_view", "bevy_ecs v0.20.0") → 1
#[test] fn tree_allows_ecs_family_in_content()     // fl_content: "bevy_ecs v0.20.0\nbevy_ptr v0.20.0" → 0; "bevy_app v0.20.0" → 1
#[test] fn render_omits_unresolvable_paths()       // fl_view (no bevy_time dep) → no "bevy_time::Time"; fl_protocol → contains "\"f32\""
#[test] fn pending_entry_is_omitted_with_comment() // fl_sim → no "fl_math::Pcg32" entry; contains "# pending: fl_math::Pcg32 until Task 10"
#[test] fn report_renders_summary_line()           // rows [fmt Pass, clippy Fail] → last line == "gate task: FAIL (clippy) in 3s"
```
- [ ] **Step 2: Run** `cargo test -p xtask`. Expected: FAIL (items not defined).
- [ ] **Step 3: Implement** the modules, the templates, `deny.toml` and `nextest.toml`. Run `cargo xtask gen-clippy` to write the per-crate files. Delete `scripts/check_deps.sh`. Rewrite README's gate section to show `cargo xtask gate task`.
- [ ] **Step 4: Run** `cargo test -p xtask`. Expected: PASS.
- [ ] **Step 5: Verify the tools.**
  - `cargo xtask arch-check` → `arch-check: OK (<n> packages)`.
  - `cargo xtask gen-clippy --check` → `gen-clippy: up to date (6 files)`.
  - `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`.
  - **Primitive ban check:** temporarily add `pub fn _probe(x: f32) -> f32 { x }` to `crates/fl_view/src/lib.rs`, then run `cargo clippy -p fl_view -- -D warnings`. Expected: a `disallowed_types` error naming `f32`. Remove the line. If clippy cannot ban a primitive this way, report it (a spec §12.2 gap) and keep `float_arithmetic` as the float guard.
- [ ] **Step 6: Run** `cargo xtask gate task --track contract`. Expected: last line `gate task: PASS in <n>s`, with n ≤ 300.
- [ ] **Step 7: Commit** `build(xtask): gate levels, arch-check, generated clippy bans, cargo-deny, nextest profile` with trailer `Refs: ADR-0009`.

### Task 3: `OWNERS.toml`, `check-scope`, `test-guard`, `bless`

**Track:** contract · **Refs:** ADR-0011, ADR-0009

**Files:**
- Create: `OWNERS.toml`, `xtask/src/{owners,scope,test_guard,bless}.rs`, `xtask/tests/fixtures/guard/*.diff`
- Modify: `xtask/src/{main,gate}.rs`

**`OWNERS.toml`** (data; tracks are evaluated in file order, and the first match wins):
```toml
[[track]]
name = "contract"
paths = ["crates/fl_math/**", "crates/fl_protocol/**", "crates/fl_view/**",
  "crates/fl_sim/src/{builder,registry,classify,schedule,feature,step}.rs", "crates/fl_sim/src/runner/**", "crates/fl_sim/src/save/**",
  "crates/fl_content/src/{registry,mods,hash,kinds,def}.rs",
  "xtask/**", ".claude/**", "CLAUDE.md", ".mcp.json", "rust-analyzer.toml", "deny.toml", "OWNERS.toml", ".config/**", ".cargo/**",
  "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "clippy.toml", "crates/*/clippy.toml", "docs/superpowers/specs/**", "docs/adr/**"]
[[track]]
name = "perf"
paths = ["docs/perf/**"]
[[track]]
name = "sim-core"
paths = ["crates/fl_sim/**", "crates/fl_catalog/**", "crates/fl_test_utils/**"]
[[track]]
name = "content"
paths = ["assets/**", "mods/**", "crates/fl_content/**"]
[[track]]
name = "client"
paths = ["crates/fl_client/**", "crates/fl_game/**"]
[[track]]
name = "tools"
paths = ["crates/fl_cli/**", "crates/fl_observe/**", "debug.ron"]
[[track]]
name = "probes"
paths = ["probes/**", "docs/research/**"]
[[track]]
name = "docs"
paths = ["docs/**", "README.md", ".gitignore"]

[protected]
paths = ["**/tests/acceptance/**", "**/tests/holdout/**", "**/tests/replays/**", "**/snapshots/**", "scenarios/**",
  "docs/design/mechanics/**", "docs/perf/baselines/**", "docs/perf/budgets.ron"]
```
Each feature adds `feature:<name>` with `features/<name>/**` (M4 onward).

**Interfaces:**
- Produces:
```rust
pub struct Owners { /* tracks: Vec<(String, GlobSet)>, protected: GlobSet */ }
pub fn load_owners(root: &Path) -> anyhow::Result<Owners>;
impl Owners { pub fn track_of(&self, path: &Path) -> Option<&str>; pub fn is_protected(&self, path: &Path) -> bool; }
pub fn check_scope(owners: &Owners, files: &[PathBuf], tracks: &[String]) -> Vec<(PathBuf, Option<String>)>;   // out-of-scope files and their owner (None = unowned)
pub enum FindingKind { RemovedAssert, NewIgnore, NewShouldPanic, ToleranceChanged, FewerProptestCases, RemovedTest, NewSeedOverride, MockInSimTest, NetTestCountDrop }
pub struct Finding { pub file: PathBuf, pub line: u32, pub kind: FindingKind, pub text: String }
pub fn scan_diff(diff: &str) -> Vec<Finding>;      // `git diff -U0 <base> -- '*.rs'`
pub fn bless(root: &Path, paths: &[PathBuf], reason: &str) -> anyhow::Result<Vec<PathBuf>>;
//   promotes `*.snap.new` → `*.snap` and any `*.new` → name without `.new` under the given paths;
//   Err on an empty reason or when env FL_ROLE=implementer
```
- CLI and output formats:
  - `cargo xtask check-scope --track <t>… [--base main]` prints `check-scope: OK (<n> files in <tracks>)`, or `check-scope: FAIL` followed by `  <file> → <owner|unowned>` lines, exiting 1.
  - `cargo xtask test-guard [--base main]` prints one `TEST-GUARD <file>:<line> <kind>: <text>` line per finding and ends with `test-guard: <n> findings (reviewer must accept each)`. It always exits 0.
  - `cargo xtask bless <paths>… --reason "<text>"` prints `bless: <n> files promoted` and `Blessed: <text>` (the trailer to paste into the commit).
- **`NetTestCountDrop`** stands in for spec §12.5's "falling `nextest list` count", which would need a build of the merge base. It fires when the diff removes more `#[test]` and `proptest!` functions than it adds.
- **Gate rows:**
  - `check-scope` (task, pr): Skip when no `--track` is given and `FL_TRACK` is unset; otherwise a finding Fails.
  - `test-guard` (task, pr): any finding is a `Warn`.

- [ ] **Step 1: Write the failing tests** (unit tests with fixture diffs; bless tests use `tempfile` dirs):
```rust
#[test] fn first_matching_track_wins()       // crates/fl_sim/src/builder.rs → "contract"; crates/fl_sim/src/movement.rs → "sim-core"
#[test] fn unowned_path_reported()           // check_scope(["foo/bar.rs"], ["sim-core"]) == [("foo/bar.rs", None)]
#[test] fn scope_flags_out_of_track_file()   // ["crates/fl_client/src/view.rs"] with ["sim-core"] → owner Some("client")
#[test] fn protected_globs()                 // protected: crates/fl_sim/tests/acceptance/a.rs, crates/fl_view/tests/snapshots/x.snap,
                                             //   scenarios/duel.ron, docs/perf/budgets.ron; NOT: assets/base/scenarios/testbed.scenario.ron,
                                             //   crates/fl_sim/tests/fixtures/core_mod/scenarios/duel.scenario.ron
#[test] fn guard_flags_removed_assert()      // "-    assert_eq!(a, b);" → RemovedAssert
#[test] fn guard_flags_new_ignore_and_should_panic()
#[test] fn guard_flags_fewer_proptest_cases()   // "-    cases: 256," "+    cases: 64," → FewerProptestCases
#[test] fn guard_flags_alpha_change()        // "-const ALPHA: f64 = 1e-4;" "+const ALPHA: f64 = 1e-2;" → ToleranceChanged
#[test] fn guard_flags_removed_test_fn()     // removed "#[test]" + "fn x()" → RemovedTest and NetTestCountDrop
#[test] fn guard_flags_mock_in_sim_test()    // "+struct MockRunner;" in crates/fl_sim/tests/x.rs → MockInSimTest
#[test] fn guard_quiet_on_added_asserts()    // only "+ assert!(..)" lines → no findings
#[test] fn bless_promotes_new_files()        // a.snap.new, b.replay.new → a.snap, b.replay; returns 2 paths
#[test] fn bless_requires_reason()           // reason "" → Err
#[test] fn bless_refuses_implementer_role()  // FL_ROLE=implementer → Err
```
- [ ] **Step 2: Run** `cargo test -p xtask`. Expected: FAIL.
- [ ] **Step 3: Implement**, including the two gate rows and the subcommands.
- [ ] **Step 4: Run** `cargo test -p xtask`. Expected: PASS.
- [ ] **Step 5: Verify.** `cargo xtask check-scope --track contract` → `check-scope: OK`; `cargo xtask test-guard` → `test-guard: 0 findings …`; `cargo xtask gate task --track contract` → `gate task: PASS`.
- [ ] **Step 6: Commit** `build(xtask): OWNERS.toml, check-scope, test-guard, bless` with trailer `Refs: ADR-0011, ADR-0009`.

### Task 4: Agent environment — CLAUDE.md, rules, hooks, role agents, bevy-api skill, `.mcp.json`

**Track:** contract · **Refs:** ADR-0013, ADR-0009

**Files:**
- Create: `CLAUDE.md`, `.claude/settings.json`, `.claude/rules/{sim,client,content,shaders}.md`, `.claude/agents/{sim-engineer,client-engineer,test-author,reviewer,perf-analyst}.md`, `.claude/skills/bevy-api/SKILL.md`, `.mcp.json`, `rust-analyzer.toml`, `xtask/src/hooks.rs`
- Modify: `xtask/src/{main,gate}.rs` (subcommand `hook`, gate row `claude-md`)

**Interfaces:**
- Produces:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Role { Lead, Implementer, ReadOnly }
pub fn role_from(input: &serde_json::Value, flag: Option<&str>) -> Role;
//   precedence: --role implementer|lead > env FL_ROLE > input["agent_type"]
//   (sim-engineer|client-engineer → Implementer; reviewer|perf-analyst → ReadOnly) > Lead
pub enum Decision { Allow, Deny(String), Ask(String) }
pub fn guard_edit(owners: &Owners, role: Role, path: &Path) -> Decision;   // protected path + Implementer → Deny
pub fn guard_bash(role: Role, command: &str) -> Decision;
pub fn stop_decision(gate_passed: bool, attempts: u32, summary: &str) -> (Option<serde_json::Value>, u32);
//   failing and attempts < 3 → Some({"decision":"block","reason": summary}), attempts + 1;
//   failing at 3 → None plus stderr "ESCALATE: gate task still failing after 3 attempts"
```
- CLI: `cargo xtask hook <stop|guard-edit|guard-bash|worktree-remove> [--role auto|implementer|lead]` reads the hook JSON from stdin and writes hook JSON to stdout:
  - Guards answer with `hookSpecificOutput.permissionDecision` set to `deny` or `ask` and a reason.
  - **`stop`** returns immediately when `changed_files` is empty. It keeps a per-session attempt counter in `target/xtask/stop-<session_id>` and respects `stop_hook_active`.
  - **`worktree-remove`** deletes the worktree's build dir, read from `cargo metadata` (`build_directory`, Task 1 Step 1), and only if it lies under `C:/t/fl/`.
- **Bash rules:**
  - Denied for `Implementer`: commands containing `cargo insta accept`, `cargo insta review`, `INSTA_FORCE_PASS`, `INSTA_UPDATE=always`, `--bless`, `xtask bless` or `git checkout -- tests/`.
  - `Ask` for every role: `cargo update`, `cargo add`, `rustup default`, `git push --force`, `git push -f`.
- **`.claude/settings.json`:**
  - `Stop` and `SubagentStop` run `cargo xtask hook stop` (timeout 900 s).
  - `PreToolUse` with matcher `Edit|Write|MultiEdit|NotebookEdit` runs `cargo xtask hook guard-edit --role auto`.
  - `PreToolUse` with matcher `Bash|PowerShell` runs `cargo xtask hook guard-bash --role auto`.
  - `WorktreeRemove` runs `cargo xtask hook worktree-remove`.
  - The implementer agents' frontmatter repeats both `PreToolUse` hooks with `--role implementer`. That way the denial holds even when the hook input carries no agent identity.

**Content decisions:**
- **`CLAUDE.md`** (≤ 200 lines, checked by the gate row `claude-md` at every level). Sections:
  1. what Frontline is (two lines)
  2. where the spec, ADRs, plans, research and `docs/debugging.md` live
  3. commands: `cargo xtask gate task --track <t>`, `cargo nextest run --profile agent -p <crate>`, `cargo run -p fl_cli -- data-check assets/base`, `cargo run -p fl_game`
  4. toolchain pin and the nightly rule
  5. worktree rules (edit only under `C:\fl\`, ≤ 2 builders, the build-dir rule from Task 1, long builds in the background)
  6. determinism and dependency rules as a 10-line summary that points to spec §5.1/§5.4
  7. protected paths, `bless` and the `BLOCKED:` escape hatch
  8. contract paths and the ADR rule
  9. observability rules
  10. "resolve Bevy APIs with the `bevy-api` skill; never from memory"
  11. the commit trailer
- **Rules** (frontmatter `paths:`):
  - `sim.md` (`crates/fl_sim/**`, `crates/fl_view/**`, `crates/fl_protocol/**`, `features/*/sim/**`): determinism rules, classification and the hook sub-sets.
  - `client.md` (`crates/fl_client/**`, `crates/fl_game/**`, `features/*/client/**`): no sim access, commands via `LocalDriver`, and `Option<Single<&Window>>`.
  - `content.md` (`assets/**`, `crates/fl_content/**`): strictness and never copying Nival.
  - `shaders.md` (`**/*.wesl`): WESL only.
- **Agents.** Each names its tools, the skill `bevy-api`, hooks and MCP servers.
  - `sim-engineer`: Read, Edit, Write, Bash, Grep, Glob, LSP; implementer hooks.
  - `client-engineer`: the same, plus `mcp__brp__*`.
  - `test-author`: "active from M2 (C3)". Edits only `**/tests/acceptance/**` and `**/tests/holdout/**`, which its own hook allows.
  - `reviewer`: Read, Grep, Glob, and Bash for read-only commands. Its body carries the determinism checklist (§5.4 bullets, classification, ordering, the `(player, seq)` path, no client→sim edges, contract changes cite an ADR, test-guard findings accepted one by one).
  - `perf-analyst`: Read, Grep, Glob, Bash; never edits.
- **`bevy-api` skill.** Resolution order:
  1. Local sources: `~/.cargo/registry/src/*/bevy*-0.20.0/` (`src/`, any shipped `examples/`, and doc comments; grep with `rg -n`)
  2. `docs/research/bevy020-api-reference.md` (including its migration notes)
  3. A compile probe in `probes/`

  Record new facts in the API reference, and never answer from memory. If a local copy of the Bevy migration guides is found during this task, add its path as step 1b.
- **`.mcp.json`:** `{"mcpServers":{"brp":{"command":"bevy_brp_mcp","args":[]}}}`. Ports are per agent, via `--remote-port` or `BRP_EXTRAS_PORT`.
- **`rust-analyzer.toml`:** `[cargo] targetDir = true` (rust-analyzer gets its own subdirectory and never holds the build lock) and `[check] command = "clippy"`.

- [ ] **Step 1: Write the failing tests** (`xtask/src/hooks.rs`):
```rust
#[test] fn role_precedence()                          // flag > FL_ROLE > agent_type > Lead
#[test] fn guard_denies_protected_for_implementer()   // crates/fl_sim/tests/snapshots/a.snap → Deny(reason contains "protected path")
#[test] fn guard_allows_protected_for_lead()
#[test] fn guard_allows_unprotected_for_implementer() // crates/fl_sim/src/step.rs → Allow
#[test] fn guard_denies_bless_variants()              // "cargo insta accept", "INSTA_UPDATE=always cargo test", "cargo xtask bless x --reason y",
                                                      //   "git checkout -- tests/" → Deny for Implementer; Allow for Lead
#[test] fn guard_asks_for_cargo_update()              // "cargo update -p serde" → Ask for every role
#[test] fn stop_blocks_then_escalates()               // attempts 0,1,2 failing → block JSON; attempt 3 → None
#[test] fn claude_md_limit()                          // a 201-line file → Fail; 200 → Pass
```
- [ ] **Step 2: Run** `cargo test -p xtask hooks`. Expected: FAIL.
- [ ] **Step 3: Implement** the hooks module and gate row. Write the config files.
- [ ] **Step 4: Run** `cargo test -p xtask`. Expected: PASS.
- [ ] **Step 5: Verify the live hooks** (lead session).
  - Dispatch a `sim-engineer` subagent to Write `crates/fl_view/tests/snapshots/probe.snap`. Expected: denied with the protected-path reason.
  - From the same subagent, `cargo insta accept`. Expected: denied.
  - Log one hook stdin to `target/xtask/hook-probe.json` and record whether it carries `agent_type`.
  - In the lead session, `cargo xtask bless --help` runs.
  - `echo '{"session_id":"t","stop_hook_active":false}' | cargo xtask hook stop` with a deliberate fmt error in a scratch file prints a block decision. After reverting, it prints nothing and exits 0.
- [ ] **Step 6: Run** `cargo xtask gate task --track contract`. Expected: `PASS claude-md` and `gate task: PASS`.
- [ ] **Step 7: Commit** `chore(agents): CLAUDE.md, path rules, gate and guard hooks, role agents, bevy-api skill, .mcp.json` with trailer `Refs: ADR-0013, ADR-0009`. The controller then runs `cargo xtask gate pr`, fast-forwards `main`, and does the T12 rebase/gate/merge (Completed work).

---

## Phase II — contract rework and new sim modules (lane 1 `C:\fl\a`; lane 2 as in the topology)

### Task 5: `fl_math` — keyed counter-based RNG

**Track:** contract · **Refs:** ADR-0003

**Files:**
- Modify: `crates/fl_math/src/{rng,lib,fx,angle}.rs`, `crates/fl_math/tests/fx_props.rs`
- Create: `crates/fl_math/tests/rolls.rs`

**Interfaces:**
- Consumes: the existing private `splitmix64` in `rng.rs`.
- Produces (the output values are a contract: changing them moves every draw):
```rust
pub const fn stream_key(name: &str) -> u64;   // splitmix64(fnv1a64(name bytes)); FNV-1a 64: offset 0xcbf29ce484222325, prime 0x100000001b3
pub fn roll(seed: u64, tick: u32, sim_id: u32, stream: u64, n: u32) -> u64;
//   h = splitmix64(seed); h = splitmix64(h ^ ((tick as u64) << 32 | sim_id as u64)); h = splitmix64(h ^ stream); splitmix64(h ^ n as u64)
#[derive(Clone, Debug)] pub struct Rolls { /* seed, tick, sim_id, stream, n */ }
impl Rolls {
    pub const fn new(seed: u64, tick: u32, sim_id: u32, stream: u64) -> Rolls;   // n = 0
    pub fn next_u64(&mut self) -> u64;                   // roll(.., n), then n += 1
    pub fn next_u32(&mut self) -> u32;                   // high 32 bits of next_u64
    pub fn range_u32(&mut self, lo: u32, hi_excl: u32) -> u32;   // Lemire, the same algorithm as Pcg32::range_u32; each rejection consumes a draw
    pub fn chance(&mut self, num: u32, den: u32) -> bool;        // range_u32(0, den) < num
    pub fn unit_fx(&mut self) -> Fx;                     // Fx::from_raw(next_u32() as i64)
    pub fn range_fx(&mut self, lo: Fx, hi: Fx) -> Fx;    // the same formula as Pcg32::range_fx
    pub fn draws(&self) -> u32;                          // the current n
}
```
- Removes `derive_seed`, which `roll` supersedes. `Pcg32` stays for tools, content time and the client.
- The crate docs change from "the Pcg32 random number generator" to "keyed counter-based RNG (`roll`, `Rolls`), plus `Pcg32` for tools".

- [ ] **Step 1: Write the failing tests** in `tests/rolls.rs`. The reference vectors were computed with an independent Python implementation of the definitions above, so they are an oracle, not a capture:
```rust
#[test] fn stream_key_reference_vectors() {
    assert_eq!(stream_key(""), 0xc381_7c01_6ba4_ff30);
    assert_eq!(stream_key("combat.dispersion"), 0xddc2_43e5_0cb8_47f4);
    assert_eq!(stream_key("debug.wander"), 0xa1cc_dba2_15ca_7a60);
    assert_eq!(stream_key("setup.scale"), 0xd9f9_6374_b859_3636);
}
#[test] fn roll_reference_vectors() {
    assert_eq!(roll(0, 0, 0, 0, 0), 0x2130_748a_aac8_0268);
    assert_eq!(roll(1, 2, 3, stream_key("debug.wander"), 4), 0xe5b4_bc46_9f17_780b);
}
#[test] fn rolls_range_reference() {
    let mut r = Rolls::new(1, 2, 3, stream_key("debug.wander"));
    let got: Vec<u32> = (0..5).map(|_| r.range_u32(0, 10)).collect();
    assert_eq!(got, [1, 0, 7, 3, 8]);
    assert_eq!(r.draws(), 5);
}
#[test] fn each_input_changes_roll()            // bump each of the 5 inputs of roll(9,3,4,5,6) by 1 → 5 values, pairwise distinct and != the base
#[test] fn tick_and_sim_id_not_interchangeable() { assert_ne!(roll(9, 3, 4, 5, 0), roll(9, 4, 3, 5, 0)); }
#[test] fn next_u32_is_high_half() { let mut r = Rolls::new(0, 0, 0, 0); assert_eq!(r.next_u32(), (roll(0, 0, 0, 0, 0) >> 32) as u32); }
#[test] fn unit_fx_and_range_fx_bounds()        // 10_000 draws: 0 ≤ unit < 1; range_fx(-5, 5) in [-5, 5)
#[test] fn range_u32_covers_all_values()        // range_u32(0, 7) over 10_000 draws hits all 7 values
#[test] fn chance_extremes()                    // chance(0, 5) never true; chance(5, 5) always true (100 draws each)
#[test] fn stream_key_is_const() { const K: u64 = stream_key("x"); assert_eq!(K, stream_key("x")); }
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_math`. Expected: FAIL (`roll` not defined).
- [ ] **Step 3: Implement.**
  - Delete `derive_seed` and its tests, and update the `lib.rs` re-exports and docs.
  - Add `#![deny(clippy::float_arithmetic)]` to `lib.rs`.
  - Put a scoped `#[expect(clippy::float_arithmetic, reason = "load-time or presentation conversion")]` only on `Fx::from_f64`, `Fx::to_f32`, `Fx::to_f64`, `Angle::to_radians_f32` and the human-readable serde visitors.
  - Test modules and `tests/*.rs` files that compare against f64 get `#![expect(clippy::float_arithmetic, reason = "f64 oracle")]`.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract`. Expected: `gate task: PASS`.
- [ ] **Step 6: Commit** `feat(fl_math)!: keyed counter-based RNG (roll, Rolls, stream_key); drop derive_seed` with trailer `Refs: ADR-0003`.

### Task 6: `fl_protocol` — lockstep envelope, payload types, `MatchHeader`, string tables

**Track:** contract (+ mechanical `sim-core` call sites) · **Refs:** ADR-0007, ADR-0004, ADR-0008

**Files:**
- Modify: split `crates/fl_protocol/src/lib.rs` into `src/{lib,ids,command,payloads,header,tables}.rs`.
- Modify mechanically: `crates/fl_sim/src/commands.rs` and `crates/fl_sim/tests/*.rs`. Rename `tick` → `exec_tick` and add a `seq` (0, 1, 2, … per player in each test). No behaviour changes.
- Create: `crates/fl_protocol/tests/protocol.rs`

**Interfaces:**
- Produces (every type derives `Debug, Clone, PartialEq, Serialize, Deserialize`; the id newtypes are as in rev 1):
```rust
pub struct PlayerCommand { pub exec_tick: Tick, pub player: PlayerId, pub seq: u32, pub units: Vec<SimId>, pub kind: CommandKind, pub queue: bool }
pub struct CommandKind { pub type_id: CommandTypeId, pub payload: Vec<u8> }   // unchanged
pub trait CommandPayload: Serialize + DeserializeOwned + Debug + Send + Sync + 'static {
    const NAME: &'static str;
    const NEEDS_UNITS: bool = true;
    const DEBUG_ONLY: bool = false;
}
// payloads.rs — all #[serde(deny_unknown_fields)]; content ids are names, never dense ids
pub struct MoveCmd { pub target: FxVec2 }                                                                  // "core.move"
pub struct StopCmd;                                                                                         // "core.stop"
pub struct DebugSpawnCmd { pub unit: String, pub owner: PlayerId, pub pos: FxVec2, pub heading: Angle }   // "debug.spawn", NEEDS_UNITS false
pub struct DebugWanderCmd { pub radius: Fx }                                                                // "debug.wander"
pub struct DebugSetOwnerCmd { pub owner: PlayerId }                                                         // "debug.set_owner"
pub struct DebugInvulnerableCmd { pub on: bool }                                                            // "debug.invulnerable"
pub struct DebugSetHpCmd { pub hp: u32 }                                                                    // "debug.set_hp"
pub struct DebugTeleportCmd { pub pos: FxVec2 }                                                             // "debug.teleport"
//   every debug.* payload has DEBUG_ONLY = true
// tables.rs
#[derive(Default, Eq)] pub struct StringTables { pub tables: BTreeMap<String, Vec<String>> }   // table → names in dense-id order
impl StringTables {
    pub fn insert(&mut self, table: &str, names: Vec<String>);
    pub fn get(&self, table: &str) -> Option<&[String]>;
    pub fn id_of(&self, table: &str, name: &str) -> Option<u32>;
    pub fn remap(&self, table: &str, current: &StringTables) -> Vec<Option<u32>>;   // index = id in self; value = id in current
}
pub mod tables {
    pub const COMMANDS: &str = "commands";             pub const DEF_COMPONENTS: &str = "def_components";
    pub const OBJECT_KINDS: &str = "object_kinds";     pub const CONTENT_KINDS: &str = "content_kinds";
    pub const RULES_SECTIONS: &str = "rules_sections"; pub const SIM_COMPONENTS: &str = "sim_components";
    pub const SIM_RESOURCES: &str = "sim_resources";   pub const RNG_STREAMS: &str = "rng_streams";
    pub const JOURNAL_RECORDS: &str = "journal_records"; pub const VIEW_CHANNELS: &str = "view_channels";
    pub const UNITS: &str = "content.unit";            // other content kinds: "content.<ext>"
}
// header.rs
pub const FORMAT_VERSION: u32 = 1;
pub struct MatchHeader { pub format_version: u32, pub build_id: String, pub sim_version: u32, pub features: Vec<(String, u32)>,
    pub mods: Vec<(String, String)>, pub rules_hash: u64, pub cheats_allowed: bool, pub seed: u64, pub string_tables: StringTables }
pub enum HeaderMismatch { FormatVersion, BuildId, SimVersion, Features, Mods, RulesHash, CheatsAllowed, Seed, StringTables }   // Display by hand
impl MatchHeader {
    pub fn check_replay(&self, recorded: &MatchHeader) -> Result<Vec<String>, HeaderMismatch>;  // format, features, mods, sim_version, rules_hash equal; a build_id difference is a warning
    pub fn check_handshake(&self, peer: &MatchHeader) -> Result<(), HeaderMismatch>;            // every field equal, build_id included
    pub fn check_save(&self, saved: &MatchHeader) -> Result<(), HeaderMismatch>;                // features and mods equal; saved.format_version ≤ ours
}
#[non_exhaustive] pub enum Controller { Human, Ai(String), None }
```
- `fl_sim` keeps its own rev-1 `CommandPayload` trait and payload structs until Task 9 replaces them. The two traits never meet, because they live in different crates.

- [ ] **Step 1: Write the failing tests** (`tests/protocol.rs`, plus the existing unit tests updated):
```rust
#[test] fn player_command_postcard_roundtrip()       // exec_tick 7, seq 3 → equal after encode/decode
#[test] fn match_header_postcard_roundtrip()         // two features, one mod, two tables
#[test] fn payload_names()                           // MoveCmd::NAME == "core.move", StopCmd "core.stop", DebugSpawnCmd "debug.spawn", … all eight
#[test] fn debug_payloads_are_debug_only()           // the six debug.* payloads are DEBUG_ONLY, the core ones are not; DebugSpawnCmd::NEEDS_UNITS == false
#[test] fn fx_payload_is_lossless_in_postcard()      // MoveCmd { target: (Fx::from_raw(0x7FFF_FFFF_0000_0001), Fx::from_raw(-3)) } round-trips bit-exact
#[test] fn payload_rejects_unknown_fields_in_ron()   // ron "(target: (1.0, 2.0), extra: 1)" → Err
#[test] fn string_tables_id_of_and_remap()           // old ["a","c"], current ["a","b","c"] → remap == [Some(0), Some(2)]; id_of(.., "x") == None
#[test] fn replay_check_warns_on_build_id_only()     // build_id differs → Ok(warnings), warnings.len() == 1
#[test] fn replay_check_rejects_rules_hash_mismatch()   // Err(HeaderMismatch::RulesHash)
#[test] fn handshake_requires_build_id()             // Err(HeaderMismatch::BuildId)
#[test] fn save_check_rejects_feature_mismatch()     // Err(HeaderMismatch::Features)
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_protocol`. Expected: FAIL.
- [ ] **Step 3: Implement**, then adapt the `fl_sim` call sites mechanically.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_protocol -p fl_sim`. Expected: PASS, with every existing `fl_sim` test still green.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_protocol)!: lockstep envelope, CommandPayload, core and debug payloads, MatchHeader, StringTables` with trailer `Refs: ADR-0007, ADR-0004, ADR-0008`.

### Task 7: `fl_view` — snapshot schema, events, journal entry, runner protocol, driver trait

**Track:** contract · **Refs:** ADR-0002

**Files:**
- Create: `crates/fl_view/src/{lib,snapshot,event,journal,runner,driver}.rs`, `crates/fl_view/tests/view.rs`

**Interfaces:**
- Produces (`#![deny(clippy::float_arithmetic)]`; data types derive `Debug, Clone, PartialEq, Serialize, Deserialize`):
```rust
pub const VIEW_FORMAT_VERSION: u32 = 1;
// snapshot.rs
pub struct SimSnapshot { pub tick: Tick, pub entities: Vec<EntityRecord> /* sorted by sim_id */, pub channels: Vec<(String, Vec<u8>)> /* sorted by name */ }
impl SimSnapshot {
    pub fn get(&self, id: SimId) -> Option<&EntityRecord>;      // binary search
    pub fn channel<C: ViewChannel>(&self) -> Option<Result<C, String>>;
    pub fn set_channel<C: ViewChannel>(&mut self, value: &C);
}
pub struct EntityRecord { pub sim_id: SimId, pub unit: UnitTypeId, pub owner: PlayerId, pub pos: FxVec2, pub heading: Angle,
    pub turrets: Vec<Angle>, pub hp_frac: u16, pub flags: u32, pub visible_to: u8 }
impl EntityRecord { pub fn hp_frac(hp: u32, max: u32) -> u16; }   // hp * 65535 / max, rounded down; max 0 → 0
pub mod flags { pub const MOVING: u32 = 1; pub const INVULNERABLE: u32 = 2; }
pub trait ViewChannel: Serialize + DeserializeOwned + Send + Sync + 'static { const NAME: &'static str; }
// event.rs
#[non_exhaustive] pub enum SimEvent { Spawned { sim_id: SimId, unit: Option<UnitTypeId>, owner: PlayerId }, Despawned { sim_id: SimId },
    CommandRejected { player: PlayerId, seq: u32, reason: String }, Custom { channel: String, data: Vec<u8> } }
// journal.rs
pub struct JournalEntry { pub tick: Tick, pub phase: u8, pub kind: String, pub sim_id: Option<SimId>, pub player: Option<PlayerId>,
    pub emitted: u32, pub data: serde_json::Value }
// runner.rs
pub struct TickInput { pub tick: Tick, pub commands: Vec<PlayerCommand> }
pub struct TickOutput { pub tick: Tick, pub hash: Option<u64>, pub snapshot: Arc<SimSnapshot>, pub events: Vec<SimEvent>, pub journal: Vec<JournalEntry> }   // not serialized
#[non_exhaustive] pub enum SimQuery { Entity(SimId), WorldSummary, JournalSince { tick: Tick, limit: u32 }, Perf, Save { pending: Vec<PlayerCommand> }, Header }
#[non_exhaustive] pub enum SimReply { Entity(Option<serde_json::Value>), WorldSummary(serde_json::Value), Journal(Vec<JournalEntry>),
    Perf(serde_json::Value), Save(Vec<u8>), Header(MatchHeader), Error(String) }
pub enum ToRunner { Tick(TickInput), Query { id: u64, query: SimQuery }, Shutdown }
pub enum FromRunner { Tick(TickOutput), Reply { id: u64, reply: SimReply }, Failed(RunnerFailure) }
pub struct RunnerFailure { pub tick: Tick, pub message: String }
pub struct RunnerPort { pub tx: crossbeam_channel::Sender<ToRunner>, pub rx: crossbeam_channel::Receiver<FromRunner> }   // client side
pub struct RunnerEnd { pub rx: crossbeam_channel::Receiver<ToRunner>, pub tx: crossbeam_channel::Sender<FromRunner> }  // runner side
pub fn runner_channel() -> (RunnerPort, RunnerEnd);   // unbounded
// driver.rs
pub struct CommandDraft { pub player: PlayerId, pub units: Vec<SimId>, pub kind: CommandKind, pub queue: bool }
#[derive(Default)] pub struct CommandStamper { /* next seq per player; held: BTreeMap<Tick, Vec<PlayerCommand>> */ }
impl CommandStamper {
    pub fn stamp(&mut self, exec_tick: Tick, draft: CommandDraft) -> PlayerCommand;   // seq starts at 0 per player and increases by 1 per stamp; holds the command
    pub fn take_due(&mut self, tick: Tick) -> Vec<PlayerCommand>;                    // held commands with exec_tick ≤ tick, sorted (player, seq)
    pub fn held(&self) -> Vec<PlayerCommand>;
    pub fn restore(&mut self, pending: Vec<PlayerCommand>, last_applied: &BTreeMap<PlayerId, u32>);   // re-holds pending; next seq = max(last applied, pending) + 1
}
pub trait TickDriver {
    fn next_tick(&self) -> Tick;
    fn input_delay(&self) -> u32;
    fn issue(&mut self, draft: CommandDraft) -> PlayerCommand;   // exec_tick = next_tick + input_delay
    fn poll(&mut self) -> Option<TickInput>;                     // the next tick's input if ready; advances next_tick
    fn pending(&self) -> Vec<PlayerCommand>;                     // stamped but not yet delivered
}
```

- [ ] **Step 1: Write the failing tests** (`tests/view.rs`):
```rust
#[test] fn snapshot_postcard_roundtrip()
#[test] fn get_finds_by_sim_id()                     // records 1, 4, 9 → get(SimId(4)).unwrap().sim_id == SimId(4); get(SimId(5)) == None
#[test] fn channel_roundtrip()                       // set_channel(&TestChan { n: 7 }) → channel::<TestChan>() == Some(Ok(TestChan { n: 7 }))
#[test] fn hp_frac_values()                          // (50, 100) → 32767; (100, 100) → 65535; (0, 0) → 0
#[test] fn stamper_assigns_increasing_seq_per_player()   // P0, P1, P0 → seqs 0, 0, 1
#[test] fn stamper_take_due_sorted_and_only_due()    // held at ticks 5 and 6; take_due(5) returns only the tick-5 commands, sorted (player, seq)
#[test] fn stamper_restore_continues_seq()           // last_applied {P0: 4}, pending [P0 seq 5] → the next P0 stamp has seq 6
#[test] fn tick_input_postcard_roundtrip()
#[test] fn runner_channel_carries_messages()         // ToRunner::Shutdown sent on port.tx arrives on end.rx
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_view`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_view): snapshot schema, events, journal entry, runner protocol, command stamper and TickDriver` with trailer `Refs: ADR-0002`.

### Task 8: `fl_content` — canonical binary rules hash, frozen registry, string tables

**Track:** contract + content · **Refs:** ADR-0007, ADR-0004 · **Lane 2** (`C:\fl\t08`, cut after Task 6)

**Files:**
- Modify: `crates/fl_content/src/{hash,registry,db,mods,lib}.rs`, `crates/fl_content/tests/load.rs`
- Create fixtures `tests/fixtures/{hash_faction_b,hash_rules_b}/`: copies of the hash fixture with one faction colour or one rules value changed.

**Interfaces:**
- Consumes: `fl_protocol::{StringTables, tables}` (Task 6).
- Produces:
```rust
pub const RULES_HASH_FORMAT: u32 = 1;
//   rules_hash = xxh64(seed 0) over RULES_HASH_FORMAT.to_le_bytes() ++ postcard(canonical tree):
//   (mods: [(id, version)] in load order,
//    units: [(id, name, tags, BTreeMap<&str, &dyn DefComponent>)] sorted by id,
//    kinds: [(ext, [postcard bytes of each item, sorted by id])] for IN_RULES_HASH kinds, sorted by ext,
//    rules: [(section name, postcard bytes)] sorted by name)
impl DefRegistry {
    pub fn freeze(&mut self);
    pub fn is_frozen(&self) -> bool;
    pub fn string_tables(&self) -> StringTables;   // DEF_COMPONENTS, OBJECT_KINDS, CONTENT_KINDS, RULES_SECTIONS, each sorted
}   // every register_* after freeze panics: "DefRegistry is frozen: register '<name>' in SimFeature::build"
impl ContentDb { pub fn string_tables(&self) -> StringTables; }   // UNITS in UnitTypeId order; "content.<ext>" per kind, in KindTable order
// load_dirs freezes the registry it receives before parsing.
```
- `ContentDb::same_id_sets` is deferred to M1 (hot reload, ADR-0012).

- [ ] **Step 1: Write the failing tests:**
```rust
// src/hash.rs (unit test; test DefComponent `Big(Fx)`)
#[test] fn rules_hash_sees_low_bits_at_large_magnitude()   // trees differing only in Fx::from_raw((1 << 53) | 1) vs ((1 << 53) | 2) → hashes differ
// tests/load.rs
#[test] fn rules_hash_changes_with_faction_color()   // hash fixture vs hash_faction_b → hashes differ
#[test] fn rules_hash_changes_with_rules_section()   // vs hash_rules_b → hashes differ
#[test] #[should_panic(expected = "DefRegistry is frozen")] fn register_after_freeze_panics()
#[test] fn registry_string_tables_sorted()           // the DEF_COMPONENTS table == the sorted registered names
#[test] fn content_string_tables_in_id_order()       // UNITS table[i] == db.units()[i].id for every i
// kept: rules_hash_stable_across_loads, rules_hash_ignores_formatting, rules_hash_ignores_visuals, rules_hash_changes_with_stat
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_content`. Expected: FAIL.
- [ ] **Step 3: Implement.** Add `#![deny(clippy::float_arithmetic)]` to `lib.rs`. Confirm with a grep that no rules-hashed type uses `#[serde(flatten)]`.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track content`. Expected: PASS.
- [ ] **Step 6: Commit** `refactor(fl_content)!: canonical postcard rules hash, frozen registry, string tables` with trailer `Refs: ADR-0007, ADR-0004`.

### Task 9: `fl_sim` — `SimFeature` + `SimBuilder`, stable registries, classification, command handlers, `MatchHeader`

**Track:** contract + sim-core · **Refs:** ADR-0001, ADR-0004, ADR-0006, ADR-0007 · Starts after Task 8 is merged into `m0/phase-a`.

**Files:**
- Create: `crates/fl_sim/src/{registry,builder,classify}.rs`, `crates/fl_sim/tests/{feature,classify}.rs`
- Modify: `crates/fl_sim/src/{feature,build,lib,hash,inspect,players,map,units,commands,events,rng,spatial,movement,scale,scenario,spawn}.rs`. Registration moves to `SimBuilder`, and the rev-1 payload structs are replaced by the `fl_protocol` payloads plus sim-side handlers.
- Port: `crates/fl_sim/tests/{schedule,hash,commands,movement}.rs` to the new API
- Delete: `crates/fl_sim/src/app_ext.rs`

**Interfaces:**
- Consumes: Tasks 5–8.
- Produces:
```rust
// registry.rs
pub struct StableRegistry<V> { .. }
impl<V> StableRegistry<V> {
    pub fn new(table: &'static str) -> Self;
    pub fn register(&mut self, name: &'static str, registrant: &'static str, value: V);
    //   panics "'<name>' registered by '<a>' and '<b>'" on a duplicate, and "registry '<table>' is frozen" after freeze
    pub fn freeze(&mut self);                           // sort names; assign dense ids; precompute name keys = fl_math::stream_key(name)
    pub fn is_frozen(&self) -> bool;
    pub fn id(&self, name: &str) -> Option<u32>;
    pub fn name(&self, id: u32) -> Option<&'static str>;
    pub fn name_key(&self, id: u32) -> Option<u64>;
    pub fn get(&self, name: &str) -> Option<&V>;
    pub fn get_by_id(&self, id: u32) -> Option<&V>;
    pub fn iter(&self) -> impl Iterator<Item = (u32, &'static str, &V)>;   // dense order; frozen only
    pub fn export(&self, tables: &mut StringTables);
}
// feature.rs
pub trait SimFeature: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn version(&self) -> u32;
    fn deps(&self) -> &'static [&'static str] { &[] }
    fn build(&self, sim: &mut SimBuilder);
}
pub struct CoreFeature;   impl CoreFeature { pub const ID: &'static str = "core"; }   // version 1; always first; callers never list it
pub enum FeatureError { Duplicate(String), Reserved(String), MissingDep { feature: String, dep: String }, Cycle(Vec<String>) }
pub fn sort_features(features: &[Arc<dyn SimFeature>]) -> Result<Vec<Arc<dyn SimFeature>>, FeatureError>;
//   Kahn's algorithm with the ready set ordered by id; core first; Cycle lists the remaining ids, sorted
// classify.rs
pub enum Class<T> { Hashed(Codec<T>), Derived { rebuild: fn(&mut World) }, Transient, External }
impl<T: SimHash + Serialize + DeserializeOwned + Send + Sync + 'static> Class<T> { pub fn hashed(version: u32) -> Class<T>; }
pub struct Codec<T> { pub version: u32, /* hash, encode (postcard), decode, dump (serde_json) fns */ }
pub fn check_classification(world: &World) -> Result<(), Vec<String>>;
//   every registered component/resource type whose outermost path starts with "fl_sim::" or "fl_feat_" (not a "_client" crate)
//   must be in the state registry; returns the sorted unclassified type names
// builder.rs
pub struct SimBuilder { .. }
impl SimBuilder {
    pub fn feature_id(&self) -> &'static str;
    pub fn register_def_component<T: DefComponent + DeserializeOwned>(&mut self, name: &'static str) -> &mut Self;
    pub fn register_content_kind<K: ContentKind>(&mut self) -> &mut Self;
    pub fn register_rules_section<S: RulesSection>(&mut self, name: &'static str) -> &mut Self;
    pub fn register_component<C: Component>(&mut self, name: &'static str, class: Class<C>) -> &mut Self;   // External panics: "External is for resources"
    pub fn register_resource<R: Resource>(&mut self, name: &'static str, class: Class<R>) -> &mut Self;
    pub fn register_command<P: CommandPayload>(&mut self, handler: impl CommandHandler<P>) -> &mut Self;
    pub fn register_stream(&mut self, name: &'static str) -> &mut Self;
    pub fn add_systems<M>(&mut self, set: SimSet, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) -> &mut Self;   // Task 10 swaps SimSet for SimHook
    pub fn app_mut(&mut self) -> &mut App;   // escape hatch; review flags every use
}
// commands.rs (the handler half of the split)
pub trait CommandHandler<P: CommandPayload>: Send + Sync + 'static {
    fn unit_can(&self, _payload: &P, _unit: EntityRef<'_>) -> bool { true }
    fn apply(&self, payload: &P, world: &mut World, ctx: &CommandCtx<'_>) -> Result<(), RejectReason>;
}
pub struct CommandCtx<'a> { pub tick: Tick, pub player: PlayerId, pub seq: u32, pub units: &'a [(SimId, Entity)], pub queue: bool }
// build.rs
#[derive(Clone, Debug, Default)] pub struct MatchSetup { pub seed: u64, pub cheats_allowed: bool }
#[derive(Clone, Debug, Default)] pub struct RunConfig { pub hash_interval: Option<u32> }   // later tasks add fields; construct it with ..Default::default()
pub enum BuildError { Feature(FeatureError), Content(ContentErrors), Unclassified(Vec<String>), RegistryMismatch }
pub const SIM_VERSION: u32 = 1;
pub fn build_id() -> String;   // "<CARGO_PKG_VERSION>+<FL_GIT_REV>" when option_env!("FL_GIT_REV") was set at compile time, else "dev"
pub fn sim_registry(features: &[Arc<dyn SimFeature>]) -> Result<DefRegistry, BuildError>;
pub fn load_content(features: &[Arc<dyn SimFeature>], mods: &[PathBuf]) -> Result<Arc<ContentDb>, BuildError>;
pub fn build_sim_app(features: &[Arc<dyn SimFeature>], content: Arc<ContentDb>, setup: MatchSetup, run: RunConfig) -> Result<App, BuildError>;
//   sort → build each feature through SimBuilder → freeze every registry in finish() → build the MatchHeader
//   (features, mods, rules hash, seed, cheats flag, string tables from fl_content and every StableRegistry, build_id, SIM_VERSION)
//   → classification check (cfg!(debug_assertions)) → initialize SimTick
#[derive(Resource)] pub struct MatchHeaderRes(pub MatchHeader);   // External "core.match_header"
```
- **Core registrations.** The names are a contract; Tasks 10–13 add theirs.
  - Components:
    - Hashed v1: `core.owner`, `core.unit_type`, `core.pose`, `core.health`, `core.footprint`, `core.mobility`, `core.move_order`, `core.wander`, `core.selectable`.
    - `core.sim_entity` is Derived with a no-op rebuild. It is structural: load recreates it, and the hash and the save use its id as the entity key.
  - Resources:
    - Hashed: `core.clock`, `core.sim_id_allocator`, `core.players`, `core.map`, plus `core.rng` (`SimRng`, until Task 10) and `core.command_queue` (until Task 11).
    - Derived: `core.sim_index`.
    - External: `core.content`, `core.match_header`, `core.def_registry`, `core.command_registry`.
    - Transient: `core.command_log`, `core.sim_events`.
  - Commands: `core.move`, `core.stop`, `debug.spawn` and `debug.wander`, with handlers that keep the rev-1 behaviour. `debug.spawn` now resolves the unit name. Task 11 adds the rest.
  - Streams: `debug.wander`, `setup.scale`.
- **Inspection:** `inspect_entity` keys components by registry name, as `{"sim_id": n, "components": {"core.pose": …}}`. `inspect_world` adds `"seed"` and `"header": {features, mods, rules_hash}`.

- [ ] **Step 1: Write the failing tests:**
```rust
// tests/feature.rs
#[test] fn features_sort_topologically_ties_by_id()   // [c, b(deps a), a] → ids ["core", "a", "b", "c"]
#[test] fn feature_cycle_is_error()                   // a(deps b), b(deps a) → Err(Cycle(vec!["a", "b"]))
#[test] fn missing_dependency_is_error()              // b(deps z) → Err(MissingDep { feature: "b", dep: "z" })
#[test] fn duplicate_feature_id_is_error()
#[test] fn core_id_is_reserved()                      // a test feature with id "core" → Err(Reserved("core"))
#[test] fn header_lists_features_mods_and_tables()
//   header.features == [("core", 1), ("test", 3)]; header.sim_version == SIM_VERSION; header.rules_hash == content.rules_hash();
//   header.cheats_allowed == setup.cheats_allowed; header.seed == setup.seed;
//   string_tables.get("commands") == ["core.move", "core.stop", "debug.spawn", "debug.wander"]
#[test] fn registry_ids_sorted_by_name()              // register "b", "a", "c" → id("a") == 0, id("c") == 2, name_key(0) == stream_key("a")
#[test] #[should_panic(expected = "is frozen")] fn register_after_freeze_panics()
#[test] #[should_panic(expected = "registered by 'test' and 'other'")] fn duplicate_name_panics_naming_both()
// tests/classify.rs
#[test] fn core_app_is_fully_classified()             // build_sim_app(core_mod) → Ok
#[test] #[should_panic(expected = "External is for resources")] fn external_component_panics()
#[test] fn classified_names_in_tables()               // "sim_components" contains "core.pose"; "sim_resources" contains "core.players"
// src/classify.rs (a unit test, so the stray type's path starts with fl_sim::)
#[test] fn unclassified_sim_type_fails_build()        // a feature inserts an unregistered `Stray` → Err(Unclassified(vec!["fl_sim::classify::tests::Stray"]))
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: FAIL.
- [ ] **Step 3: Implement**, and port the existing tests. Behaviour doesn't change; only names and APIs do.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim)!: SimFeature + SimBuilder, stable registries, state classification, command handlers, MatchHeader` with trailer `Refs: ADR-0001, ADR-0004, ADR-0006, ADR-0007`.

### Task 10: `fl_sim` — eight phases, `step_tick`, keyed RNG, name-keyed state hash, counters

**Track:** contract + sim-core · **Refs:** ADR-0005, ADR-0003, ADR-0004

**Files:**
- Create: `crates/fl_sim/src/{step,counters}.rs`, and `crates/fl_sim/tests/fixtures/core_mod_extra/`: a copy of `core_mod` plus `units/aaa.unit.ron` with id `core_test.aaa_first`, which sorts first.
- Modify: `crates/fl_sim/src/{schedule,hash,rng,clock,events,builder,build,movement,scale,commands,units,inspect,lib}.rs`
- Modify: `xtask/clippy-template.toml` (drop the `Pcg32` pending entry), then regenerate `crates/fl_sim/clippy.toml`
- Rewrite: `crates/fl_sim/tests/{schedule,hash}.rs`. Update `tests/movement.rs`.

**Interfaces:**
- Consumes: Task 9; `fl_math::{Rolls, stream_key}`; `fl_view::{TickInput, TickOutput}`.
- Produces:
```rust
// schedule.rs
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)] pub struct SimTick;
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Phase { Input, Think, Act, Resolve, Economy, Perceive, Lifecycle, Publish }
pub enum InputSet { Housekeeping, Validate, Apply }     pub enum ThinkSet { Orders, Behaviour }     pub enum ActSet { Move, Fire }
pub enum ResolveSet { Projectiles, Effects, Stats }     pub enum EconomySet { Main }                pub enum PerceiveSet { Spatial, Vision }
pub enum LifecycleSet { Death, Spawn, Despawn }         pub enum PublishSet { Snapshot, Events }   // all derive SystemSet + Copy
pub trait SimHook: SystemSet + Copy { fn phase(self) -> Phase; }   // implemented by exactly these eight enums
pub fn configure_sim_schedule(app: &mut App, shuffle_seed: Option<u64>);
//   phases chained; hooks chained inside their phase; one ApplyDeferred system ends each phase;
//   ScheduleBuildSettings { ambiguity_detection: LogLevel::Error, auto_insert_apply_deferred: false, shuffle_seed, .. }; SingleThreadedExecutor
pub fn system_order(app: &mut App) -> String;   // initializes SimTick; one line "<Phase>/<Hook> <system name>" per system, in executed order
// builder.rs (changed)
//   add_systems<M>(&mut self, hook: impl SimHook, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) -> &mut Self
//   add_message<M: Message>(&mut self) -> &mut Self   // rotated once per tick by `rotate_messages` in InputSet::Housekeeping; classified Transient
// step.rs
pub fn step_tick(world: &mut World, input: TickInput) -> TickOutput;
//   panics "driver out of sync: input tick {a}, sim tick {b}" unless input.tick == SimClock.tick
//   1 sort commands by (player, seq) into the inbox (the rev-1 CommandQueue until Task 11)   2 run SimTick
//   3 hash = Some(compute_state_hash) if HashInterval > 0 and tick % HashInterval == 0   4 outputs (snapshot and events: Task 12; journal: Task 18)
//   5 world.clear_trackers()   6 SimClock.tick += 1
#[derive(Resource)] pub struct HashInterval(pub u32);   // External "core.hash_interval"; from RunConfig.hash_interval, else rules StateHash.interval_ticks
// rng.rs  (SimRng and SIM_RNG_STREAM are deleted)
#[derive(Resource, Clone, Copy, Debug, Serialize, Deserialize)] pub struct MatchSeed(pub u64);   // Hashed "core.match_seed", from the header
pub fn rolls(seed: &MatchSeed, tick: Tick, sim_id: SimId, stream_key: u64) -> Rolls;
// counters.rs
#[derive(Resource, Default)] pub struct SimCounters { .. }   // Transient "core.counters"; cleared in InputSet::Housekeeping
impl SimCounters { pub fn add(&mut self, name: &'static str, n: u64); pub fn get(&self, name: &str) -> u64; pub fn snapshot(&self) -> Vec<(&'static str, u64)>; }   // sorted by name
// hash.rs
impl StateHasher { pub fn write_registry_id(&mut self, table: &'static str, id: u32); }   // writes the name key, looked up in the hasher's table context
pub fn compute_state_hash(world: &World) -> u64;
//   the tick; each Hashed resource in name order (name key, then data); the entity count; per SimIndex entry: SimId, then each Hashed component in name order (0, or 1 + data)
// RunConfig gains: pub shuffle_seed: Option<u64>
```
- **System placement:**
  - `apply_commands` → `InputSet::Apply`.
  - `wander_system` → `ThinkSet::Behaviour`. It is entity-local, so any iteration order is allowed, and it draws `rolls(seed, tick, sim_id, stream_key("debug.wander"))`.
  - `move_system` → `ActSet::Move` (entity-local).
  - The `SimEvents` and `SimCounters` clears → `InputSet::Housekeeping`.
- `ScaleScenario::populate` draws positions from `Rolls::new(seed, 0, 0, stream_key("setup.scale"))`, x then y per unit, and inserts `MatchSeed(seed)`.
- `UnitType` hashes through `write_registry_id(tables::UNITS, id)`.

- [ ] **Step 1: Write the failing tests:**
```rust
// tests/schedule.rs
#[test] fn hooks_run_in_declared_order()          // one tracing system per hook (18) → trace == ["Input/Housekeeping", "Input/Validate", …, "Publish/Events"]
#[test] fn clock_advances_per_step()              // 3 × step_tick → SimClock.tick == Tick(3)
#[test] #[should_panic(expected = "driver out of sync")] fn input_tick_must_match_clock()
#[test] #[should_panic] fn ambiguous_systems_fail_schedule_build()   // two unordered writers of one resource in ThinkSet::Orders
#[test] fn commands_visible_only_after_phase_sync()   // ThinkSet::Orders inserts Marker via Commands; ThinkSet::Behaviour sees 0; ActSet::Move sees 1
#[test] fn messages_rotate_once_per_tick()        // written in Think and read in Lifecycle of the same tick (1 read); Messages<T>::len() == 0 two steps later
#[test] fn clear_trackers_runs_each_step()        // despawn in tick t; after step t+1 the RemovedComponents<SimEntity> buffer is empty
#[test] fn counters_reset_each_tick()             // a system adds 5 to "test.n" each tick → get("test.n") == 5 after 3 steps
#[test] fn system_order_lists_core_systems()      // has an "Input/Apply " line naming apply_commands and an "Act/Move " line naming move_system
// tests/hash.rs
#[test] fn equal_worlds_hash_equal()  #[test] fn component_change_changes_hash()  #[test] fn tick_changes_hash()
#[test] fn seed_changes_hash()
#[test] fn hash_every_interval()                  // interval 5, 20 steps → Some at ticks [0, 5, 10, 15], None elsewhere
#[test] fn resources_are_hashed()                 // different Players → different hashes
#[test] fn external_resources_not_hashed()        // HashInterval 1 vs 2 → equal tick-0 hashes
#[test] fn unit_type_hashes_by_name_not_dense_id()   // core_mod vs core_mod_extra, the same tank spawned (dense ids 0 vs 1) → equal hashes
// ported: hash_independent_of_spawn_order, sim_id_allocator_changes_hash
// tests/movement.rs
#[test] fn adding_a_stream_does_not_move_wander() // an extra feature registers "aaa.noise" and draws it every tick → all poses after 200 ticks equal the run without it
// ported to Rolls: wander_stays_within_radius_and_map, scale_populate_is_deterministic
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: FAIL.
- [ ] **Step 3: Implement.** Drop the `Pcg32` pending entry and regenerate (`cargo xtask gen-clippy`).
- [ ] **Step 4: Run** the tests. Expected: PASS, and `cargo clippy -p fl_sim --all-targets -- -D warnings` is clean with the `Pcg32` ban active.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim)!: eight phases with hook sub-sets, step_tick driver steps, keyed RNG, name-keyed state hash, counters` with trailer `Refs: ADR-0005, ADR-0003, ADR-0004`.

### Task 11: T7 rework — command apply order, spawn funnel, `debug.*` handlers, scenario seed

**Track:** sim-core · **Refs:** ADR-0007, ADR-0005, ADR-0008

**Files:**
- Create: `crates/fl_sim/src/debug_cmds.rs`, `crates/fl_sim/tests/funnel.rs`
- Modify: `crates/fl_sim/src/{commands,spawn,units,scenario,scale,step,feature,ids,lib}.rs`, `crates/fl_sim/tests/{commands,movement}.rs`

**Interfaces:**
- Consumes: Tasks 6, 9, 10.
- Produces:
```rust
// commands.rs
#[derive(Resource, Default)] pub struct CommandInbox(pub Vec<PlayerCommand>);   // Transient "core.command_inbox"; filled by step_tick, sorted (player, seq)
#[derive(Resource, Default, Clone, Serialize, Deserialize)] pub struct LastSeq(pub BTreeMap<PlayerId, u32>);   // Hashed "core.last_seq"
pub enum RejectReason { Late, Early, DuplicateSeq, UnknownType, BadPayload, UnknownPlayer, NoValidUnits, DebugDisabled, UnknownContent }
#[derive(Resource, Default)] pub struct CommandLog { pub applied: Vec<PlayerCommand>, pub rejected: Vec<(PlayerCommand, RejectReason)> }   // Transient: this tick only
pub fn last_seqs(world: &World) -> BTreeMap<PlayerId, u32>;
// spawn.rs
#[derive(Message, Clone, Debug)] pub struct SpawnRequest { pub unit: Option<UnitTypeId>, pub owner: PlayerId, pub pos: FxVec2, pub heading: Angle, pub init: Option<SpawnInit> }
pub type SpawnInit = fn(&mut EntityWorldMut<'_>, &SpawnRequest);   // runs after def instantiation; how features spawn non-unit entities
//   unit: Some(id) → SimEntity + Owner + UnitType + Pose + the unit's defs; unit: None → SimEntity + Owner + Pose only; then init
#[derive(Message, Clone, Copy, Debug)] pub struct DespawnRequest(pub SimId);
pub enum SpawnError { UnknownUnit, UnknownPlayer, OutsideMap, NoMap }
pub fn setup_spawn(world: &mut World, req: &SpawnRequest) -> Result<SimId, SpawnError>;   // the setup and load path, outside SimTick; the funnel calls the same code
pub fn setup_despawn(world: &mut World, id: SimId) -> bool;
// scenario.rs
impl MatchSetup { pub fn for_scenario(content: &ContentDb, scenario_id: &str) -> Result<MatchSetup, SetupError>; }   // seed = the scenario's seed, cheats false
pub fn setup_scenario(world: &mut World, scenario_id: &str) -> Result<Vec<SimId>, SetupError>;   // map, Players, units in file order via setup_spawn; never reseeds
// debug_cmds.rs: handlers for the six debug payloads
#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize)] pub struct Invulnerable;   // Hashed "core.invulnerable"
```
- **`apply_commands`** (exclusive, `InputSet::Apply`) takes inbox commands in `(player, seq)` order and checks, in this order:
  1. `exec_tick < tick` → `Late`; `exec_tick > tick` → `Early`.
  2. `seq ≤ LastSeq[player]` → `DuplicateSeq`. `LastSeq` is updated by every command that isn't a duplicate, whether or not it then applies.
  3. Unknown type → `UnknownType`; a decode failure → `BadPayload`; an unknown player → `UnknownPlayer`.
  4. `DEBUG_ONLY` while `!cheats_allowed` → `DebugDisabled`.
  5. Units are deduped and sorted, kept only if they exist, are owned by the player (`DEBUG_ONLY` skips ownership) and pass `unit_can`. None left with `NEEDS_UNITS` → `NoValidUnits`.
  6. The handler may return `UnknownContent`.

  Every rejection is logged at `debug` and pushed as `SimEvent::CommandRejected`.
- **`spawn_funnel`** (exclusive, `LifecycleSet::Spawn`) handles requests in write order through the shared spawn code. A failure increments counter `spawn.rejected` and is logged, never a panic. A success increments `spawn.entities` and pushes `SimEvent::Spawned`.
- **`despawn_funnel`** (`LifecycleSet::Despawn`) dedupes requests, sorts them by `SimId`, increments `despawn.entities` and pushes `SimEvent::Despawned`.
- `SimIdAllocator::alloc` becomes `pub(crate)`.
- **Debug handlers:**
  - `debug.spawn` writes a `SpawnRequest`, resolving the unit name with `unit_id`. An unknown name → `UnknownContent`.
  - `debug.set_owner` rejects an unknown owner with `UnknownPlayer`.
  - `debug.set_hp` sets `hp = min(hp, max)`.
  - `debug.teleport` sets `pos = map.clamp(pos)` and clears `MoveOrder`.
  - `debug.invulnerable` inserts or removes `Invulnerable`.
  - `debug.wander` keeps its rev-1 behaviour.

- [ ] **Step 1: Write the failing tests:**
```rust
// tests/commands.rs — ported: move_command_reaches_target, queued_moves_append, stop_clears_the_move_order,
//   foreign_units_rejected, duplicate_and_missing_units_are_filtered, unknown_player_rejected. New:
#[test] fn commands_apply_in_player_seq_order()   // input [P0 seq 1 Move(300,100), P0 seq 0 Move(200,100)], both non-queued → final waypoint (300,100)
#[test] fn duplicate_seq_rejected()               // two P0 seq 4 commands in one tick → the second is DuplicateSeq
#[test] fn seq_must_increase_across_ticks()       // P0 seq 5 at tick 1; seq 5 at tick 2 → DuplicateSeq; seq 3 at tick 3 → DuplicateSeq
#[test] fn late_and_early_rejected()              // exec_tick = tick − 1 → Late; tick + 1 → Early
#[test] fn debug_requires_cheats()                // cheats false: debug.spawn → DebugDisabled; SimIndex.len() stays 2
#[test] fn debug_spawn_appears_at_end_of_tick()   // cheats true: after the step SimIndex has SimId(3), and SimEvents holds Spawned { sim_id: SimId(3), .. }
#[test] fn debug_spawn_unknown_unit_rejected()    // unit "nope" → UnknownContent
#[test] fn debug_commands_target_any_player()     // P0's debug.set_hp { hp: 1 } on P1's unit → applied; Health.hp == 1
#[test] fn debug_set_owner_invulnerable_teleport()   // Owner == PlayerId(1); Invulnerable present; pos == map.clamp((5000,5000)) and MoveOrder empty
#[test] fn bad_commands_do_not_change_state()     // run B adds a late, an early, a duplicate-seq, a type-999, a [0xFF; 3]-payload, a foreign-SimId and a
                                                  //   debug.spawn "nope" command: 7 rejections summed over ticks; final compute_state_hash == run A's
#[test] fn despawned_unit_command_same_tick_filtered()   // a test system writes DespawnRequest(2) in tick t; a move for unit 2 at t+1 → NoValidUnits
// tests/funnel.rs
#[test] fn spawn_requests_processed_in_write_order()   // P1 seq 0 then P0 seq 0 debug.spawn in one tick → apply order P0, P1 → SimId 3 is owned by P0
#[test] fn spawn_outside_map_rejected_not_panicking()  // a request at (5000,5000) → no entity; counter "spawn.rejected" == 1
#[test] fn despawns_dedupe_and_sort()                  // DespawnRequest 5, 3, 5 → Despawned events in order [3, 5]; counter "despawn.entities" == 2
#[test] fn setup_and_funnel_share_code()               // the same request via setup_spawn and via the funnel → equal inspect_entity apart from sim_id
#[test] fn spawn_init_runs_after_defs()                // a SpawnInit inserting TestTag → TestTag present after the tick
// tests/movement.rs
#[test] fn for_scenario_uses_scenario_seed()           // the duel fixture's seed → MatchSetup.seed
// ported: setup_scenario_spawns_units (ids [1, 2]), spawn_outside_map_is_error, spawn_unknown_unit_is_error (via setup_spawn)
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: FAIL.
- [ ] **Step 3: Implement.** Remove the rev-1 `CommandQueue` and `submit`; tests feed `TickInput`s through `step_tick`.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track sim-core --track contract`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim)!: (player, seq) command apply, spawn funnel, debug.* handlers gated by cheats_allowed` with trailer `Refs: ADR-0007, ADR-0005, ADR-0008`.

### Task 12: `SimRunner` (inline and threaded), Test/Replay/loopback-Lockstep drivers, snapshot publishing

**Track:** contract + sim-core · **Refs:** ADR-0002

**Files:**
- Create: `crates/fl_sim/src/runner/{mod,threaded,test_driver,replay_driver,lockstep}.rs`, `crates/fl_sim/src/publish.rs`, `crates/fl_sim/tests/runner.rs`
- Modify: `crates/fl_sim/src/{step,inspect,feature,lib}.rs`

**Interfaces:**
- Consumes: the `fl_view` runner and driver types (Task 7); `step_tick` (Task 10).
- Produces:
```rust
// publish.rs  (PublishSet::Snapshot; the only place Changed<T> is allowed)
#[derive(Resource, Default)] pub struct SnapshotOut { /* current and previous Arc<SimSnapshot> */ }   // Transient "core.snapshot_out"
//   one record per entity with SimEntity + UnitType + Owner + Pose; hp_frac from Health; flags MOVING (MoveOrder non-empty) and INVULNERABLE;
//   visible_to = 0xFF (vision arrives in M3); records are rebuilt only where Changed<Pose|Health|Owner|MoveOrder|Invulnerable>, the rest copied;
//   the previous buffer is reused when Arc::get_mut succeeds
// runner/mod.rs
pub struct SimRunner { .. }
impl SimRunner {
    pub fn new(app: App) -> SimRunner;
    pub fn next_tick(&self) -> Tick;
    pub fn step(&mut self, input: TickInput) -> TickOutput;
    pub fn run(&mut self, driver: &mut dyn TickDriver, ticks: u32) -> Vec<TickOutput>;   // panics "driver stalled at tick T" if poll returns None
    pub fn query(&mut self, q: SimQuery) -> SimReply;
    pub fn world(&self) -> &World;
    pub fn world_mut(&mut self) -> &mut World;    // setup and tests only
    pub fn spawn_threaded(self) -> (RunnerPort, std::thread::JoinHandle<()>);
}
pub struct TestDriver { .. }
impl TestDriver { pub fn new() -> Self /* delay 0, next Tick(0) */; pub fn with_delay(delay: u32) -> Self; pub fn starting_at(self, next: Tick) -> Self;
                  pub fn restore(&mut self, pending: Vec<PlayerCommand>, last_seq: &BTreeMap<PlayerId, u32>); }
pub struct ReplayDriver { .. }   impl ReplayDriver { pub fn new(inputs: Vec<TickInput>, start: Tick, until: Option<Tick>) -> Self; }
pub trait NetSession: Send {
    fn send(&mut self, tick: Tick, player: PlayerId, commands: Vec<PlayerCommand>);
    fn recv(&mut self) -> Vec<(Tick, PlayerId, Vec<PlayerCommand>)>;
}
pub struct LoopbackSession { .. }
pub fn loopback(players: &[PlayerId]) -> Vec<LoopbackSession>;   // one per player; every send reaches every session, the sender's included
pub struct LockstepDriver<S: NetSession> { .. }
impl<S: NetSession> LockstepDriver<S> { pub fn new(local: PlayerId, players: Vec<PlayerId>, session: S, delay: u32) -> Self; }
// TestDriver, ReplayDriver and LockstepDriver implement fl_view::TickDriver
```
- **Lockstep `poll` for tick T:**
  1. Send the local batch for exec tick `T + delay`: every command issued since the last poll, possibly none.
  2. Drain the session.
  3. Return the input for T only once every player's batch for T is present. Ticks `< delay` need no batches.
- **Threaded runner:** a thread named `fl-sim`. This is the one thread spawn in a sim crate, behind `#[expect(clippy::disallowed_methods, reason = "SimRunner owns the sim thread (spec §5.3)")]`.
  - `ToRunner::Tick` → `step` under `catch_unwind`. On a panic it sends `Failed { tick, message }`, and the thread exits.
  - `Query` → `Reply`.
  - `Shutdown` or a disconnected channel → exit.
- **Query answers:**
  - `Entity` → `inspect_entity`; `WorldSummary` → `inspect_world`.
  - `Perf` → `{"counters": {name: n}}` for the last tick.
  - `Header` → the `MatchHeaderRes`.
  - `JournalSince` → an empty list until Task 18.
  - `Save` → `Error("save: not implemented")` until Task 13.
- `step_tick` now fills `TickOutput.snapshot` from `SnapshotOut` and drains `SimEvents` into `TickOutput.events`.

- [ ] **Step 1: Write the failing tests** (`tests/runner.rs`):
```rust
#[test] fn inline_and_threaded_hash_streams_equal()   // duel + debug.wander 100, 200 ticks, interval 1 → identical Vec<(Tick, u64)>
#[test] fn threaded_panic_is_reported()              // a test feature panics "boom" at tick 5 → FromRunner::Failed { tick: Tick(5), message containing "boom" }; join() returns
#[test] fn drop_port_stops_thread()                  // drop the RunnerPort → the JoinHandle finishes within 2 s
#[test] fn query_between_ticks()                     // after 10 ticks: Entity(SimId(1)) → JSON with components["core.pose"]
#[test] fn test_driver_delay_zero_applies_same_tick() // issued when next is 3 → exec_tick 3; MoveOrder present after that tick
#[test] fn delay_one_applies_next_tick()
#[test] fn replay_driver_reproduces_hashes()         // the TickInputs of a 150-tick TestDriver run with commands → ReplayDriver gives the same hashes
#[test] fn loopback_lockstep_two_sims_identical()    // two runners, LockstepDriver delay 2; P0 moves at local tick 10, P1 at tick 37; 300 ticks →
                                                     //   equal hash streams; the commands appear in the TickInputs of ticks 12 and 39
#[test] fn lockstep_waits_for_missing_peer()         // P1's session never sends → poll for tick 2 == None
#[test] fn snapshot_matches_world()                  // entities.len() == SimIndex.len(); record(1).pos == Pose(1).pos; sorted by sim_id
#[test] fn incremental_publish_equals_full_rebuild() // 500-unit scale, 200 ticks: every snapshot == the test helper publish_full(world)
#[test] fn snapshot_buffers_are_recycled()           // outputs dropped promptly → Arc::as_ptr(out0.snapshot) == Arc::as_ptr(out2.snapshot)
#[test] fn events_drained_per_tick()                 // debug.spawn → that tick's events hold one Spawned; the next tick's events are empty
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test runner`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim): SimRunner (inline + threaded), Test/Replay/loopback Lockstep drivers, snapshot publishing` with trailer `Refs: ADR-0002`.

### Task 13: Save/load skeleton with typed migrations, round-trip and branch tests

**Track:** contract + sim-core · **Refs:** ADR-0006, ADR-0004

**Files:**
- Create: `crates/fl_sim/src/save/{mod,format,load}.rs`, `crates/fl_sim/tests/save.rs`
- Modify: `crates/fl_sim/src/{classify,units,runner/mod,ids,lib}.rs`

**Interfaces:**
- Consumes: the state registry and codecs (Task 9), the spawn path (Task 11) and `MatchHeader::check_save` (Task 6).
- Produces:
```rust
pub const SAVE_FORMAT_VERSION: u32 = 1;
#[derive(Serialize, Deserialize)] pub struct SaveFile { pub format_version: u32, pub header: MatchHeader, pub sim: SimSave }
#[derive(Serialize, Deserialize)] pub struct SimSave { pub tick: Tick, pub sim_id_next: u32, pub pending_commands: Vec<PlayerCommand>,
    pub resources: Vec<(String, u32, Vec<u8>)>,                 // Hashed resources, sorted by name
    pub entities: Vec<(SimId, Vec<(String, u32, Vec<u8>)>)> }   // sorted by SimId, then component name
pub fn save_world(world: &World, pending: &[PlayerCommand]) -> Vec<u8>;   // postcard
pub fn load_world(features: &[Arc<dyn SimFeature>], content: Arc<ContentDb>, bytes: &[u8], run: RunConfig) -> Result<Loaded, LoadError>;
pub struct Loaded { pub app: App, pub pending: Vec<PlayerCommand>, pub last_seq: BTreeMap<PlayerId, u32> }
pub enum LoadError { Decode(String), FormatVersion { found: u32, supported: u32 }, Header(HeaderMismatch), UnknownComponent(String),
    UnknownResource(String), NoMigration { name: String, from: u32, to: u32 }, Codec { name: String, message: String }, Build(BuildError) }
impl<T> Codec<T> {
    pub fn migrate_from<Old: DeserializeOwned + Into<T>>(self, version: u32) -> Self;   // decodes bytes saved at `version`, then .into()
    pub fn custom(version: u32, encode: fn(&T, &SaveCtx) -> Vec<u8>, decode: fn(u32, &[u8], &LoadCtx) -> Result<T, String>) -> Self;
}
pub struct SaveCtx<'a> { .. }
pub struct LoadCtx<'a> { .. }   impl LoadCtx<'_> { pub fn remap(&self, table: &str, old: u32) -> Result<u32, String>; }   // saved tables → current tables
```
- **Load procedure:**
  1. Decode, and check `format_version`.
  2. Build a fresh app with `MatchSetup { seed: header.seed, cheats_allowed: header.cheats_allowed }`.
  3. Run `current_header.check_save(&saved.header)`. The rules hash is not compared, so a hot-reload rebuild can load against new content.
  4. Spawn the entities in `SimId` order with their saved ids, through the spawn path (`pub(crate) fn spawn_with_id`).
  5. Decode components and resources by name, migrating by version.
  6. Set `SimIdAllocator` and `SimClock`.
  7. Run every `Derived` rebuild in registry-name order.
- `UnitType` uses `Codec::custom`: it saves the dense id and remaps it through `tables::UNITS` on load.
- The runner answers `SimQuery::Save { pending }` with `SimReply::Save(save_world(..))`.

- [ ] **Step 1: Write the failing tests** (`tests/save.rs`; hash interval 1):
```rust
#[test] fn round_trip_is_byte_identical()        // duel + wander, 50 ticks: s1 = save; load; s2 = save → s1 == s2
#[test] fn save_at_tick_zero_round_trips()
#[test] fn branch_test_hashes_match_every_tick() // A runs 0..120 with moves at 10 and 60; B runs to 40, saves, loads fresh and feeds the same inputs to 120 → hashes equal for 40..120
#[test] fn branch_after_despawns()               // two units despawned before tick 40 (SimId gaps) → branch hashes equal
#[test] fn pending_commands_survive_save()       // TestDriver delay 2 holds a move; save at that point; load + driver.restore → applied at the same exec_tick, same hashes
#[test] fn load_rejects_unknown_component_name() // rename "core.pose" to "core.posee" in the decoded SaveFile and re-encode → Err(UnknownComponent("core.posee"))
#[test] fn load_rejects_feature_mismatch()       // saved with an extra feature → Err(Header(HeaderMismatch::Features))
#[test] fn migration_from_v1()                   // a test component at Codec v2 .migrate_from::<v1::TestHp>(1); a save holding v1 bytes → the migrated value
#[test] fn load_remaps_unit_type_ids()           // saved with core_mod, loaded with core_mod_extra → UnitType names preserved; equal state hash
#[test] fn derived_state_rebuilt_on_load()       // SimIndex after load == SimIndex before save
#[test] fn runner_save_query()                   // SimQuery::Save { pending: vec![] } → SimReply::Save(bytes), and load_world(bytes) is Ok
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test save`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim): versioned save/load with typed migrations, round-trip and branch tests` with trailer `Refs: ADR-0006`. The controller then runs `cargo xtask gate pr` and fast-forwards `main`; the contracts are in place.

---

## Phase III — revised rev-1 tasks (two lanes)

### Task 14: Base content and `fl_catalog` (rev-1 T8)

**Track:** content + sim-core · **Lane 1**

**Files:**
- Create `crates/fl_catalog/src/lib.rs` and `crates/fl_catalog/tests/base_content.rs`, plus the content files, all as listed in rev-1 Task 8:
  - `assets/base/{mod,rules,credits}.ron`, `factions/`
  - `units/templates/` and `units/{de,su}/`
  - `visuals/`, `maps/{testbed_1km,scale_2km}.map.ron`, `scenarios/testbed.scenario.ron`

**Interfaces:**
- Consumes: `SimFeature`, `load_content`, `build_sim_app`, `MatchSetup::for_scenario`, `setup_scenario`, `SimRunner`, `TestDriver`.
- Produces:
  - `pub fn features() -> Vec<Arc<dyn SimFeature>>`, empty in M0 (core is implicit; each feature crate adds one line here)
  - `pub fn base_mod_dir() -> PathBuf`: `env!("CARGO_MANIFEST_DIR")/../../assets/base`, canonicalized
- Content: identical to rev-1 Task 8's **Content** list (factions, maps, the `base.testbed` scenario with seed 1, `rules.ron` with `StateHash` and `Spatial`, an empty `credits.ron`, ten units using only `Selectable`, `Hull` and `Mobility`, and the same stats). It is our own numbers, never Nival's.

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn base_content_loads()    // load_content(&features(), &[base_mod_dir()]) → Ok: 10 concrete units, 2 factions, 2 maps, 1 scenario, warnings empty
#[test] fn base_testbed_runs()     // MatchSetup::for_scenario(&db, "base.testbed") has seed 1; with cheats_allowed = true, build → setup_scenario returns 10 ids;
                                   //   SimRunner + TestDriver run 100 ticks; the last snapshot has 10 records
#[test] fn base_string_tables()    // header table "content.unit" has 10 names, sorted ascending
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_catalog`. Expected: FAIL.
- [ ] **Step 3: Write** the RON files and `lib.rs`.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track content --track sim-core`. Expected: PASS. `data-check` stays SKIP until Task 16.
- [ ] **Step 6: Commit** `feat(content): base mod with DE/SU placeholder roster; fl_catalog`.

### Task 15: Incremental spatial grid (rev-1 T9)

**Track:** sim-core · **Lane 1**

**Files:**
- Modify: `crates/fl_sim/src/spatial.rs`, plus a one-line `init_spatial` call each in `scenario.rs` (`setup_map`) and `scale.rs`
- Create: `crates/fl_sim/tests/spatial.rs`

**Interfaces:**
- Consumes: `MapInfo`, `Pose`, `SimIndex`, `SpatialRules`, `SimCounters`, `PerceiveSet::Spatial`, `Class`, `ScaleScenario`.
- Produces:
```rust
#[derive(Resource, Debug, Clone)] pub struct SpatialGrid { .. }   // Derived "core.spatial_grid", rebuild = rebuild_spatial
impl SpatialGrid {
    pub fn new(map: &MapInfo, cell: Fx) -> Self;
    pub fn cell_of(&self, pos: FxVec2) -> u32;             // clamps into the grid; pos == size maps to the last cell
    pub fn insert(&mut self, id: SimId, cell: u32);
    pub fn remove(&mut self, id: SimId, cell: u32) -> bool;
    pub fn relocate(&mut self, id: SimId, from: u32, to: u32);
    pub fn members(&self, cell: u32) -> &[SimId];          // sorted ascending
    pub fn query_radius(&self, center: FxVec2, radius: Fx, out: &mut Vec<SimId>) -> u32;   // broad phase over cells overlapping the circle's AABB;
                                                           //   out cleared, sorted, unique; returns the candidate count for "spatial.query_candidates"
    pub fn len(&self) -> usize;
}
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)] #[component(on_remove = …)] pub struct SpatialCell(pub u32);   // Derived "core.spatial_cell"
pub fn rebuild_spatial(world: &mut World);   // a new grid from MapInfo + every Pose; also the load rebuild
pub(crate) fn init_spatial(world: &mut World);
```
- **`track_spatial`** (`PerceiveSet::Spatial`) walks `SimIndex` in order. It inserts a `SpatialCell` where one is missing and relocates when the cell changed. It counts `spatial.inserts` and `spatial.relocations`.
- No `Changed<Pose>` and no observer is used. The `SpatialCell` `on_remove` hook removes the entity from the grid; that is index maintenance, which hooks may do.

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn insert_keeps_cells_sorted()
#[test] fn position_on_max_edge_maps_to_last_cell()   // 1000 m map, cell 32: cell_of((1000,1000)) == cols * rows − 1
#[test] fn relocate_moves_between_cells()
#[test] fn query_radius_matches_brute_force()         // proptest: random ids and positions on a 2 km map, radius 1..300 → contains the exact set; sorted, unique
#[test] fn grid_tracks_wandering_units()              // ScaleScenario with 500 units, 300 ticks: every SpatialCell == cell_of(pose), membership exact, len() == 500
#[test] fn despawn_removes_from_grid()
#[test] fn grid_rebuilt_after_load()                  // save at tick 100, load → the grid equals the live grid
#[test] fn spatial_counters_recorded()                // on the first tick after setup: counter "spatial.inserts" == unit count
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test spatial`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim): incremental uniform spatial grid (derived, rebuilt on load)`.

### Task 16: `fl_cli` data-check and hash-dump (rev-1 T10)

**Track:** tools · **Lane 2** (cut after Task 14)

**Files:**
- Create: `crates/fl_cli/src/{lib,main,data_check,hash_dump}.rs`, `crates/fl_cli/tests/data_check.rs`
- Create fixture `crates/fl_cli/tests/fixtures/bad_mod/**` (a missing dependency, an unknown component, an unknown file kind)

**Interfaces:**
- Consumes: `fl_catalog::{features, base_mod_dir}`, `load_content`, `SimRunner`, `TestDriver`, `MatchSetup::for_scenario`.
- Produces:
```rust
pub struct DataCheckReport { pub mods: Vec<String>, pub units: usize, pub kinds: Vec<(&'static str, usize)>, pub warnings: Vec<String>, pub rules_hash: u64 }
pub fn data_check(dirs: &[PathBuf]) -> Result<DataCheckReport, BuildError>;
pub fn hash_dump(mods: &[PathBuf], scenario: &str, ticks: u32, interval: u32) -> Result<Vec<(Tick, u64)>, CliError>;
#[derive(Debug, thiserror::Error)] pub enum CliError { Build(String), Setup(String), Io(String), Usage(String) }
```
- CLI (`clap` derive):
  - `fl_cli data-check <DIRS>...` prints the report, including `rules hash: 0x{:016x}`. On failure it prints every error and exits 1.
  - `fl_cli hash-dump --mods <DIR>... --scenario <ID> --ticks <N> [--interval <N>]` prints one `tick hash` line per hash.

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn base_mod_passes()               // data_check(&[base_mod_dir()]) is Ok with units == 10
#[test] fn bad_mod_lists_all_errors()      // ≥ 3 errors, and each one's Display contains "bad_mod"
#[test] fn data_check_bad_mod_exit_code()  // the binary (env!("CARGO_BIN_EXE_fl_cli")) on the bad fixture → exit code 1, output contains every error
#[test] fn hash_dump_is_repeatable()       // base.testbed, 100 ticks, interval 10 → two calls equal, 10 entries
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_cli`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS, and `cargo run -p fl_cli -- data-check assets/base` prints the report.
- [ ] **Step 5: Gate** `cargo xtask gate task --track tools`. Expected: `PASS data-check` (the row is now active) and `gate task: PASS`.
- [ ] **Step 6: Commit** `feat(fl_cli): data-check and hash-dump`.

### Task 17: `fl_test_utils` — ScenarioBuilder, twin-run, shuffle runs, save fitness helpers (rev-1 T11)

**Track:** sim-core · **Refs:** ADR-0005, ADR-0006 · **Lane 1**

**Files:**
- Create: `crates/fl_test_utils/src/{lib,builder,determinism,fitness}.rs`, `crates/fl_test_utils/tests/determinism.rs`

**Interfaces:**
- Consumes: Tasks 9–14.
- Produces:
```rust
pub fn base_content() -> Arc<ContentDb>;                 // OnceLock; fl_catalog features + base mod
pub struct ScenarioBuilder { .. }
impl ScenarioBuilder {
    pub fn new() -> Self;                                // base_content(), catalog features, seed 0, hash interval 1, cheats true, no shuffle
    pub fn with_content(content: Arc<ContentDb>, features: Vec<Arc<dyn SimFeature>>) -> Self;
    pub fn feature(self, f: Arc<dyn SimFeature>) -> Self;
    pub fn seed(self, seed: u64) -> Self;                // overrides the scenario seed
    pub fn cheats(self, on: bool) -> Self;
    pub fn hash_interval(self, n: u32) -> Self;
    pub fn shuffle_seed(self, seed: Option<u64>) -> Self;
    pub fn scenario(self, id: &str) -> Self;
    pub fn map_flat(self, width_m: i32, height_m: i32) -> Self;
    pub fn player(self, faction: &str, team: u8) -> Self;          // PlayerId in call order
    pub fn unit(self, unit: &str, player: u8, x: i32, y: i32) -> Self;   // builder units get SimIds after scenario units, in call order
    pub fn scale(self, s: ScaleScenario) -> Self;
    pub fn wander_all(self, radius_m: i32) -> Self;                // a debug.wander per player at tick 0
    pub fn command<P: CommandPayload>(self, tick: u32, player: u8, units: &[u32], payload: P) -> Self;   // issued through the TestDriver to apply at `tick`
    pub fn build(self) -> Scenario;                      // panics with the setup error's text
}
pub struct Scenario { pub runner: SimRunner, pub driver: TestDriver, .. }
impl Scenario {
    pub fn run_ticks(&mut self, n: u32);
    pub fn tick(&self) -> Tick;
    pub fn hashes(&self) -> Vec<(Tick, u64)>;
    pub fn last_output(&self) -> Option<&TickOutput>;
    pub fn world(&self) -> &World;
    pub fn pose(&self, id: u32) -> Option<Pose>;
    pub fn issue<P: CommandPayload>(&mut self, player: u8, units: &[u32], payload: P, queue: bool);
    pub fn save(&mut self) -> Vec<u8>;                   // SimQuery::Save with the driver's pending commands
    pub fn load(features: Vec<Arc<dyn SimFeature>>, content: Arc<ContentDb>, bytes: &[u8]) -> Scenario;   // restores the driver from Loaded
}
pub fn assert_deterministic(make: impl Fn() -> ScenarioBuilder, ticks: u32);   // twin run; panics "diverged at tick T: a=0x… b=0x…"
pub fn shuffle_seeds() -> Vec<u64>;   // FL_SHUFFLE_SEEDS=k (default 1) → the first k of [0x5EED_0001, 0x5EED_0002, 0x5EED_0003]
pub fn assert_shuffle_invariant(make: impl Fn() -> ScenarioBuilder, ticks: u32);   // every shuffle_seeds() run == the unshuffled hash stream
pub fn assert_save_round_trip(s: &mut Scenario);                                   // save(load(save)) == save, byte for byte
pub fn assert_branch(make: impl Fn() -> ScenarioBuilder, n: u32, m: u32);         // run A to m; B to n, save, load, run to m; hashes equal for n..m
```
- [ ] **Step 1: Write the failing tests** (`tests/determinism.rs`; `smoke_` tests run in every `gate task`):
```rust
#[test] fn smoke_testbed_twin_run()        // base.testbed + wander_all(100), 600 ticks
#[test] fn smoke_shuffle_invariant()       // base.testbed + wander_all(100), 200 ticks, shuffle_seeds()
#[test] fn smoke_save_round_trip()         // base.testbed + wander, at tick 50
#[test] fn smoke_branch()                  // base.testbed + wander + moves at 10 and 60, n = 40, m = 120
#[test] fn scale_twin_run_500_units()      // ScaleScenario { units: 500, .. }, 200 ticks
#[test] fn commands_mid_run_twin()         // moves at ticks 10, 50 and 90
#[test] fn different_seeds_diverge()       // seeds 1 vs 2, 500-unit scale → different final hashes
#[test] fn builder_units_get_sequential_ids()
#[test] #[should_panic(expected = "diverged at tick")] fn divergence_is_reported()   // the builder reads a process-global counter so the second run moves one unit
#[test] fn golden_stability_extra_feature()
//   a dummy feature registers command "aaa.noop", Hashed component "aaa.marker" (never inserted) and stream "aaa.stream" (drawn every tick)
//   → the base.testbed + wander hash stream over 300 ticks equals the run without it
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_test_utils`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_test_utils`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track sim-core`. Expected: four `smoke_*` tests run in `test-headless`, and `gate task: PASS`.
- [ ] **Step 6: Commit** `feat(fl_test_utils): ScenarioBuilder, twin-run, shuffle runs, save round-trip and branch helpers`.

### Task 18: Sim observability core — journal, phase markers, tick span, replays (rev-1 T18)

**Track:** sim-core (+ contract for `step.rs`, `builder.rs`) · **Refs:** ADR-0007, ADR-0004 · **Lane 1**

**Files:**
- Create:
  - `crates/fl_sim/src/{observe,replay}.rs`, `crates/fl_sim/tests/{observe,replay}.rs`
  - `crates/fl_test_utils/tests/golden.rs`
  - `crates/fl_test_utils/tests/replays/testbed_wander.replay`, produced as `.new` and blessed
- Modify: `crates/fl_sim/src/{step,builder,build,commands,spawn,scale,runner/mod,lib}.rs`, and `crates/fl_test_utils/src/builder.rs`, which gains `Scenario::journal(&self) -> &[JournalEntry]` (every entry of the run) and `Scenario::replay(&self) -> Replay` (`Scenario` records every input and output through a `ReplayRecorder`)

**Interfaces:**
- Consumes: `JournalEntry`, `TickInput`, `TickOutput` (Task 7), `MatchHeader::check_replay` (Task 6), `ReplayDriver` (Task 12).
- Produces:
```rust
// observe.rs
pub trait JournalRecord: Serialize + Debug + Send + Sync + 'static { const KIND: &'static str; }
#[derive(Resource)] pub struct Journal { .. }   // Transient "core.journal"; registered with allow_ambiguous_resource so unordered systems may record
impl Journal {
    pub fn enabled(&self) -> bool;
    pub fn record<R: JournalRecord>(&mut self, tick: Tick, phase: Phase, sim_id: Option<SimId>, player: Option<PlayerId>, rec: &R);
    //   no-op when disabled; debug-asserts R::KIND is registered
    pub fn len(&self) -> usize;
}
// SimBuilder::register_journal_record<R: JournalRecord>(&mut self) -> &mut Self   // stable-string registry, table JOURNAL_RECORDS
// RunConfig gains: pub journal: Option<bool> (None = cfg!(debug_assertions)), pub phase_markers: bool
// step_tick: runs SimTick inside tracing::info_span!("sim_tick", tick = n); drains the journal, stable-sorts by (phase, kind, sim_id, emitted) into TickOutput.journal
// phase markers (when RunConfig.phase_markers): the first and last system of each phase emit
//   tracing::trace!(target: "fl_sim::phase", phase = "<Phase>", edge = "start" | "end") — events only, no clock in the sim
// core records: "core.command_applied" { command, seq, units, queue }, "core.command_rejected" { command, seq, reason },
//               "core.unit_spawned" { unit, owner, pos }, "core.unit_despawned" {}, "core.move_order" { waypoints }
// runner: SimQuery::JournalSince { tick, limit } answered from a ring of the last 10_000 entries
// replay.rs
pub const REPLAY_FORMAT_VERSION: u32 = 1;
#[derive(Serialize, Deserialize)] pub enum ReplayStart { Scenario(String), Scale(ScaleScenario), Save(Vec<u8>) }   // ScaleScenario gains Serialize + Deserialize
#[derive(Serialize, Deserialize)] pub enum ReplayEvent { ContentReload { tick: Tick, rules_hash: u64 } }
#[derive(Serialize, Deserialize)] pub struct Replay { pub version: u32, pub header: MatchHeader, pub start: ReplayStart, pub hash_interval: u32,
    pub ticks: u32, pub inputs: Vec<TickInput> /* non-empty ticks only */, pub hashes: Vec<(Tick, u64)>, pub events: Vec<ReplayEvent> }
impl Replay {
    pub fn to_bytes(&self) -> Vec<u8>;                       // postcard
    pub fn from_bytes(bytes: &[u8]) -> Result<Replay, ReplayError>;
    pub fn to_json(&self) -> serde_json::Value;              // the readable view for `fl_cli replay dump --json`
    pub fn reproducible(&self) -> bool;                      // false if any ContentReload event
}
pub struct ReplayRecorder { .. }
impl ReplayRecorder { pub fn new(header: MatchHeader, start: ReplayStart, hash_interval: u32) -> Self; pub fn record(&mut self, input: &TickInput, output: &TickOutput); pub fn finish(self) -> Replay; }
pub enum VerifyOutcome { Verified { ticks: u32, warnings: Vec<String> }, Diverged(Divergence), NonReproducible }
#[derive(Debug, Clone, PartialEq)] pub struct Divergence { pub tick: Tick, pub expected: u64, pub actual: u64 }
pub fn verify_replay(r: &Replay, features: &[Arc<dyn SimFeature>], content: Arc<ContentDb>) -> Result<VerifyOutcome, ReplayError>;
pub fn first_divergence(a: &[(Tick, u64)], b: &[(Tick, u64)]) -> Option<Divergence>;
#[derive(Debug)] pub enum ReplayError { Decode(String), Version { found: u32 }, Header(HeaderMismatch), Setup(String) }
```
- **Golden replay.**
  - `golden.rs::golden_replays_verify` verifies every `tests/replays/*.replay` and expects `Verified`.
  - With `FL_RECORD_GOLDEN=1` it records base.testbed + `wander_all(100)` + moves at ticks 10 and 60, for 400 ticks, into `testbed_wander.replay.new`.
  - The lead blesses it with `cargo xtask bless crates/fl_test_utils/tests/replays --reason "M0 golden: testbed wander"`.

- [ ] **Step 1: Write the failing tests:**
```rust
// tests/observe.rs
#[test] fn applied_and_rejected_commands_are_journaled()   // one move + one foreign-unit move → exactly one "core.command_applied" and one "core.command_rejected" with reason "NoValidUnits"
#[test] fn journal_disabled_records_nothing()
#[test] fn journal_does_not_change_state_hash()           // journal on vs off → equal hash streams
#[test] fn journal_identical_across_shuffle_seeds()       // two unordered test systems record; the journals of 3 shuffle seeds are equal
#[test] fn journal_entries_sorted()                       // within each tick, sorted by (phase, kind, sim_id, emitted)
#[test] fn phase_markers_emit_16_events_per_tick()        // a capturing subscriber counts 8 "start" + 8 "end" events on target fl_sim::phase per step
#[test] fn tick_span_carries_tick()                       // an event emitted inside a system at tick 7 is inside a span with field tick == 7
#[test] fn journal_since_query()                          // after 20 ticks, JournalSince { tick: 10, limit: 5 } → 5 entries, all with tick ≥ 10
// tests/replay.rs
#[test] fn replay_round_trip_verifies()                   // record → to_bytes → from_bytes → verify → Verified
#[test] fn tampered_replay_reports_divergence()           // change the target of the command at tick 37 → Diverged { tick: Tick(37), .. } (hash interval 1)
#[test] fn replay_with_reload_event_is_non_reproducible() // → NonReproducible
#[test] fn replay_build_id_mismatch_warns()               // Verified { warnings.len() == 1 }
#[test] fn replay_rules_hash_mismatch_fails()             // Err(Header(HeaderMismatch::RulesHash))
#[test] fn first_divergence_finds_earliest()
#[test] fn replay_json_lists_inputs()                     // to_json()["inputs"][0]["commands"][0]["seq"] is a number
// fl_test_utils/tests/golden.rs
#[test] fn golden_replays_verify()
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test observe --test replay`. Expected: FAIL.
- [ ] **Step 3: Implement.** Run `FL_RECORD_GOLDEN=1 cargo nextest run -p fl_test_utils --test golden`, then ask the lead to bless the `.new` file.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS, including `golden_replays_verify`.
- [ ] **Step 5: Gate** `cargo xtask gate task --track sim-core --track contract`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim): event journal with registry and per-tick sort, phase markers, tick span, postcard replays` with trailers `Refs: ADR-0007` and `Blessed: M0 golden: testbed wander`.

### Task 19: `fl_observe` — logging setup, profiler layer, journal sink, `debug.ron` (rev-1 T19)

**Track:** tools · **Lane 2**

**Files:**
- Create: `crates/fl_observe/src/{lib,config,logging,profiler,sink}.rs`, `crates/fl_observe/tests/{profiler,profiler_sim}.rs`, `debug.ron`

**Interfaces:**
- Consumes: `JournalEntry` (Task 7). Its dev-dependencies use `fl_sim` with the `core_mod` fixture by path.
- Produces (as rev-1 Task 19, with these changes):
```rust
#[derive(Clone, Debug, Deserialize, Serialize)] #[serde(deny_unknown_fields, default)]
pub struct DebugConfig { pub log_filter: String /* "info,fl_sim=debug" */, pub json_log: bool, pub journal_file: bool, pub profiler: bool, pub budgets: Budgets }
#[derive(Clone, Debug, Default, Deserialize, Serialize)] #[serde(deny_unknown_fields, default)]
pub struct Budgets { pub tick_ms: f64 /* 15.0 */, pub per_phase_ms: BTreeMap<String, f64>, pub per_feature_ms: BTreeMap<String, f64> }   // warn only
pub fn load_debug_config(path: Option<&Path>) -> DebugConfig;     // explicit path, else ./debug.ron, else defaults; RUST_LOG overrides log_filter
pub struct RunDir { pub id: String, pub path: PathBuf }
pub fn new_run_dir(root: &Path) -> std::io::Result<RunDir>;       // root/<YYYYMMDD-HHMMSS>-<pid>/ plus root/latest.txt
pub fn json_file_layer<S>(run: &RunDir) -> impl Layer<S>;          // log.jsonl: timestamp, level, target, fields, and the span fields (so "tick" comes from the sim_tick span)
pub fn init_headless(cfg: &DebugConfig, run: Option<&RunDir>) -> &'static Profiler;   // EnvFilter + stderr fmt + json file + profiler; idempotent
pub fn set_global_run_dir(run: RunDir); pub fn global_run_dir() -> Option<&'static RunDir>; pub fn global_profiler() -> &'static Profiler;
pub struct Profiler { .. }
impl Profiler {
    pub fn new() -> Profiler;
    pub fn layer(&'static self) -> ProfilerLayer;   // times bevy_ecs "system" spans by name, on any thread; times phases from fl_sim::phase start/end events;
                                                    //   folds one tick when a "sim_tick" span closes; client frames fold on the "frame" span
    pub fn set_budgets(&self, b: Budgets);
    pub fn snapshot(&self) -> PerfSnapshot;
    pub fn reset(&self);
}
#[derive(Clone, Debug, Serialize)] pub struct Stat { pub mean_ms: f64, pub p50_ms: f64, pub p95_ms: f64, pub max_ms: f64, pub samples: u64 }
#[derive(Clone, Debug, Serialize)] pub struct PerfSnapshot { pub ticks: u64, pub tick: Stat, pub phases: Vec<(String, Stat)>, pub features: Vec<(String, Stat)>,
    pub systems: Vec<SystemStat>, pub history_ms: Vec<f32> /* last 240 ticks */ }
#[derive(Clone, Debug, Serialize)] pub struct SystemStat { pub name: String, pub feature: String, pub stat: Stat, pub history_ms: Vec<f32> }
pub fn feature_of(system_name: &str) -> String;
pub struct JournalSink { .. }   impl JournalSink { pub fn create(run: &RunDir) -> std::io::Result<Self>; pub fn write(&mut self, entries: &[JournalEntry]) -> std::io::Result<()>; }
```
- Rolling statistics use a 240-tick window with nearest-rank percentiles.
- `Instant` here sits behind `#[expect(clippy::disallowed_types, reason = "observe-only wall clock")]`.
- `debug.ron` holds the defaults above plus per-phase budgets of 3.0 ms each.

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn feature_of_maps_paths()
//   "fl_feat_artillery::fire::x" → "artillery"; "fl_feat_infantry_client::ui::y" → "infantry (client)"; "fl_sim::movement::move_system" → "core.movement";
//   "fl_client::view::interpolate_views" → "client.view"; "bevy_render::view::x" → "bevy"; "other::a" → "other"
#[test] fn phase_times_from_marker_events()    // synthetic start/end events for 8 phases plus a sim_tick span, 30 ticks → snapshot.phases.len() == 8, each with samples == 30
#[test] fn system_spans_on_another_thread()    // synthetic "system" spans emitted on a spawned thread → attributed and counted
#[test] fn budget_overrun_warns()              // tick_ms 0.0 → a captured warning on target fl_observe::budget
#[test] fn percentiles_nearest_rank()          // [1..=100] → p50 == 50, p95 == 95
#[test] fn debug_config_defaults_and_rust_log_override()
#[test] fn json_layer_writes_tick_field()      // an event inside a "sim_tick" span with tick 3 → a log.jsonl line containing "tick":3
#[test] fn journal_sink_writes_jsonl()
// tests/profiler_sim.rs (its own process, global subscriber)
#[test] fn profiler_times_real_sim_systems()   // core_mod duel with RunConfig { phase_markers: true, .. }, threaded runner, 30 ticks →
                                               //   snapshot has system "fl_sim::movement::move_system" with feature "core.movement", samples ≥ 30
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_observe`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track tools`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_observe): logging setup, profiler layer (both threads, phase markers), journal sink, debug.ron`.

### Task 20: Client view — `ClientFeature`, `LocalDriver`, runner link, snapshot interpolation, placeholders (rev-1 T13)

**Track:** client · **Refs:** ADR-0002, ADR-0001 · **Lane 2** (cut after Tasks 7 and 8)

**Files:**
- Create: `crates/fl_client/src/{feature,driver,link}.rs`, `crates/fl_client/tests/view.rs` (a fake runner thread built on `fl_view::runner_channel`)
- Modify: `crates/fl_client/src/{view,placeholder,lib}.rs`

**Interfaces:**
- Consumes: `fl_view` (Task 7), `fl_protocol` payloads and `StringTables` (Task 6), `fl_content::{VisualDef, Placeholder, Faction, ContentDb}`.
- Produces:
```rust
// feature.rs
pub trait ClientFeature: Send + Sync + 'static { fn sim_feature(&self) -> &'static str; fn build(&self, app: &mut App); }
// driver.rs
#[derive(Resource)] pub struct LocalDriver { .. }   // input delay 1; wraps fl_view::CommandStamper
impl LocalDriver {
    pub fn new(next: Tick) -> Self;
    pub fn advance(&mut self, dt: Duration);         // dt × speed is added to the accumulator unless paused
    pub fn pause(&mut self); pub fn resume(&mut self); pub fn is_paused(&self) -> bool;
    pub fn step(&mut self, n: u32);                  // while paused, allow n ticks
    pub fn set_speed(&mut self, x: f32);             // clamped to [0.25, 8.0]
    pub fn speed(&self) -> f32;
    pub fn overstep_fraction(&self) -> f32;          // accumulator / tick length, in [0, 1)
    pub fn restore(&mut self, next: Tick, pending: Vec<PlayerCommand>, last_seq: &BTreeMap<PlayerId, u32>);
}
impl TickDriver for LocalDriver { /* poll: Some while the accumulator ≥ 50 ms (or a step is allowed), carrying take_due(tick) */ }
// link.rs
#[derive(Resource)] pub struct RunnerLink { pub port: RunnerPort, pub header: MatchHeader }
#[derive(Resource, Default)] pub struct SnapshotBuffer { pub prev: Option<Arc<SimSnapshot>>, pub curr: Option<Arc<SimSnapshot>> }
#[derive(Message, Clone, Debug)] pub struct SimEventMsg(pub SimEvent);
#[derive(Resource, Default)] pub struct RunnerStatus { pub failed: Option<RunnerFailure>, pub last_tick: Option<Tick>, pub last_hash: Option<u64> }
#[derive(Resource, Default)] pub struct PendingReplies { /* id → SimReply; next id */ }
pub fn encode_command<P: CommandPayload>(tables: &StringTables, payload: &P) -> Result<CommandKind, String>;   // id from the "commands" table by P::NAME
pub fn send_query(link: &RunnerLink, replies: &mut PendingReplies, q: SimQuery) -> u64;
// view.rs
#[derive(Resource, Clone)] pub struct ClientContent(pub Arc<ContentDb>);
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)] pub struct LocalPlayer(pub PlayerId);
#[derive(Resource, Default, Clone, Copy)] pub struct RevealAll(pub bool);   // render override: draw every record regardless of visible_to
#[derive(Component, Clone, Copy, Debug)] pub struct UnitView { pub sim_id: SimId, pub unit: UnitTypeId, pub owner: PlayerId }
#[derive(Component, Clone, Copy, Debug)] pub struct ViewPose { pub prev: Vec3, pub curr: Vec3, pub prev_yaw: f32, pub curr_yaw: f32 }
#[derive(Resource, Default)] pub struct ViewIndex(pub BTreeMap<SimId, Entity>);
pub fn sim_to_world(pos: FxVec2, height: f32) -> Vec3;   // Vec3::new(x, height, y)
pub fn heading_to_yaw(h: Angle) -> f32;                    // -h.to_radians_f32()
pub fn lerp_yaw(a: f32, b: f32, t: f32) -> f32;           // shortest arc
// SimViewPlugin, in Update, chained:
//   drive_runner: LocalDriver.advance(real dt), send every polled TickInput; drain ALL FromRunner messages:
//     Tick → rotate SnapshotBuffer, write one SimEventMsg per event, update RunnerStatus; Reply → PendingReplies; Failed → RunnerStatus.failed + pause
//   sync_views: merge SnapshotBuffer.curr with ViewIndex (spawn new, despawn vanished, shift curr → prev)
//   interpolate_views: Transform from ViewPose at LocalDriver.overstep_fraction()
// PlaceholderPlugin: as rev-1 Task 13 (shared mesh and material handles per unit type and player)
```
- [ ] **Step 1: Write the failing tests** (`MinimalPlugins` + `SimViewPlugin`, with a fake runner thread replying to `ToRunner::Tick` with canned `TickOutput`s):
```rust
#[test] fn sim_to_world_axes()  #[test] fn heading_90_faces_plus_z()  #[test] fn lerp_yaw_wraps()  #[test] fn interpolation_midpoint()   // as rev-1 T13
#[test] fn local_driver_stamps_delay_one()          // next 5 → exec_tick 6; seqs 0, 1 for two P0 commands
#[test] fn local_driver_accumulates_fixed_steps()   // advance(120 ms) at speed 1 → 2 polls, then None; overstep_fraction ≈ 0.4 (±1e-3)
#[test] fn speed_clamped()                          // set_speed(100.0) → 8.0; set_speed(0.0) → 0.25
#[test] fn paused_driver_emits_nothing_until_step() // paused, advance(1 s) → no poll; step(2) → exactly 2 polls
#[test] fn paused_orders_apply_on_resume()          // paused at next 10; issue → exec_tick 11; resume; the TickInput for 11 carries it
#[test] fn visuals_follow_snapshots()               // fake runner with 10 records → 10 UnitView entities after 3 updates
#[test] fn despawned_record_removes_visual()        // a record missing from the next snapshot → its entity is gone; no panic in interpolation
#[test] fn multiple_outputs_per_frame_keep_all_events()   // 3 TickOutputs, each with one Spawned, arrive before one frame → 3 SimEventMsg read that frame
#[test] fn first_frame_without_snapshot_draws_nothing()   // no TickOutput yet → 0 UnitView, no panic
#[test] fn runner_failure_is_surfaced()             // FromRunner::Failed → RunnerStatus.failed is Some, LocalDriver paused
#[test] fn encode_command_uses_string_table()       // tables {"commands": ["core.move", "core.stop"]} → MoveCmd encodes with type_id 0; an unknown name → Err
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client`. Expected: FAIL.
- [ ] **Step 3: Implement.** Check every Bevy API with the `bevy-api` skill.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS, and `arch-check` confirms `fl_client` has no `fl_sim` edge.
- [ ] **Step 6: Commit** `feat(fl_client): ClientFeature, LocalDriver, runner link, snapshot interpolation, placeholder visuals` with trailer `Refs: ADR-0002`.

### Task 21: Debug tools — console, spawn panel, stats, time controls, inspector (rev-1 T14)

**Track:** client · **Refs:** ADR-0008 · **Lane 2**

**Files:**
- Create: `crates/fl_client/src/debug/{mod,console,spawn_panel,stats,inspector}.rs`, `crates/fl_client/tests/debug.rs`

**Interfaces:**
- Consumes: `LocalDriver`, `RunnerLink`, `encode_command`, `send_query`, `PendingReplies`, `SnapshotBuffer`, `LocalPlayer`, `RevealAll`, `ClientContent`, `RunnerStatus` (Task 20); the debug payloads (Task 6).
- Produces:
```rust
pub type ConsoleFn = fn(&mut World, &[&str]) -> Result<String, String>;
#[derive(Resource, Default)] pub struct ConsoleCommands { /* BTreeMap<&'static str, (help, ConsoleFn)> */ }
pub trait ConsoleAppExt { fn register_console_command(&mut self, name: &'static str, help: &'static str, f: ConsoleFn) -> &mut Self; }
#[derive(Resource, Default)] pub struct ConsoleState { pub input: String, pub history: Vec<String>, pub open: bool }
pub fn run_console_line(world: &mut World, line: &str) -> String;   // tokenize, dispatch, else "unknown command: <name>"
#[derive(Resource, Default)] pub struct CursorGround(pub Option<Vec3>);
#[derive(Resource)] pub struct SpawnPanel { pub unit: Option<String>, pub owner: PlayerId, pub placing: bool, pub heading_deg: i32 }
#[derive(Resource, Default)] pub struct InspectorState { pub sim_id: Option<SimId>, pub pending: Option<u64>, pub last: Option<serde_json::Value> }
```
- **Console commands.** Every sim change is a command issued through `LocalDriver`; the console never mutates anything else.

  | Command | Effect |
  |---|---|
  | `help` | lists commands |
  | `spawn <unit_id> <player> <x> <y>` | `debug.spawn` |
  | `wander <player> <radius>` | `debug.wander` on that player's snapshot units |
  | `setowner <sim_id> <player>` | `debug.set_owner` |
  | `invuln <sim_id> on\|off` | `debug.invulnerable` |
  | `sethp <sim_id> <hp>` | `debug.set_hp` |
  | `teleport <sim_id> <x> <y>` | `debug.teleport` |
  | `player <id>` | sets `LocalPlayer` (client-local) |
  | `reveal` | toggles `RevealAll` |
  | `pause`, `resume`, `step [n]`, `speed <0.25..8>` | `LocalDriver` controls |
  | `hash` | `RunnerStatus.last_hash` |
  | `tick` | current tick |
  | `inspect <sim_id>` | sends `SimQuery::Entity`; the reply shows in the inspector window |

- **`DebugToolsPlugin`:**
  - It adds `EguiPlugin` only if absent, plus `FrameTimeDiagnosticsPlugin`.
  - It draws, in `EguiPrimaryContextPass`: the console (backquote or F1), the spawn panel, stats (FPS, tick, last hash, rules hash, unit count, local player, runner status), time controls and the inspector.
  - Click-to-place issues `debug.spawn` at `CursorGround`, unless egui wants the pointer.

- [ ] **Step 1: Write the failing tests** (World level, no egui; a fake runner port):
```rust
#[test] fn console_spawn_issues_debug_spawn()   // "spawn base.de.pz3 0 500 500" → LocalDriver.pending() has 1 command; type is "debug.spawn" in the header table;
                                                //   its payload decodes to DebugSpawnCmd { unit: "base.de.pz3", owner: PlayerId(0), .. }
#[test] fn console_unknown_command()            // "frobnicate" → "unknown command: frobnicate"
#[test] fn console_spawn_bad_args()             // "spawn base.de.pz3 0 x 1" → error text; no command queued
#[test] fn console_debug_cheats_map_to_kinds()  // setowner, invuln, sethp, teleport → kinds "debug.set_owner", "debug.invulnerable", "debug.set_hp", "debug.teleport"
#[test] fn console_player_switch()              // "player 1" → LocalPlayer(PlayerId(1)); nothing queued
#[test] fn console_step_while_paused()          // "pause", then "step 2" → exactly 2 TickInputs reach the fake runner
#[test] fn console_reveal_toggles_render_override()   // RevealAll false → true; nothing queued
#[test] fn console_inspect_sends_query()        // "inspect 3" → the fake runner receives SimQuery::Entity(SimId(3)); its reply lands in InspectorState.last
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test debug`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_client): debug console, spawn panel, stats, time controls and inspector, all via commands, driver or queries` with trailer `Refs: ADR-0008`.

### Task 22: `GameBuilder` and the `fl_game` testbed binary (rev-1 T15)

**Track:** client · **Refs:** ADR-0001, ADR-0002 · **Lane 2**

**Files:**
- Create: `crates/fl_game/src/{lib,builder}.rs`, `crates/fl_game/tests/{builder,wiring}.rs`
- Modify: `crates/fl_game/src/main.rs`, `crates/fl_client/src/lib.rs` (`ClientPlugins` gets the five plugins and exports the new modules)

**Interfaces:**
- Consumes: `fl_catalog`, `fl_sim` (`load_content`, `build_sim_app`, `MatchSetup`, `setup_scenario`, `ScaleScenario`, `SimRunner::spawn_threaded`), `fl_client` (Tasks 20–21), `fl_observe` (Task 19).
- Produces:
```rust
pub enum StartMode { Scenario(String), Stress { units: u32, seed: u64 } }
pub struct GameBuilder { .. }
impl GameBuilder {
    pub fn new() -> Self;                                  // Scenario("base.testbed"), fl_catalog::features(), base mod, cheats on
    pub fn sim_features(self, fs: Vec<Arc<dyn SimFeature>>) -> Self;
    pub fn client_feature(self, f: impl ClientFeature) -> Self;
    pub fn content_mod(self, dir: impl Into<PathBuf>) -> Self;
    pub fn scenario(self, id: &str) -> Self;
    pub fn stress(self, units: u32) -> Self;               // ScaleScenario { units, ..default } with wander
    pub fn exit_after(self, secs: Option<f32>) -> Self;
    pub fn debug_remote(self, port: Option<u16>) -> Self;  // stored; Task 25 consumes it
    pub fn agent_mode(self, on: bool) -> Self;             // continuous update mode when unfocused (benches and agents)
    pub fn prepare(self) -> Result<App, String>;           // everything except run(); content, feature and setup errors as text
    pub fn prepare_headless(self) -> Result<App, String>;  // the same with MinimalPlugins and no window (tests)
    pub fn run(self) -> AppExit;
}
pub fn check_client_features(sim: &[Arc<dyn SimFeature>], client: &[Arc<dyn ClientFeature>]) -> Result<(), String>;
//   "client feature for '<id>' has no SimFeature"
```
- **`prepare`:**
  1. `load_content`.
  2. `check_client_features`.
  3. `MatchSetup` (scenario seed or stress seed; `cheats_allowed = true` for the testbed).
  4. `build_sim_app` with `RunConfig { phase_markers: cfg.profiler, .. }`.
  5. `setup_scenario` or `ScaleScenario::populate`.
  6. `SimRunner::spawn_threaded`.
  7. Build the client `App`:
     - plugins: `DefaultPlugins` (title `"Frontline — testbed"`; `AutoNoVsync` in stress mode; continuous unfocused updates in agent mode), `MeshPickingPlugin`, `ClientPlugins`, then the client features in sorted feature order
     - resources: `RunnerLink`, `LocalDriver::new(runner.next_tick())`, `ClientContent`, `LocalPlayer(PlayerId(0))`, and `MapView` from the scenario's map
  8. Observability: `load_debug_config`; in debug builds, `new_run_dir("logs")` and `set_global_run_dir`. Set `LogPlugin { filter, custom_layer }`, where the custom layer boxes the profiler layer plus the json file layer. A `PostUpdate` system writes every drained `TickOutput.journal` through `JournalSink`.
- **`exit_after`** writes `AppExit::Success` after the duration and logs `stress: units={n} avg_fps={:.1} p1_low_fps={:.1} frames={}`, ignoring the first 3 s.
- **CLI:** `--scenario <ID>`, `--stress <N>`, `--exit-after <SECS>`, `--mods <DIR>...`, `--remote [--remote-port <P>]`, `--agent`.

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn builder_rejects_broken_content()     // content_mod(crates/fl_cli/tests/fixtures/bad_mod).prepare_headless() → Err naming the bad file
#[test] fn orphan_client_feature_is_error()     // a ClientFeature with sim_feature "ghost" → Err("client feature for 'ghost' has no SimFeature")
#[test] fn testbed_visuals_appear_headless()    // prepare_headless() of base.testbed (threaded runner); update until 10 UnitView entities exist (≤ 200 updates, 10 ms sleeps)
#[test] fn debug_spawn_round_trips_through_runner()   // run_console_line("spawn base.de.pz3 0 500 500") → 11 UnitView within 200 updates
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_game`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS. Then `cargo run -p fl_game` opens the window; check by hand that the ground, 10 placeholders and pan/zoom/rotate all work.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_game): GameBuilder wiring the threaded runner to the client; testbed binary with stress and agent modes`.

### Task 23: Bench harness — `fl_cli bench` and criterion (rev-1 T16; tier 3 wall clock)

**Track:** tools · **Refs:** ADR-0010 · **Lane 2**

**Files:**
- Create: `crates/fl_cli/src/bench.rs`, `crates/fl_cli/tests/bench.rs`
- Modify: `crates/fl_cli/src/{lib,main}.rs`, `crates/fl_sim/benches/sim_tick.rs`

**Interfaces:**
- Consumes: `SimRunner`, `ScaleScenario`, `SimQuery::Perf`, `fl_observe::{Profiler, PerfSnapshot}`, `RunConfig { phase_markers: true, .. }`.
- Produces:
```rust
pub struct BenchConfig { pub units: Vec<u32>, pub ticks: u32, pub warmup: u32, pub seed: u64, pub repeat: u32 }
pub struct BenchRow { pub units: u32, pub entities: usize, pub mean_ms: f64, pub p95_ms: f64, pub max_ms: f64,
    pub per_phase_mean_ms: Vec<(String, f64)>, pub counters_per_tick: Vec<(String, f64)> }
pub fn run_bench(cfg: &BenchConfig) -> Vec<BenchRow>;          // per size: the median-of-`repeat` run, by mean_ms; warmup excluded
pub fn median(xs: &[f64]) -> f64;
pub fn profile_json(rows: &[BenchRow], perf: &[PerfSnapshot], meta: &serde_json::Value) -> serde_json::Value;
pub fn render_markdown(rows: &[BenchRow], meta: &str) -> String;
pub fn compare(rows: &[BenchRow], baseline: &[BenchRow], max_regression: f64) -> Result<(), String>;   // mean_ms per size
```
- **CLI:** `fl_cli bench [--units 500,3000,5000] [--ticks 400] [--warmup 40] [--seed 1] [--repeat 5] [--out <md>] [--json <path>] [--baseline <json> --max-regression 0.20]`. It prints the markdown table, and exits 1 on a regression.
- The wall clock here sits behind `#[expect(clippy::disallowed_types, reason = "bench wall clock")]`.
- **Criterion `sim_tick`:** groups `tick/500`, `tick/3000` and `tick/5000`, each iteration one `step_tick` after a 40-tick warmup, on the `core_mod` fixture content (ruling R3).

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn bench_reports_each_size()        // units [50, 100], 20 ticks, repeat 1 → 2 rows; per_phase_mean_ms.len() == 8; mean_ms > 0
#[test] fn median_of_repeats()              // median(&[3., 1., 2., 5., 4.]) == 3.0
#[test] fn compare_flags_regression()       // baseline 1.0 ms at 0.20: 1.3 → Err naming the size; 1.1 → Ok
#[test] fn instrumented_hash_unchanged()    // phase markers and profiler on vs off → equal final state hash
#[test] fn profile_json_has_features_and_phases()   // 100 units → "features" contains "core.movement" with p95_ms > 0; "phases" has 8 keys
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_cli --test bench`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS, and `cargo bench -p fl_sim --bench sim_tick -- --quick` runs.
- [ ] **Step 5: Gate** `cargo xtask gate task --track tools --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_cli): scale bench with median-of-N, per-phase and counter breakdown; criterion tick bench`.

### Task 24: `fl_cli` agent commands with `--json` (rev-1 T20)

**Track:** tools · **Refs:** ADR-0007, ADR-0006 · **Lane 2**

**Files:**
- Create: `crates/fl_cli/src/{agent,inspect,journal_cmd,save_cmd}.rs`, `crates/fl_cli/tests/agent.rs`
- Modify: `crates/fl_cli/src/{lib,main}.rs`

**Interfaces:**
- Consumes: `Replay`, `ReplayRecorder`, `verify_replay`, `first_divergence` (Task 18); `save_world`, `load_world` (Task 13); `fl_observe::{init_headless, new_run_dir, JournalSink}`.
- Produces: a global `--json` flag. With it, every subcommand prints exactly one JSON document on stdout and logs go to stderr.

| command | behaviour |
|---|---|
| `run-scenario <ID\|path.scenario.ron> --ticks N [--seed S] [--trace] [--dump-state-every K] [--out <dir>]` | Runs headless and writes `<run dir>/{log.jsonl, journal.jsonl, run.replay, states.jsonl}`. JSON: `{run_dir, ticks, final_hash, rules_hash, journal_entries, rejected_commands}` |
| `inspect <run.replay\|scenario\|file.save> --tick T [--entity N]` | Re-simulates to tick T; prints `inspect_world`, or `inspect_entity` with `--entity` |
| `journal <run dir\|journal.jsonl> [--kind K] [--sim-id N] [--player P] [--from T] [--to T] [--limit L]` | Filtered entries in tick order |
| `replay-verify <file.replay>` | Exit 0 when Verified. Exit 1 with `{tick, expected, actual}` on divergence. Exit 3 with `"non-reproducible"` |
| `replay dump <file.replay> --json` | `Replay::to_json` |
| `save <ID\|file.replay> --tick T --out <file.save>` | Runs to T and writes `save_world` |
| `load <file.save> --ticks N` | Continues N ticks and prints the final hash |
| `bisect-desync <a> <b>` | Replays or run dirs, re-simulated with hash interval 1. Reports the first divergent tick (the tick whose inputs differ, because hashes are taken after each tick) and the ids whose `inspect_entity` differs there |
| `data-check`, `hash-dump`, `bench` | Gain `--json` |

- [ ] **Step 1: Write the failing tests** (`tests/agent.rs`; they run the binary with `--json` and parse stdout):
```rust
#[test] fn run_scenario_json_outputs_paths()     // the files exist; journal_entries ≥ 1
#[test] fn inspect_entity_at_tick()              // base.testbed, tick 10, entity 1 → JSON with components["core.pose"]
#[test] fn journal_filter_by_kind()              // --kind core.unit_spawned → only that kind
#[test] fn replay_verify_ok_and_tampered()       // ok → exit 0; one command target edited → exit 1 with "tick"
#[test] fn replay_dump_json_lists_inputs()
#[test] fn save_then_load_continues_identically()   // 100-tick final hash == save at 40 + load + 60 ticks
#[test] fn bisect_desync_finds_tick()            // two replays differing by one command at tick 37 → first divergence 37
#[test] fn scenario_file_path_accepted()
#[test] fn non_reproducible_replay_reported()    // a replay with a ContentReload event → exit 3
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_cli --test agent`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track tools`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_cli): agent debugging commands (run, inspect, journal, replay verify/dump, save/load, bisect) with --json`.

### Task 25: Live debug — BRP `fl.*` methods, agent tools, offscreen screenshot, perf panel (rev-1 T21)

**Track:** client · **Refs:** ADR-0008, ADR-0013 · **Lane 2** (after Tasks 22 and 27)

**Files:**
- Create: `crates/fl_client/src/debug/{remote,perf,screenshot,codecs}.rs`, `crates/fl_client/tests/remote.rs`, `crates/fl_game/tests/remote.rs`
- Modify: `crates/fl_client/src/debug/mod.rs`, `crates/fl_game/src/builder.rs`

**Interfaces:**
- Consumes: Task 27's probe findings (method registration, how to block for a runner reply, offscreen `Screenshot::image`, `register_agent_tool`); `RunnerLink`, `LocalDriver`, `PendingReplies`; `fl_observe::global_profiler`.
- Produces:
```rust
#[derive(Resource, Default)] pub struct ClientCommandCodecs { /* name → fn(serde_json::Value) -> Result<Vec<u8>, String> (postcard) */ }
pub trait CommandCodecAppExt { fn register_command_json<P: CommandPayload>(&mut self) -> &mut Self; }   // fl_client registers the core and debug payloads; feature clients theirs
#[derive(Resource)] pub struct ScenarioLoader(pub Box<dyn Fn(LoadRequest) -> Result<LoadedMatch, String> + Send + Sync>);   // inserted by fl_game
pub enum LoadRequest { Scenario(String), Save(Vec<u8>) }
pub struct LoadedMatch { pub link: RunnerLink, pub next: Tick, pub pending: Vec<PlayerCommand>, pub last_seq: BTreeMap<PlayerId, u32> }
pub struct RemoteDebugPlugin { pub port: u16 }   // cfg(feature = "debug_remote"); default 15702
pub struct OverlayRegistry;                      // name → toggle; "grid" (F3) in M0
```
- **Methods.** Params and results are JSON. Reads go through `SimQuery`; writes go through commands or the driver.

  | Method | Behaviour |
  |---|---|
  | `fl.sim_state` | `WorldSummary` + `{tick, last_hash, paused, speed}` |
  | `fl.entity {sim_id}` | `SimQuery::Entity` |
  | `fl.hash` | the last hash |
  | `fl.issue_command {player, units, command, payload}` | `ClientCommandCodecs` → `LocalDriver::issue`; returns `{exec_tick, seq}` |
  | `fl.pause`, `fl.resume` | driver controls |
  | `fl.step {n}` | steps; returns the new tick once reached |
  | `fl.set_speed {x}` | driver speed |
  | `fl.spawn {unit, player, x, y}` | issues `debug.spawn` |
  | `fl.journal_since {tick, limit}` | `SimQuery::JournalSince` |
  | `fl.perf` | profiler snapshot + `SimQuery::Perf` + frame stats |
  | `fl.screenshot {path}` | dedicated offscreen camera into an `Image`, captured with `Screenshot::image` and saved |
  | `fl.toggle_overlay {name}` | overlay toggle |
  | `fl.load_scenario {id}`, `fl.load {path}` | `ScenarioLoader`; swaps `RunnerLink` and restores `LocalDriver` |
  | `fl.save_scenario {path}` | writes the current snapshot's units as a `*.scenario.ron` that `fl_content` loads |
  | `fl.save {path}` | `SimQuery::Save { pending: driver.pending() }` |

  A method that needs a runner reply blocks for at most 2 s. Any `TickOutput`s that arrive meanwhile are queued for `drive_runner`, never dropped.
- Every `fl.*` method is published with a description through `bevy_brp_extras` as an agent tool.
- `world.*` stays client-only and is never wired to sim state.
- **Perf panel** (F2): a table of features, phases and systems (p50/p95/max) with a sparkline per row, FPS and frame-time graphs, an entity count and render stats.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_client/tests/remote.rs  (#![cfg(feature = "debug_remote")]; MinimalPlugins + fake runner)
#[test] fn brp_issue_command_stamps_and_queues()   // JSON-RPC fl.issue_command {player:0, units:[1], command:"core.move", payload:{"target":[200,100]}} → {exec_tick, seq: 0}
#[test] fn brp_unknown_command_is_error()          // command "core.fly" → JSON-RPC error naming it
// crates/fl_game/tests/remote.rs  (#![cfg(feature = "debug_remote")]; GameBuilder::prepare_headless with remote on a free port; real threaded runner)
#[test] fn brp_step_state_command()                // fl.step {n:5} → tick ≥ 5; fl.sim_state tick ≥ 5; fl.issue_command core.move for unit 1;
                                                   //   fl.step {n:40}; fl.entity {sim_id:1} → position changed
#[test] fn gpu_screenshot_offscreen()              // full DefaultPlugins with the window hidden; fl.screenshot {path} → PNG exists, larger than 1 KB, not all black
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client -p fl_game --features fl_game/debug_remote -E 'test(/brp_/)'`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the step 2 command. Expected: PASS. Then run `cargo nextest run -p fl_game --features debug_remote -E 'test(/gpu_/)'` on the dev GPU. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`, then `cargo xtask gate nightly`. Expected: `PASS brp`, `PASS gpu` and `gate nightly: PASS`.
- [ ] **Step 6: Commit** `feat(fl_client): BRP fl.* methods via runner queries and commands, agent tools, offscreen screenshot, perf panel` with trailer `Refs: ADR-0008`.

---

## Phase III (continued) — probes, contract snapshots, perf gates

### Task 26: Probe — `bevy/dynamic_linking` with `bevy_ecs`-only sim crates

**Track:** probes · **Refs:** ADR-0013 (§13.5) · **Lane 2** (`C:\fl\t26`, cut after Task 1)

**Files:**
- Modify: `probes/dynlink/src/main.rs`; the section "Probe: dynamic_linking" in `docs/research/bevy020-api-reference.md`
- Modify `crates/fl_game/Cargo.toml` only on success: add the feature `dynamic = ["bevy/dynamic_linking"]`

**Interfaces:**
- Consumes: the `fl_sim` API as it stands on the probe's base commit (rev-1 `build_sim_app` plus `run_schedule(SimTick)` is fine; the probe is throwaway).
- Produces: a recorded decision on whether dev builds may use dynamic linking.

- [ ] **Step 1: Write the probe.** `main` builds a sim `App` from the `core_mod` fixture (by path) and steps it 10 ticks. It also builds a `bevy` `App` with `MinimalPlugins` and one system, runs it 10 updates, and prints `dynlink probe: OK sim_ticks=10 app_updates=10`.
- [ ] **Step 2: Run** `cargo run -p probe_dynlink` (static). Expected: the OK line.
- [ ] **Step 3: Run** `cargo run -p probe_dynlink --features dynamic`. Expected: either the OK line, or a duplicate-crate, link or type-mismatch error. Record the exact error text.
- [ ] **Step 4: Record** the result, the commands and the decision in the API reference:
  - On success: dev runs may use `cargo run -p fl_game --features dynamic`. Add the feature line.
  - On failure: dynamic linking is not used. Add nothing to `fl_game`.

  The controller adds one line about it to `CLAUDE.md` when it merges.
- [ ] **Step 5: Gate** `cargo xtask gate task --track probes`. Use the interim gate if Task 2 has not merged.
- [ ] **Step 6: Commit** `chore(probes): dynamic_linking with bevy_ecs-only sim crates — <result>`.

### Task 27: Probe — `bevy_remote` custom methods, a runner-thread round trip, offscreen screenshot

**Track:** probes · **Refs:** ADR-0008, ADR-0013 · **Lane 2** (after Task 4)

**Files:**
- Modify: `probes/remote/src/main.rs`; the section "Probe: bevy_remote" in `docs/research/bevy020-api-reference.md`

**Interfaces:**
- Produces findings that Task 25 consumes:
  - the 0.20 names for adding the remote and HTTP plugins on a chosen port and for registering a custom method (instant vs watching)
  - whether a method may block up to 2 s waiting on a crossbeam reply from another thread without stalling rendering
  - the `bevy_brp_extras` `register_agent_tool` signature and the `brp_extras/*` listing method
  - the offscreen camera → `Image` → `Screenshot::image(handle)` → `save_to_disk` recipe that works with the window hidden or unfocused
  - how to set continuous update mode when the window is unfocused

- [ ] **Step 1: Write the probe.**
  - App: `DefaultPlugins` with the window hidden, plus the remote and HTTP plugins on port 15799.
  - A worker thread stands in for the runner: it echoes `{n}` → `{n + 1}` over crossbeam.
  - A custom method `fl.ping {n}` that blocks on the worker's reply, and one agent tool registered for it.
  - An offscreen camera rendering a lit cube into an `Image`.
  - A self-test thread posts JSON-RPC over `std::net::TcpStream`: `fl.ping {n: 41}` must return `42`; `rpc.discover` must list `fl.ping`; the agent-tool listing must list `fl.ping`. It then requests a screenshot of the offscreen image to `target/probe-remote.png`.
  - On success it prints `remote probe: OK ping=42 screenshot=<bytes>` and exits 0.
- [ ] **Step 2: Run** `cargo run -p probe_remote` (dev GPU) twice: once with the window focused, once with it behind another window. Expected: the OK line both times, and a non-black PNG of more than 1 KB.
- [ ] **Step 3: Record** every finding above, with the API names exactly as compiled, in the API reference.
- [ ] **Step 4: Gate** `cargo xtask gate task --track probes`. Expected: PASS.
- [ ] **Step 5: Commit** `chore(probes): bevy_remote custom methods, runner round trip, offscreen screenshot`.

### Task 28: Contract snapshots — public API, serde formats, `SimTick` system order

**Track:** contract + sim-core · **Refs:** ADR-0011, ADR-0005, ADR-0006 · **Lane 1**

**Files:**
- Create:
  - `xtask/src/api.rs`, `xtask/api-toolchain.txt` (the installed dated nightly)
  - blessed `xtask/snapshots/api/{fl_math,fl_protocol,fl_view,fl_sim}.txt`
  - `crates/{fl_protocol,fl_view,fl_sim}/tests/formats.rs` with blessed snapshots
  - `crates/fl_test_utils/tests/system_order.rs` with a blessed snapshot
- Modify: `xtask/src/{main,gate}.rs` (subcommand `api`, gate row `api`), `crates/fl_sim/src/classify.rs` (each `Codec` gains a serde-reflection `trace` fn), `crates/fl_sim/src/lib.rs`

**Interfaces:**
- Produces:
```rust
// xtask/src/api.rs
pub const API_CRATES: &[&str] = &["fl_math", "fl_protocol", "fl_view", "fl_sim"];
pub fn public_api(root: &Path, krate: &str, toolchain: &str) -> anyhow::Result<String>;   // cargo-public-api, simplified output, with the pinned toolchain
pub fn compare(current: &str, committed: Option<&str>) -> ApiStatus;
pub enum ApiStatus { Same, Changed, New }
// CLI: cargo xtask api [--check] — writes xtask/snapshots/api/<crate>.txt.new for every Changed/New crate; --check fails if any exist
// gate row "api": task (only if a touched package is in API_CRATES, else Skip), pr, milestone
// fl_sim
pub fn codec_formats(app: &App) -> String;    // one block per Hashed codec, in name order: "<name>@<version>:\n<serde-reflection format>"
```
- **Format guard.** Each `formats.rs` snapshots the traced formats as text whose first line names its version: `FORMAT_VERSION = n` for protocol, `VIEW_FORMAT_VERSION = n` for view, `SAVE_FORMAT_VERSION = n` plus per-codec `@version` for sim. A small guard in the same test file reads the committed `.snap`. If a type's format text differs while its version is unchanged, it panics with `wire format of <Type> changed without a version bump` (or `<name>: format changed without a version bump` per codec). The plain insta diff then needs a bless.
- **Traced types:**
  - `fl_protocol`: `PlayerCommand`, `CommandKind`, `MatchHeader`, `StringTables` and the eight payloads.
  - `fl_view`: `SimSnapshot`, `EntityRecord`, `SimEvent`, `TickInput`.
  - `fl_sim`: `SaveFile` plus every core codec.
- **System order:** `system_order__sim_tick.snap` holds `system_order` for `fl_catalog::features()`.

- [ ] **Step 1: Write the failing tests:**
```rust
// xtask/src/api.rs
#[test] fn compare_detects_change()             // compare("a\nb", Some("a")) == Changed; compare("a", Some("a")) == Same; compare("a", None) == New
// crates/fl_protocol/tests/formats.rs
#[test] fn protocol_formats()                   // insta snapshot "protocol_formats"
#[test] fn format_guard_flags_unbumped_change() // guard(committed "FORMAT_VERSION = 1\nMoveCmd: A", current "FORMAT_VERSION = 1\nMoveCmd: B") → Err naming MoveCmd;
                                                //   with "= 2" in current → Ok
// crates/fl_view/tests/formats.rs
#[test] fn view_formats()
// crates/fl_sim/tests/formats.rs
#[test] fn save_codec_formats()                 // contains "core.pose@1:" and "core.match_seed@1:"
// crates/fl_test_utils/tests/system_order.rs
#[test] fn sim_tick_system_order()
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p xtask -p fl_protocol -p fl_view -p fl_sim -p fl_test_utils -E 'test(/formats|system_order|compare_detects/)'`. Expected: FAIL. insta writes `*.snap.new`, and the guard tests fail on missing items.
- [ ] **Step 3: Implement.** Run `cargo xtask api` to write the `.txt.new` files.
- [ ] **Step 4: Bless** (lead): `cargo xtask bless xtask/snapshots crates/fl_protocol/tests/snapshots crates/fl_view/tests/snapshots crates/fl_sim/tests/snapshots crates/fl_test_utils/tests/snapshots --reason "M0 contract baseline"`.
- [ ] **Step 5: Run** the step 2 command and `cargo xtask api --check`. Expected: PASS and `api: 4 crates unchanged`.
- [ ] **Step 6: Gate** `cargo xtask gate pr`. Expected: `PASS api` and `gate pr: PASS`.
- [ ] **Step 7: Commit** `test(contract): public API, serde format and SimTick system-order snapshots` with trailers `Refs: ADR-0011` and `Blessed: M0 contract baseline`.

### Task 29: Work-counter perf gates — entity-mix scale scenario, counting allocator, budgets

**Track:** sim-core + perf · **Refs:** ADR-0010 · **Lane 1**

**Files:**
- Create:
  - `crates/fl_test_utils/src/perf.rs`, `crates/fl_test_utils/tests/perf_counters.rs`, `crates/fl_sim/tests/scale_mix.rs`
  - blessed: `docs/perf/budgets.ron` and `crates/fl_test_utils/tests/snapshots/perf_counters__*.snap`
- Modify: `crates/fl_sim/src/{scale,step,spawn,lib}.rs` (the mix; counter `sort.items` where the inbox and despawn requests are sorted), `crates/fl_test_utils/src/lib.rs`

**Interfaces:**
- Consumes: `SpawnRequest` (`unit: None` plus `init`), `DespawnRequest`, `SpatialGrid::query_radius`, `SimCounters`, `rolls`.
- Produces:
```rust
// fl_sim/src/scale.rs
pub struct ScaleScenario { pub units: u32, pub players: u8, pub map_m: i32, pub wander_m: i32, pub seed: u64,
    pub followers_per_unit: u32, pub projectiles: u32, pub projectile_ttl: u32, pub sensor_every: u32 }   // + Serialize/Deserialize (Task 18)
impl Default for ScaleScenario { /* units 3000, players 4, map 2000, wander 150, seed 1, followers 2, projectiles 1000, ttl 20, sensor_every 10 */ }
impl ScaleScenario { pub fn with_units(units: u32) -> Self; }   // the same mix scaled: followers 2 per unit, projectiles = units / 3
#[derive(Component, Clone, Copy, Serialize, Deserialize)] pub struct Follower { pub leader: SimId, pub offset: FxVec2 }   // Hashed "core.follower"
#[derive(Component, Clone, Copy, Serialize, Deserialize)] pub struct Projectile { pub vel: FxVec2, pub ttl: u16 }        // Hashed "core.projectile" (stand-in until M2)
#[derive(Resource, Clone)] pub struct ScaleMix { .. }    // External "core.scale_mix"; present only in scale runs
// follow_system   (ActSet::Move, .after(move_system)): pos = map.clamp(leader pos + offset)
// scale_fire      (ActSet::Fire): ceil(projectiles / ttl) SpawnRequests per tick; shooters picked by rolls(seed, tick, SimId(0), stream_key("scale.fire"))
// projectile_step (ResolveSet::Projectiles): pos += vel; ttl -= 1; at 0 → DespawnRequest
// scale_sense     (PerceiveSet::Vision): units with sim_id % sensor_every == tick % sensor_every run query_radius(pos, 150 m) → "spatial.query_candidates"
// fl_test_utils/src/perf.rs
#[derive(Deserialize, Serialize)] pub struct BudgetFile { pub scenarios: BTreeMap<String, BTreeMap<String, u64>> }   // max per tick
pub fn load_budgets(path: &Path) -> BudgetFile;
pub struct CounterRun { pub per_tick_max: BTreeMap<String, u64>, pub totals: BTreeMap<String, u64>, pub entities: usize }
pub fn run_counter_scenario(units: u32, warmup: u32, ticks: u32, alloc: Option<&'static stats_alloc::StatsAlloc<std::alloc::System>>) -> CounterRun;
//   adds "alloc.count" and "alloc.bytes" per tick when `alloc` is given
pub fn check_budgets(scenario: &str, run: &CounterRun, budgets: &BudgetFile) -> Result<(), Vec<String>>;   // "scale_500: spawn.entities max 31 > budget 25"
pub fn record_budgets(scenario: &str, run: &CounterRun, path: &Path);   // writes <path>.new with ceil(max × 1.25)
```
- The counting allocator lives in `tests/perf_counters.rs` as `#[global_allocator] static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;`. That is safe code, so the workspace keeps `unsafe_code = "forbid"`.
- `stats_alloc` must pass `cargo deny`.
- Alloc counts are budgeted, never snapshotted.
- With `FL_RECORD_BUDGETS=1`, the tests write `docs/perf/budgets.ron.new` for the lead to bless.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_sim/tests/scale_mix.rs
#[test] fn scale_mix_entity_count()        // with_units(3000) after a 40-tick warmup: SimIndex.len() in [8_000, 12_000]
#[test] fn projectiles_steady_state()      // with_units(3000): projectiles in flight after warmup within ±10% of 1000
#[test] fn followers_track_leaders()       // with_units(100): every tick, follower pos == map.clamp(leader pos + offset)
#[test] fn scale_mix_twin_run()            // with_units(500), 200 ticks: equal hash streams
// crates/fl_test_utils/tests/perf_counters.rs
#[test] fn smoke_counters_scale_500()      // warmup 20, 200 ticks: check_budgets("scale_500") Ok; insta snapshot of totals without alloc.*
#[test] fn counters_scale_3000()           // the same at 3000 (gate pr)
#[test] fn milestone_counters_scale_5000() // the same at 5000 (gate milestone)
#[test] fn budget_violation_names_counter()   // synthetic run with spawn.entities max 31 vs budget 25 → Err(["scale_500: spawn.entities max 31 > budget 25"])
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test scale_mix`. Expected: FAIL.
- [ ] **Step 3: Implement.** Then run `FL_RECORD_BUDGETS=1 cargo nextest run --profile agent -p fl_test_utils --test perf_counters -E 'test(/scale/)'` and ask the lead to bless `docs/perf/budgets.ron` and the snapshots, with the reason `M0 counter baseline (entity mix)`.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS. Also run a temporary O(n²) test system (every unit queries all units each tick) and expect `smoke_counters_scale_500` to fail naming `spatial.query_candidates`. Remove the system afterwards.
- [ ] **Step 5: Gate** `cargo xtask gate task --track sim-core --track perf`. Expected: `smoke_counters_scale_500` runs, and `gate task: PASS`.
- [ ] **Step 6: Commit** `feat(perf): entity-mix scale scenario, work-counter budgets and counting allocator` with trailers `Refs: ADR-0010` and `Blessed: M0 counter baseline (entity mix)`.

---

## Phase IV — integration (lane 1)

### Task 30: Integration — demo, determinism, perf baseline, render stress, debugging guide (rev-1 T17)

**Track:** docs + perf · **Refs:** ADR-0009, ADR-0010

**Files:**
- Create: `docs/debugging.md`, `docs/m0-demo-checklist.md`, `docs/perf/M0.md`, `docs/perf/M0.json`
- Modify: `README.md`

- [ ] **Step 1: Merge and gate.** With every branch merged into `m0/phase-a`, run `cargo xtask gate pr`. Expected: `gate pr: PASS`.
- [ ] **Step 2: Write `docs/debugging.md`** for humans and agents. It covers:
  - launching in debug mode: `--remote`, `--remote-port`, `--agent`, `debug.ron`, `RUST_LOG`, and the `trace`/`trace_chrome`/`trace_tracy` features
  - picking a BRP port per agent
  - every `fl_cli` command with a `--json` example
  - every `fl.*` method with example request and response JSON
  - where logs, journals, replays and saves are written
  - the reproduce-a-bug recipe: seed + scenario → `run-scenario --trace` → `replay-verify` → `bisect-desync` → `inspect --entity` → `journal --sim-id`
  - the dynamic-linking result (Task 26)

  Run every documented command and check its output matches the document.
- [ ] **Step 3: Content and determinism.**
  - `cargo run -p fl_cli --release -- data-check assets/base` → exit 0, 10 units, rules hash printed.
  - `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_sim -p fl_test_utils` → PASS.
  - `cargo run -p fl_cli -- replay-verify crates/fl_test_utils/tests/replays/testbed_wander.replay` → exit 0.
- [ ] **Step 4: Perf baseline (tier 3).** Ask the user to confirm the fixed power plan first (a system setting). Run `cargo run -p fl_cli --release -- bench --units 500,3000,5000 --ticks 400 --warmup 40 --repeat 5 --out docs/perf/M0.md --json docs/perf/M0.json`.
  - Expected: three rows, with entity counts of about 1.7k, 10k and 17k.
  - Expected: a 3000-unit mean of **≤ 15 ms** (G7, entity mix).
  - Add the machine, rustc version, commit and power plan to the files.
  - If the 3000-unit mean is over 15 ms, profile with `--features fl_game/trace_chrome` (or the `perf-analyst` agent on the Tracy CSV) and fix before closing M0.
- [ ] **Step 5: Render stress test.** Run `cargo run -p fl_game --release -- --stress 3000 --exit-after 25 --agent`.
  - Expected: `stress: units=3000 avg_fps=…` with an average of at least 60. Append the line to `docs/perf/M0.md`.
  - If it is below 60, check shared mesh and material handles, change-only snapshot records, and that only on-screen visuals update their transforms. Fix before closing M0.
- [ ] **Step 6: BRP smoke through `bevy_brp_mcp`.** Launch `cargo run -p fl_game --features debug_remote -- --remote --remote-port 15702 --agent`. Through the MCP, poll `fl.sim_state` until tick ≥ 1, then call `fl.spawn`, `fl.step {n: 20}`, `fl.entity` and `fl.screenshot`. Record the results in the checklist.
- [ ] **Step 7: Demo checklist.** Write `docs/m0-demo-checklist.md` and walk it with `cargo run -p fl_game --release`:
  - Rev-1 Task 17 Step 6's items: the iso view, 10 placeholders, camera controls clamped at the edge, console `spawn`, click-to-place, `wander 0 100` with smooth interpolation, and the stats panel.
  - The `setowner`/`invuln`/`sethp`/`teleport` console cheats work, and are rejected in a match started with `cheats_allowed = false`.
  - Orders given while paused apply on resume; speed ×0.25–×8; single step.
  - `reveal` toggles; `player 1` switches the controlled player.
  - The `inspect` window shows components.
  - The F2 perf panel lists phases and features.
  - `fl.save` then `fl.load` resumes at the same tick and hash.
  - **ADR confirmations:** one row per Confirmation bullet of ADR-0001…0013, naming the passing test or gate step, or `deferred: <reason>` (D1 cross-OS, D2 instruction counts).
- [ ] **Step 8: Milestone gate.** Run `cargo xtask gate milestone --milestone M0`. Expected: `gate milestone: PASS`.
- [ ] **Step 9: Review and commit.** Run the final whole-branch review (`superpowers:requesting-code-review`), fix the findings, and re-run `cargo xtask gate pr`. Commit `docs: M0 debugging guide, demo checklist and perf baseline`. The controller fast-forwards `main`.

---

## Roadmap M1–M9 (milestone level; each gets its own detailed plan just before it starts)

**Definition of done for every milestone:**
- **Gate:** `cargo xtask gate milestone --milestone M<n>` is green. That covers the pr checks, nightly GPU and BRP tests, the `milestone_*` tests and the perf file.
- **Demo:** `docs/m<n>-demo-checklist.md` is walked in the testbed build.
- **Determinism:**
  - Every new scenario test runs twin and under 3 shuffle seeds.
  - A save round trip and a branch test run on a scenario that uses the new mechanics.
  - At least one golden replay per mechanic family. From M2, versioned and re-blessed under C8.
- **Perf:**
  - Each new algorithm gets work counters and budgets in `docs/perf/budgets.ron` (blessed).
  - `docs/perf/M<n>.json` holds the median-of-5 bench, within 20% of `M<n-1>` or with a written justification.
  - G7 holds: ≤ 15 ms at 3,000 units in the entity mix, and ≥ 60 fps.
- **Observability:** each new mechanic has its journal record kinds, log targets, profiler spans (manual spans in hot loops) and counters. `docs/debugging.md` documents every new CLI command, BRP method and journal kind.
- **Contracts:**
  - Every new component and resource is classified, and Hashed ones have codecs.
  - Format snapshots are bumped when formats change.
  - New contracts come with an ADR and snapshot updates.
  - Each feature is two crates plus one line in `fl_catalog` (and the `fl_game` client list), with an `OWNERS.toml` `feature:<name>` track.
- **From M2:** the held-out suite (C3), the statistical policy (C4), mutation testing (C9) and risk-tiered review (C10) apply.

### M2 entry requirements (Group C, an "M2 prelude" plan after M1)

| Item | Deliverable | Verified by |
|---|---|---|
| C1 mechanic specs | `docs/design/GDD.md` index; `docs/design/mechanics/<area>.md` with EARS requirements `AREA.NAME.N`, formulas, parameter tables with RON paths, acceptance criteria | `cargo xtask spec-lint`; `cargo xtask trace` maps `@req` tags in tests to requirement IDs, and untraced requirements fail |
| C2 provenance ledger | `assets/base/provenance/*.prov.ron` (Original, Manual, ResearchDerived, OursScaled, Guess, Tuned); a generated tuning-tables doc | `data-check` rejects missing or unknown provenance |
| C3 test-author agent | the `test-author` role is activated; protected acceptance tests from the spec alone; a held-out 10–20% under `tests/holdout/` | the gate runs held-out tests at milestone and nightly only; the hooks deny implementers |
| C4 statistical policy | `assert_rate_within_wilson` (α = 1e-4) in `fl_test_utils`; formula tier at 10⁵ draws, scenario tier at 200–500 seeds; nightly seed rotation; one reroll allowed | `test-guard` flags `ALPHA` and case-count changes |
| C5 core vocabulary | conditions (granted tokens + `requires`), a modifier/stat spec with a deterministic stacking order and cached effective stats (ADR), tag bitsets plus class expressions, effect and trigger verb registries; command-panel buttons gain `requires` | an ADR plus registry tests; all registries are stable-string |
| C6 content load phases | discover → parse → migrate (`format_version`) → patch (Set, Merge, Remove, Add, AddTag, If) → inherit → typed deserialize → post-process hooks → link → freeze; provenance per path | loader tests per phase; deprecated keys warn in `data-check` |
| C7 behaviour and entity graph | an order queue plus activity stack; state as table-stored enums; custom `SimId` relationships (squad, garrison, transport, towing, crew, ownership); the event-model ADR (Messages for gameplay, hooks for invariants, observers only for order-independent reactions) | ADR; save codecs for relationships; branch test |
| C8 golden replay versioning | the `SIM_VERSION` bump policy, semantic assertions next to hashes, `fl_cli rebless --reason MCR-n` with a report, change-class commit trailers (refactors stay bit-identical) | golden replays in `gate pr`; re-bless needs the user |
| C9 mutation testing | `cargo-mutants --in-diff` per task (time-capped) and per merge; a full sharded run nightly; missed-mutant triage at milestones | gate rows at task (capped), pr, nightly and milestone |
| C10 risk-tiered review | tiers by touched paths (contract or determinism → two reviews; leaf feature code → one); a per-mechanic mini-gate | the reviewer agent checklist |
| C11 render scale baseline | one material per unit type, per-instance data via `MeshTag`/`GpuComponentArrayBuffer`, `VisibilityRange` LOD | the render stress test (3,000 units at ≥ 60 fps) re-run, with per-instance data |

### Milestones

| M | Scope (spec §9.1) | Where | New observability (journal kinds · counters · spans) | Parallel tracks |
|---|---|---|---|---|
| **M1 Control & movement** | **Terrain:** height and terrain PNG layers, roads, forest, river, bridges; the speed formula; movement-class blocking and clearance. **Pathfinding:** HPA* (16×16 clusters) with a per-tick budgeted request queue, results applied in request order; flow fields for group moves; steering and separation; hull rotation. **Selection:** click, box, Shift-add, double-click, control groups. **Commands:** `core.rotate`, `core.follow`, Shift-queue. **Overlays:** paths, passability, cells. **Hot reload** per ADR-0012 (`ContentDb::same_id_sets`; value-only reloads in place; ID-set changes rebuild through save/load; `ContentReload` replay events). **AI seam:** `PlayerView` stub and a do-nothing `Controller::Ai` | `fl_sim::{map, path, movement}`, `fl_content` terrain kind, `fl_client::{selection, input, overlays}` | `core.path_request`, `core.path_result`, `core.order_changed`, `core.content_reload` · `path.nodes_expanded`, `path.requests`, `path.queue_len`, `flowfield.cells` · spans around HPA* search and flow-field build | pathfinding ∥ terrain mesh and overlays ∥ selection and input ∥ hot reload |
| **M2 Direct-fire combat** (after the prelude) | Weapon and shell kinds; turrets; the firing cycle; dispersion-ellipse hit test; armour facing; penetration; damage; HE splash; ammo; track breaks; wrecks. Target acquisition staggered by `SimId % N` in a pure parallel phase with ordered apply. `core.attack`, `core.attack_ground`, `core.swarm`, `core.stand_ground`. Range and armour overlays; combat log from the journal; tracer and impact VFX; sounds (`bevy_audio` features added; any download needs the user's OK). Executor parity before any multi-threaded executor | `fl_sim::combat`, `fl_client::{vfx, audio, debug}` | `combat.shot` (every roll), `combat.hit`, `combat.penetration`, `combat.splash`, `combat.track_break`, `core.death` · `combat.shots`, `combat.target_scans`, `spatial.query_candidates` · spans in target scoring and the hit test | ballistics and penetration ∥ targeting ∥ VFX and audio ∥ overlays |
| **M3 Vision** | Per-team ref-counted visibility grids, shadowcasting templates, height bonus, time-sliced updates. Camouflage and reveal-by-fire, ghosts and forgetting. `visible_to` bits and a grid-delta view channel. Fog as a WESL material extension over an `R8Unorm` texture. `PlayerView` filled for the AI | `fl_sim::vision`, `fl_client::fog` | `vision.spotted`, `vision.lost`, `vision.revealed_by_fire` · `vision.los_cells`, `vision.sources_updated`, `vision.queue_len` · spans in shadowcasting | LOS and visibility ∥ fog shader ∥ ghosts |
| **M4 Infantry & buildings** | `fl_feat_infantry` + `_client`: squads, formations, prone and suppression, cover, grenades, garrisoning buildings and trenches, building damage and destruction, enter/leave, transports. The building object kind (extend the rules hash, R13). Relationships as `SimId` (C7) | `features/infantry/{sim,client}` | `infantry.prone`, `infantry.suppressed`, `infantry.garrison`, `infantry.grenade` · `infantry.suppression_checks`, `infantry.formation_slots` · spans in formation layout | squads ∥ buildings and garrison ∥ transports |
| **M5 Artillery, supply, morale** | `fl_feat_artillery`: indirect fire with spotting, minimum range, ranging, counter-battery circles, crews, capturable guns. `fl_feat_supply`: generic resources, depots, trucks, resupply, re-manning, morale | `features/{artillery,supply}/{sim,client}` | `artillery.fire`, `artillery.ranging`, `artillery.counter_battery`, `supply.resupply`, `supply.morale_change` · `artillery.spotting_checks`, `supply.depot_scans` · spans in depot chain resolution | artillery ∥ supply |
| **M6 HUD & art** | A QWER/ASDF/ZXCV command panel built from feature buttons (with `requires`), info panel, 4 status bars, minimap with fog, icons, OFL fonts. The first CC0/CC-BY models through `VisualDef.model` + `WorldAssetRoot`; credits screen (downloads need the user's OK) | `fl_client::ui`, `assets/` | client spans for HUD and minimap; render stats in the perf panel | HUD ∥ minimap ∥ model pipeline |
| **M7 Engineering, towing, entrenchment** | `fl_feat_engineering`: AP/AT mines, wire, trenches, AT obstacles, repairs, bridges (dynamic obstacles invalidate only the HPA* clusters they touch). `fl_feat_towing`: towing force vs weight, hook/unhook, deploy/pack. Entrenchment | two feature crate pairs | `engineering.mine_laid`, `engineering.mine_triggered`, `engineering.repair`, `towing.hook` · `path.clusters_invalidated` | engineering ∥ towing |
| **M8 Air support & AA** | `fl_feat_air_support`: recon, bombers, attack aircraft, fighters, paratroopers from the map edge with cooldowns; AA ceiling | feature crate pair | `air.call_in`, `air.strike`, `aa.fire` · `air.active`, `aa.target_scans` | aircraft ∥ AA |
| **M9 Experience & specials** | `fl_feat_experience` (4 ranks as a C5 modifier source), snipers, officers with binoculars, smoke, `fl_feat_weather`, medics, storming buildings | one feature crate pair each | one record family and one counter set per feature | one track per feature |

- **Roster growth:** each milestone adds the unit types its mechanic exercises, up to spec §9.1's iteration-1 roster per faction. Aircraft arrive in M8.
- **Group D (spec §9.3)** is started only when its trigger fires: AI controllers, the lockstep net crate (`NetSession` already exists), desync tooling, remote and 3-OS CI, the campaign layer, economy core, VAT infantry, `.bsn` prefabs, `bevy_lint`, scripting, F8 bug bundles, `fl_cli sweep`, experiment flags, a feedback ledger, projectile pools (the M0 counters `spawn.entities`/`despawn.entities` measure the churn that triggers it) and update skipping.
- **User decisions D1/D2** (deferred): determinism is verified on Windows x64 only, and there is no instruction-count tier. Revisit both at the net-crate milestone at the latest.

---

## Builder's calls and deviations recorded here

- **Build dir:** per-worktree `build.build-dir` in the **repo** config (ruling R19), not the user config that spec §3/§13.3 and ADR-0013 describe. Task 1 probes it and has a fallback.
- **Command delivery:** drivers deliver each command at exactly its `exec_tick`, and the sim rejects `Early` as well as `Late`. Commands that are stamped but not yet due live in the driver. Saves get them through `SimQuery::Save { pending }`, and `CommandStamper::restore` resumes the per-player `seq` from `LastSeq`.
- **Names in payloads:** command payloads carry content **names** (`DebugSpawnCmd.unit: String`), never dense IDs, because payload bytes can't be remapped through string tables.
- **Typed classes:** classification is `Class<T>`, with `Class::hashed(version)` building a typed `Codec<T>`. Spec §5.8's `Class::Hashed { version: 1 }` example becomes `Class::hashed(1)`.
- **No clock in the sim:** phase timing comes from `fl_sim::phase` trace events and the tick from a `sim_tick` span, both timed by `fl_observe`. The rev-1 `SetTimings` and `current_tick()` static are gone.
- **Runner thread:** the only thread spawn in a sim crate is `SimRunner::spawn_threaded`, behind a scoped `#[expect]`.
- **Channels:** `crossbeam-channel`, because its receivers are `Sync`, which Bevy resources need (spec §14 left this open).
- **Counting allocator:** `stats_alloc`, a safe `#[global_allocator]` declaration, so the workspace keeps `unsafe_code = "forbid"`.
- **Protected artifacts:** tests write `*.new` files and only `cargo xtask bless` promotes them. Golden replays use `FL_RECORD_GOLDEN=1`; budgets use `FL_RECORD_BUDGETS=1`.
- **Test-name conventions:** `smoke_`, `gpu_`, `milestone_` and `brp_` select the gate subsets.
- **Guard hooks:** both `settings.json` (auto role) and the implementer agents' frontmatter (explicit role) carry the guards, because hook input may not identify the subagent (verified in Task 4).
- **`GameBuilder`** moves from rev-1's `fl_client/src/game.rs` to `fl_game`, because only `fl_game` may depend on both halves. Loading a scenario or save at runtime goes through a `ScenarioLoader` callback that `fl_game` inserts.
- **`test-guard`'s "falling `nextest list` count"** is a textual proxy (net removed `#[test]`/proptest functions), so `gate task` never builds the merge base.
- **Unchanged from rev 1:** `I32F32`, the 16-bit `Angle`, `fl_catalog` as the single feature list, built-in content kinds, M0 loading with `std::fs`, and `Wander` as a debug and bench behaviour.

## Self-review

**Spec coverage** (revision 2 → task):

| Spec item | Task(s) |
|---|---|
| §3 toolchain, per-worktree build dir, rust-lld, nightly for rustdoc JSON | 1, 28 |
| §5.1 crates, dependency rules, two-crate features | 1, 2 (arch-check), 20, 22 |
| §5.2 `SimFeature`/`ClientFeature`, topological order, `SimBuilder` | 9, 20, 22 |
| §5.3 runner, `step_tick`, `TickOutput`, `fl_view`, `SimQuery`, the four drivers, failures | 7, 10, 12, 20 |
| §5.4 keyed RNG, collections, narrowed ordering, executor, hazards, classification, state hash, rules hash, registries | 5, 8, 9, 10, 2 (lints) |
| §5.5 envelope, input delay, `(player, seq)`, postcard, debug commands, `MatchHeader`, replays | 6, 11, 12, 18 |
| §5.6 eight phases, sync points, spawn funnel, Messages, driver steps, system-order snapshot | 10, 11, 28 |
| §5.7 spatial grid, entity-mix scale scenario, work-counter budgets | 15, 29 |
| §5.8 save/load, codecs, migrations, round-trip and branch tests | 13, 17 |
| §6 content, string tables, canonical hash; §6.4 hot reload | 8, 14; hot reload → M1 |
| §9.1 M0 list; §9.2 Group C; §9.3 Group D | all tasks; Roadmap |
| §10 client view, input → `LocalDriver` | 20, 22 (T12 for the camera) |
| §11 testbed tools as commands, driver controls, render overrides, inspector | 21, 25 |
| §12.1 gate levels; 12.2 lints and arch-check; 12.3 determinism checks; 12.5 protection; 12.6 perf tiers; 12.7 contract snapshots | 2, 3, 17, 28, 29, 23, 30 |
| §13.2 ADRs, OWNERS; 13.3 agent environment; 13.5 probes | 3, 4, 26, 27 |
| §15 A logging, B journal, C agent CLI and BRP, D profiling | 19, 18, 24, 25, 19/23 |

- **Type consistency.** Checked across tasks: `PlayerCommand { exec_tick, seq }`, `CommandDraft`, `TickDriver::{issue, poll, pending}`, `CommandStamper::restore`, `SimRunner::{step, run, query, spawn_threaded}`, `MatchSetup`, `RunConfig` (fields added in Tasks 9, 10 and 18 with `..Default::default()`), `SpawnRequest { unit: Option<_>, init }`, `RejectReason` (9 variants), `Class::hashed`, `Codec::{migrate_from, custom}`, `ScaleScenario` (fields extended in Task 29) and `SimEvent::Spawned { unit: Option<UnitTypeId> }`. The client names (`RunnerLink`, `UnitView`, `ViewIndex`) replace rev-1's `SimHost`/`SimLink`/`SimLinks`, so nothing is named twice.
- **Proportion.** The plan carries signatures, values and test assertions. Bodies appear only where they are contracts: the `roll`/`stream_key` definitions and the `OWNERS.toml` data. Content data and repeated rev-1 detail are referenced, not copied.
