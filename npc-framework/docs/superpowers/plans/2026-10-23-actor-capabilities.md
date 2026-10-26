# Actor Capabilities Implementation Plan (waves C–H)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add opt-in actor capabilities and one Actor Testbed with all of them enabled. The capabilities are perception, interaction, RTS command, vehicles, rules, progression, crafting and pathfinding.

**Architecture:**
- Each capability is its own crate and plugin, built on `npc_framework` (`NpcAppExt` registration by name, `NpcSet` seams, journal, hash registry).
- **Core and content:** core crates stay content-free. Each crate's built-in verbs, skills and recipes live in its test or testbed content; generic defaults are allowed only where the spec names them as built-ins.

**Tech Stack:** Rust 1.99, Bevy 0.20, our kernel. No new runtime dependencies without coordinator approval.

**Spec:** `docs/superpowers/specs/2026-10-23-actor-capabilities-design.md`, which is approved and includes `npc_nav`.
- **Assets:** `docs/research/testbed-assets.md`.
- **Base:** `CLAUDE.md`, `docs/adr/`, and the baseline-hardening spec and plan.

## Global Constraints

- **The CLAUDE.md golden rules apply:**
  - determinism (integers only, SimId order, counter RNG);
  - the core/content boundary;
  - no host hijack;
  - protected oracles;
  - `cargo xtask verify` ok as the gate.
- **Every capability is opt-in.** With no new plugin added, the village behaves byte-identically: the corpus and goldens must pass unchanged.
- **Behaviour changes need a staged re-baseline.** Any intended change to village behaviour needs one, following the P3 method: equivalence or diff evidence in `docs/_notes/`, new oracles staged in `tests/replays.next/` or as `.snap.new`, then a stop and report. Only the coordinator swaps them in, after user approval.
- **Each new crate gets:**
  - an `arch_fitness` rule (headless, no render);
  - a `clippy.toml` copied verbatim from `npc_kernel`;
  - perf mechanic tags with a budget;
  - journal kinds for its events, added to the kernel's journal kind names list;
  - inspect output through `set_inspector` or a component registry;
  - debug gizmos in `npc_view_helpers` behind a feature, if visual.
- **Do not `git commit`.** Only edit files your task owns. Concurrent agents share the target dir.

## Review Focus

1. **Pathfinding under budget:** many simultaneous requests are served deterministically in SimId order across ticks (`nav_budget_carries_over_deterministically`, C1).
2. **Unreachable goals and blocked starts:** these produce `PathFailed{reason}`, not hangs or panics (`nav_unreachable_fails_cleanly`, C1).
3. **Dynamic blocking:** a cell on the current path becomes blocked and the unit replans (`nav_replans_when_blocked`, C1).
4. **Stale claim right after a meal:** fixed with a re-baseline (`eat_releases_claim_on_completion`, C2).
5. **Profiler noise:**
   - no budget checks before the minimum sample count;
   - dev-build budget scaling.

   Test: `budgets_wait_for_min_samples` (C2).

---

## Wave C

### Task C1: spatial grid, locomotion seam, `npc_nav` (one agent)

**Files:**
- Kernel: `crates/npc_kernel/src/spatial.rs` for the grid. It is generic and needed by perception, nav and interaction.
- New crates: `crates/npc_locomotion/`, `crates/npc_nav/`. Re-export them from the facade (features `locomotion` and `nav`; `locomotion` is in the default set).
- Village: the village content's move action.
- Tests: the new crates' tests, and `content/village_content/tests` (village regression only).

**Interfaces (fixed names):**
- **Kernel:**
  - `SpatialGrid` resource:
    - `cell_mm: i32` (default 1000);
    - bounds;
    - per-cell `CellFlags { blocking, occluding }` plus an extra cost (`u16`);
    - an index of `SimId`s per cell, kept up to date from `SimPos` in `NpcSet::Inputs`, after commands;
    - queries `in_radius(pos, r) -> Vec<SimId>` (sorted) and `in_box`;
    - `cell_of(pos)` and `center(cell)`.
  - `GridBlocker` and `GridOccluder` components on sim entities, which mark their cell(s) via a footprint.
  - `SpatialPlugin`, part of `NpcPlugins`. It is cheap; with no blockers it is a no-op. Register its resource hash only if non-default, to keep the village hash identical.
- **`npc_locomotion`:**
  - `MoveIntent { goal: SimPos, speed_mm_per_tick: i32, tolerance_mm: i32 }` (SparseSet).
  - `MoveStatus { Idle, Moving, Arrived, Blocked { ticks: u32 } }`.
  - `NpcSet::Locomotion` runs after Actions and before Effects. Its position in the schedule is documented.
  - `StraightLineMover` plugin: today's behaviour, moved out of village actions.
- **Village move action:** `MoveToDest` in village_content now writes `MoveIntent` and waits on `MoveStatus` with `StraightLineMover`. **The village must stay byte-identical.** Prove it with the P3 capture method. The hash will change if `MoveIntent` is a registered component; if that is unavoidable, stage a re-baseline with journal equivalence.
- **`npc_nav`:**
  - `GridPathMover` plugin, which replaces `StraightLineMover` per agent via `NavAgent` (opt-in per unit).
  - `PathRequest` queue: A\* with integer costs 10/14, no corner cutting, ties broken by `(f, h, cell index)`, per-tick node budget `NavBudget(default 4000)`, served in SimId order with carry-over.
  - LOS string-pulling smoothing; the result is a `Path { waypoints: Vec<SimPos> }` component.
  - Replanning on a `GridChanged` cell event affecting the path, or after `Blocked{ticks}` of 10 or more.
  - Door hook, consumed by interaction later: a registered `PassCost` fn per cell flag. Closed doors get cost plus an "open" waypoint marker, which wave E consumes.
  - Journal entries `PathFound{len, nodes}` and `PathFailed{reason}`; perf mechanic `nav.astar`.
  - Path gizmo in view_helpers.

