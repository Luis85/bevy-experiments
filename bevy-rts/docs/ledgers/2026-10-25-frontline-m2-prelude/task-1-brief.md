### Task 1: Gate time trim and test-guard allowlist (M17, M16)

**Track:** contract · **Refs:** ADR-0009, ADR-0022 · **Lane 1** · first

**Files:**
- Modify: `xtask/src/gate.rs` (task-level headless filter, level budgets, `task-time` row, timing log), `xtask/src/test_guard/mod.rs` (suppression), `OWNERS.toml` (`[protected]` gains `xtask/heavy-tests.txt` and `xtask/test-guard-accepted.toml`)
- Create: `xtask/heavy-tests.txt`, `xtask/test-guard-accepted.toml`, `xtask/src/test_guard/accepted.rs`

**Interfaces:**
- Consumes: `gate::{steps, task_headless_filter, nextest, Ctx, GateReport}`, `test_guard::{run, Finding, FindingKind, summary}`.
- Produces:
```rust
// xtask/src/gate.rs
const LEVEL_BUDGET: [(Level, Duration); 4] = [(Level::Task, 300 s), (Level::Pr, 900 s), (Level::Nightly, 5400 s), (Level::Milestone, 3600 s)];
const SMOKE_BUDGET: Duration = Duration::from_secs(120);  // the task-level smoke set alone (touched = none)
pub fn heavy_filter(root: &Path) -> anyhow::Result<Option<String>>
    // xtask/heavy-tests.txt: one `<package> <binary> <test>` per line, `#` comments, blank lines ignored;
    // → `(package(=p) & binary(=b) & test(=t)) | …`; a line without exactly three fields is an error naming its line number
fn task_headless_filter(headless: &[String], touched: &[String], content: bool, excl: &str, heavy: Option<&str>) -> Option<String>
    // as today, then `& not (<heavy>)`
fn task_time(ctx: &Ctx) -> Outcome
    // nightly row "task-time": runs the task-level test-headless selection twice, with touched = [] (the smoke set) and
    // touched = ["fl_sim", "fl_test_utils"]; FAIL when the first exceeds SMOKE_BUDGET or the second TASK budget, naming both times
fn append_gate_times(root: &Path, report: &GateReport, head: &str)
    // appends {"level","head","total_secs","rows":[{"name","secs","outcome"}]} to target/xtask/gate-times.jsonl
// xtask/src/test_guard/accepted.rs
pub struct Accepted { pub file: String, pub kind: FindingKind, pub text: String, pub ruling: String }
pub fn load_accepted(root: &Path) -> anyhow::Result<Vec<Accepted>>
    // xtask/test-guard-accepted.toml: [[accepted]] file, kind ("RemovedAssert", …), text, ruling ("R109"); empty ruling → error
pub fn suppress(findings: Vec<Finding>, accepted: &[Accepted]) -> (Vec<Finding>, usize)
    // a finding matches when its normalized file, kind and whitespace-normalized text equal an entry's; the line is ignored
```
- Every level's footer prints `WARN gate <level> took <n>s (budget <b>s)` when its total exceeds its `LEVEL_BUDGET`; only the `task-time` row fails.

- [ ] **Step 1: Write the failing tests** (`xtask/src/gate.rs` and `xtask/src/test_guard/accepted.rs` unit tests):
```rust
#[test] fn heavy_filter_lists_each_test()          // "fl_sim flow group_move_uses_one_flow_request" →
                                                    //   "(package(=fl_sim) & binary(=flow) & test(=group_move_uses_one_flow_request))"
#[test] fn heavy_line_needs_three_fields()          // "fl_sim flow" → Err containing "heavy-tests.txt:1"
#[test] fn task_filter_excludes_heavy()             // task_headless_filter(.., Some(h)) ends with "& not (<h>)"
#[test] fn pr_runs_heavy_tests()                    // the pr-level test-headless filter does not contain the heavy expression
#[test] fn task_time_row_is_nightly_only()          // steps(): "task-time" levels == [Nightly]
#[test] fn level_budget_warns()                     // GateReport { level: Pr, total: 1000 s } renders "WARN gate pr took 1000s (budget 900s)"
#[test] fn gate_times_appended()                    // append_gate_times twice → 2 lines, each parses as JSON with "level"
#[test] fn accepted_findings_are_suppressed()       // RemovedAssert "assert_eq!(a, 1);" in crates/x/tests/t.rs line 10, entry with same file/kind/text,
                                                    //   ruling "R109" → (remaining 0, suppressed 1); the same text in another file stays
#[test] fn accepted_entry_needs_ruling()            // ruling "" → load error naming the entry index
#[test] fn summary_counts_suppressed()              // "2 findings (3 accepted findings suppressed)"
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p xtask -E 'test(/heavy|task_time|level_budget|gate_times|accepted|summary_counts/)'`. Expected: FAIL (functions missing).
- [ ] **Step 3: Implement.** Then measure: run `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_catalog -p fl_content -p fl_cli` and read the per-test times from `target/nextest/agent/junit.xml`. Put in `xtask/heavy-tests.txt`: every test of those packages over 10 s that is not a `smoke_` test, plus every `scale_*` and `counters_*` test, `group_move_uses_one_flow_request`, `queue_burst_drains_at_5000` and `path_budget_holds_at_scale` (M17). If the `smoke_` tests together exceed 120 s, list the slowest `smoke_` tests too until they don't. Each line carries a `# <seconds> s` comment. Start `xtask/test-guard-accepted.toml` empty (header comment only); only the lead adds entries, after a reviewer accepts a finding.
- [ ] **Step 4: Run** the step 2 command and `cargo nextest run --profile agent -p xtask`. Expected: PASS.
- [ ] **Step 5: Measure the trim.** Run `cargo xtask gate task --base 3d78b45` (M1 Task 14's base: it touches `fl_sim` and `fl_test_utils`). Its scope, protected and test-guard rows may fail on that base; ignore them. Paste the `test-headless` row's time. Expected: at most 150 s, against 916 s before (ledger line 76).
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract`. Expected: last line `gate task: PASS in <n>s`.
- [ ] **Step 7: Commit** `feat(xtask): task gate runs touched tests minus a heavy list; level budgets, nightly task-time row; test-guard allowlist` with trailers `Refs: ADR-0009, ADR-0022`.

