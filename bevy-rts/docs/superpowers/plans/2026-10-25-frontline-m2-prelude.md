# Frontline M2 Prelude (Group C + M1 carry-overs) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the M2 entry requirements (Group C, C1–C11) and the M1 carry-over list (ruling R145), so that M2's combat mechanics are specified, tested, versioned and composed through mechanical checks: machine-checkable mechanic specs with requirement tracing, a provenance ledger, a test-author with held-out tests, a statistical test policy, the core vocabulary (conditions, stats and modifiers, tags, verbs), phased content loading with patches, an order queue with an activity stack and `SimId` relationships, versioned golden replays, mutation testing, risk-tiered review, and a render scale baseline.

**Architecture:** Two kinds of work run side by side. Lane 1 builds the verification pipeline (gate time, golden replay versioning, release-profile determinism, spec-lint and trace, the acceptance pipeline) and then reworks the sim core (C7, the carried movement fixes, the sim half of the vocabulary). Lane 2 reworks the content loader into explicit phases over a lossless RON tree (C6), adds the provenance ledger (C2) and the content half of the vocabulary (C5), then mutation testing and review tiers (C9, C10), the runner carry-overs, the render baseline (C11), path perf and the client command buttons. Every sim change is proven outcome-neutral where it claims to be: by unchanged golden hash streams for refactors, and by a pose trace recorded before C7 for the order-queue rework.

**Tech Stack:** Rust 1.99.1 (edition 2024), Bevy `=0.20.0` (`bevy_app` + `bevy_ecs` only in sim crates; `ExtendedMaterial`, `GpuComponentArrayBuffer`, `VisibilityRange` and WESL in the client), serde + ron 0.12 + postcard 1, erased-serde 0.4, serde_json, proptest 1.12 (`RngSeed::Fixed`), insta, cargo-nextest, cargo-mutants 27.1 (installed). Nothing new is downloaded: every crate is already in `Cargo.lock`.

**Spec:** `docs/superpowers/specs/2026-10-23-frontline-rts-design.md` (revisions 2 to 2.4; where they disagree, the newest wins): §5.2, §5.4–§5.8, §6.1, §6.4, §7, §9.2 (Group C), §12, §13, §15. ADRs: 0002 runner, 0004 registries, 0005 phases, 0006 classification and saves, 0007 envelope, 0009 gates, 0010 perf counters, 0011 contract governance, 0012 hot reload, 0015 path requests. This plan adds ADR-0017 core vocabulary, ADR-0018 unit behaviour and entity graph, ADR-0019 sim event model, ADR-0020 content load phases, patches and provenance, ADR-0021 golden replay versioning and change classes, ADR-0022 verification from M2, and refinements to ADR-0002, ADR-0006, ADR-0007 and ADR-0015 (Task 3). Other inputs:
- `docs/superpowers/plans/2026-10-23-frontline-m0-rev2.md` ("M2 entry requirements (Group C …)" table and the milestone definition of done) and `docs/superpowers/plans/2026-10-24-frontline-m1.md` (its Global Constraints and the structure this plan follows).
- `docs/research/baseline-recommendations.md` (Group C), `docs/research/workflow-and-verification.md` §2.2, §3, §4.4, §4.6, §5.3–§5.5, §6.1–§6.3, §8, and `docs/research/architecture-best-practices.md` N8–N15, N22.
- Rulings R20–R80 (`.superpowers/sdd/2026-10-23-frontline-m0-rev2/progress.md`) and R81–R146 (`.superpowers/sdd/2026-10-24-frontline-m1/progress.md`), all binding. R145 is the carry-over list; `final-review-m1.md` (same directory) details its items.
- Bevy API facts: the `bevy-api` skill, never memory.

## Global Constraints

**The M0 rev-2 Global Constraints** (`docs/superpowers/plans/2026-10-23-frontline-m0-rev2.md`) **and the M1 Global Constraints** (`docs/superpowers/plans/2026-10-24-frontline-m1.md`) apply to every task. Below are only the additions and changes.

