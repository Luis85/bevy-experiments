---
status: "accepted"
date: 2026-10-23
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: architecture-best-practices.md N3/C3; workflow-and-verification.md §6.6; baseline-recommendations.md A3"
---

# Keyed, counter-based randomness; no sequential RNG in the sim

## Context and Problem Statement

Revision 1 (§5.4) put one PCG generator in a sim resource (`SimRng(Pcg32)` in M0 code), drawn in system order, with
per-entity derived seeds only in parallel phases. With one shared stream, any system that adds or removes a draw shifts
every later draw in the tick. Adding a feature then changes unrelated golden replays, outcomes depend on cross-feature
system order, and parallelism is blocked. Factorio hit exactly this (FFF #421: the selector combinator couldn't be
multithreaded because it relied on a global random generator).

## Decision Drivers

* Adding a feature must not perturb scenarios that don't use it (extensibility, G2; change class N stays bit-identical).
* Randomness must not be order-sensitive, so systems can be reordered and parallelised safely.
* Statistical tests and A/B sweeps need draws that stay coupled across variants (common random numbers).
* Bit-identical on every platform.

## Considered Options

* One sequential PCG resource, drawn in system order (revision 1)
* One sequential PCG per feature or per stream, stored in resources
* Keyed, counter-based randomness: a stateless hash of `(seed, tick, sim_id, stream, n)`

## Decision Outcome

Chosen option: "Keyed, counter-based randomness", because it is the only option where a draw depends on what it is for,
not on when it happens.

- `fl_math::roll(seed: u64, tick: u32, sim_id: u32, stream: u64, n: u32) -> u64` is a stateless hash: inputs packed and
  folded with a splitmix64-style finaliser, so every input affects every output bit, with reference vectors in tests.
- `fl_math::Rolls` is a short-lived local cursor over one `(seed, tick, sim_id, stream)` key. It increments `n` per
  draw (including rejection-sampling retries) and offers `next_u32`, `range_u32`, `chance`, `unit_fx` and `range_fx`,
  matching today's `Pcg32` methods. It lives in a local variable, never in a resource, component or `Local`.
- `stream` is the **stable 64-bit hash of a namespaced stream name** (`"combat.dispersion"`), computed by a `const fn` in
  `fl_math`. It is deliberately **not** a dense registry ID: a dense ID would shift when a feature adds a name that
  sorts earlier, which would move every later stream's draws. The stream registry (ADR-0004) still exists, to reject
  duplicate names and hash collisions and to list streams in the header.
- World-level draws use `SimId(0)`. Each stream name has one owner and purpose; `n` separates draws within it.
- The match seed is a `Hashed` resource set from the `MatchHeader`.
- Banned in sim crates: `fl_math::Pcg32`, `rand`, `bevy_rand`, or any RNG state in a resource, component or `Local`
  (generated `clippy.toml`, ADR-0009). `Pcg32` stays for content-time tools and the client.
- `derive_seed` (M0 code) is replaced by `roll`.

### Consequences

* Good, because adding, removing or reordering draws in one stream never changes another stream.
* Good, because parallel phases need no RNG rule, and randomness drops out of the order-sensitive list (ADR-0005).
* Good, because A/B sweeps with common random numbers stay tightly coupled.
* Bad, because every draw site must name a stream, and two sites sharing a name would correlate. The registry and review
  guard against that.
* Bad, because a hash per draw costs a little more than a PCG step; negligible next to the work that consumes it.
* Neutral, because `SimId` allocation order still matters: a feature that spawns more entities shifts later `SimId`s
  and therefore their draws. That is inherent and covered by goldens.

### Confirmation

* Clippy bans in every sim crate (no `Pcg32` or `rand` types or calls).
* `fl_math` tests: reference vectors, every input changes the output, tick and `sim_id` are not interchangeable,
  `range_*` bounds and coverage (ported from the `Pcg32` tests).
* A golden-stability test: adding a dummy stream and a dummy drawing system leaves an existing scenario's hash stream
  unchanged.
* Review checklist: every draw site names a registered stream.

## Pros and Cons of the Options

### One sequential resource

* Good, because it is simple, and the state is small and hashable.
* Bad, because of global draw-order coupling, perturbed replays and no parallelism.

### A sequential generator per stream

* Good, because it decouples features from each other.
* Bad, because draws within a stream are still order-dependent across entities, the state must be hashed and saved,
  and parallel use is still blocked.

### Keyed counter-based

* Good, because it is stateless, order-independent and parallel-safe.
* Bad, because of the discipline of naming streams.

## More Information

* Spec §5.4 (randomness), §12.2 (lints).
* Research: `architecture-best-practices.md` N3, C3; Factorio FFF #421; `workflow-and-verification.md` §6.6.
* Related: ADR-0004 (stream registry), ADR-0006 (seed is hashed state).
