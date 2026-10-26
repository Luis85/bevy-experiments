# Task 3 report: `OWNERS.toml`, `check-scope`, `test-guard`, `bless`

Status: DONE_WITH_CONCERNS
Commit: `4b2bf08 build(xtask): OWNERS.toml, check-scope, test-guard, bless` (trailers `Refs: ADR-0011, ADR-0009`, `Co-Authored-By`)

## What was implemented
- **`OWNERS.toml`**: the brief's data verbatim (track order, `[protected]`). I added a header comment and a comment line for the `feature:<name>` convention.
- **`xtask/src/owners.rs`**:
  - `Owners { tracks: Vec<(String, GlobSet)>, protected: GlobSet }`, plus `load_owners(root)`, `parse_owners(text)`, `track_of`, `is_protected`, `track_names` and `normalize`.
  - Globs are built with `literal_separator(true)`, so `*` stays inside one segment (`crates/*/clippy.toml` does not match nested files) and `**` spans segments.
  - Paths are normalised to `/` and lose any leading `./`.
  - Parsing uses `deny_unknown_fields`, and rejects duplicate, empty or whitespace-padded track names and tracks with no paths.
  - `is_protected` has `#[cfg_attr(not(test), expect(dead_code, reason = "read by the protected-path hook (Task 4)"))]`, because Task 4's `guard_edit` is its first non-test caller.
