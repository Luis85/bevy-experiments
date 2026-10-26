# ADR 0002: An integer, deterministic, headless simulation

- Status: Accepted
- Date: 2026-10-23
- Source: villager spec §4 and §6.2, `docs/research/ai-prior-art.md` (Determinism), `docs/research/framework-best-practices.md` §4

## Context

The framework targets colony sims and RTS games that will later need lockstep multiplayer (opposing armies,
shared colonies), replays and reproducible bug reports. Lockstep sends only commands, so every peer must
compute bit-identical state. Float transcendentals, `mul_add`, SIMD code paths, `HashMap` iteration, query
order and wall-clock time all differ between machines or runs.

## Decision

- The sim runs on its own schedule (`SimTick`, 20 ticks/s today) and reads only `SimClock { tick }`, never
  `Time`, frame delta or a wall clock.
- All sim state is integers: `SimPos` in millimetres (`i32`), speeds in mm/tick, needs in milli-units
  `0..=100_000`, integer `isqrt` for distances.
- Ambiguity detection on the sim schedule is `Error`, and effects are resolved in sorted order.
- A platform-independent FNV-1a state hash over all sim components in `SimId` order is the oracle.
- The sim crates build and test without rendering.
- Enforcement: clippy `disallowed-types`/`disallowed-methods` (`HashMap`/`HashSet`, `Instant::now`,
  `SystemTime::now`, `Time`, thread RNG) and `#![deny(clippy::float_arithmetic)]` in the sim crates, plus
  determinism, schedule-shuffle and replay-corpus tests.

## Consequences

- Same seed + same commands = same hash, across runs and operating systems; replays and `diverge` work.
- Fractional math needs care (fixed point via the `fixed` crate only if trigonometry or steering needs it).
- Presentation uses floats freely but must never feed values back into the sim.
- Changing the hash encoding invalidates the replay corpus. That needs an equivalence proof and a
  user-approved re-baseline.
