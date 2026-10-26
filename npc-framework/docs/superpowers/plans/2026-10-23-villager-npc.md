# Villager NPC (iteration 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One deterministic, behavior-tree-driven villager with needs, living on a 3D tile in Bevy 0.20.

**Architecture:** Three crates: `npc_ai` (generic BT IR plus think pass), `npc_sim` (headless integer simulation on a `SimTick` schedule) and `npc_view` (Bevy presentation reading sim components). The `village` app wires them together. The public types and signatures already exist as compiling stubs (`todo!("Task X")`). **Those stubs are the interface contract: keep every public name and signature; you may add private items and new public helpers.**

**Tech Stack:** Rust 1.99 (edition 2024), Bevy 0.20.0 (`bevy_ecs`/`bevy_app` for the sim crates), serde.

**Spec:** `docs/superpowers/specs/2026-10-23-villager-npc-design.md`. API reference: `docs/research/bevy-0.20-cheatsheet.md`. Assets: `docs/research/assets.md`, `assets/CREDITS.md`.

## Global Constraints

- Bevy **0.20.0**. `npc_ai` depends on `bevy_ecs` only; `npc_sim` on `bevy_ecs` + `bevy_app` only. No rendering crates in either.
- Sim code: no `Time`, no floats in state, no `HashMap`/`HashSet` iteration, no `Entity` ordering, no `Single<>` queries; order-sensitive loops sort by `SimId`.
- No presentation component is ever inserted on a sim entity.
- `SimTick` at 20 Hz; the day is 2400 ticks; night is the last 300‰ of the day.
- Positions are in mm (`i32`). Needs run 0..=100_000 (satisfaction, 100_000 = full).
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` must pass at the end.
- **Do not `git commit`.** The coordinator commits after review. Run cargo from `npc-framework/`. Other agents build in the same target dir at the same time, so waiting on "Blocking waiting for file lock" is normal.
- Edit only the files your task owns (listed per task).

## Review Focus

1. **A reservation leaking after an abort mid-walk** (e.g. food reserved, walking, then urgent sleep fires). Expected: no reservation survives an aborted branch. Pinned by `reservations_released_on_abort` (Task D).
2. **Urgent hunger with an empty stockpile at night.** Expected: he forages (works) instead of starving in bed. Pinned by `forages_at_night_when_starving` (Task D).
3. **Speed changes or pausing altering outcomes.** Expected: N ticks give the same state regardless of how frames batch them. Pinned by `hash_independent_of_batching` (Task D).
4. **Branch flip-flop at threshold edges.** Expected: hysteresis keeps switches bounded. Pinned by `no_flip_flop` (Task D) and `hysteresis_levels` (Task B).
5. **Missing or not-yet-loaded assets and missing sim resources in the view.** Expected: the view never panics; it skips that frame. Pinned by the `view_smoke` example running without `SimPlugin` (Task C).

---

### Task 0: Scaffold, assets, toolchain (done by coordinator)

Workspace, stubs, `scripts/fetch_assets.sh`, `assets/` + `CREDITS.md`, rustc 1.99.1. Asset facts (measured):

- **Villager clips** in `characters/Rogue_Hooded.glb`: `Idle`, `Walking_A`, `Interact`, `PickUp`, `Use_Item`, `1H_Melee_Attack_Chop`, `Lie_Down`, `Lie_Idle`, `Lie_StandUp`, `Sit_Floor_Idle`, `Cheer`, and others. Hand bone: `handslot.r`.
- **Model sizes** (local units): character about 2.2 tall; house 0.93 tall; grain field 1.9×2.1; tree 1.1; bed 1.6×1.0×3.0; barrel 0.21; crate 0.21; sack 0.06.

---

### Task A: `npc_ai` runtime

**Files:**
- Modify: `crates/npc_ai/src/{tree.rs,builder.rs,runtime.rs}`
- Test: `crates/npc_ai/tests/runtime.rs`

**Interfaces:**
- **Produces** exactly the stubs in `crates/npc_ai/src/` (`TreeBuilder`, `TreeDef::content_hash`, `think`).
- **`StopFn`:** `fn(&mut World, Entity, StopReason)`. `think` passes `Finished` when consuming a completed leaf and `Aborted` on preemption.
- **`think` contract** (the `runtime.rs` doc comment is normative), plus these rules:
  - `BrainState.mem` is resized to the tree length on first use.
  - When `think` consumes a finished leaf, it continues evaluating in the same call. A sequence advances to its next child and may start it in this call. If the root finishes (Success/Failure), `running_path` is cleared and the call ends for that agent; it does not re-run the root.
  - Abort resets `mem` for every node in the aborted subtree.
  - `ActiveLeaf` is inserted only for `Running`.
  - Test helpers may drive status by writing `ActiveLeaf.status` directly.

- [ ] **Step 1: Write failing tests** in `tests/runtime.rs`. Test actions: `instant_ok` (returns Success), `instant_fail` (returns Failure), and `run` (inserts a `TestRun` component and returns Running; its stop pushes `(name, reason)` to a `Log(Vec<String>)` resource). Helpers: `world_with(tree) -> (World, Entity)` and `finish(world, e, Status)`, which sets `ActiveLeaf.status`.
  - `builder_emits_preorder_ir`: `selector("Root"){ guard("G"){ leaf("A") }, leaf("B") }` gives names `["Root","G","A","B"]`, `nodes[0].children == [1,3]`, `nodes[1].children == [2]`.
  - `builder_panics_on_guard_without_child`: `#[should_panic]`.
  - `content_hash_stable_and_sensitive`: two identical builds have equal hashes; changing one leaf `param` changes the hash.
  - `sequence_runs_children_in_order`: `Seq[instant_ok "a", run "b", run "c"]`. After think 1, the active leaf is "b". After `finish(Success)` and think 2, the active leaf is "c" and the log holds `["b:Finished"]`.
  - `running_leaf_is_not_restarted`: three thinks without finishing give `leaf_starts == 1`.
  - `selector_falls_back_on_failure`: `Sel[instant_fail, run "x"]` makes "x" active.
  - `abort_guard_preempts_running_lower_branch`: `Sel[Guard(flag, LowerPriority){run "a"}, run "b"]`. With the flag false, "b" runs. Set the flag true, insert `ThinkRequest`, and think. Expect `"b:Aborted"` in the log, `aborts == 1`, "a" active, and `running_path == [0,1,2]`.
  - `non_abort_guard_does_not_preempt`: same tree with `Abort::None`. "b" stays active and the log is empty.
  - `no_request_no_preemption`: flag true but no `ThinkRequest`, with "b" still Running. No abort.
  - `utility_picks_highest_score_ties_lower_index`: scores `[5, 9, 9]` start child 1.
  - `think_request_removed`: `ThinkRequest` is gone after `think`.
  - `root_completion_clears_path`: `Seq[run]`. Finish Success, think, then `running_path` is empty and there is no `ActiveLeaf`.
