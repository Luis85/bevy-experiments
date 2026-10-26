## Global Constraints

- Bevy pinned exactly: `bevy = "=0.20.0"`, `bevy_app = "=0.20.0"`, `bevy_ecs = "=0.20.0"`. Rust ≥ 1.97.1, pinned to `1.99.1` in `rust-toolchain.toml`. Edition 2024.
- Build output on a short path: `.cargo/config.toml` has `[build] target-dir = "C:/t/fl"` (Windows MAX_PATH / LNK1104).
- **Where to edit.** `main` lives in `C:\Projects\bevy-experiments`. A guard blocks the Write and Edit tools under `C:\Projects\`, so all editing happens in git worktrees under `C:\fl\` (`C:\fl\a` for Phase A, `C:\fl\tNN` for parallel tasks). The controller fast-forwards or merges `main` with `git -C C:/Projects/bevy-experiments …`.
- Dependency rules (§5.1):
  - `fl_math` and `fl_protocol` have no Bevy dependency.
  - `fl_content` may use `bevy_ecs` only.
  - `fl_sim`, `fl_catalog`, `fl_test_utils` and `fl_cli` must never depend, even transitively, on `bevy_render`, `bevy_window`, `bevy_input`, `bevy_time`, `bevy_winit` or the `bevy` umbrella crate.
  - `scripts/check_deps.sh` enforces these.
- Determinism (§5.4):
  - Sim state uses `Fx` and integers only. `fl_sim` and `fl_protocol` have no `f32`/`f64` arithmetic, enforced by `#![deny(clippy::float_arithmetic)]`.
  - `std::collections::HashMap`/`HashSet` and `std::time::Instant`/`SystemTime` are banned workspace-wide through `clippy.toml` `disallowed-types`. Client code uses `BTreeMap` or `bevy::platform::collections::HashMap`; bench timing code carries a local `#[allow]`.
  - All randomness goes through `SimRng`.
  - Any iteration over entities that changes state follows `SimIndex` (SimId) order.
- `SimTick` runs these sets in order: `ApplyCommands → Orders → Behaviour → Movement → Combat → Damage → Vision → Lifecycle → Hash`. Single-threaded executor; ambiguity detection at `LogLevel::Error`.
- Sim tick rate is `SIM_TICK_HZ = 20` (in `fl_protocol`). The state hash (xxh64) runs every 20 ticks by default, set by `StateHash.interval_ticks` in `rules.ron`.
- Unit types are never a closed Rust enum. Content IDs are strings namespaced by mod (`"base.de.pz3"`) and interned to dense IDs in sorted order.
- Content is strict. Every def and kind struct uses `#[serde(deny_unknown_fields)]`. Unknown component names, rules sections and `*.X.ron` kinds are errors that carry the file and line.
- Never copy code or data from nival/Blitzkrieg; every stat in `assets/base` is our own number. M0 uses primitive placeholders only, and nothing is downloaded.
- Sim coordinates are metres on the ground plane, `x` east and `y` north. Heading is an `Angle`: 0 is +x, increasing counter-clockwise toward +y.
  - The client maps sim `(x, y)` to Bevy `Vec3::new(x, height, y)`, and Bevy yaw = `-heading_radians`.
  - Placeholder meshes are built with their length along local +X.
- **Observability (spec §15).**
  - Journal, profiler timings, `SimCounters` and `SetTimings` are observe-only. Sim logic never reads them, and they stay out of the state hash.
  - Sim systems live in module paths that name their owner (`fl_sim::<module>::…`, `fl_feat_<name>::…`); the profiler attributes time to features from those paths.
  - Log with `tracing` using the default module-path targets.
  - `std::time::Instant` is allowed only inside `fl_sim::observe`, `fl_observe` and bench code, each with a local `#[allow(clippy::disallowed_types)]`.
- **Task gate.** Run these from the worktree root before every commit; all must pass:
  ```bash
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  bash scripts/check_deps.sh
  ```
- Commit only the files your task owns. Use messages like `feat(fl_x): …`, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Environment notes for agents (controller)
- Edit files ONLY with absolute paths under your worktree (C:\fl\...). The Write/Edit tools are blocked under C:\Projects\ by a guard hook; never write there.
- The shell may reset its cwd; always `cd <worktree> && …` in each Bash call.
- Cold Bevy builds take 5–10+ min: run long cargo commands with a 600000 ms timeout or in the background.
- Plan file (read-only reference; do not read the whole thing unless your brief points you to a section): C:\fl\a\docs\superpowers\plans\2026-10-23-frontline-m0-foundation.md
- Spec: docs/superpowers/specs/2026-10-23-frontline-rts-design.md ; Bevy 0.20 API: docs/research/bevy020-api-reference.md
- Never copy code/data from nival/Blitzkrieg. Never download assets.
