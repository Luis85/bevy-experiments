---
status: "accepted"
date: 2026-10-23
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: workflow-and-verification.md §4.2, §4.5, §5.1, §5.4; agent-dev-environment.md #2, #4; architecture-best-practices.md N16/C20; baseline-recommendations.md B1–B4"
---

# `cargo xtask gate`, erosion lints and protected tests

## Context and Problem Statement

Revision 1 (§12) ended every task with `cargo fmt --check`, `cargo clippy --workspace`, `cargo test --workspace` and a
`check_deps.sh` script, run because the plan says so. The rules are advisory: an agent can skip them or report success
without evidence. Determinism checks (twin runs) come late, and a workspace-wide `cargo tree` reflects unified features
and hides dependency violations. The implementer agent writes and can edit its own tests. Benchmarks find frontier
coding agents weakening or skipping tests (ImpossibleBench, EvilGenie). No remote exists, so there is no CI to fall back
on (B8 skipped).

## Decision Drivers

* "Verified" must mean the same thing for agents, hooks and the user.
* Determinism and dependency rules fail at the commit that breaks them, not milestones later.
* Agents can't weaken, delete or auto-accept the tests that judge them.
* The per-task gate stays fast (under 3–5 minutes), or agents learn to dodge it.

## Considered Options

* Prose instructions in the plan (revision 1)
* An `xtask` gate as the single entry point, enforced by Stop/SubagentStop hooks, with generated lints, `arch-check`,
  cargo-deny, early determinism checks and protected tests
* Hosted CI as the only enforcement (not possible: no remote)

## Decision Outcome

Chosen option: "An `xtask` gate as the single entry point, enforced by hooks".

- **`cargo xtask gate {task,pr,nightly,milestone}`** runs the checks in spec §12.1. The task level covers the touched
  crates plus an `fl_sim` smoke set; `pr` covers the workspace before a merge into `main`.
- **Hooks:** `Stop` and `SubagentStop` run `gate task` on changed crates and block with a compact summary until it
  passes, with a retry cap and escalation. Task reports paste the output.
- **Lints (B2):** `xtask gen-clippy` writes a `clippy.toml` per sim crate from one template (clippy reads only the
  nearest file):
  - banned types: `f32`/`f64`, std and `bevy_platform` hash maps, `Instant`/`SystemTime`, `ParallelCommands`, `Time`,
    `Pcg32`
  - banned methods: `thread::spawn`, `rand::rng`, `SystemTime::now`, `env::var`
  - `clippy::float_arithmetic = "deny"`, with scoped `#[expect]` only at `fl_math` and `fl_content` conversion edges
  - only resolvable paths are emitted
  - The gate fails if a generated file drifts.
- **`xtask arch-check`:** an allowed-edges table and `cargo tree -p` one package at a time, plus per-package headless
  test runs. It replaces `scripts/check_deps.sh`.
- **`cargo-deny`:** licences, advisories, duplicate `bevy_*`, bans on bincode and `bevy_dynamic_plugin`.
- **Determinism moved left (B3):** ambiguity `Error`, `shuffle_seed` runs (1 per task, 3 per `pr`), an insta snapshot
  of the `SimTick` system order, replay and save round trips, and the branch test.
- **Protected tests (B4):**
  - Protected paths: `**/tests/{acceptance,holdout,replays}/**`, `**/snapshots/**`, root `scenarios/**`,
    `docs/design/mechanics/**`, perf baselines and budgets.
  - A PreToolUse hook denies `Edit|Write` on them for implementer roles.
  - A Bash hook blocks `cargo insta accept`, `INSTA_FORCE_PASS`, `INSTA_UPDATE=always`, `--bless`, `cargo xtask bless`
    and `git checkout -- tests/` for those roles.
  - Acceptance goes only through `cargo xtask bless <paths> --reason …` by the lead or the user.
  - `xtask test-guard` flags weakened tests for explicit reviewer acceptance.
  - The escape hatch: `BLOCKED: test X contradicts spec Y because …`, then stop.

### Consequences

* Good, because "done" is mechanically checked, with evidence in every report.
* Good, because erosion (floats, hash maps, a client dependency in the sim, a schedule-order change) fails immediately.
* Good, because self-graded tests lose their main failure mode.
* Bad, because of upfront `xtask` work in M0 and some friction: new snapshots wait for the lead to bless them.
* Bad, because everything runs on one Windows machine; cross-OS determinism stays unverified until a remote exists
  (ADR-0013).

### Confirmation

* The hooks are in `.claude/settings.json` and the role agent definitions; a smoke test tries a protected-path edit
  from an implementer role and expects a denial.
* `gate task` timing is reported; over 5 minutes is a defect.
* `gate pr` before every merge into `main`, its output in the merge commit or task report.

## Pros and Cons of the Options

### Prose instructions

* Good, because there is nothing to build.
* Bad, because they are advisory; agents skip or misreport them.

### `xtask` gate and hooks

* Good, because one definition of verified is enforced at the point of completion.
* Bad, because of the tooling effort and the need to keep the task gate fast.

### Hosted CI only

* Good, because it is independent of the agent's machine.
* Bad, because it is unavailable: the user skipped the remote (B8).

## More Information

* Spec §12.1–12.5, §13.3 (hooks and roles).
* Research: `workflow-and-verification.md` §4.2, §4.5, §5.1, §5.4; `agent-dev-environment.md` #2, #4;
  `architecture-best-practices.md` N16, C20.
* Related: ADR-0005 (schedule snapshot), ADR-0006 (branch test), ADR-0010 (perf gates), ADR-0011 (contract snapshots).
