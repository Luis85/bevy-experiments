### Task 5: Golden replay versioning and change classes (C8, M10)

**Track:** contract + sim-core + tools · **Refs:** ADR-0021 · **Lane 1** (after 1, 3, 4, and 2 merged: both edit `golden.rs`)

**Files:**
- Modify: `crates/fl_sim/src/{replay,scale,lib}.rs`, `crates/fl_sim/tests/replay.rs`, `crates/fl_cli/src/{main,lib}.rs`, `crates/fl_test_utils/tests/golden.rs`, `xtask/src/{gate,main}.rs`, `docs/design/changelog-mechanics.md` (MCR-000)
- Create: `crates/fl_cli/src/rebless.rs`, `crates/fl_cli/tests/rebless.rs`, `xtask/src/change_class.rs`
- Blessed: `crates/fl_test_utils/tests/replays/{testbed_wander,testbed_paths}.replay` (format 4, hash streams unchanged), `crates/fl_sim/tests/snapshots/formats__save_codec_formats.snap`, `xtask/snapshots/api/fl_sim.txt`

**Interfaces:**
- Consumes: `Replay`, `ReplayRecorder`, `verify_replay`, `replay_app`, `replay_inputs`, `first_divergence`, `ScaleScenario`, `Scenario::replay`, `testbed_paths`, `TerrainGrid::terrain_at`, `unit_cells`.
- Produces:
```rust
// crates/fl_sim/src/replay.rs
pub const REPLAY_FORMAT_VERSION: u32 = 4;   // 4: Scale params versioned, semantic checks, journal summary; 3 still reads
pub enum ReplayStart { Scenario(String), Scale { version: u32, params: Vec<u8> }, Save(Vec<u8>) }
pub struct Replay { /* existing fields */ pub semantic: Vec<SemanticCheck>, pub journal_summary: BTreeMap<String, u32> }
#[non_exhaustive] #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SemanticCheck {
    UnitWithin { id: String, tick: Tick, unit: SimId, min: FxVec2, max: FxVec2 },   // the unit's Pose.pos is inside the rect after `tick` ran
    UnitExists { id: String, tick: Tick, unit: SimId, exists: bool },
    EntityCount { id: String, tick: Tick, min: u32, max: u32 },                       // SimIndex length
    NeverOnTerrain { id: String, units: Vec<SimId>, terrain: String },                // after every tick, no listed live unit's cell has this terrain
}
impl SemanticCheck { pub fn id(&self) -> &str; }
#[derive(Clone, Debug, PartialEq)] pub struct SemanticFailure { pub id: String, pub tick: Tick, pub detail: String }
impl Replay {
    pub fn from_bytes(bytes: &[u8]) -> Result<Replay, ReplayError>;  // reads 3 (frozen `mod v3`, semantic empty, summary empty) and 4
    pub fn with_semantics(self, checks: Vec<SemanticCheck>) -> Replay;
}
#[non_exhaustive] pub enum VerifyOutcome { Verified { ticks: u32, warnings: Vec<String> }, Diverged(Divergence),
    SemanticFailed(Vec<SemanticFailure>), NonReproducible }      // semantic checks run during every verify; hashes are checked first
#[derive(Clone, Debug, PartialEq)] pub struct ReblessEntry { pub first_divergence: Option<Divergence>,
    pub journal_diff: Vec<(String, u32, u32)>, pub entities: (u32, u32), pub semantic: Vec<SemanticFailure> }
pub fn rebless_replay(replay: &Replay, features: &[Arc<dyn SimFeature>], content: Arc<ContentDb>)
    -> Result<(Replay, ReblessEntry), ReplayError>
    // re-runs the start and the remapped inputs on this build with RunConfig { journal: Some(true), hash_interval: Some(replay.hash_interval) };
    // the new replay has this build's header, the new hashes and journal summary, the same start, inputs, ticks and semantic checks
// crates/fl_sim/src/scale.rs
impl ScaleScenario { pub const VERSION: u32 = 1; pub fn encode(&self) -> (u32, Vec<u8>); pub fn decode(version: u32, bytes: &[u8]) -> Result<ScaleScenario, String>; }
// crates/fl_cli: `fl_cli rebless --reason MCR-nnn [--dir crates/fl_test_utils/tests/replays] [--changelog docs/design/changelog-mechanics.md]
//   [--report target/rebless-report.md] [--json]`
//   exit 1, writing nothing: the changelog has no `## MCR-nnn` heading, or a replay's semantic check fails;
//   else exit 0: `<name>.replay.new` for every replay whose bytes change, and the report (per replay: first divergent tick, journal summary
//   diff, entity count before/after, semantic results); JSON {"reason", "replays": [{"name", "changed", "first_divergence", "journal_diff"}]}
// xtask/src/change_class.rs
pub const CLASSES: [char; 6] = ['R', 'D', 'F', 'N', 'E', 'X'];
pub fn trailer_class(message: &str) -> Option<char>;       // "Change-Class: F"
pub fn trailer_mcr(message: &str) -> Option<String>;        // "MCR: MCR-001"
pub fn needs_class(files: &[PathBuf]) -> bool;              // any path under crates/, features/, assets/ or the root scenarios/, or the
                                                            //   root Cargo.toml or Cargo.lock
