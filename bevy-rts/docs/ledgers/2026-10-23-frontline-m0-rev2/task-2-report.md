# Task 2 report: `xtask` gate levels, arch-check, generated clippy bans, cargo-deny, nextest profile

Status: DONE_WITH_CONCERNS
Commit: `03c0e04 build(xtask): gate levels, arch-check, generated clippy bans, cargo-deny, nextest profile` (trailer `Refs: ADR-0009`)

## What was implemented
- `xtask/src/gate.rs`: `Level`, `Outcome`, `Ctx`, `Step`, `steps()` (ordered table: fmt, clippy-drift, clippy, arch-check, test-headless, test-client, deny, data-check, gpu, brp, perf-file), `GateReport { passed, render }`, `run(&Ctx)`. Output: one line per step (`PASS fmt (2.1s)`, `WARN x (1.0s): msg`, `SKIP x: reason`, `FAIL x (34.0s)` + at most 40 indented tail lines plus `(full log: target/xtask/<step>.log)`), optional `WARN gate task took <n>s (budget 300s)`, last line `gate <level>: PASS|FAIL (<steps>) in <n>s`. Exit 0/1.
  - test-headless at task level is ONE nextest invocation: `-p <touched headless> -p fl_sim -p fl_test_utils -E '(package(=a) | … | ((package(=fl_sim) | package(=fl_test_utils)) & test(/smoke_/))) & not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))'`. pr/nightly/milestone: all headless packages. Env `FL_SHUFFLE_SEEDS` 1 (task) / 3 (others), `PROPTEST_CASES=4096` (nightly).
  - All nextest runs use `--no-tests=warn`; a run that matched no test becomes `WARN … no test matched <filter>` (so empty filters are visible, not silently green).
- `xtask/src/changed.rs`: `merge_base`, `changed_files` (`git diff --name-only --no-renames -z <base>` ∪ untracked), `touched_packages` (longest manifest-dir prefix; workspace-wide files touch everything).
- `xtask/src/arch.rs`: `Role` (from crate name), `is_probe` (manifest under `probes/`), `check_edges` (allowed table from the brief; dev edges checked for fl_client, fl_feat_*_client, fl_game; unknown crate names are a violation), `check_tree` (Pure: any `bevy*`; fl_content: `bevy*` minus ECS family; headless: exact list), `check` (edges + parallel `cargo tree -p X -e normal,build --prefix none` per pure/content/headless crate).
- `xtask/src/clippy_gen.rs` + `xtask/clippy-template.toml`: profiles `sim` (fl_protocol, fl_view, fl_sim, fl_feat_* excl. *_client), `edge` (fl_math, fl_content), `client` (root, std paths only — enforced at parse). `[[ban]]` entries with `kind/path/reason/profiles`; `[[pending]]` `fl_sim / fl_math::Pcg32 / Task 10` (the only existing violation found). Resolvability = crate in `cargo tree -p X -e normal` (std/core/alloc/primitives always). `render`, `render_root`, `write`, `drift`, `check`. Drift also flags a hand-written `clippy.toml` in any workspace crate that the template does not produce. CRLF-tolerant comparison.
- `xtask/src/cmd.rs`: cached `cargo metadata --no-deps`, `run_logged` (stdout+stderr into `target/xtask/<step>.log` via a file — not a pipe — so an sccache server spawned by a child cannot hang the read), `cargo_tree(s)` (parallel threads: 5 trees 3.5 s vs 8.8 s sequential), `git`.
- `xtask/src/main.rs`: clap CLI `gate <task|pr|nightly|milestone> [--base main] [--track T]... [--milestone M]`, `arch-check`, `gen-clippy [--check]`.
- Fixtures: `cargo metadata --format-version 1 --no-deps` of the workspace, edited by script: `metadata_client_sim.json` adds fl_client → fl_sim; `metadata_features.json` adds fl_feat_a, fl_feat_b (→ a, no sim_deps), fl_feat_c (→ a, `sim_deps = ["a"]`), fl_feat_a_client.
- `deny.toml`, `.config/nextest.toml` (agent profile verbatim; `[test-groups] gpu`, default-profile override `test(/gpu_/)` → gpu, which applies to all profiles), generated `crates/{fl_math,fl_protocol,fl_view,fl_content,fl_sim}/clippy.toml` and root `clippy.toml`, README gate section rewritten, `scripts/check_deps.sh` deleted.

