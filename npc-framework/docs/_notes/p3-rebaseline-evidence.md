# P3 re-baseline evidence (staged, awaiting approval)

Task P3 (crate split, stable-name registration, `SimHash` registry) changes the **state-hash encoding**
once (ADR 0009). It does not change behaviour. This note is the proof the plan asks for (Task P3, step 3)
and lists exactly which oracle files are staged. Nothing protected was overwritten: the regenerated corpus
is in `tests/replays.next/`, the regenerated golden is an insta `.snap.new`, and the original oracles are
untouched.

## Why the hash changes (and nothing else does)

1. **Registry order.** `state_hash` now writes registered components in *name* order (`active_leaf`,
   `activity`, `blackboard`, `brain_state`, `eat`, `idle`, `move_to`, `needs`, `order`, `order_move`, `pos`,
   `sleep`, `stockpile`, `work`), each with a presence tag; before, a hand-written list in
   `crates/npc_sim/src/hash.rs` used a fixed ad-hoc order (`pos`, `needs`, `activity`, ...) and wrote the
   `OrderMove` marker as a raw byte.
2. **Action ids.** `ActiveLeaf.action` is hashed; ids are now frozen sorted by name (`DoWork`=0 ...
   `SleepInBed`=9) instead of registration order (`ReserveFood`=0 ... `FollowOrder`=9).
3. **Needs.** `Needs` is data-driven: a length-prefixed list in need-id order (`Energy`, `Hunger`) instead
   of the fixed `hunger, energy` struct.
4. **Replay format 2** adds `content_hash` (hash of every registered name + tree content hashes).

Everything observable stays the same: the journal (every decision, need level change, reservation,
effect, command, spawn/despawn), inspect snapshots, run summaries and divergence reports.

## Method

1. Pre-split binary: `git worktree add --detach <scratch>/pre-split e9b5f71`, built with its own target dir
   (`CARGO_TARGET_DIR=<scratch>/pre-target cargo build --release -p npcctl`). Sanity check: its
   `replay record` output for every corpus header + commands equals the committed corpus files
   (JSON-equal, all four).
2. Post-split binary: this tree, `cargo build --release -p npcctl`.
3. Both binaries ran `scripts/p3/capture.sh <npcctl> <out> <copy of tests/replays>`: for every corpus
   replay (seed, scenario, villagers, ticks, commands from the file) `run --journal` (JSONL),
   `run --verbose --journal` (with every `CondEval`), `replay record --checkpoint-every 100` and
   `inspect --all` at the last tick; plus `run --ticks 7200 --journal` for seeds 1, 2, 3, 7, a 9-villager
   `village_n` run (seed 5, 4800 ticks), `inspect --unit 5` at ticks 600/1800/4000 (the golden ticks), and
   `diverge --seed-a 1 --seed-b 2 --ticks 2400`.
4. `python scripts/p3/compare.py <pre> <post>`: journals must be byte-identical; JSON outputs
   byte-identical, or identical after removing only `final_hash` (run summaries) and `version`,
   `content_hash` and the checkpoint hash values (replays; checkpoint ticks kept).
5. `print_three_day_timeline` SUMMARY lines (pre: `cargo test -p npc_sim --test villager ...`, post:
   `cargo test -p village_content --test villager ...`): compared as text.

## Results

`compare.py` verdict: **all equivalent** (38 files).

| Output | Result |
|---|---|
| 13 journals (4 corpus runs, 4 verbose corpus runs, seeds 1/2/3/7, n=9 seed 5), 25 718 lines in total | byte-identical |
| 3 `inspect --unit 5` (t600, t1800, t4000) and 4 `inspect --all` | byte-identical |
| `diverge --seed-a 1 --seed-b 2` (first divergent tick 666, component diff) | byte-identical |
| 13 run summaries (per-day meals, work shifts, ticks asleep, need ranges, aborts, leaf starts) | identical except `final_hash` |
| 4 re-recorded corpus replays | identical except `version` 1→2, `content_hash`, checkpoint hash values |

Test-suite goldens: `golden_inspect_unit5` and `golden_journal_seed1_first_200` pass unchanged against
the existing snapshots (moved with their test to `content/village_content/tests/snapshots/`, content
untouched). `golden_run_seed1_7200` differs only in `final_hash`.

