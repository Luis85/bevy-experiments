# Frontline

Frontline is a deterministic WWII real-time strategy game (a Blitzkrieg-style RTS) in Rust on Bevy 0.20.
A headless fixed-point simulation (`fl_sim`, 20 ticks/s) runs beside a Bevy client that only renders and sends commands.

## Where things live

- Spec: `docs/superpowers/specs/2026-10-23-frontline-rts-design.md` (section numbers below are spec §).
- ADRs: `docs/adr/NNNN-*.md`, indexed in `docs/adr/README.md`.
- Plans: `docs/superpowers/plans/` (M0: `2026-10-23-frontline-m0-rev2.md`).
- Research: `docs/research/`; Bevy 0.20 facts in `docs/research/bevy020-api-reference.md`.
- Debugging guide (CLI commands, BRP methods, journal kinds): `docs/debugging.md` (written in M0, Task 30).
- Path ownership and protected paths: `OWNERS.toml`. Path-scoped rules: `.claude/rules/`.

## Commands

```bash
cargo xtask gate task --track <t>                  # "verified": run before every commit, paste the output
cargo nextest run --profile agent -p <crate>       # focused tests while iterating
cargo run -p fl_cli -- data-check assets/base      # validate content
cargo run -p fl_game                               # the game (testbed)
cargo run -p fl_game --features dynamic            # faster dev relinks (bevy/dynamic_linking)
```

- `--features dynamic` works with the `bevy_ecs`-only sim crates (Task 26 probe: headless and `fl_game` builds
  linked; the windowed run is untested). Never use it for gate, bench or release builds.
- Other gate levels: `gate pr` (before merging into `main`), `gate nightly`, `gate milestone --milestone M0`.
- Building blocks: `cargo xtask arch-check | gen-clippy [--check] | check-scope --track <t> | test-guard`.
- The gate's diff base: `--base` > env `FL_GATE_BASE` > `.gate-base` (worktree root, git-ignored) > merge base
  with `main`. Full logs of every gate step are in `target/xtask/<step>.log`.
- Never edit a generated `clippy.toml`; edit `xtask/clippy-template.toml` and run `cargo xtask gen-clippy`.

## Toolchain

- Rust `1.99.1` pinned in `rust-toolchain.toml` (MSRV 1.97.1), edition 2024. Never run `rustup default`.
- One dated nightly exists only for `cargo-public-api` rustdoc JSON; its name is in `xtask/api-toolchain.txt`
  (added with `xtask api`). Nothing builds, tests or lints with it.
- Cargo uses the platform compiler/linker and local `target/` by default (ADR-0023). Caches and custom paths
  are optional personal configuration; agents never change user-level config in `~/.cargo` or `~/.claude`.
- Bevy crates are pinned `=0.20.0` with `default-features = false` and explicit features.

## Worktrees and builds

- Use the user's current checkout or an isolated branch/worktree as appropriate (ADR-0023). This workspace
  is the `bevy-rts/` subdirectory of the Bevy Experiments Git repository; run Cargo from this directory.
- At most **two** cargo builders run at once across all agents; a gate run counts as one.
- Each workspace/worktree uses its own local `target/`. The legacy `cargo xtask hook worktree-remove`
  only cleans optional build paths under `C:/t/fl/`; it leaves platform-default output directories alone.
- A cold Bevy build takes 15+ minutes per build dir: run long cargo commands in the background or with a
  long timeout, and never start a second build while one runs.
- After Task 1, no task edits `Cargo.toml` or `Cargo.lock` unless its **Files** list says so. If a dependency
  is missing, stop and report.

## Determinism and dependencies (summary; the rules are spec §5.1 and §5.4)

1. Sim state is `Fx` (fixed point) and integers only; no `f32`/`f64`, `clippy::float_arithmetic` is denied.
2. Randomness only via `fl_math::Rolls` over `(MatchSeed, tick, sim_id, stream_key(name))`; no RNG state anywhere.
3. No std/`bevy_platform` `HashMap`/`HashSet`, `Instant`, `SystemTime`, `Time`, `thread::spawn`, `env::var`.
4. Order-sensitive work (shared writes, spawns, despawns, `SimId` allocation, queues) follows `SimIndex`.
5. Every sim component/resource is classified `Hashed`, `Derived`, `Transient` or `External`.
6. No `Local<T>`, observers, or gameplay on `Changed`/`Added`/`RemovedComponents` in sim crates; no `Entity`
   in hashed or saved state, commands, journal or replays.
