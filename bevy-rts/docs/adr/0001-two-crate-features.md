---
status: "accepted"
date: 2026-10-23
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: architecture-best-practices.md N1/C1; baseline-recommendations.md A1"
---

# Two crates per feature; `SimFeature` and `ClientFeature` with topological order

## Context and Problem Statement

Revision 1 of the spec (§5.1, §5.2) put each feature in one crate with a `sim` module and a `client` module behind a
`client` Cargo feature, and one `GameFeature` trait with `sim()` and `client()` methods. Cargo unifies features across
the workspace, and the opt-out (`resolver.feature-unification`) is still unstable on cargo 1.99.1 (verified by probe).
So `cargo clippy --workspace` and `cargo test --workspace` compile every sim half *with* the client code and full Bevy.
The gates could never prove the sim is headless. Clippy reads only the nearest `clippy.toml`, so per-half lint bans
(no `f32` in the sim half) were impossible too.

## Decision Drivers

* The sim must provably compile and test without `bevy_render`, `bevy_window`, `bevy_input` or `bevy_time` (G6, §5.1).
* Determinism lints must apply to sim code and not to client code.
* Ownership must be expressible per half for parallel agents (`OWNERS.toml`).
* Registering a feature stays one line (G2).
* Bevy has no plugin dependency resolution; feature order must not depend on `add_plugins` order.

## Considered Options

* One crate per feature with a `client` Cargo feature (revision 1)
* Two crates per feature: `fl_feat_x` (sim) and `fl_feat_x_client`
* One crate per feature, gated with `cfg` plus the unstable `-Zfeature-unification`

## Decision Outcome

Chosen option: "Two crates per feature", because it is the only stable option under which per-package builds, lints and
dependency checks see the sim half alone.

- Layout: `features/<name>/{sim,client}` → crates `fl_feat_<name>` and `fl_feat_<name>_client`.
- Traits (object-safe, so catalogs hold `Vec<Arc<dyn SimFeature>>`):
  ```rust
  pub trait SimFeature: Send + Sync + 'static {          // fl_sim
      fn id(&self) -> &'static str;
      fn version(&self) -> u32;
      fn deps(&self) -> &'static [&'static str] { &[] }
      fn build(&self, sim: &mut SimBuilder);
  }
  pub trait ClientFeature: Send + Sync + 'static {       // fl_client
      fn sim_feature(&self) -> &'static str;
      fn build(&self, app: &mut App);
  }
  ```
  Each feature type also exposes `pub const ID: &str`, for other features' `deps()` lists.
- `GameBuilder` (and `build_sim_app` when headless) sorts features topologically by `deps()`, breaking ties by id, with
  `core` first. Cycles, missing dependencies, duplicate ids and a `ClientFeature` whose `SimFeature` is absent are
  errors.
- The sorted `(id, version)` list goes into the `MatchHeader` (ADR-0007).
- `fl_catalog` lists the linked `SimFeature`s for every headless binary; `fl_game` lists the `ClientFeature`s.
- A client crate never depends on `fl_sim` directly. `fl_feat_x_client` may depend on its own `fl_feat_x` for view, def
  and command payload types.

### Consequences

* Good, because `xtask arch-check` can verify, one package at a time, that no sim crate reaches a render, window, input
  or time crate.
* Good, because each sim crate gets its own generated `clippy.toml` with the float and collection bans.
* Good, because feature order is explicit and deterministic, and the header records exactly which features ran.
* Bad, because there are twice as many crates and `Cargo.toml` files. `xtask` generates the skeleton of a new feature.
* Bad, because a feature's view and payload types must live in its sim crate (in `view` and `protocol` modules) so the
  client half can use them without touching sim internals.

### Confirmation

* `cargo xtask arch-check`: the allowed-edges table and per-package `cargo tree` (spec §12.2).
* Per-package headless test runs in `gate task` and `gate pr`.
* `GameBuilder` unit tests for cycle, missing-dependency, duplicate and orphan-client errors.
* The public-API snapshot of `SimFeature`, `ClientFeature` and `SimBuilder` (ADR-0011).

## Pros and Cons of the Options

### One crate with a `client` Cargo feature

* Good, because it is one crate per mechanic, with shared private types.
* Bad, because workspace feature unification compiles the sim with the client code in every `--workspace` gate.
* Bad, because lint bans and ownership can't distinguish the halves.

### Two crates per feature

* Good, because boundaries are enforced by the compiler and checkable by tools.
* Neutral, because shared types move to `view`/`protocol` modules of the sim crate, which is cleaner anyway.
* Bad, because of more crates and manifests.

### `cfg` gating with `-Zfeature-unification`

* Good, because it keeps one crate.
* Bad, because it needs nightly cargo (RFC 3692 is unstabilised); the project builds on stable.

### Associated consts on the trait (`const ID`, `const VERSION`)

Considered as a variant of the chosen option, as sketched in the research. Rejected because associated consts make the
trait non-object-safe, and both catalogs and the builder need trait objects. Methods plus a `pub const ID` on each
feature type give the same stability.

## More Information

* Spec §5.1 (layout, dependency rules), §5.2 (feature contract).
* Research: `docs/research/architecture-best-practices.md` N1, C1, C20; `baseline-recommendations.md` A1.
* Related: ADR-0004 (feature ids namespace the stable names), ADR-0009 (arch-check), ADR-0011 (ownership).
