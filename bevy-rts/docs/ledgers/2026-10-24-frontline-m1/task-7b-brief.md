### Task 7b: HPA* cost-bound counterexample (controller-added)

**Track:** sim-core (+ docs if ADR-0015 wording changes) · **Refs:** ADR-0015, Task 7 (HPA*), R96–R99 · **Lane 1**

**Why:** `crates/fl_sim/tests/hpa.rs::hpa_cost_near_optimal` failed once with a random seed during Task 10b: a searched HPA plan cost 3719 against an optimal grid-A* cost of 2361 (ratio ≈ 1.575), above the test's documented bound `cost × 10 ≤ optimal × 13 + 40 × COST_ROAD` (≈ 1.3 × optimal + 4 road cells). Nothing in Task 10b touches HPA, so this is a latent Task 7 defect or a wrong bound. The shrunk case (Tracked, size 2, three lakes on a large ground map) is saved at `C:\fl\a\.superpowers\sdd\2026-10-24-frontline-m1\task-10b-hpa.proptest-regressions`.

**Do (systematic debugging — root cause first):**
1. Copy the regression seed into `crates/fl_sim/tests/hpa.proptest-regressions` (proptest's own regressions file for that test binary — check the file naming proptest uses for integration tests and commit it so the case always re-runs). Reproduce the failure deterministically.
2. Find the root cause. Candidates to check, in order: entrance placement for size-2 profiles along borders broken by lakes (missing entrances → long detours); intra-cluster edge costs computed with a different profile/clearance than refinement uses; abstract search heuristic or tie-break; refinement/smoothing producing a costlier path than the abstract estimate; the bound itself being unjustified for HPA with 16×16 clusters (HPA's classic suboptimality is typically ≤ ~10–20% but can be worse with sparse entrances).
3. Fix the defect if there is one (deterministic, integer-only, keep R96 one-node-per-border-cell, keep R84 charged-work neutrality). If the cause is inherent to HPA with sparse entrances, do NOT just loosen the test: choose the minimal deterministic improvement (e.g. one entrance per maximal border run per side plus run endpoints when a run is longer than N cells, or a refinement-time direct A* fallback when the abstract path's estimated cost exceeds k× the octile lower bound), justify it, and only then set the bound the improved algorithm guarantees, documented in ADR-0015 and the test.
4. Strengthen the proptest: run it with more cases (e.g. 2,000) at least once locally and record the worst observed ratio in the report.

**Constraints:** outcomes change only where plans change; if the golden replay, perf snapshots or budgets change, write `.new` (never bless) and explain. No `Local<T>` in sim systems; no `git stash`. Read crates/fl_sim/src/nav/{hpa,astar,los,grid}.rs and task-7-report.md / task-7-review.md first.

**Verification:** `cargo nextest run -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli` in full; `cargo xtask gate task --track sim-core,perf,docs` (add contract if ADR changes).

**Commit:** `fix(fl_sim): HPA* <what> (cost-bound counterexample)` with `Refs: ADR-0015` and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
