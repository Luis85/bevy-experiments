# Baseline recommendations (synthesis of 3 research reports, 2026-10-23)

Sources:
- `agent-dev-environment.md` (agent tooling)
- `architecture-best-practices.md` (framework patterns, design critique C1–C24)
- `workflow-and-verification.md` (design → verification pipeline)

The detail, versions and references are in those files. Items are grouped by **when** to adopt them. Contract-affecting
items are cheapest now, while the M0 contracts are still being written.

## A. Adopt in M0: architecture contracts (cheap now, expensive later)

| # | Change | Why | Ref |
|---|---|---|---|
| A1 | **Two crates per feature** (`fl_feat_x` sim + `fl_feat_x_client`) replacing the `client` Cargo feature. The `GameFeature` trait is split into `SimFeature { ID, VERSION, deps }` and `ClientFeature`. `GameBuilder` sorts features topologically. | Cargo's feature-unification opt-out is still unstable (verified on 1.99.1), so `--workspace` gates compile the sim with client code and can't prove the sim is headless. Clippy uses only the nearest `clippy.toml`, so per-half lint bans are impossible. | arch N1/C1 |
| A2 | **Threaded `SimRunner`** that owns the sim World. Commands go in. `Arc<SimSnapshot>` and `SimEvents` come out. The snapshot schema lives in a new small `fl_view` crate, so the client never touches sim component types. Add a `TickDriver` trait with Local, Lockstep, Replay and Test implementations. | `SubApp::update` is synchronous on the main thread, so a 15 ms tick breaks 60 fps. The "sync step" couples the client to sim internals. | arch N2/C2/C19 |
| A3 | **Keyed counter-based RNG**: `roll(seed, tick, sim_id, stream_id, n)` everywhere. Ban any sequential RNG resource in the sim. | With one shared stream, adding a feature shifts every later draw. That breaks golden replays, couples features and blocks parallelism (Factorio hit this, FFF #421). | arch N3/C3 |
| A4 | **Stable-string registries for every runtime ID** (command kinds, def components, stats, conditions, tags, RNG streams, journal events): sort by name in `finish()`, then assign dense IDs. Replays and saves carry the string tables. | Assigning IDs in registration order makes them depend on the build and breaks replays and netcode. | arch N4/C5 |
| A5 | **Sim schedule hygiene**: phases `Input → Think → Act → Resolve → Economy → Perceive → Lifecycle → Publish`, with named hook sub-sets. `auto_insert_apply_deferred: false` and explicit `ApplyDeferred`. A single **spawn funnel** assigns SimIds in request order. Set the executor kind explicitly. Hashing and `clear_trackers` run in the driver, outside the schedule. Message rotation is part of housekeeping. | Auto sync points make visibility timing depend on unrelated features. Manual `run_schedule` skips message rotation and `clear_trackers`. | arch N5/C8/C15 |
| A6 | **Mandatory component classification**: every sim component is `hashed+saved`, `derived` (rebuilt on load) or `transient`. Check this at startup. Hash resources too. | An unregistered component can't be seen by the hash or saves. | arch N6/C6 |
| A7 | **Narrow the "sort everything by SimId" rule** to order-sensitive operations: shared writes, spawns, budgets and queues, cross-entity reads. Keep a hook-maintained `SimIndex`. Allow `par_iter_mut` for entity-local updates. | Sorting 3,000 entities in every system costs time and blocks safe parallelism. The branch test (A8) enforces correctness. | arch C4 |
| A8 | **Save/load skeleton** with per-component versioned codecs (postcard), typed migrations, and relationships stored as SimId. Fitness tests: save→load→save is byte-identical, and a **branch test** where save at N, reload and continue matches an uninterrupted run (Factorio heavy mode). | Campaigns, reconnects and late join need it, and retrofitting is how saves rot. The branch test also catches iteration-order bugs. | arch N7/C7 |
| A9 | **Lockstep-ready command envelope**: `exec_tick`, per-player `seq`, apply in `(player, seq)` order, input delay of 1 tick in single player. Use **postcard**, not bincode (discontinued). Handshake and replay header include `build_id`, `features[(id,version)]`, `mods[(id,version)]` and the rules hash. Define the canonical rules-hash encoding. | Single player and multiplayer should share one path. A desync caused by different code or features should be caught at handshake. | arch N20/C9/C14 |
| A10 | **Debug and testbed actions become `DebugCommand`s** in the protocol (spawn, set owner, invulnerable, set HP, teleport), gated by a `cheats_allowed` header flag. "Reveal all" becomes a client render override. BRP sim methods are custom `fl.*` only, and BRP `world.mutate_*` must never touch the sim. | Client-side mutation of the sim breaks replays and the "commands only" anti-goal. | arch N19/C12, workflow §4.10 |
| A11 | Hot reload: reloads that change only values apply in place. If the set of IDs changes, rebuild the match from a snapshot. A reload is logged as a replay event, or the replay is marked non-reproducible. | Live entities hold dense IDs that would go stale. | arch C16 |
| A12 | **Redefine the scale target** by entity kind: 3,000 units ≈ 8–12k sim entities plus projectiles in flight. Benchmark that mix. | Squads, crews and wrecks multiply the entity count. | arch C17 |

## B. Adopt in M0: verification and erosion gates (the workflow baseline)

| # | Change | Ref |
|---|---|---|
| B1 | **`cargo xtask gate {task,pr,nightly,milestone}`**. CI and agents run the same entry points. A **Stop/SubagentStop hook** runs `gate task` on the crates that changed, and task reports paste its output as evidence. The task gate must stay under about 3–5 minutes. | workflow §5.1, agent #4 |
| B2 | **Determinism and erosion lints**. Every sim crate gets a generated `clippy.toml`: ban f32/f64, std/bevy `HashMap`/`HashSet`, `Instant`, `SystemTime`, `ParallelCommands`, `thread::spawn`, `rand::rng`. Add `clippy::float_arithmetic = deny`, with a scoped `#[expect]` in the RON→Fx module. `xtask arch-check` checks allowed crate edges using a separate `cargo tree -p` per crate, plus headless test jobs per package. `cargo-deny` covers licences, duplicate `bevy_*` versions, and a ban on bincode and `bevy_dynamic_plugin`. | arch N16/C20, workflow §4.2 |
| B3 | **Determinism checks earlier**: ambiguity detection at `Error`, a `shuffle_seed` run (1 seed per task, 3 per PR), an **insta snapshot of the SimTick system order** so schedule edits show up in diffs, replay round-trips, and save/load parity. | workflow §4.5, agent #2 |
| B4 | **Protect tests from implementers**. A PreToolUse hook denies edits to `tests/acceptance/**`, `tests/holdout/**`, `tests/replays/**`, `**/snapshots/**`, `scenarios/**`, `docs/design/mechanics/**` and perf baselines. A Bash hook blocks `cargo insta accept`, `INSTA_FORCE_PASS` and `INSTA_UPDATE=always`. `xtask test-guard` flags removed or weakened asserts, new `#[ignore]`, changed tolerances, lower proptest case counts and new seed overrides. If a test looks wrong, the implementer writes "BLOCKED: test X contradicts spec Y" and stops. | workflow §5.4, agent #4 |
| B5 | **Perf gates use deterministic work counters** (path nodes, LOS cells, query candidates, allocations), budgeted and snapshotted. Instruction counts (gungraun on Linux CI, or CodSpeed) show trends. Criterion wall-clock runs only at milestones, on the dev machine, interleaved and median of 5. This replaces the noisy 20% wall-clock gate. | workflow §4.9, arch N21/C18, agent #9 |
| B6 | **Contract snapshots**: public API of `fl_protocol`, `fl_view` and the sim extension API (via `cargo-public-api` + insta), and wire and save format snapshots (via `serde-reflection`). | arch N17 |
| B7 | **ADRs** (MADR 4.0) in `docs/adr/`. Write the first 10 from these decisions. Add a deprecation policy, and an `OWNERS.toml` with `xtask check-scope --track` so parallel agents stay inside their paths. Contract paths change only through an ADR. | arch N18/C24 |
| B8 | **Remote + 3-OS CI** (GitHub Actions). The sim job and the client job are split with separate rust-cache keys. Windows WARP and Linux lavapipe are used for nightly screenshots. Golden replays are verified on every OS. *Needs a remote: user decision.* | workflow §5.2 |
| B9 | **Agent dev environment**: a per-worktree short `build.build-dir` (`C:/t/fl/{workspace-path-hash}`), `rust-lld`, less debug info for dependencies, sccache, and at most 2 concurrent builds. A short `CLAUDE.md` (under 200 lines) plus path-scoped rules. A project skill points to the **local Bevy 0.20 sources, examples and migration guides in `~/.cargo/registry`**, then the verified-snippets file, then a `probes/` crate. `bevy_brp_mcp` 0.23 for driving the live game, with a distinct BRP port per agent. A nextest "agent" profile with compact output. 4–5 role subagents. Probe early whether `dynamic_linking` combined with sim crates depending only on `bevy_ecs` causes duplicate-crate link errors. | agent top-10 |
| B10 | `fl.screenshot` renders from an **offscreen camera** (windows hidden behind the app capture as black). Benchmarks switch the window to continuous updates, because an unfocused window throttles. | agent findings |

## C. Adopt with M2, before mechanics land

| # | Change | Ref |
|---|---|---|
| C1 | **Machine-checkable mechanic specs**: `docs/design/GDD.md` index plus one spec per mechanic (EARS requirements with stable `AREA.NAME.N` IDs, formulas, parameter tables with RON paths, acceptance criteria). `xtask spec-lint`. `xtask trace` maps requirements to tests via `@req` tags. | workflow §3 |
| C2 | **Provenance ledger** (`assets/base/provenance/*.prov.ron`). The vocabulary is closed: Original, Manual, ResearchDerived, OursScaled, Guess, Tuned. `data-check` enforces it, and a tuning-tables doc is generated from it. | workflow §3.3 |
| C3 | **Separate test-author agent** writes protected acceptance tests from the spec alone, plus a **held-out** 10–20% that runs only at milestones and nightly. | workflow §2.2 |
| C4 | **Statistical test policy**: oracles come from the spec or an f64 reference, never measured from the code. `assert_rate_within_wilson` at α = 1e-4. Two tiers: formula level (10⁵ draws) and scenario level (200–500 seeds). Nightly runs rotate seeds. A failing seed set may be rerolled once. | workflow §4.4 |
| C5 | **Core vocabulary**: OpenRA-style **conditions** (granted tokens plus `requires` expressions), a **modifier and stat spec** with deterministic stacking order and a cached effective stat, **tag bitsets plus class expressions**, and **effect/trigger verb registries**. Command-panel buttons carry `requires`. | arch N8, N9, N11, N12, C10, C22, C23 |
| C6 | **Content load phases and patch ops**: discover → parse → migrate (`format_version`) → patch (Set, Merge, Remove, Add, AddTag, If) in passes → inherit → typed deserialize → feature post-process hooks → link → freeze. Provenance is tracked per path. | arch N10/C11 |
| C7 | **Order queue + activity stack** for unit behaviour, built in-house. State is stored as table enums, with SparseSet markers only for rare states. **Custom relationships** for squad, garrison, transport, towing, crew and ownership (not `ChildOf`). Event-model ADR: Messages for gameplay, hooks for invariants, observers only for order-independent reactions in the sim. | arch N13–N15/C21 |
| C8 | **Golden replay versioning**: `SIM_VERSION`, semantic assertions next to hashes, `fl_cli rebless --reason MCR-n` with a report, and change classes in commit trailers (refactors must stay bit-identical). | workflow §6.1–6.3 |
| C9 | **Mutation testing**: `cargo-mutants --in-diff` per PR and per task (time-capped), a full sharded run nightly, missed-mutant triage at milestones. Coverage stays diagnostic only. | workflow §5.4 |
| C10 | Risk-tiered review, so not every task gets two full LLM reviews. A per-mechanic mini-gate inside each milestone. | workflow §8 |
| C11 | Render scale baseline: one material per unit type, per-instance data via `MeshTag`/`GpuComponentArrayBuffer`, `VisibilityRange` LOD. | arch N22 |

## D. Later (with triggers)

- AI controllers **outside** the sim on a designated peer, with `PlayerView` in `fl_view`. Layered utility AI, HTN and influence maps.
- Lockstep net crate: spike lightyear 0.31 against a thin custom session behind a `NetSession` trait. Desync tooling: per-phase and per-component hashes, `desync-diff`, `track_location`.
- Campaign meta layer as a separate state and save. Economy core: stocks, flows, aggregate networks, sleeping entities, reservations.
- Vertex-animation-texture (VAT) infantry plus animation LOD. `.bsn` prefabs once the loader ships. `bevy_lint` once it supports 0.20. Scripting tier.
- F8 bug bundle → `/triage-bug` → minimized regression replay. `fl_cli sweep` with common random numbers and Lanchester refits. Runtime experiment flags inside the rules hash. Feedback ledger.

## Avoid

- A `cargo check` hook after every edit.
- One shared target dir for all worktrees.
- Nightly-only speedups.
- `bevy_lint` (it only supports 0.18).
- Stale Bevy MCP servers.
- Vision models as pass/fail gates.
- Testing the sim by clicking with the mouse.
- `dssim`, because it is AGPL.
- `moonshine-save` for sim state, because it has no versioning.
- bincode.
- Global wall-clock perf gates.