- [ ] **Step 2:** Run `cargo test -p npc_ai` and expect failures (todo panics).
- [ ] **Step 3:** Implement `content_hash`, a hand-rolled FNV-1a 64 over a deterministic byte encoding (no `DefaultHasher`). Implement `TreeBuilder` with a stack of open parents, then `think`. Use a recursive `eval(node) -> Status` over a context holding `&mut World`, the entity, the tree (cloned out of the registry, or accessed via `resource_scope`) and the brain state taken out of the entity and written back afterwards.
- [ ] **Step 4:** `cargo test -p npc_ai` passes; `cargo clippy -p npc_ai --all-targets -- -D warnings` is clean.

### Task B: `npc_sim` core

**Files:**
- Modify: `crates/npc_sim/src/{ids,rng,math,needs,reservations,effects,hash,schedule}.rs`
- Test: `crates/npc_sim/tests/core.rs`

**Interfaces:**
- **Consumes:** `npc_ai::{think, Brain, ThinkRequest, AiRegistry}`. Task A may still be in progress; core tests must not depend on `think` behaving correctly beyond "agents without `Brain` are untouched".
- **Produces** the stubs as documented, plus:
  - `SimId` keeps `SimIndex` current via `#[component(on_add = …, on_remove = …)]` hooks.
  - `SimSet::Inputs` contains `copy_prev_pos_system`.
  - `SimSet::Needs` contains `decay_needs_system` (replace the stub with real params: `Query<(Entity, &mut Needs, Has<Asleep>)>`, `Res<NeedTuning>`, `Commands`), plus `day_night_system`, which inserts `ThinkRequest` on every `Brain` entity when `SimConfig::phase_changes_at(clock.tick)`.
  - `SimSet::Think` contains the exclusive `think_system(world: &mut World)`. It collects `(SimId, Entity)` for `With<Brain>`, sorts by SimId and calls `npc_ai::think`.
  - `SimSet::Effects` contains `resolve_effects_system`. It **drains** `ResMut<Messages<SimEffect>>` (do not rely on `Messages::update`, since `First` never runs in tests), sorts by `(source, kind)`, and applies the effects to `Stockpile`s found via `SimIndex`.
  - `SimSet::Cleanup` contains `advance_clock_system` (`tick += 1`).
  - Use `Commands` or direct inserts as you see fit, but **the schedule must build with ambiguity detection = Error**.
