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

