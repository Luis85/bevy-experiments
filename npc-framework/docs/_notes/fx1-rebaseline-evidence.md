# FX1 re-baseline evidence: testbed oracles (2026-10-24)

FX1 fixes the simulation bugs and cleanups in `final-review-sim.md`. They change the **content hash** and every
**state hash** of the Actor Testbed. They change **no behaviour** in the four testbed corpus runs: the journals are
byte-identical before and after. The village oracles are unchanged (the village registers none of the changed
names, and village `corpus`/`golden` tests pass).

## What changed and why

1. **The content hash changed.** `PerceptionPlugin` now registers a new hashed resource, `pending_noises`.
   - This is final review cleanup 5: a noise written after `NpcSet::Sense` waits in `Messages<Noise>` across the
     tick boundary and is heard in the next tick, so it is sim state.
   - A new registered name changes the content hash by design (ADR 0009).
   - Every testbed replay and session record therefore fails with `content hash mismatch` before any tick runs.
2. **Every state hash changed.** The `pending_noises` entry has a presence tag and a length, and it is written at
   every checkpoint, even when no noise is pending. Other hash additions only matter in states the corpus may
   reach:
   - `nav_queue` also hashes the paused search's whole progress, but only while a search is paused. Its encoding
     with no paused search is the same as before.
   - `crafting` also hashes the claimed `tools` and `(input, units)` (bug 6).
   - `Brain.tree` is **not** hashed by default; hashing it is opt-in through `register_brain_hash`, so the village
     is unaffected.
3. **No behaviour change in these runs.** None of the behaviour fixes triggers in the corpus scenarios:
   - nav restart/limit: no paused search is hit by an unrelated change, and no request reaches `SearchLimit`;
   - no nav agent has its own blocker;
   - no holder or carried item despawns;
   - no door closes on a unit that walked in that tick;
   - claimed craft inputs equal the re-picked ones;
   - no mid-tick submit happens;
   - formations and spacing stay in range;
   - no squad despawns;
   - `per_roll` keeps its old dice in every unambiguous case, and the smithy/vehicles scenarios use `automatic`
     detection.

## Evidence (journal and summary, before vs after)

- **Before:** `npcctl` built from a `git worktree` of `c4d0611` (own `CARGO_TARGET_DIR`).
- **After:** the same commit plus **only** the FX1 patch (FX1's files only, no FX2/FX3 changes), built the same
  way.

`npcctl journal` (full journal, `--journal-from-build true`):

| run | command | lines | before == after |
|---|---|---|---|
| default 2 days | `--scenario testbed --villagers 8 --ticks 4800` | 11127 | identical (sha256 `d2794c279ab025bf…`) |
| orders + craft | `--scenario testbed:smithy --villagers 3 --ticks 2400 --commands tests/replays/testbed_orders_craft.json` | 1251 | identical (`b9c1c75dbf6fb583…`) |
| vehicles | `--scenario testbed:vehicles --villagers 2 --ticks 1200 --commands tests/replays/testbed_vehicles.json` | 625 | identical (`40ae334e57680e48…`) |
| agent session | `--scenario <session scenario> --ticks 600 --commands <session replay commands>` | 401 | identical (`6660559c6eb97f6b…`) |

`npcctl run --scenario testbed --villagers 8 --ticks 4800`: the JSON summaries differ in exactly one field,
`final_hash` (`261736043509267895 → 10481591832455268674`). Every count, need, day, door, item and unit is the same.

## Staged files (not applied)

The regenerated files are staged. They were recorded with the FX1-only `npcctl` and use the same scenarios,
commands, villager counts and checkpoint ticks as the originals:

- `tests/replays.next/testbed_default_2days.json`
- `tests/replays.next/testbed_orders_craft.json`
- `tests/replays.next/testbed_vehicles.json`
- `tests/replays.next/testbed_agent_session.json`

In the staged files:

- Only `content_hash` and the checkpoint hashes differ from the originals.
- `npcctl replay verify` reports `ok` for each file: 48, 24, 12 and 6 checkpoints.
- `apps/npcctl/tests/snapshots/testbed__run_testbed_4800.snap.new` is the insta pending snapshot. Its only change
  is `final_hash`.

## Applying (user)

1. Create `.claude/rebaseline-approved`.
2. Move `tests/replays.next/*.json` over `tests/replays/` and accept the `.snap.new` (`cargo insta accept`).
3. Delete the marker.

**Caveat:** FX2/FX3 were changing files at the same time, including testbed content and agent glue. If their
changes alter the testbed's registered content or behaviour, re-record these files after integration, with the
same commands as above.
