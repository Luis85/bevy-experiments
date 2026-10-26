# Task 1 report: workspace scaffold

Status: DONE_WITH_CONCERNS. Commit 0090f47 `chore: workspace scaffold for Frontline M0` (includes Cargo.lock; .superpowers/ ignored).

Resolved versions: bevy/bevy_app/bevy_ecs =0.20.0, bevy_egui 0.43.1, fixed 1.31.0, serde 1.0.229, ron 0.12.2, erased-serde 0.4.10, postcard 1.1.3, xxhash-rust 0.8.19, thiserror 2.0.21 (1.0.69 also in lock, transitive), clap 4.6.7, criterion 0.8.2 (workspace pin "0.8"), proptest 1.12.0, tracing 0.1.44.

Gate: cargo fmt --check OK; clippy --workspace --all-targets -D warnings OK; cargo test --workspace OK (18 test binaries, 0 tests); check_deps.sh prints `deps OK`. Cold build ~14.5 min.

Files: Cargo.toml, rust-toolchain.toml, clippy.toml, .gitignore (incl. /.superpowers/), README.md, scripts/check_deps.sh, 9 crates (lib.rs each; fl_cli and fl_game main.rs as `fn main() {}`), fl_sim/benches/sim_tick.rs (`fn main() {}`, harness=false), fl_client stubs (camera, map_view, view, placeholder, game, debug/mod.rs, ClientPlugins). .cargo/config.toml unchanged.

Concerns:
- check_deps.sh allowlist for fl_content needed one addition beyond the brief: `bevy_ecs_macro_logic` (internal bevy_ecs subcrate pulled by bevy_ecs 0.20). Without it the check failed.
- Dependency rules bevy_reflect_derive also appear and are matched by `bevy_reflect.*`.
- game.rs holds only a one-line doc comment (rustfmt rejects a truly empty file).
- fl_cli has both lib.rs and main.rs; fl_game is bin-only.
