---
status: "accepted"
date: 2026-10-25
decision-makers: "controller (accepts under the user's delegation, R85), lead"
consulted: "M2 prelude planner (planner-m2p-report.md; spec-gap resolutions 3, 5, 10–13 accepted as R148); M1 final review (final-review-m1.md I2, M16, M17); research: workflow-and-verification.md §2.2, §3, §4.4, §5.3–§5.5, §8; baseline-recommendations.md C1, C3, C4, C9, C10"
informed: "M2 prelude implementers (Tasks 1, 4, 6, 9, 11, 14, 21); the test-author; reviewers"
---

# Verification from M2: mechanic specs and tracing, test author and holdout, statistics, mutants, review tiers

## Context and Problem Statement

M0 and M1 verified mechanics with tests written by the same agent that wrote the code, against plan text, with every
task reviewed twice in full. M2 adds randomised combat, where a test can pass by measuring the code instead of the
rule, and many small mechanics, where two full reviews per task and a 15-minute task gate (M17: 916 s against a
300 s budget) bound throughput. Spec §9.2 asks for machine-checkable mechanic specs with requirement tracing (C1), a
separate test author with held-out tests (C3), a statistical test policy (C4), mutation testing (C9) and risk-tiered
review with a per-mechanic mini-gate (C10). The M1 final review adds a release-build determinism check (I2), and the
M1 rulings fix the journal opt-in (R139) and one proptest policy (R140, R142).

## Decision Drivers

* Tests come from the spec, not from the code; a mechanic's rules map to the tests that check them.
* Statistical tests are never flaky and never tuned to the implementation.
* Gate time stays near its budget while coverage grows (M17).
* Review effort follows risk; protected and contract changes get the most.
* Release builds behave like the debug builds that recorded the goldens (I2).

## Considered Options

* Spec-first verification
* Keep the M1 process
* Coverage gates

## Decision Outcome

Chosen option: "Spec-first verification", because it is the only option that checks tests against an independent
statement of each rule and spends review where the risk is.

- **Mechanic specs (C1).** `docs/design/GDD.md` indexes one spec per mechanic, `docs/design/mechanics/<AREA>-<name>.md`
  (protected; written as `.md.new` and blessed by the lead), in the format of `docs/design/mechanic-template.md`: a
  header with status (`draft | approved | implemented | deprecated`), spec version, crate and milestone; EARS rules
  with ids `AREA.NAME.N`; formulas; a parameter table with path keys (ADR-0020) and provenance; acceptance criteria
  naming `<file>::<test>`; non-goals, open questions and guesses (`G-<AREA>-<n>`); a changelog.
  - `cargo xtask spec-lint` checks them: unique, prefixed rule ids; every rule in an acceptance criterion; every
    named test exists (unless draft); every parameter key matches the provenance ledger with the same class; every
    journal kind is registered; guess ids are unique and every ledger `Guess` id is defined; every `Tuned` MCR has a
    changelog heading; the GDD registry lists exactly the spec files. It reads the tables and ledger from `fl_cli
    schema --json`.
  - `cargo xtask trace` maps `// @req AREA.NAME.N` tags in `crates/**/tests/**` and `crates/**/src/**` to rules and
    writes `target/trace.md`; an untraced rule of an `approved` or `implemented` spec, or a tag naming no rule,
    fails. Holdout coverage appears in its output and in `target/trace.md` as counts per rule and per spec, never
    as test names or file paths (R151).
  - The lead approves the five backfilled M1 specs under the user's delegation; they are listed for ratification
    (R148).
- **Test author and holdout (C3).** The test-author agent writes acceptance tests (`tests/acceptance/`, run at every
  level) and holdout tests (`tests/holdout/`, built only with the `fl_test_utils` `holdout` feature, run at nightly and
  milestone) from the spec alone, against the black-box API (`ScenarioBuilder`, `fl_protocol` payloads, `Scenario`
  poses and world reads, `fl_view` snapshots), each tagged `@req`. The lead commits them with a `Blessed:` trailer;
  implementers never edit them (protected) and may not read holdout files. Holdout is at least 10 % of a spec's
  acceptance and holdout tests: `trace --milestone` fails below 10 % and warns above 20 %.
  - **Holdout secrecy is best-effort, and stated as such** (R151). The `guard-read` hook denies `Read`, `Grep` and
    `Glob` on `**/tests/holdout/**` to implementer roles, and a Bash pattern guard denies implementer commands that
    name a holdout path (`cat`, `rg`, `git show` and the like on `tests/holdout`). Implementer briefs forbid reading
    them where the hooks do not run (general-purpose agents, R148). None of this is a mechanical guarantee: a search
    rooted at an ancestor directory, or a command that builds the path indirectly, can still reach the files.
  - The tools this ADR defines never print holdout content to implementers: `cargo xtask mini-gate` runs holdout
    tests only with `--holdout`, which it refuses unless `FL_ROLE=lead` (the role variable the hooks and `cargo xtask
    bless` already read, R153), and then prints only pass and fail counts; `trace` reports holdout coverage as
    counts. The `holdout` feature keeps the tests out of every task-level build and gate output.
