# Baseline hardening: portable framework and agent workflow (design)

Date: 2026-10-23 · Status: **draft for approval**
Inputs: `docs/research/framework-best-practices.md` (FBP), `docs/research/agent-driven-gamedev.md` (ADG), and the user's requirements:
- a solid baseline to iterate on;
- a design → implementation → test → verification workflow;
- easy to extend without compromising the framework;
- **portable to other Bevy projects**, e.g. sophisticated units for the user's own large-scale RTS.

## 1. Intent

Turn the iteration-1 code into a **library-grade framework plus example content**. A different Bevy game must be able to add `npc_framework`, register its own unit types, and plug in its own movement, physics, schedule and tick rate, without forking or editing core crates. At the same time, the repo must become an environment where AI agents can safely design, build, verify and extend the game. The rules that keep the framework sound (determinism, crate boundaries, protected goldens) are enforced by tools, not by hoping agents remember them.

## 2. Goals

1. **Crate split** (FBP §0.2):
   - Generic crates hold no game content: `npc_kernel`, `npc_ai`, `npc_needs`, `npc_jobs`, `npc_locomotion`, `npc_perf`, `npc_tools`, and the facade `npc_framework` (`NpcPlugins`, `prelude`, feature flags).
   - The villager game moves to `content/village_content` and `apps/village`.
2. **Host-friendly plugin** (FBP §0.3):
   - `NpcPlugins::new().tick_hz(n).run_in(Schedule)` with its own accumulator. It never touches the global `Time<Fixed>`.
   - No key bindings in the library.
   - Labels are prefixed (`NpcSimTick`, `NpcSet::*`) and public sets are the ordering seams.
   - Determinism mode is `Strict` (default) or `Relaxed`.
3. **Open/closed extension:**
   - A registration API keyed by **stable names**: `register_action/cond/scorer/need/sim_component/command`. Ids are frozen sorted by name, and replays and trees store names.
   - Typed leaf params.
   - A `SimHash` registry replaces the hard-coded list in `hash.rs`.
   - A content command extension, so `SpawnVillager` lives in content.
4. **Locomotion seam:** actions write `MoveIntent` and read `MoveStatus`. `npc_locomotion` ships a deterministic straight-line mover; hosts may swap in Avian, a navmesh or flow fields.
5. **Portability proof:** `apps/rts_units_demo` depends only on `npc_framework` and has no village content. It runs about 200 units with a different tree, a host-side mover, `GoTo`/`AttackMove`-style orders and its own tick rate, in `FixedUpdate`, next to a host system.
6. **Mechanical guardrails:**
   - `clippy.toml` bans `HashMap`/`HashSet`, `Instant`/`SystemTime`, `Time` and thread RNG in sim crates, and adds `float_arithmetic` checks on kernel crates.
   - A `cargo metadata` fitness test fails if a core crate depends on content, rendering or `npc_view`.
   - `deny.toml`.
   - A `shuffle_seed` determinism test.
   - proptest-state-machine invariants over random `SimCommand` sequences.
   - insta goldens for journal, inspect and `npcctl run`.
   - A committed replay corpus that must verify.
7. **Agent workflow** (ADG):
   - **Docs:** a short `CLAUDE.md` of at most 120 lines and path-scoped `.claude/rules/` for sim and view.
   - **`scripts/verify`:** a single JSON verdict covering fmt, clippy, nextest, replays, goldens and an optional visual check. A Claude Code **Stop hook** blocks finishing a turn while it fails.
   - **PreToolUse hook:** protects goldens, snapshots, baselines and lint config unless a re-baseline is explicitly approved.
   - **Project skills:** `/add-mechanic`, `/add-unit-type`, `/verify-visual`, `/debug-desync`, `/why`.
   - **Records:** ADRs 0001–0008 for decisions already made, plus a CHANGELOG and a Bevy↔framework version table.
8. **CI** (GitHub Actions file in-repo; runs if a remote is configured): fmt, clippy, nextest, cargo-deny, `cargo hack --each-feature` on the facade, and a Windows + Linux determinism job that compares replay hashes.

## 3. Non-goals (this round)
- Data-driven RON trees, needs and defs with patching (FBP P1). The registration API keeps names stable so RON can follow without breaking anything.
- Job board, squads, spatial grid and pathfinding: next iteration, groups.
- Multiplayer, save/load, hot patching (blocked on Windows), and an MCP server. Evaluate `bevy_brp_mcp` after our own BRP methods land.

## 4. Anti-goals
- **Hidden dependencies:** a core crate that names a villager type, imports `npc_view`, or hard-codes content in hash, commands or the view.
- **Clock hijacking:** a library plugin that changes global time, input bindings or resources the host owns.
- **Agents "fixing" tests by editing them:** goldens may only change through an explicit re-baseline.
- **Behaviour regressions:** the village demo must keep its rhythm, its determinism tests and its tooling (journal, replay, BRP, profiler) through the split. The replay corpus and goldens prove it.

## 5. Sequencing (decided)
1. **Finish the in-flight tooling waves** (journal, commands and replays → BRP + `npcctl`) on the current layout.
2. **Guardrails first, before moving code:** lints, fitness test, goldens and replay corpus, verify script plus hooks, `CLAUDE.md`. The split is then checked by the same goldens.
3. **Crate split, registration API, hash registry, command extension, driver rework.** These are sequential and touch the same files.
4. **Locomotion seam, then `rts_units_demo`.**
5. **CI, ADRs, skills, docs.**
6. **Verification:**
   - A fresh agent with only `CLAUDE.md` adds a new mechanic (e.g. a "thirst" need with a well smart object) to the village, and a new unit type to the RTS demo, without touching core crates.
   - Its friction notes become doc and skill fixes.

## 6. Calls made by the designer (flag if wrong)
- Crate names and boundaries as in FBP §0.2.
- `Strict` determinism is the default; `Relaxed` is opt-in.
- Goldens are protected by a hook, not just by convention.
- RON content waits until after this round. If you want unit types as data files now, we pull it in, which adds roughly one more agent wave.
- CI is written as GitHub Actions, the most likely host.
