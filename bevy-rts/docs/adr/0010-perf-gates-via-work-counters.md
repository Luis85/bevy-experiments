---
status: "accepted"
date: 2026-10-23
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: workflow-and-verification.md §4.9; architecture-best-practices.md N21/C17/C18; agent-dev-environment.md #9; baseline-recommendations.md A12, B5"
---

# Perf gates via deterministic work counters; wall clock only at milestones

## Context and Problem Statement

Revision 1 (§5.7, §12) gated performance on criterion wall clock: a tick-time regression above 20% fails. The dev
machine is a mobile workstation whose thermal and power state vary run to run, so absolute wall-clock thresholds either
cry wolf or hide real regressions; agents will argue with noise. The scale target "3,000 units" was also ambiguous:
squads, crews, projectiles and wrecks multiply the real entity count.

## Decision Drivers

* A gate an agent can't argue with: exact and reproducible.
* It catches algorithmic regressions (an accidental O(n²)) at the task that introduces them.
* The G7 tick budget is still measured honestly, on real hardware.
* The benchmark reflects the real entity mix.
* No remote or Linux runner is available (B8 skipped).

## Considered Options

* A criterion wall-clock gate at 20% per change (revision 1)
* Three tiers: deterministic work counters as the gate, instruction counts for trends, wall clock at milestones
* Paired A/B wall clock per change (interleaved baseline/candidate)

## Decision Outcome

Chosen option: "Three tiers", with tier 2 parked until it has a home.

- **Scale target as an entity mix:** 3,000 units ≈ 8–12k sim entities plus projectiles in flight. The M0 scale scenario
  synthesises it: 3,000 wandering units, 6,000 followers (cross-entity reads), about 1,000 projectiles in flight
  (spawn/despawn churn through the funnel). The 500 and 5,000 sizes scale the same mix.
- **Tier 1, the gate:** the `Transient` `SimCounters` resource counts per-tick work: path nodes expanded, LOS cells
  visited, spatial-query candidates, entities sorted, entities spawned and despawned, and allocations (through a
  counting allocator in test binaries). Tests assert budgets from `docs/perf/budgets.ron` (protected) on the scale
  scenario and snapshot the totals with insta. It runs in `gate task` and `gate pr`.
- **Tier 2, trends:** instruction counts with `gungraun` (Linux/Valgrind) or CodSpeed. Neither runs on the Windows dev
  machine without a remote or WSL; the tier activates when the user provides one (open question).
- **Tier 3, milestones:** criterion and `fl_cli bench` on the dev machine, fixed power plan, baseline and candidate
  interleaved, median of 5, recorded in `docs/perf/M<n>.json`. The G7 budget (≤ 15 ms at 3,000 units, entity mix) is
  checked here. A regression above 20% against the previous milestone must be justified or fixed before the milestone
  closes.
- The per-system profiler (spec §15 D) remains a diagnostic; its time budgets in `debug.ron` warn, never fail.

### Consequences

* Good, because the gate is exact, fast and noise-free, and every mechanic must expose counters (definition of done).
* Good, because the benchmark measures the real mix, not one entity per unit.
* Bad, because counters measure algorithmic work, not constant factors or cache behaviour. Wall clock at milestones
  covers those, but only at milestone cadence.
* Bad, because a counter budget needs re-blessing when an algorithm legitimately does more work. That goes through
  `xtask bless` with a reason.

### Confirmation

* `docs/perf/budgets.ron` exists and is protected; `gate task` runs the counter tests on the scale scenario.
* The milestone gate requires a `docs/perf/M<n>.json` entry with median-of-5 numbers.

## Pros and Cons of the Options

### 20% wall-clock gate per change

* Good, because it measures what users feel.
* Bad, because it is noisy on a laptop and invites arguing with the gate.

### Three tiers

* Good, because each tier is used where it is reliable.
* Bad, because tier 2 needs Linux, which isn't available locally yet.

### Paired A/B wall clock per change

* Good, because it is more robust than absolute thresholds.
* Bad, because it doubles bench time per task and still flakes under thermal throttling. It is kept for milestone runs
  (interleaving).

## More Information

* Spec §2 G7, §5.7, §12.6, §15 D.
* Research: `workflow-and-verification.md` §4.9; `architecture-best-practices.md` N21, C17, C18;
  `agent-dev-environment.md` #9.
* Related: ADR-0009, ADR-0013.
