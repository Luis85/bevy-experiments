# Re-baselining the oracles

The replay corpus (`tests/replays/*.json`), the insta snapshots (`**/snapshots/*.snap`) and the hashes that
tests pin are **oracles**. Fix the code when they fail. Re-baseline only when a change is *meant* to change
behaviour or the hash encoding: a new mechanic, a new registered name, a tuned constant, a new scenario
entity. There is one procedure, **stage → evidence → user approval → swap**:

| Step | Who | What |
|---|---|---|
| 1. Stage | agent | write the new oracles **next to** the old ones (`tests/replays.next/`, `*.snap.new`); never over them |
| 2. Evidence | agent | before/after captures from a pre-change worktree, plus an evidence note in `docs/_notes/` |
| 3. Ask | agent | stop and ask the user to approve, linking the note; the agent's verify is red until the swap |
| 4. Swap | **user** | approves, then runs the swap commands (§4) in their own shell; or creates `.claude/rebaseline-approved` and tells the agent in chat to run exactly those commands |
| 5. Close | user | `cargo xtask verify` is green again; the user deletes `.claude/rebaseline-approved` |

Agents never overwrite an oracle on their own initiative, and they never create or delete the marker; the
`protect_goldens.sh` hook blocks both (`docs/debugging.md` §8.2). Approval is given by the user, in chat or with
the marker, for one staged set. It does not carry over to later changes. The same procedure applies whether the
change is an encoding change with identical journals (P3, FX1) or a real behaviour change (C2, a new mechanic).
Past runs: `docs/_notes/p3-rebaseline-evidence.md`, `c2-eat-claim-evidence.md`, `fx1-rebaseline-evidence.md`.

