# Task 1 report
Status: DONE_WITH_CONCERNS

## Step 1 probe (R19)
`build-dir = "C:/t/fl/{workspace-path-hash}"` works: cargo expands it to `C:/t/fl/c5/fdf61b108f2dc9/` (two-level hash). `target/debug` holds only final artifacts (no .fingerprint); `.fingerprint` is under the build dir. `cargo metadata --no-deps` reports `build_directory` = `C:/t/fl/c5\fdf61b108f2dc9` (Task 4 hook can rely on it). No fallback needed. Old `C:/t/fl/debug` from rev-1 is stale and can be deleted.

## Step 2 sccache
rustc-wrapper + SCCACHE_BASEDIRS set. Stats after cold build: 627 compile requests, 473 Rust misses, 0 hits (expected, cold cache). Cross-worktree reuse to be verified at T12 rebase.

## Build / gate
- `cargo build --workspace --all-targets`: success, 18m25s, 0 warnings.
- fmt --check OK; clippy --workspace --all-targets -D warnings OK; cargo test --workspace all green (0 failures); `check_deps.sh` -> `deps OK`.

## Notes / concerns
- bevy features: `default` is a collection list (2d,3d,ui,audio). Used top-level `["2d","3d","ui"]` (= default minus audio) rather than the fully flattened leaf list; flattening 80+ leaves would be less auditable. Change if a leaf list is wanted.
- `bevy_remote` (fl_client debug_remote, probe_remote) pulls bevy_dev_tools -> bevy_audio/rodio/cpal regardless; unavoidable. Since the workspace builds all probes, cpal/rodio compile in `--workspace`.
- `serde-reflection = "0.5"` kept verbatim from brief (0.6.0 exists).
- `default-members = ["crates/*","xtask"]` accepted by cargo (glob works).
- `.gitignore` in the repo differed from the brief's description (had `/.claude/`, no logs etc.); removed `/.claude/`, appended `/logs/`, `.claude/settings.local.json`, `.claude/worktrees/`.
- fl_game has a lib.rs stub plus the existing main.rs; fl_client had no use of fl_sim, so dropping the dep needed no code change.
- check_deps.sh: added fl_view to no-Bevy list, fl_observe to headless list (fl_game lib not added).
- Cargo.lock committed (required by manifest changes).

## Fix round 1
1. check_deps.sh: my earlier sed silently did not apply (the report was wrong). Now applied: `fl_view` added to the no-Bevy list, `fl_observe` to the headless list. `bash scripts/check_deps.sh` -> `deps OK`.
2. Bevy features: root `[workspace.dependencies] bevy` now carries the explicit list (58 entries: `default` = 2d,3d,ui,audio expanded to leaf features, minus audio/vorbis; includes std, async_executor, bevy_gilrs, bevy_ui_widgets, ...). fl_client, fl_game, probe_dynlink use `bevy.workspace = true`; probe_remote adds only `features = ["bevy_remote"]`; fl_game/dynlink keep their feature-forwarding entries.
3. Cargo.toml restored to LF (it was CRLF because my Python rewrite ran in text mode on Windows). All touched manifests rewritten with LF; no .gitattributes added.
4. Cargo.lock `image` 0.25.10 -> 0.25.9: cargo's MSRV-aware resolver ("Locking ... highest Rust 1.97.1 compatible versions", from workspace rust-version = 1.97.1) picks versions compatible with MSRV 1.97.1; `cargo update -p image --dry-run` confirms 0 changes available. Not a bevy_brp_extras constraint. Cargo.lock unchanged this round.

Commands: `cargo build --workspace --all-targets` (rebuild 3m10s, 0 warnings); `cargo fmt --check` OK; `cargo clippy --workspace --all-targets -- -D warnings` OK; `cargo test --workspace` 0 failing result lines; `bash scripts/check_deps.sh` -> `deps OK`.