- **Hysteresis rules for `update_level`:**
  - From `Ok`: if `v < urgent_enter`, go to Urgent; else if `v < low_enter`, go to Low.
  - From `Low`: if `v < urgent_enter`, go to Urgent; else if `v > low_exit`, go to Ok.
  - From `Urgent`: if `v > urgent_exit`, go to Ok when `v > low_exit`, otherwise to Low.
- **`step_towards`:**
  - Let `d = goal - self` (i64) and `len = isqrt(dx²+dz²)`.
  - If `len <= step`, return `goal`; otherwise return `self + d*step/len`, using integer division truncating toward 0.
- **`sim_rand`:** `splitmix64(seed ^ splitmix64(id as u64 ^ splitmix64(tick ^ splitmix64(purpose as u64))))`. Any fixed combination works if documented. `sim_range` returns `lo + (r % (hi-lo) as u64) as i32`.

- [ ] **Step 1: Write failing tests** in `tests/core.rs`. Helper: `app() -> App` adds `SimPlugin { seed: 7 }` to a bare `App::new()`.
  - `sim_plugin_builds_headless`: `run_ticks(app.world_mut(), 5)` leaves `SimClock.tick == 5`.
  - `sim_ids_sequential_and_indexed`: three `spawn_sim` calls give SimIds 1, 2 and 3, and `SimIndex.get(SimId(2))` returns the 2nd entity. After a despawn, `get` returns `None`.
  - `step_towards_exact_and_capped`: `(0,0)→(3000,4000)` with step 1000 gives `(600,800)`. `(0,0)→(30,40)` with step 100 gives `(30,40)`. `dist2((0,0),(3,4)) == 25`.
  - `sim_rand_deterministic`: equal inputs give equal outputs. Different `tick` gives a different output. 1000 `sim_range(…, -5, 5)` draws all land in `-5..5` and every value appears.
  - `clock_day_night`: with the default config, ticks 0 and 1679 are day; 1680 and 2399 are night; 2400 is day. `phase_changes_at` holds at 1680 and 2400, not at 0 or 1681.
  - `hysteresis_levels`: using hunger thresholds, from Ok:
    - 49_999 gives Low; 70_000 stays Low; 19_999 gives Urgent;
    - 59_000 stays Urgent; 61_000 gives Low; 85_001 gives Ok.
    - `update_level` returns true only on changes.
  - `needs_decay_and_clamp`: an entity with `Needs` loses 40 hunger and 40 energy per tick. With `Asleep`, energy is unchanged. Starting from hunger 30, hunger clamps to 0 after 1 tick. A level change inserts `ThinkRequest` on an entity without `Brain`, and it is still present after the tick.
  - `prev_pos_copied`: set `SimPos` to (5,5) between ticks; after the next tick `PrevSimPos.0 == (5,5)`.
  - `reservations_semantics`:
    - `try_reserve(t, a, 1)` succeeds and `try_reserve(t, b, 1)` fails;
    - re-claiming by `a` succeeds and `count == 1`;
    - after `release_all(a)`, `count == 0`;
    - with capacity 2, `a` and `b` both succeed.
  - `effects_resolved_and_clamped`: stockpile food 9, capacity 10. Writing `AddFood 3` and running one tick gives food 10. With food 0, `TakeFood 1` keeps food at 0.
  - `state_hash_equal_and_sensitive`: two apps with identical spawns hash equal; changing one `SimPos` changes the hash.