## TDD evidence
RED — `cargo test -p xtask` with only the test modules written:
```
error[E0425]: cannot find function `check_edges` in this scope   (x3)
error[E0425]: cannot find function `check_tree` in this scope    (x5)
error[E0425]: cannot find function `render` in this scope        (x6)
error[E0425]: cannot find function `touched_packages` in this scope (x2)
error[E0422]: cannot find struct ... `GateReport` / `Violation`
error[E0433]: cannot find type `Level` / `Outcome` / `Template`
error: couldn't read `xtask\src\../clippy-template.toml`
error: could not compile `xtask` (bin "xtask" test) due to 24 previous errors
```
Expected: the items did not exist yet.

GREEN — `cargo test -p xtask`:
```
running 13 tests
test arch::tests::edges_accept_fixture_workspace ... ok
test arch::tests::edges_flag_client_to_sim ... ok
test arch::tests::feature_edge_needs_declared_dep ... ok
test arch::tests::tree_allows_ecs_family_in_content ... ok
test arch::tests::tree_flags_any_bevy_in_pure_crate ... ok
test clippy_gen::tests::pending_entry_is_omitted_with_comment ... ok
test clippy_gen::tests::render_omits_unresolvable_paths ... ok
test gate::tests::report_renders_summary_line ... ok
test gate::tests::skips_and_warnings_still_pass ... ok
test gate::tests::fail_tail_is_capped_and_budget_warned ... ok
test arch::tests::tree_flags_render_in_headless_crate ... ok
test changed::tests::root_manifest_touches_everything ... ok
test changed::tests::touched_maps_files_to_packages ... ok
test result: ok. 13 passed; 0 failed
```
(11 brief tests + 2 extra: tail cap/budget warning, skip/warn still pass.)