7. Commands are `PlayerCommand`s applied in `(player, seq)` order; content IDs in payloads are names.
8. Sim crates use `bevy_app` + `bevy_ecs` only; never `bevy`, `bevy_render`, `bevy_window`, `bevy_input`, `Time`.
9. No client crate depends on `fl_sim` (not even as a dev-dependency); only `fl_game` links both sides.
10. `cargo xtask arch-check` and the generated per-crate `clippy.toml` enforce these; `.claude/rules/sim.md` has more.

## Protected paths and blessing

- `OWNERS.toml` `[protected]`: `**/tests/{acceptance,holdout,replays}/**`, `**/snapshots/**`, root
  `scenarios/**`, `docs/design/mechanics/**`, `docs/perf/baselines/**`, `docs/perf/budgets.ron`.
- Implementers never edit them; hooks deny the edit and every bless variant (`cargo insta accept|review`,
  `INSTA_UPDATE=always`, `INSTA_FORCE_PASS`, `--bless`, `xtask bless`, `git checkout -- tests/`).
- Tests and tools write `*.new` files (`insta` writes `*.snap.new`). Only the lead promotes them:
  `cargo xtask bless <paths> --reason "…"`, and the commit carries `Blessed: <reason>`.
- Escape hatch: if you believe a protected test is wrong, write
  `BLOCKED: test X contradicts spec Y because …` and stop. Never weaken the test.
- The `test-guard` gate row lists weakened tests (removed asserts, new `#[ignore]`, …); the reviewer accepts
  each finding one by one.

## Contract paths and ADRs

- The `contract` track in `OWNERS.toml`: `fl_math`, `fl_protocol`, `fl_view`, the `fl_sim` extension API
  (builder, registries, classification, phases, `step_tick`, runner, save format), the `fl_content` loader and
  registry, `xtask`, `.claude/`, `CLAUDE.md`, `.mcp.json`, `deny.toml`, `OWNERS.toml`, clippy configs, the spec.
- Contract paths change only with an ADR (`docs/adr/`, MADR 4.0). Cite it in the commit: `Refs: ADR-NNNN`.
- Each task names its **Track**; `check-scope` (a gate row) fails a branch that touches paths outside it.

## Observability (spec §15)

- Log with `tracing` on module-path targets; sim code lives in modules named after its owner
  (`fl_sim::<module>`, `fl_feat_<name>::…`), so the profiler attributes time by system name.
- The sim never reads a clock. `step_tick` wraps each tick in a `sim_tick` span (field `tick`); phase timing
  comes from `fl_sim::phase` marker events that `fl_observe` timestamps.
- Wall-clock reads only in `fl_observe`, `fl_cli` bench code and `fl_client`, each behind a scoped
  `#[expect(clippy::disallowed_types, reason = "…")]`.
- The journal, `SimCounters`, profiler timings and phase markers are observe-only: never read by sim logic,
  never hashed. Every mechanic ships with its journal records, counters and spans.

## Bevy APIs

Resolve every Bevy API with the `bevy-api` skill (`.claude/skills/bevy-api/`); never from memory. Bevy 0.20
shipped after your training data: names, modules and features changed (WESL shaders, Messages, `bevy_shape`, …).

## Live game (BRP)

`.mcp.json` registers `bevy_brp_mcp` as server `brp`. Every agent that runs the game uses its own BRP port
(`--remote-port` / `BRP_EXTRAS_PORT`; the default 15702 belongs to the lead). Drive the game through `fl.*`
methods, not mouse clicks; built-in `world.*` methods see only the client World, not the sim.

## Roles and hooks

- Role agents in `.claude/agents/`: `sim-engineer`, `client-engineer` (implementers), `test-author` (from M2),
  `reviewer` and `perf-analyst` (read-only). The main session is the lead; any other subagent counts as an
  implementer. `FL_ROLE=implementer|lead|read-only` overrides the role.
- Hooks (`.claude/settings.json`) run `.claude/hooks/xtask-hook.sh`, which calls the xtask copy in
  `target/hooks/` (refreshed by every `cargo xtask gate`, or `cargo xtask install-hooks`) and fails closed.
  Run `cargo xtask install-hooks` once in a new worktree.
- The Stop (lead) and SubagentStop (implementers) gate blocks finishing while `gate task` fails, three times,
  then escalates. Edit and Bash guards enforce the rules above (including shell writes onto protected paths);
  `cargo update`, `cargo add`, `rustup default` and `git push --force` always ask first.

## Commits

- Commit only the files your task owns, with a conventional message (`feat(fl_x): …`, `!` for contract breaks).
- Trailers include `Refs: ADR-NNNN` (contract changes) and `Blessed: <reason>` (blessed artifacts).
  Include co-authorship only when it accurately identifies a contributor.
- Keep LF line endings (`.gitattributes`); never rewrite a whole file you only edit partially.
