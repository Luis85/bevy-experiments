# Planner report: M0 revision 2 plan

Plan: `docs/superpowers/plans/2026-10-23-frontline-m0-rev2.md` (2,280 lines, **30 tasks**, `### Task N:` headings).

## Task count and mapping
- **Phase I, infrastructure (1–4):**
  - 1: build-dir probe, repo Cargo config, manifests
  - 2: xtask gate, arch-check, gen-clippy, deny, nextest
  - 3: OWNERS.toml, check-scope, test-guard, bless
  - 4: CLAUDE.md, rules, hooks, agents, skill, `.mcp.json`
- **Phase II, contract rework (5–13):**
  - 5 `fl_math`, 6 `fl_protocol`, 7 `fl_view`, 8 `fl_content`
  - 9 `fl_sim` feature contract, registries and classification; 10 phases, `step_tick`, RNG and hash
  - 11: T7 rework (envelope, spawn funnel, debug handlers)
  - 12: runner and drivers, including loopback Lockstep; snapshot publishing
  - 13: save/load
- **Phase III, revised rev-1 tasks:** 14 (T8), 15 (T9), 16 (T10), 17 (T11), 18 (T18), 19 (T19), 20 (T13), 21 (T14), 22 (T15), 23 (T16), 24 (T20), 25 (T21).
- **Probes, snapshots and perf:** 26 (`dynamic_linking` probe), 27 (`bevy_remote` probe), 28 (contract snapshots: public-api, serde-reflection, system order), 29 (work-counter perf gates, entity-mix scale scenario, budgets).
- **Phase IV:** 30 (integration: demo, determinism, perf baseline, render stress, `docs/debugging.md`).

## Topology summary (≤ 2 builders)
- Lane 1 is `C:\fl\a` (sequential). Lane 2 is one `C:\fl\tNN` at a time.
- **Phase I.** Lane 1 runs 1→2→3→4. Lane 2 runs 26 after Task 1.
- **Phase II.** Lane 1 runs 5→6→7→9→10→11→12→13; Task 9 waits for Task 8 to merge. Lane 2 runs 27 → the T12 rebase, gate and merge → 8 (after 6) → 20 (after 7 and 8) → 21.
- **Phase III.** Lane 1 runs 14→15→17→18→29→28. Lane 2 runs 19→16→22→23→24→25.
- **Phase IV.** Task 30 in lane 1.
- **Merges.** `main` is fast-forwarded after Tasks 4, 13 and 30, each with `gate pr` green. Every parallel pair's file ownership is tabled in the plan.

## Spec gaps found (for the architect/user)
1. **Build-dir location.** Spec §3, §13.3 and ADR-0013 put `build.build-dir` in the *user* Cargo config, but ruling R19 puts it in the repo config. The spec and ADR need a clarification edit.
2. **Runner thread.** §5.4 and §12.2 ban `thread::spawn` in sim crates, yet §5.1 and §5.3 put `SimRunner`, which spawns the sim thread, in `fl_sim`. The plan allows that one spawn behind a scoped `#[expect]`.
3. **Phase timing vs. no clock.** §15 D wants per-phase timing from "marker systems", while §5.4 bans clocks in the sim. The plan's markers emit trace events, and `fl_observe` timestamps them.
4. **Early commands.** §5.5 doesn't say what happens to a command that arrives before its `exec_tick`. The plan has drivers deliver at exactly `exec_tick`, and the sim rejects `Early`. Pending commands live in the driver and enter saves through `SimQuery::Save { pending }`.
5. **Dense IDs in payloads.** The dense IDs in command payloads (e.g. `UnitTypeId` in `debug.spawn`, as the architect report has it) can't be remapped through string tables. The plan's payloads carry content names instead.
6. **Counting allocator.** §12.6's counting allocator conflicts with the workspace's `unsafe_code = "forbid"`. The plan uses the `stats_alloc` crate through a safe `#[global_allocator]` declaration.
7. **Test count.** §12.5's "falling `nextest list` count" would need a build of the merge base. The plan uses a textual proxy (net removed tests).
8. **Primitive float ban.** It is unverified whether clippy's `disallowed-types` can ban the primitives `f32`/`f64` (§12.2). Task 2 checks it, with `float_arithmetic` as the fallback.
9. **Hook role detection.** §12.5 and §13.3 hooks assume hook input identifies the subagent role, which is unverified. The plan registers the guards both in `settings.json` (auto role) and in the implementer agents' frontmatter.
10. **First protected artifacts.** §12.5 doesn't say how protected artifacts are first created. The plan's convention: tests write `*.new` files, and only `cargo xtask bless` promotes them.
11. **Runtime loads.** `fl.load_scenario` and `fl.load` (§15 C) need a sim rebuild that `fl_client` can't perform, because it has no `fl_sim` dependency. The plan has `fl_game` insert a `ScenarioLoader` callback.
12. **Typed codecs.** The `Class::Hashed { version: 1 }` example in §5.8 and ADR-0006 can't carry typed codecs. The plan uses `Class::hashed(1)` returning `Class<T>`.
13. **`WorktreeRemove` hook.** It needs each worktree's build-dir path, and whether `cargo metadata` reports `build_directory` is unverified. Task 1 probes it.

## Assumptions
- **Preconditions:**
  - T7 is committed and reviewed on `m0/phase-a` before Task 1, because Phase I shares `C:\fl\a`.
  - T12 is rebased and gated after Task 4. That is also the first build in a second worktree, so it verifies sccache reuse.
- **Dependencies:**
  - Bevy features for the client crates are Bevy's `default` list, written out, minus `bevy_audio`/`vorbis` (M2 adds them).
  - All new workspace dependencies are added in Task 1, so later tasks never touch `Cargo.toml`.
  - `crossbeam-channel` is the runner channel, because its receivers are `Sync`, which Bevy resources need.
  - Exact crate versions are resolved at implementation time; Bevy and `bevy_brp_extras` stay pinned.
- **Determinism contracts:**
  - The `stream_key` and `roll` definitions, and their reference vectors, are new contracts. They were computed with an independent Python oracle and pinned in Task 5.
  - Command `seq` starts at 0 per player. The lockstep loopback test uses input delay 2.
- **Gate conventions:**
  - Test-name prefixes `smoke_`, `gpu_`, `milestone_` and `brp_` select the gate subsets.
  - `FL_SHUFFLE_SEEDS` is 1 at task level and 3 at pr level.
- **Scale scenario:** its defaults at 3,000 units are 2 followers per unit, 1,000 projectiles with a 20-tick time to live, and a sensor query every 10 ticks, giving about 10k entities.
- **User actions:** the user confirms the fixed power plan before the tier-3 bench in Task 30.
- **Deferred to M1:** hot reload, including `ContentDb::same_id_sets`.
- **Deferred to later:** D1 and D2, as the user decided; the roadmap says to revisit them at the net-crate milestone at the latest.