- [ ] **Step 2:** `cargo test -p npc_sim --test core` fails.
- [ ] **Step 3:** Implement the stubs. `state_hash` reads action components (`MoveTo`, `Work`, `Eat`, `Sleep`, `Idle` from `actions.rs`) as optional too. They are already defined.
- [ ] **Step 4:** `cargo test -p npc_sim --test core` passes; clippy is clean for `npc_sim`.

### Task C: `npc_view` and the `village` app

**Files:**
- Modify: `crates/npc_view/src/lib.rs`, `apps/village/src/main.rs`
- Create: `crates/npc_view/src/*.rs` modules (your structure) and `crates/npc_view/examples/view_smoke.rs`

**Interfaces:**
- **Consumes:**
  - From `npc_sim`: `SimId`, `SimPos`, `PrevSimPos`, `Place{kind,yaw_deg}`, `PlaceKind`, `Villager`, `Activity`, `Needs`, `Stockpile`, `SimClock`, `SimConfig`, `SimTick`, `Reservations`, `SimIndex`, `scenario::{spawn_village, VillageIds}`, `villager::VillagerPlugin`, `hash::state_hash`.
  - From `npc_ai`: `Brain`, `BrainState`, `Blackboard`, `ActiveLeaf`, `AiRegistry`, `TreeDef::path_string`.
- **Produces:** `SimDriverPlugin`, `ViewPlugin`, `ScreenshotRequest` (already stubbed), `SimSpeed` (pub resource: `multiplier: u32`, `paused: bool`).
- **Blackboard keys** used by Task D (show them in the overlay): `BB_TARGET = BbKey(0)` (`U32(sim_id)`) and `BB_DEST = BbKey(1)` (`Pos(x,z)`). Task D defines them as `npc_sim::villager::{BB_TARGET, BB_DEST}`; until then, mirror them as local constants.

**Requirements** (spec §6.7):

1. **`SimDriverPlugin`:**
   - Sets `Time::<Fixed>::from_hz(20.0)`.
   - An exclusive system in `FixedUpdate` runs `SimTick` `multiplier` times unless paused.
   - Space toggles pause; 1/2/3/4 set 1×/3×/10×/0×.
   - It must **not** panic when `SimTick` is absent (skip if `!world.contains_resource::<SimClock>()`).
2. **Visuals:**
   - For each sim entity with `Place` or `Villager`, spawn a separate visual entity with `VisualOf(sim_entity)`. Track spawned visuals in a view-side resource; never insert onto the sim entity.
   - Despawn a visual when its sim entity is gone.
   - **Scales** (from Task 0 sizes; verify by screenshot):
     - house `hex/building_home_A_blue.gltf` ×5
     - field `hex/building_grain.gltf` ×3
     - bed `furniture/bed_single_A.gltf` ×0.7
     - stockpile: `hex/crate_A_big.gltf` + `hex/barrel.gltf` ×5, plus `min(food,10)` `hex/sack.gltf` ×5 laid out next to them and updated when `food` changes
     - villager `characters/Rogue_Hooded.glb` ×0.8
     - `Place.yaw_deg` gives the yaw.
3. **Transform:**
   - Lerp `PrevSimPos → SimPos` by `Time<Fixed>::overstep_fraction()`, with mm/1000 giving metres (sim x → world x, sim z → world z).
   - The villager yaw faces the movement direction, smoothed. While `Activity::Sleep`, render at the bed's position, lying along the bed.
