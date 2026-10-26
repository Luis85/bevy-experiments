# Thirst + well re-baseline evidence (staged, awaiting approval)

> **Superseded** by `docs/_notes/thirst-merge-rebaseline-evidence.md`: this note was written on the branch
> (base `c4d0611`, before FX1–FX4); the re-baseline was redone after merging it into the FX4 tree, with the
> documented id remap (`docs/rebaselining.md` §5) and every golden staged. The "Open decisions" below are
> resolved there.

The village gets a third need (`Thirst`) and a smart object (`Well`): villagers drink at the well when
thirsty. This is a deliberate behaviour change, so it follows the P3/C2 method
(`docs/_notes/p3-rebaseline-evidence.md`, `docs/_notes/c2-eat-claim-evidence.md`): capture before/after,
explain the differences, stage the new oracles, stop. Nothing protected was overwritten; the staged files
are listed at the end. Two points need a decision by the user (see "Open decisions").

## The change (content only: `content/village_content`, `content/village_view`)

- Need `Thirst` (`key` `thirst`, `order` 2, decay 45/tick, thresholds Low 50 000 / exit 85 000, Urgent
  20 000 / exit 60 000). Sleeping pauses it like energy (`PauseDecay` now covers energy and thirst).
- Smart object `Well { slots: 2 }` + `Place { kind: Well }` + `Provides { Thirst, 2500/tick }` + `Approach`,
  one per plot at (-4500, 4500), approach (-4500, 3300). It is spawned **after** the plot (in `village_n`
  after all plots), so house 1, bed 2, stockpile 3, field 4, villager 5 keep their SimIds; the well is 6.
- Actions `ReserveWell` (nearest well with a free slot, ties by SimId; claim with cap = slots) and
  `DrinkWater` (`Drink { well, remaining }`, hashed as `drink`; 40 ticks; fills thirst; releases the slot
  in the tick it completes, journaled `Release { Finished }`, like a meal). `Activity::Drink` (u8 5).
- Conditions `thirst_urgent_well` (Urgent and a free slot) and `thirsty_well` (≥ Low and a free slot);
  guards `UrgentDrink` (`LowerPriority`, between `UrgentEat` and `UrgentSleep`) and `CasualDrink` (`None`,
  between `NightSleep` and `CasualEat`).
- Agent control: verb/affordance `drink`, place label `well`. Perf mechanic `actions.drink`. View: well
  model (stone pile + barrel), drink uses the eat clip, thirst bar in the F1 overlay.

Test: `content/village_content/tests/thirst.rs` (5 tests). It failed first for the right reason
(`unknown need 'Thirst' (known: Energy, Hunger)`; `the village has exactly one well`) and passes now.

## What changes, and why

1. **Content hash:** new names (need, actions, conditions, `drink` component, tree content) change it:
   `6937084237724115216` → `7988500353009835332`. So every village replay stops verifying
   (`content hash mismatch`), whatever its checkpoints.
2. **State hash from tick 0:** `Needs` is a length-prefixed list, now 3 long, and the well is a new hashed
   entity, so every checkpoint hash differs (0 of 192 equal).
3. **Behaviour:** identical until thirst first goes Low (tick 1111 for a villager spawned at tick 0: every
   journal is entry-for-entry equal before its first `NeedLevel Thirst`). After that the villager spends
   ~1-2 short trips a day at the well (walk + 40 ticks), mostly in place of wander time.
4. **SimIds of anything spawned at runtime shift by one** (the well takes 6): the first `SpawnVillager`
   is now 7, an agent scenario's first actor 7. This affects `village_spawn_despawn` (see Open decisions)
   and tests that hard-coded "the spawned villager is 6" or "the layout has 5 entities" (updated, see the
   file list in the final report).

Seed-1 example (post), the first drink:

