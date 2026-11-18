# Island replays re-baseline evidence (third staging, approved for swap 2026-11-16)

Tree: `a9e5afa8` (branch `claude/npc-framework-economy-review-7ec4cf`). Recorded on 2026-11-16 with this tree's
release `npcctl` (`cargo build --locked --release -p npcctl`).

**This is the third staging.** The second staging (`7f08c855`, content hash `17177643955394559808`) went stale with
the S4 sim pass of this session:

- `98bab437` (tick cost): hash-neutral; the second staging still verified on it.
- `8a55a11c` (economy: 3-day food window, food use is what is eaten, standing staple work, help-haul leaf, the
  sociable trait): new tree leaf `life.haul help`, so the **content hash** changes, and every state hash.
- `a9e5afa8` (building materials between villages): state hashes only.

On this tree the second staging is refused before the first checkpoint:

```
old s7:  {"ok":false,"checked":0,"first_mismatch":null,"error":"content hash mismatch: replay ee63402a976dbf40, this build 4d0558f93ec4b1a9 (registered actions, conditions, trees, needs, components or commands differ)"}
old s11: (same error)
old s23: (same error)
```

So the three staged files were recorded again and replaced. The new content hash is `5549939943327183273`
(`4d0558f93ec4b1a9`). The user approved the swap of these files in this session (handover task 1: "they must
verify and re-record byte-identically on the tree you swap on").

## What changed and why

The island moved into its own crates (`island_content`, `island_view`, `island_game`; ADR 0044), and the older
island tiers are gone (scenarios `ai_village`, `living_village`, `island_colony` and their worlds). One
construction, `construction::shipped()`, now builds the island everywhere. The replay corpus has never held an
island replay: `tests/replays/` has only the 4 `testbed_*` and 4 `village_*` files. So this is a **first
baseline**, not a replacement. No existing oracle changes, and there is no "before" capture to compare against.

The island's content hash at the second staging was `17177643955394559808` (now `5549939943327183273`, same in all three recordings and both runs of each). It was the same in all three recordings, in both runs of each,
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

These replays pin the island as it ships at `a9e5afa8`. A later registration, encoding or behaviour change to the
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
    --out target/island-split/final3/island_living_world_s$S.json
  target/release/npcctl replay verify target/island-split/final3/island_living_world_s$S.json
  # a second, independent recording, compared byte for byte (cmp)
  target/release/npcctl replay record --scenario living_world --seed $S --ticks 24000 --checkpoint-every 600 \
    --out target/island-split/final3/second/island_living_world_s$S.json
done
cp target/island-split/final3/island_living_world_s{7,11,23}.json tests/replays.next/   # replaces the second staging
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

| Staged file | Bytes | SHA-256 | 2nd recording | 2nd staging (7f08c855) |
|---|---|---|---|---|
| `island_living_world_s7.json` | 1269 | `dea11e348f20e3fc7013e974ead79762e7f6313e137c9dd755e8af7a3c531391` | identical | `088a0bb2…`, stale |
| `island_living_world_s11.json` | 1266 | `94e59ac361a809c8fa8167415572c22da8a1159d4287355e9b74453f1a7efff4` | identical | `f6e0e603…`, stale |
| `island_living_world_s23.json` | 1260 | `f501f29ddd56ee49eed9facfddabdebf1133b878f848959d7338bafc74970af6` | identical | `a62434d2…`, stale |

Each file: `"version":2`, `"scenario":"island:living_world"`, `"content_hash":5549939943327183273`,
`"ticks":24000`, `"commands":[]`, 40 checkpoints (every 600 ticks, 600..24000), `"villagers":1`, which is
`npcctl`'s default; the island's population comes from its construction. Recording, verifying and recording again
took 103-115 s per seed in release on this machine.

Not done: a Linux/Windows cross-check. The CI determinism job covers that; differing hashes across OSes would be
a determinism bug, not a re-baseline.

## Staged files (swapped in on 2026-11-16)

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

## Re-baseline 2026-11-17: residents notice the player (backlog item 3)

Approved by the owner in chat for one swap per hash-changing backlog item. The new condition
`life.player_within` and leaf `life.greet` change the content hash `5549939943327183273` (`4d0558f93ec4b1a9`)
-> `16261449980358703144` (`e1ac4699ac103828`), and every state hash. The old files are refused:
`content hash mismatch: replay 4d0558f93ec4b1a9, this build e1ac4699ac103828`.

Recorded with the commands above into `target/island-split/b3/` (release `npcctl` from this tree), each seed twice,
byte-identical (`cmp`); each verifies `{"ok":true,"checked":40,"first_mismatch":null}` before and after the swap.
Marker created and deleted for this swap; `tests/replays.next` removed.

| File | SHA-256 |
|---|---|
| `island_living_world_s7.json` | `5f2b6c22a63f3da64de372308e23bfad14272b9737abeec5d702428e960db3fe` |
| `island_living_world_s11.json` | `3b7a18a9f5ddaf36e9be00c2db3ce16d7ed4c2d41778986c2da71733cc373467` |
| `island_living_world_s23.json` | `343ad09bc218d6c0c2b9191182e30c1b298e4cba65af874837531562d7df22e3` |

## Re-baseline 2026-11-18: second crop and livestock (backlog item 6)

Same approval (one swap per hash-changing backlog item). Turnips, hen houses, the herder trade, the work-fits-before-rest
rule, the granary-room fix and the advance recipe check change the content hash `16261449980358703144`
(`e1ac4699ac103828`) -> `8404440510714457534` (`74a2921da909adbe`) and every state hash. Old files refused:
`content hash mismatch: replay e1ac4699ac103828, this build 74a2921da909adbe`.

Recorded into `target/island-split/b6/`, each seed twice, byte-identical; each verifies
`{"ok":true,"checked":40,"first_mismatch":null}` before and after the swap. Marker created and deleted.

| File | SHA-256 |
|---|---|
| `island_living_world_s7.json` | `c46829b6fdf731bd2a2a8959988b0e2061be7f6eb759c8b3547d9973b9158f3c` |
| `island_living_world_s11.json` | `1851a39258f8f9686397220760cb6853db48f09d2249384fd18384b3d3f4806d` |
| `island_living_world_s23.json` | `88c3f0c7818536adb11facd9931044520622e963e4b0276aab9bb267da3ead63` |

## Re-baseline 2026-11-18: several civic projects per settlement (backlog item 7)

Same approval. Content hash unchanged (`8404440510714457534`); state hashes change from the first checkpoint
(old s7: `{"ok":false,"checked":1,"first_mismatch":[600,6339958633871876553,12402308894804814256]}`).
Recorded into `target/island-split/b7/`, each seed twice, byte-identical; each verifies `"ok":true`, 40 checkpoints,
before and after the swap. Marker created and deleted.

| File | SHA-256 |
|---|---|
| `island_living_world_s7.json` | `e7c08c1cf59eb1aacbdb887eda05de7d29202f6f09f9d5d477b34280656c2e72` |
| `island_living_world_s11.json` | `058f35ad5c2bb7f8592dd0dc8fb31252ae2c135bd3584206b67dd1115a4f8a73` |
| `island_living_world_s23.json` | `5922dd4dc8f753c969266dbc9959e473bc8975ab1071e3edf922f52155f82baf` |

## Re-baseline 2026-11-18: wider GOAP (backlog item 8)

Same approval. New leaf parameter `life.pursue_goal elsewhere` and the planner changes: content hash
`8404440510714457534` (`74a2921da909adbe`) -> `17349830680797832784` (`f0c6fb14b220e650`), and every state hash.
Old files refused: `content hash mismatch: replay 74a2921da909adbe, this build f0c6fb14b220e650`.
Recorded into `target/island-split/b8/`, each seed twice, byte-identical; each verifies `"ok":true`, 40 checkpoints,
before and after the swap. Marker created and deleted.

| File | SHA-256 |
|---|---|
| `island_living_world_s7.json` | `c0ddbc692d16f1328a3a40fbe1c55f1476ead8d9caa81e9d68979f8dc16be0ed` |
| `island_living_world_s11.json` | `a3e82fff0cdfa89644232ec32499349b680c4fe5df6bce881a2bb5d7bf298c26` |
| `island_living_world_s23.json` | `c3ab5f9a1eb42fdc4e001218f957c832f45d894b0b178dc018d4faaa6613b6c1` |
