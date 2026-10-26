### Spec Compliance

- ✅ Spec compliant (every listed file has its hunk; interfaces match the brief).
- ⚠️ Cannot verify from diff: `LocalDriver::issue` stamping/ordering/recording (the tests in `crates/fl_client/tests/ai.rs:359` show exec_tick 101 and seq 0, which is evidence, but the code is outside the diff). `data-check` exit 0 and the gate PASS are the implementer's claims; the scenario's unit and map ids (`base.de.pz2`, `base.su.kv1`, `base.testbed_1km`, ...) were not checked against content. Controller should run `fl_cli data-check assets/base` once.

### Strengths
- `ai_think` (ai.rs:90-123) takes only `SnapshotBuffer`, `RunnerLink.header.string_tables` and `LocalDriver`. The controller sees only `&PlayerView` and the tables, so the host never touches sim internals. `fl_client` has no `fl_sim` dependency.
- Drafts go through `driver.issue(draft)`, the same path as a human. The host mutates no sim state.
- One think per frame is structural: one system run per frame, guarded by `Local<Option<Tick>>`. The test (tests/ai.rs:330) sends 3 outputs in one frame and gets 1 think on tick 7.
- Tests are deterministic: no runner thread, frozen clock with `ManualDuration(ZERO)`, channel pre-filled.
- The team-8 test with its team-7 control is meaningful, because it shows `visible_to` has only 8 bits.
- `ai_host` fails `prepare()` with the exact message the brief asks for. `unknown_ai_is_error` builds a real layered temp mod.

### Issues

#### Critical
- none

#### Important
- none

#### Minor
- ai.rs:101 uses `!=` where ADR-0016 line 52-53 says "advanced". The ADR text and the code disagree. Either amend the ADR to "changed" (a reload restarting at an earlier tick is a legitimate case) or key off a load generation counter.
- ai.rs:101-104 plus the unrefreshed host after `fl.load` / `fl.load_scenario` (known item). Details in the recommendations below.
- ai.rs:114: the "another player dropped" warn fires on every think (every tick) for a misbehaving AI. That is log spam at the sim rate. Consider warning once per controller.
- ai.rs:108-109: `thinks` counts per controller, not per tick. The brief says "thinks == ticks observed". These are the same for one AI, but the doc comment should say which it is. It already does; just keep it in mind with 2+ AIs.
- tests/ai.rs: no test with 2 AI players, and none with a tick-regression (reload) case. The `!=` behaviour has no test.
- TDD: no RED run was captured (the report admits this). Process note only.
- The `team_*` tests exercise `fl_view` behaviour through the host. It is fine given the Files limit, but a unit test belongs in `fl_view/tests/view.rs` too, if it is not there already.
- Gate took 374s against a 300s budget (cold build). Not a code issue.

### Assessment

**Task quality:** Approved

**Reasoning:** A small, correct seam. The AI runs through the normal command path with a view-only input, a once-per-frame cadence, deterministic tests and a valid scenario. Nothing blocks. The two known items are documented limitations, not defects.

### Recommendations on the two known items
1. Host not refreshed after `fl.load` / `fl.load_scenario`: accept for T20. Open a tracked follow-up in a task that owns `LoadedMatch`: carry the new `AiHost` (built by `ai_host(&Players)`) in `LoadedMatch` and re-insert it on load. Add a one-line note in ADR-0016 or the task ledger so the demo (Task 21) does not load an AI scenario mid-run. Until then, `testbed_ai` only works as the startup scenario.
2. `!=` vs "advanced": keep `!=`. A reload to an earlier tick must not freeze the AI, and `>` would do that. Amend the ADR wording to "changed (including a restart at an earlier tick)". Once item 1 is fixed, replace this with an explicit reset of the `Local` on load, and add a regression test.