## Step 5 tool checks
- `cargo xtask arch-check` → `arch-check: OK (12 packages)` (~9 s)
- `cargo xtask gen-clippy --check` → `gen-clippy: up to date (6 files)` (~5.6 s)
- `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (~9.5 s warm). `deny-multiple-versions` verified to work: temporarily adding `{ crate = "windows-sys", deny-multiple-versions = true }` gave `error[duplicate]: found 4 duplicate entries for crate 'windows-sys'` / `bans FAILED` (reverted).
- Primitive ban: `pub fn _probe(x: f32) -> f32 { x }` in fl_view → `cargo clippy -p fl_view -- -D warnings`:
  ```
  error: use of a disallowed type `f32`
   --> crates\fl_view\src\lib.rs:2:18
    = note: sim state is Fx and integers only (spec §5.4)
    = note: `-D clippy::disallowed-types` implied by `-D warnings`
  ```
  (two errors, arg and return). Line removed; `git diff crates/fl_view` empty. Clippy CAN ban primitives — no §12.2 gap.

## Step 6 gate (`cargo xtask gate task --track contract`)
Steady state (final run, nothing changed since previous run):
```
PASS fmt (1.4s)
PASS clippy-drift (2.1s)
PASS clippy (3.4s)
PASS arch-check (4.4s)
PASS test-headless (11.3s)
WARN test-client (4.5s): no test matched `not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))`
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 27s
```
test-headless ran 169 tests across 18 binaries (all passed; includes xtask's 13). The WARN is genuine: fl_client and fl_game have no tests yet.

After an xtask source change (warm deps): `gate task: PASS in 167s` (clippy 70.8 s, test-headless 44.3 s; machine shared with lane 2).

First (cold) run: `gate task: PASS in 2141s` with `WARN gate task took 2141s (budget 300s)` — test-client 1627 s, clippy 272 s, test-headless 229 s. Cause: per-package selection (`-p fl_client -p fl_game`, `-p <headless>`, clippy without probes) resolves different Bevy features than Task 1's `--workspace` builds (probes add `bevy_remote` etc.), so Bevy was rebuilt once per new feature set in this build dir. One-time per build dir/feature set.

## Concerns / decisions to confirm
1. **Cold-cache cost:** the first gate run in any fresh build dir (or after a feature change) far exceeds 300 s because of the per-package feature sets above. Steady state is 27 s. Other worktrees' first gate run will pay this (sccache may help across worktrees).
2. **On m0/phase-a every task gate covers the whole workspace**: root Cargo.toml/Cargo.lock differ from `main` since Task 1, and they touch every package. Expected per the brief, but it means `gate task` == near-`pr` scope until phase-a merges.
3. **deny.toml policy additions beyond the brief's list** (needed for "advisories ok … licenses ok"):
   - advisory ignores with reasons: RUSTSEC-2026-0344 (ruzstd 0.8 decompression bomb via bevy_image KTX2; fixed 0.9.1 is not semver-compatible with bevy 0.20), RUSTSEC-2026-0192 (ttf-parser unmaintained via winit/sctk-adwaita), RUSTSEC-2023-0089 (atomic-polyfill unmaintained via postcard → heapless 0.7, non-atomic targets only).
   - licences `MIT-0` (encase*), `BSL-1.0` (xxhash-rust, clipboard-win, error-code); exception `OFL-1.1` + `Ubuntu-font-1.0` for `epaint_default_fonts` only; `unused-allowed-license = "allow"` (MPL-2.0 not in tree yet).
   - Bevy duplicate check is 74 explicit `{ crate = "bevy…", deny-multiple-versions = true }` entries from today's Cargo.lock (cargo-deny has no glob). A new bevy* crate needs a new line.
4. **Small extensions of the brief:** `rust-toolchain.toml` and root `clippy.toml` also count as workspace-wide in `touched_packages`; probes are excluded from `clippy` at every level (brief says "workspace minus probes" for pr; at task a root-manifest change would otherwise lint probes too); `xtask` is in the headless test set (so its own tests run when touched); test-client runs every client package (fl_client, fl_game, fl_feat_*_client) when any is touched.
5. Fixtures use `--no-deps` metadata (60 KB) rather than full metadata (4 MB); check_edges only needs workspace packages.
6. `gate.rs` uses `std::time::Instant` (banned by the root client profile) under a scoped `#[expect(clippy::disallowed_types, reason = …)]` on `gate::run`; `Ctx::tracks` has a scoped `#[expect(dead_code)]` until Task 3's check-scope row reads it.
7. `clippy::float_arithmetic = "deny"` for sim crates (global constraints) is not in this task's file list (crate Cargo.toml lints) and was not added.
8. fl_sim's file includes `rand::rng`/`rand::random` because rand 0.10 is in its normal graph via bevy_ecs (`debug` feature); clippy accepted the paths (no unresolved-path warning).

## Files changed
`.config/nextest.toml`, `README.md`, `clippy.toml`, `crates/{fl_content,fl_math,fl_protocol,fl_sim,fl_view}/clippy.toml`, `deny.toml`, `scripts/check_deps.sh` (deleted), `xtask/clippy-template.toml`, `xtask/src/{arch,changed,clippy_gen,cmd,gate,main}.rs`, `xtask/tests/fixtures/{metadata_ok,metadata_client_sim,metadata_features}.json`.

## Self-review
- Fixed during review: WARN rows printed a blank line after the message (seen in the first gate output); now `trim()`ed.
- No new dependencies; Cargo.toml/Cargo.lock untouched. Untracked `docs/research/*.md` (not mine) left uncommitted.

---

# Fix round 1

Commit: `718ed42 fix(xtask): all-features arch tree, unified task build, tighter drift checks` (`Refs: ADR-0009`)