4. **Animation:**
   - Load the glb as `Handle<Gltf>`. When loaded, build an `AnimationGraph` from `named_animations` for `Idle`, `Walking_A`, `Interact` (work), `Use_Item` (eat) and `Lie_Idle` (sleep).
   - Attach to the `AnimationPlayer` found on `WorldInstanceReady` via `iter_descendants`.
   - Map `Activity` to a clip and cross-fade over 200 ms with `AnimationTransitions` whenever it changes; loop every clip.
5. **Scene:**
   - 32×32 m ground plane using `textures/grass_color.jpg` with a repeat sampler, a `uv_transform` scale of 8, and a green tint to match KayKit's flat style.
   - View-only dressing: trees, rocks, fence and lumber around the edges, kept clear of straight walking paths between places.
   - Directional light with shadows plus ambient light.
6. **Day/night:**
   - The sun direction, colour and illuminance follow `SimConfig::time_of_day_permille(clock.tick)`.
   - Night is dim and blue but still readable. Ambient light dims at night too.
7. **RTS camera:**
   - WASD/arrow keys pan, mouse wheel zooms (clamped), Q/E rotate around the focus point.
   - Starts at about 45° looking at the origin from about 25 m away.
8. **Overlay** (F1 toggle, on by default):
   - Text panel: tick, day N and HH:MM, speed or paused, `state_hash`, stockpile food/capacity.
   - Villager hunger % and energy % with level, `tree.path_string(running_path)`, `Activity`, `BB_DEST`, and reservations (target→claimant SimIds).
   - Gizmos: a line from the villager to `BB_DEST`, and a circle at every reserved target's position.
   - Missing resources mean that part is skipped (`Option<Res<…>>`).
9. **`ScreenshotRequest`:** after `after_secs`, take a screenshot of the primary window with the Bevy 0.20 screenshot API (verify it on docs.rs) and save it to `path`. Exit with `AppExit::Success` once it is saved.
10. **`village` main:**
    - Args: `--seed N` (default 1), `--speed N` (initial multiplier), `--screenshot PATH`, `--after-secs S` (default 3).
    - Plugins: `DefaultPlugins` (window title "npc-framework — village", asset root `assets` via `AssetPlugin { file_path: "../../assets" … }` or the `BEVY_ASSET_ROOT` approach, so `cargo run -p village` works from `npc-framework/`), `SimPlugin { seed }`, `VillagerPlugin`, `SimDriverPlugin`, `ViewPlugin`.
    - At `Startup`, an exclusive system calls `spawn_village(world)`.
11. **`examples/view_smoke.rs`:**
    - Does **not** add `SimPlugin` or `VillagerPlugin`. It inserts `SimClock` and `SimConfig` itself and spawns sim entities directly (house, bed, stockpile, field, villager with `Activity::Walk`, `PrevSimPos`, `Needs`).
    - It takes the same `--screenshot/--after-secs` args.
    - Used to verify visuals before Tasks B/D land.

- [ ] **Step 1:** `cargo build -p npc_view --example view_smoke` succeeds.
- [ ] **Step 2:** Run `cargo run -p npc_view --example view_smoke -- --screenshot ../screenshots/smoke.png --after-secs 6` (adjust the path so it lands in `npc-framework/screenshots/`). Open the PNG with the Read tool. Check that the models are visible, the scales are plausible (house roughly 2.5× villager height), the villager is animated (take 2 screenshots at different `after-secs`) and the overlay is readable. Iterate until it looks right.
- [ ] **Step 3:** `cargo clippy -p npc_view -p village --all-targets -- -D warnings` is clean. Building `village` must compile even while `spawn_village` is still `todo!()`.

### Task D: villager actions, tree, scenario and tests (after A and B)

**Files:**
- Modify: `crates/npc_sim/src/{actions.rs,villager.rs,scenario.rs}`, and `hash.rs` only if needed.
- Test: `crates/npc_sim/tests/villager.rs`, `crates/npc_sim/tests/determinism.rs`

