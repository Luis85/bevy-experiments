---
status: "accepted"
date: 2026-10-25
decision-makers: "controller (accepts under the user's delegation, R85), lead"
consulted: "M2 prelude planner (planner-m2p-report.md; spec-gap resolution 4 accepted as R148); M1 final review (final-review-m1.md M10); research: workflow-and-verification.md §4.6, §6.1–§6.3; baseline-recommendations.md C8"
informed: "M2 prelude implementers (Tasks 4, 5, 13, 15); every later task that touches code or content"
---

# Golden replay versioning, change classes, MCRs and the re-bless protocol

## Context and Problem Statement

Golden replays (`crates/fl_test_utils/tests/replays/*.replay`) pin the hash stream of the testbed scenarios. In M1 a
golden was re-recorded with `FL_RECORD_GOLDEN=1` and blessed by the lead whenever a task changed the stream, and
nothing tied that change to a stated reason, a `SIM_VERSION` bump or a predicted effect. M2 changes rules on purpose
(combat formulas, suppression), so a careless re-bless would hide a real regression behind an "expected" hash change.
Spec §9.2 C8 asks for a `SIM_VERSION` bump policy, semantic assertions next to the hashes, `fl_cli rebless --reason
MCR-n` with a report, and change classes in commit trailers, with refactors kept bit-identical. Separately (M10),
`ReplayStart::Scale(ScaleScenario)` embeds the bench struct, so every new scale-mix field bumps the replay format
(2 → 3 in M1) and invalidates every replay. C8 says re-blessing needs the user; the user delegated it (R146).

## Decision Drivers

* A refactor proves itself by leaving every golden hash stream bit-identical.
* Every intended change to outcomes is written down, with its predicted effect, before the code (research §6.2).
* A re-bless cannot pass a broken behaviour: named semantic checks must still hold.
* The replay format survives bench changes, and old replays still read.
* The gate enforces the rules mechanically; review checks only the reasons.

## Considered Options

* Versioned goldens with classes, MCRs and a gated tool
* Free re-recording
* Backward-compatible goldens forever

## Decision Outcome

Chosen option: "Versioned goldens with classes, MCRs and a gated tool", because it
makes every hash change carry a stated reason that the gate can check, while keeping the command streams as the
durable artefact.

- **Replay format 4** (`REPLAY_FORMAT_VERSION` 4):
  - `ReplayStart::Scale { version, params }`: the scale scenario is a versioned params blob
    (`ScaleScenario::VERSION`, `encode`, `decode`; an unknown version is an error naming it), so a `ScaleScenario`
    field no longer bumps the replay format (M10).
  - Named **semantic checks** next to the hashes (`Replay.semantic: Vec<SemanticCheck>`, `#[non_exhaustive]`:
    `UnitWithin`, `UnitExists`, `EntityCount`, `NeverOnTerrain`), run during every verify after the hashes;
    a failure is `VerifyOutcome::SemanticFailed` with the check's id and tick. The checks follow from a scenario's
    setup (no unit dies, no unit stands on water), never from a recorded run.
  - A **journal summary** (record counts by kind) for re-bless reports.
  - Format 3 still reads (a frozen `mod v3`, with no semantic checks and an empty summary).
- **Change classes.** Every commit that touches `crates/**`, `features/**`, `assets/**`, the root `scenarios/**`, or
  the root `Cargo.toml` or `Cargo.lock` (class E) carries a `Change-Class: R|D|F|N|E|X` trailer: Refactor, Data
  tuning, Formula or rule, New mechanic, Engine or crate upgrade, eXperiment (research §6.1). Merge commits are
  exempt.
  - **R and E** keep every golden hash stream bit-identical. **N** keeps every existing golden bit-identical (a new
    mechanic is off in the existing scenarios). **F** bumps `SIM_VERSION`. **X** changes outcomes only behind its
    flag.
  - **D** changes the rules hash. Every golden's header records the rules hash of the base content, and replay
    verification requires it equal (`check_replay`), so a class D commit that touches the base mod re-blesses every
    base-content golden, and needs an MCR like class F (its entry names the tuning and its sweep).
  - **The format exemption is per commit** (R151, R153). A golden's *bytes* may change under R or E only in the
    commit that itself changes `REPLAY_FORMAT_VERSION` (MCR-000, Task 5), and only when that golden's hash stream at
    the commit equals its hash stream at the commit's parent, which the `golden-policy` row checks. Every other R or E
    commit on the branch that changes a golden is a violation.
- **`SIM_VERSION` policy.** `SIM_VERSION` (`crates/fl_sim/src/build.rs`) is bumped by every class F change, at most
  once per task, and by no other class. It is 2 at the start of the M2 prelude, 3 after Task 13 (MCR-001) and 4 after
  Task 15 (MCR-002). The rules hash covers content; `SIM_VERSION` covers code semantics.
- **MCR.** A `## MCR-nnn — <title>` entry in `docs/design/changelog-mechanics.md`, written before the code: why, the
  requirement ids added, changed or removed, the predicted effect on outcomes, and the `SIM_VERSION` change (and its
  class). A class F or D commit carries `MCR: MCR-nnn`, and so does a format-only re-record (MCR-000). The re-bless
  report must confirm the prediction; an unpredicted difference is a finding, not a re-bless.
