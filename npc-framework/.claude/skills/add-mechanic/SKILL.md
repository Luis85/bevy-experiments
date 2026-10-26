---
name: add-mechanic
description: Add a new sim mechanic (a need, smart object, action or decision branch, e.g. thirst + a well, or a testbed verb or recipe) to a content crate of the NPC framework, test-first, registered by stable name through the facade, without breaking determinism or core/content boundaries. Use when asked to add or change gameplay behaviour of NPCs.
argument-hint: "<mechanic name> [short description] [content: village|testbed|rts]"
---

# Add a mechanic: $ARGUMENTS

Follow the steps in order. Rules: `CLAUDE.md` golden rules plus `.claude/rules/sim.md` and `.claude/rules/content.md`.
API guide: `docs/using-npc-framework.md` (§4 registration, §6 capability hooks). Re-baseline: `docs/rebaselining.md`.

## 1. Pin down the behaviour (no code yet)

- Read the relevant spec (`docs/superpowers/specs/`) and ADRs (`docs/adr/`; 0009 registration by name, 0011/0012
  capability glue).
- Pick the content crate: `content/village_content` (villager), `content/testbed_content` (Actor Testbed, every
  capability on) or the RTS demo's own content in `apps/rts_units_demo/src/sim.rs`. Content depends on the facade
  `npc_framework` only; core crates are off limits (if the API cannot express it, stop and propose the core
  change separately: spec + ADR).
- Write down in 5–10 lines: trigger (need threshold, time of day, order, percept), what the unit does, which smart
  object it uses (`Provides`, `Approach`, reservations) or capability (an interaction verb, a recipe, an order
  executor), the tree position (which guard, `Abort` mode, above/below which branch), the journal entries an agent
  will need to explain it, and the expected effect on existing behaviour.
- **"Content only" means no `crates/` edits, not one file.** Expect to touch, besides the content crate:
  - the view's exhaustive matches on a new enum variant: a `PlaceKind` in `content/village_view/src/visuals.rs`,
    an `Activity` in `content/village_view/src/anim.rs` (and `examples/view_smoke.rs`);
  - `apps/npcctl/src/sim.rs`, whose village run summary matches `Activity` (meals, work shifts, sleep);
  - app tests (`apps/npcctl/tests`, `apps/village/tests`) that count entities or name ids;
  - the content's `perf_tags` and budgets (step 3), the content README and `CHANGELOG.md`.
  The `npcctl run` summary's `needs` stays hunger/energy. `npcctl run --all-needs` reports every registered need
  (`needs_all`).
- **Plan the oracle impact now and say it.** Each of these changes the content hash, so every replay of that
  content (`tests/replays/village_*` or `testbed_*`) stops verifying, and the goldens and pinned hashes in
  `docs/rebaselining.md` §1 fail: a new registered name (need, action, cond, component, command), a new
  scenario entity, a tuned constant. Ending with a staged re-baseline that the user must approve is a normal
  outcome.
- **Ids.** Need ids are sorted by **name** (`Energy` < `Hunger` < `Thirst`; a `Comfort` need would renumber
  `Hunger`), so always look them up (`NeedRegistry::id("Thirst")`). Spawn a new scenario entity (the well)
  **after** the existing ones, at the end of `spawn_plot`. Existing scenario ids then stay put, but runtime spawns
  shift up by one (the `SpawnVillager` villager becomes 7): `.claude/rules/content.md` "SimId stability".

## 2. Test first

- Village: `content/village_content/tests/<mechanic>.rs` (setup from `tests/villager.rs` / `tests/common/mod.rs`:
  `village(seed)`, `ids(&app)`, `next_sim_id(&app)`, `send`, `run_until`). Testbed: `content/testbed_content/tests/`
  (`scenarios.rs`, `glue.rs`: build with `try_new_sim_app(seed, name, 0)` or a scenario file; ids via
  `TestbedIds::id("<name>")`). RTS demo: `apps/rts_units_demo/tests/small_scenario.rs` (a few units, headless).
- Never hard-code a `SimId` in a new test: look it up (`VillageIds`, `TestbedIds`, `next_sim_id`).
- Assert through the journal or state (a `LeafStart` of the new action within N ticks, `Interacted {outcome:
  "success"}`, `CraftDone`, the need never reaching 0 over 10 days, the reservation released on abort).
- `SetNeed` timing: a `SetNeed` submitted with `at: None` applies when the next tick runs (`NpcSet::Inputs`). That
  tick's `NpcSet::Needs` then decays it once and updates the level. Assert after `run_ticks(world, 1)`, and expect
  `value - decay_per_tick`. Writing `Needs` directly leaves the level stale until the next `Needs` pass
  (`update_level`).
- `cargo test -p <content crate> --test <mechanic>` must fail for the right reason. This crate-level loop takes
  seconds to minutes. Keep the full `cargo xtask verify` (40–50 min) for the end.

## 3. Implement (content side; core only through the registration API)

All of it in the content crate's plugin (`VillagePlugin` in `content/village_content/src/lib.rs`, `TestbedPlugin`
in `content/testbed_content/src/lib.rs`, `RtsContentPlugin` in `apps/rts_units_demo/src/sim.rs`), through
`NpcAppExt` and the capability `*AppExt` traits (`use npc_framework::prelude::*`):

- [ ] Need: `app.register_need(NeedDef { name, key, order, decay_per_tick, thresholds })`; look ids up by name
      after freeze (`NeedRegistry::id("Thirst")`, village `need_id(world, "Thirst")`), never by index.