- [ ] **Step 1: write the tests first.**
  - Grid queries are sorted and correct.
  - Kernel spatial index after movement and despawn.
  - A\*: open field (path equals straight line after smoothing); wall with a gap; maze; unreachable goal → `PathFailed`; start inside a blocker → `PathFailed{StartBlocked}`.
  - Budget carry-over with 50 agents is deterministic; a hash is equal across two runs.
  - Replans on a newly blocked cell.
  - Diagonal corner-cutting prevented.
  - A `village_nav` scenario test: villagers with `NavAgent` around a wall (new scenario; the existing ones are untouched).
- [ ] **Step 2: implement.** Keep the village byte-identical.
- [ ] **Step 3: run `cargo xtask verify`.** It must be ok without touching any oracle.

### Task C2: follow-ups, eat-claim fix and zstd (one agent, parallel with C1)

**Files:** `crates/npc_perf`, `crates/npc_tools`, `crates/npc_view_helpers`, the journal module in `crates/npc_kernel` only, the eat code in village content only (C1 owns the move code), `content/village_view`/`apps/village` Cargo features, and `deny.toml`.

- **Profiler budgets:**
  - Only check a mechanic's budget once it has `min_samples` (default 60).
  - `PerfBudgets::scaled_for_profile()` multiplies budgets by 10 when `cfg(debug_assertions)`.
  - The game uses the scaled budgets.
  - Test: `budgets_wait_for_min_samples`.
- **F4 panel:** cache the report. Recompute it at most every 0.5 s, and `report()` sorting must cost under 1 ms on 300 systems; add a micro test or bench.
- **Remote ports:**
  - Probe the BRP port before binding and log a clear error with the port on conflict.
  - `--remote-port` also sets the render-app port (port + 1), or disables it. Verify the bevy_remote 0.20 API first.
- **Journal:**
  - `JournalKind::NAMES` constant, and npcctl uses it.
  - Journal-before-spawn option: `new_sim_app` (or the npc_tools equivalent) can enable the journal before scenario spawn, and npcctl `journal`/`inspect` use it.
  - This changes CLI journal output: new Spawn lines. Goldens that capture the first 200 journal entries via the API must not change unless the API default changes. Keep the API default off, and make the npcctl flag default on only for the `journal` command. If any golden changes, stage it.
- **Recording:** unify npcctl's Recorder and `replay::record` into one npc_tools recorder used by CLI, BRP and village. Behaviour is identical and the corpus is unchanged.
- **Command validation:** a `submit_deferred` (validated at apply time only) for command files, removing the same-tick spawn-then-target limitation. npcctl uses it. Document it.
- **Eat-claim fix:** release the stockpile claim in `eat_system` on completion, in the same tick as `TakeFood`. This is a behaviour change, so:
  - capture pre/post journals and summaries;
  - write the diff summary to `docs/_notes/c2-eat-claim-evidence.md` (which entries change, and why only those);
  - stage the corpus and goldens;
  - stop and report.
  - Test: `eat_releases_claim_on_completion`.
  - The property-test exception for the stale claim is removed, so no tolerance is left.
- **zstd (user decision):** switch the Bevy zstd feature to `zstd_c` in `village_view`/`village`, check the full feature graph so `ruzstd` is gone (`cargo tree -i ruzstd`), and remove the RUSTSEC-2026-0344 ignore from `deny.toml`. `cargo deny check` must pass without the ignore.

## Wave D (parallel): D1 `npc_perception`, D2 `npc_rules`, D3 `npc_agent_io`

- **D1 and D2:** per spec §4.2 and §4.6.
- **D3:** per the approved `docs/superpowers/specs/2026-10-23-agent-control-design.md`: sessions and scenarios, attach/observe/act/events (real time), the provider registries, text rendering, village affordances, the CLI client, the MCP server and `greedy_controller`.
- **Integration with later waves:** every later capability task must register its observation sections and affordances with `npc_agent_io`.
- **Details:** decided when the wave starts and appended here.

## Wave E: `npc_interaction` (items, inventory, doors, tools, verbs)

Per spec §4.3. It consumes the grid door hook from C1 and skill checks from D2.

## Wave F (parallel): F1 `npc_command`, F2 `npc_vehicles`, F3 `npc_progression` + `npc_crafting`

Per spec §4.4, §4.5, §4.7 and §4.8.

## Wave G: Actor Testbed and RTS units demo

Per spec §4.9:
- assets per `testbed-assets.md`;
- a scenario test and replay per feature;
- screenshot checks;
- `apps/rts_units_demo` on `npc_command`.

## Wave H: CI, docs and the fresh-agent extension test

- CI with a Windows and Linux determinism job.
- `docs/debugging.md` and the "Using npc-framework" guide covering every plugin.
- The fresh-agent tests from the baseline-hardening plan's Phase 6, extended with "add a new interaction verb" and "add a recipe".
