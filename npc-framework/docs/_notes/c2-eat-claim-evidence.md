# C2 eat-claim re-baseline evidence (staged, awaiting approval)

Plan Task C2 fixes a stale reservation: a finished meal kept its stockpile claim for one tick after it took
its food. This is a deliberate behaviour change, so it follows the P3 method
(`docs/_notes/p3-rebaseline-evidence.md`): capture before/after, explain every difference, stage the new
oracles, stop. Nothing protected was overwritten; the staged files are listed at the end.

## The change

Before: `eat_system` (`content/village_content/src/actions.rs`) finished a meal at tick T (need restored,
`TakeFood` effect written, leaf `Success`), but the claim on the stockpile was only released by `stop_eat`
when `think` stopped the leaf at tick T+1. During the rest of tick T (and in T+1 until the eater's own
think) the stockpile was claimed for food that was already gone. The property tests had to tolerate it
(claims with `Eat { remaining: 0 }` were not counted).

After: `eat_system` releases the claim in tick T, in `NpcSet::Actions`, right where it writes `TakeFood`:
finished meals are collected, sorted by `SimId` (query order is not `SimId` order) and released with a
journaled `Release { target, reason: Finished }`. `stop_eat` then finds no claim (its `release` is a no-op
and journals nothing). The property-test exception is removed (`check_world` counts every claim); 512
cases pass (`PROPTEST_CASES=512 cargo test -p village_content --test properties`).

Test: `content/village_content/tests/eat_claim.rs::eat_releases_claim_on_completion` (claim gone in the
tick the meal completes, `Release` before `TakeFood` in that tick, no second `Release` when the leaf
stops). It failed before the fix (`the claim is released in the tick the meal completes`).

## What may change, and why only that

1. **Journal:** each meal's `Release { Finished }` moves from tick T+1 (after `LeafStop EatFood`) to tick T
   (before `Effect TakeFood`). Seqs within those two ticks shift by one. Nothing else may move unless an
   agent's decision depends on the claim during that one tick.
2. **State hash:** `Reservations` is hashed, so the state *after tick T* differs (claim present vs gone);
   after tick T+1 both runs hold no claim again and the hashes agree. A checkpoint differs only if it is
   taken exactly after a meal-completion tick.
3. **Downstream (possible, not observed):** another agent thinking in tick T+1 before the eater (lower
   `SimId`) could be refused (`Reserve ok:false`) by the stale claim before; now it would get the claim.
   That needs two agents on one stockpile with food at most the outstanding claims. It does not happen in
   any captured run (single-villager scenarios; `village_n` gives each villager its own stockpile; in
   `village_spawn_despawn` and `village_orders` the shared stockpile never ran that low at those ticks).

## Method

- Pre binary: `git worktree add --detach <scratch>/c2-pre c1bdb8d`, `CARGO_TARGET_DIR=<scratch>/c2-pre-target
  cargo build --release -p npcctl` (worktree removed afterwards). Its `replay record` output is JSON-equal
  to all four committed corpus files.
- Mid binary: this tree with every other C2 item (profiler, ports, `JournalKind::NAMES`, journal-from-build,
  unified `Recorder`, `submit_deferred`, zstd) and C1's work in progress, but without the eat fix.
- Post binary: the same tree plus the eat fix.
- Each ran `scripts/p3/capture.sh <npcctl> <out> <copy of tests/replays>`: per corpus replay `run
  --journal`, `run --verbose --journal`, `replay record --checkpoint-every 100`, `inspect --all`; plus
  `run --ticks 7200 --journal` for seeds 1, 2, 3, 7, a 9-villager `village_n` run (seed 5, 4800 ticks),
  `inspect --unit 5` at ticks 600/1800/4000, `diverge --seed-a 1 --seed-b 2`.
- `scripts/p3/compare.py` (pre vs mid, pre vs post) and `scripts/c2/eat_diff.py` (pre vs post journals:
  finds the meal `Release`s on both sides, pairs them per unit and target, checks each moved exactly one
  tick earlier, removes them, drops `seq`, compares everything else).

## Results

**Pre vs mid (every C2 item except the eat fix):** `compare.py` verdict *all equivalent* over 38 files;
every journal, inspect, diverge and replay file is byte-identical, and every run summary is identical
including `final_hash` (the only text difference is the output-path field `journal`, normalized). So
the other C2 items change no sim result and no recorded replay.

**Pre vs post (eat fix):** `eat_diff.py` verdict *only meal-release timing differs*.