pub struct CommitFacts { pub sha: String, pub message: String, pub files: Vec<PathBuf>, pub replay_format_changed: bool }
    // replay_format_changed: REPLAY_FORMAT_VERSION in crates/fl_sim/src/replay.rs differs between this commit and its parent (R151)
pub struct BranchFacts { pub sim_version_changed: bool, pub replay_format_changed: bool, pub content_changed: bool, pub goldens_changed: Vec<PathBuf>,
    pub refactor_goldens: Vec<(String, PathBuf, bool)> }
    // content_changed: a file under assets/** or the root scenarios/** differs between the merge base and HEAD;
    // refactor_goldens: per class R or E commit (sha) and each golden it changes, whether the golden's hash stream at that commit equals its hash
    //   stream at the commit's parent (the row decodes both versions' bytes with `fl_cli replay dump --json`, which reads formats 3 and 4);
    //   per commit, never merge base against HEAD, since later class F re-blesses change HEAD's streams (R153)
pub fn class_violations(commits: &[CommitFacts]) -> Vec<String>
    // merge commits skipped; a commit needing a class has a known one; class F or D needs an `MCR:` trailer; a class R or E commit that
    //   changes a golden is a violation unless that same commit has replay_format_changed (the exemption is per commit, ADR-0021, R151)
pub fn golden_violations(facts: &BranchFacts, golden_commits: &[(String, String)], changelog: &str) -> Vec<String>
    // also: every refactor_goldens entry with an unequal hash stream is a violation