## Changes per finding
1. **arch tree sees every feature and target.** `arch::TREE_ARGS = ["-e", "normal,build", "--all-features", "--target", "all"]` is used for every pure/content/headless crate (`cmd::cargo_tree(s)` now take extra args; the clippy_gen resolvability tree stays `-e normal`). New test `tree_args_see_feature_gated_and_target_specific_deps` builds a scratch workspace in a tempdir: its `fl_sim` has `bevy_render` behind a non-default feature and `wgpu` in a `[target.'cfg(target_os = "android")'.dependencies]` table. It runs the real `cargo tree`: default args give 0 violations, and `TREE_ARGS` gives both (`fl_sim must not depend on bevy_render v0.1.0`, `… wgpu v0.1.0`).
2. **R23 unified task build.** At task level, `clippy` and both nextest rows use `--workspace --exclude probe_dynlink --exclude probe_remote` (probes are found from metadata), so there is one feature-unified build. Clippy covers every crate, which closes the reverse-dependents gap; it is skipped only when nothing is touched. Which tests run is chosen by filter: test-headless runs `(package(=touched…) | ((package(=fl_sim) | package(=fl_test_utils)) & test(/smoke_/))) & not (excl)`, and test-client runs `package(=touched client…) & not (excl)` (skipped if no client crate is touched). pr/nightly/milestone keep the isolated `-p <headless…>` and `-p <client…>` runs. Clippy at pr/milestone uses the same workspace-minus-probes selection. A pure `task_filter` has a test (`task_filter_selects_touched_and_smoke`).
3. **R25 advisories.** In the root Cargo.toml I removed `ktx2` and `zstd_rust` from the Bevy feature list, plus `smaa_luts` and `tonemapping_luts`. Those two force `ktx2` + `bevy_image/zstd`, and `zstd` without a backend is a `compile_error!` in bevy_image. postcard is now `default-features = false, features = ["alloc"]`. Cargo.lock lost atomic-polyfill, heapless, hash32, spin and ktx2. The atomic-polyfill ignore is deleted.
   - **The ruzstd ignore (RUSTSEC-2026-0344) is still needed.** bevy_egui 0.43's `render` feature declares `bevy_image = { features = ["zstd_rust"] }`, so ruzstd stays in the tree. bevy_image calls ruzstd only from its `ktx2` module (`#[cfg(feature = "ktx2")] mod ktx2;`), which is now compiled out, so the vulnerable decoder is unreachable. The reason in deny.toml now says this.
   - The ttf-parser ignore (via winit) stays unchanged.
   - `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`.
   - **Client follow-up:** without `tonemapping_luts`, Bevy's default `Tonemapping::TonyMcMapface` has no LUT. Client cameras should set a non-LUT tonemapper (e.g. `Reinhard` or `None`), otherwise Bevy warns at runtime. This is noted in the Cargo.toml comment, and no client code was changed.
4. **Stray clippy configs.** Drift now lists every `clippy.toml` or `.clippy.toml` from `git ls-files --cached --others --exclude-standard` that exists and is not generated. Test: `stray_clippy_config_anywhere_is_drift`. Manual check: `touch crates/clippy.toml` → `gen-clippy: FAIL, 1 drifted or stray clippy configs …  crates/clippy.toml`, exit 1; removing it → `up to date (6 files)`.
5. **`[[pending]]` validation.** Parse fails if the crate has no non-root profile, or if the path is not a ban in that crate's profile. `generate` also fails if the crate is not a workspace package. Test: `pending_entries_are_validated` (fl_client, fl_nope, fl_math/f32, fl_sim/f64 rejected; fl_sim/f32 accepted).
6. **Setup failures.** `gate::run_with_setup(level, setup)` turns a metadata or merge-base error into a `FAIL setup (…)` row and the usual `gate <level>: FAIL (setup) in <n>s` line, with exit 1. Test: `setup_failure_still_prints_summary`.
7. **Dev-dependency rule.** Pure, content and headless crates may not dev-depend on a client crate (fl_client, fl_game, fl_feat_*_client). Client crates still have all their dev edges checked. Tests use JSON-edited fixtures (`fixture::metadata_edited`, `add_dep`, `add_package`):
   - `headless_dev_dep_on_client_flagged` (fl_sim dev → fl_client)
   - `headless_dev_dep_on_headless_allowed`
   - `client_dev_dep_on_sim_flagged`
   - `unknown_crate_flagged`
   - `probes_are_skipped` (probe → xtask, plus a new probe → fl_client)