SUMMARY lines of `print_three_day_timeline`: identical (see the last section).

### Old → new checkpoint hash of each corpus replay (final tick)

Content hash of this build: `6937084237724115216`.

| replay | checkpoints | final tick | v1 hash | v2 hash |
|---|---|---|---|---|
| village_n_25_1day | 24 | 2400 | 13886179512587426278 | 5944504627119479413 |
| village_orders | 48 | 4800 | 18361407838058466037 | 10664135355296312218 |
| village_seed1_3days | 72 | 7200 | 7367151541562869069 | 14375080320772130166 |
| village_spawn_despawn | 48 | 4800 | 12052698805299998978 | 6007703804331993291 |

## Staged oracle files (not swapped in)

| Oracle | Staged as | Change |
|---|---|---|
| `tests/replays/village_n_25_1day.json` | `tests/replays.next/village_n_25_1day.json` | version 2, content_hash, 24 checkpoint hashes |
| `tests/replays/village_orders.json` | `tests/replays.next/village_orders.json` | version 2, content_hash, 48 checkpoint hashes |
| `tests/replays/village_seed1_3days.json` | `tests/replays.next/village_seed1_3days.json` | version 2, content_hash, 72 checkpoint hashes |
| `tests/replays/village_spawn_despawn.json` | `tests/replays.next/village_spawn_despawn.json` | version 2, content_hash, 48 checkpoint hashes |
| `apps/npcctl/tests/snapshots/golden__run_seed1_7200.snap` | `….snap.new` (written by insta) | `final_hash` 7367151541562869069 → 14375080320772130166 |

Staged files were produced with the post-split release binary:
`npcctl replay record --seed S --scenario X --villagers V --commands tests/replays/<name>.json --ticks T
--checkpoint-every 100 --pretty --out tests/replays.next/<name>.json` (header values from each original),
and each verifies (`npcctl replay verify tests/replays.next/<name>.json` → `ok:true`).
`content/village_content/tests/corpus_equivalence.rs` checks, while `replays.next/` exists, that every
staged replay has the same seed, scenario, villagers, ticks, commands (JSON-equal) and checkpoint ticks
as its original, is version 2 and verifies; and that the v1 original still loads and is reported as
"version 1 ... re-record" by `verify`.

## To finish the swap (needs user approval: `.claude/rebaseline-approved`)

```sh
mv tests/replays.next/*.json tests/replays/ && rmdir tests/replays.next
mv apps/npcctl/tests/snapshots/golden__run_seed1_7200.snap.new \
   apps/npcctl/tests/snapshots/golden__run_seed1_7200.snap      # or: cargo insta accept
cargo xtask verify
```

Until then `cargo xtask verify` fails exactly on: `test` (`corpus::replay_corpus_verifies`,
`golden::golden_run_seed1_7200`), `replays` (4 v1 files) and `goldens` (1 pending `.snap.new`).

## print_three_day_timeline SUMMARY lines (10 days)

Pre-split (`cargo test -p npc_sim --test villager print_three_day_timeline -- --ignored --nocapture`) and
post-split (`cargo test -p village_content --test villager ...`) print the same 10 lines (`diff`: no output):

```text
SUMMARY day 1: meals 1, work shifts 2, ticks asleep 582, min hunger 32%, min energy 27%
SUMMARY day 2: meals 1, work shifts 2, ticks asleep 632, min hunger 27%, min energy 29%
SUMMARY day 3: meals 3, work shifts 1, ticks asleep 379, min hunger 20%, min energy 19%
SUMMARY day 4: meals 1, work shifts 2, ticks asleep 445, min hunger 34%, min energy 16%
SUMMARY day 5: meals 2, work shifts 1, ticks asleep 575, min hunger 30%, min energy 27%
SUMMARY day 6: meals 1, work shifts 2, ticks asleep 477, min hunger 26%, min energy 23%
SUMMARY day 7: meals 3, work shifts 1, ticks asleep 483, min hunger 17%, min energy 23%
SUMMARY day 8: meals 1, work shifts 2, ticks asleep 469, min hunger 33%, min energy 22%
SUMMARY day 9: meals 2, work shifts 1, ticks asleep 587, min hunger 31%, min energy 27%
SUMMARY day 10: meals 1, work shifts 2, ticks asleep 465, min hunger 26%, min energy 22%
```
