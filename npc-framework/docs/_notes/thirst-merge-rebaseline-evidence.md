# Thirst merge re-baseline evidence (staged, awaiting approval)

Merge of `worktree-agent-a198fa7b0aa7f2f3e` (`27c8e31`, "village thirst need and wells", built on `c4d0611`)
into `claude/bevy-3d-npc-framework-705347` at `17e2376` (the FX1–FX4 tree plus the RTS repair merge;
`17e2376` is the amended `805291c`, differing only by one CHANGELOG blank line). Procedure:
`docs/rebaselining.md` (stage → evidence → approval → swap). Nothing protected was overwritten: the new
oracles are staged next to the old ones. The branch's own note, `thirst-well-evidence.md`, was written before
FX4 and is superseded by this one.

## What changed and why

- **The change** (content only, `village_content` / `village_view`): need `Thirst` (decay 45/tick awake,
  paused asleep like energy), a `Well` smart object per plot (`Provides Thirst`, `Approach`, 2 slots through
  `Reservations`), actions `ReserveWell` / `DrinkWater`, conditions `thirst_urgent_well` / `thirsty_well`,
  guards `UrgentDrink` (below `UrgentEat`, abort lower) and `CasualDrink` (below `NightSleep`), the agent verb
  `drink`. In the merge the branch's ad-hoc `well_free_for` was replaced by FX4a's `jobs::free_slots`
  (identical arithmetic: `slots − claims of others`), and `actions.drink` got a budget row in
  `VILLAGE_BUDGETS` (10 µs + 2 µs/villager, like eat's).
- **Why the oracles change:** new registered names change the content hash
  (`6937084237724115216` → `7988500353009835332`); `Needs` is now 3 long and the well is a new hashed entity,
  so every checkpoint hash differs from tick 0; and villagers now drink (intended behaviour).
- **SimIds:** the well is spawned after each plot (in `village_n` after all plots), so house 1, bed 2,
  stockpile 3, field 4, villager 5 keep their ids and the well is 6. Runtime spawns move up by one (the first
  `SpawnVillager` villager is 7, not 6): `village_spawn_despawn` is staged with an id remap (below).

## Checks that fail until the swap

From `docs/rebaselining.md` §1, and nothing else:

- `content/village_content/tests/corpus.rs` `replay_corpus_verifies`: content hash of the 4 village replays.
- `content/village_content/tests/village_perception.rs` `without_the_plugin_the_village_is_unchanged`: it
  pins the content hash recorded in `tests/replays/village_seed1_3days.json`.
- `content/village_content/tests/golden.rs` `golden_inspect_unit5_t600` / `_t1800` / `_t4000`,
  `golden_journal_seed1_first_200`; `apps/npcctl/tests/golden.rs` `golden_run_seed1_7200`.
- `cargo xtask verify` steps `test` (the tests above), `replays` (the 4 village files) and `goldens` (the 5
  pending `.snap.new`).

Pass: `corpus_equivalence` (village, with the id remap; testbed: nothing staged), `golden_run_testbed_4800`,
`testbed_replay_corpus_verifies`, every other test. Layout pins edited with the change (tests, not oracles):
`inspect.rs` (`[1..=6]`, 600 entities in `spawn_village_n_layout`), `replay.rs` `new_sim_app_scenarios`
(6 entities), the reference `Model` in `properties.rs` (wells after all plots), `apps/npcctl/tests/all_needs.rs`
(the village now has 3 needs; `--all-needs` reports `thirst`, the default `needs` stays hunger/energy).

## Evidence (before vs after)

- **Before:** `npcctl` (release) from a temporary `git worktree --detach` of `17e2376` (removed afterwards;
  its target dir had to live at `npc-framework/target/pre-thirst` because the scratchpad path exceeded the
  Windows linker's MAX_PATH, and was `cargo clean`ed and deleted). **After:** this tree.
- Both ran `scripts/p3/capture.sh` on copies of `tests/replays/`; the post copy of
  `village_spawn_despawn.json` has the remapped commands (with the original ones the post run would order and
  then despawn the well). `scripts/p3/compare.py` verdict: `DIFFERENCES FOUND`, all of them in village files;
  every testbed file is identical (journals byte for byte, `final_hash` equal; `testbed_agent_session.run.json`
  differs only in its `cap-pre`/`cap-post` scenario path). `diverge --seed-a 1 --seed-b 2` is byte-identical.
- Journal check (a scratch script; seq ignored only for the verbose journals): each post journal is
  **entry-for-entry equal to the pre journal up to its first thirst entry**, and the first difference is that
  entry (`NeedLevel Thirst Ok→Low` at tick 1111, 1111 = ⌈50 000 / 45⌉ awake ticks). Per unit, no journal
  diverges before that unit's own first thirst entry, except in `village_spawn_despawn` (below).

| run | journal lines pre → post | first difference | drinks (finished) | per villager per day | aborts pre → post |
|---|---|---|---|---|---|
| seed1 7200 | 448 → 469 | #64, t1111 `NeedLevel Thirst` | 3 | 1 / 1 / 1 | 0 → 0 |
| seed2 7200 | 476 → 487 | #61, t1111 `NeedLevel Thirst` | 3 | 1 / 1 / 1 | 1 → 0 |
| seed3 7200 | 454 → 478 | #64, t1111 `NeedLevel Thirst` | 3 | 1 / 1 / 1 | 1 → 0 |
| seed7 7200 | 451 → 486 | #64, t1111 `NeedLevel Thirst` | 3 | 1 / 1 / 1 | 1 → 1 (same `UrgentEat`) |
| n9 seed5 4800 | 2435 → 2572 | #589, t1111 `NeedLevel Thirst` | 18 | 1 each, both days | 7 → 9 |
| village_n_25_1day | 3383 → 3606 | #1614, t1111 `NeedLevel Thirst` | 25 | 1 each | 24 → 28 |
| village_orders | 349 → 377 | #97, t1111 `NeedLevel Thirst` | 2 | 1 / 1 | 7 → 7 (same guards) |
| village_seed1_3days | 448 → 469 | #64, t1111 `NeedLevel Thirst` | 3 | 1 / 1 / 1 | 0 → 0 |
| village_spawn_despawn (remapped) | 888 → 927 | #18, t72 `ThinkStart Request` of unit 8 (pre 7) | 7 | 0–1 / 0–2 | 1 → 1 (same `FollowOrder`) |

- **`village_spawn_despawn`:** with pre ids ≥ 6 shifted by one, the journal is equal up to tick 72. Unit 5
  (the scenario villager, despawned at 500 before any thirst) is identical throughout. The runtime villagers
  (7–10, pre 6–9) differ from their first re-think on: the stagger is `(tick + sim_id) % 40`
  (`npc_kernel::clock::rethink_timer_system`), so unit 8 re-thinks at 72 instead of 73 and its wander point
  (`sim_rand` keyed by unit) differs. That is the id shift itself, not thirst, and it is unavoidable with a new
  scenario entity; the commands keep their meaning through the remap (the same villager is ordered at 200 and
  despawned at 2000).
- **Verbose journals:** every think also evaluates `thirst_urgent_well` (and `thirsty_well` when reached)
  from tick 0 (285 extra `CondEval`s in `village_seed1_3days`, 172 in `village_orders`, 2062 in
  `village_n_25_1day`). Without those, and ignoring `seq`, they too are equal up to t1111 `NeedLevel Thirst`.
- **Aborts / flip-flop:** only two aborts involve a drink, both single (n9: `UrgentEat` aborting a
  `CasualDrink` walk at t3500, `UrgentDrink` upgrading a `CasualDrink` walk at t3678). The extra aborts in
  `village_n_25_1day` are `UrgentSleep` upgrading a `NightSleep` walk (5 → 10; same bed), at most 2 aborts per
  villager as before. No guard aborts and restarts repeatedly; every started drink finishes.
- **Starvation:** unchanged. The seed runs keep min hunger ≥ 14 % (seed2/seed3 rise from 17.6/14.0 % to
  20.2 %); the runs that already hit 0 before (n9 day 2, `village_orders` day 2, `village_spawn_despawn`
  energy) hit it equally before the merge.

Run summary, seed 1 (`npcctl run --seed 1 --ticks 7200 --all-needs`; per day meals / work shifts / ticks
asleep / min hunger / min energy):

| day | before | after |
|---|---|---|
| 1 | 1 / 2 / 582 / 32 480 / 27 280 | 1 / 2 / 578 / 36 480 / 27 120 |
| 2 | 1 / 2 / 632 / 27 800 / 29 320 | 1 / 2 / 607 / 27 800 / 28 320 |
| 3 | 3 / 1 / 379 / 20 240 / 19 200 | 3 / 1 / 326 / 20 240 / 17 080 |

aborts 0 → 0, leaf starts 44 → 45, `needs_all` gains `thirst {min 26 290, max 100 000}`.

10-day timeline (`cargo test --release -p village_content --test villager print_three_day_timeline --
--ignored --nocapture`), before → after: meals per day 1,1,3,1,2,1,3,1,2,1 → 1,1,3,1,2,2,2,3,1,2; work
shifts 2,2,1,2,1,2,1,2,1,2 → 2,2,1,2,2,1,1,1,2,1; **drinks 1,1,1,2,1,1,2,1,1,2**; min thirst 26–40 %; min
hunger 17–34 % → 15–38 %; min energy 16–29 % → 16–29 %; `BrainStats` leaf starts 155 → 160, aborts 1 → 1.
Sleep every night (326–625 ticks asleep). `never_starves_over_ten_days`, `no_flip_flop`,
`never_dehydrates_over_ten_days` and the 5 `thirst.rs` tests pass.

## Staged files (not applied)

| Oracle | Staged as | Change |
|---|---|---|
| `tests/replays/village_n_25_1day.json` | `tests/replays.next/village_n_25_1day.json` | content_hash, 24 checkpoint hashes |
| `tests/replays/village_orders.json` | `tests/replays.next/village_orders.json` | content_hash, 48 checkpoint hashes |
| `tests/replays/village_seed1_3days.json` | `tests/replays.next/village_seed1_3days.json` | content_hash, 72 checkpoint hashes |
| `tests/replays/village_spawn_despawn.json` | `tests/replays.next/village_spawn_despawn.json` | content_hash, 48 checkpoint hashes, commands remapped |
| (id remap) | `tests/replays.next/village_spawn_despawn.idmap.json` | ids 6 → 7 in `unit`, `units` (the `Order` at t200, the `Despawn` at t2000) |
| `content/village_content/tests/snapshots/golden__inspect_unit5_t600.snap` | `….snap.new` | `thirst` need (and insta's header) |
| `…/golden__inspect_unit5_t1800.snap` | `….snap.new` | `thirst`, the journal tail and needs after the first drink |
| `…/golden__inspect_unit5_t4000.snap` | `….snap.new` | the same at t4000 |
| `…/golden__journal_seed1_first_200.snap` | `….snap.new` | equal up to entry 64 (t1111 `NeedLevel Thirst`), then the first drink |
| `apps/npcctl/tests/snapshots/golden__run_seed1_7200.snap` | `….snap.new` | `final_hash`, ticks asleep, min energy/hunger, leaf starts (table above) |

Replays were staged with `scripts/rebaseline/stage_replays.sh target/release/npcctl village_n_25_1day
village_orders village_seed1_3days`, then re-recorded with the same flags plus `--pretty` so they keep the
originals' layout (the diff against each original is the content hash and the checkpoint lines only);
`village_spawn_despawn` was recorded from a copy of the original with the remap applied (`--commands`). All
four verify (`checked` 24 / 48 / 72 / 48). The goldens were staged with `INSTA_UPDATE=new cargo test -p
village_content --test golden` and `INSTA_UPDATE=new cargo test -p npcctl --test golden`. The testbed needs
nothing: its corpus verifies with this build and `golden_run_testbed_4800` passes.

## Applying (user)

The swap commands of `docs/rebaselining.md` §4 (the `.idmap.json` is not copied), then `cargo xtask verify`.