```text
1111/0 NeedLevel Thirst Ok→Low          1147/1 Reserve target 6 ok
1147/2 LeafStart Villager > CasualDrink > DrinkSeq > ReserveWell
1147/3 LeafStart Villager > CasualDrink > DrinkSeq > MoveToDest
1298/2 LeafStart Villager > CasualDrink > DrinkSeq > DrinkWater
1337/0 Release target 6 Finished        1338/0 NeedLevel Thirst Low→Ok
1339/2 LeafStart Villager > CasualEat > EatSeq > ReserveFood   (the walk delayed lunch; same meal)
```

## Method

- Pre binary: this worktree at `c4d0611` (unchanged), `cargo build --release -p npcctl`, copied out before
  any edit. Post binary: the same tree with the change.
- Both ran `scripts/p3/capture.sh <npcctl> <out> <copy of the village corpus>`; for the post run the copy of
  `village_spawn_despawn.json` has its two commands on unit 6 remapped to 7 (see Open decisions; with the
  original commands the post binary refuses the run: `command #2 at tick 200 rejected: unit 6 is not a unit
  that takes orders`, exit 2).
- A comparison script (journal kind-count deltas, first difference, aborts, per-day run summaries,
  checkpoint hashes); scratch only, not committed.

## Results

Journals (pre → post lines; first difference):

| journal | lines | first difference | aborts pre → post |
|---|---|---|---|
| seed1 (7200 ticks) | 448 → 469 | 1111 `NeedLevel Thirst` | 0 → 0 |
| seed2 | 476 → 487 | 1111 `NeedLevel Thirst` | 1 → 0 |
| seed3 | 454 → 478 | 1111 `NeedLevel Thirst` | 1 → 0 |
| seed7 | 451 → 486 | 1111 `NeedLevel Thirst` | 1 → 1 (same `UrgentEat` at 7051) |
| n9 seed 5 (4800 ticks, 9 villagers) | 2435 → 2572 | 1111 `NeedLevel Thirst` | 7 → 9 |
| village_n_25_1day | 3383 → 3606 | 1111 `NeedLevel Thirst` | 24 → 28 |
| village_orders | 349 → 377 | 1111 `NeedLevel Thirst` | 7 → 7 (same guards) |
| village_seed1_3days | 448 → 469 | 1111 `NeedLevel Thirst` | 0 → 0 |
| village_spawn_despawn (remapped) | 888 → 927 | tick 50 `Spawn` (id 7 instead of 6) | 1 → 1 (same `FollowOrder` at 200) |

The verbose journals (`--verbose`, with `CondEval`) differ from tick 0: every think now also evaluates
`thirst_urgent_well` (and `thirsty_well` when reached); apart from those `CondEval`s they show the same
changes as the plain journals.

Abort changes in the multi-villager runs (no flip-flop: each guard fires once per villager):
`village_n_25_1day` `UrgentSleep` aborting a `NightSleep` walk 5 → 10, `UrgentEat` aborting `CasualEat`
(walk or meal) 18 → 18; `n9_seed5` one `UrgentDrink` and one `UrgentEat` aborting a `CasualDrink` walk.
Drinks per run: seed 1: 3 in 3 days; village_n_25: 25 in one day (one per villager).

Run summaries (seed 1, 3 days, pre → post): meals 1/1/3 → 1/1/3, work shifts 2/2/1 → 2/2/1, ticks asleep
582/632/379 → 578/607/326, min hunger 32 480/27 800/20 240 → 36 480/27 800/20 240, min energy
27 280/29 320/19 200 → 27 120/28 320/17 080, aborts 0 → 0. (`npcctl run`'s summary has no thirst column:
it is built in `apps/npcctl`, out of scope here; thirst stats come from the timeline test below.)

10-day timeline (`cargo test -p village_content --test villager print_three_day_timeline -- --ignored
--nocapture`, post): 1-2 drinks a day, min thirst 26-40 %, min hunger 15-38 %, min energy 16-29 %,
`BrainStats { leaf_starts: 160, aborts: 1 }` over 10 days. `never_dehydrates_over_ten_days` and the
existing `never_starves_over_ten_days` / `no_flip_flop` pass.

Other outputs: `diverge --seed-a 1 --seed-b 2` byte-identical (first divergent tick unchanged);
`inspect --unit 5` t600/t1800/t4000 and every `inspect --all` differ (the `thirst` need, the well entity,
the shifted journal).

