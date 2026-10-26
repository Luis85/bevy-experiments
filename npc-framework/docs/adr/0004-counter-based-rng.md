# ADR 0004: Counter-based randomness

- Status: Accepted
- Date: 2026-10-23
- Source: villager spec §6.2, `crates/npc_kernel/src/rng.rs` (was `crates/npc_sim/src/rng.rs`)

## Context

With a stateful RNG, every draw depends on how many draws happened before it, so the result depends on
processing order, on which agents exist and on whether tooling draws numbers. That breaks determinism as
soon as systems or agents are reordered, added or removed.

## Decision

Randomness is a pure function: `sim_rand(world_seed, sim_id, tick, purpose) -> u64` (SplitMix64 over the
inputs), with `sim_range` for bounded integers. Each call site uses its own `purpose` constant. There is no
global or per-agent RNG state, and `rand::thread_rng`/`rand::rng` are banned in the sim crates.

## Consequences

- A draw depends only on who, when and why, so it is independent of processing order and of other agents.
- Replays need only the seed and the commands.
- Two call sites sharing a `purpose` on the same tick get the same number, so new sites need new constants.
