---
status: "accepted"
date: 2026-10-23
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: agent-dev-environment.md (top 10, things to avoid); workflow-and-verification.md §5.2; baseline-recommendations.md B8, B9, B10"
---

# Agent development environment; local-only CI

## Context and Problem Statement

Frontline is built mostly by parallel Claude Code subagents on one Windows 11 laptop. Revision 1 shared one target dir
(`C:/t/fl`) across all worktrees, which serialises builds on Cargo's file lock and costs about 10 GB per Bevy build dir.
Agents write Bevy 0.20 API from memory, but 0.20 shipped after their training data. Checks were prose in `CLAUDE.md` and
plans. A live game was only reachable by humans. The research also recommended a GitHub remote with 3-OS CI (B8); the
user decided to skip it.

## Decision Drivers

* Parallel agents must not block each other or exhaust the machine.
* Correct Bevy 0.20 APIs without guessing.
* Compact, machine-readable feedback (agent context is the binding constraint).
* Agents can drive and observe the live game.
* The user's decision: no remote; CI stays local.

## Considered Options

* Status quo: shared target dir, prose rules, no MCP
* The baseline agent environment (below), with local-only CI
* The baseline environment plus a remote and 3-OS GitHub Actions (B8)

## Decision Outcome

Chosen option: "The baseline agent environment, with local-only CI".

- **Build loop:**
  - a per-worktree short build dir in the repo `.cargo/config.toml` (R19), `build.build-dir = "C:/t/fl/{workspace-path-hash}"`, and
    no shared `target-dir` in the repo config; a `WorktreeRemove` hook deletes it
  - `rust-lld` for `x86_64-pc-windows-msvc`
  - less debug info (`line-tables-only` for workspace crates, none for dependencies)
  - sccache with `SCCACHE_BASEDIRS`
  - **at most two concurrent builds**
- **Claude config in the repo:**
  - `CLAUDE.md` under 200 lines, plus path-scoped rules in `.claude/rules/`
  - hooks in `.claude/settings.json` (ADR-0009)
  - role subagents in `.claude/agents/`: `sim-engineer`, `client-engineer`, `test-author` (active from M2), `reviewer`
    (read-only) and `perf-analyst` (never edits)
  - a project skill `bevy-api` that resolves APIs from the local Bevy 0.20 sources, examples and migration guides in
    `~/.cargo/registry`, then `docs/research/bevy020-api-reference.md`, then a `probes/` compile probe
- **Feedback:** a nextest `agent` profile (failure-only output, JUnit, fail-fast) and the rust-analyzer LSP plugin. No
  `cargo check` hook after every edit.
- **Live game:** `bevy_brp_mcp` 0.23 in the project `.mcp.json`, one BRP port per agent, `fl.*` methods published as
  agent tools (ADR-0008), offscreen screenshots, continuous updates in agent and bench mode.
- **Global installs** (approved by the user): cargo-nextest, cargo-insta, cargo-deny, cargo-public-api, cargo-mutants,
  sccache, bevy_brp_mcp. Plus a dated nightly toolchain for rustdoc JSON (ADR-0011).
- **Early probes in M0:** `bevy/dynamic_linking` with `bevy_ecs`-only sim crates (duplicate-crate link errors?), and
  `bevy_remote` custom methods with a runner round trip and an offscreen screenshot.
- **CI is local only** (B8 skipped by the user): `cargo xtask gate pr` before merging into `main`, `gate nightly` on the
  dev machine.
- **Left to the user** (system and security settings): Dev Drive or Defender exclusions, and a fixed power plan for
  milestone benches.

### Consequences

* Good, because parallel worktrees build independently on short paths, and dependencies are shared through sccache.
* Good, because API mistakes are caught against the pinned sources instead of memory.
* Good, because agents verify features in the running game through the command path.
* Bad, because of disk use: one build dir per worktree (about 10 GB each) until the worktree is removed.
* Bad, because without a remote, determinism is verified on Windows x64 only, and instruction-count trends (ADR-0010)
  have no runner. A local WSL2 Linux job could cover both; that is the user's call. B8 returns as a Group D item with a
  trigger (spec §9.3).

### Confirmation

* `gate task` timings; the build-dir layout under `C:/t/fl/`.
* The M0 probes are recorded in `docs/research/bevy020-api-reference.md`.
* A `CLAUDE.md` line count of 200 or less, checked by `xtask`.

## Pros and Cons of the Options

### Status quo

* Good, because there is no setup.
* Bad, because of lock contention, API hallucination and advisory checks.

### Baseline environment, local CI

* Good, because it covers everything the research ranked highest, within the user's no-remote decision.
* Bad, because it is single-OS.

### Plus a remote and 3-OS CI

* Good, because it gives cross-OS determinism and independent verification.
* Bad, because the user declined a remote for now.

## More Information

* Spec §3, §13.3–13.5, §15 C.
* Research: `agent-dev-environment.md` (all), `workflow-and-verification.md` §5.2 (the B8 sketch, kept for later).
* Related: ADR-0008, ADR-0009, ADR-0010, ADR-0011.
