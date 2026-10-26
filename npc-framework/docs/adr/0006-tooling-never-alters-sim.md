# ADR 0006: Debug tooling never alters simulation results

- Status: Accepted
- Date: 2026-10-23
- Source: `docs/superpowers/specs/2026-10-23-debug-perf-tooling-design.md` §2–3

## Context

Humans and AI agents debug through a journal, inspect snapshots, replays, a BRP remote interface and a
profiler. If any of these changed what the sim computes, a bug could vanish when observed, and a replay
recorded with tooling on would not verify with it off.

## Decision

- Journal, inspect, replay and command code are always compiled into the sim crates but record nothing
  until enabled at runtime. Journal content is deterministic (`(tick, seq)` is a total order).
- `SimCommand` is the only way external input changes the sim. BRP `npc/command` goes through the same
  queue, so it is recorded for replays. Bevy's built-in `world.mutate_*` on sim entities is unsupported.
- Profiler data lives in non-hashed resources, timers never feed sim decisions, and budget breaches are
  never journaled.
- The remote interface and the profiler are behind the `debug-tools` feature, so release builds do not pay
  for them.
- A test asserts the same state hash with the journal and profiler on vs off.

## Consequences

- Observing is safe: any run can be journaled, inspected or profiled and still match its replay.
- All tool output is plain JSON or JSON Lines with documented shapes, shared by the CLI, BRP and the panels.
- Some conveniences are off-limits, such as editing sim state live through `world.mutate_*`.
