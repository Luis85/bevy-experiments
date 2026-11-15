# Island replays re-baseline evidence (staged, awaiting approval)

Tree: `7f08c855` (branch `claude/npc-framework-economy-review-7ec4cf`). Recorded on 2026-11-15 with this tree's
release `npcctl` (`cargo build --locked --release -p npcctl`; binary built 2026-11-15 08:49).

**This is the second staging.** The first staging was recorded on `68c2d1fd`. Commit `7f08c855` then changed the
island's hashed state: `SeasonLedger`'s `Totals` gained `children` and `came_of_age`, and `hash_totals` now hashes
both (`content/island_content/src/village/ledger.rs`). This is a hash-encoding change. It moves every island state
hash from the first checkpoint on, but not the content hash, because no registered name changed. On this tree the
first staging fails at tick 600:

```
old s7:  {"ok":false,"checked":1,"first_mismatch":[600,13914578685478396185,7969326067423200921]}
old s11: {"ok":false,"checked":1,"first_mismatch":[600,8516525264000701901,9710609323910368013]}
old s23: {"ok":false,"checked":1,"first_mismatch":[600,6824013137921424504,9007799059673750488]}
```

So the three staged files were recorded again and replaced. Neither staging was ever approved or swapped in.

## What changed and why

The island moved into its own crates (`island_content`, `island_view`, `island_game`; ADR 0044), and the older
island tiers are gone (scenarios `ai_village`, `living_village`, `island_colony` and their worlds). One
construction, `construction::shipped()`, now builds the island everywhere. The replay corpus has never held an
island replay: `tests/replays/` has only the 4 `testbed_*` and 4 `village_*` files. So this is a **first
baseline**, not a replacement. No existing oracle changes, and there is no "before" capture to compare against.

The island's content hash is `17177643955394559808`. It is the same in all three recordings, in both runs of each,
and in the first staging. During this pass the island's registered names and registrations changed several
times, so any island hash from before `68c2d1fd` would not match it:

- `1b589db5` (island split step 5): the island's own names, e.g. `TestbedUnit` -> `IslandUnit`, `TbIdling` ->
  `IslandIdling`, kind name `testbed` -> `island`, one idle tree in place of three. It also dropped `TbWalking` and
  the `TestbedDrive` command.
- `dbeb8272` (review fix pass): new sim components `SuspendedWork`, `Chatting`, `DayRhythm`, `Errand`; new sim
  resources `EditorMode`, `SeasonLedger`, `ActiveWorld`; new commands `SetEditorMode`, `ReplaceBehavior`.
- `a75c2269` (closing round 3): sim resource `StapleAdvances`.
- `68c2d1fd` (closing round 4): sim resource `FoodEaten`.
- `7f08c855` (final patch): no new name. The `SeasonLedger` encoding changed, which alters the state hashes only.

These replays pin the island as it ships at `7f08c855`. A later registration, encoding or behaviour change to the
island will need its own re-baseline by the same procedure.

## Checks that fail until the swap

- None expected. `7f08c855` changed `content/village_content/tests/corpus_equivalence.rs` to pass when nothing is
  staged for the village. Before that, it failed with "replays.next exists but is empty" while only island files
  were staged; the full verify on `68c2d1fd` showed this. The full verify on this tree confirms it now passes (see
  `target/npc-out/closing/final2/final-verify-report.md`).
- The testbed `corpus_equivalence` ignores island files. The village and testbed corpora are unchanged and all 8
  verify `"ok":true`. `cargo xtask verify --island` verifies the staged files as "staged, not approved yet".

## Commands

```sh
cargo build --locked --release -p npcctl
# recorded into target/ first (the hook blocks `--out tests/replays...`)
for S in 7 11 23; do
  target/release/npcctl replay record --scenario living_world --seed $S --ticks 24000 --checkpoint-every 600 \
    --out target/island-split/final2/island_living_world_s$S.json
  target/release/npcctl replay verify target/island-split/final2/island_living_world_s$S.json
  # a second, independent recording, compared byte for byte (cmp)
  target/release/npcctl replay record --scenario living_world --seed $S --ticks 24000 --checkpoint-every 600 \
    --out target/island-split/final2/second/island_living_world_s$S.json
done
cp target/island-split/final2/island_living_world_s{7,11,23}.json tests/replays.next/   # replaces the first staging
```

## Evidence

Verify outputs (this tree):

```
island_living_world_s7.json   {"ok":true,"checked":40,"first_mismatch":null}
island_living_world_s11.json  {"ok":true,"checked":40,"first_mismatch":null}
island_living_world_s23.json  {"ok":true,"checked":40,"first_mismatch":null}
```

Determinism: each seed was recorded twice in separate `npcctl` processes, and the two files are **byte-identical**
(`cmp` and SHA-256):

| Staged file | Bytes | SHA-256 | 2nd recording | 1st staging (68c2d1fd) |
|---|---|---|---|---|
| `island_living_world_s7.json` | 1272 | `088a0bb2bb2b5ee17ed864dbb5e60e2681d6ba0e455f32b55153e0b3ada9d30d` | identical | `dceb2df2…`, stale |
| `island_living_world_s11.json` | 1270 | `f6e0e6033efaf20c6074b7bec9a2566226361f2d49d48191773edc02551ce70a` | identical | `a8dcc402…`, stale |
| `island_living_world_s23.json` | 1265 | `a62434d27840dad3f25296ead1815e6944f6b37cccbb732f86267fe4bac1b818` | identical | `a53eb6c5…`, stale |

Each file: `"version":2`, `"scenario":"island:living_world"`, `"content_hash":17177643955394559808`,
`"ticks":24000`, `"commands":[]`, 40 checkpoints (every 600 ticks, 600..24000), `"villagers":1`, which is
`npcctl`'s default; the island's population comes from its construction. Recording, verifying and recording again
took 103-115 s per seed in release on this machine.

Not done: a Linux/Windows cross-check. The CI determinism job covers that; differing hashes across OSes would be
a determinism bug, not a re-baseline.

## Staged files (not applied)

| Oracle | Staged as | Change |
|---|---|---|
| (none: new) `tests/replays/island_living_world_s7.json` | `tests/replays.next/island_living_world_s7.json` | new, 40 checkpoints |
| (none: new) `tests/replays/island_living_world_s11.json` | `tests/replays.next/island_living_world_s11.json` | new, 40 checkpoints |
| (none: new) `tests/replays/island_living_world_s23.json` | `tests/replays.next/island_living_world_s23.json` | new, 40 checkpoints |

No id remap: there are no commands.

Note for the verify tiers: `xtask` lists `ISLAND_LONG_REPLAYS = ["island_seed7_1day"]`, which names none of these
files. So `--island` verifies all three, which took about 6.5 minutes in the `68c2d1fd` run. Adding them there would
leave them to the full gate only.

## Applying (owner)

Approve in chat, then in `npc-framework/`, in your own shell (docs/rebaselining.md §4):

```sh
for f in tests/replays.next/*.json; do
  case "$f" in *.idmap.json) ;; *) cp "$f" tests/replays/ ;; esac
done
rm -r tests/replays.next
cargo xtask verify            # must print "ok":true
```

Or create `.claude/rebaseline-approved` and tell the agent in chat to run exactly those commands, then delete the
marker. Commit the three replays with this note and add a `CHANGELOG.md` line.