| journal | lines (pre = post) | meal `Release`s moved one tick earlier | everything else |
|---|---|---|---|
| seed1 (7200 ticks) | 448 | 5 of 5 | identical |
| seed2 | 476 | 5 of 5 | identical |
| seed3 | 454 | 4 of 4 | identical |
| seed7 | 451 | 4 of 4 | identical |
| n9 seed 5 (4800 ticks) | 2435 | 21 of 21 | identical |
| village_n_25_1day (+ verbose: 10 943) | 3383 | 18 of 18 | identical |
| village_orders (+ verbose: 895) | 349 | 5 of 5 | identical |
| village_seed1_3days (+ verbose: 1546) | 448 | 5 of 5 | identical |
| village_spawn_despawn (+ verbose: 3002) | 888 | 7 of 7 | identical |

No `Release` is unpaired and no other entry (including every `Reserve`, so no refusal changed) differs.
Example, seed 1, villager 5, stockpile 3 (pre, then post):

```text
pre   1688/0 Effect TakeFood   1689/2 LeafStop EatFood   1689/3 Release{3,Finished}   1689/4 RootDone
post  1688/0 Release{3,Finished}   1688/1 Effect TakeFood   1689/2 LeafStop EatFood   1689/3 RootDone
```

Other outputs:

| output | result |
|---|---|
| 13 run summaries (per-day meals, work shifts, ticks asleep, need ranges, aborts, leaf starts) | identical, `final_hash` included |
| `diverge --seed-a 1 --seed-b 2` | byte-identical |
| `inspect --unit 5` t600, t4000; 4 × `inspect --all` | byte-identical |
| `inspect --unit 5` t1800 | `recent_journal` only: the tick-1688 meal `Release` moved (as above) |
| replays `village_orders`, `village_seed1_3days`, `village_spawn_despawn` | byte-identical (all checkpoints) |
| replay `village_n_25_1day` | 1 of 24 checkpoints: tick 1700, `14900502127412976126` → `6944963594145370923` |

The one changed checkpoint is point 2 above: villager 20 finished its meal at stockpile 8 in tick 1699, so
after 1700 ticks the pre run still held the claim (released at 1700 in pre: `1700/3 Release{8}`; post:
`1699/0 Release{8}`, `1699/1 TakeFood`). The content hash is unchanged (`6937084237724115216`): no name was
added.

## Staged oracle files (not swapped in)

| oracle | staged as | change |
|---|---|---|
| `tests/replays/village_n_25_1day.json` | `tests/replays.next/village_n_25_1day.json` | checkpoint 1700 hash (one line) |
| `content/village_content/tests/snapshots/golden__inspect_unit5_t1800.snap` | `….snap.new` (insta) | `recent_journal`: meal `Release` at 1688/0 instead of 1689/3 |
| `content/village_content/tests/snapshots/golden__journal_seed1_first_200.snap` | `….snap.new` (insta) | the two meal `Release`s in the first 200 entries (ticks 1688, 2994) move one tick earlier |

Unchanged and passing: the other three corpus replays, `golden__inspect_unit5_t600`/`t4000` (t4000 is only
reached by the test once t1800 is accepted; it is byte-identical in the capture), and
`apps/npcctl/tests/snapshots/golden__run_seed1_7200.snap` (the run summary, `final_hash` included, does not
change). The staged replay was written by the post release binary (`npcctl replay record --seed 1
--scenario village_n --villagers 25 --commands tests/replays/village_n_25_1day.json --ticks 2400
--checkpoint-every 100 --pretty --out tests/replays.next/village_n_25_1day.json`) and verifies
(`{"ok":true,"checked":24}`); the original now fails at its tick-1700 checkpoint.
`content/village_content/tests/corpus_equivalence.rs` checks the staged file against the original (same
header, commands and checkpoint ticks; verifies); it was generalized to accept a version-2 original
(it used to require the P3 version-1 originals).

## To finish the swap (needs user approval)

```sh
mv tests/replays.next/village_n_25_1day.json tests/replays/ && rmdir tests/replays.next
mv content/village_content/tests/snapshots/golden__inspect_unit5_t1800.snap.new \
   content/village_content/tests/snapshots/golden__inspect_unit5_t1800.snap
mv content/village_content/tests/snapshots/golden__journal_seed1_first_200.snap.new \
   content/village_content/tests/snapshots/golden__journal_seed1_first_200.snap   # or: cargo insta accept
cargo xtask verify
```

Until then `cargo xtask verify` fails exactly on: `test` (`corpus::replay_corpus_verifies`,
`golden::golden_inspect_unit5`, `golden::golden_journal_seed1_first_200`), `replays`
(`village_n_25_1day.json`) and `goldens` (2 pending `.snap.new`).
