### Task 3: `OWNERS.toml`, `check-scope`, `test-guard`, `bless`

**Track:** contract · **Refs:** ADR-0011, ADR-0009

**Files:**
- Create: `OWNERS.toml`, `xtask/src/{owners,scope,test_guard,bless}.rs`, `xtask/tests/fixtures/guard/*.diff`
- Modify: `xtask/src/{main,gate}.rs`

**`OWNERS.toml`** (data; tracks are evaluated in file order, and the first match wins):
```toml
[[track]]
name = "contract"
paths = ["crates/fl_math/**", "crates/fl_protocol/**", "crates/fl_view/**",
  "crates/fl_sim/src/{builder,registry,classify,schedule,feature,step}.rs", "crates/fl_sim/src/runner/**", "crates/fl_sim/src/save/**",
  "crates/fl_content/src/{registry,mods,hash,kinds,def}.rs",
  "xtask/**", ".claude/**", "CLAUDE.md", ".mcp.json", "rust-analyzer.toml", "deny.toml", "OWNERS.toml", ".config/**", ".cargo/**",
  "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "clippy.toml", "crates/*/clippy.toml", "docs/superpowers/specs/**", "docs/adr/**"]
[[track]]
name = "perf"
paths = ["docs/perf/**"]
[[track]]
name = "sim-core"
paths = ["crates/fl_sim/**", "crates/fl_catalog/**", "crates/fl_test_utils/**"]
[[track]]
name = "content"
paths = ["assets/**", "mods/**", "crates/fl_content/**"]
[[track]]
name = "client"
paths = ["crates/fl_client/**", "crates/fl_game/**"]
[[track]]
name = "tools"
paths = ["crates/fl_cli/**", "crates/fl_observe/**", "debug.ron"]
[[track]]
name = "probes"
paths = ["probes/**", "docs/research/**"]
[[track]]
name = "docs"
paths = ["docs/**", "README.md", ".gitignore"]

[protected]
paths = ["**/tests/acceptance/**", "**/tests/holdout/**", "**/tests/replays/**", "**/snapshots/**", "scenarios/**",
  "docs/design/mechanics/**", "docs/perf/baselines/**", "docs/perf/budgets.ron"]
```
Each feature adds `feature:<name>` with `features/<name>/**` (M4 onward).

**Interfaces:**
- Produces:
```rust
pub struct Owners { /* tracks: Vec<(String, GlobSet)>, protected: GlobSet */ }
pub fn load_owners(root: &Path) -> anyhow::Result<Owners>;
impl Owners { pub fn track_of(&self, path: &Path) -> Option<&str>; pub fn is_protected(&self, path: &Path) -> bool; }
pub fn check_scope(owners: &Owners, files: &[PathBuf], tracks: &[String]) -> Vec<(PathBuf, Option<String>)>;   // out-of-scope files and their owner (None = unowned)
pub enum FindingKind { RemovedAssert, NewIgnore, NewShouldPanic, ToleranceChanged, FewerProptestCases, RemovedTest, NewSeedOverride, MockInSimTest, NetTestCountDrop }
pub struct Finding { pub file: PathBuf, pub line: u32, pub kind: FindingKind, pub text: String }
pub fn scan_diff(diff: &str) -> Vec<Finding>;      // `git diff -U0 <base> -- '*.rs'`
pub fn bless(root: &Path, paths: &[PathBuf], reason: &str) -> anyhow::Result<Vec<PathBuf>>;
//   promotes `*.snap.new` → `*.snap` and any `*.new` → name without `.new` under the given paths;
//   Err on an empty reason or when env FL_ROLE=implementer
```
- CLI and output formats:
  - `cargo xtask check-scope --track <t>… [--base main]` prints `check-scope: OK (<n> files in <tracks>)`, or `check-scope: FAIL` followed by `  <file> → <owner|unowned>` lines, exiting 1.
  - `cargo xtask test-guard [--base main]` prints one `TEST-GUARD <file>:<line> <kind>: <text>` line per finding and ends with `test-guard: <n> findings (reviewer must accept each)`. It always exits 0.
  - `cargo xtask bless <paths>… --reason "<text>"` prints `bless: <n> files promoted` and `Blessed: <text>` (the trailer to paste into the commit).
- **`NetTestCountDrop`** stands in for spec §12.5's "falling `nextest list` count", which would need a build of the merge base. It fires when the diff removes more `#[test]` and `proptest!` functions than it adds.
- **Gate rows:**
  - `check-scope` (task, pr): Skip when no `--track` is given and `FL_TRACK` is unset; otherwise a finding Fails.
  - `test-guard` (task, pr): any finding is a `Warn`.

- [ ] **Step 1: Write the failing tests** (unit tests with fixture diffs; bless tests use `tempfile` dirs):
```rust
#[test] fn first_matching_track_wins()       // crates/fl_sim/src/builder.rs → "contract"; crates/fl_sim/src/movement.rs → "sim-core"
#[test] fn unowned_path_reported()           // check_scope(["foo/bar.rs"], ["sim-core"]) == [("foo/bar.rs", None)]
#[test] fn scope_flags_out_of_track_file()   // ["crates/fl_client/src/view.rs"] with ["sim-core"] → owner Some("client")
#[test] fn protected_globs()                 // protected: crates/fl_sim/tests/acceptance/a.rs, crates/fl_view/tests/snapshots/x.snap,
                                             //   scenarios/duel.ron, docs/perf/budgets.ron; NOT: assets/base/scenarios/testbed.scenario.ron,
                                             //   crates/fl_sim/tests/fixtures/core_mod/scenarios/duel.scenario.ron
#[test] fn guard_flags_removed_assert()      // "-    assert_eq!(a, b);" → RemovedAssert
#[test] fn guard_flags_new_ignore_and_should_panic()
#[test] fn guard_flags_fewer_proptest_cases()   // "-    cases: 256," "+    cases: 64," → FewerProptestCases
#[test] fn guard_flags_alpha_change()        // "-const ALPHA: f64 = 1e-4;" "+const ALPHA: f64 = 1e-2;" → ToleranceChanged
#[test] fn guard_flags_removed_test_fn()     // removed "#[test]" + "fn x()" → RemovedTest and NetTestCountDrop
#[test] fn guard_flags_mock_in_sim_test()    // "+struct MockRunner;" in crates/fl_sim/tests/x.rs → MockInSimTest
#[test] fn guard_quiet_on_added_asserts()    // only "+ assert!(..)" lines → no findings
#[test] fn bless_promotes_new_files()        // a.snap.new, b.replay.new → a.snap, b.replay; returns 2 paths
#[test] fn bless_requires_reason()           // reason "" → Err
#[test] fn bless_refuses_implementer_role()  // FL_ROLE=implementer → Err
```
- [ ] **Step 2: Run** `cargo test -p xtask`. Expected: FAIL.
- [ ] **Step 3: Implement**, including the two gate rows and the subcommands.
- [ ] **Step 4: Run** `cargo test -p xtask`. Expected: PASS.
- [ ] **Step 5: Verify.** `cargo xtask check-scope --track contract` → `check-scope: OK`; `cargo xtask test-guard` → `test-guard: 0 findings …`; `cargo xtask gate task --track contract` → `gate task: PASS`.
- [ ] **Step 6: Commit** `build(xtask): OWNERS.toml, check-scope, test-guard, bless` with trailer `Refs: ADR-0011, ADR-0009`.