- **`xtask/src/scope.rs`**:
  - `check_scope(owners, files, tracks)` returns the out-of-scope files with their owner (`None` = unowned).
  - `effective_tracks(cli, FL_TRACK)`: the `--track` values, otherwise the tracks in `FL_TRACK` separated by commas or spaces.
  - `run(root, base, tracks)` works on merge base ∪ committed/staged/unstaged/untracked files (Task 2's `changed_files`). An empty track list or an unknown track name is an error that lists the defined tracks.
  - `ScopeReport::render` prints `check-scope: OK (<n> files in <tracks>)`, or `check-scope: FAIL` followed by `  <file> → <owner|unowned>` lines.
- **`xtask/src/test_guard.rs`**:
  - Provides `FindingKind` (the 9 kinds in the brief's order), `Finding { file, line, kind, text }`, `Finding::render` (`TEST-GUARD <file>:<line> <kind>: <text>`), `summary(n)`, `scan_diff(diff)` and `run(root, base_rev)`.
  - `run` diffs `git -c core.quotepath=off diff --no-color --no-ext-diff --no-renames --src-prefix=a/ --dst-prefix=b/ -U20 <base_rev> -- '*.rs'`. Untracked `.rs` files are appended as synthetic new-file diffs.
  - **Deviation:** I use `-U20` instead of the brief's `-U0`, so that a multi-line assert or attribute around a changed line is visible as a whole (see below). `scan_diff` also accepts `-U0` input, and the brief's tests use `-U0` fixtures.
  - **How it scans:**
    - A small lexer drops comments and makes string and char literals opaque (raw strings included). So `// assert!(x)` is not an assert, and `"#[ignore]"` is not an attribute.
    - Each hunk is analysed as an old side (context + removed) and a new side (context + added).
    - Every `.rs` file is scanned, not just `tests/`, because unit tests live in `src/`.
  - **Rules:**
    - **RemovedAssert:** finds every macro whose name contains `assert` (`assert*!`, `debug_assert*!`, `prop_assert*!`, `insta::assert_*!`). Each invocation is bracket-balanced across lines and compared without whitespace. An old-side invocation that the diff touched is flagged unless a touched new-side invocation has the same text (a multiset across the whole diff). So moved or reformatted asserts stay quiet, while a weakened continuation line of a multi-line `assert_eq!(…)` is flagged.
    - **NewIgnore / NewShouldPanic:** an added or changed `#[ignore…]` or `#[should_panic…]`, also inside `#[cfg_attr(…)]`.
    - **ToleranceChanged:** a changed binding or field whose identifier contains the word alpha, tol, tolerance, eps, epsilon, ulps, rtol or atol, in the forms `NAME = v`, `NAME: T = v` or `NAME: <number>`. Identical (name, value) pairs cancel each other out. Client crates are skipped (UI `alpha:` values).
    - **FewerProptestCases:** `cases: n`, `cases = n` or `with_cases(n)`, paired per file in order. A lower or removed value is flagged.
    - **RemovedTest:** a `#[test]`, `#[*::test]`, `#[rstest]`, `#[test_case]` or `#[proptest]` function, or a proptest function with `x in strategy` parameters. It is flagged when the diff touched the old-side function (attribute or `fn` line) and no touched new-side test has the same name, which covers deletion, renaming and de-attributing. A removed `#[cfg(test)]` line, or a test `mod x;` line, that is not re-added in the same file is flagged too.
    - **NetTestCountDrop:** fires once, at the first removed test, when more test functions are removed than added (the text is `net test count falls by N (R removed, A added)`).
    - **NewSeedOverride:** one of these on an added line:
      - an override identifier: `rng_seed`, `RngSeed`, `shuffle_seed`, `from_seed`, `seed_from*`, `with_seed` or `new_with_rng`
      - `.env(…)`, `.envs(…)` or `set_var(…)` with `FL_SHUFFLE_SEEDS` or `PROPTEST_RNG_SEED`
      - a literal seed (`MatchSeed(7)`, `seed: 7`) in a file where a literal seed line was removed, i.e. a changed seed. A fixed seed in a brand-new test does not fire.
    - **MockInSimTest:** an added identifier containing `mock`, `fake` or `stub` under `fl_sim`, `fl_test_utils`, `fl_feat_*` or `features/`, except `*_client` paths.
  - **Line numbers:** findings on removed code cite the merge-base line; findings on added code cite the working-tree line. Output is sorted by (file, line, kind) and de-duplicated per (file, line, kind).
- **`xtask/src/bless.rs`**:
  - `bless(root, paths, reason)` reads `FL_ROLE`. `bless_as(role, …)` is the testable core, so the tests never mutate the process environment.
  - It refuses `FL_ROLE=implementer` (the message points to the `BLOCKED:` escape hatch) and an empty or blank reason.
  - Each path may be a directory (searched recursively for `*.new`, skipping `target`/`.git`), a `*.new` file, or a file whose `<file>.new` sibling exists. A path with nothing to promote is an error.
  - All candidates are collected before anything is renamed. Each is renamed to its name without `.new`, replacing the old file. Returns the promoted paths relative to `root`, sorted.
- **`xtask/src/main.rs`**: subcommands
  - `check-scope --track <t>… [--base main]`: exits 1 on FAIL; falls back to `FL_TRACK`.
  - `test-guard [--base main]`: always exits 0 unless git itself fails.
  - `bless <paths>… --reason "<text>"`: paths are relative to the cwd. Prints `bless: <n> files promoted`, one `  <path>` line per file, then `Blessed: <reason>`.
- **`xtask/src/gate.rs`**:
  - Two rows, placed after clippy-drift, at levels `BRANCH = [Task, Pr]`:
    - `check-scope`: Skip with `no --track given and FL_TRACK is unset`, otherwise Fail on any out-of-scope file.
    - `test-guard`: Warn on any finding.
  - Row messages are capped by `capped()` (a header plus 38 detail lines plus `… and N more (`cargo xtask …` lists all)`), so the header survives the 40-line tail.
  - The `#[expect(dead_code)]` on `Ctx::tracks` is removed.
- **Fixtures**: `xtask/tests/fixtures/guard/*.diff` (11 files).

## TDD evidence
RED: I wrote the test modules (owners, scope, bless, test_guard) and declared the modules, then ran `cargo test -p xtask`:
```
error[E0432]: unresolved import `crate::owners::parse_owners`
 --> xtask\src\scope.rs:4:9
error[E0432]: unresolved import `FindingKind`
 --> xtask\src\test_guard.rs:4:9
warning: unused import: `super::*`   (x4: bless, owners, scope, test_guard)
error: could not compile `xtask` (bin "xtask" test) due to 2 previous errors; 4 warnings emitted
```
This was expected: none of the items existed yet. Resolution stops at the imports; the remaining missing items would surface next.

GREEN: `cargo test -p xtask` gives `test result: ok. 45 passed; 0 failed`. That is 23 earlier tests plus 22 new:
- owners: first_matching_track_wins, protected_globs, duplicate_track_is_rejected
- scope: unowned_path_reported, scope_flags_out_of_track_file, tracks_fall_back_to_fl_track
- test_guard:
  - the brief's guard_* tests: removed_assert, new_ignore_and_should_panic, fewer_proptest_cases, alpha_change, removed_test_fn, mock_in_sim_test, quiet_on_added_asserts
  - extras: weakened_multiline_and_commented_out_asserts, quiet_on_moved_and_reformatted_test, counts_proptest_fns_in_deleted_file, flags_seed_overrides
- bless: promotes_new_files, accepts_a_file_or_its_new_sibling, requires_reason, refuses_implementer_role
- gate: capped_detail_keeps_header_in_the_tail

The guard tests assert the exact `(file, line, kind)` list, so an extra or missing finding fails them. `cargo clippy -p xtask --all-targets -- -D warnings` is clean, and `cargo fmt --all --check` is clean.

## Step 5 tool checks
- `cargo xtask check-scope --track contract --base HEAD` (before the commit, so it covers only this task's changes) → `check-scope: OK (18 files in contract)`, exit 0.
- `cargo xtask check-scope --track contract` (base `main`) → `check-scope: FAIL` with 24 files, exit 1. See concern 1.
- `cargo xtask test-guard` (base `main`) → `test-guard: 0 findings (reviewer must accept each)`, exit 0. This is correct: the whole branch diff against main has only 2 deleted `.rs` lines and no removed test code. `--base HEAD` also gives 0.
- **Live probe**, with every change reverted afterwards (`git status` clean apart from the task files):
  - In `xtask/src/changed.rs` I changed `assert_eq!(touched_packages(...), ["fl_sim"])` to `assert!(true)` and added `#[ignore]`.
  - I added an untracked `crates/fl_sim/tests/zz_guard_probe.rs` with `struct FakeClock;` and `#[cfg_attr(windows, ignore)]`.
  - Result:
    ```
    TEST-GUARD crates/fl_sim/tests/zz_guard_probe.rs:1 MockInSimTest: struct FakeClock;
    TEST-GUARD crates/fl_sim/tests/zz_guard_probe.rs:3 NewIgnore: #[cfg_attr(windows, ignore)]
    TEST-GUARD xtask/src/changed.rs:91 RemovedAssert: assert_eq!(touched_packages(&meta, &files), ["fl_sim"]);
    TEST-GUARD xtask/src/changed.rs:95 NewIgnore: #[ignore]
    test-guard: 4 findings (reviewer must accept each)
    ```
  - Exit code 0.
- `bless` CLI (in a scratch dir under `target/`, removed afterwards):
  - `cargo xtask bless target/blesstest --reason "probe run"` → `bless: 1 files promoted` / `  target/blesstest/snapshots/a.snap` / `Blessed: probe run`, exit 0.
  - With `FL_ROLE=implementer` → `error: FL_ROLE=implementer may not bless protected artifacts; …`, exit 1.
  - `--reason " "` → `error: bless needs a non-empty --reason …`, exit 1.
  - With no arguments → clap usage error, exit 2.

## Gate
`cargo xtask gate task --track contract --base 97ed4a6` (97ed4a6 = phase-a HEAD when this task started, so this is the task's own diff):
```
PASS fmt (1.1s)
PASS clippy-drift (3.0s)
PASS check-scope (0.3s)
PASS test-guard (0.3s)
PASS clippy (24.3s)
PASS arch-check (5.4s)
PASS test-headless (51.5s)
SKIP test-client: no client package touched since 97ed4a6
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 87s
```
`cargo xtask gate task --track contract` (default base `main`):
```
PASS fmt (1.1s)
PASS clippy-drift (6.4s)
FAIL check-scope (2.0s)
    24 of 68 changed files are outside track(s) contract:
      .gitignore → docs
      README.md → docs
      crates/fl_cli/Cargo.toml → tools
      crates/fl_client/Cargo.toml → client
      crates/fl_client/src/game.rs → client
      crates/fl_client/src/lib.rs → client
      crates/fl_content/Cargo.toml → content
      crates/fl_game/Cargo.toml → client
      crates/fl_game/src/lib.rs → client
      crates/fl_observe/Cargo.toml → tools
      crates/fl_observe/src/lib.rs → tools
      crates/fl_sim/Cargo.toml → sim-core
      crates/fl_test_utils/Cargo.toml → sim-core
      docs/research/agent-dev-environment.md → probes
      docs/research/architecture-best-practices.md → probes
      docs/research/baseline-recommendations.md → probes
      docs/research/bevy020-api-reference.md → probes
      docs/research/workflow-and-verification.md → probes
      docs/superpowers/plans/2026-10-23-frontline-m0-rev2.md → docs
      probes/dynlink/Cargo.toml → probes
      probes/dynlink/src/main.rs → probes
      probes/remote/Cargo.toml → probes
      probes/remote/src/main.rs → probes
      scripts/check_deps.sh → unowned
PASS test-guard (0.9s)
PASS clippy (6.6s)
PASS arch-check (4.4s)
PASS test-headless (17.0s)
WARN test-client (6.8s): no test matched `(package(=fl_client) | package(=fl_game)) & not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))`
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: FAIL (check-scope) in 46s
```
test-headless: `Starting 201 tests across 18 binaries` → `201 tests run: 201 passed`.

## Concerns
1. **On `m0/phase-a`, check-scope against `main` fails for every task.** The branch carries Tasks 1, 2 and 26, the research docs and the plan. Those files belong to client, tools, sim-core, probes and docs, and `scripts/check_deps.sh` (deleted in Task 2) is unowned. The tool behaves correctly; the base is what is wrong for an integration branch. Options for the controller:
   - (a) run task gates with `--base <commit the task started from>`, which is what I did for the PASS above;
   - (b) give Task 4's Stop hook a way to pass that base (e.g. an `FL_BASE` env var);
   - (c) accept the FAIL until phase-a merges.
   Step 5's expected `check-scope: OK` and `gate task: PASS` hold only under (a).
2. **test-guard is a textual proxy and misses some weakenings**, in decreasing order of concern:
   - an early `return;` placed before untouched asserts;
   - test removal through `Cargo.toml` (`[[test]]`, `autotests`, `harness`), because only `.rs` files are scanned;
   - narrowed proptest strategies (`0..1000` → `0..10`);
   - a multi-line assert whose opening line is more than 20 lines away from the change (outside the `-U20` context).
   It errs towards noise: a renamed test reports RemovedTest, adding a message to an assert reports RemovedAssert, and any new `#[should_panic(expected = …)]` change is flagged. Findings are only warnings for the reviewer.
3. `test_guard.rs` is 1354 lines after rustfmt (about 1000 of implementation, 300 of tests). It has one responsibility (lexer → hunk sides → rules → pairing), but it is the largest xtask file.
4. `-U20` instead of the brief's `-U0` (explained above). Fixtures and `scan_diff` work with both.
5. Small additions beyond the brief:
   - `FL_TRACK` fallback in the CLI as well as the gate row;
   - unknown-track error;
   - `bless` accepts a file whose `.new` sibling exists, and lists the promoted files between its two brief lines;
   - the `capped()` message limit in the gate rows.

## Files changed
`OWNERS.toml`, `xtask/src/{owners,scope,test_guard,bless}.rs` (new), `xtask/src/{main,gate}.rs`, `xtask/tests/fixtures/guard/{removed_assert,ignore_should_panic,proptest_cases,alpha,removed_test,mock_in_sim,added_asserts,multiline_assert,moved_test,proptest_fn,seed_override}.diff`. No Cargo.toml or Cargo.lock change (globset, toml, serde and tempfile were already xtask dependencies). LF line endings throughout.

## Self-review
- **Fixed while working:** shell heredocs collapsed `\\` in two test literals, and a Python edit of `gate.rs` wrote `…` in cp1252 and turned `'\n'` into literal newlines. Both were repaired byte-exactly. The file is valid UTF-8 with LF, and the diff shows only the intended hunks.
- **Self-flag check:** `test-guard` is quiet on its own source, because its keyword tables are string literals, which the lexer treats as opaque.

---

# Fix round 1

Commit: `494eae2 fix(xtask): whole-file lexing, cfg/proptest weakening, protected-path row, gate base` (trailers `Refs: ADR-0009, ADR-0011`, `Co-Authored-By`)

## Changes per finding
1. **Whole-file lexing.**
   - `test_guard::run` takes only the changed line numbers from `git diff -U0 <base> -- '*.rs'` (untracked `.rs` files are added as new-file diffs).
   - Each changed file is then lexed whole on both sides: the old side from `git show <base>:<path>`, the new side from the working tree. The `-U20` reach limit is gone.
   - `scan_with_sources(diff, source)` is the core. `scan_diff(diff)` (the brief's interface) rebuilds each side from the hunk lines and is used by the fixture tests.
   - New test `guard_lexes_whole_files_across_raw_strings`. Its diff starts inside an `r##"…"##` RON string that contains `\"`, an unbalanced `(`, and `#[test]`/`assert!`/`fake_unit` look-alikes.
     - Lexed whole: exactly `scenario.rs:13 RemovedAssert`.
     - The same test asserts that the hunk-only rebuild finds nothing, which documents the bug being fixed.
   - Split into `xtask/src/test_guard/{mod,diff,lex,side,patterns}.rs` (804/171/205/440/274 lines).
2. **cfg-disabled tests → `NewIgnore`.** Attributes are grouped into stacks, each with the `fn` or `mod` item it annotates. These are flagged:
   - an added or changed `#[cfg(…)]` other than exactly `cfg(test)` on a test function's stack;
   - the same on a test module (its name contains `test`, it is in `tests/`, or its stack has `cfg(test)`);
   - an added inner `#![cfg(…)]` in a `tests/` file.

   Fixture `cfg_disabled.diff`:
   - `#[cfg(not(windows))]` above `#[test]` → `NewIgnore`.
   - `#[cfg(test)]` → `#[cfg(any())]` on `mod more_tests` → `NewIgnore`, plus `RemovedTest` for the removed `cfg(test)` line.
   - `#![cfg(feature = "never")]` in `tests/combat.rs` → `NewIgnore`.
   - Quiet negatives: `#[cfg(feature = "trace")]` on a non-test fn, and a new `#[cfg(test)] mod extra_tests`.
3. **FewerProptestCases.**
   - Counts are paired by owner: the innermost enclosing fn (including its attribute stack), else the enclosing `proptest!` block (keyed by its first fn), else the file.
   - An added count below 256 with no removed partner is flagged when the file existed at the base.
   - A line that sets `PROPTEST_CASES` (via `.env(` / `.envs(` / `set_var`) is flagged.
   - Fixtures:
     - `proptest_pairing.diff`: two blocks swapped, so the counts move between positions → quiet (index pairing would have flagged 64 < 1024).
     - `proptest_added.diff`: a new `with_cases(16)` block in an existing file → flagged; `cmd.env("PROPTEST_CASES", "8")` → flagged; `cases: 16` in a new file → quiet.
4. **Base resolution (R33).**
   - `changed::base_spec(cli, env, file)` sets the precedence `--base` > `FL_GATE_BASE` > first line of `.gate-base` > `main`, skipping empty values.
   - `changed::resolve_base` returns the spec, its source and `git merge-base <spec> HEAD`.
   - This is used by the gate (touched packages and every row via `Ctx::base_rev`), by `check-scope` and by `test-guard`. Their `--base` is now optional.
   - `/.gate-base` is appended to `.gitignore`.
   - Test `base_precedence_flag_env_file_default`. Live check:
     - `FL_GATE_BASE=97ed4a6 cargo xtask check-scope --track contract` → only `.gitignore → docs`.
     - With a `.gate-base` file containing `97ed4a6` → `check-scope: OK (32 files in contract, docs)`. `git status` does not list the file.
5. **`protected` gate row (R34), `xtask/src/protected.rs`.**
   - The row runs at task and pr level.
   - It checks the changed files since the base (committed, staged, unstaged and untracked) that match `[protected]`, excluding `*.new`.
   - At task level, any such file fails.
   - At pr level, a file fails if:
     - it has an uncommitted change;
     - no commit in `base..HEAD` touches it;
     - any touching commit lacks a `Blessed: <non-empty reason>` line.

     The commits come from `git log --format=%H%x1f%B%x1e base..HEAD -- <file>`.
   - Tests:
     - `task_level_fails_on_changed_snapshot`
     - `pending_new_files_are_ignored`
     - `pr_level_needs_blessed_commits`: a Blessed commit passes; a mix with one unblessed commit fails, naming that commit; an uncommitted change fails.
     - `blessed_trailer_needs_a_reason`
   - Live probe: an untracked `crates/fl_view/tests/snapshots/probe.snap` gave `FAIL protected … probe.snap: changed protected path; only the lead promotes it with cargo xtask bless`. A sibling `pending.snap.new` was ignored. Both files were removed afterwards.
   - `Owners::is_protected` lost its `expect(dead_code)`.
6. **Minor fixes.**
   - **`prop_assume!`:** an added `prop_assume!`/`assume!` with no identical removed one in the same crate is flagged with the new kind `NewAssume` (between `MockInSimTest` and `NetTestCountDrop`).
   - **`--track`:** `--track a,b` now uses `value_delimiter = ','` in `gate` and `check-scope`. `effective_tracks` also splits and trims each CLI value.
   - **`bless`:**
     - It rejects absolute paths and any `..`/root/prefix component.
     - The CLI turns cwd-relative arguments into root-relative ones, so running from a subdirectory works. Output paths are root-relative with `/`.
     - Promotion is all-or-nothing. It collects and validates first (a destination that is a directory is an error), then for each file sets an existing destination aside as `.bless-old` and renames `.new` over it. On any failure every completed step is undone and the backups are restored.
     - `bless()` reads the environment through `bless_with_env(env lookup, …)`.
     - Tests: `bless_reads_role_from_env`, `bless_rejects_absolute_and_parent_paths`, `bless_rolls_back_on_failure` (an injected renamer fails on the second file, and the first file's original and `.new` are both restored).
     - Live check from `target/`:
       - `cargo xtask bless bt --reason probe` → `bless: 1 files promoted` / `  target/bt/s/a.snap`.
       - `../xtask` and an absolute path → rejected, exit 1.
7. **Tolerance vocabulary and crate-keyed cancellation.**
   - Added threshold, margin, slack, delta, significance, confidence, and the fragments max_err/maxerr/p_value/pvalue.
   - To keep `delta` from being noisy, a tracked value must contain a digit or be a plain path (`f64::EPSILON`). So `let delta = target - pos` is quiet.
   - Cancellation of moved asserts, assumes and tests is keyed by the crate directory (the path before the first `src/`, `tests/`, `benches/` or `examples/`) plus the statement or name. I did this rather than defer it, because it was cheap.
   - Fixture `assume_and_tolerance.diff`: `MAX_ERR` and `SIGNIFICANCE` changes are flagged; a new `prop_assume!` is flagged; a moved `prop_assume!` and a non-numeric `delta` are quiet.

**test-guard exit code:** test-guard still exits 1 when git itself fails (fail closed), which is a deviation from "always exits 0". With findings it exits 0.

## Tests
- `cargo test -p xtask` → `test result: ok. 59 passed; 0 failed`. That is 45 earlier tests plus 14 new:
  - test_guard: raw string, cfg, proptest pairing, low added counts and env, assume and tolerance, crate_key
  - protected: 4 tests
  - bless: 3 tests
  - changed: 1 test (base precedence)
  - scope: the `effective_tracks` test was extended
- `cargo clippy -p xtask --all-targets -- -D warnings` is clean; `cargo fmt --all --check` is clean.
- **TDD note:** these tests were written alongside the rework rather than strictly before it. The raw-string test carries its own RED evidence: it asserts that the hunk-only (old) lexing path misses the assert.
- **Live test-guard runs:**
  - `cargo xtask test-guard --base 97ed4a6` → `test-guard: 0 findings (reviewer must accept each)`.
  - `--base HEAD` before the commit → 1 finding: `xtask/src/test_guard.rs:1207 RemovedTest: #[cfg(test)]`. The file moved to `test_guard/mod.rs`, and test gates cancel only within the same file (deliberately, so that removing `mod tests;` in one file cannot be cancelled by an addition elsewhere).

## Gate
`cargo xtask gate task --track contract --base 97ed4a6`. It fails only because the R33 `.gitignore` line belongs to the `docs` track:
```
PASS fmt (4.5s)
PASS clippy-drift (7.6s)
FAIL check-scope (0.4s)
    1 of 32 changed files are outside track(s) contract:
      .gitignore → docs
PASS protected (0.4s)
PASS test-guard (0.9s)
PASS clippy (49.8s)
PASS arch-check (8.0s)
PASS test-headless (23.2s)
SKIP test-client: no client package touched since 97ed4a6 (--base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: FAIL (check-scope) in 96s
```
`cargo xtask gate task --track contract,docs --base 97ed4a6`:
```
PASS fmt (1.5s)
PASS clippy-drift (2.7s)
PASS check-scope (0.2s)
PASS protected (0.3s)
PASS test-guard (0.7s)
PASS clippy (4.3s)
PASS arch-check (5.9s)
PASS test-headless (9.2s)
SKIP test-client: no client package touched since 97ed4a6 (--base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 26s
```
test-headless: `59 tests run: 59 passed, 54 skipped`.

## Remaining concerns
- **`.gitignore` is owned by `docs`.** R33's ignore line makes this task touch two tracks, so it needs `--track contract,docs`. Alternatively the controller can move `.gitignore` into `contract`.
- **Old-side source fallback.** If `git show <base>:<path>` fails, the old side falls back to the hunk-only rebuild, silently.
- **Test gates cancel only per file.** Moving a `#[cfg(test)]` module to another file reports `RemovedTest` (noise, not a miss).
- **Known textual misses** (from the first report, apart from the ones fixed here):
  - an early `return;` before untouched asserts;
  - test removal through Cargo.toml (`[[test]]`, `autotests`, `harness`);
  - narrowed proptest strategies (`0..1000` → `0..10`).
- **New `NewAssume` kind.** This extends the brief's `FindingKind` enum.

---

# Fix round 2 (ruling R35, concern 2)

Commits:
- `93e26cc fix(owners): .gitignore/.gitattributes in contract track` (`Refs: ADR-0011`). `.gitignore` moved from the `docs` track to `contract`, and `.gitattributes` was added to `contract`. `first_matching_track_wins` now asserts that both belong to `contract`.
- `51254b7 fix(xtask): test-guard fails closed when a file side cannot be read` (`Refs: ADR-0009`).
  - `run()` reads only the sides that exist: a file added since the base has no old side, so `git show` is never asked for it.
  - If `git show <base>:<path>` or the working-tree read fails, the scan now returns `Err`, and `cargo xtask test-guard` and the gate row both fail. It no longer falls back to the hunk-only rebuild.
  - The rebuild is now used only by `scan_diff` (fixtures).
  - `scan_with_sources` takes a `Fn(&Path, bool) -> Result<String>` and returns `Result<Vec<Finding>>`.
  - New test `guard_fails_closed_on_unreadable_source`: a failing old-side read gives `Err` containing the git error. A new-file diff never requests its old side and still reports `MockInSimTest`.

Tests: `cargo test -p xtask` → `test result: ok. 60 passed; 0 failed`. clippy `-D warnings` and fmt are clean.

Gate (`cargo xtask gate task --track contract --base 97ed4a6`):
```
PASS fmt (1.8s)
PASS clippy-drift (4.1s)
PASS check-scope (0.4s)
PASS protected (0.3s)
PASS test-guard (0.7s)
PASS clippy (6.7s)
PASS arch-check (5.2s)
PASS test-headless (23.4s)
SKIP test-client: no client package touched since 97ed4a6 (--base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 43s
```
