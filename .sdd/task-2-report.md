# Task 2 report: logging, crash reports, lints, build profile

Branch: worktree-agent-a024936d5416b64ee (fast-forwarded to bf7272d first).

## Implemented
- horde_core: `tracing` (std, attributes, no default features); Cargo.lock gained only `tracing` under horde_core. `sim/persistence.rs`: `Game::save`/`Game::load` log start/success (info), failure and backup fallback (warn). `sim.rs` untouched.
- New `horde_game/src/logging.rs`: `DEFAULT_FILTER`; `file_layer` (LogPlugin::custom_layer, plain-text file `<save dir>/logs/lastlight-<unix-ts>.log`, newest 10 kept); `install_panic_hook` (chains previous hook, writes `crash-<ts>.txt` with version, OS/arch, panic info, forced backtrace, logs at error). No new dependency (`bevy::log::tracing_subscriber` re-export). RUST_LOG override confirmed from bevy_log docs.
- main.rs: LogPlugin set once via DefaultPlugins.set; panic hook installed first.
- println/eprintln to info!/warn!/error! in art.rs, visual_assets.rs, capture_sequence.rs, diagnostics.rs, main.rs capture fn. capture_sequence unwraps to let-else; diagnostics NonZeroU8 expect to AppExit::error(); bounded_count warns on rejected values.
- Cargo: dev `debug = "line-tables-only"`; `[profile.playtest]`; `[workspace.lints]` per brief; crates `[lints] workspace = true`. package_playtest.ps1 defaults to `-Profile playtest`.
- Docs: README, PLAYTEST_GUIDE, ARCHITECTURE.

## Rulings
- Plain println/eprintln kept with `#[allow(clippy::print_*)]` + reason where no subscriber exists yet or scripts read stdout: main() (startup errors, `--smoke` report read by verify_playtest.ps1, animation-export arg check), catalogue-source lines, "Player models:" line, file_layer failure message.
- "Native frame evidence"/"Animation evidence" are not parsed by scripts, so now info!.
- horde_core examples and integration tests got file-level `#![allow(clippy::print_stdout, clippy::print_stderr)]`.
- Crash files are not pruned.

## Tests
Added 2 in logging.rs (prune, crash report). None removed/changed. Baseline 432 passed/1 ignored -> 434/1.

## Gate
`cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test --locked --workspace`: green, 434 passed, 1 ignored.

## Concerns
Log file/panic hook only unit-tested via helpers, not in a launched window. Dev debug change forces a one-time dependency rebuild.

# Fix report (review finding: Frontend::load before LogPlugin)

Change: in `main.rs` moved `.insert_resource(ui::Frontend::load())` from before `add_plugins(DefaultPlugins...)` to right after it (before EguiPlugin), with a comment, so `Game::load` info/warn events are emitted after the subscriber is installed. Verified: `AtmosphereAudioPlugin` does not reference `Frontend` (grep of atmosphere_audio.rs); EguiPlugin takes no Frontend at build; capture-sequence `insert_resource` calls earlier in main are untouched. `Time::<Fixed>` and ClearColor inserts remain before plugins as before.
Verification: full gate re-run green (fmt, clippy --locked -D warnings, test --locked --workspace): 434 passed, 0 failed, 1 ignored.
