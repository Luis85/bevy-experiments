---
status: "accepted"
date: 2026-10-23
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: architecture-best-practices.md N17/N18/C24; baseline-recommendations.md B6, B7"
---

# Contract governance: API and format snapshots, ownership map, ADR rule

## Context and Problem Statement

Parallel subagents build on shared contracts: `fl_math`, `fl_protocol`, `fl_view`, the `fl_sim` extension API, and the
content loader. Revision 1 (§13) said each agent owns "separate crates or modules", but nothing enforced it, and a
contract change was invisible unless a reviewer happened to notice. The crates are internal and unpublished, so semver
tooling doesn't apply by default. "Semver discipline" here means no surprise changes to what other agents build on.

## Decision Drivers

* Contract changes are visible in every diff that makes them.
* Wire and save format changes can't slip in without a version bump.
* Parallel agents stay inside their declared paths.
* Decisions and their rationale survive agent context resets.

## Considered Options

* Review discipline only (revision 1)
* Snapshots of the public API and serde formats, `OWNERS.toml` with `xtask check-scope`, and ADRs for contract paths
* Publish the contract crates and use `cargo-semver-checks`

## Decision Outcome

Chosen option: "Snapshots, an ownership map and ADRs".

- **Public API snapshots:** `cargo-public-api` for `fl_math`, `fl_protocol`, `fl_view` and the `fl_sim` extension API
  (`SimBuilder`, phases, registries, `SimFeature`, the runner protocol), stored as insta snapshots. It needs rustdoc
  JSON from a pinned, dated nightly toolchain, used for nothing else.
- **Format snapshots:** `serde-reflection` for `PlayerCommand`, core command payloads, `MatchHeader`, the snapshot types
  and every registered save codec. A format change without a version bump fails the gate.
- **`OWNERS.toml`** maps path globs to tracks (`contract`, `sim-core`, `client`, `content`, `tools`, `feature:<name>`).
  `cargo xtask check-scope --track <name>` fails a branch that touches paths outside its track. Plans name each task's
  track.
- **ADRs** (MADR 4.0) in `docs/adr/`. **Contract paths change only through an ADR**: a new or amended ADR in the same
  branch, and an updated spec section.
- **Deprecation policy** (in `docs/adr/README.md`): `#[deprecated]` with removal one milestone later; content keys via
  `serde(alias)`, a `data-check` warning and a content migration; save migrations kept forever or until a save-break
  ADR. It applies from the end of M0.
- Contract enums and structs are `#[non_exhaustive]` where growth is expected.

### Consequences

* Good, because every contract change shows up as a snapshot diff that needs `xtask bless` with a reason.
* Good, because ownership is checked, not assumed, so parallel tracks don't collide.
* Bad, because of a nightly toolchain install (rustdoc JSON only) and some friction on legitimate contract changes.
* Neutral, because `cargo-semver-checks` stays optional: it misses field and parameter type changes.

### Confirmation

* `gate pr` runs the API and format snapshot tests and `check-scope`.
* Review: a contract-path diff without an ADR reference is rejected.

## Pros and Cons of the Options

### Review discipline only

* Good, because it is free.
* Bad, because it is invisible and unenforced, and drifts with parallel agents.

### Snapshots, ownership map and ADRs

* Good, because it is mechanical, visible and documented.
* Bad, because of tooling setup and the nightly dependency for rustdoc JSON.

### Publish and semver-check

* Good, because it uses standard tooling.
* Bad, because publishing internal crates is pointless, and the checks miss type changes.

## More Information

* Spec §12.7, §13.2.
* Research: `architecture-best-practices.md` N17, N18, C24; `baseline-recommendations.md` B6, B7.
* Related: ADR-0009, and every ADR that defines a contract (0001–0008).