## Staged oracle files (not swapped in)

| oracle | staged as | change |
|---|---|---|
| `tests/replays/village_n_25_1day.json` | `tests/replays.next/village_n_25_1day.json` | content hash, 24 checkpoint hashes |
| `tests/replays/village_orders.json` | `tests/replays.next/village_orders.json` | content hash, 48 checkpoint hashes |
| `tests/replays/village_seed1_3days.json` | `tests/replays.next/village_seed1_3days.json` | content hash, 72 checkpoint hashes |
| `tests/replays/village_spawn_despawn.json` | `tests/replays.next/village_spawn_despawn.json` | content hash, 48 checkpoint hashes, **commands: unit 6 → 7 at ticks 200 and 2000** |
| `content/village_content/tests/snapshots/golden__inspect_unit5_t600.snap` | `….snap.new` (insta) | `thirst` need, `source` header |
| `content/village_content/tests/snapshots/golden__journal_seed1_first_200.snap` | `….snap.new` (insta) | thirst `NeedLevel` and the first drink inside the first 200 entries |
| `apps/npcctl/tests/snapshots/golden__run_seed1_7200.snap` | `….snap.new` (insta) | `final_hash`, ticks asleep, min energy/hunger, leaf starts |

Not yet staged: `golden__inspect_unit5_t1800.snap` and `…_t4000.snap`. `golden_inspect_unit5` stops at
its first failing snapshot (t600), so insta writes their `.snap.new` only after t600 is accepted (and t4000
after t1800), as in C2. The headless equivalent of each is in the capture (`inspect --unit 5`).

Each staged replay was written by the post release binary (`npcctl replay record --seed S --scenario X
--villagers V --commands <original> --ticks T --checkpoint-every 100 --pretty --out
tests/replays.next/<name>.json`; for `village_spawn_despawn` with the remapped command file) and
verifies: `{"ok":true,"checked":24}`, `48`, `72`, `48`.

## Open decisions (user)

1. **`village_spawn_despawn` commands.** Its commands order and despawn "unit 6", the first spawned
   villager. With the well at SimId 6 the same JSON targets the well: the `Order` is rejected (recording
   fails) and `Despawn 6` would remove the well. Staged here: the commands remapped to unit 7 (same
   meaning as before). `content/village_content/tests/corpus_equivalence.rs` requires staged commands to be
   byte-identical to the original, so it **fails on this file** by design until the user decides:
   (a) accept the remap (then the equivalence check needs a documented exception, or simply passes once
   the swap removes `replays.next/`), or (b) give the well a different spawn order / keep the replay's
   meaning another way.
2. **Approve the re-baseline** (create `.claude/rebaseline-approved`), then swap.

## To finish the swap (needs user approval)

```sh
mv tests/replays.next/*.json tests/replays/ && rmdir tests/replays.next
mv content/village_content/tests/snapshots/golden__inspect_unit5_t600.snap.new \
   content/village_content/tests/snapshots/golden__inspect_unit5_t600.snap
mv content/village_content/tests/snapshots/golden__journal_seed1_first_200.snap.new \
   content/village_content/tests/snapshots/golden__journal_seed1_first_200.snap
mv apps/npcctl/tests/snapshots/golden__run_seed1_7200.snap.new \
   apps/npcctl/tests/snapshots/golden__run_seed1_7200.snap        # or: cargo insta accept
cargo test -p village_content --test golden    # writes t1800.snap.new; review, accept, repeat for t4000
cargo xtask verify
```

Until then `cargo xtask verify` fails on: `test` (`corpus::replay_corpus_verifies`,
`corpus_equivalence::staged_corpus_is_the_same_runs` (decision 1), `golden::golden_inspect_unit5`,
`golden::golden_journal_seed1_first_200`, `npcctl golden::golden_run_seed1_7200`,
`village_perception::without_the_plugin_the_village_is_unchanged` (it compares the content hash with
`tests/replays/village_seed1_3days.json`)), `replays` (the 4 village files) and `goldens` (pending
`.snap.new`).