// gate rows: "change-class" (Task, Pr), "golden-policy" (Pr, Milestone)
```
- The record functions in `golden.rs` attach semantic checks: `testbed_wander` gets `ten_units` (`EntityCount` tick 399, 10..=10) and `wander_off_water` (`NeverOnTerrain` units #1–#10, `base.water`); `testbed_paths` gets `thirteen_units` (`EntityCount` tick 899, 13..=13) and `no_unit_on_water` (`NeverOnTerrain` #1–#13, `base.water`). These follow from the setups (no unit dies, M1 Review Focus 1), not from a run.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_sim/tests/replay.rs
#[test] fn format_3_replays_still_read()            // the committed testbed_wander.replay (format 3) → Replay with semantic empty, same hashes
#[test] fn scale_start_round_trips_versioned_params() // ScaleScenario::with_units(50) encode → decode equal; ReplayStart::Scale { version: 1, .. }
#[test] fn scale_params_of_unknown_version_fail()      // decode(2, ..) → Err containing "ScaleScenario version 2"
#[test] fn semantic_failure_is_reported()           // a replay with UnitExists { tick 5, unit #99, exists: true } → VerifyOutcome::SemanticFailed with id
#[test] fn never_on_terrain_checks_every_tick()     // an ASCII map with water; a unit teleported onto water at tick 3 → failure at tick 3
#[test] fn rebless_keeps_hashes_for_unchanged_code() // rebless_replay(testbed_wander) → first_divergence None; hashes equal; journal_summary non-empty
// crates/fl_test_utils/tests/golden.rs (existing tests stay; the record functions attach the checks above)
#[test] fn golden_semantics_hold()                  // both committed goldens verify with their semantic checks (after the re-record)
// crates/fl_cli/tests/rebless.rs
#[test] fn rebless_refuses_unknown_mcr()            // --reason MCR-999 with a changelog lacking it → exit 1, stderr "MCR-999", no .new written
#[test] fn rebless_refuses_failing_semantic_check() // a temp dir replay whose check fails → exit 1, no .new
#[test] fn rebless_writes_new_and_report()          // a format-3 replay in a temp dir, changelog with "## MCR-000" → exit 0, <name>.replay.new
                                                    //   and the report naming "first divergent tick: none"
// xtask/src/change_class.rs (unit tests)
#[test] fn code_commit_needs_class()                // ("a1", "feat: x", [crates/fl_sim/src/a.rs]) → 1 violation naming a1; docs-only commit → none
#[test] fn class_must_be_known()                    // "Change-Class: Q" → violation
#[test] fn golden_change_needs_mcr_and_version()    // goldens changed, no MCR trailer → violation; MCR present but no version, format or content change → violation
#[test] fn mcr_must_exist_in_changelog()            // MCR: MCR-004, changelog without "## MCR-004" → violation
#[test] fn sim_version_bump_needs_a_golden_change() // sim_version_changed && goldens_changed empty → violation
#[test] fn refactor_class_cannot_change_goldens()   // a commit with Change-Class: R touching a .replay, replay_format_changed false → violation,
                                                    //   even when an earlier commit of the branch changed the format
#[test] fn format_commit_may_rewrite_goldens()      // Change-Class: R, MCR: MCR-000, touching a .replay with replay_format_changed true → no violation
#[test] fn format_exemption_needs_equal_hash_streams() // refactor_goldens [("a1", testbed_wander.replay, false)] → violation naming a1 and the file
#[test] fn later_reblesses_do_not_fail_the_format_commit() // the format commit's entry is true (equal to its parent) and a later F commit
                                                    //   re-blesses the golden → no violation
#[test] fn data_commit_needs_mcr()                  // Change-Class: D without an MCR: trailer → violation
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim -p fl_cli -p fl_test_utils -p xtask -E 'test(/format_3|scale_start|scale_params|semantic|never_on|rebless|golden_semantics|class|golden_change|mcr_must|sim_version_bump|refactor_class|format_commit|format_exemption|later_reblesses|data_commit/)'`. Expected: FAIL.
- [ ] **Step 3: Implement.** `mod v3` in `replay.rs` freezes the format-3 `Replay` and `ReplayStart` exactly as at tag `m1` (`ReplayStart::Scale(ScaleScenario)` with `ScaleScenario`'s fields at tag `m1`, copied as a frozen `v3::ScaleScenario`). The rows read `SIM_VERSION` and `REPLAY_FORMAT_VERSION` from the `pub const` lines of `crates/fl_sim/src/build.rs` and `crates/fl_sim/src/replay.rs` at the merge base and at `HEAD` (`git show`). Append to the changelog:
  `## MCR-000 — Replay format 4 (C8, M10)` — Why: versioned scale params, semantic checks, journal summary. REQs: none. Expected effect: replay bytes change; hash streams identical (first divergent tick: none). SIM_VERSION: unchanged (2). Change-Class: R.
- [ ] **Step 4: Run** the step 2 command. Expected: PASS except `golden_semantics_hold` and the committed-golden checks, which need the re-recorded files.
- [ ] **Step 5: Re-record.** The semantic checks come from the record functions, so this first re-record uses them: `FL_RECORD_GOLDEN=1 cargo nextest run --profile agent -p fl_test_utils --test golden` writes both `.replay.new` files (format 4, with the checks). Confirm with `cargo run -q -p fl_cli -- replay dump --json <file>` that each `.new` file's `hashes` equal the committed file's. Then exercise the tool on copies: put the two `.new` files, renamed to `.replay`, into a scratch directory and run `cargo run -q -p fl_cli -- rebless --reason MCR-000 --dir <scratch> --json`. Expected: both `first_divergence: null` and `changed: false`. Paste the comparison and the report. Regenerate the format and API snapshots (`.new`).
- [ ] **Step 6: Bless (lead, R146)** `cargo xtask bless crates/fl_test_utils/tests/replays crates/fl_sim/tests/snapshots xtask/snapshots --reason "MCR-000: replay format 4 with semantic checks; hash streams unchanged"`. List MCR-000 for Task 21's ratification list.
- [ ] **Step 7: Run** `cargo nextest run --profile agent -p fl_sim -p fl_cli -p fl_test_utils -p xtask`. Expected: PASS.
- [ ] **Step 8: Gate** `cargo xtask gate task --track contract --track sim-core --track tools`. Expected: PASS (`change-class` passes: every commit since the base carries a class).
- [ ] **Step 9: Commit** `feat(fl_sim)!: replay format 4 with semantic checks, fl_cli rebless, change-class and golden-policy gate rows` with trailers `Change-Class: R`, `MCR: MCR-000`, `Refs: ADR-0021`, `Blessed: …`.

