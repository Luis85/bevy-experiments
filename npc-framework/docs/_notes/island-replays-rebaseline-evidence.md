# Island replays re-baseline evidence (staged, awaiting approval)

Tree: `68c2d1fd` (branch `claude/npc-framework-economy-review-7ec4cf`) plus one lint fix in
`content/island_view/src/keymap.rs`. The fix is view code that `npcctl` does not link, so it cannot reach a hash.
Recorded on 2026-11-15 with this tree's release `npcctl` (`cargo build --locked --release -p npcctl`).

## What changed and why

The island moved into its own crates (`island_content`, `island_view`, `island_game`; ADR 0044), and the older
island tiers are gone (scenarios `ai_village`, `living_village`, `island_colony` and their worlds). One
construction, `construction::shipped()`, now builds the island everywhere. The replay corpus has never held an
island replay: `tests/replays/` has only the 4 `testbed_*` and 4 `village_*` files. So this is a **first
baseline**, not a replacement. No existing oracle changes, and there is no "before" capture to compare against.

The island's content hash is `17177643955394559808`. It is the same in all three recordings and in both runs of
each. During this pass the island's registered names and registrations changed several times, so any earlier
island hash would not match it:

- `1b589db5` (island split step 5): the island's own names, e.g. `TestbedUnit` -> `IslandUnit`, `TbIdling` ->
  `IslandIdling`, kind name `testbed` -> `island`, one idle tree in place of three. It also dropped `TbWalking` and
  the `TestbedDrive` command.
- `dbeb8272` (review fix pass): new sim components `SuspendedWork`, `Chatting`, `DayRhythm`, `Errand`; new sim
  resources `EditorMode`, `SeasonLedger`, `ActiveWorld`; new commands `SetEditorMode`, `ReplaceBehavior`.
- `a75c2269` (closing round 3): sim resource `StapleAdvances`.
- `68c2d1fd` (closing round 4): sim resource `FoodEaten`.

These replays pin the island as it ships at `68c2d1fd`. A later registration or behaviour change to the island
will need its own re-baseline by the same procedure.

## Checks that fail until the swap

- `content/village_content/tests/corpus_equivalence.rs` `staged_corpus_is_the_same_runs`: probably red while
  only island files are staged. The test drops `island_*` files from `tests/replays.next/` and then asserts
  `!names.is_empty()` ("replays.next exists but is empty"). The full gate's `test` step confirms or rules this
  out (see the closing report). This is a defect in that test's early return, not in the staged files. The
  swap removes `tests/replays.next/`, which clears it.
- Nothing else should change. The testbed `corpus_equivalence` ignores island files and has no empty-check. The
  village and testbed corpora are unchanged and all 8 verify `"ok":true`. `cargo xtask verify --island` verifies
  the staged files as "staged, not approved yet".

## Commands

```sh
cargo build --locked --release -p npcctl
# recorded into target/ first (the hook blocks `--out tests/replays...`)
for S in 7 11 23; do
  target/release/npcctl replay record --scenario living_world --seed $S --ticks 24000 --checkpoint-every 600 \
    --out target/island-split/final/island_living_world_s$S.json
  target/release/npcctl replay verify target/island-split/final/island_living_world_s$S.json
  # a second, independent recording, compared byte for byte
  target/release/npcctl replay record --scenario living_world --seed $S --ticks 24000 --checkpoint-every 600 \
    --out target/island-split/final/second/island_living_world_s$S.json
done
mkdir -p tests/replays.next
cp target/island-split/final/island_living_world_s{7,11,23}.json tests/replays.next/
```

## Evidence

Verify outputs:

```
island_living_world_s7.json   {"ok":true,"checked":40,"first_mismatch":null}
island_living_world_s11.json  {"ok":true,"checked":40,"first_mismatch":null}
island_living_world_s23.json  {"ok":true,"checked":40,"first_mismatch":null}
```

Determinism: each seed was recorded twice in separate `npcctl` processes, and the two files are **byte-identical**
(SHA-256 equal):

| Staged file | Bytes | SHA-256 | 2nd recording |
|---|---|---|---|
| `island_living_world_s7.json` | 1272 | `dceb2df20af7b88c5089d53990355c892101eb68eb8adfdd60722a0e612ea5c2` | identical |
| `island_living_world_s11.json` | 1268 | `a8dcc402bc422bb407ee58f88841c0e6c2d8f4ef3013ad0cab49d9d15d223b3f` | identical |
| `island_living_world_s23.json` | 1265 | `a53eb6c5daaff443c5f6cb34846de2517e746e2d9dec53f598ad05a55cac3984` | identical |

Each file: `"version":2`, `"scenario":"island:living_world"`, `"content_hash":17177643955394559808`,
`"ticks":24000`, `"commands":[]`, 40 checkpoints (every 600 ticks, 600..24000), `"villagers":1`, which is
`npcctl`'s default; the island's population comes from its construction. Recording takes about 40-50 s per seed
and verifying about the same, in release on this machine.

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
files. So `--island` will verify all three, about 2 minutes in total. Adding them there would leave them to the
full gate only.

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