- [1. What fails on a re-baseline](#1-what-fails-on-a-re-baseline)
- [2. Stage](#2-stage)
- [3. Evidence](#3-evidence)
- [4. Swap (user)](#4-swap-user)
- [5. SimIds moved: the id remap](#5-simids-moved-the-id-remap)

## 1. What fails on a re-baseline

The checks below are expected to fail until the swap (the `corpus_equivalence` row only when a staged file is
wrong). Name each one that fails in the evidence note. Anything
**not** on this list that fails is a bug in the change, not part of the re-baseline.

| Check | Fails when | What to stage |
|---|---|---|
| `content/village_content/tests/corpus.rs` `replay_corpus_verifies` | any village registered name, hash encoding or behaviour changes | `tests/replays.next/village_*.json` |
| `content/testbed_content/tests/corpus.rs` `testbed_replay_corpus_verifies` | the same for the testbed (including `testbed_agent_session.json`) | `tests/replays.next/testbed_*.json` |
| `cargo xtask verify` step `replays` (full verify only) | the same, for every `tests/replays/*.json` through `npcctl replay verify` | (as above) |
| `content/village_content/tests/village_perception.rs` `without_the_plugin_the_village_is_unchanged` | the village content hash changes: it pins the hash recorded in `tests/replays/village_seed1_3days.json` | nothing; it passes again after the swap |
| `content/village_content/tests/golden.rs` `golden_inspect_unit5_t600` / `_t1800` / `_t4000`, `golden_journal_seed1_first_200` | villager 5's inspected state or the first 200 journal entries change | `content/village_content/tests/snapshots/*.snap.new` |
| `apps/npcctl/tests/golden.rs` `golden_run_seed1_7200` | the `npcctl run` summary changes (even only `final_hash`) | `apps/npcctl/tests/snapshots/golden__run_seed1_7200.snap.new` |
| `apps/npcctl/tests/testbed.rs` `golden_run_testbed_4800` | the testbed run summary changes | `apps/npcctl/tests/snapshots/testbed__run_testbed_4800.snap.new` |
| `cargo xtask verify` step `goldens` | any `*.snap.new` exists, i.e. as soon as you staged one: this is the signal | (as above) |
| `content/*/tests/corpus_equivalence.rs` | a staged replay is not the same run as its original (other seed, length, commands without a remap, checkpoint ticks) or does not verify | fix the staged file |
| layout pins: `villager.rs` `spawn_village_layout`, `replay.rs` `new_sim_app_scenarios`, the reference `Model` in `properties.rs` | a scenario entity is added or reordered | edit these tests with the layout change and say so in the note (they are tests, not oracles) |

Never re-baseline the CI determinism job: hashes that differ between Linux and Windows are a determinism bug
(`/debug-desync`). Each golden snapshot has its own test, so one failure does not hide the others: every changed snapshot gets its
`.snap.new` in a single run. `*.proptest-regressions` files are oracles too. They store generated inputs, not
hashes, so they never need re-baselining.

## 2. Stage

Run everything from `npc-framework/`. Build the changed `npcctl` once: `cargo build --release -p npcctl`.

### 2.1 Replays → `tests/replays.next/`

```sh
sh scripts/rebaseline/stage_replays.sh target/release/npcctl                       # every corpus file
sh scripts/rebaseline/stage_replays.sh target/release/npcctl village_orders testbed_vehicles   # or some
```

For each `tests/replays/<name>.json` the script records `tests/replays.next/<name>.json` with the original's
seed, scenario, villager count, length, commands and checkpoint interval, then verifies it. A session record is
re-recorded from its scenario document. It runs, per file:

```sh
npcctl replay record --seed S --scenario SC --villagers V --ticks T --checkpoint-every N \
  --commands tests/replays/<name>.json --out target/rebaseline/stage/<name>.json
cp target/rebaseline/stage/<name>.json tests/replays.next/<name>.json
```

Record into `target/` first: the hook blocks `--out tests/replays...` (it matches `tests/replays.next` too).
Stage only the files whose checks fail. `corpus_equivalence.rs` refuses a staged file without any checkpoint
change. Check the staged set with:

```sh
cargo test -p village_content --test corpus_equivalence
cargo test -p testbed_content --test corpus_equivalence
```

### 2.2 Snapshots → `*.snap.new`

insta writes `<snapshot>.snap.new` next to the `.snap` when an assertion fails, unless updates are off.
`cargo xtask verify` and CI set `INSTA_UPDATE=no`, and insta also writes nothing when `CI` is set. So run the
golden tests yourself, with `CI` unset:

```sh
INSTA_UPDATE=new cargo test -p village_content --test golden
INSTA_UPDATE=new cargo test -p npcctl --test golden --test testbed
git diff --no-index content/village_content/tests/snapshots/golden__inspect_unit5_t600.snap \
  content/village_content/tests/snapshots/golden__inspect_unit5_t600.snap.new        # review each one
```

`INSTA_UPDATE=new` (or the default `auto` with `CI` unset) only writes `.snap.new`. `INSTA_UPDATE=always`,
`INSTA_FORCE_PASS`, `cargo insta accept` and `cargo insta review` would overwrite the oracle; the hook blocks
them.

### 2.3 Pinned hashes

`without_the_plugin_the_village_is_unchanged` reads its hash from the corpus, so it passes again once the staged
`village_seed1_3days.json` is swapped in. Stage nothing for it. Do not add a test that hard-codes a hash value;
read it from a corpus file instead.

## 3. Evidence

Compare the old build with the new one on the same inputs, using a separate worktree (never `git stash`):

```sh
git worktree add --detach target/rebaseline/pre-src HEAD    # the commit before your change (HEAD if uncommitted)
CARGO_TARGET_DIR=target/pre cargo build --release -p npcctl --manifest-path target/rebaseline/pre-src/npc-framework/Cargo.toml
cargo build --release -p npcctl
mkdir -p target/rebaseline/corpus && cp tests/replays/*.json target/rebaseline/corpus/   # a copy: capture reads it
sh scripts/p3/capture.sh target/pre/release/npcctl target/rebaseline/cap-pre  target/rebaseline/corpus
sh scripts/p3/capture.sh target/release/npcctl     target/rebaseline/cap-post target/rebaseline/corpus
python scripts/p3/compare.py target/rebaseline/cap-pre target/rebaseline/cap-post
git worktree remove target/rebaseline/pre-src
```

`capture.sh` writes, per corpus file, the journal (plain and verbose), the run summary, a re-recorded replay and
`inspect --all`. It also runs four village seeds, a 9-villager village, `inspect --unit 5` at the golden ticks and
a `diverge` run. Keep the output directory names `cap-pre` and `cap-post`: `compare.py` removes them from the
file paths inside the summaries. `compare.py` passes when the journals are byte-identical and the JSON is identical apart from the
hashes. Use that verdict for an encoding-only change. For a behaviour change it lists the files that differ:
summarise those differences (per-day counts from the run summaries, the first differing journal entries per
unit) and argue that they are the intended ones. Extra checks for a mechanic: `npcctl run --seed 1 --ticks 7200
--pretty` before and after, and `npcctl journal .. --kind LeafStart,Abort --grep <Action>` (no flip-flop).

Write `docs/_notes/<task>-rebaseline-evidence.md`:

```markdown
# <task> re-baseline evidence (staged, awaiting approval)

## What changed and why
- The change (one paragraph), and why the oracles must change: a new registered name (content hash), a hash
  encoding change, or intended behaviour (what a unit now does differently).

## Checks that fail until the swap
- `<test or verify step>`: <why> (from docs/rebaselining.md §1; nothing else fails).

## Evidence (before vs after)
- Before: `npcctl` from a worktree of `<commit>`; after: this tree. `capture.sh` + `compare.py` verdict.
- Table: run | command | before | after (journal lines/sha, per-day meals/sleep/aborts, final_hash).
- For a behaviour change: the differences and why each is intended; no starvation, no flip-flop.

## Staged files (not applied)
| Oracle | Staged as | Change |
|---|---|---|
| `tests/replays/<name>.json` | `tests/replays.next/<name>.json` | content_hash, N checkpoint hashes |
| `<..>/snapshots/<snap>.snap` | `<..>.snap.new` | `final_hash` only / <fields> |
| (id remap) | `tests/replays.next/<name>.idmap.json` | ids X→Y in `unit`, `units` (§5) |

## Applying (user)
The swap commands of docs/rebaselining.md §4.
```

Then stop and ask the user to approve. Say which checks are red and why. The Stop hook's verify will report
`ok:false` until the swap, and that is expected: say so instead of trying to make it pass.

## 4. Swap (user)

The user runs these in `npc-framework/`, in their own shell (the hook does not apply there). Alternatively they
create `.claude/rebaseline-approved` and tell the agent in chat to run exactly these commands. Without the
marker and that instruction the agent must not run them (the hook blocks the writes it can recognise, such as
the `cp` into `tests/replays/`).

```sh
# replays (an id remap file is evidence, not a replay: it is not copied)
for f in tests/replays.next/*.json; do
  case "$f" in *.idmap.json) ;; *) cp "$f" tests/replays/ ;; esac
done
rm -r tests/replays.next
# snapshots (or `cargo insta accept` with cargo-insta installed)
find . -path ./target -prune -o -name '*.snap.new' -print | while read -r f; do mv "$f" "${f%.new}"; done
cargo xtask verify            # must print "ok":true
rm .claude/rebaseline-approved   # the user, if they created it
```

Commit the swapped oracles together with the change and the evidence note, and add a `CHANGELOG.md` line.

## 5. SimIds moved: the id remap

`SimId`s are allocated 1, 2, 3, ... in spawn order: first the scenario's entities, then runtime spawns
(`SpawnVillager`, a scenario's `actors` after its world) get the next ids. Commands in replays name units by id
(`"unit": 5`, `"units": [33]`, `"stockpile": 3`). So:

- **Spawn new scenario entities after the existing ones** (at the end of `spawn_plot`, after the walls and
  furnishing of a testbed layout). Every existing scenario id then stays the same.
- **Ids after it still move.** A new scenario entity shifts every *later* id up by one: runtime-spawned villagers
  (`village_spawn_despawn`: the `SpawnVillager` villager is 6, a well spawned after the villager makes it 7),
  scenario actors spawned after the world, and later plots of `village_n`.
- Replay command files keep their numbers (they are oracles). A staged replay whose commands must name the moved
  ids is the same run *with remapped ids*. Write the remap next to it, by hand,
  as `tests/replays.next/<name>.idmap.json`:

```json
{"reason": "the well spawns after the villager, so the SpawnVillager villager is 7, not 6",
 "fields": ["unit", "units"], "ids": {"6": 7}}
```

`corpus_equivalence.rs` (village and testbed) applies it to the original's commands before comparing. Every
integer (or integer in an array) under a listed key, at any depth, that is a key of `ids` is replaced. The staged
commands must then equal the remapped originals exactly. Record the staged replay with the remapped commands:
copy the original's `commands` into a file, apply the same remap, and pass it as `--commands` to `replay record`
(`stage_replays.sh` never edits commands). List the remap in the evidence note.

Tests look ids up instead of hard-coding them (`VillageIds`, `TestbedIds::id("<name>")`, `next_sim_id(&app)` in
`content/village_content/tests/common/mod.rs` for the id the next spawn gets), so they survive such a change.