**Interfaces:**
- **Consumes:** Tasks A and B.
- **Produces:**
  - `pub const BB_TARGET: BbKey = BbKey(0); pub const BB_DEST: BbKey = BbKey(1);` in `villager.rs`.
  - `#[derive(Resource)] pub struct VillagerTree(pub TreeId)`.
  - `VillagerPlugin`.
  - `spawn_village`, with the layout from the `scenario.rs` doc comment. Bed `Provides{Energy, 200}`, stockpile `Provides{Hunger, 1000}`, villager `Needs::default()`, `Activity::Idle`, `PrevSimPos`, `Brain{tree}`.
- **Availability:** `available_food(stockpile) = food − reservations.count(stockpile)` (saturating).
- **Conditions** (registered in `AiRegistry`):
  - `hunger_urgent_food`: hunger Urgent and some stockpile has available food.
  - `energy_urgent`: energy Urgent.
  - `forage`: hunger Urgent, no available food, and a work site is free (no claimant, or the agent itself).
  - `is_night`.
  - `peckish_food`: hunger level ≥ Low and some stockpile has available food.
  - `day_work`: not night, the work site's stockpile `food < capacity`, and the site is free.
- **Actions** (start / per-tick system / stop). All stops remove their component and set `Activity::Idle`. On `StopReason::Aborted`, **every** stop calls `reservations.release_all(me)`.
  - `ReserveFood` (instant): `release_all(me)`, then pick the lowest-SimId stockpile with available food and `try_reserve(stockpile, me, food)`. Set `BB_TARGET`/`BB_DEST` (= its `Approach`) and return Success; otherwise Failure.
  - `ReserveBed` (instant): `release_all(me)`, then pick the bed whose `home == villager.home` and reserve it with capacity 1. Return Success or Failure.
  - `ReserveWork` (instant): `release_all(me)`, then pick the lowest-SimId free work site and reserve it with capacity 1. Return Success or Failure.
  - `MoveToDest`: if already at `BB_DEST`, return Success. Otherwise insert `MoveTo{goal}`, set `Activity::Walk` and return Running. The system calls `step_towards(goal, villager.speed)` and sets Success on arrival.
  - `EatFood`: insert `Eat{stockpile: BB_TARGET, remaining: 60}` with `Activity::Eat`. Each tick adds the stockpile's `Provides.amount_per_tick` to hunger (clamped at `NEED_MAX`). If the stockpile's food is 0, set Failure. When `remaining` reaches 0, write `SimEffect TakeFood 1` and set Success. On Finished, stop releases `(stockpile, me)`.
  - `DoWork`: insert `Work{site, remaining: site.work_ticks}` with `Activity::Work`. When `remaining` reaches 0, write `AddFood{site.stockpile, yield_food}` and set Success. On Finished, stop releases `(site, me)`.
  - `SleepInBed(param: 0 = Full, 1 = Morning)`: insert `Sleep` and `Asleep` with `Activity::Sleep`. Each tick adds the bed's `Provides` to energy. `Full` succeeds when energy ≥ `NEED_MAX`. `Morning` succeeds when it is not night and energy > `energy.low_exit`. Stop also removes `Asleep` and, on Finished, releases the bed.
  - `PickWanderPoint` (instant): `BB_DEST` = home approach + `sim_range(seed, me, tick, purpose 1|2, -5000, 5001)` on each axis, clamped to ±14000. Return Success.
  - `IdleFor(param = max ticks)`: `Idle{remaining: sim_range(…, purpose 3, 60, param)}` with `Activity::Idle`. Succeeds at 0.
- **Tree** (names matter: they appear in the overlay and in tests):
  ```
  Selector "Villager"
  ├ Guard "UrgentEat"   [hunger_urgent_food, LowerPriority] → Sequence "EatSeq"   [ReserveFood, MoveToDest, EatFood]
  ├ Guard "UrgentSleep" [energy_urgent,      LowerPriority] → Sequence "SleepSeq" [ReserveBed, MoveToDest, SleepInBed(0)]
  ├ Guard "Forage"      [forage,             LowerPriority] → Sequence "WorkSeq"  [ReserveWork, MoveToDest, DoWork]
  │   // RTS orders slot in here later (spec §6.6)
  ├ Guard "NightSleep"  [is_night,   None] → Sequence "SleepSeq" [ReserveBed, MoveToDest, SleepInBed(1)]
  ├ Guard "CasualEat"   [peckish_food, None] → Sequence "EatSeq" [...]
  ├ Guard "DayWork"     [day_work,   None] → Sequence "WorkSeq" [...]
  └ Sequence "Wander" [PickWanderPoint, MoveToDest, IdleFor(200)]
  ```
