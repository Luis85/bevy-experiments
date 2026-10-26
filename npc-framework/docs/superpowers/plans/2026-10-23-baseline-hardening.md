# Baseline Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the repo into a portable, library-grade NPC framework with example content, and an environment where AI agents can safely extend it.

**Architecture:**
- Generic crates: kernel, ai, needs, jobs, locomotion, perf and tools, behind a facade `npc_framework` (`NpcPlugins`, `prelude`).
- Content lives in `content/village_content` and the apps.
- Guardrails (lints, fitness tests, goldens, replay corpus, hooks) land **before** the split, so the split is proven against them.

**Tech Stack:** Rust 1.99, Bevy 0.20, proptest + proptest-state-machine, insta, cargo-nextest, cargo-deny, cargo-hack.

**Spec:** `docs/superpowers/specs/2026-10-23-baseline-hardening-design.md`. Research: `docs/research/framework-best-practices.md` (FBP) and `docs/research/agent-driven-gamedev.md` (ADG).

## Global Constraints

- Village behaviour must not regress:
  - every existing test stays green (tests may move with their code, but their assertions don't change);
  - the `print_three_day_timeline` SUMMARY lines stay identical;
  - the replay corpus verifies.
  - The state hash *value* may change only in Phase 3, and only with a regenerated corpus whose old→new mapping is checked by an equivalence test (see Task P3).
- Core crates (`npc_kernel`, `npc_ai`, `npc_needs`, `npc_jobs`, `npc_locomotion`, `npc_perf`, `npc_tools`, `npc_framework`) must not name villager content or depend on rendering crates or `content/*`.
- **The library never claims host globals:** no writes to `Time<Fixed>`, no input bindings, no `insert_resource` over host-owned types. Public labels are prefixed `Npc*`.
- Determinism rules from the base spec still hold. `ambiguity_detection = Error` applies only to our sim schedule.
- **Do not edit goldens, snapshots, baselines or lint config to make a test pass.** Re-baselining needs explicit coordinator approval.
- Do not `git commit`. The coordinator commits per phase.
- Final gate for every task: `cargo xtask verify` (once it exists) reports `ok`. Before that, the gate is `cargo test --workspace` plus `cargo clippy --workspace --all-targets -- -D warnings`.

## Review Focus

1. **A host app with its own `FixedUpdate` physics at 64 Hz plus `NpcPlugins` at 20 Hz.** The host's timestep must stay unchanged. Pinned by `host_fixed_timestep_untouched` (Task P3).
2. **Plugin order and content registration order.** Swapping the order must not change ids or replay hashes. Pinned by `registration_order_independent` (Task P3).
3. **Content leaking into core.** Pinned by the `arch_fitness` tests (Task P1).
4. **Agents bypassing guardrails** by editing goldens or lint config. Pinned by `hook_blocks_golden_edit` (Task P2).
5. **RTS demo units without the village content.** The demo must build with only `npc_framework`. Pinned by `rts_demo_uses_only_facade` (Task P4).

---

### Phase 2 (wave A, parallel; starts after the tooling waves)

#### Task P1: Lints, fitness tests, deny

**Files:**
- Create: `crates/npc_sim/clippy.toml`, `crates/npc_ai/clippy.toml`, `deny.toml`, `xtask/`, and the workspace member `xtask` with `cargo xtask` alias via `.cargo/config.toml`.
- Create: `xtask/tests/arch_fitness.rs`.

**Requirements:**
- `clippy.toml` in the sim crates:
  - `disallowed-types`: `std::collections::HashMap`, `std::collections::HashSet`, `bevy::platform::collections::HashMap`, `bevy::platform::collections::HashSet`.
  - `disallowed-methods`: `std::time::Instant::now`, `std::time::SystemTime::now`, `rand::thread_rng`, `rand::rng`.
  - `disallowed-types` also covers `bevy_time::Time`.
  - Each entry has a `reason` that points to the spec.
- Crate attribute `#![deny(clippy::float_arithmetic)]` in `npc_ai` and `npc_sim`.
  - Exemptions are allowed only with `#[allow(...)]` and a `// determinism: <why>` comment.
  - Today the only expected exemption is the perf/tracing span code, if any.
- `arch_fitness.rs` uses `cargo_metadata`. It asserts:
  - `npc_ai` and `npc_sim` do not depend transitively on `bevy_render`, `bevy_pbr`, `wgpu`, `npc_view`, or any `content/*` crate;
  - `npc_perf` has no bevy render dependency.
  - The crate-name lists come from a table at the top of the test file, which Phase 3 extends.
- `deny.toml`:
  - licenses allow-list: MIT, Apache-2.0, BSD, Zlib, ISC, Unicode, CC0-1.0, MPL-2.0;
  - `bans` with `deny` entries for wrappers mirroring the arch rules;
  - `advisories`.
- `cargo xtask verify` runs, in order: `cargo fmt --check`, clippy `-D warnings`, `cargo nextest run --workspace` (falls back to `cargo test` if nextest is missing), arch_fitness, replay corpus verify (P2), insta goldens (P2), and `cargo deny check` if installed.
  - It prints one JSON object: `{ ok, steps:[{name, ok, ms, summary, log_tail}] }`.
  - `--quick` skips deny and the long tests.

- [ ] **Step 1:** Write `arch_fitness` tests. Add a deliberately bad dependency in a scratch copy to prove the test catches it; never commit that.
- [ ] **Step 2:** Add the lints. Fix every violation they reveal in the sim crates. Report each fix; a real determinism bug is a finding.
- [ ] **Step 3:** Pass: `cargo xtask verify` gives `ok:true`.

#### Task P2: Goldens, replay corpus, properties, shuffle test

**Files:**
- `tests/golden/` (insta snapshots, owned by an `xtask` test or `npc_sim` dev-tests).
- `tests/replays/*.json` (corpus).
- `crates/npc_sim/tests/{properties.rs, shuffle.rs, corpus.rs}`.

**Requirements:**
- **Corpus:** record replays with `npcctl replay record`:
  - `village_seed1_3days` (no commands);
  - `village_orders` (GoTo, SetNeed and SetFood commands);
  - `village_spawn_despawn`;
  - `village_n_25_1day`.
  - `corpus.rs` verifies every file in `tests/replays/`.
- **insta goldens** (YAML or JSON):
  - the `npcctl run --seed 1 --ticks 7200` summary;
  - `inspect` of unit 5 at ticks 600, 1800 and 4000;
  - the first 200 journal entries of seed 1.
  - CI uses `INSTA_UPDATE=no`.
- **`properties.rs`** uses proptest-state-machine. Random sequences of valid `SimCommand`s run interleaved with random tick batches; the invariants are checked every tick:
  - needs are in `0..=NEED_MAX`;
  - food is `<= capacity`;
  - every claimant and target in `Reservations` is alive in `SimIndex`;
  - beds and work sites have `count <= 1`;
  - claims on a stockpile are `<= food`;
  - `ActiveLeaf` exists iff the running path is non-empty;
  - a replay of the generated sequence gives the same final hash.
  - Default 64 cases, with `PROPTEST_CASES` respected.
- **`shuffle.rs`:** with `ScheduleBuildSettings::shuffle_seed` (bevy_ecs feature `debug`), 8 different shuffle seeds over 5 000 ticks give the same state hash as an unshuffled run.

- [ ] Steps: write the tests, run them, and fix every real bug found. Report each bug.

#### Task P2b: Agent workflow files (parallel with P1/P2)

**Files:**
- `CLAUDE.md` (≤120 lines);
- `.claude/rules/{sim.md,view.md,content.md}` (path-scoped);
- `.claude/settings.json` (hooks);
- `.claude/hooks/{protect_goldens.*,verify_on_stop.*}`;
- `.claude/skills/{add-mechanic,add-unit-type,verify-visual,debug-desync,why}/SKILL.md`;
- `docs/adr/0001…0008-*.md`;
- `CHANGELOG.md`.

**Requirements:**
- **`CLAUDE.md`:**
  - what the repo is;
  - the crate map;
  - the five golden rules: determinism, core/content boundary, no clock hijack, goldens protected, run verify;
  - the commands: `cargo xtask verify`, `npcctl …`, `village --screenshot`, BRP curl;
  - links to `docs/debugging.md` and the specs.
- **Hooks** must work on Windows with Git Bash and on Linux, so they are POSIX sh scripts. Configure them in `.claude/settings.json` at `npc-framework/`, for sessions opened in this folder.
  - `PreToolUse` on Edit/Write: block paths under `tests/golden/`, `tests/replays/`, `bench/baseline.json`, `**/clippy.toml`, `deny.toml` and `xtask/tests/arch_fitness.rs` unless the file `.claude/rebaseline-approved` exists. Exit 2 with a message telling the agent to ask the user.
  - `Stop`: run `cargo xtask verify --quick`. If `ok:false`, block with the failing step summaries. Skip when no `.rs` or `.toml` file changed since the last verify, judged by mtime against a stamp file.
  - Test `hook_blocks_golden_edit`: shell-level tests in `xtask/tests/hooks.rs` pipe sample hook JSON into the scripts and assert the exit codes.
- **Skills:** each is short and procedural, with the exact commands and a checklist ending in `cargo xtask verify`.
  - `/add-mechanic`: register via the API (after P3), write a scenario test first, add a journal kind if needed, add a mechanic tag and budget, update the docs.
  - `/add-unit-type`: compose needs, tree and actions in content, without touching core.
  - `/verify-visual`: screenshot at a fixed tick, Read the PNG, check the checklist.
  - `/debug-desync`: `npcctl diverge`, read the entity diff, bisect.
  - `/why`: journal filter plus inspect for a unit and tick range, ending in a one-paragraph explanation.
- **ADRs** 0001–0008 record decisions already made:
  - our own BT runtime;
  - integer deterministic sim;
  - SimId ordering;
  - counter RNG;
  - actions as components;
  - tooling never alters sim;
  - core/content split and facade;
  - host-owned schedule and movement.

---

### Phase 3 (wave B, sequential, one agent): crate split, registries, driver

#### Task P3: Split and stable-name registration

**Target layout** (FBP §0.2, adapted):
- `crates/npc_kernel`: ids, clock (`SimClock`, configurable `SimRate`), rng, fixed math, `NpcSimTick`/`NpcSet`, command bus, journal core, replay core, hash registry, inspect core, driver.
- `crates/npc_ai`: unchanged plus name registration.
- `crates/npc_needs`: data-driven `NeedDef`s, hysteresis and decay.
- `crates/npc_jobs`: smart objects (`Provides`, `Approach`), `Reservations` and generic reserve helpers.
- `crates/npc_locomotion`: Task P4.
- `crates/npc_tools`: `npcctl` library parts, BRP method impls, perf glue. `npc_remote` merges here behind feature `remote`.
- `crates/npc_framework`: facade with `NpcPlugins` (a PluginGroup), `prelude`, and feature flags `tools`, `remote`, `perf`, `view-helpers`.
- `crates/npc_view_helpers`: driver UI (optional), `VisualOf`, interpolation, generic overlay, journal and perf panels.
- `content/village_content`:
  - villager needs (hunger, energy), places, actions, conditions, tree, scenarios and the `SpawnVillager` command;
  - village visuals, animations, scene and overlay specifics (as a `village_view` module or crate).
- `apps/village` and `apps/npcctl` consume only `npc_framework` plus `village_content`.

**Interfaces (fixed):**
```rust
pub trait NpcAppExt {
    fn register_action(&mut self, name: &'static str, start: StartFn, stop: StopFn) -> &mut Self;
    fn register_cond(&mut self, name: &'static str, f: CondFn) -> &mut Self;
    fn register_scorer(&mut self, name: &'static str, f: ScoreFn) -> &mut Self;
    fn register_tree(&mut self, name: &'static str, build: fn(&mut TreeBuilder)) -> &mut Self;
    fn register_need(&mut self, def: NeedDef) -> &mut Self;
    fn register_sim_component<T: Component + SimHash + Serialize + Reflect + GetTypeRegistration>(&mut self) -> &mut Self;
    fn register_command<C: ContentCommand>(&mut self) -> &mut Self; // dispatch by C::NAME
}
```
- **Ids:** they are frozen once in `Plugin::finish` of the kernel, sorted by name. `TreeBuilder` leaves reference actions and conditions **by name**, resolved at freeze time; an unknown name panics with a list of known names.
  - Typed params: `ParamValue { None, Int(i32), Pos(i32,i32), Name(String) }`.
  - Trees, the journal and replays store names.
  - The replay header carries a `content_hash` over all registered names plus tree content hashes. `verify` errors on a mismatch.
- **`SimHash` trait:** `fn sim_hash(&self, h: &mut Fnv64)`. `state_hash` iterates registered components in name order for each SimId. Inspect and the BRP JSON use the same registry.
- **`SimCommand`:** core variants are `Despawn`, `SetNeed{need: String}`, `Order` and `Content { kind: String, payload: serde_json::Value }`.
  - `ContentCommand: DeserializeOwned + Serialize { const NAME: &str; fn validate(&self, &World) -> Result<(), CommandError>; fn apply(self, &mut World); }`.
  - `SpawnVillager` and `SetFood` move to content. The JSON shapes accepted by the CLI and BRP stay backward compatible through the `kind` tag.
- **Driver:**
  - `NpcPlugins::new().tick_hz(20).run_in(FixedUpdate)` (or any `ScheduleLabel`).
  - The kernel keeps its own accumulator `SimTime` and runs `NpcSimTick` from one exclusive system in the chosen schedule. It never writes `Time<Fixed>`.
  - The pause/speed resource `SimControl` is in the kernel. Keyboard bindings move to `apps/village`.
  - `DeterminismMode::{Strict, Relaxed}` is a resource. Strict keeps ambiguity Error plus hash checks; Relaxed allows host-provided positions via an adapter (documented, tested with a tiny host-mover test).

- [ ] **Step 1:** Add tests first.
  - `host_fixed_timestep_untouched`: a host app sets `Time<Fixed>` to 64 Hz, adds `NpcPlugins` at 20 Hz in `FixedUpdate`, and runs 1 s. The host's `Time<Fixed>` timestep is still 64 Hz, and about 20 sim ticks advanced.
  - `registration_order_independent`: two apps register the same content in different orders and get the same ids and the same hash after 5 000 ticks.
  - `unknown_action_name_panics_helpfully`.
  - `content_command_roundtrip`.
- [ ] **Step 2:** Move code in small steps, keeping the workspace green after each move. Start with kernel extraction, then needs, then jobs, then content, then the facade.
- [ ] **Step 3:** Hash change. Once registration changes the hash encoding, regenerate the replay corpus *only after* proving equivalence: the old binary's and the new binary's journals for the corpus seeds must be identical (both captured as text). Report it and ask the coordinator to approve the re-baseline.
- [ ] **Step 4:** Update `arch_fitness` tables for the new crates, then run `cargo xtask verify`. The SUMMARY lines must be identical.

### Phase 4 (wave C): Locomotion seam and portability proof

#### Task P4: `npc_locomotion` plus `apps/rts_units_demo`

- **Locomotion contract:**
  - `MoveIntent { goal: SimPos, speed_mm_per_tick: i32, tolerance_mm: i32 }` (SparseSet) and `MoveStatus { Idle, Moving, Arrived, Blocked }`.
  - The built-in `StraightLineMover` plugin is deterministic.
  - `MoveToDest`, from content, writes the intent and waits on the status.
  - Host adapters implement a system in `NpcSet::Locomotion` that consumes the intent.
- **`apps/rts_units_demo`** depends only on `npc_framework` (dev-dep `bevy`).
  - About 200 units in 4 squads, each with a `Squad` entity related by `MemberOf`/`Members`.
  - Unit tree: FollowOrder → Hold or Idle-scan. Orders are GoTo and AttackMove; AttackMove is just a GoTo with a different journal tag, since there is no combat yet.
  - The host mover is a simple separation-steering system in host `f32` space in Relaxed mode, proving the seam. Strict mode uses `StraightLineMover`.
  - Simple capsule visuals: no assets needed, colour per squad.
  - Runs the sim in `FixedUpdate` at 10 Hz next to a host system.
- **Tests:**
  - `rts_demo_uses_only_facade`: arch_fitness asserts the demo's dependencies.
  - Headless `rts_demo_orders_move_squads`: units arrive within N ticks.
  - A screenshot verify.

### Phase 5 (wave D): CI and docs

#### Task P5

- **`.github/workflows/ci.yml`**:
  - jobs fmt, clippy, nextest, deny, `cargo hack --each-feature -p npc_framework`, `cargo check -p npc_kernel --no-default-features`;
  - a determinism matrix (ubuntu-latest and windows-latest) runs `npcctl replay verify tests/replays/*.json` and uploads `npcctl run` final hashes. A final job asserts the hashes are equal across OSes.
- **`docs/debugging.md`** is finalised from the tooling reports, along with the README crate map, a "Using npc-framework in your own Bevy game" guide with RTS snippets, and the version table.

### Phase 6: Fresh-agent extension test and final review (coordinator)

- [ ] **Fresh agent A**, given only `CLAUDE.md`:
  - adds a **thirst** need plus a **well** smart object to the village via `/add-mechanic`;
  - the villager drinks;
  - tests and goldens are updated only through the approved re-baseline flow;
  - no core crate changes (checked by `git diff --stat`).
- [ ] **Fresh agent B** adds an **engineer** unit type, which repairs a "damaged" marker, to the RTS demo with no core changes.
- [ ] Their friction notes feed fixes to the docs and skills.
- [ ] **Whole-branch review** by a fresh reviewer, then fixes, then the coordinator commits per phase.
