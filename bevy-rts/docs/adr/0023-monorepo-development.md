---
status: accepted
date: 2026-10-26
decision-makers: repository maintainer, under the requested repository setup
---

# Portable development inside the Bevy Experiments monorepo

## Context and Problem Statement

The maintainer copied Frontline alongside three other Cargo workspaces into the
GitHub repository and requested a development and AI development setup. Previous
instructions assumed Frontline was the Git root, worktrees lived under `C:/fl/`,
and all contributors had sccache, rust-lld and a `C:/t/fl/` build directory.
Those assumptions break fresh clones and make nested change/policy checks read
the wrong paths. A nested `.github/workflows/` also does not provide monorepo CI.

## Decision Outcome

Keep four independent Cargo workspaces and lockfiles. Root documentation,
`AGENTS.md`, a PowerShell helper and a root GitHub Actions matrix provide the
shared workflow. Frontline retains its own architecture, protected oracles,
ownership tracks, gates and Claude hooks.

Use platform-default build paths/compiler/linker. Users may opt into caching and
short build paths through personal Cargo configuration or environment variables.
Current checkouts and ordinary isolated branches/worktrees are supported; there
is no mandatory absolute worktree location. The old cleanup hook remains narrowly
restricted to its existing optional `C:/t/fl/` root and never expands its deletion
authority to arbitrary paths.

Diffs and commit path lists are scoped to and relative to this Cargo workspace.
Historical file reads use Git's `REV:./path` syntax. Hook policy discovery uses
the nearest `OWNERS.toml`, falling back to the Git root, so nested protected paths
remain guarded. Regression tests cover nested workspaces and sibling changes.

Root CI runs format check, Clippy and workspace tests on Windows independently
for each project. This baseline complements Frontline's full local gates; GPU,
BRP, API snapshot and performance verification keep their existing requirements.
This decision supersedes the absolute machine paths and local-only CI assumptions
in ADR-0013. No gameplay, replay, protected baseline or dependency version changes
are part of the repository setup.
