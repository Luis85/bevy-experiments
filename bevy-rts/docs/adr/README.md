# Architecture Decision Records

Frontline records its architecture decisions here, one file per decision, in
[MADR 4.0](https://adr.github.io/madr/) format. The design spec
(`docs/superpowers/specs/2026-10-23-frontline-rts-design.md`) says *what* the architecture is; the ADRs say *why*, and
which alternatives were rejected.

## Index

| ADR | Title | Status | Date | Spec |
|---|---|---|---|---|
| [0001](0001-two-crate-features.md) | Two crates per feature; `SimFeature` and `ClientFeature` with topological order | accepted | 2026-10-23 | §5.1, §5.2 |
| [0002](0002-threaded-sim-runner-and-view-snapshots.md) | Threaded `SimRunner`, `fl_view` snapshots and `TickDriver` | accepted (M2 prelude refinement 2026-10-25: proposed) | 2026-10-25 | §5.3 |
| [0003](0003-keyed-counter-based-rng.md) | Keyed, counter-based randomness; no sequential RNG in the sim | accepted | 2026-10-23 | §5.4 |
| [0004](0004-stable-string-registries.md) | Stable-string registries for every runtime ID | accepted | 2026-10-23 | §5.4, §5.5 |
| [0005](0005-sim-schedule-phases-and-spawn-funnel.md) | Sim phases, explicit sync points, spawn funnel, driver housekeeping, narrowed ordering rule | accepted | 2026-10-23 | §5.4, §5.6 |
| [0006](0006-component-classification-and-save-format.md) | Mandatory state classification, versioned save format and the branch test | accepted (M1 refinement 2026-10-24; M2 prelude refinement 2026-10-25: proposed) | 2026-10-25 | §5.4, §5.8 |
| [0007](0007-lockstep-command-envelope-and-match-header.md) | Lockstep command envelope, postcard, `MatchHeader` and the canonical rules hash | accepted (M1 refinement 2026-10-24; M2 prelude refinement 2026-10-25: proposed) | 2026-10-25 | §5.4, §5.5 |
| [0008](0008-debug-actions-as-commands.md) | Debug actions are commands; BRP reaches the sim only through `fl.*` | accepted | 2026-10-23 | §5.5, §11, §15 C |
| [0009](0009-verification-gates-and-protected-tests.md) | `cargo xtask gate`, erosion lints and protected tests | accepted | 2026-10-23 | §12.1–12.5 |
| [0010](0010-perf-gates-via-work-counters.md) | Perf gates via deterministic work counters; wall clock only at milestones | accepted | 2026-10-23 | §5.7, §12.6 |
| [0011](0011-contract-governance.md) | Contract governance: API and format snapshots, ownership map, ADR rule | accepted | 2026-10-23 | §12.7, §13.2 |
| [0012](0012-hot-reload-policy.md) | Hot reload: values in place, ID-set changes rebuild, reloads are replay events | accepted (M1 refinement 2026-10-24) | 2026-10-24 | §6.4 |
| [0013](0013-agent-dev-environment.md) | Agent development environment; local-only CI | accepted | 2026-10-23 | §13.3–13.5 |
| [0014](0014-terrain-layers-and-movement-classes.md) | Terrain layers and movement classes: PNG layers decoded by `fl_content`, per-class blocking and clearance | accepted | 2026-10-24 | §6.2, §7 |
| [0015](0015-path-requests-derived-caches-debug-queries.md) | Path requests, derived navigation caches and debug queries | accepted (M2 prelude refinement 2026-10-25: proposed) | 2026-10-25 | §5.3, §5.7, §11 |
| [0016](0016-ai-controller-seam.md) | AI controller seam: a client-hosted `AiController` on a client-built `PlayerView` (stub scope) | accepted | 2026-10-24 | §5.5, §8 |
| [0017](0017-core-vocabulary.md) | Core vocabulary: conditions, stats and modifiers, tags, verbs and command buttons | accepted | 2026-10-25 | §5.4, §6.1, §9.2 C5 |
| [0018](0018-unit-behaviour-and-entity-graph.md) | Unit behaviour and the entity graph: order queue, activity stack and `SimId` relationships | accepted | 2026-10-25 | §5.4, §5.6, §5.8, §7, §9.2 C7 |
| [0019](0019-sim-event-model.md) | Sim event model: Messages for gameplay, hooks for invariants, observers only for order-independent reactions | accepted | 2026-10-25 | §5.4, §5.6, §9.2 C7 |
| [0020](0020-content-load-phases-patches-provenance.md) | Content load phases over a lossless RON tree; patches, per-path origins and the provenance ledger | accepted | 2026-10-25 | §6.1, §6.4, §9.2 C2, C6 |
| [0021](0021-golden-replay-versioning.md) | Golden replay versioning, change classes, MCRs and the re-bless protocol | accepted | 2026-10-25 | §5.5, §9.2 C8, §12.5 |
| [0022](0022-verification-from-m2.md) | Verification from M2: mechanic specs and tracing, test author and holdout, statistics, mutants, review tiers | accepted | 2026-10-25 | §9.2 C1, C3, C4, C9, C10, §12 |

The M2 entry requirements (spec §9.2, Group C) are recorded in ADR-0017 to ADR-0022 and in the M2 prelude
refinements of ADR-0002, ADR-0006, ADR-0007 and ADR-0015 (spec revision 2.5); the controller accepts them at the M2
prelude's Task 3 review under the user's delegation (R85), and the plan's final report lists them for ratification.
AI placement (spec §8) gets its ADR with the first AI faction; ADR-0016 records only the M1 stub seam.

## Rules

- **Contract paths change only through an ADR.** The contract paths are listed in spec §13.2 and in the `contract`
  track of `OWNERS.toml`: `fl_math`, `fl_protocol`, `fl_view`, the `fl_sim` extension API (builder, phases, registries,
  feature traits, runner protocol, save format), the `fl_content` loader phases and registry, `xtask`, `.claude/`,
  `deny.toml`, `OWNERS.toml`, the generated `clippy.toml` template and the design spec. A branch that changes them
  without a new or amended ADR fails review; `xtask check-scope` keeps other tracks out of them.
- **One decision per file.** Number sequentially (`NNNN-short-title.md`); never reuse a number.
- **Status flow:** `proposed` → `accepted` (or `rejected`). An accepted ADR is not rewritten to change its decision.
  Write a new ADR and mark the old one `superseded by ADR-NNNN`. Clarifications and typo fixes are fine in place.
- **Who decides:** the user, or the architect with the user's approval. Implementer agents may propose an ADR (status
  `proposed`) and stop; they don't accept one.
- **Keep the spec in sync.** An ADR that changes a contract updates the spec section it names in the same commit.
- Start from [`template.md`](template.md).

## Deprecation policy

Applies from the end of M0. Until then the contracts are still being written; they change freely, but still through
ADRs.

- **Rust contract items** (public items of contract crates): mark `#[deprecated(since = "M<n>", note = "use … instead")]`
  and remove them one milestone later. The public-API snapshot shows both steps.
- **Content keys** (def-component fields, kinds, rules sections): accept the old name with `#[serde(alias = "…")]`,
  make `data-check` warn with the list of deprecated keys in use, and when removing the key add a content migration
  (`format_version` step, spec §9.2 C6).
- **Saves:** migrations are kept forever, or until an explicit save-break ADR that names the last loadable version.
- **Replays:** guaranteed only when `sim_version`, features, mods and the rules hash match (spec §5.5). Golden replays
  are re-blessed under the protocol of C8 rather than kept compatible.
- **Wire format** (`PlayerCommand`, payloads, `MatchHeader`): any change bumps `format_version` or the payload's version
  and updates the `serde-reflection` snapshot (spec §12.7).
