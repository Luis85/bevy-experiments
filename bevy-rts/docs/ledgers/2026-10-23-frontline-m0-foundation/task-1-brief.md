### Task 1: Workspace scaffold

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `clippy.toml`, `.gitignore`, `README.md`, `scripts/check_deps.sh`
- Modify: `.cargo/config.toml` (keep the existing `[build] target-dir`)
- Create: `crates/<crate>/Cargo.toml` and `src/lib.rs` for each of `fl_math fl_protocol fl_content fl_sim fl_catalog fl_test_utils fl_cli fl_client fl_game`, plus `fl_cli/src/main.rs` and `fl_game/src/main.rs`
- Create stubs: `crates/fl_client/src/{camera,map_view,view,placeholder,game}.rs`, `crates/fl_client/src/debug/mod.rs`

**Interfaces — Produces:**
- Compiling empty crates.
- `fl_client` plugin stubs `CameraPlugin`, `MapViewPlugin`, `SimViewPlugin`, `PlaceholderPlugin` and `DebugToolsPlugin`, each a unit struct with an empty `impl Plugin` `build`.
- `pub struct ClientPlugins;` in `lib.rs`, a `PluginGroup` that adds all five.
- An empty `game.rs` module.

- [ ] **Step 1: Root `Cargo.toml`.**
  - Virtual workspace with `resolver = "3"` and `members = ["crates/*"]`.
  - `[workspace.package]`: `edition = "2024"`, `rust-version = "1.97.1"`, `license = "MIT OR Apache-2.0"`.
  - `[workspace.dependencies]`, resolved to the latest compatible versions at implementation time (Bevy stays exact): `bevy = "=0.20.0"`, `bevy_app = "=0.20.0"`, `bevy_ecs = "=0.20.0"`, `bevy_egui = "0.43"`, `fixed = "1"`, `serde = { version = "1", features = ["derive"] }`, `ron = "0.12"`, `erased-serde = "0.4"`, `postcard = { version = "1", features = ["alloc"] }`, `xxhash-rust = { version = "0.8", features = ["xxh64"] }`, `thiserror = "2"`, `clap = { version = "4", features = ["derive"] }`, `criterion` (latest), `proptest = "1"`, `tracing = "0.1"`, and path deps for every `fl_*` crate.
  - `[workspace.lints.rust] unsafe_code = "forbid"`.
  - Profiles: `[profile.dev] opt-level = 1`, `[profile.dev.package."*"] opt-level = 3`, `[profile.release] debug = "line-tables-only"`.

- [ ] **Step 2: Per-crate dependencies.** Each crate also sets `[lints] workspace = true`.

| crate | deps | dev-deps |
|---|---|---|
| fl_math | fixed, serde | proptest, ron |
| fl_protocol | serde | postcard, ron |
| fl_content | fl_math, fl_protocol, bevy_ecs, serde, ron, erased-serde, xxhash-rust, thiserror, tracing | — |
| fl_sim | fl_math, fl_protocol, fl_content, bevy_app, bevy_ecs, serde, postcard, xxhash-rust, thiserror, tracing | criterion, proptest (`[[bench]] name = "sim_tick", harness = false`) |
| fl_catalog | fl_sim | — |
| fl_test_utils | fl_math, fl_protocol, fl_content, fl_sim, fl_catalog, bevy_app, bevy_ecs | — |
| fl_cli (lib + bin `fl_cli`) | fl_math, fl_protocol, fl_content, fl_sim, fl_catalog, fl_test_utils, bevy_app, bevy_ecs, clap, thiserror, tracing, ron, serde | — |
| fl_client | bevy, bevy_egui, fl_math, fl_protocol, fl_content, fl_sim, tracing | — |
| fl_game (bin) | fl_client, fl_catalog, clap | — |

- [ ] **Step 3: Tooling files.**
  - `rust-toolchain.toml`: `channel = "1.99.1"`, components `rustfmt` and `clippy`.
  - `clippy.toml`: `disallowed-types = ["std::collections::HashMap", "std::collections::HashSet", "std::time::Instant", "std::time::SystemTime"]`.
  - `.gitignore`: `/target`, `/.claude/`, `*.log`.
  - `README.md`: one paragraph, the gate commands, and `cargo run -p fl_game`.
- [ ] **Step 4: `scripts/check_deps.sh`.** It checks each crate's normal dependencies with `cargo tree -p <crate> -e normal --prefix none`:
  - `fl_math`, `fl_protocol`: fail if any line starts with `bevy`.
  - `fl_content`: fail on any `bevy*` crate except `bevy_ecs`, `bevy_ecs_macros`, `bevy_ptr`, `bevy_utils`, `bevy_platform`, `bevy_tasks`, `bevy_reflect*` and `bevy_macro_utils`.
  - `fl_sim`, `fl_catalog`, `fl_test_utils`, `fl_cli`: fail on `^(bevy |bevy_render|bevy_window|bevy_input|bevy_time|bevy_winit|wgpu)`.

  On success it prints `deps OK`. On failure it prints the offending crate and line, then exits 1.
- [ ] **Step 5: Build** with `cargo build --workspace`. A cold Bevy build takes about 5–10 min, so run it in the background. Expected: success.
- [ ] **Step 6: Run the task gate.** Expected: everything passes, and the script prints `deps OK`.
- [ ] **Step 7: Commit** everything above together with this plan: `chore: workspace scaffold for Frontline M0`.