**Where and how to build** (changes)
- **Lane 1** is `C:\fl\a` on branch `m2p/phase-a`, cut from `main` at tag `m1` by the controller (`git -C C:\fl\a switch -c m2p/phase-a m1`).
- **Lane 2** worktrees are `C:\fl\m2pNN` on branch `m2p/tNN-<slug>`, cut from `m2p/phase-a` at the commit their dependencies need. Every cargo command there runs with `CARGO_BUILD_BUILD_DIR=C:/t/fl/28/2710d09957a748` (R31), and only one lane-2 worktree builds at a time.
- **At most two builders** (the lane-1 task and the lane-2 task). The controller's gate and merge runs use the slot of the lane they gate. Docs-only tasks (Tasks 3 and 4) never run cargo and are not builders. A `cargo mutants` run (Task 14) is a builder.
- **No `git stash`** (lesson from M1 Task 10): set work aside with a WIP commit.
- **Shared append points.** `xtask/src/gate.rs` `steps()` (and the `data_check` row's argument list), `xtask/src/main.rs` and `crates/fl_cli/src/main.rs` subcommand enums, `crates/*/src/lib.rs` re-exports, `SimBuilder` methods in `crates/fl_sim/src/builder.rs` and `crates/fl_protocol/src/tables.rs` constants only ever gain lines. When two lanes both append, the controller resolves the conflict at merge; a protected file both branches moved is regenerated after the second merge, never hand-merged.
- **Dependencies.** Task 6 adds `[profile.release-check]` to the root `Cargo.toml`; no other task edits `Cargo.toml` or `Cargo.lock` unless its **Files** list says so. Nothing is downloaded.
- **Downloads and toolchain.** No asset download and no toolchain update in this plan; either needs the user's OK. C11 uses primitive meshes only; real LOD models and impostor textures arrive with M6's asset pipeline, where a download would first need the user's OK.

**Determinism** (additions)
- **No `Local<T>` in sim systems** (spec §5.4). Reused buffers are `Class::TransientReset` resources (R102 as corrected in M1 Task 8).
- **Wire and content values (R104, M1 final review M2).** Every wire-supplied `Fx` or coordinate is clamped or validated at handler entry, and every content value that reaches sim arithmetic is range-checked in its link pass (`validate`), with bounds written in the doc comment. A new order kind, stat, modifier or verb states its bounds.
- **Journal (R139).** Determinism tests (twin, shuffle, branch, round trip, golden) never read the journal. A test that reads the journal builds its scenario with `ScenarioBuilder::journal(true)` (Task 6), so it passes in release builds too.
- **Proptests (R140, R142).** Every proptest uses `fl_test_utils::proptest_policy` (Task 6): a fixed ChaCha seed, no failure persistence, case count from `PROPTEST_CASES` with a per-test default, and random seeds only at nightly (`FL_PROPTEST_RANDOM=1`). Committed regressions become named tests. Whole-match proptests cap their cases (gate 64, milestone 512).
- **Change classes (ADR-0021, from the first commit of this plan).** Every commit that touches `crates/**`, `features/**` or `assets/**` carries a `Change-Class: R|D|F|N|E|X` trailer (Refactor, Data tuning, Formula or rule, New mechanic, Engine or crate upgrade, eXperiment). A class F commit also carries `MCR: MCR-nnn`, and its MCR entry in `docs/design/changelog-mechanics.md` is written before the code. Merge commits are exempt. Task 5's `change-class` gate row enforces it.
- **`SIM_VERSION` and core feature version.** `SIM_VERSION` is 2 at the start. It becomes 3 in Task 13 (MCR-001) and 4 in Task 15 (MCR-002); no other task changes it. `CoreFeature::version()` follows: 3 in Task 13, 4 in Task 15. Class F changes bump `SIM_VERSION` at most once per task.
- **Re-blesses (R146).** Golden replays are re-blessed only through `fl_cli rebless --reason MCR-nnn` (Task 5), by the lead, under the user's delegation. Every re-bless is listed in Task 21's final report for the user's ratification.
- **ADRs (R85).** New ADRs and refinements start `proposed`. The controller accepts them at Task 3's review under the user's delegation and lists them in the final report for ratification. Tasks that change contract paths cite them in a `Refs:` trailer.
- **Outcome-neutral claims are tested.** A task that says "hash-neutral" (Change-Class R or N) keeps every golden replay verifying unchanged, with no re-bless. Task 13 instead proves unchanged unit poses against the pose trace recorded in Task 2.

**Protected artifacts** (additions to the M1 bless rule)
- New protected paths (Task 1 and Task 11 add them to `OWNERS.toml [protected]`): `xtask/heavy-tests.txt`, `xtask/test-guard-accepted.toml`, `xtask/mutants-triage.toml` (Task 14), and the pose trace `crates/fl_test_utils/tests/replays/*.poses` (already under `tests/replays/**`).
- `docs/design/mechanics/**` is protected. Its files are written as `*.md.new` and blessed by the lead.
- Acceptance and holdout tests are written by the test-author (Task 11) and committed by the lead with a `Blessed:` trailer. Implementers never edit them; a test that contradicts its spec is reported as `BLOCKED: test X contradicts spec Y because …`.

**Observability** (additions; the definition of done requires all of them)
- **Journal kinds:** `core.activity_changed` (Task 13), `core.condition_changed` and `core.modifier_changed` (Task 17).
- **Counters:** `orders.activity_ticks` (Task 13), `stats.recomputed` (Task 17), `effects.applied` (Task 17).
- **Spans:** `run_activities` (Task 13), `recompute_stats` (Task 17).
- **Debug queries:** `core.activities` (Task 13), `core.stats` (Task 17); `core.passability` gains `since_version` (Task 16), and `core.paths` answers immobile units (Task 13).
- **CLI and xtask:** `fl_cli rebless`, `schema`, `tuning-tables`; `cargo xtask spec-lint`, `trace`, `mutants`, `review-tier`, `mini-gate`. Task 21 documents them in `docs/debugging.md`.

**Gate tracks.** Every task names its tracks. A task that touches a contract path (`OWNERS.toml` `contract` track: `xtask/**`, `.claude/**`, `docs/adr/**`, the spec, `fl_math`, `fl_protocol`, `fl_view`, the `fl_sim` extension API and save format, the `fl_content` loader and registry, `Cargo.toml`) includes `--track contract`. A task that changes `docs/perf/budgets.ron` or perf snapshots includes `--track perf`. Tasks that create new `fl_sim` extension-API files (`behaviour.rs`, `relation.rs`, `vocab.rs`) add them to the `contract` track in `OWNERS.toml`.

## Review Focus

The five failure modes most likely to bite a person using the prelude's work that the spec implies but no Group C line names, most likely first. Each one's tests are added to the owning task.

1. **An M1 save loaded under the new order queue.** A save taken mid-path, with Shift-queued rotate and move orders, a follower and a flow group, loads and continues exactly as M1 would have: same poses every tick. Tests: Task 2 `m1_save_continues_with_m1_pose_trace`; Task 13 `m1_save_migrates_to_order_queue`, `m1_save_continues_with_m1_pose_trace` (kept passing), `testbed_paths_pose_trace_matches`.
2. **Two mods touching the same content.** Two patches setting one field (the later mod wins, provenance names it), a patch to a missing target or path, `If(ModLoaded(..))` with the mod absent, and a patch on a template reaching every child. Each is either applied in a defined order or reported with the patch file and op index; nothing panics. Tests: Task 10 `later_mod_patch_wins_and_owns_provenance`, `patch_on_missing_path_names_file_and_op`, `if_mod_loaded_skips_absent_mod`, `template_patch_flows_to_children`.
3. **Provenance drifting from the data.** A value added without a ledger entry, a ledger key whose path no longer exists, a `Guess` naming an unknown guess id, and a `Tuned` entry without an MCR. Each fails `data-check` or `spec-lint` naming the file and key. Tests: Task 8 `missing_ledger_entry_is_error`, `stale_ledger_key_is_error`; Task 9 `unknown_guess_id_is_lint_error`.
4. **Golden replays re-blessed carelessly.** A re-bless without an MCR entry, a golden changed under `Change-Class: R`, a `SIM_VERSION` bump with no golden change, or a semantic check that now fails. Each is refused or fails the gate. Tests: Task 5 `rebless_refuses_unknown_mcr`, `rebless_refuses_failing_semantic_check`, `golden_change_needs_mcr_and_version`, `sim_version_bump_needs_a_golden_change`.
5. **Content expressions at their edges.** An unknown condition or tag in a `requires` or class expression, a malformed expression, more than 128 tags, an empty modifier stack, and modifier values at their bounds. Each is a content error or an exact value; none panics or overflows. Tests: Task 12 `unknown_condition_in_requires_is_error`, `malformed_expression_names_column`, `tag_overflow_is_error`; Task 17 `no_modifiers_is_exactly_base`, `extreme_modifiers_stay_in_bounds`.

---

## Preconditions

- M1 is closed: `cargo xtask gate milestone --milestone M1` is green on `main`, `docs/perf/M1.json` is blessed (`95f03a6`), and the controller has tagged `main`'s tip `m1` (`git -C C:\fl\a tag m1 main`). Task 2 generates the M1 save fixture from that tag (R83).
- The M1 final report lists ADR-0014–0016 and the M1 refinements for the user's ratification (R85).
- Final-review items already closed by the M1 fix wave (R144) are not repeated here: I3, I4, M2, M9 and M15 (`0a1b2d4` protects `**/*.proptest-regressions`).

## Carry-over map (R145 → task)

| Item | What | Task |
|---|---|---|
| M17 | gate-time trim: touched-crate tests plus a capped smoke set, heavy tests to pr, budget overrun FAIL at nightly | 1 |
| M16 | test-guard allowlist of accepted findings | 1 |
| M11 | m1 save fixture, frozen `mod v2` `MoveOrder`, before C7 | 2 (C7 migration in 13) |
| M10 | replay format decoupled from `ScaleScenario` | 5 |
| R139 | journal-reading tests opt in | 6 |
| R140 | proptest policy helper and migration | 6 |
| I2 | release-profile determinism row | 6 |
| M12, M13, M14, M7 | `PathFollow::reset()`, enqueue symmetry, `core.no_map`, immobile units in `core.paths` | 13 |
| M1, M4, M5 | reload invalidates newly blocked paths; flow members fall off after a stall, not by age; path queue lanes | 15 |
| M3, M8 | `FromRunner` `#[non_exhaustive]` + `ToRunner::reply_id`; `core.passability` `since_version` | 16 |
| R98, R125, Task 7b | per-request search buffers reused; in-cluster search work cut without changing plans | 19 |
| R136, M6 | HPA* densification and resumable flow integration: triggers re-checked, stay parked | 19 (decision only) |

---

## Execution topology

At most two builders run at once. **Lane 1** is `C:\fl\a` (verification infrastructure, then the sim core). **Lane 2** is one worktree at a time (content loader, tools, client, perf), building in the shared lane-2 dir (R31). The controller merges lane-2 branches into `m2p/phase-a` (`--no-ff`) and re-runs `gate task`. `main` is fast-forwarded with `gate pr` green after Tasks 6, 13 and 17, and after Task 21.

```
Phase I   lane 1: 1 ──────────────→ 5 (after 2) → 6      (3 and 4: docs-only, alongside 1 in C:\fl\a)
          lane 2: 2 → 7 (after 3)
Phase II  lane 1: 9 (after 6, 8) → 11 → 13 (after 2, 5, 11)
          lane 2: 8 (after 4, 7) → 10 → 12 → 14 (after 11)
Phase III lane 1: 15 → 17 (after 12, 16)
          lane 2: 16 → 18 → 19 (after 17) → 20
Phase IV  lane 1: 21
```

Ruling R40 applies: a dependent may start while its dependency is in review when the dependency's API is unlikely to change. Ruling R21 applies to Tasks 3 and 4: they edit only docs, so they run in `C:\fl\a` alongside Task 1 without a build.

| Parallel pair | Lane-1 task owns | Lane-2 task owns |
|---|---|---|
| 1 (+3, 4) ∥ 2 | 1: `xtask/src/{gate,main}.rs`, `xtask/src/test_guard/**`, `xtask/{heavy-tests.txt,test-guard-accepted.toml}`, `OWNERS.toml`. 3: `docs/adr/**`, `docs/superpowers/specs/**`. 4: `docs/design/**` | `crates/fl_sim/src/units.rs` (the `mod v2` block and its unit test), `crates/fl_test_utils/src/{trace,lib}.rs`, `crates/fl_test_utils/tests/{m1_save,golden}.rs`, `crates/fl_test_utils/tests/replays/{m1_testbed.save,testbed_paths.poses}(.new)` |
| 5–6 ∥ 7 | 5: `crates/fl_sim/src/{replay,scale,lib}.rs`, `crates/fl_sim/tests/replay.rs`, `crates/fl_cli/src/{rebless,main,lib}.rs`, `crates/fl_cli/tests/rebless.rs`, `crates/fl_test_utils/tests/golden.rs`, replays, `xtask/src/{change_class,gate,main}.rs`, `docs/design/changelog-mechanics.md`. 6: `Cargo.toml`, `crates/fl_test_utils/src/{builder,proptest_policy,lib}.rs`, every `crates/*/tests/*.rs` that reads the journal or runs a proptest, `crates/fl_sim/tests/hpa.rs`, `xtask/src/gate.rs`, `.claude/rules/sim.md` | `crates/fl_content/src/{tree,object,inherit,mods,db,error,lib}.rs`, `crates/fl_content/tests/tree.rs` |
| 9–11 ∥ 8, 10, 12 | 9: `xtask/src/spec/**`, `xtask/src/{gate,main}.rs`, `crates/fl_cli/src/{schema,main,lib}.rs`, `@req` comment lines in `crates/*/tests/**`. 11: `.claude/agents/**`, `.claude/settings.json`, `xtask/src/{hooks,gate}.rs`, `xtask/src/test_guard/patterns.rs`, `crates/fl_test_utils/{Cargo.toml,src/stats.rs,src/lib.rs}`, `crates/fl_test_utils/tests/{acceptance,holdout,stats}/**` | 8: `crates/fl_content/src/{provenance,mods,db,lib}.rs`, `crates/fl_content/tests/provenance.rs`, `assets/base/provenance/**`, `crates/fl_cli/src/{tuning_tables,data_check,main,lib}.rs`, `crates/fl_cli/tests/tuning_tables.rs`, `docs/design/generated/**`, one `data_check` line in `xtask/src/gate.rs`. 10: `crates/fl_content/src/{patch,migrate,post,mods,registry,tree,lib}.rs`, `crates/fl_content/tests/patch.rs`. 12: `crates/fl_content/src/{expr,tags,verbs,registry,db,link,lib}.rs`, `crates/fl_content/tests/vocabulary.rs`, the new constants in `crates/fl_protocol/src/tables.rs` |
| 13 ∥ 14 | 13: `crates/fl_sim/**` (except `nav/**`), `crates/fl_protocol/src/{command,tables,lib}.rs`, `crates/fl_client/src/orders.rs` (the `MAX_COMMAND_UNITS` import), `crates/fl_test_utils/**` (except `src/stats.rs`), `OWNERS.toml` | 14: `xtask/src/{mutants,review_tier,mini_gate,gate,main}.rs`, `.cargo/mutants.toml`, `xtask/mutants-triage.toml`, `.claude/agents/reviewer.md` |
| 15–17 ∥ 16, 18, 19, 20 | 15: `crates/fl_sim/src/{path,movement,reload,orders,commands}.rs`, their tests. 17: `crates/fl_sim/src/{vocab,movement,publish,builder,lib}.rs`, `crates/fl_view/src/{conditions,lib}.rs`, tests | 16: `crates/fl_view/src/runner.rs`, `crates/fl_sim/src/runner/**`, `crates/fl_sim/src/nav/mod.rs` (the passability query), `crates/fl_client/src/{link,overlays}.rs`. 18: `crates/fl_client/src/{placeholder,camera,render}.rs`, `crates/fl_client/src/shaders/**`, client tests. 19: `crates/fl_sim/src/nav/{hpa,astar,flow,mod}.rs`, `crates/fl_sim/tests/{hpa,astar}.rs`, perf artifacts. 20: `crates/fl_client/src/{feature,buttons,lib}.rs`, `crates/fl_client/tests/buttons.rs` |

## File map (new or substantially changed)

```
docs/adr/{0017-core-vocabulary,0018-unit-behaviour-and-entity-graph,0019-sim-event-model,0020-content-load-phases-patches-provenance,
          0021-golden-replay-versioning,0022-verification-from-m2}.md   docs/adr/{0002,0006,0007,0015}-*.md (Refinements)  docs/adr/README.md
docs/design/{GDD.md,mechanic-template.md,changelog-mechanics.md}   docs/design/mechanics/{MOVE-speed,PATH-requests,FLOW-groups,ORDER-queue,STEER-separation}.md
docs/design/generated/tuning-tables.md   assets/base/provenance/{rules,units,terrain}.prov.ron
xtask/src/{gate,change_class,mutants,review_tier,mini_gate,hooks}.rs   xtask/src/spec/{mod,parse,lint,trace}.rs   xtask/src/test_guard/accepted.rs
xtask/{heavy-tests.txt,test-guard-accepted.toml,mutants-triage.toml}   .cargo/mutants.toml   .claude/agents/*.md   .claude/settings.json
crates/fl_content/src/{tree,provenance,patch,migrate,post,expr,tags,verbs}.rs (+ mods, db, registry, inherit, object, link)
crates/fl_sim/src/{behaviour,relation,vocab}.rs (+ units, orders, commands, path, movement, reload, replay, scale, save, runner)
crates/fl_view/src/conditions.rs   crates/fl_protocol/src/{command,tables}.rs
crates/fl_test_utils/src/{trace,proptest_policy,stats}.rs   crates/fl_test_utils/tests/{m1_save,acceptance/*,holdout/*,stats}.rs
crates/fl_cli/src/{rebless,schema,tuning_tables}.rs   crates/fl_client/src/{render,buttons}.rs   crates/fl_client/src/shaders/unit_instance.wesl
docs/{debugging.md,m2p-checklist.md}   docs/perf/{M2P.md,M2P.json}
```

---

## Phase I — gate time, fixtures, contracts and docs

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

### Task 2: M1 save fixture, frozen `MoveOrder` v2, pose trace (M11)

**Track:** sim-core · **Refs:** ADR-0006 (R83) · **Lane 2** (`C:\fl\m2p02`, `m2p/t02-m1-fixture`) · first, alongside Task 1

**Files:**
- Modify: `crates/fl_sim/src/units.rs` (a frozen `pub(crate) mod v2` and one unit test), `crates/fl_test_utils/src/lib.rs`, `crates/fl_test_utils/tests/golden.rs`
- Create: `crates/fl_test_utils/src/trace.rs`, `crates/fl_test_utils/tests/m1_save.rs`
- Blessed: `crates/fl_test_utils/tests/replays/m1_testbed.save`, `crates/fl_test_utils/tests/replays/testbed_paths.poses`

**Interfaces:**
- Consumes: `fl_cli save`, `Scenario::{load, run_ticks, world}`, `fl_sim::{StateHasher, SimHash, SimIndex, Pose, Health, load_world, SimClock}`, `testbed_paths()` and `TICKS` (900) from `tests/support/testbed_paths.rs`.
- Produces:
```rust
// crates/fl_sim/src/units.rs — "core.move_order" v2 exactly as at tag m1; never edited again (Task 13 migrates from it)
pub(crate) mod v2 {
    #[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
    pub(crate) struct MoveOrder { #[serde(deserialize_with = "super::capped")] pub(crate) orders: Vec<Order> }
    #[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
    pub(crate) enum Order { Move { target: FxVec2 }, Rotate { facing: Angle }, Follow { leader: SimId, offset: FxVec2 } }
}
// crates/fl_test_utils/src/trace.rs
pub fn pose_hash(world: &World) -> u64
    // a StateHasher over every SimIndex entry in order: SimId, Pose.pos, Pose.heading, Health.hp (u32::MAX without Health)
pub fn pose_trace(scenario: &mut Scenario, ticks: u32) -> Vec<(Tick, u64)>   // after each of the next `ticks` ticks: (the tick that ran, pose_hash)
pub fn write_pose_trace(path: &Path, trace: &[(Tick, u64)]) -> std::io::Result<()>  // "<tick> <hash as 016x>\n" per entry
pub fn read_pose_trace(path: &Path) -> std::io::Result<Vec<(Tick, u64)>>
```

- [ ] **Step 0: Generate the M1 save fixture (R83), before any code change.** In a temporary worktree of tag `m1` (`git -C C:\fl\a worktree add C:\fl\m1fix m1`), with `CARGO_BUILD_BUILD_DIR=C:/t/fl/m1fix` (this build is lane 2's builder), run `cargo run -q -p fl_cli -- save crates/fl_test_utils/tests/replays/testbed_paths.replay --tick 310 --out C:/t/fl/m1_testbed.save --json`. Expected JSON: `"tick": 310`, `"entities": 13`, `"pending_commands": 0`. At tick 310 the replay has a Shift-queued rotate and move (tick 200), a follower (#5, tick 300) and player 1's flow group. Copy the file to `crates/fl_test_utils/tests/replays/m1_testbed.save.new`, remove the worktree (`git worktree remove C:\fl\m1fix`), and paste the JSON, the byte size and the sha256 into the report.
- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_sim/src/units.rs (#[cfg(test)])
#[test] fn frozen_v2_reads_current_move_order_bytes()  // MoveOrder { orders: [Move (1, 2), Rotate 90°, Follow #3 (0, -8)] } → postcard →
                                                        //   v2::MoveOrder with the same three orders field by field
// crates/fl_test_utils/tests/m1_save.rs
#[test] fn m1_testbed_save_loads()                  // load_world(fl_catalog::features(), base_content(), fixture, hash_interval 1) → Ok;
                                                    //   SimClock tick 310; SimIndex len 13
#[test] fn m1_save_continues_with_m1_pose_trace()   // Scenario::load(fixture) then pose_trace(590) == the recorded trace's entries for ticks 310..=899
// crates/fl_test_utils/tests/golden.rs
#[test] fn testbed_paths_pose_trace_matches()       // pose_trace(testbed_paths().build(), TICKS) == read_pose_trace(replays/testbed_paths.poses);
                                                    //   with FL_RECORD_POSES=1 it writes replays/testbed_paths.poses.new instead
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -E 'test(/frozen_v2|m1_|pose_trace/)'`. Expected: FAIL (module and files missing).
- [ ] **Step 3: Implement.** `pose_hash` uses `fl_sim::StateHasher::new` and its `write_*` methods, so no new dependency.
- [ ] **Step 4: Record** `FL_RECORD_POSES=1 cargo nextest run --profile agent -p fl_test_utils --test golden -E 'test(pose_trace)'`, which writes `testbed_paths.poses.new` (900 lines). The tests that read the fixture and the trace pass only after the bless (step 6).
- [ ] **Step 5: Gate** `cargo xtask gate task --track sim-core`. Expected: PASS apart from the two tests that read the pending fixtures; paste which.
- [ ] **Step 6: Bless (lead)** `cargo xtask bless crates/fl_test_utils/tests/replays --reason "M2P Task 2: m1 testbed save fixture and testbed_paths pose trace (M11, R83)"`, then re-run the step 2 command. Expected: PASS.
- [ ] **Step 7: Commit** `test(fl_test_utils): m1 testbed save fixture, frozen MoveOrder v2, pose trace of the movement testbed` with trailers `Change-Class: R` and `Blessed: …`.

### Task 3: ADRs 0017–0022, refinements, spec revision 2.5

**Track:** contract (docs only) · **Lane 1**, alongside Task 1 (R21) · No cargo commands.

**Files:**
- Create (from `docs/adr/template.md`, MADR 4.0, status `proposed`): `docs/adr/0017-core-vocabulary.md`, `0018-unit-behaviour-and-entity-graph.md`, `0019-sim-event-model.md`, `0020-content-load-phases-patches-provenance.md`, `0021-golden-replay-versioning.md`, `0022-verification-from-m2.md`
- Modify: `docs/adr/{0002,0006,0007,0015}-*.md` (a `## Refinements (2026-10-25, M2 prelude)` section each), `docs/adr/README.md`, `docs/superpowers/specs/2026-10-23-frontline-rts-design.md`

**Decisions each ADR records** (the contracts later tasks implement):
- **ADR-0017, core vocabulary (C5).**
  - **Conditions.** Features register condition names (`SimBuilder::register_condition`, stable-string table `"conditions"`). A unit's `Conditions` component (Hashed `"core.conditions"`) holds sorted `(ConditionId, count)` pairs. A grant returns a `ConditionToken`, and revoking it decrements the count. A `requires` expression (`&&`, `||`, `!`, parentheses, condition names) compiles at content link time into a `ConditionExpr`; an unknown name is a content error.
  - **Stats and modifiers.** Stats are registered (`register_stat(name, StatDef { base, min, max })`, table `"stats"`). A modifier is `(stat, op ∈ {Add, AddPct, Mul, Override}, value: Fx, source: (key name, Option<SimId>))`, held in a unit's Hashed `"core.modifiers"`. Evaluation: `v = override.unwrap_or(base)`, clamp; `v += Σ add`, clamp; `v *= (1 + Σ add_pct)`, clamp; `v *= Π mul` one factor at a time, clamping after each; within each bucket, modifiers apply in `(source key, source SimId)` order. "Clamp" is to `[min, max]` of the stat. Bounds: `max ≤ 1,000,000`, `Mul` values in `[0, 100]`, `AddPct` in `[-1, 100]`, `Add` in `[-max, max]`, so no intermediate overflows `Fx`. Effective values are cached in the Derived `EffectiveStats`, recomputed for units marked dirty, at the start of `ThinkSet::Orders` and in `ResolveSet::Stats`. Only unit scope exists in the prelude; global, player, team and aura scopes come with their first user, applied before unit modifiers in the same buckets.
  - **Tags.** Content tags (object `tags:`) form the sorted table `"tags"`, at most 128. A unit's tags are a `TagSet(u128)`. A class expression (`vehicle & !truck & (tank | gun)`) compiles to `all_of`, `none_of` and one `any_of` mask; an unknown tag or a second `any` group is a content error.
  - **Verbs.** Effect, trigger-condition and trigger-action verbs are registered by name (tables `"effects"`, `"trigger_conditions"`, `"trigger_actions"`) and written in content as `[Name(args), …]` (`VerbList`). Effects run in `ResolveSet::Effects` as `EffectRequest` messages, in `(source SimId, emission order)` order. Trigger verbs have registries only, until map triggers exist.
  - **Command buttons.** A `ClientFeature` declares `CommandButton`s with an optional `requires`, evaluated on the client against the `core.conditions` view channel.
- **ADR-0018, unit behaviour and entity graph (C7).**
  - **Order queue.** `OrderQueue { orders: Vec<QueuedOrder> }` replaces M1's closed `Order` enum (R69) without a deprecation period, as R69 declared it interim. It keeps the Hashed state name `"core.move_order"` and becomes codec v3, migrated from the frozen v2 with `Codec::migrate_with`. `QueuedOrder { kind: OrderKindId, target: OrderTarget, param: Vec<u8> }`. Order kinds are registered (table `"orders"`); the dense id is saved and hashed by name. At most `MAX_WAYPOINTS` (32) orders, a code constant: it bounds the save decoder, so it can't be content.
  - **Activity stack.** `ActivityStack { frames: Vec<Activity> }` (Hashed `"core.activity_stack"`, top = last, at most 8 frames). `Activity { kind: ActivityKindId, target, param, state: Active | Canceling }`. An order becomes current by pushing its root activity (`OrderBehaviour::begin`); `run_activities` ticks the top frame of every unit in `SimIndex` order (`ActivityBehaviour::tick → Continue | Done(Completed | Failed) | Push(child)`). An emptied stack pops the current order. Cancelling runs `cancel` top-down. State lives in these table-stored components; SparseSet markers are allowed only for rare states that need filtering (`Embarked`, later).
  - **Relationships.** `RelatedTo<R>(SimId)` (Hashed, `R::NAME`) on the source and `Related<R>` (Derived, sources sorted by `SimId`) on the target, maintained by component hooks and rebuilt after a load by the framework, before feature rebuilds. One-to-one relations reject a second source (`RelationError::TargetTaken`). When a target despawns, features apply their rule in `LifecycleSet::Death`, and the despawn funnel then removes any `RelatedTo<R>` still pointing at it, in `SimId` order. No `ChildOf`, no `linked_spawn`. Ownership stays `Owner(PlayerId)`: players are not entities.
  - **Hashed state that names derived layouts** (M1 final review, focus 2): any cache key stored in Hashed state, like `PathState::Flow { key }`, needs a re-plan when its layout rules change (R129). The ADR lists it as a rule for new keys.
- **ADR-0019, sim event model (C7).** Messages are the default for gameplay events. Component hooks maintain indices and invariants only. Observers are allowed in sim crates only for order-independent reactions: insert a marker or push into a buffer; never draw randomness, allocate `SimId`s or write shared state. Entity disabling is not used in the sim. The reviewer checklist and `.claude/rules/sim.md` carry the rule.
- **ADR-0020, content load phases, patches and provenance (C6, C2).**
  - **Phases:** discover → parse (a lossless RON tree, `ContentTree`, with spans) → migrate (`mod.ron` `format_version`, default 1, current 1) → patch (`*.patch.ron`: `Set`, `Merge`, `Remove`, `Add`, `AddTag`, `If`; pass `Default` then pass `Final`, each in mod load order, then file order) → inherit (per component) → typed deserialize → feature post-process hooks (feature id order) → link → freeze → rules hash.
  - **Why an own tree:** ron 0.12's `deserialize_any` reports bare identifiers as unit (`handle_any_struct`), so a generic value tree loses enum variants such as `class: Tracked`. The own parser keeps identifiers and struct names. Patch values for enum variants may be written as identifiers or strings.
  - **Path keys:** `<doc>/<field path>`, where `doc` is `rules` or `<kind>:<id>` (`unit:base.de.pz3`, `terrain:base.forest`) and the field path is dot-separated, with map keys as written and sequence indices as numbers: `unit:base.de.pz3/components.Hull.footprint.0`.
  - **Origins:** every leaf records the mod and file that last set it; inherited components keep their parent's.
  - **Provenance ledger (C2):** `<mod>/provenance/*.prov.ron` maps path keys to `(src, conf)` with `src ∈ {Original(file, note), Manual(section), ResearchDerived(doc), OursScaled(from), Guess(id), Tuned(mcr, sweep)}` and `conf ∈ {High, Medium, Low}`. Every tunable leaf (everything in `rules.ron`, number, boolean and identifier leaves under `components.` of objects, and the leaves a kind lists as tunable) set by a mod needs an entry in that mod's ledger; a key naming no such leaf is an error. Ledgers are not content and never enter the rules hash. `Guess` ids are defined in a mechanic spec's Guesses section or `docs/design/GDD.md` §Guesses; `spec-lint` checks them.
  - **Deprecated keys:** `DefRegistry::deprecate_field` renames an old key during migrate, with a `data-check` warning.
- **ADR-0021, golden replay versioning and change classes (C8).**
  - Replay format 4: `ReplayStart::Scale { version, params }` (M10: a `ScaleScenario` field no longer bumps the replay format), named semantic checks next to the hashes, and a journal summary (record counts by kind) for re-bless reports. Format 3 still reads.
  - **`SIM_VERSION` policy:** bumped by every class F change, at most once per task; class R and E changes keep every golden hash stream bit-identical.
  - **MCR:** a `## MCR-nnn` entry in `docs/design/changelog-mechanics.md` with why, requirement changes, the predicted effect and the `SIM_VERSION` change, written before the code.
  - **Re-bless:** only `fl_cli rebless --reason MCR-nnn`: it refuses an MCR without an entry and any failing semantic check, writes `.new` replays and a report (first divergent tick, journal summary diff, outcome diff), and the lead blesses them (R146), listed for the user's ratification.
  - **Gate rules:** a changed golden needs an `MCR:` trailer and a changed `SIM_VERSION`, replay format or content; a changed `SIM_VERSION` needs a changed golden; every code or content commit carries `Change-Class:`.
- **ADR-0022, verification from M2 (C1, C3, C4, C9, C10, I2, M17, R139, R140).**
  - **Mechanic specs (C1):** `docs/design/GDD.md` indexes one spec per mechanic, `docs/design/mechanics/<AREA>-<name>.md` (protected), in the template of `docs/design/mechanic-template.md` (EARS rules with ids `AREA.NAME.N`, formulas, a parameter table with path keys and provenance, acceptance criteria naming tests, guesses). `cargo xtask spec-lint` checks them; `cargo xtask trace` maps `// @req AREA.NAME.N` tags in tests to rules, and an untraced rule of an `approved` or `implemented` spec fails.
  - **Test author and holdout (C3):** the test-author writes acceptance tests (`tests/acceptance/`, every level) and holdout tests (`tests/holdout/`, built only with the `holdout` feature, run at nightly and milestone) from the spec alone. Holdout is at least 10 % of a spec's acceptance and holdout tests (milestone FAIL below 10 %, WARN above 20 %). Implementers may not read holdout files (agent guard).
  - **Statistical policy (C4):** oracles from the spec or an f64 reference; `assert_rate_within_wilson` at α = 1e-4; formula tier 10⁵ draws, scenario tier 200–500 seeds; nightly sets `FL_SEED_BASE` to the days since 2026-01-01; one recorded reroll (`seed_base + 1`) allowed.
  - **Mutation testing (C9):** `cargo xtask mutants` wraps cargo-mutants in copy mode with its own build dir and a time cap; the controller runs it per task at review (`--cap 600`), pr runs it in-diff (WARN), nightly runs the full set in 4 shards (WARN), and milestone fails on a missed mutant not listed in `xtask/mutants-triage.toml`.
  - **Risk-tiered review (C10):** `cargo xtask review-tier` names tier P (protected paths: the lead, listed for the user), A (contract or sim core: spec and determinism review plus quality review), B (client, tools: one combined review) or C (docs, data with data-check green: one short review). `cargo xtask mini-gate <AREA.NAME>` runs one mechanic's lint, trace and traced tests, holdout included.
  - **Release determinism (I2):** a `release-determinism` row at nightly and milestone runs the headless sim tests under `[profile.release-check]` (release with `overflow-checks = true`), including the debug-recorded goldens.
  - **Gate time (M17), journal (R139), proptests (R140, R142):** the task level runs touched packages minus `xtask/heavy-tests.txt`, plus the smoke set; journal readers opt in; one proptest policy.
- **Refinements.**
  - ADR-0002: `FromRunner` becomes `#[non_exhaustive]`; `ToRunner::reply_id()` lets the threaded runner answer an unsupported message with `SimReply::Error` (M3, R123).
  - ADR-0006: `Codec::migrate_with(version, fn(Old, &LoadCtx) -> Result<T, String>)` for migrations that need the string tables; `LoadCtx::current_id(table, name)`; core version 3 (Task 13: fill `core.activity_stack` from the migrated queue) and 4 (Task 15); relationship codecs save `SimId`s.
  - ADR-0007: `MAX_COMMAND_UNITS` moves to `fl_protocol` (the client reads it there); the reject code `core.no_map` (`RejectReason::Feature`) replaces `core.no_valid_units` when no map is set up (M14).
  - ADR-0015: the path queue has two FIFO lanes, command-issued requests (enqueued in `Phase::Input`) before background ones, with at least one background request processed per tick when its lane is not empty (M5); a flow unit falls back to a single path only after 10 consecutive ticks without moving (M4); a value reload that changes nav content invalidates every followed path (M1); `core.passability` takes `since_version` (M8); `core.paths` answers `{"state": "immobile"}` for a unit without `Mobility` (M7).

**Spec edits** (same commit): a section `## Revision 2.5 (2026-10-25, M2 prelude contracts)` above Revision 2.4, one bullet per ADR and refinement above in 2.4's format; the status line names revisions 2.4 and 2.5; §5.4's observer hazard line points at ADR-0019; §6.1's last line points at ADR-0020; §9.2 gains "Delivered by `docs/superpowers/plans/2026-10-25-frontline-m2-prelude.md`"; §12.5 adds that re-blesses follow R146.

- [ ] **Step 1: Write ADR-0017 to ADR-0022** with Context, Decision Drivers, at least two Considered Options with pros and cons, Decision, Consequences and Confirmation. Confirmation bullets name the tests and gate rows of this plan by name.
- [ ] **Step 2: Write the four refinement sections and the README rows** (status `proposed`).
- [ ] **Step 3: Edit the spec** as listed.
- [ ] **Step 4: Verify.** `git diff --stat` lists only the files above; `grep -c '^status: "proposed"' docs/adr/001[7-9]-*.md docs/adr/002[0-2]-*.md` prints `1` for each file.
- [ ] **Step 5: Acceptance (controller, R85).** The controller reviews and accepts: `status: "accepted"` with the date. Each accepted ADR goes on Task 21's ratification list.
- [ ] **Step 6: Commit** `docs(adr): ADR-0017–0022 (core vocabulary, unit behaviour, event model, content phases, golden versioning, verification); refine 0002, 0006, 0007, 0015` with trailer `Refs: ADR-0011`.

### Task 4: GDD, mechanic template, M1 mechanic specs, MCR changelog

**Track:** docs (protected specs as `.new`) · **Refs:** ADR-0021, ADR-0022 · **Lane 1**, alongside Task 1 (R21) · No cargo commands.

**Files:**
- Create: `docs/design/GDD.md`, `docs/design/mechanic-template.md`, `docs/design/changelog-mechanics.md`
- Create as `.new` (protected): `docs/design/mechanics/{MOVE-speed,PATH-requests,FLOW-groups,ORDER-queue,STEER-separation}.md.new`

**Content decisions:**
- **`GDD.md`** (at most 300 lines): pillars and anti-goals (spec §1–2, condensed); a glossary (pass, terrain factor, profile, clearance, cluster, flow field, cold cost, order, activity, condition, modifier, MCR, change class); the `SimTick` phase and sub-set order (spec §5.6); the **mechanic registry** table (`ID | Name | Crate | Milestone | Status | Spec | Owner track`), one row per spec below; **§Guesses** (`G-CORE-1` `rules/StateHash.interval_ticks` = 20 from spec §5.4; `G-CORE-2` `rules/Spatial.cell_m` = 32; `G-ROSTER-1` every unit's `Hull.hp`; `G-ROSTER-2` every unit's `Mobility.turn_rate_deg_s`); the `SIM_VERSION` policy, change classes and the MCR and re-bless process (ADR-0021, R146); the statistical policy (ADR-0022); the review tiers and mini-gate (ADR-0022).
- **`mechanic-template.md`**: the format `spec-lint` parses (Task 9), exactly:
  - line 1 `# <AREA.NAME> — <title>`; line 2 `status: draft|approved|implemented|deprecated · spec-version: <n> · crate: <module path> · milestone: M<n>`; line 3 `Depends on: <ids or none> · Emits journal records: <kinds or none>`;
  - sections `## Intent`, `## Rules` (lines `- <AREA.NAME.N>  <EARS sentence>`), `## Formulas`, `## Parameters` (table `| Name | Path key | Unit | Default | Valid range | Provenance | Confidence |`; a path key may use `*` for one id or path segment), `## Acceptance criteria` (lines `- AC-<n> (<rule ids or ranges>): <criterion> → <crates/…/tests/….rs>::<test fn>`), `## Non-goals · Open questions · Guesses` (guess lines `- Guess G-<AREA>-<n>: <text>`), `## Changelog`.
- **The five M1 specs** describe M1 as built (status `implemented`, spec-version 1), from spec §7, ADR-0014, ADR-0015, the M1 plan and rulings R93–R137, with 5–10 rules each. Every acceptance criterion names an existing M1 test (read the test files; never invent a name). Guesses:
  - `MOVE.SPEED` (`MOVE-speed.md`, crate `fl_sim::movement`): speed formula with the `[min_speed_frac, 1]` clamp, class blocking, slope blocking, clearance, hull turning before moving. Guesses `G-MOVE-1` min_speed_frac, `G-MOVE-2` max_slope_m, `G-MOVE-3` terrain `pass` values, `G-MOVE-4` turn_in_place_deg, `G-MOVE-5` arrive_m.
  - `PATH.QUEUE` (`PATH-requests.md`, `fl_sim::path`): FIFO, budget, cold-cost charge, stale requests at charge 0, request-order application, partial and unreachable plans, the HPA* cost bound (R136). Guesses `G-PATH-1` cluster_cells, `G-PATH-2` budget_per_tick, `G-PATH-3` max_clearance, `G-PATH-4` snap_radius_cells, `G-PATH-5` path_terrain_factor.
  - `FLOW.GROUP` (`FLOW-groups.md`, `fl_sim::nav::flow`): group threshold, one request per group, outcome-neutral cache, tick-boundary eviction (R124). Guesses `G-FLOW-1` flow_min_group, `G-FLOW-2` flow_cache_max and flow_cache_hard_max.
  - `ORDER.QUEUE` (`ORDER-queue.md`, `fl_sim::orders`): replace versus Shift-queue, the 32-order cap and `core.queue_full`, stop, rotate facing (R105), follow (R106, R107), formation slots and spacing (R110). Guesses `G-ORDER-1` formation_max_spread_m, `G-ORDER-2` follow_distance_m, `G-ORDER-3` follow_repath_ticks.
  - `STEER.SEP` (`STEER-separation.md`, `fl_sim::steer`): pair distance and push, idle cadence and cap (R113), slide order (R114), movers push idle units (R115), stranded re-plan (R116). Guesses `G-STEER-1` separation_gain, `G-STEER-2` separation_margin_m, `G-STEER-3` idle_every, `G-STEER-4` max_push_frac.
  - Parameter rows use path keys under `rules/Movement.*`, `rules/Path.*`, `rules/Steering.*`, `terrain:*/pass`, `unit:*/components.Mobility.*`; the Provenance column is the class Task 8 assigns (`Guess`, `ResearchDerived`, `OursScaled`).
- **`changelog-mechanics.md`**: the MCR entry format (ADR-0021) and an empty list; Tasks 5, 13 and 15 add MCR-000, MCR-001 and MCR-002.

- [ ] **Step 1: Write `GDD.md`, `mechanic-template.md` and `changelog-mechanics.md`.**
- [ ] **Step 2: Write the five specs as `.md.new`.** Check each against the template by hand: unique rule ids, every rule covered by an acceptance criterion, every criterion's test exists (`grep -n "fn <name>" <file>`), every guess id unique. Task 9's `spec-lint` re-checks them mechanically.
- [ ] **Step 3: Verify.** `wc -l docs/design/GDD.md` is at most 300; `git status --short` lists only the files above.
- [ ] **Step 4: Bless and approve (lead).** The lead reads the specs (status `implemented` describes built behaviour, so the lead approves under the user's delegation and lists the five specs for ratification in Task 21) and runs `cargo xtask bless docs/design/mechanics --reason "M2P Task 4: M1 mechanic specs (backfill, C1)"`.
- [ ] **Step 5: Commit** `docs(design): GDD index, mechanic template, M1 mechanic specs, MCR changelog` with trailers `Refs: ADR-0021, ADR-0022` and `Blessed: …`.

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

### Task 6: Journal opt-in, proptest policy, release-profile determinism (R139, R140, I2)

**Track:** contract + sim-core + content + tools · **Refs:** ADR-0022 · **Lane 1** (after 5)

**Files:**
- Modify: `Cargo.toml` (`[profile.release-check]`), `crates/fl_test_utils/src/{builder,lib}.rs`, `xtask/src/gate.rs`, `.claude/rules/sim.md`, and the tests that read the journal or run proptests: `crates/fl_sim/tests/{flow,observe,orders,pathing,reload,scale_mix,steering,terrain,astar,spatial,hpa}.rs`, `crates/fl_test_utils/tests/{m0_save,movement_determinism,determinism}.rs`, `crates/fl_content/tests/terrain.rs`, `crates/fl_math/tests/fx_props.rs`, `crates/fl_cli/tests/agent.rs`
- Create: `crates/fl_test_utils/src/proptest_policy.rs`

**Interfaces:**
- Consumes: `ScenarioBuilder`, `RunConfig::journal`, `proptest::test_runner::{Config, RngAlgorithm, RngSeed, TestRunner}`.
- Produces:
```rust
// crates/fl_test_utils/src/builder.rs
impl ScenarioBuilder { pub fn journal(self, on: bool) -> Self; }      // RunConfig.journal = Some(on); default None (debug on, release off)
impl Scenario { pub fn journal(&self) -> &[JournalEntry]; }
    // panics unless built with .journal(true): "Scenario::journal needs ScenarioBuilder::journal(true): determinism tests never read the journal (R139)"
// crates/fl_test_utils/src/proptest_policy.rs — depends only on proptest and std, so fl_math and fl_content tests include it with
//   #[path = "../../fl_test_utils/src/proptest_policy.rs"] mod proptest_policy;   (R67 precedent)
pub const PROPTEST_SEED: u64 = 0x5EED_F00D_CAFE_0001;
pub fn cases(default: u32, max: u32) -> u32;          // PROPTEST_CASES if set, else default; at most max
pub fn config(default: u32, max: u32) -> proptest::test_runner::Config
    // cases(default, max), rng_algorithm ChaCha, rng_seed Fixed(PROPTEST_SEED) unless FL_PROPTEST_RANDOM=1 (then Random),
    // failure_persistence None; reads the environment under #[allow(clippy::disallowed_methods, reason = "…")] (allow, not expect:
    // the ban is not configured in every including crate)
pub fn runner(default: u32, max: u32) -> proptest::test_runner::TestRunner
// Cargo.toml
[profile.release-check]  inherits = "release", overflow-checks = true, debug-assertions = false
// xtask/src/gate.rs: row "release-determinism" (Nightly, Milestone):
//   cargo nextest run --cargo-profile release-check --profile agent -p fl_sim -p fl_test_utils -p fl_catalog -p fl_content -E 'not (<exclusions>)'
//   nightly also sets FL_PROPTEST_RANDOM=1 on the test-headless row (not on release-determinism)
```

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_test_utils/tests/determinism.rs
#[test] fn journal_does_not_change_hashes()     // testbed_paths().journal(true) and .journal(false), 300 ticks each → equal hash streams
#[test] #[should_panic(expected = "ScenarioBuilder::journal(true)")] fn journal_needs_opt_in()  // a default-built scenario's journal()
// crates/fl_test_utils/src/proptest_policy.rs (#[cfg(test)])
#[test] fn cases_respects_cap()                 // with PROPTEST_CASES unset: cases(12, 64) == 12; cases(600, 512) == 512
#[test] fn config_is_fixed_and_unpersisted()    // config(8, 8): rng_seed Fixed(PROPTEST_SEED), failure_persistence None, rng_algorithm ChaCha
// crates/fl_sim/tests/hpa.rs: one named test per `cc` line of hpa.proptest-regressions
#[test] fn hpa_regression_1()                   // the shrunk input from the file's first comment, run through the same property body
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_test_utils -p fl_sim -E 'test(/journal_|cases_respects|config_is_fixed|hpa_regression/)'`. Expected: FAIL.
- [ ] **Step 3: Implement the helper and the builder option.** Then migrate:
  - every test that calls `Scenario::journal`, reads `TickOutput::journal` or the `Journal` resource builds with `.journal(true)` (or sets `RunConfig.journal = Some(true)`); find them with `cargo nextest run --cargo-profile release-check --profile agent -p fl_sim -p fl_test_utils -p fl_cli` (40 failed in release in M1 Task 14) plus the new panic;
  - every `proptest!` block uses `#![proptest_config(proptest_policy::config(<its current case count>, <cap>))]`, and every `TestRunner::new*` uses `proptest_policy::runner`; whole-match proptests keep their caps (R142: 64 at gate, 512 at milestone);
  - each `cc` line of a committed `*.proptest-regressions` file becomes a named test; the file stays (protected).
  - `.claude/rules/sim.md` gains the R139 and R140 rules.
  List every test-guard finding (`NewSeedOverride`, `FewerProptestCases`) in the report for the reviewer.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_math -p fl_content -p fl_sim -p fl_test_utils -p fl_cli`. Expected: PASS.
- [ ] **Step 5: Run the release row once** `cargo nextest run --cargo-profile release-check --profile agent -p fl_sim -p fl_test_utils -p fl_catalog -p fl_content -E 'not (test(/gpu_/) | test(/brp_/))'` (a cold release build; this is the lane's builder). Expected: PASS, the goldens recorded in debug verifying in release (I2's cross-profile check). Paste the summary.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track sim-core --track content --track tools`. Expected: PASS.
- [ ] **Step 7: Commit** `test: journal readers opt in (R139), one proptest policy (R140), release-check determinism row (I2)` with trailers `Change-Class: R`, `Refs: ADR-0022`.
- [ ] **Step 8: Checkpoint (controller).** `cargo xtask gate pr` on `m2p/phase-a` with Task 2 merged. Expected: `gate pr: PASS`. Fast-forward `main`.

### Task 7: Content tree pipeline and per-path origins (C6, part 1)

**Track:** contract + content · **Refs:** ADR-0020 · **Lane 2** (`C:\fl\m2p07`, after 2 and 3)

**Files:**
- Create: `crates/fl_content/src/tree.rs`, `crates/fl_content/tests/tree.rs`
- Modify: `crates/fl_content/src/{object,inherit,mods,db,error,lib}.rs`
- Blessed: `xtask/snapshots/api/fl_content.txt`

**Interfaces:**
- Consumes: `load_dirs`, `parse_object`, `resolve_objects`, `DefRegistry` (component, kind and rules-section seeds), `ContentError`.
- Produces:
```rust
// crates/fl_content/src/tree.rs
#[derive(Clone, Debug, PartialEq)] pub struct Span { pub line: u32, pub col: u32 }
#[derive(Clone, Debug, PartialEq)]
pub enum ContentTree {
    Unit, Bool(bool), Int(i128), Float(f64), Str(String),
    Ident(String),                                            // a bare identifier: an enum unit variant (`Tracked`)
    Option(Option<Box<Node>>), Seq(Vec<Node>), Tuple(Vec<Node>),
    Map(Vec<(Node, Node)>),                                   // `{ k: v }`, in written order
    Struct { name: Option<String>, fields: Vec<(String, Node)> },        // `Name(a: 1)` or `(a: 1)`
    TupleStruct { name: Option<String>, items: Vec<Node> },              // `Name(1, 2)`
}
#[derive(Clone, Debug, PartialEq)] pub struct Node { pub value: ContentTree, pub span: Span }
pub fn parse_ron(file: &Path, text: &str) -> Result<Node, ContentError>
    // the RON subset our content uses: // and /* */ comments, named and unnamed structs and tuples, lists, maps, strings with \" \\ \n \t \u{…}
    // escapes, decimal integers, floats (digits with '.' or exponent; parsed with str::parse::<f64>), true/false, Some(..)/None, (), bare
    // identifiers, trailing commas; anything else is an error naming file, line and column
pub fn to_ron(node: &Node) -> String                         // a printer whose output parse_ron reads back equal (spans aside)
pub struct TreeDeserializer<'a> { .. }                       // serde::Deserializer over a &Node: enums from Ident/Str (unit), single-field
                                                             //   Struct/TupleStruct named as the variant, or one-entry Map; is_human_readable true;
                                                             //   errors carry the field path and the node's span
pub fn from_tree<T: DeserializeOwned>(node: &Node, file: &Path) -> Result<T, ContentError>
pub fn path_key(doc: &str, path: &[PathSeg]) -> String      // "unit:base.de.pz3/components.Hull.footprint.0", "rules/Movement.min_speed_frac"
pub enum PathSeg { Field(String), Index(usize) }
// crates/fl_content/src/db.rs
#[derive(Clone, Debug, PartialEq, Eq)] pub struct Origin { pub mod_id: String, pub file: PathBuf }
impl ContentDb { pub fn origins(&self) -> &BTreeMap<String, Origin>; }   // every leaf of every document: objects (abstract ones too), kinds, rules
```
- `load_dirs` keeps its signature. Its phases become: discover → `parse_ron` per file → inherit per component on trees (child replaces the parent's component whole; `remove` drops it; inherited leaves keep the parent's origin) → typed deserialize through the registry seeds over `TreeDeserializer` → link → freeze → hash. Migrate and patch arrive in Task 10, between parse and inherit.

- [ ] **Step 1: Write the failing tests** (`crates/fl_content/tests/tree.rs`):
```rust
#[test] fn identifiers_survive_parsing()          // "(class: Tracked, p: Box(size: (1.0, 2.0)))" → Ident("Tracked"), Struct { name: Some("Box") }
#[test] fn floats_parse_like_ron()                // for "0.05", "2.8", "1e-3", "-4.0", "40": from_tree::<Fx>() == ron::from_str::<Fx>()
#[test] fn printer_round_trips()                  // proptest (proptest_policy::config(256, 1024)): random trees → to_ron → parse_ron equal, spans aside
#[test] fn unsupported_syntax_names_position()    // "(a: 'c')" → error "line 1, column 5" and "char literals are not supported"
#[test] fn typed_errors_name_field_path()         // a Hull with "hp: -1" → error path "components.Hull.hp" with its line
#[test] fn tree_load_matches_typed_load()         // the base mod and every fixture mod (crates/fl_sim/tests/fixtures/**, fl_cli fixtures): the rules
                                                  //   hash equals the hash committed for it (base: data-check's rules hash at the task base)
#[test] fn origins_cover_every_leaf()             // base: "unit:base.de.pz3/components.Hull.hp" → mod "base", file …/pz3.unit.ron;
                                                  //   "unit:base.de.pz2/components.Selectable" absent (pz2 inherits it: origin under the template's key)
#[test] fn inherited_leaf_keeps_parent_origin()   // a fixture child without "Hull": its resolved Hull came from the template file
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_content --test tree`. Expected: FAIL.
- [ ] **Step 3: Implement.** Before changing `load_dirs`, record the current rules hash of every fixture and base mod (`cargo run -q -p fl_cli -- data-check <dir> --json`) into the test's expectations; `tree_load_matches_typed_load` must then pass unchanged, which proves the new pipeline produces the same typed content.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_content -p fl_catalog -p fl_sim -p fl_test_utils -p fl_cli`. Expected: PASS; golden replays verify unchanged (the rules hash is unchanged).
- [ ] **Step 5: Regenerate** the `fl_content` API snapshot (`cargo xtask api`); the lead blesses it: `M2P Task 7: content tree and origins`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track content`. Expected: PASS, with the content tests run.
- [ ] **Step 7: Commit** `feat(fl_content): lossless RON tree, typed deserialize from trees, per-path origins` with trailers `Change-Class: R`, `Refs: ADR-0020`, `Blessed: …`.

---

## Phase II — provenance, tracing, acceptance pipeline, order queue

### Task 8: Provenance ledger and tuning tables (C2)

**Track:** content + tools · **Refs:** ADR-0020 · **Lane 2** (after 4 and 7)

**Files:**
- Create: `crates/fl_content/src/provenance.rs`, `crates/fl_content/tests/provenance.rs`, `assets/base/provenance/{rules,units,terrain}.prov.ron`, `crates/fl_cli/src/tuning_tables.rs`, `crates/fl_cli/tests/tuning_tables.rs`, `docs/design/generated/tuning-tables.md`
- Modify: `crates/fl_content/src/{mods,db,kinds,terrain,lib}.rs`, `crates/fl_cli/src/{data_check,main,lib}.rs`, `xtask/src/gate.rs` (the `data_check` arguments)
- Blessed: `xtask/snapshots/api/fl_content.txt`

**Interfaces:**
- Consumes: `ContentDb::origins`, `parse_ron`, `from_tree`, `path_key`.
- Produces:
```rust
// crates/fl_content/src/provenance.rs
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvSource { Original { file: String, note: Option<String> }, Manual { section: String }, ResearchDerived { doc: String },
    OursScaled { from: String }, Guess { id: String }, Tuned { mcr: String, sweep: String } }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum Confidence { High, Medium, Low }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] #[serde(deny_unknown_fields)] pub struct ProvEntry { pub src: ProvSource, pub conf: Confidence }
// file: Provenance({ "<path key>": (src: Guess(id: "G-MOVE-1"), conf: Low), … })
impl ContentDb { pub fn provenance(&self) -> &BTreeMap<String, ProvEntry>; }
// ContentKind gains `const TUNABLE: &'static [&'static str] = &[];`  — TerrainDef: &["pass", "blocks"]
// load rules: `provenance/` files are read per mod and never enter the rules hash; every tunable leaf (rules.ron leaves; number, bool and
//   identifier leaves under `components.` of objects; a kind's TUNABLE fields and their elements) whose origin is mod M needs an entry in M's
//   ledger ("<file>: no provenance for <key>"), judged against the final origins; an entry of M's ledger naming no tunable leaf of M's own
//   pre-patch documents (nor a leaf M's patches write, from Task 10) is an error ("<ledger file>: stale provenance key <key>"), judged per mod
//   (R151); `Guess.id` must match `G-[A-Z]+-[0-9]+`; `Tuned.mcr` must match `MCR-[0-9]{3}`
// test fixture mods (a mod root under crates/**/tests/fixtures/**) without a `provenance/` directory are exempt from these rules (R151); a
//   fixture that tests the ledger opts in by carrying one, as the fixtures of this task's tests do
// crates/fl_cli: `fl_cli tuning-tables [dirs] [--out docs/design/generated/tuning-tables.md] [--check]`
//   one table per document: | Key | Value | Source | Confidence |, values printed with to_ron; --check exits 1 when --out differs
// data-check gains `--tuning-tables <path>`: also fails when that file is not what tuning-tables would write; the gate's data-check row passes
//   `--tuning-tables docs/design/generated/tuning-tables.md`
```
- **Classification of the current base values** (the `// guess` comments become `Guess`; spec ids from Task 4):

| Path keys | Source | Confidence |
|---|---|---|
| `rules/StateHash.interval_ticks`, `rules/Spatial.cell_m` | `Guess` `G-CORE-1`, `G-CORE-2` | Medium |
| `rules/Movement.{min_speed_frac,max_slope_m.*,turn_in_place_deg,arrive_m}` | `Guess` `G-MOVE-1`, `G-MOVE-2`, `G-MOVE-4`, `G-MOVE-5` | Low |
| `rules/Movement.{formation_max_spread_m,follow_distance_m,follow_repath_ticks}` | `Guess` `G-ORDER-1..3` | Low |
| `rules/Movement.path_terrain_factor.*`, `rules/Path.*` | `Guess` `G-PATH-5`, `G-PATH-1..4`, `G-FLOW-1`, `G-FLOW-2` | Low (`Path.cluster_cells`: Medium, spec §5.7) |
| `rules/Steering.*` | `Guess` `G-STEER-1..4` | Low |
| `terrain:*/pass` | `Guess` `G-MOVE-3` | Low |
| `terrain:*/blocks.*` | `ResearchDerived` `docs/research/blitzkrieg-mechanics.md §1.8` | Medium |
| `unit:*/components.Hull.hp`, `unit:*/components.Mobility.turn_rate_deg_s` | `Guess` `G-ROSTER-1`, `G-ROSTER-2` | Low |
| `unit:*/components.Hull.hitbox_shrink` | `ResearchDerived` `blitzkrieg-mechanics.md §1.3 (SmallAABBCoeff)` | Medium |
| `unit:*/components.Hull.footprint.*`, `unit:*/components.Mobility.road_speed_kmh` | `OursScaled` "historical hull dimensions and road speeds, our own numbers" | Medium |
| `unit:*/components.Mobility.terrain_factor` | `OursScaled` "blitzkrieg-mechanics.md §1.8 unit passability, rescaled" | Low |
| `unit:*/components.Mobility.class` | `ResearchDerived` `blitzkrieg-mechanics.md §1.2` | High |

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_content/tests/provenance.rs
#[test] fn base_ledger_is_complete()            // load the base mod → Ok; provenance().len() == origins() tunable leaf count
#[test] fn missing_ledger_entry_is_error()      // a fixture mod without "rules/Movement.arrive_m" → error naming rules.ron and that key
#[test] fn stale_ledger_key_is_error()          // an entry "unit:base.de.pz9/components.Hull.hp" → error "stale provenance key"
#[test] fn fixture_mods_are_exempt_from_provenance() // a mod under crates/fl_content/tests/fixtures/ without provenance/ → loads with no ledger error
#[test] fn closed_vocabulary()                  // src: Hunch(id: "x") → parse error naming the ledger file
#[test] fn bad_guess_and_mcr_ids_are_errors()   // Guess(id: "guess1"), Tuned(mcr: "14", sweep: "s.json") → two errors
#[test] fn ledger_is_not_hashed()               // the base rules hash with and without the ledger files is equal
// crates/fl_cli/tests/tuning_tables.rs
#[test] fn tuning_tables_lists_every_key()      // output has a row "rules/Movement.min_speed_frac | 0.05 | Guess G-MOVE-1 | Low"
#[test] fn tuning_tables_check_detects_drift()  // --check against an edited copy → exit 1
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_content -p fl_cli -E 'test(/ledger|provenance|vocabulary|guess|tuning|fixture_mods/)'`. Expected: FAIL.
- [ ] **Step 3: Implement**, then write the three ledger files from the table above (one entry per tunable leaf the loader reports; `data-check` prints the missing keys), and generate `docs/design/generated/tuning-tables.md` with `fl_cli tuning-tables --out …`. Remove the `// guess` comments from `rules.ron` and the terrain files: the ledger now carries them.
- [ ] **Step 4: Run** the step 2 command and `cargo run -q -p fl_cli -- data-check assets/base --tuning-tables docs/design/generated/tuning-tables.md`. Expected: PASS; `data-check` exits 0 with the same rules hash as before (the comments were not hashed).
- [ ] **Step 5: Regenerate** the `fl_content` API snapshot; the lead blesses it: `M2P Task 8: provenance ledger`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track content --track tools --track contract`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_content): provenance ledger with every base value classified; fl_cli tuning-tables` with trailers `Change-Class: N`, `Refs: ADR-0020`, `Blessed: …`.

### Task 9: Spec-lint, trace and the `@req` backfill (C1)

**Track:** contract + tools + sim-core · **Refs:** ADR-0022 · **Lane 1** (after 6 and 8)

**Files:**
- Create: `xtask/src/spec/{mod,parse,lint,trace}.rs`, `crates/fl_cli/src/schema.rs`, `crates/fl_cli/tests/schema.rs`
- Modify: `xtask/src/{gate,main}.rs`, `crates/fl_cli/src/{main,lib}.rs`; comment lines (`// @req …`) in the existing tests named by the five specs' acceptance criteria
- Possibly re-blessed: `docs/design/mechanics/*.md` (fixes `spec-lint` finds)

**Interfaces:**
- Consumes: the spec format of `docs/design/mechanic-template.md`, `ContentDb::provenance`, the string tables, `debug_query("core.debug_queries")`.
- Produces:
```rust
// crates/fl_cli: `fl_cli schema [dirs] --json` → {"tables": {<table>: [names]}, "debug_queries": [..],
//    "provenance": {<key>: {"class": "Guess", "id": "G-MOVE-1", "conf": "Low"}}}  (catalog features, base content)
// xtask/src/spec/parse.rs
pub enum SpecStatus { Draft, Approved, Implemented, Deprecated }
pub struct Spec { pub id: String, pub file: PathBuf, pub status: SpecStatus, pub version: u32, pub module: String, pub milestone: String,
    pub journal: Vec<String>, pub rules: Vec<Rule>, pub params: Vec<Param>, pub acs: Vec<Ac>, pub guesses: Vec<String> }
pub struct Rule { pub id: String, pub line: usize }
pub struct Param { pub key: String, pub provenance: String, pub line: usize }
pub struct Ac { pub id: String, pub covers: Vec<String>, pub test: Option<(PathBuf, String)>, pub line: usize }
pub fn parse_spec(path: &Path, text: &str) -> Result<Spec, Vec<String>>      // "file:line: …" messages
// xtask/src/spec/lint.rs
pub struct Schema { pub tables: BTreeMap<String, Vec<String>>, pub provenance: BTreeMap<String, (String, Option<String>)> }
pub fn lint(root: &Path, specs: &[Spec], gdd: &str, changelog: &str, schema: &Schema) -> Vec<String>
    // rule ids `^[A-Z]+\.[A-Z_]+\.[0-9]+$`, unique, prefixed by their spec id; every rule covered by an AC; every AC of a non-draft spec names
    // an existing file containing `fn <test>`; every param key pattern (`*` = one segment) matches ≥ 1 provenance key, and its Provenance
    // column equals the class of every match ("mixed: A, B" lists several); every journal kind is in tables["journal_records"]; every guess id
    // is unique across specs and GDD §Guesses, and every provenance Guess id is defined; every Tuned mcr has a changelog heading; the GDD
    // registry table lists exactly the spec files
// xtask/src/spec/trace.rs
pub enum Suite { Unit, Acceptance, Holdout }
pub struct Tag { pub id: String, pub file: PathBuf, pub line: usize, pub suite: Suite }
pub fn scan_tags(root: &Path) -> anyhow::Result<Vec<Tag>>     // `@req ID[, ID]…` in comments of crates/**/tests/**/*.rs and crates/**/src/**/*.rs
pub struct TraceReport { pub untraced: Vec<String>, pub unknown: Vec<Tag>, pub holdout_share: BTreeMap<String, (u32, u32)> }
pub fn trace(specs: &[Spec], tags: &[Tag]) -> TraceReport     // untraced: rules of approved/implemented specs with no tag
pub fn render_matrix(specs: &[Spec], tags: &[Tag]) -> String  // target/trace.md; holdout tags appear as counts per rule and spec, never as test
                                                              //   names or paths (R151)
// commands: `cargo xtask spec-lint [--spec AREA.NAME]`, `cargo xtask trace [--spec AREA.NAME] [--milestone]`
// gate rows "spec-lint" and "trace" (Task, Pr, Milestone); at milestone, trace fails a spec with acceptance tests whose holdout share is
//   below 10 % and warns above 20 %
```
- `spec-lint` gets its `Schema` by running `cargo run -q -p fl_cli -- schema --json` (built already at every gate level).

- [ ] **Step 1: Write the failing tests:**
```rust
// xtask/src/spec/*.rs (unit tests on inline Markdown)
#[test] fn parses_template_spec()                // a minimal valid spec → 2 rules, 1 param, 2 ACs, 1 guess
#[test] fn duplicate_rule_id_is_error()          // MOVE.SPEED.1 twice (two files) → error naming both files
#[test] fn uncovered_rule_is_error()             // MOVE.SPEED.3 in no AC → "MOVE.SPEED.3 is in no acceptance criterion"
#[test] fn missing_test_is_error()               // AC naming crates/x/tests/y.rs::nope → error unless status draft
#[test] fn param_key_must_match_ledger()         // "rules/Movement.nope" → error; "terrain:*/pass" with ledger classes Guess → ok
#[test] fn param_provenance_must_agree()         // column "OursScaled" over Guess entries → error
#[test] fn unknown_guess_id_is_lint_error()      // ledger Guess "G-MOVE-9" defined nowhere → error naming the key
#[test] fn tags_are_scanned()                    // "// @req MOVE.SPEED.1, MOVE.SPEED.2" → 2 tags, suite from the path
#[test] fn untraced_rule_fails()                 // implemented spec, rule without tag → untraced ["MOVE.SPEED.2"]; draft spec → none
#[test] fn unknown_tag_fails()                   // "@req MOVE.NOPE.1" → unknown
#[test] fn holdout_share_bounds()                // 9 acceptance + 0 holdout → milestone FAIL; 8 + 1 → pass (11 %); 6 + 3 → WARN (33 %);
                                                 //   render_matrix and the printed report contain no holdout test name
// crates/fl_cli/tests/schema.rs
#[test] fn schema_lists_tables_queries_provenance() // "journal_records" contains "core.path_result"; "debug_queries" contains "core.paths";
                                                    //   provenance "rules/Movement.min_speed_frac" class "Guess", id "G-MOVE-1"
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p xtask -p fl_cli -E 'test(/spec|rule|test_is|param_|guess_id|tags_|untraced|unknown_tag|holdout_share|schema_/)'`. Expected: FAIL.
- [ ] **Step 3: Implement.** Then backfill: add `// @req <ids>` above every test that the five specs' acceptance criteria name (comment lines only; no test code changes), and run `cargo xtask spec-lint` and `cargo xtask trace`. Fix the specs where `spec-lint` disagrees, as `.md.new` files (the lead blesses them: `M2P Task 9: spec-lint fixes`).
- [ ] **Step 4: Run** the step 2 command, `cargo xtask spec-lint` (expected `spec-lint: 5 specs, 0 errors`) and `cargo xtask trace` (expected `trace: <n> rules, 0 untraced, 0 unknown tags`, and `target/trace.md` written).
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track tools --track sim-core`. Expected: PASS, with `spec-lint` and `trace` rows PASS.
- [ ] **Step 6: Commit** `feat(xtask): spec-lint and trace with @req tags; fl_cli schema; M1 tests traced to their specs` with trailers `Change-Class: R`, `Refs: ADR-0022` (and `Blessed: …` if specs changed).

### Task 10: Content migrate, patches, post-process hooks, deprecated keys (C6, part 2)

**Track:** contract + content · **Refs:** ADR-0020 · **Lane 2** (after 8)

**Files:**
- Create: `crates/fl_content/src/{patch,migrate,post}.rs`, `crates/fl_content/tests/patch.rs`, fixture mods under `crates/fl_content/tests/fixtures/{patch_a,patch_b,old_format}/`
- Modify: `crates/fl_content/src/{mods,registry,tree,db,lib}.rs`, `crates/fl_sim/src/builder.rs` (pass-through `SimBuilder::register_content_post`)
- Blessed: `xtask/snapshots/api/{fl_content,fl_sim}.txt`

**Interfaces:**
- Consumes: `ContentTree`, `Node`, `path_key`, `Origin`, the load phases of Task 7, the ledger rules of Task 8 (a patching mod's ledger covers the leaves its patches set).
- Produces:
```rust
// crates/fl_content/src/migrate.rs
pub const CONTENT_FORMAT_VERSION: u32 = 1;          // mod.ron `format_version` (default 1 when absent)
pub type Migration = fn(doc_kind: &str, doc: &mut Node) -> Result<(), String>;   // from version n to n + 1
impl DefRegistry {
    pub fn register_migration(&mut self, from_version: u32, migration: Migration);   // panics on a second migration from the same version
    pub fn deprecate_field(&mut self, component: &'static str, old: &'static str, new: &'static str);
        // during migrate, `components.<component>.<old>` becomes `<new>`, warning "deprecated key '<old>' in <file>: use '<new>'"
}
// a mod newer than CONTENT_FORMAT_VERSION is an error; an older one runs every migration from its version up, in order
// crates/fl_content/src/patch.rs — `<mod>/patches/*.patch.ron`: Patch(target: "unit:base.de.pz3" | "terrain:<id>" | "rules", phase: Default | Final, ops: [..])
#[derive(Deserialize)] pub enum PatchPhase { Default, Final }
pub enum PatchOp {
    Set(String, Node),        // path must exist; replaces the value
    Merge(String, Node),      // path must be a struct or map; sets each given field or key
    Remove(String),           // path must exist (a component, field or key)
    Add(String, Node),        // appends to a sequence, or inserts a map key or component that must not exist
    AddTag(String),           // appends to the document's `tags` unless present
    If(PatchCond, Vec<PatchOp>),
}
pub enum PatchCond { Exists(String), ModLoaded(String), Not(Box<PatchCond>) }
// passes: every Default patch, then every Final patch; within a pass, in mod load order, then file name order, then op order; applied to
//   documents before inheritance, so a template patch reaches its children; each op sets the origin of the leaves it writes to the patch's mod
//   and file; an error names the patch file, the op index and the path
// crates/fl_content/src/post.rs
pub struct PostCtx<'a> { .. }   // units_mut(): &mut [ResolvedObject]; kind_mut::<K>(): &mut KindTable<K>; error(path, msg)
pub type PostProcess = fn(&mut PostCtx<'_>);
impl DefRegistry { pub fn register_post_process(&mut self, feature: &'static str, f: PostProcess); }   // run in feature id order, after typed
                                                                                                        //   deserialize, before link
// crates/fl_sim/src/builder.rs
impl SimBuilder { pub fn register_content_post(&mut self, f: PostProcess) -> &mut Self; }   // under the building feature's id
```

- [ ] **Step 1: Write the failing tests** (`crates/fl_content/tests/patch.rs`, with the fixture mods):
```rust
#[test] fn set_changes_one_field()                      // patch_a: Set("components.Hull.hp", 180) on t.tank → resolved Hull.hp 180; origin = patch_a
#[test] fn later_mod_patch_wins_and_owns_provenance()   // patch_a and patch_b (load_after patch_a) both Set hp: 180 then 200 → 200, origin patch_b
#[test] fn final_pass_runs_after_default()              // patch_a Final sets 190, patch_b Default sets 200 → 190
#[test] fn template_patch_flows_to_children()           // Set on the abstract template → both children without their own Hull change
#[test] fn merge_remove_add_addtag()                    // Merge Mobility { road_speed_kmh: 30.0 }; Remove("components.Selectable"); Add("tags", "elite");
                                                        //   AddTag("elite") twice → one "elite"
#[test] fn if_mod_loaded_skips_absent_mod()             // If(ModLoaded("nope"), [Set(..)]) → unchanged; If(Exists("components.Hull"), [..]) → applied
#[test] fn patch_on_missing_path_names_file_and_op()    // Set("components.Nope.x", 1) → error containing "patch_a/patches/a.patch.ron", "op 0", "components.Nope.x"
#[test] fn patch_on_missing_target_is_error()           // target "unit:base.nope" → error naming the target
#[test] fn enum_patch_values()                          // Set("components.Mobility.class", Wheeled) and the string "Wheeled" both give MoveClass::Wheeled
#[test] fn old_format_is_migrated()                     // old_format (format_version 0) with a registered 0→1 migration renaming a field → loads
#[test] fn newer_format_is_error()                      // format_version 2 → error "format_version 2 is newer than 1"
#[test] fn deprecated_field_warns()                     // deprecate_field("Hull", "health", "hp"); a unit writing "health" loads, with the warning in warnings()
#[test] fn post_process_runs_in_feature_order()         // two features registering post hooks "b" then "a" → hooks run a, then b; a hook's change is hashed
#[test] fn base_content_unchanged()                     // the base mod's rules hash equals the one before this task
#[test] fn override_discards_earlier_patches()          // patch_a patches t.tank, patch_b overrides it whole, patch_b's later Final patch applies:
                                                        //   patch_a's change is gone, leaves have patch_b's origin, the Final patch's value stands
#[test] fn patched_leaf_keeps_base_ledger_valid()       // patch_b sets a leaf the fixture base mod set: base's ledger entry is not stale, patch_b's
                                                        //   ledger covers the final value (staleness judged per mod, R151)
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_content --test patch`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_content -p fl_catalog -p fl_sim -p fl_test_utils -p fl_cli`. Expected: PASS; goldens verify unchanged.
- [ ] **Step 5: Regenerate** the `fl_content` and `fl_sim` API snapshots; the lead blesses: `M2P Task 10: content migrate, patches, post-process hooks`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track content`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_content): format migrations, patch ops in two passes, post-process hooks, deprecated keys` with trailers `Change-Class: N`, `Refs: ADR-0020`, `Blessed: …`.

### Task 11: Acceptance pipeline: test author, holdout, statistical policy (C3, C4)

**Track:** contract + sim-core · **Refs:** ADR-0022 · **Lane 1** (after 9)

**Files:**
- Modify: `.claude/agents/{test-author,sim-engineer,client-engineer}.md`, `.claude/settings.json`, `xtask/src/{hooks,gate}.rs`, `xtask/src/test_guard/patterns.rs`, `crates/fl_test_utils/{Cargo.toml,src/lib.rs}`, `OWNERS.toml`
- Create: `crates/fl_test_utils/src/stats.rs`, `crates/fl_test_utils/tests/stats.rs`; by the test-author: `crates/fl_test_utils/tests/acceptance/{main,move_speed,order_queue}.rs`, `crates/fl_test_utils/tests/holdout/{main,move_speed,order_queue}.rs`

**Interfaces:**
- Consumes: `trace` (Task 9), the specs `MOVE.SPEED` and `ORDER.QUEUE` (Task 4), `ScenarioBuilder`, `fl_math::rolls`-backed `Rolls::chance`.
- Produces:
```rust
// crates/fl_test_utils/src/stats.rs (f64 is allowed here: fl_test_utils uses the root clippy profile)
pub const ALPHA: f64 = 1e-4;
pub const FORMULA_DRAWS: u64 = 100_000;
pub const SCENARIO_SEEDS_MIN: u32 = 200;
pub const SCENARIO_SEEDS_MAX: u32 = 500;
pub fn z_two_sided(alpha: f64) -> f64                         // the standard normal quantile at 1 − α/2 (Wichura AS241)
pub fn wilson_interval(successes: u64, trials: u64, z: f64) -> (f64, f64)
pub fn assert_rate_within_wilson(successes: u64, trials: u64, p0: f64, alpha: f64)
    // panics unless p0 lies in the Wilson interval at z_two_sided(alpha): "rate p̂=<p> (<k>/<n>), oracle p0=<p0>, Wilson 1−α interval [<lo>, <hi>], α=<alpha>"
pub struct SeedSet { pub base: u64, pub count: u32 }
impl SeedSet { pub fn seeds(&self) -> impl Iterator<Item = u64>; }   // seed i = base.wrapping_mul(1_000_003).wrapping_add(i)
pub fn seed_base(default: u64) -> u64                             // FL_SEED_BASE when set (nightly), else default
pub fn reroll_base(base: u64) -> u64                              // the one allowed reroll: base + 100_000, disjoint from every nightly base (R151)
// crates/fl_test_utils/Cargo.toml
[features] holdout = []
[[test]] name = "acceptance"  path = "tests/acceptance/main.rs"
[[test]] name = "holdout"     path = "tests/holdout/main.rs"  required-features = ["holdout"]
// xtask: test-headless at Nightly and Milestone adds `--features fl_test_utils/holdout`; Nightly sets FL_SEED_BASE = days since 2026-01-01 (UTC);
//   hooks: `cargo xtask hook guard-read --deny "**/tests/holdout/**"` denies Read/Grep/Glob of holdout paths to implementer roles, and the Bash
//   guard denies implementer commands naming a holdout path (`tests/holdout`); secrecy is best-effort (ADR-0022, R151)
// test-guard: a changed `seed_base(` literal is NewSeedOverride; a changed ALPHA, FORMULA_DRAWS or SCENARIO_SEEDS_* value, or a lowered
//   SeedSet { count }, is ToleranceChanged
```
- **Agents.** `test-author.md` drops "do not dispatch it in M0 or M1" and gains the rules: tests from the spec alone (cite the rule ids with `@req`); black-box API only (`ScenarioBuilder`, `fl_protocol` payloads, `Scenario::{pose, world}` reading `Pose`, `Health`, `SimIndex`, and `fl_view` snapshots), so no sim refactor breaks them; holdout at least 10 % per spec; oracles from the spec or an f64 reference; statistical tests through `fl_test_utils::stats`. `sim-engineer.md` and `client-engineer.md` gain the `guard-read` hook on `Read|Grep|Glob`; `settings.json` gains it with role `auto` (R37). Under R82 the controller dispatches the test-author as a general-purpose agent with the test-author brief, so the brief repeats the rules; the gate rows still enforce edits.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_test_utils/tests/stats.rs
#[test] fn z_matches_known_quantiles()          // |z_two_sided(0.05) − 1.959963984540054| < 1e-9; |z_two_sided(1e-4) − 3.890591886413| < 1e-9
#[test] fn wilson_matches_reference()           // wilson_interval(50, 100, 1.959963984540054) ≈ (0.40383, 0.59617) within 1e-5
#[test] fn rolls_chance_matches_probability()   // formula tier: FORMULA_DRAWS draws of chance(3/10) over SeedSet { base: seed_base(7), count: 1 }
                                                //   → assert_rate_within_wilson(k, n, 0.3, ALPHA)
#[test] #[should_panic(expected = "oracle p0=0.31")] fn wilson_rejects_a_wrong_oracle()  // the same draws against p0 0.31
#[test] fn seed_set_is_fixed()                  // SeedSet { base: 7, count: 3 }.seeds() == [7_000_021, 7_000_022, 7_000_023]
// xtask unit tests
#[test] fn holdout_feature_at_nightly_only()    // the test-headless args at Nightly and Milestone contain "--features fl_test_utils/holdout"; Task and Pr do not
#[test] fn guard_read_denies_holdout()          // guard-read for an implementer on crates/fl_test_utils/tests/holdout/a.rs → deny; for lead → allow
#[test] fn seed_base_change_is_flagged()        // seed_base(7) → seed_base(8) → NewSeedOverride
#[test] fn bash_guard_denies_holdout_paths()    // implementer Bash "rg foo crates/fl_test_utils/tests/holdout" → deny; "cargo nextest run" → allow
#[test] fn reroll_base_is_disjoint()            // reroll_base(300) == 100_300; its SeedSet shares no seed with bases 0..=1000 at count 500
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_test_utils -p xtask -E 'test(/z_matches|wilson|rolls_chance|seed_set|holdout_feature|guard_read|seed_base|bash_guard|reroll_base/)'`. Expected: FAIL.
- [ ] **Step 3: Implement** the stats module, the feature, the gate and hook changes, and the agent files.
- [ ] **Step 4: Run** the step 2 command. Expected: PASS.
- [ ] **Step 5: Pilot (test-author, then lead).** The controller dispatches the test-author with the specs `MOVE-speed.md` and `ORDER-queue.md` only. It writes acceptance tests for every acceptance criterion and at least one holdout test per spec (10–20 % of that spec's acceptance and holdout tests), each with `@req` tags. They test implemented M1 behaviour, so they must pass; a failing one is a finding (the spec, the test or the code is wrong) for the lead. Run `cargo nextest run --profile agent -p fl_test_utils --test acceptance` and `cargo nextest run --profile agent -p fl_test_utils --features fl_test_utils/holdout --test holdout`. Expected: PASS. The lead reviews them and commits them separately with `Blessed: M2P Task 11: C3 pilot acceptance and holdout tests (test-author)`.
- [ ] **Step 6: Trace.** `cargo xtask trace --milestone`. Expected: `MOVE.SPEED` and `ORDER.QUEUE` holdout shares within [10 %, 20 %], 0 untraced.
- [ ] **Step 7: Gate** `cargo xtask gate task --track contract --track sim-core` (before the lead's pilot commit, since its protected files would fail the task-level `protected` row). Expected: PASS.
- [ ] **Step 8: Commit** `feat(test): statistical helpers, holdout feature and read guard, test-author active` with trailers `Change-Class: R`, `Refs: ADR-0022`.

### Task 12: Content vocabulary: conditions, tags, class expressions, verbs (C5, content half)

**Track:** contract + content · **Refs:** ADR-0017 · **Lane 2** (after 10)

**Files:**
- Create: `crates/fl_content/src/{expr,tags,verbs}.rs`, `crates/fl_content/tests/vocabulary.rs`
- Modify: `crates/fl_content/src/{registry,db,link,lib}.rs`, `crates/fl_protocol/src/tables.rs` (constants `CONDITIONS`, `TAGS`, `STATS`, `EFFECTS`, `TRIGGER_CONDITIONS`, `TRIGGER_ACTIONS`)
- Blessed: `xtask/snapshots/api/{fl_content,fl_protocol}.txt`

**Interfaces:**
- Consumes: `DefRegistry`, `LinkCtx`, `ObjectDef.tags`, `TreeDeserializer`.
- Produces:
```rust
// crates/fl_content/src/expr.rs
#[derive(Clone, Debug, PartialEq, Eq, Serialize)] pub struct ConditionExpr { .. }      // postfix bytecode over condition dense ids
impl ConditionExpr {
    pub fn compile(text: &str, conditions: &[String]) -> Result<ConditionExpr, String>  // `&&`, `||`, `!`, parentheses, names; precedence ! > && > ||;
                                                                                          //   "unknown condition 'x'", "column <n>: expected …"
    pub fn eval(&self, has: impl Fn(u16) -> bool) -> bool;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub struct Requires(pub String);  // a def field; compiled at link
impl LinkCtx<'_> { pub fn requires(&mut self, path: &str, r: &Requires); }   // compiles and records it, or reports the error at `path`
impl ContentDb { pub fn condition_expr(&self, text: &str) -> Option<&ConditionExpr>; }
// crates/fl_content/src/tags.rs
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)] pub struct TagSet(pub u128);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)] pub struct ClassExpr { pub all: u128, pub none: u128, pub any: u128 }
impl ClassExpr {
    pub fn compile(text: &str, tags: &[String]) -> Result<ClassExpr, String>   // terms joined by `&`: `tag`, `!tag`, at most one `(a | b | …)`
    pub fn matches(&self, set: TagSet) -> bool;                               // all ⊆ set, none ∩ set = ∅, any = 0 or any ∩ set ≠ ∅
}
impl ContentDb { pub fn tags(&self) -> &[String]; pub fn unit_tags(&self, id: UnitTypeId) -> TagSet; }
    // every object tag in content, sorted; more than 128 → content error "129 tags exceed the limit of 128"
// crates/fl_content/src/verbs.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)] pub enum VerbFamily { Effect, TriggerCondition, TriggerAction }
pub trait Verb: erased_serde::Serialize + Debug + Send + Sync + 'static { fn validate(&self, _ctx: &mut LinkCtx<'_>) {} fn as_any(&self) -> &dyn Any; }
pub struct VerbList<F: FamilyMarker> { pub verbs: Vec<(u16, Arc<dyn Verb>)> }   // `[Name(args), …]`; ids index the family's table
pub struct EffectFamily; pub struct TriggerConditionFamily; pub struct TriggerActionFamily;   // FamilyMarker impls
impl DefRegistry {
    pub fn register_condition(&mut self, name: &'static str);
    pub fn register_verb<V: Verb + DeserializeOwned>(&mut self, family: VerbFamily, name: &'static str);
}
// VerbList deserializes through the registry the loader installs for the typed phase (a scoped thread-local in fl_content); outside a load it
//   is an error "verb lists deserialize only inside a content load". Tables "conditions", "tags", "effects", "trigger_conditions",
//   "trigger_actions" join the string tables (sorted names). Canonical hashing writes a verb as its name then its fields.
```

- [ ] **Step 1: Write the failing tests** (`crates/fl_content/tests/vocabulary.rs`, with a test feature registering conditions `test.deployed`, `test.moving` and an effect verb `test.heal(amount: u32)`):
```rust
#[test] fn requires_compiles_and_evaluates()        // "test.deployed && !test.moving": {deployed} → true; {deployed, moving} → false; {} → false
#[test] fn precedence_and_parentheses()             // "a || b && !c" == "a || (b && (!c))" on all 8 assignments
#[test] fn unknown_condition_in_requires_is_error() // a def with requires "test.nope" → link error at its field path, "unknown condition 'test.nope'"
#[test] fn malformed_expression_names_column()      // "test.deployed &&" → "column 17"
#[test] fn class_expressions_match()                // tags [armored, tank, truck, vehicle]: "vehicle & !truck & (tank | gun)" matches {vehicle, tank}, not {vehicle, truck}
#[test] fn unknown_tag_is_error()                   // "vehicle & hovercraft" → "unknown tag 'hovercraft'"
#[test] fn second_any_group_is_error()              // "(a | b) & (c | d)" → error "one any-group"
#[test] fn tag_overflow_is_error()                  // a fixture with 129 distinct tags → content error naming 129 and 128
#[test] fn base_unit_tags()                         // base pz3's TagSet has exactly the bits of "vehicle", "tank", "tracked"
#[test] fn verb_list_deserializes_registered_verbs() // "[Heal(amount: 5)]" in a test def → one verb, family Effect, name "test.heal"
#[test] fn unknown_verb_is_error()                  // "[Burn(..)]" → error listing the registered effects
#[test] fn vocabulary_tables_are_sorted()           // string_tables "conditions" == ["test.deployed", "test.moving"]
#[test] fn base_rules_hash_unchanged()              // the base mod's rules hash equals Task 10's
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_content --test vocabulary`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_content -p fl_protocol -p fl_catalog -p fl_sim -p fl_test_utils`. Expected: PASS; goldens verify unchanged (string tables are remapped, not compared).
- [ ] **Step 5: Regenerate** the `fl_content` and `fl_protocol` API snapshots; the lead blesses: `M2P Task 12: content vocabulary`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track content`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_content): conditions and requires expressions, tag sets and class expressions, verb registries` with trailers `Change-Class: N`, `Refs: ADR-0017`, `Blessed: …`.

### Task 13: Order queue, activity stack and relationships (C7, M7, M12, M13, M14)

**Track:** contract + sim-core + client · **Refs:** ADR-0018, ADR-0019, ADR-0006, ADR-0007 · **Lane 1** (after 2, 5 and 11)

**Files:**
- Create: `crates/fl_sim/src/{behaviour,relation}.rs`, `crates/fl_sim/tests/{behaviour,relation}.rs`
- Modify:
  - `crates/fl_sim/src/{units,orders,commands,debug_cmds,path,movement,classify,builder,feature,spawn,build,lib}.rs`, `crates/fl_sim/src/save/{format,load}.rs`
  - `crates/fl_protocol/src/{command,tables,lib}.rs` (`MAX_COMMAND_UNITS`, `ORDERS`, `ACTIVITIES`), `crates/fl_client/src/orders.rs` (imports `fl_protocol::MAX_COMMAND_UNITS` instead of its copy)
  - ports in `crates/fl_sim/tests/{orders,pathing,commands,movement,flow,save,reload,runner,terrain}.rs` and `crates/fl_test_utils/tests/{golden,m0_save,m1_save,movement_determinism,schedule}.rs`
  - `OWNERS.toml` (`behaviour.rs`, `relation.rs` join the `contract` track), `docs/design/changelog-mechanics.md` (MCR-001), `.claude/rules/sim.md` (ADR-0019), `.claude/agents/reviewer.md` (observer item)
- Blessed: system order, `formats__save_codecs.snap`, `fl_protocol`/`fl_sim` format and API snapshots, both golden replays (MCR-001), perf counter snapshots and `docs/perf/budgets.ron` if the counters move

**Interfaces:**
- Consumes: `units::v2` and the pose trace (Task 2), `rebless` (Task 5), `PathFollow`, `enqueue_path_in`, `enqueue_group_in`, `formation_targets`, `space_slots`, `journal_order_changed`, `Codec`, `LoadCtx`.
- Produces:
```rust
// crates/fl_protocol/src/command.rs
pub const MAX_COMMAND_UNITS: usize = 256;            // moved from fl_sim (re-exported there); the client imports it
// crates/fl_sim/src/behaviour.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)] pub struct OrderKindId(pub u16);    // table "orders"
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)] pub struct ActivityKindId(pub u16); // table "activities"
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderTarget { None, Point(FxVec2), Unit(SimId), Facing(Angle), UnitOffset { unit: SimId, offset: FxVec2 } }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub struct QueuedOrder { pub kind: OrderKindId, pub target: OrderTarget, pub param: Vec<u8> }
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)] pub struct OrderQueue { pub orders: Vec<QueuedOrder> }
    // Hashed "core.move_order" v3 (custom codec: kinds saved as dense ids, remapped by name; at most MAX_WAYPOINTS);
    // Codec::migrate_with::<units::v2::MoveOrder>(2, ..): Move → "core.move" Point, Rotate → "core.rotate" Facing, Follow → "core.follow" UnitOffset
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum ActivityState { Active, Canceling }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub struct Activity { pub kind: ActivityKindId, pub target: OrderTarget, pub param: Vec<u8>, pub state: ActivityState }
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)] pub struct ActivityStack { pub frames: Vec<Activity> }
    // Hashed "core.activity_stack" v1; top = last; at most MAX_ACTIVITY_DEPTH = 8; fill_missing (core version 3): each unit with an OrderQueue
    //   gets its current order's root activity (OrderBehaviour::begin), or an empty stack
pub enum Outcome { Completed, Failed }
pub enum Step { Continue, Done(Outcome), Push(Activity) }
pub trait OrderBehaviour: Send + Sync + 'static { fn begin(&self, world: &World, unit: SimId, entity: Entity, order: &QueuedOrder) -> Activity; }
pub trait ActivityBehaviour: Send + Sync + 'static {
    fn tick(&self, world: &mut World, unit: SimId, entity: Entity, frame: &Activity) -> Step;
    fn cancel(&self, _world: &mut World, _unit: SimId, _entity: Entity, _frame: &Activity) {}
}
impl SimBuilder {
    pub fn register_order(&mut self, name: &'static str, behaviour: impl OrderBehaviour) -> &mut Self;       // panics on a duplicate (both features)
    pub fn register_activity(&mut self, name: &'static str, behaviour: impl ActivityBehaviour) -> &mut Self;
    pub fn register_relation<R: Relation>(&mut self) -> &mut Self;
}
pub fn order_kind(world: &World, name: &str) -> Option<OrderKindId>;
pub const MAX_ORDERS_BEGUN_PER_TICK: u32 = 32;       // root orders run_activities begins per unit per tick (= MAX_WAYPOINTS, so M1 timing never changes);
                                                    //   more waits for the next tick; pushes and pops inside a chain are bounded by the depth cap only;
                                                    //   cancel steps are exempt (R153)
pub const MAX_REPUSH_PER_TICK: u32 = 4;             // loop guard: one activity kind pushed onto the same parent frame more often in one tick
                                                    //   is a debug assertion (unreachable through the behaviour traits; guards run_activities)
pub const MAX_ORDER_PARAM_BYTES: usize = 64;         // QueuedOrder.param and Activity.param; checked by give_order and by the codecs
pub fn give_order(world: &mut World, entity: Entity, order: QueuedOrder, queue: bool) -> Result<bool, RejectReason>;
    // queue: append (QueueFull at MAX_WAYPOINTS); otherwise mark each frame Canceling and give it one cancel step, top-down, remove it, replace
    //   the orders and begin the new one in the same tick; a param over MAX_ORDER_PARAM_BYTES → Feature("core.param_too_large");
    //   returns whether the order is now current
pub fn stop_unit(world: &mut World, entity: Entity);   // cancel every frame, clear the orders, PathFollow::reset
pub(crate) fn run_activities(world: &mut World, query: &mut ActivitiesQuery);
    // replaces advance_orders in ThinkSet::Orders' core slot (first until Task 17, then right after recompute_stats); in SimIndex order, per
    //   unit, the stepping rules of ADR-0018 (R151): begin the current order if the stack is empty; tick the top frame; Push → the child ticks
    //   in the same tick; a child's Done → pop it (journal "core.activity_changed"), its parent ticks on the next tick; the root's Done → pop
    //   it and the order (journal "core.order_changed" "completed"/"failed") and begin the next order in the same tick, as advance_orders did;
    //   at most MAX_ORDERS_BEGUN_PER_TICK root orders begun per unit, then the unit waits; the MAX_REPUSH_PER_TICK loop guard (debug_assert!);
    //   units with no order and a Done/Failed path go Idle as before; span "run_activities"; counter "orders.activity_ticks"
// core kinds (orders.rs): orders "core.move", "core.rotate", "core.follow"; activities "core.drive" (M1's move logic), "core.turn"
//   (facing from the target), "core.follow" (M1's follow logic, R106/R107); their behaviour is M1's exactly
// path.rs
impl PathFollow { pub fn reset(&mut self); }        // state Idle, corners cleared, next 0 — the one copy (M12)
pub(crate) fn enqueue_group_in(..)                  // inserts a missing PathQueue like enqueue_path_in (M13)
// "core.paths": a unit without Mobility answers {"state": "immobile"} (M7); "orders": [{"kind": "core.move", "target": …}, …]
// debug query "core.activities" {"units": [ids]} → {"<id>": {"orders": [..], "frames": [{"kind", "state", "target"}]}}
// journal "core.activity_changed" { kind: String, event: "push"|"pop"|"cancel", depth: usize, outcome: "completed"|"failed"|null };
//   "core.order_changed" keeps its fields; `current` becomes the order kind's name ("core.move")
// commands.rs, debug_cmds.rs: no map set up → RejectReason::Feature("core.no_map") (M14)
// crates/fl_sim/src/relation.rs
pub trait Relation: Send + Sync + 'static { const NAME: &'static str; const ONE_TO_ONE: bool; }
#[derive(Component)] pub struct RelatedTo<R: Relation> { .. }   // Hashed R::NAME v1 (codec: the target SimId); pub fn target(&self) -> SimId
#[derive(Component)] pub struct Related<R: Relation> { .. }     // Derived "<R::NAME>.sources", maintained by RelatedTo's hooks; pub fn sources(&self) -> &[SimId]
#[non_exhaustive] #[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationError { NoSource(SimId), NoTarget(SimId), SelfRelation(SimId), TargetTaken { target: SimId, by: SimId } }
pub fn relate<R: Relation>(world: &mut World, source: SimId, target: SimId) -> Result<(), RelationError>;   // replaces the source's old target
pub fn unrelate<R: Relation>(world: &mut World, source: SimId) -> bool;
// classify.rs / save
impl<T> Codec<T> { pub fn migrate_with<Old: DeserializeOwned>(self, version: u32, f: fn(Old, &LoadCtx<'_>) -> Result<T, String>) -> Self; }
impl LoadCtx<'_> { pub fn current_id(&self, table: &str, name: &str) -> Option<u32>; }
// build.rs: SIM_VERSION = 3; feature.rs: CoreFeature::version() = 3
```
- **MCR-001** (written to the changelog first): "Order queue and activity stack (C7)". REQs: `ORDER.QUEUE.*` unchanged. Expected effect: unit poses identical every tick (the Task 2 pose trace); hash layout changes (new Hashed state, codec v3); journal records `core.order_changed` gain full kind names, `core.activity_changed` is new. SIM_VERSION 2 → 3. Change-Class: F.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_sim/tests/behaviour.rs
#[test] fn orders_and_activities_are_registered()   // tables "orders" == ["core.follow", "core.move", "core.rotate"]; "activities" has "core.drive", "core.follow", "core.turn"
#[test] fn move_runs_as_a_drive_activity()         // a move: stack [core.drive] while driving; empty and no orders on arrival; journal push then pop "completed"
#[test] fn queued_orders_begin_in_turn()           // move A, Shift rotate, Shift move B: the stack's root kind is drive, turn, drive in that order
#[test] fn replacing_cancels_top_down()            // a test activity that pushes a child; a new move → cancel called child first, then parent
#[test] fn stop_clears_queue_stack_and_path()      // stop → orders empty, frames empty, PathFollow Idle with no corners
#[test] fn feature_order_kind_runs()               // a test feature registers "test.wait" (activity done after 3 ticks) → completed at tick +3
#[test] fn activity_depth_is_capped()              // a test activity that pushes another of its kind on every tick → 8 frames in the first
                                                   //   tick, the 9th push fails the order ("failed"); the 8 cancel steps trip no bound or
                                                   //   assert (cancel steps are exempt; each push has a new parent), no panic
#[test] fn pushed_child_ticks_in_the_same_tick()   // a parent pushing a child that counts its ticks → the child's first tick is the push tick
#[test] fn parent_resumes_on_the_next_tick()       // a child done at once → the parent's next tick() call is one tick later
#[test] fn full_queue_of_instant_orders_finishes_in_one_tick() // 32 queued rotates already facing their target → all complete in one tick
                                                   //   (journal: 32 "completed" in that tick), as M1 did
// crates/fl_sim/src/behaviour.rs (#[cfg(test)])
#[test] fn repush_guard_counts_per_parent()        // the guard's counter: 5 pushes of one kind onto one parent frame in a tick → over
                                                   //   MAX_REPUSH_PER_TICK; 5 pushes onto 5 different parents → not
// crates/fl_sim/tests/behaviour.rs (continued)
#[test] fn cancel_gets_one_step_then_new_order_begins() // a two-frame stack, then a new move → cancel called once per frame (child first),
                                                   //   the drive frame of the new move is on the stack in the same tick
#[test] fn oversized_order_param_is_rejected()     // a 65-byte param → "core.param_too_large", nothing changed
#[test] fn activity_stack_round_trips()            // save/load with a pushed child frame → equal stacks; assert_branch(n = 5, m = 40)
#[test] fn no_map_is_its_own_code()                // core.move with no map set up → rejection code "core.no_map"
#[test] fn paths_query_answers_immobile_units()    // "core.paths" {"units": [a unit without Mobility]} → {"<id>": {"state": "immobile"}}
#[test] fn activities_query_lists_frames()         // "core.activities" for a driving unit → frames [{"kind": "core.drive", "state": "active", ..}]
#[test] fn max_command_units_lives_in_protocol()   // fl_sim::MAX_COMMAND_UNITS == fl_protocol::MAX_COMMAND_UNITS == 256
// crates/fl_sim/tests/relation.rs (a test feature: "test.tow" one-to-one, "test.squad" one-to-many)
#[test] fn relate_maintains_sources()              // relate #2 → #1 and #3 → #1 (squad) → Related(#1).sources == [#2, #3]
#[test] fn one_to_one_rejects_second_source()      // tow #2 → #1, then #3 → #1 → Err(TargetTaken { target: #1, by: #2 })
#[test] fn self_relation_is_error()                // relate #1 → #1 → Err(SelfRelation(#1))
#[test] fn despawned_target_strips_sources()       // despawn #1 → #2 and #3 have no RelatedTo<Squad>
#[test] fn relations_survive_save_and_load()       // a source with a lower SimId than its target (#2 → #9) → after load Related(#9) == [#2]; round trip byte-equal
#[test] fn relations_branch()                      // assert_branch with relate commands at ticks 3 and 7
// crates/fl_test_utils/tests/m1_save.rs (extended)
#[test] fn m1_save_migrates_to_order_queue()       // every unit's v3 queue: the kinds in order equal the fixture's v2 orders decoded with units::v2
                                                   //   (Move → core.move, …); every unit with a current order has its root activity
// kept passing unchanged: m1_save_continues_with_m1_pose_trace, golden.rs testbed_paths_pose_trace_matches, the Task 11 acceptance tests
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -E 'test(/orders_and|drive_activity|queued_orders|cancels|stop_clears|feature_order|depth|stack_round|no_map|immobile|activities_query|max_command|relat|despawned_target|m1_save|pushed_child|parent_resumes|full_queue|repush_guard|cancel_gets|oversized/)'`. Expected: FAIL.
- [ ] **Step 3: Write MCR-001 to the changelog, then implement.** Keep M1's behaviour exactly: the drive, turn and follow activities hold M1's `advance_unit` logic, `give_order` begins a non-queued order in `Phase::Input` as M1 did, and the handlers enqueue paths as before. Port the tests that used `MoveOrder`/`Order`; list every test-guard finding.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_protocol -p fl_client -p fl_cli`. Expected: PASS apart from the protected artifacts. **The pose trace tests must pass unchanged**: they prove the refactor leaves every unit's pose identical. Then run `cargo nextest run --profile agent -p fl_test_utils --test acceptance`. Expected: PASS (the protected pilot tests).
- [ ] **Step 5: Re-bless through the tool (R146).** `cargo run -q -p fl_cli -- rebless --reason MCR-001 --json`. Expected: both goldens change (hash layout), semantic checks pass. Paste `target/rebless-report.md`; its journal diff shows only `core.activity_changed` added. Regenerate system order, formats, API, perf counters (`FL_RECORD_BUDGETS=1 … -E 'test(/scale|counters/)'`; a moved budget is justified in the report). The lead blesses all with `MCR-001: order queue and activity stack (C7)` and lists MCR-001 for ratification.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track sim-core --track client --track perf`. Expected: PASS (`change-class` and the golden rules hold).
- [ ] **Step 7: Commit** `feat(fl_sim)!: order queue and activity stack replace the closed Order enum; SimId relationships; core.no_map` with trailers `Change-Class: F`, `MCR: MCR-001`, `Refs: ADR-0018, ADR-0019, ADR-0006, ADR-0007`, `Blessed: …`.
- [ ] **Step 8: Checkpoint (controller).** Merge lane 2's finished tasks, `cargo xtask gate pr`. Expected: PASS. Fast-forward `main`.

### Task 14: Mutation testing, review tiers, mini-gate (C9, C10)

**Track:** contract · **Refs:** ADR-0022 · **Lane 2** (after 12, and after 11 merged, for `gate.rs`)

**Files:**
- Create: `xtask/src/{mutants,review_tier,mini_gate}.rs`, `.cargo/mutants.toml`, `xtask/mutants-triage.toml`
- Modify: `xtask/src/{gate,main}.rs`, `.claude/agents/reviewer.md`, `OWNERS.toml` (`xtask/mutants-triage.toml` protected)

**Interfaces:**
- Consumes: `spec::{parse_spec, scan_tags, trace}` (Task 9), `owners::Owners`, `changed::touched_packages`, `test_guard` patterns for test names.
- Produces:
```rust
// cargo xtask mutants [--base <rev>] [--full] [--shard k/n] [--cap <secs>] [--json]
//   runs `cargo mutants` in copy mode (never --in-place) with CARGO_BUILD_BUILD_DIR=C:/t/fl/mutants, `--test-tool nextest`, `-j 1`, over the
//   packages fl_math, fl_content, fl_sim (and fl_feat_* sims when they exist) that the diff touches; --base: `--in-diff` on `git diff <base>`;
//   kills the run at the cap and reports what finished; reads mutants.out/outcomes.json
pub struct MutantsSummary { pub caught: u32, pub missed: Vec<String>, pub timeout: u32, pub unviable: u32, pub capped: bool }
pub fn summarize(outcomes_json: &str) -> anyhow::Result<MutantsSummary>
pub fn untriaged(missed: &[String], triage_toml: &str) -> Vec<String>   // xtask/mutants-triage.toml: [[mutant]] name, reason (non-empty)
// gate rows: "mutants" (Pr: in-diff vs base, cap 1200 s, WARN on missed or capped; Nightly: --full in 4 sequential shards, cap 3600 s each, WARN),
//   "mutants-triage" (Milestone: in-diff since the previous milestone tag, cap 3600 s; FAIL on any missed mutant not in the triage file)
// .cargo/mutants.toml: exclude_re for Debug/Display impls and tracing calls; examine only crates/{fl_math,fl_content,fl_sim}/src/**;
//   timeout multiplier 3, minimum test timeout 20 s (key names checked with `cargo mutants --emit-schema config`)
// cargo xtask review-tier [--base <rev>]
pub enum Tier { P, A, B, C }
pub fn tier_of(files: &[PathBuf], owners: &Owners) -> (Tier, Vec<PathBuf>)
    // P: any protected path; A: contract track, crates/fl_sim/src/**, crates/fl_math/**, crates/fl_content/src/**, features/*/sim/**;
    // B: crates/fl_client/**, crates/fl_game/**, crates/fl_cli/**, crates/fl_observe/**, crates/fl_test_utils/** (non-protected);
    // C: docs/**, assets/**, README.md; the highest tier wins; prints "review tier: A (crates/fl_sim/src/path.rs, …)"
// cargo xtask mini-gate <AREA.NAME> [--holdout]: spec-lint and trace of that spec, then nextest of every non-holdout test tagged with its rules;
//   with --holdout (refused unless FL_ROLE=lead, the role variable the hooks and `xtask bless` read; R153) also the tagged holdout tests (holdout
//   feature on), reported only as pass and fail counts, never names or output
//   (R151); prints "mini-gate <AREA.NAME>: PASS (<n> tests)" (with --holdout: "PASS (<n> tests, holdout <p> passed, <f> failed)")
```
- `reviewer.md` gains the tier table (what each tier reviews, from ADR-0022), the instruction to read `cargo xtask review-tier` and the controller's mutants output, and the observer rule of ADR-0019.

- [ ] **Step 1: Write the failing tests** (xtask unit tests):
```rust
#[test] fn summarize_counts_outcomes()       // a fixture outcomes.json with 3 caught, 1 missed, 1 timeout → MutantsSummary { 3, ["src/a.rs:10: replace + with -"], 1, 0, false }
#[test] fn untriaged_lists_new_missed()      // missed [m1, m2], triage has m1 with reason → [m2]; an entry without reason → error
#[test] fn mutants_never_in_place()          // the built cargo-mutants argument list contains no "--in-place" and contains "--test-tool" "nextest"
#[test] fn mutants_rows_by_level()           // steps(): "mutants" levels [Pr, Nightly]; "mutants-triage" [Milestone]
#[test] fn tier_by_paths()                   // [docs/x.md] → C; [crates/fl_client/src/a.rs] → B; [crates/fl_sim/src/a.rs] → A;
                                             //   [crates/fl_sim/tests/snapshots/a.snap] → P; [docs/x.md, crates/fl_sim/src/a.rs] → A
#[test] fn mini_gate_selects_tagged_tests()  // tags for MOVE.SPEED.1 in tests a and b → nextest filter "test(=a) | test(=b)"; holdout tags join the
                                             //   filter, with --features fl_test_utils/holdout, only with --holdout
#[test] fn mini_gate_hides_holdout_names()   // --holdout with a failing holdout test → the output has the counts and no holdout test name
#[test] fn mini_gate_holdout_needs_lead_role() // --holdout with FL_ROLE unset or "implementer" → error before any build; FL_ROLE=lead → runs
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p xtask -E 'test(/summarize|untriaged|mutants_|tier_by|mini_gate/)'`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the step 2 command. Expected: PASS. Then one real run: `cargo xtask mutants --base <the Task 13 base> --cap 900` (a builder; the lane-1 builder must be idle or this waits). Paste the summary; triage its missed mutants into the report (Task 21 decides each: a new test, or a triage entry the lead writes).
- [ ] **Step 5: Run** `cargo xtask review-tier --base <Task 13 base>` (expected `review tier: A …`) and `cargo xtask mini-gate MOVE.SPEED` (expected `mini-gate MOVE.SPEED: PASS (<n> tests)`).
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(xtask): cargo-mutants wrapper and gate rows, review tiers, per-mechanic mini-gate` with trailer `Refs: ADR-0022`.

---

## Phase III — movement carry-overs, sim vocabulary, runner, render, perf, buttons

### Task 15: Movement carry-overs: reload invalidation, flow fall-off, path lanes (M1, M4, M5)

**Track:** contract + sim-core + perf · **Refs:** ADR-0015, ADR-0006, ADR-0021 · **Lane 1** (after 13)

**Files:**
- Modify: `crates/fl_sim/src/{path,movement,reload,orders,commands,build,feature}.rs`, `crates/fl_sim/tests/{pathing,flow,reload}.rs`, `docs/design/changelog-mechanics.md` (MCR-002), `docs/design/mechanics/{PATH-requests,FLOW-groups}.md.new` (rules for lanes and the stall rule)
- Blessed: goldens (MCR-002), `formats__save_codecs.snap`, API, perf counter snapshots and budgets, the two specs

**Interfaces:**
- Consumes: `PathQueue`, `PathFollow`, `enqueue_path_in`, `process_path_queue`, `apply_content_values`, `invalidate_paths`, `rebuild_derived`.
- Produces:
```rust
// path.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum Lane { Command, Background }
pub struct PathQueue { pub commands: VecDeque<PathRequest>, pub background: VecDeque<PathRequest>, pub next_id: u32 }
    // Hashed "core.path_queue" v2; migrate_from v1: `pending` → `commands` (FIFO kept), background empty
    // a request enqueued in Phase::Input (command handlers) is Command; every other is Background
    // process_path_queue: pops Command requests while spent < budget (at least one per tick), then Background while spent < budget, and if no
    //   Background request was processed this tick and its lane is not empty, exactly one more (never starved)
pub struct PathFollow { /* M1 fields */ pub moved: Tick }   // Hashed "core.path_follow" v2; migrate_from v1: moved = planned
// movement.rs: a Flow unit whose step is fully blocked falls back to a single path only after BLOCKED_REPLAN_TICKS (10) consecutive ticks
//   since `moved`, the last tick it moved; any movement sets `moved` (M4)
// reload.rs: after rebuild_derived, when the reload changed any terrain def or the Movement or Path rules, invalidate_paths over the whole map
//   (every unit Following or on a flow field re-requests, journal "replanned") (M1)
// build.rs: SIM_VERSION = 4; feature.rs: CoreFeature::version() = 4
// path debug query "core.paths" unit entry gains "lane": "command"|"background" for a Waiting unit; journal "core.path_request" gains "lane"
```
- **MCR-002** (written first): "Path lanes, flow stall rule, reload invalidation (M5, M4, M1)". REQs: new `PATH.QUEUE` rules for lanes and `FLOW.GROUP` for the stall rule. Predicted effect: in `testbed_paths`, command requests are processed before background re-plans in the same tick; flow members in the choke fall back later (`choke_crowd` `path.nodes_expanded` drops); a reload changing a `TerrainDef` re-plans followed paths at once. SIM_VERSION 3 → 4. Change-Class: F.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_sim/tests/pathing.rs
#[test] fn command_requests_go_first()           // budget 300: 30 wanderers' background requests queued, then a player move of 1 unit → that request
                                                 //   is processed in the command's tick, before any background request of that tick
#[test] fn background_lane_never_starves()        // a player issues 40-unit moves every tick for 20 ticks with budget 300 → every background
                                                 //   request's waited ≤ its lane position (one per tick at least)
#[test] fn path_queue_v1_migrates_to_command_lane() // v1 bytes with 3 pending → commands == those 3 in order, background empty
#[test] fn path_follow_v1_migrates_moved()       // v1 PathFollow { planned: 17 } → moved 17
// crates/fl_sim/tests/flow.rs
#[test] fn flow_member_survives_short_stalls()    // a group older than 10 ticks; one member blocked for 5 ticks → stays on the field
#[test] fn flow_member_falls_back_after_stall()   // blocked 10 consecutive ticks → single request; journal "replanned"
// crates/fl_sim/tests/reload.rs
#[test] fn reload_that_blocks_a_path_replans()    // a truck following through forest; reload makes forest block Wheeled → that tick "replanned",
                                                  //   and it never enters a forest cell afterwards
#[test] fn reload_without_nav_change_keeps_paths() // a reload changing only Hull.hp → no "replanned"
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim -E 'test(/command_requests|starves|migrates|stall|reload_that|reload_without/)'`. Expected: FAIL.
- [ ] **Step 3: Write MCR-002, then implement.** Update the two specs (`.md.new`) with the new rules and their acceptance criteria naming the step-1 tests; tag those tests with `@req`.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS apart from the protected artifacts; the Task 11 acceptance tests pass.
- [ ] **Step 5: Re-bless (R146).** `cargo run -q -p fl_cli -- rebless --reason MCR-002 --json`; paste the report and check it against MCR-002's prediction (an unpredicted difference is a finding, not a re-bless). Regenerate formats, API, perf counters and budgets (`FL_RECORD_BUDGETS=1`). The lead blesses with `MCR-002: path lanes, flow stall rule, reload invalidation` and lists it for ratification.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track sim-core --track perf`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_sim)!: command and background path lanes, flow stall rule, reload invalidates blocked paths` with trailers `Change-Class: F`, `MCR: MCR-002`, `Refs: ADR-0015`, `Blessed: …`.

### Task 16: Runner and passability carry-overs (M3, M8)

**Track:** contract + sim-core + client · **Refs:** ADR-0002, ADR-0015 · **Lane 2** (after 14)

**Files:**
- Modify: `crates/fl_view/src/runner.rs`, `crates/fl_sim/src/runner/{mod,threaded}.rs`, `crates/fl_sim/src/nav/mod.rs` (the `core.passability` query), `crates/fl_client/src/{link,overlays}.rs`, `crates/fl_client/tests/overlays.rs`, `crates/fl_sim/tests/{runner,debug_query}.rs`
- Blessed: `xtask/snapshots/api/{fl_view,fl_sim}.txt`

**Interfaces:**
- Produces:
```rust
// crates/fl_view/src/runner.rs
#[non_exhaustive] pub enum FromRunner { Tick(TickOutput), Reply { id: u64, reply: SimReply }, Failed(RunnerFailure) }
impl ToRunner { pub fn reply_id(&self) -> Option<u64>; }   // the id a variant expects a reply under (Query and Control: Some; others: None)
// threaded runner: a ToRunner variant it does not handle is answered SimReply::Error("unsupported runner message") when reply_id() is Some
// "core.passability" args gain "since_version": u64 → {"unchanged": true, "version": v} when the grid's version equals it; replies always carry "version"
// the client's passability overlay sends the last version it drew and keeps its cells on {"unchanged": true}
```
- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_view tests (view.rs)
#[test] fn reply_id_of_query_and_control()        // ToRunner::Query { id: 4, .. } → Some(4); Control { id: 9, .. } → Some(9); Shutdown → None
// crates/fl_sim/tests/debug_query.rs
#[test] fn passability_since_version_unchanged()  // first reply has "version": v; again with since_version v → {"unchanged": true, "version": v}
#[test] fn passability_after_terrain_change()     // debug.set_terrain, then since_version v → full reply with "version" > v
// crates/fl_client/tests/overlays.rs
#[test] fn passability_overlay_keeps_cells_when_unchanged() // the fake runner answers {"unchanged": true} → the drawn cell list is unchanged
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_view -p fl_sim -p fl_client -E 'test(/reply_id|since_version|after_terrain|keeps_cells/)'`. Expected: FAIL.
- [ ] **Step 3: Implement**; add wildcard arms where client code matches `FromRunner`.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_view -p fl_sim -p fl_client`. Expected: PASS.
- [ ] **Step 5: Regenerate** API snapshots; the lead blesses: `M2P Task 16: FromRunner non_exhaustive, reply_id, passability since_version`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track sim-core --track client`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_view)!: FromRunner non-exhaustive, ToRunner::reply_id; core.passability since_version` with trailers `Change-Class: R`, `Refs: ADR-0002, ADR-0015`, `Blessed: …`.

### Task 17: Sim vocabulary: conditions, stats and modifiers, tags, effects (C5, sim half)

**Track:** contract + sim-core · **Refs:** ADR-0017 · **Lane 1** (after 15, and 12 and 16 merged: 16 also edits `nav/mod.rs`)

**Files:**
- Create: `crates/fl_sim/src/vocab.rs`, `crates/fl_sim/tests/vocab.rs`, `crates/fl_view/src/conditions.rs`
- Modify: `crates/fl_sim/src/{builder,movement,publish,spawn,nav/mod,lib}.rs` (`move.speed` read through `effective_stat`; `rebuild_core` gains tags and stats), `crates/fl_view/src/lib.rs`, `OWNERS.toml` (`vocab.rs` contract)
- Blessed: system order, `formats__save_codecs.snap`, view formats, API snapshots. **No golden change**: the goldens must verify unchanged (Change-Class N).

**Interfaces:**
- Consumes: `ConditionExpr`, `Requires`, `TagSet`, `ClassExpr`, `VerbList`, `Verb`, `DefRegistry::{register_condition, register_verb}` (Task 12).
- Produces:
```rust
// crates/fl_sim/src/vocab.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)] pub struct ConditionId(pub u16);
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)] pub struct Conditions { .. }   // Hashed "core.conditions" v1: sorted (ConditionId, count); ids hashed by name
#[must_use] pub struct ConditionToken { .. }
pub fn grant_condition(world: &mut World, entity: Entity, id: ConditionId) -> ConditionToken;  // inserts Conditions on first grant; journal "core.condition_changed"
pub fn revoke_condition(world: &mut World, entity: Entity, token: ConditionToken);             // removes the pair at count 0, the component when empty
pub fn has_condition(world: &World, entity: Entity, id: ConditionId) -> bool;
pub fn requires_met(world: &World, entity: Entity, expr: &ConditionExpr) -> bool;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)] pub struct StatId(pub u16);
pub struct StatDef { pub base: fn(EntityRef<'_>) -> Option<Fx>, pub min: Fx, pub max: Fx }   // -1,000,000 ≤ min ≤ max ≤ 1,000,000 (asserted)
pub const MAX_MODIFIERS_PER_STAT: usize = 16;                 // per unit and stat (R149)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum ModOp { Add, AddPct, Mul, Override }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub struct ModSource { pub key: String, pub unit: Option<SimId> }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub struct Modifier { pub stat: StatId, pub op: ModOp, pub value: Fx, pub source: ModSource }
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)] pub struct Modifiers { pub list: Vec<Modifier> }   // Hashed "core.modifiers" v1;
    // kept sorted by (key bytes, unit with None first, op, value raw); the component is removed when the list empties (R151)
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)] pub struct EffectiveStats { .. }               // Derived "core.effective_stats":
    // per stat, the base it was computed from and the effective value (R151)