- **Statistical policy (C4).** Oracles come from the spec or an f64 reference, never from a run.
  `fl_test_utils::stats::assert_rate_within_wilson` at `ALPHA` = 1e-4 (Wilson interval, two-sided). Two tiers:
  formula (`FORMULA_DRAWS` = 10⁵ draws) and scenario (200–500 seeds, `SeedSet`). Nightly sets `FL_SEED_BASE` to the
  number of days since 2026-01-01 (UTC). A failing seed set may be rerolled once, to the disjoint base
  `seed_base + 100_000` (not `seed_base + 1`, which is the next night's set, so a pass there would only repeat
  tomorrow's run), and the reroll is recorded in the report. `test-guard` flags a changed `ALPHA`, draw or seed
  count (`ToleranceChanged`) and a changed `seed_base(` literal (`NewSeedOverride`).
- **Mutation testing (C9).** `cargo xtask mutants` wraps cargo-mutants in copy mode (never `--in-place`, so a killed
  run leaves no mutated source), with its own build dir, nextest, one job and a time cap, over `fl_math`,
  `fl_content` and `fl_sim` (and feature sims). The controller runs it per task at review (`--cap 600`, `--base` the
  task base), not in the Stop hook. Gate rows: `mutants` at pr (in-diff against the base, cap 1,200 s, WARN on a
  missed mutant or the cap) and at nightly (the full set in 4 shards, cap 3,600 s each, WARN); `mutants-triage` at
  milestone (in-diff since the previous milestone tag; FAIL on any missed mutant not listed with a reason in
  `xtask/mutants-triage.toml`, protected). Coverage stays diagnostic only.
- **Risk-tiered review (C10).** `cargo xtask review-tier [--base]` names the highest tier a diff touches:
  - **P**: protected paths. The lead reviews and blesses; the change is listed for the user.
  - **A**: contract paths or the sim core (`crates/fl_sim/src/**`, `fl_math`, `crates/fl_content/src/**`, feature
    sims). A spec and determinism review, then a quality review.
  - **B**: client, tools and test utilities (non-protected). One combined review.
  - **C**: docs, and data with `data-check` green. One short review.

  `cargo xtask mini-gate <AREA.NAME> [--holdout]` runs that spec's `spec-lint` and `trace`, then every
  non-holdout test tagged with its rules; with `--holdout` (the lead) it also runs the tagged holdout tests and
  reports them as counts only. A milestone is a set of mechanics that each passed their own gate, holdout included.
- **Release determinism (I2).** A `release-determinism` row at nightly and milestone runs the headless sim tests
  (`fl_sim`, `fl_test_utils`, `fl_catalog`, `fl_content`) under `[profile.release-check]` (release with
  `overflow-checks = true`), including the goldens recorded in debug builds.
- **Gate time (M17).** The task level runs the touched packages' tests minus `xtask/heavy-tests.txt` (protected),
  plus the smoke set; heavy tests run from pr. Every level warns over its budget (task 300 s, pr 900 s, nightly
  5,400 s, milestone 3,600 s); the nightly `task-time` row fails when the smoke set exceeds 120 s or the task-level
  selection its budget; every run appends to `target/xtask/gate-times.jsonl`. `test-guard` suppresses findings
  accepted in `xtask/test-guard-accepted.toml` (protected; each entry names its ruling), so the row shows only new
  ones (M16).
- **Journal (R139).** Determinism tests (twin, shuffle, branch, round trip, golden) never read the journal. A test
  that reads it builds its scenario with `ScenarioBuilder::journal(true)`; `Scenario::journal` panics otherwise, so
  journal tests pass in release builds too.
- **Proptests (R140, R142).** Every proptest uses `fl_test_utils::proptest_policy`: a fixed ChaCha seed, no failure
  persistence, the case count from `PROPTEST_CASES` with a per-test default and cap, and random seeds only at nightly
  (`FL_PROPTEST_RANDOM=1`). Committed regressions become named tests; whole-match proptests cap their cases (gate 64,
  milestone 512).

### Consequences

* Good, because each rule of each mechanic is stated once, traced to its tests, and partly tested by tests its
  implementer never saw.
* Good, because statistical tests have a false-failure rate set by α, not by luck, and cannot drift to the code.
* Good, because review and gate time follow risk: docs and data get a short review, the sim core a full one.
* Bad, because every mechanic needs a spec before code, and the trace and holdout rules add bookkeeping.
* Bad, because holdout secrecy is best-effort: hooks and brief rules, not a mechanical guarantee, while
  general-purpose agents run without the repo's hooks.
* Bad, because mutation runs are long builders; they run at review and on schedules, not in every gate.

### Confirmation

* Gate time and allowlist (Task 1, `xtask` unit tests): `heavy_filter_lists_each_test`,
  `heavy_line_needs_three_fields`, `task_filter_excludes_heavy`, `pr_runs_heavy_tests`, `task_time_row_is_nightly_only`,
  `level_budget_warns`, `gate_times_appended`, `accepted_findings_are_suppressed`, `accepted_entry_needs_ruling`,
  `summary_counts_suppressed`; the measured `test-headless` row at most 150 s on M1 Task 14's base.
* Journal, proptests and release (Task 6): `journal_does_not_change_hashes`, `journal_needs_opt_in`,
  `cases_respects_cap`, `config_is_fixed_and_unpersisted`, `hpa_regression_1`; the `release-determinism` row.
* Specs and tracing (Task 9): `parses_template_spec`, `duplicate_rule_id_is_error`, `uncovered_rule_is_error`,
  `missing_test_is_error`, `param_key_must_match_ledger`, `param_provenance_must_agree`,
  `unknown_guess_id_is_lint_error`, `tags_are_scanned`, `untraced_rule_fails`, `unknown_tag_fails`,
  `holdout_share_bounds`, `schema_lists_tables_queries_provenance`; the `spec-lint` and `trace` rows
  (`spec-lint: 5 specs, 0 errors`, `trace: <n> rules, 0 untraced, 0 unknown tags`).
* Test author and statistics (Task 11): `z_matches_known_quantiles`, `wilson_matches_reference`,
  `rolls_chance_matches_probability`, `wilson_rejects_a_wrong_oracle`, `seed_set_is_fixed`,
  `holdout_feature_at_nightly_only`, `guard_read_denies_holdout`, `bash_guard_denies_holdout_paths`,
  `seed_base_change_is_flagged`, `reroll_base_is_disjoint`; the pilot
  acceptance and holdout tests for `MOVE.SPEED` and `ORDER.QUEUE`, with `trace --milestone` holdout shares in
  [10 %, 20 %].
* Mutants, tiers and mini-gate (Task 14): `summarize_counts_outcomes`, `untriaged_lists_new_missed`,
  `mutants_never_in_place`, `mutants_rows_by_level`, `tier_by_paths`, `mini_gate_selects_tagged_tests`,
  `mini_gate_hides_holdout_names`, `mini_gate_holdout_needs_lead_role`; the `mutants` and `mutants-triage` rows;
  `mini-gate MOVE.SPEED: PASS`. Task 9's `holdout_share_bounds` also checks that the trace output names no holdout
  test.
* Integration (Task 21): `gate milestone --milestone M2P` with the holdout, `release-determinism`, `mutants-triage`,
  `golden-policy`, `spec-lint` and `trace` rows passing.

## Pros and Cons of the Options

### Spec-first verification

* Good, because tests have an independent oracle, and tracing shows which rules lack one.
* Good, because the process scales to many small mechanics through tiers and mini-gates.
* Bad, because it front-loads work (specs, ledger, holdout) before each mechanic's code.

### Keep the M1 process

* Good, because it is known and needs no new tooling.
* Bad, because the implementer's tests share the implementer's misreading of the rule, and randomised tests drift to
  whatever the code does.
* Bad, because two full reviews per task and a 15-minute task gate bound M2's throughput.

### Coverage gates

* Good, because coverage is cheap to measure and easy to explain.
* Bad, because a covered line is not a checked line; mutation testing measures whether a test would notice a change,
  and tracing whether a rule has a test at all.

## More Information

* Spec §9.2 C1, C3, C4, C9, C10; §12 (gates, protected tests, test-guard); §13 (delivery process).
* Research: `workflow-and-verification.md` §2.2 (why a separate test stage), §3 (specs, provenance, traceability),
  §4.4 (statistical tests), §5.3–§5.5 (milestone gate, anti-cheating, review checklist), §8.
* Related: ADR-0009 (gates, protected tests), ADR-0010 (perf gates), ADR-0011 (contract governance), ADR-0013 (agent
  environment, hooks), ADR-0020 (path keys, provenance), ADR-0021 (MCRs, change classes).
* Rulings: R139, R140, R142, R146, R148 (holdout guard and feature, copy-mode mutants at review, mini-gate per
  `AREA.NAME`, `FL_SEED_BASE`, the nightly `task-time` row, lead-approved backfilled specs), R151 (best-effort
  holdout secrecy, counts-only tools, the disjoint reroll base).
* Revisit: when role agents run with the repo's hooks for every dispatch (holdout secrecy becomes mechanical), or
  when mutation runs fit inside `gate task`.