R26: no change. Only direct edges are checked.

## Tests
`cargo test -p xtask` → `test result: ok. 23 passed; 0 failed` (13 earlier + 10 new). `cargo clippy -p xtask --all-targets -- -D warnings` is clean, and `cargo fmt --all --check` is clean.
Tools: `cargo xtask arch-check` → `arch-check: OK (12 packages)`. `cargo xtask gen-clippy --check` → `gen-clippy: up to date (6 files)`. `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`.

## Gate timings (`cargo xtask gate task --track contract`, machine shared with lane 2's builder)
- **Cold** (fresh `CARGO_BUILD_BUILD_DIR=C:/t/fl/t2-cold`, warm sccache): `gate task: PASS in 2615s` (wall 2857 s including the xtask build). clippy took 876 s, test-headless 1523 s and arch-check 142 s (package-cache lock waits). A cold build dir still compiles Bevy twice: once as clippy check metadata and once as the test build. clippy-driver output cannot be reused for codegen, so this floor stays even with one unified feature set. Before the fix there were three feature sets plus the per-package runs. The scratch build dir (3.7 GB) was deleted afterwards.
- **After the Bevy feature change, regular build dir:** `gate task: PASS in 1132s` (test-headless 937 s: Bevy rebuild).
- **Warm (pasted):**
```
PASS fmt (2.9s)
PASS clippy-drift (2.3s)
PASS clippy (11.9s)
PASS arch-check (5.7s)
PASS test-headless (18.7s)
WARN test-client (8.4s): no test matched `(package(=fl_client) | package(=fl_game)) & not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))`
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 50s
```
test-headless: `Starting 179 tests across 18 binaries (3 binaries skipped)` → `179 tests run: 179 passed`. The test-client WARN is genuine: fl_client and fl_game have no tests yet.

## Remaining concerns
- The cold build-dir floor (two Bevy compiles) is still well above 300 s. Only the warm gate is within budget.
- The ruzstd ignore remains because bevy_egui forces it (see 3).
- Removing tonemapping_luts and smaa_luts needs a client-side tonemapper choice (see 3).
- Cargo.toml and Cargo.lock were edited for R25 only (feature list and postcard).

---

# Fix round 2 (ruling R29)

Commit: `fix(build): restore bevy LUT features` (Refs: ADR-0009)

- **Bevy features restored.** `ktx2`, `smaa_luts`, `tonemapping_luts` and `zstd_rust` are back in the root Bevy feature list. The list is now identical to 0d1f1f2; the only Cargo.toml differences are the comment and postcard. Cargo.lock regains `ktx2 v0.5.0`.
- **postcard stays without default features.** `postcard = { default-features = false, features = ["alloc"] }` is kept, so atomic-polyfill, heapless, hash32 and spin stay out of the lock file. The atomic-polyfill ignore stays deleted.
- **RUSTSEC-2026-0344 (ruzstd) reason now reads:** "only Bevy's embedded, trusted LUT KTX2 files are decoded; no untrusted KTX2 is loaded (M0 uses primitives)". The text also says no semver-compatible fixed version exists, and to revisit on the next Bevy upgrade. The ttf-parser ignore is unchanged.
- **Client concern withdrawn.** Bevy's default TonyMcMapface tonemapper works again, so client cameras need no special tonemapper.
- **cargo deny:** `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`.

Gate (`cargo xtask gate task --track contract`):
- **First run after the feature change:** `gate task: PASS in 480s`. test-headless took 442 s because Bevy had to rebuild; this run also printed `WARN gate task took 480s (budget 300s)`.
- **Warm run (pasted):**
```
PASS fmt (1.1s)
PASS clippy-drift (1.9s)
PASS clippy (4.4s)
PASS arch-check (5.3s)
PASS test-headless (14.8s)
WARN test-client (5.4s): no test matched `(package(=fl_client) | package(=fl_game)) & not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))`
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 33s
```
- **Tests:** 179/179 headless tests passed.