pub fn add_modifier(world: &mut World, entity: Entity, m: Modifier) -> Result<(), String>;
    // value bounds of ADR-0017 and at most MAX_MODIFIERS_PER_STAT on the stat, else Err with nothing added; marks dirty; an EffectVerb that
    //   gets the Err journals it (core.modifier_changed "rejected") and never panics
pub fn remove_modifiers_from(world: &mut World, entity: Entity, key: &str, unit: Option<SimId>) -> usize;
pub fn effective_stat(world: &World, entity: Entity, stat: StatId) -> Option<Fx>;
    // evaluates on the spot when the unit is marked dirty or the current base (StatDef.base) differs from the cached base; else the cache;
    //   the clamped base for a unit without modifiers (R149, R151: a read never observes a stale value)
pub fn evaluate(base: Fx, def: &StatDef, mods: &[&Modifier]) -> Fx;
    // ADR-0017's buckets and clamps, the add-pct factor clamped at 0, after sorting its input by the total order; pure
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)] pub struct Tags(pub TagSet);  // Derived "core.tags", from content at spawn
#[derive(Message, Clone, Debug)] pub struct EffectRequest { pub verb: Arc<dyn Verb>, pub effect: u16, pub source: SimId, pub target: OrderTarget }
#[derive(Resource, Clone, Debug, Default)] pub struct EffectQueue { .. }   // Hashed "core.effect_queue" v1: effects requested after the core
    // Effects slot, in queue order; codec: effect name (remapped through "effects"), canonical argument bytes, source, target (R151);
    // present only while it holds entries (inserted by queue_effect, removed when drained), so goldens stay hash-neutral; at most
    // MAX_EFFECT_QUEUE = 4096 entries (an entry past it is dropped: journal "core.effect_dropped" reason "queue_full", counter
    // "effects.dropped", debug_assert!); an entry whose Unit target despawned is dropped when applied (journal reason "target_gone") (R153)