- **Re-bless** only through `fl_cli rebless --reason MCR-nnn`:
  - It refuses (exit 1, writing nothing) when the changelog has no `## MCR-nnn` heading, or when any replay's
    semantic check fails on the new build.
  - Otherwise it re-runs each golden's start and remapped inputs on this build (`rebless_replay`, journal on), writes
    `<name>.replay.new` for every replay whose bytes change (same start, inputs, ticks and checks; this build's header,
    hashes and journal summary) and a report, `target/rebless-report.md`: per replay the first divergent tick, the
    journal summary diff, the entity count before and after, and the semantic results.
  - The lead blesses the `.new` files with `cargo xtask bless … --reason "MCR-nnn: …"` under the user's delegation
    (R146), and the final report of the plan lists every re-bless for the user's ratification.
  - Commands are the durable artefact; hashes are derived and regenerated.
- **Gate rules** (`xtask/src/change_class.rs`):
  - `change-class` row (task, pr): every commit since the base that needs a class carries a known one, and a class F
    or D commit an `MCR:` trailer naming an existing changelog heading. A class R or E commit that changes a golden
    is a violation unless that same commit changes `REPLAY_FORMAT_VERSION` (read from the `pub const` line in
    `crates/fl_sim/src/replay.rs` at the commit and at its parent).
  - `golden-policy` row (pr, milestone): a changed golden needs an `MCR:` trailer and a changed `SIM_VERSION`,
    replay format or content, where "content changed" means a file under `assets/**` or the root `scenarios/**`
    differs between the merge base and `HEAD`. A changed `SIM_VERSION` needs a changed golden, so no bump is
    decorative. The row reads `SIM_VERSION` and `REPLAY_FORMAT_VERSION` from their `pub const` lines at the merge base
    and at `HEAD`.
  - The hash-stream check is **per commit, never merge base against `HEAD`** (R153): for every golden that a class R
    or E commit changes, the row decodes that golden's bytes at the commit and at the commit's parent with
    `Replay::from_bytes` (it reads formats 3 and 4) and requires equal hash streams. Later class F re-blesses on the
    same branch (MCR-001, MCR-002) change the streams at `HEAD` legitimately, and a branch-wide comparison would
    report them against the earlier format commit.

### Consequences

* Good, because a hash change without a class, an MCR and a version or content change fails the gate before review.
* Good, because the semantic checks keep a re-bless from blessing a unit that walked into water.
* Good, because the bench can grow scale-mix fields without invalidating every replay.
* Bad, because every code commit needs a trailer, and every class F task writes an MCR and a prediction first.
* Bad, because a re-bless now runs a tool and produces a report instead of one environment variable.
* Neutral, because old replays are not verified against old binaries; replays are guaranteed only for matching
  `sim_version`, features, mods and rules hash (spec §5.5).

### Confirmation

* Replay format (Task 5, `fl_sim/tests/replay.rs`): `format_3_replays_still_read`,
  `scale_start_round_trips_versioned_params`, `scale_params_of_unknown_version_fail`, `semantic_failure_is_reported`,
  `never_on_terrain_checks_every_tick`, `rebless_keeps_hashes_for_unchanged_code`; `golden_semantics_hold`
  (`fl_test_utils/tests/golden.rs`).
* Re-bless tool (Task 5, `fl_cli/tests/rebless.rs`): `rebless_refuses_unknown_mcr`,
  `rebless_refuses_failing_semantic_check`, `rebless_writes_new_and_report`.
* Gate rules (Task 5, `xtask/src/change_class.rs`): `code_commit_needs_class`, `class_must_be_known`,
  `golden_change_needs_mcr_and_version`, `mcr_must_exist_in_changelog`, `sim_version_bump_needs_a_golden_change`,
  `refactor_class_cannot_change_goldens`, and for R151 and R153 `format_commit_may_rewrite_goldens` (the exemption
  holds only in the commit that changes the format), `format_exemption_needs_equal_hash_streams`,
  `later_reblesses_do_not_fail_the_format_commit` and `data_commit_needs_mcr`; the `change-class` and
  `golden-policy` rows.
* Use: MCR-000 (Task 5, hash streams unchanged), MCR-001 (Task 13) and MCR-002 (Task 15), each re-blessed through
  `fl_cli rebless` with the report pasted; class R and N tasks (5, 6, 7, 10, 12, 16, 17, 19) keep
  `golden_replays_verify` passing without a re-bless.

## Pros and Cons of the Options

### Versioned goldens with classes, MCRs and a gated tool

* Good, because each hash change is explained, predicted and checked by the gate.
* Good, because a refactor's bit-identical goldens are a strong safety net, now enforced rather than hoped for.
* Bad, because of process overhead on every code commit and every rule change.

### Free re-recording

* Good, because it costs nothing per change.
* Bad, because any hash change can be blessed, including an unintended one, and nothing records why.
* Bad, because the replay format stays coupled to the bench struct.

### Backward-compatible goldens forever

* Good, because every old replay stays verifiable.
* Bad, because it needs an archived binary and a build per `SIM_VERSION`, which the local-only CI (ADR-0013) cannot
  afford, and bit-exact backward compatibility is not a goal (research §6.3).

## More Information

* Spec §5.5 (replays and their guarantees), §9.2 C8, §12.5 (bless rule).
* Research: `workflow-and-verification.md` §4.6 (golden replays), §6.1 (classes), §6.2 (MCR), §6.3 (versioning and
  re-bless protocol).
* Related: ADR-0007 (`MatchHeader`, `sim_version`, rules hash), ADR-0009 (gate and protected paths), ADR-0011
  (contract governance), ADR-0022 (verification from M2).
* Rulings: R146 (the lead re-blesses with an MCR reason, listed for ratification), R148, R149, R151 and R153 (the
  per-commit format exemption, checked by hash-stream equality against the commit's parent).
* Revisit: when players' saved replays need cross-version playback (a "requires version N" message), or if a tagged
  binary per `SIM_VERSION` becomes affordable.