- [ ] Smart object: a sim entity with `Provides { need, .. }` and an `Approach` point, spawned by the scenario
      after the existing entities. Claims go through `Reservations` (`npc_framework::jobs`). For a capacity-1
      target use `free_for(&res, target, me)`. For several slots (a well with 3 buckets) use `free_slots(&res,
      target, capacity, me)` and `try_reserve_slot(world, me, target, capacity)` (`docs/using-npc-framework.md`
      §4.3).
- [ ] Action: a `SparseSet` component, `start`/`stop` fns (`StartFn = fn(&mut World, Entity, &ParamValue) ->
      Status`), one system in `NpcSet::Actions` **in the content's existing `.chain()`**. The village's list is in
      `village_content::add_systems`. A second unordered system writing `ActiveLeaf` is an ambiguity error.
      Walking writes `MoveIntent` and ends on `MoveStatus` in `NpcSet::Locomotion` `.after(NpcMoverSet)`. Stop
      releases reservations.
- [ ] Condition: a `CondFn`; for an `Abort::LowerPriority` guard it must imply the child can start.
- [ ] Register by name (`register_action("Drink", ..)`, `register_cond("thirst_urgent", ..)`) and place the guard
      in the tree (`b.guard("Drink", "thirst_urgent", Abort::LowerPriority, ..)`).
- [ ] Capability content instead of a new action, where it fits: an interaction verb
      (`register_interaction(InteractionDef::new(..))`, its precondition `Err` is the `why_not`), an item kind
      (`register_item_kind`), a recipe (`register_recipe`), a skill/trait/template (`register_skill`, ..), an order
      executor (`register_order_executor`, `register_custom_order`), an XP source (`register_xp_source`), a
      detection check, an agent observation section / affordance / handler (`register_observation`,
      `register_affordances`, `register_action_handler`).
- [ ] Hash: `impl SimHash` (stable `NAME`) and `register_sim_component::<T>()`; resources with
      `register_sim_resource::<R>()`.
- [ ] External input: a `ContentCommand` (`register_command::<C>()`), accepted as `{"type": NAME, ..fields}`.
- [ ] Journal: content effects as `JournalKind::Effect` JSON; a new `JournalKind` is a kernel (core) change.
- [ ] Perf, two parts:
      - Tag the new systems with a mechanic name: the village's `perf_tags` (`content/village_content/src/lib.rs`,
        e.g. `("village_content::actions::drink_system", "actions.drink")`), the testbed's in
        `content/testbed_view`, or the host's `NpcPerfPlugin::new().tag(prefix, mechanic)`.
      - Give it a budget without editing `npc_perf`: `app.register_budget("actions.drink", 10, 2)`
        (`npc_framework::perf::PerfAppExt`: fixed µs, µs per agent; any plugin order). The village adds a row to
        `VILLAGE_BUDGETS`, which `register_perf_budgets` registers under feature `perf`.
      - An untagged system shows up as `untagged`, and a mechanic without a budget is never checked
        (`docs/debugging.md` §7.2).
- [ ] Inspect/agents: expose new state where an agent needs it (village `src/inspect.rs`, and `src/agent.rs` for
      observations/affordances); registered components appear in the generic inspector by name.
- [ ] Fix the exhaustive matches and app tests from step 1 (view, `apps/npcctl/src/sim.rs`).

## 4. Check the behaviour

```sh
cargo test -p <content crate> --test <mechanic>
cargo run -p npcctl -- run --seed 1 --ticks 7200 --pretty --all-needs         # village per-day summary still sane?
cargo run -p npcctl -- journal --seed 1 --ticks 7200 --unit 5 --kind LeafStart,Abort --grep <Action> --limit 40
cargo run -p npcctl -- run --scenario testbed --ticks 4800 --pretty           # testbed: content_summary
cargo run -p npcctl -- journal --scenario testbed:<name> --ticks 2400 --kind Interacted,CraftDone,OrderFailed --limit 40
cargo test -p village_content --test villager print_three_day_timeline -- --ignored --nocapture
```

- No flip-flop (aborts per day stay low), no starvation, the day rhythm is still recognisable.

## 5. Re-baseline (when the oracles must change)

When replays, goldens or pinned hashes fail **because the behaviour is meant to change**, follow
`docs/rebaselining.md` and nothing else:

1. Check that only the checks listed in its §1 fail. Anything else is a bug in your change.
2. **Stage**: `sh scripts/rebaseline/stage_replays.sh target/release/npcctl <names>` writes
   `tests/replays.next/`. Run `INSTA_UPDATE=new cargo test -p village_content --test golden` (and `-p npcctl
   --test golden --test testbed`) to write `*.snap.new`. If your scenario entity moved runtime ids, write
   `tests/replays.next/<name>.idmap.json` (§5 there) and record with the remapped commands.
3. **Evidence**: run `capture.sh` with a pre-change `git worktree` build and with this build, then `compare.py`.
   Write the note `docs/_notes/<mechanic>-rebaseline-evidence.md` from the template there.
4. **Stop and ask** the user to approve. Name the red checks; the Stop hook's verify stays `ok:false` until the
   swap, and that is expected. The **user** swaps (or creates `.claude/rebaseline-approved` and tells you to run
   the swap commands). Never overwrite an oracle yourself, and never create or delete the marker.

## 6. Document and gate

- [ ] README / content README (`content/testbed_content/README.md`), `CHANGELOG.md` (Unreleased → Added), an
      ADR if a design decision was made.
- [ ] `/verify-visual` if it is visible (new animation, object, panel).
- [ ] `cargo xtask verify` → `"ok":true` (after the swap). Quote the result. If a staged re-baseline is waiting,
      quote the verdict and say which steps are red and why.