pub fn queue_effect(world: &mut World, request: EffectRequest);  // for every emitter after the core Effects slot (feature Effects slot, Stats,
                                                                  //   Lifecycle, an effect's own apply)
pub trait EffectVerb: Verb { fn apply(&self, world: &mut World, source: SimId, target: &OrderTarget); }
impl SimBuilder {
    pub fn register_condition(&mut self, name: &'static str) -> &mut Self;   // also registers it with the content registry; the id is
                                                                              //   condition_id(world, name) once the registry is frozen
}
pub fn condition_id(world: &World, name: &str) -> Option<ConditionId>;
pub fn stat_id(world: &World, name: &str) -> Option<StatId>;
impl SimBuilder {
    pub fn register_stat(&mut self, name: &'static str, def: StatDef) -> &mut Self;   // dense ids exist only after the freeze (stat_id)
    pub fn register_effect<V: EffectVerb + DeserializeOwned>(&mut self, name: &'static str) -> &mut Self;
}
// systems (core slots): recompute_stats first in ThinkSet::Orders (before run_activities) and in ResolveSet::Stats (units marked in the
//   TransientKept "core.stats_dirty", emptied only by the recompute (R149); SimId order; span "recompute_stats"; counter "stats.recomputed");
//   apply_effects in ResolveSet::Effects: the EffectQueue first, in queue order, then the message "core.msg.effect" sorted by (source, emission
//   order); counter "effects.applied"; in debug builds step_tick asserts that no EffectRequest message was written after apply_effects ran
// core stat "move.speed": base Mobility.speed, min 0, max 100 (m per tick); movement reads effective_stat(.., move.speed)
// debug query "core.stats" {"unit": id} → {"<stat>": {"base": raw, "effective": raw, "modifiers": [{"op", "value": raw, "source"}]}}
// crates/fl_view/src/conditions.rs
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)] pub struct ConditionsView { pub units: Vec<(SimId, Vec<u16>)> }  // channel "core.conditions"
```
- [ ] **Step 1: Write the failing tests** (`crates/fl_sim/tests/vocab.rs`, with a test feature registering conditions `test.deployed`, a stat `test.range` (base 100, [0, 1000]) and the effect `test.heal`):
```rust
#[test] fn grants_count_and_revoke()          // two grants of test.deployed → count 2; revoke one → still has; revoke both → no Conditions component
#[test] fn requires_reads_conditions()        // "test.deployed" compiled → false, grant → true
#[test] fn no_modifiers_is_exactly_base()     // evaluate(Fx 100, def, []) == Fx 100 bit for bit
#[test] fn stacking_order_is_fixed()          // base 100; Add +10 (key b), Add +5 (key a), AddPct 0.5, Mul 2, Mul 0.5 → ((100 + 5 + 10) × 1.5) × 2 × 0.5 = 172.5
#[test] fn override_replaces_base()           // Override 40 then Add 5 → 45
#[test] fn extreme_modifiers_stay_in_bounds() // calls the pure evaluate (Add −2,000,000 is outside add_modifier's bounds): base 1,000,000,
                                              //   five Mul 100 → 1,000,000 (clamped after each); Add −2,000,000 → 0 (min)
#[test] fn add_pct_factor_never_negative()    // base 100, min −100: two AddPct −0.6 → factor 0 → 0, not −20
#[test] fn bucket_order_ignores_insertion_order() // two Mul and two Override from one source, inserted in every order, and after a removal
                                              //   → bit-identical results
#[test] fn direct_base_write_is_never_stale() // a modified unit; Mobility.speed written directly (no mark) → effective_stat follows the new base
#[test] fn seventeenth_modifier_is_rejected() // 16 modifiers on move.speed → the 17th is Err, the list unchanged
#[test] fn modifiers_component_removed_when_empty() // add then remove_modifiers_from → no Modifiers component; hash equals an untouched twin
#[test] fn late_effect_is_queued_for_next_tick() // a LifecycleSet::Death system calls queue_effect → applied at the next tick's Effects slot;
                                              //   a save between the two ticks holds it; assert_branch(n = 3, m = 10)
#[cfg(debug_assertions)] #[test] #[should_panic(expected = "EffectRequest")] fn late_effect_message_is_debug_asserted()
                                              //   an EffectRequest message written in ResolveSet::Stats
#[test] fn queued_effect_on_despawned_target_is_dropped() // queue_effect on #3, #3 despawns that tick → not applied next tick; journal
                                              //   "core.effect_dropped" "target_gone"
#[cfg(not(debug_assertions))] #[test] fn effect_queue_is_bounded() // a chain re-queuing two effects per link → the queue stays at 4096, the
                                              //   newest dropped, "effects.dropped" counts them (debug builds assert instead)
#[test] fn out_of_bounds_modifier_is_rejected() // Mul 101 → Err; AddPct −2 → Err
#[test] fn effective_stats_rebuild_after_load() // a modifier, save, load → effective_stat equal; assert_branch(n = 4, m = 30)
#[test] fn move_speed_modifier_slows_unit()   // a unit with Mul 0.5 on move.speed covers half the distance of an unmodified twin in 40 ticks (±1 step)
#[test] fn unit_tags_from_content()           // a spawned base pz3 has Tags with the "tank" bit; ClassExpr "vehicle & !truck" matches it
#[test] fn effects_apply_in_source_order()    // heal requests from #5 then #2 in one tick → applied #2 first, then #5 (journal order)
#[test] fn conditions_channel_published()     // after a grant, the snapshot's "core.conditions" channel lists (unit, [test.deployed id])
#[test] fn stats_query_breaks_down()          // "core.stats" for a modified unit → base, effective and the modifier list
#[test] fn vocabulary_is_hash_neutral()       // the golden replays verify unchanged (no unit has conditions or modifiers)
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test vocab`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_view`. Expected: PASS apart from the protected snapshots; `golden_replays_verify` passes **without** re-bless, and the pose trace tests pass.
- [ ] **Step 5: Regenerate** system order, formats and API snapshots; the lead blesses: `M2P Task 17: sim vocabulary (C5)`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track sim-core`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_sim): conditions, stats and modifiers with cached effective stats, tags, effect verbs; conditions view channel` with trailers `Change-Class: N`, `Refs: ADR-0017`, `Blessed: …`.
- [ ] **Step 8: Checkpoint (controller).** Merge lane 2's finished tasks, `cargo xtask gate pr`. Expected: PASS. Fast-forward `main`.

### Task 18: Render scale baseline (C11)

**Track:** client · **Refs:** spec §5.7, research N22 · **Lane 2** (after 16)

**Files:**
- Create: `crates/fl_client/src/render.rs`, `crates/fl_client/src/shaders/unit_instance.wesl`, `crates/fl_client/tests/render.rs`
- Modify: `crates/fl_client/src/{placeholder,camera,lib}.rs`, `crates/fl_game/src/builder.rs` (plugin wiring)

**Interfaces:**
- Consumes: `PlaceholderAssets`, `placeholder_parts`, `PlayerColors`, `RtsCamera`, `fit_distance`; Bevy `ExtendedMaterial`, `GpuComponentArrayBuffer`/`GpuComponentArrayBufferPlugin`, `VisibilityRange` (examples `shader/gpu_component_array_buffer.rs`, `shader/automatic_instancing.rs`, `3d/visibility_range.rs`; resolve every name with the `bevy-api` skill), `embedded_asset!`.
- Produces:
```rust
// crates/fl_client/src/render.rs
pub type UnitMaterial = ExtendedMaterial<StandardMaterial, UnitInstanceExt>;   // one per unit type (not per player, not per unit)
#[derive(Component, Clone, Copy, Debug, PartialEq)] pub struct UnitInstance { pub palette: u32, pub hp_frac: u16, pub flags: u32 }
    // GpuComponentArrayBuffer, Out = [u32; 4]: palette index, hp_frac, flags (SELECTED = 1), 0
pub fn pack_instance(i: &UnitInstance) -> [u32; 4];
pub const PALETTE_SIZE: usize = 16;                        // player colours in the material's uniform; player index mod 16
#[derive(Clone, Copy, Debug, PartialEq)] pub enum LodLevel { Full, Low, Marker }
pub fn lod_parts(placeholder: &Placeholder, level: LodLevel) -> Vec<(Mesh, Transform)>;   // Full: today's parts; Low: one hull box; Marker: one flat quad
pub fn lod_ranges(view_height_full: f32, view_height_low: f32, view_height_max: f32) -> [VisibilityRange; 3]   // contiguous, crossfading margins
// camera.rs: the rig's distance becomes max(fit_distance(..), view_height × DISTANCE_PER_VIEW_HEIGHT), so VisibilityRange (a camera distance)
//   follows the orthographic zoom; the picture is unchanged (orthographic)
```
- [ ] **Step 1: Write the failing tests** (`crates/fl_client/tests/render.rs`, `MinimalPlugins` plus asset plugins as the existing placeholder tests do):
```rust
#[test] fn one_material_per_unit_type()          // 10 units of 2 types owned by 2 players → Assets<UnitMaterial> has 2 entries
#[test] fn instance_data_packs_owner_and_hp()    // UnitInstance { palette 3, hp_frac 32767, flags 1 } → [3, 32767, 1, 0]
#[test] fn lod_levels_have_fewer_parts()         // a tank placeholder: Full 3 parts, Low 1, Marker 1 (a quad, no height)
#[test] fn lod_ranges_are_contiguous()           // each level's end_margin == the next level's start_margin
#[test] fn distance_follows_zoom()               // view_height ×2 → rig distance ×2 (when above the fit), and the rendered transform's direction is unchanged
#[test] fn gpu_render_stress_3000_instanced()    // gpu_ (nightly, milestone): 3000 units for 25 s → avg_fps ≥ 60
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test render -E 'not test(/gpu_/)'`. Expected: FAIL.
- [ ] **Step 3: Implement** (the WESL shader reads the instance array by the mesh's tag and tints with the palette colour; no texture, no download).
- [ ] **Step 4: Run** the step 2 command. Expected: PASS. Then `cargo run -p fl_game --release -- --stress 3000 --exit-after 25 --agent` (the lane-2 builder). Expected: an `avg_fps` line ≥ 60; paste it with M1's number (M1.md) for comparison.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_client): one material per unit type, per-instance GPU data, three VisibilityRange LOD levels` with trailer `Change-Class: N`.

### Task 19: Path perf: reused search buffers, in-cluster search work (R98, R125, Task 7b; R136 and M6 decisions)

**Track:** sim-core + perf · **Refs:** ADR-0015, ADR-0010 · **Lane 2** (after 18, and 17 merged: both edit `nav/mod.rs`)

**Files:**
- Modify: `crates/fl_sim/src/nav/{hpa,astar,flow,mod}.rs`, `crates/fl_sim/src/path.rs` (call sites only: passes the scratch), `crates/fl_sim/tests/{hpa,astar}.rs`
- Blessed: `docs/perf/budgets.ron` (allocation budgets may only fall), perf counter snapshots, API

**Interfaces:**
- Produces:
```rust
// crates/fl_sim/src/nav/mod.rs
#[derive(Resource, Default)] pub struct NavScratch { .. }   // TransientReset "core.nav_scratch" (clear keeps capacity): A*/Dijkstra cost, parent, heap
                                                            //   and closed buffers, the abstract search's buffers, smoothing buffers
pub fn plan_path_with(nav: &NavGrid, hpa: &HpaGraphs, p: MoveProfile, from: FxVec2, to: FxVec2, rules: &PathRules,
    work: &mut PathWork, scratch: &mut NavScratch) -> PathPlan
// plan_path keeps its signature (a fresh scratch per call) for tests and tools; process_path_queue and flow builds use plan_path_with
// the in-cluster searches before the abstract search (Task 7b) stop as soon as their result can no longer change (an early exit that
//   returns identical distances for every cell the plan reads); charges stay cold costs (ADR-0015), so PathWork is unchanged
```
- **Parked, re-checked:** R136 densification and M6 resumable flow integration stay parked. Their triggers (a map over 1,024 cells per side; HPA* or flow among the top systems in a milestone profile) are re-checked in Task 21's profile and recorded in `docs/perf/M2P.md`.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_sim/tests/hpa.rs
#[test] fn scratch_reuse_gives_identical_plans()  // proptest_policy::config(128, 1024): random maps and request sequences planned with one reused
                                                  //   NavScratch == planned with a fresh one each time (plans and PathWork equal)
#[test] fn in_cluster_early_exit_is_exact()       // the cell costs read by the plan equal the full Dijkstra's on 200 fixed-seed maps
// crates/fl_test_utils/tests/perf_counters.rs (existing): counters_choke_crowd and counters_scale_3000 pass with budgets whose alloc.count
//   and alloc.bytes are at most the current ones
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test hpa -E 'test(/scratch_reuse|early_exit/)'`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS; **goldens verify unchanged** (Change-Class R).
- [ ] **Step 5: Re-record budgets** `FL_RECORD_BUDGETS=1 cargo nextest run --profile agent -p fl_test_utils --test perf_counters`; paste the `alloc.*` before/after per scenario (expected: `choke_crowd` and `scale_3000` allocations fall). The lead blesses: `M2P Task 19: reused search buffers (R98, R125, 7b)`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track sim-core --track perf`. Expected: PASS.
- [ ] **Step 7: Commit** `perf(fl_sim): reused search buffers and an exact early exit for in-cluster searches` with trailers `Change-Class: R`, `Refs: ADR-0015`, `Blessed: …`.

### Task 20: Command buttons with `requires` (C5, client half)

**Track:** client · **Refs:** ADR-0017 · **Lane 2** (after 19, and 17 merged)

**Files:**
- Create: `crates/fl_client/src/buttons.rs`, `crates/fl_client/tests/buttons.rs`
- Modify: `crates/fl_client/src/{feature,lib}.rs`, `crates/fl_game/src/builder.rs`

**Interfaces:**
- Consumes: `ConditionExpr::compile` (Task 12), `ConditionsView` (Task 17), the match header's `"conditions"` table, `ResolvedObject::has`, the selection.
- Produces:
```rust
// crates/fl_client/src/feature.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct HotkeySlot { pub row: u8, pub col: u8 }   // rows QWER / ASDF / ZXCV, 0..3 × 0..4
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandButton { pub id: &'static str, pub icon: &'static str, pub slot: HotkeySlot, pub command: &'static str,
    pub enabled_by: &'static str, pub requires: Option<&'static str> }
pub trait ClientFeature { /* existing */ fn buttons(&self) -> Vec<CommandButton> { Vec::new() } }
// crates/fl_client/src/buttons.rs
#[derive(Resource)] pub struct CommandButtons { .. }          // every feature's buttons plus core's (move Q, stop S, rotate R, follow F); requires
                                                              //   compiled when a match starts; a bad expression or two buttons in one slot is an error at startup
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum ButtonState { Hidden, Disabled, Enabled }
pub fn button_state(button: &CommandButton, compiled: Option<&ConditionExpr>, selected: &[(UnitTypeId, Vec<u16>)], content: &ContentDb) -> ButtonState
    // Hidden: no selected unit's type has `enabled_by`; Enabled: some such unit meets `requires` (or there is none); else Disabled
```
- [ ] **Step 1: Write the failing tests** (`crates/fl_client/tests/buttons.rs`):
```rust
#[test] fn button_hidden_without_enabling_def()   // enabled_by "Mobility", only a unit type without it selected → Hidden
#[test] fn requires_disables_button()             // requires "test.deployed", selected unit without the condition → Disabled; with it → Enabled
#[test] fn any_selected_unit_enables()            // two selected units, one meets requires → Enabled
#[test] fn slot_clash_is_startup_error()          // two buttons in slot (0, 0) → error naming both ids
#[test] fn bad_requires_is_startup_error()        // requires "test.nope" → error "unknown condition 'test.nope'"
#[test] fn core_buttons_registered()              // CommandButtons has "core.move" at Q (0, 0) and "core.stop" at S (1, 1)
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test buttons`. Expected: FAIL.
- [ ] **Step 3: Implement** (no drawing: M6 draws the panel).
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_client -p fl_game`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_client): command buttons with requires, evaluated against the conditions channel` with trailers `Change-Class: N`, `Refs: ADR-0017`.

---

## Phase IV — integration (lane 1)

### Task 21: Integration: prelude gate, perf re-baseline, debugging guide, final report

**Track:** docs + perf · **Refs:** ADR-0009, ADR-0010, ADR-0017–ADR-0022 · **Lane 1** (after every task is merged)

**Files:**
- Create: `docs/m2p-checklist.md`, `docs/perf/M2P.md`, `docs/perf/M2P.json.new` (protected; the lead blesses it into `docs/perf/M2P.json`)
- Modify: `docs/debugging.md` (a section `## 9. M2 prelude`), `README.md` (status line), `xtask/mutants-triage.toml` (lead only)

- [ ] **Step 1: Merge and gate.** With every branch merged into `m2p/phase-a`, run `cargo xtask gate pr`. Expected: `gate pr: PASS`.
- [ ] **Step 2: Update `docs/debugging.md`.** Add, each with an example and its output:
  - `fl_cli rebless`, `schema`, `tuning-tables` and `data-check --tuning-tables`; `cargo xtask spec-lint`, `trace`, `mutants`, `review-tier`, `mini-gate`, `test-guard` with the allowlist;
  - the journal kinds `core.activity_changed`, `core.condition_changed`, `core.modifier_changed`, and `core.order_changed`'s full kind names; the counters `orders.activity_ticks`, `stats.recomputed`, `effects.applied`; the spans `run_activities`, `recompute_stats`;
  - the debug queries `core.activities`, `core.stats`, `core.paths` (immobile, lanes), `core.passability` (`since_version`), with request and response JSON;
  - the reject code `core.no_map`; content patches, the provenance ledger and the path-key syntax; the MCR and re-bless recipe; the release-check profile; `FL_PROPTEST_RANDOM`, `FL_SEED_BASE`, `FL_RECORD_POSES`;
  - a "why did my golden change" recipe: `fl_cli rebless --reason MCR-n` → report → `replay dump --json` → `bisect-desync`.
  Run every documented command and check its output matches.
- [ ] **Step 3: Content, determinism and traceability.**
  - `cargo run -p fl_cli --release -- data-check assets/base --tuning-tables docs/design/generated/tuning-tables.md` → exit 0.
  - `cargo xtask spec-lint` → 0 errors; `cargo xtask trace --milestone` → 0 untraced, holdout shares in range.
  - `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_sim -p fl_test_utils` → PASS; `replay-verify` on both goldens → exit 0.
  - The latest `cargo xtask mutants` results (Task 14 and the controller's per-task runs): every missed mutant gets a test or a `xtask/mutants-triage.toml` entry with a reason (the lead writes entries; `Blessed:` trailer).
- [ ] **Step 4: Perf (tier 3, R128).** Ask the user to confirm the fixed power plan first (a system setting the agents don't change).
  - Run `cargo run -p fl_cli --release -- bench --units 500,3000,5000 --ticks 400 --warmup 40 --repeat 5 --out docs/perf/M2P.md --json docs/perf/M2P.json.new --baseline docs/perf/M1.json --profile`, interleaved with the `m1`-tag binary at equal load as in M1 Task 21.
  - Expected: 3000-unit mean ≤ 15 ms (G7); within 20 % of `docs/perf/M1.json` at every size, or a written per-system justification in `M2P.md` (Task 13's activity ticks, Task 17's stat reads; Task 19's buffers should offset them).
  - Record the R136 and M6 trigger check (Task 19) in `M2P.md`, and add machine, rustc, commit and power plan. `M2P.json.new`'s `meta.head` names the measured commit.
  - The lead blesses `docs/perf/M2P.json` (`--reason "M2P Task 21: prelude perf baseline (R128)"`); it becomes the baseline for M2's 20 % rule (replacing R143's M1.json).
- [ ] **Step 5: Render stress.** `cargo run -p fl_game --release -- --stress 3000 --exit-after 25 --agent`. Expected: `avg_fps` ≥ 60; append to `M2P.md`.
- [ ] **Step 6: Checklist.** Write `docs/m2p-checklist.md` with one row per Confirmation bullet of ADR-0017 to ADR-0022 and the four refinements, naming the passing test or gate row, plus a testbed walk: units still path, queue, follow and form up as in M1; the F5–F7 overlays work (the passability overlay stops re-polling an unchanged grid); zooming out switches LOD levels with a crossfade.
- [ ] **Step 7: Prelude gate.** `cargo xtask gate milestone --milestone M2P`. Expected: `gate milestone: PASS`, with the holdout, `release-determinism`, `mutants-triage`, `golden-policy`, `spec-lint` and `trace` rows passing, and every `milestone_`, `gpu_` and `brp_` filter matching tests.
- [ ] **Step 8: Final report.** The lead's report to the user lists for ratification: ADR-0017 to ADR-0022 and the four refinements (R85); the five backfilled mechanic specs (Task 4); the re-blesses MCR-000 (Task 5), MCR-001 (Task 13) and MCR-002 (Task 15) (R146); the parked items with triggers (see Builder's calls); the power-plan note.
- [ ] **Step 9: Review and commit.** Run the final whole-branch review (`superpowers:requesting-code-review`), fix its findings, re-run `cargo xtask gate pr`, and commit `docs: M2 prelude debugging guide, checklist and perf baseline`. The controller fast-forwards `main` and tags it `m2p`.

---

## Builder's calls and deviations recorded here

- **Own RON tree (C6).** ron 0.12's `deserialize_any` drops bare identifiers (`handle_any_struct` → `visit_unit`), so neither `ron::Value` nor `serde_json::Value` keeps enum variants such as `class: Tracked`. The loader parses our content's RON subset into `ContentTree`, and Task 7 proves the typed result unchanged with byte-identical rules hashes.
- **Engine parameters in the provenance vocabulary.** The closed vocabulary has no class for values the spec fixes (`StateHash.interval_ticks`, `Spatial.cell_m`); they are `Guess` with ids in `GDD.md` §Guesses and Medium confidence.
- **Roster values.** Footprints and road speeds match historical vehicles (`OursScaled`); hit points and turn rates are invented (`Guess`); `hitbox_shrink` follows research §1.3 (`ResearchDerived`). Every value is explicit in the ledger rather than defaulting concrete units to `OursScaled`.
- **Holdout secrecy.** Under R65/R82 the controller dispatches general-purpose agents, whose sessions don't load the repo's hooks. The `guard-read` hook protects holdout files when role agents run; until then the implementer briefs forbid reading them, and the `holdout` feature keeps them out of every implementer build and gate output.
- **Re-bless authority (R146).** C8's "re-bless needs the user" becomes: the lead re-blesses with an MCR reason under the user's delegation, and the final report lists every re-bless for ratification.
- **Mutation testing time cap.** cargo-mutants has no global time limit, and an in-place run killed mid-mutant would leave mutated source behind. The xtask wrapper uses copy mode, a dedicated warm build dir and a kill at the cap. The per-task run is the controller's at review time (`--cap 600`), not a Stop-hook row, so `gate task` stays fast.
- **`core.move_order` keeps its name** for the order queue: renaming a Hashed state name would need a codec rename mechanism, and the name is only a key.
- **`MAX_WAYPOINTS` stays a code constant** (R77 asked for a rules section with C7): it bounds the save decoder, so a content value could make valid saves undecodable. `MAX_COMMAND_UNITS` moves to `fl_protocol`, where the client reads it.
- **Command buttons before M6.** No button API existed, so Task 20 adds the declaration contract, the `requires` evaluation and the core buttons; M6 draws the panel.
- **LOD under an orthographic camera.** `VisibilityRange` measures camera distance, which an orthographic zoom doesn't change, so Task 18 ties the rig distance to the view height; the picture is unchanged.
- **Seed rotation source.** Nightly `FL_SEED_BASE` is the number of days since 2026-01-01 (UTC), set by the gate.
- **"Budget overrun FAIL at nightly" (M17)** becomes the nightly `task-time` row that times the task-level selection; every level warns over its own budget, and every run appends to `target/xtask/gate-times.jsonl` for trends.
- **Mini-gate.** `cargo xtask mini-gate <AREA.NAME>` runs a mechanic's lint, trace and traced tests with holdout, so a milestone is a collection of mechanics that each passed their own gate.
- **Parked with triggers:** R136 HPA* densification and M6 resumable flow integration (a map over 1,024 cells per side, or HPA*/flow among the top systems in a milestone profile); R115 yielding, R127 head-on jams, R130 regrouping onto a shared field (the first M2 chase or swarm scenario that jams); R134 always-on terrain-change event and R116 unsticking (before M7's bridges); player, team, global and aura modifier scopes (their first user, M5 morale or M9 experience); trigger verb execution (map triggers); M7's per-unit immobile answer exists now, and its M4 overlay use comes with buildings.

## Self-review

**Spec coverage** (Group C and R145 → task):

| Item | Task(s) |
|---|---|
| C1 GDD, mechanic specs, `spec-lint`, `trace` | 4, 9 (specs updated in 15) |
| C2 provenance ledger, `data-check`, tuning tables | 8 (path keys and origins from 7) |
| C3 test-author, protected acceptance tests, held-out 10–20 % at milestone and nightly | 11 (trace bounds in 9, gate in 11) |
| C4 Wilson at α = 1e-4, two tiers, nightly rotation, one reroll, test-guard on `ALPHA` and counts | 11 |
| C5 conditions and `requires`; modifier and stat spec (ADR) with stacking and cache; tags and class expressions; effect and trigger verbs; buttons with `requires` | 3 (ADR-0017), 12, 17, 20 |
| C6 load phases, migrate, patch ops in passes, post-process hooks, provenance per path, deprecated keys | 3 (ADR-0020), 7, 10 |
| C7 order queue + activity stack, table-stored state, custom `SimId` relationships, event-model ADR, v2 → v3 migration | 3 (ADR-0018, ADR-0019), 2 (frozen v2), 13 |
| C8 `SIM_VERSION` policy, semantic assertions, `fl_cli rebless --reason MCR-n` with report, change-class trailers | 3 (ADR-0021), 4 (policy text), 5; used in 13, 15 |
| C9 cargo-mutants per task (capped), per merge, nightly sharded, milestone triage | 14, 21 |
| C10 risk-tiered review, per-mechanic mini-gate | 14 |
| C11 one material per unit type, per-instance data, `VisibilityRange` LOD, stress ≥ 60 fps | 18, 21 |
| R145: I2, M1, M3, M4, M5, M7, M8, M10–M14, M16, M17, R139, R140, parked perf | carry-over map above |
| Closing: milestone-style gate, perf re-baseline, `debugging.md` | 21 |

- **Type consistency.** These names are used the same way across tasks: `ContentTree`, `Node`, `Span`, `path_key`, `Origin`, `ContentDb::{origins, provenance, condition_expr, tags, unit_tags}`; `ProvSource`, `ProvEntry`, `Confidence`; `PatchOp`, `PatchCond`, `PatchPhase`, `CONTENT_FORMAT_VERSION`, `DefRegistry::{register_migration, deprecate_field, register_post_process, register_condition, register_verb}`; `ConditionExpr`, `Requires`, `TagSet`, `ClassExpr`, `Verb`, `VerbFamily`, `VerbList`; `REPLAY_FORMAT_VERSION` 4, `ReplayStart::Scale { version, params }`, `SemanticCheck`, `SemanticFailure`, `VerifyOutcome::SemanticFailed`, `rebless_replay`, `ReblessEntry`; `OrderQueue`, `QueuedOrder`, `OrderTarget`, `OrderKindId`, `ActivityStack`, `Activity`, `ActivityState`, `ActivityKindId`, `Step`, `Outcome`, `OrderBehaviour`, `ActivityBehaviour`, `give_order`, `stop_unit`, `run_activities`; `Relation`, `RelatedTo`, `Related`, `relate`, `unrelate`, `RelationError`; `Codec::migrate_with`, `LoadCtx::current_id`; `PathFollow::{reset, moved}`, `Lane`, `PathQueue { commands, background, next_id }`; `Conditions`, `ConditionId`, `ConditionToken`, `condition_id`, `StatId`, `stat_id`, `StatDef`, `Modifier`, `ModOp`, `ModSource`, `Modifiers`, `EffectiveStats`, `Tags`, `EffectRequest`, `EffectVerb`, `ConditionsView`; `UnitMaterial`, `UnitInstance`, `LodLevel`; `NavScratch`, `plan_path_with`; `CommandButton`, `HotkeySlot`, `CommandButtons`, `ButtonState`; `pose_hash`, `pose_trace`; `proptest_policy::{config, runner, cases}`; `stats::{ALPHA, z_two_sided, wilson_interval, assert_rate_within_wilson, SeedSet, seed_base}`.
- **Review Focus.** Each of its five lines has its tests in the owning task (2 and 13; 10; 8 and 9; 5; 12 and 17).
- **Proportion.** The plan carries signatures, rules values, file formats and test assertions. Bodies appear only where they are contracts: the stacking order and bounds (ADR-0017), the patch order (ADR-0020), the lane rule (ADR-0015 refinement), the classification table and the MCR texts.