- **System ordering:** action systems go in `SimSet::Actions`. Each touches disjoint components, or is explicitly ordered/chained to satisfy ambiguity = Error.

- [ ] **Step 1: Write failing tests.** Helper `village(seed) -> (App, VillageIds)` with `SimPlugin` + `VillagerPlugin` + `spawn_village`. Helpers `run_until(app, max_ticks, pred) -> Option<u64>` and `path(app) -> String` (the overlay path string).
  - `works_then_eats_when_starving_and_empty`: stockpile food 0, villager hunger value 15_000. Within 1200 ticks `Activity::Work` is seen, then the stockpile food is > 0, then `Activity::Eat` is seen, in that order.
  - `urgent_hunger_interrupts_wander`: food 10 at 10:00 (`tick 600`, via `SimClock` insert after spawn). Run until the path contains "Wander". Set hunger to 10_000. Within 3 ticks the path starts with "Villager > UrgentEat" and `aborts` has increased.
  - `urgent_hunger_interrupts_work`: food 0, then set food to 5 once the path contains "DayWork > WorkSeq > DoWork". Hunger to 10_000 gives "UrgentEat" within 3 ticks.
  - `sleeps_at_night_and_wakes_in_morning`: food 10, needs full. At tick 1680+400, `Activity::Sleep` and the path contains "NightSleep". By tick 2400+300, the path does not contain "Sleep".
  - `full_stockpile_means_free_time`: food 10, tick 600, needs full. Over 600 ticks `Activity::Work` is never seen and "Wander" is seen.
  - `reservations_released_on_abort`: food 0, tick 600. Run until the path contains "WorkSeq > MoveToDest" and the field is reserved. Set food 5 and hunger 10_000. Within 3 ticks the field is not held by the villager.
  - `forages_at_night_when_starving`: tick 1700 (night), food 0, hunger 15_000. Within 600 ticks the path contains "Forage" and `Activity::Work` is seen.
  - `never_starves_over_ten_days`: run 24_000 ticks from default spawn. The minimum hunger value stays > 0 and the minimum energy value stays > 0.
  - `no_flip_flop`: over 24_000 ticks, `BrainStats.aborts / 10 <= 6` and `leaf_starts / 10 <= 250`.
  - `determinism.rs::same_seed_same_hash`: two `village(42)` runs of 20_000 ticks have equal `state_hash` at ticks 5_000, 10_000 and 20_000. `village(43)` differs at 20_000.
  - `determinism.rs::hash_independent_of_batching`: `run_ticks(…, 20_000)` gives the same hash as 4_000 calls of `run_ticks(…, 5)`.
- [ ] **Step 2:** `cargo test -p npc_sim` fails.
- [ ] **Step 3:** Implement. Tune `NeedTuning`, `SimConfig` and the scenario numbers **only** if a test proves the defaults are wrong, and keep the spec's rhythm: about 1–2 meals and one night's sleep per day. Report every tuning change.
- [ ] **Step 4:** `cargo test --workspace` passes and `cargo clippy --workspace --all-targets -- -D warnings` is clean.

### Task E: Integration and verification (coordinator)

- [ ] **Screenshots:** `cargo run -p village -- --screenshot screenshots/day.png --after-secs 8`, then `--speed 10 --after-secs 10` for night. Inspect both.
- [ ] **Watch for a few minutes:** confirm the villager cycles through Work → Eat → Wander → Sleep, in a window or through a run of screenshots.
- [ ] **Review:** a whole-branch code review by a fresh subagent; fix the findings.
- [ ] **Commit and README:** commit with a README (how to run, controls, architecture sketch).
