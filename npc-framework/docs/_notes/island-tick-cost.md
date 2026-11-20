# Island tick cost: S4 lookups (bench note)

Before: release `npcctl` of the tree at `bc15e19a` plus the S4 profiler probes only (no behaviour change).
After: the same plus the S4 changes below. Both are hash-identical (see "Hash neutrality").
Raw JSON: `target/npc-out/s4/bench-before/`, `target/npc-out/s4/bench-after/` (with `*.procs-start.txt` /
`*.procs-end.txt`: the processes running when each pair started and ended). Probe profiles:
`target/npc-out/s4/profile-before/`, `profile-after/`, `profile-dev/`.

Command (each seed and warm-up, as in `closing/tick-cost.md`):
`npcctl bench --scenario island:living_world --seed S --warmup T --ticks 2000 --top 25`, T = day × 24,000.

**Conditions.** The machine was not quiet: other agents' `cargo build` / `cargo clippy` / test runs and one or
two `island_game` windows (UI capture) ran throughout. To keep before and after comparable, each pair (same seed
and warm-up) ran **side by side at the same time**, one before and one after process, pairs one after another.
Absolute numbers are therefore lower than the quiet closing run (`closing/tick-cost.md`: 228-394 ticks/s); compare
within a row. Caveat for the day-28 rows of seeds 11 and 23: the after process finishes its 28-day warm-up first, so the before process measured alone (seed 23) or next to the seed-23 pair's warm-ups (seed 11); load varied by more than that between pairs, so treat single rows as ±30%. The paired probe profile below (same warm-up, measured at the same time) is the cleanest comparison. Residents: from the year soak of the closing tree (the bench
reports no population); the profile test counts them directly (46 at seed 11 day 7).

## What dominated

`npcctl bench` sees only `tracing` spans and `island_content` has no `tracing` dependency, so the island's own
probes (`village::probe`, hash-neutral, read no clock unless a profiler or the profile test switches them on) are
read by an ignored profile test on the same world:
`PROFILE_SEED=11 PROFILE_DAYS=7 cargo test --locked --release -p island_content --test island_tick_profile -- --ignored`.
In the game (debug tools, `PerfHandle`) the same probes are `npc_perf` mechanic rows.

Seed 11, day 7 (46 residents), 2,000 ticks, before and after run side by side:

| probe | calls | before ms (share of tick) | after ms (share of tick) | worst tick ms |
|---|---|---|---|---|
| `start.life.buy` | 7116 | 3429 (38%) | 1222 (25%) | 9.6 → 3.1 |
| `sup.procurement.start` | 3659 | 3266 (36%) | 1147 (24%) | 9.5 → 3.0 |
| `sup.market_supplier.start` | 9349 | 3294 (36%) | 1162 (24%) | 10.6 → 3.5 |
| `sup.route_estimate.start` | 19973 | 2148 (24%) | 704 (14%) | 24.1 → 7.8 |
| `sup.route_search.start` | 9120 | 2065 (23%) | 638 (13%) | 23.8 → 7.6 |
| `sup.depot_offers.start` | 9349 | 611 (7%) | 263 (5%) | 2.5 → 1.0 |
| `start.life.change_job` | 2263 | 816 (9%) | 265 (5%) | 19.8 → 6.8 |
| `start.life.hunt` | 1268 | 288 (3%) | 95 (2%) | 19.4 → 5.8 |
| `eco.total` | 2000 | 2016 (22%) | 1535 (32%) | 45.4 → 24.5 |
| `eco.production_refresh` | 2000 | 196 (2%) | 106 (2%) | 36.8 → 19.1 |
| `eco.market` | 2000 | 122 (1%) | 64 (1%) | 21.2 → 11.0 |
| `eco.ledger` | 2000 | 23 (0%) | 13 (0%) | 19.9 → 10.6 |

| | before | after |
|---|---|---|
| ticks/s | 222 | 411 |
| mean / p50 / p95 / p99 / max ms per tick | 4.51 / 3.30 / 11.8 / 25.2 / 52.6 | 2.43 / 1.89 / 5.9 / 11.4 / 27.3 |
| all leaf starts (ms in 2,000 ticks) | 5,382 | 1,995 |
| state hash at the end of the warm-up | `c4065c40008a5591` | `c4065c40008a5591` |

- Leaf starts were 41-57% of the tick (bench `think.start`, before). Inside them `ai::market_supplier` was 61%
  (`life.buy` → `ai::procurement`, which asks it once per wanted good).
- Inside the supplier lookup the largest part was **route estimates, not the seller scan**: `routes::estimate`
  ran its bounded A* (`search`) 9,120 times in 2,000 ticks (memo misses: the start cell is the buyer's own,
  which moves; the grid never changed in the window, so the memo was never cleared). A* with a `BTreeMap` of
  costs took 190-230 µs a search.
- Next: building a fresh `QueryState` in every lookup (`world.try_query`, 60 µs a call, 0.56 s of 2.37 s in a
  finer probe run) and the stock counts of every candidate seller (`econ::count` copied every item with its
  `String` kind; 216,000 calls, 0.74 s). The per-seller test itself (`person_sells` over the ~69 living actors)
  was 0.2 s, about 8% of the lookup.
- So the offer index of design S4.1 would have removed only that 8% scan; it is **not built**. The three costs
  above are cut instead, each exactly (same answers, same order, same tie-breaks).

## What was built (all in `content/island_content`)

- `src/village/frontier/routes.rs`: `search` keeps the best cost per cell in a dense per-thread scratch array
  with a stamp per search, instead of a `BTreeMap`. Pushes, pops, comparisons and the expansion cap are those
  of the map version (kept as `search_reference`), so the expansion order and the answer are the same
  (unit test `dense_search_equals_the_map_search`, 720 searches on mazes, open fields and grids larger than
  the cap). 3.3× faster per search.
- `src/village/lookup.rs` (new): `LookupCache`, query states built once per world and brought up to date;
  `ids_with::<T>()` gives the `(SimId, Entity)` list sorted by identity. Used by `ai::market_supplier`
  (living actors), `ai::gather_source` (patches) and `labor::living_actors`.
- `src/village/economy.rs`: `count` sums the matching free items without copying them (the filters are pure,
  so testing the kind first gives the same sum).
- `src/village/probe.rs` (new) and probes in `ai.rs`, `routes.rs`, `resident/actions.rs` (every leaf start,
  `start.<action>`), `mod.rs` / `ai::tick` (`eco.*`, the island tick by part) and `production.rs` (`plan.*`,
  the plan's phases).
- Test switch `lookup::BruteForce` (a resource, not state): puts every S4 path back on its original code.

## Before and after (`npcctl bench`)

| seed | residents (day) | ticks/s before → after | mean ms/tick | `think` p95 ms | `think.start` share of tick | `think` max ms | `village.economy` p95 / max ms |
|---|---|---|---|---|---|---|---|
| 7 | 46 (day 14) | 230 → 300 (+30%) | 4.35 → 3.33 | 7.4 → 4.1 | 52% → 35% | 26.1 → 13.0 | 1.21 / 29.5 → 1.39 / 22.2 |
| 11 | 46 (day 7) | 163 → 327 (+100%) | 6.13 → 3.06 | 10.9 → 3.6 | 53% → 34% | 44.2 → 11.9 | 2.02 / 41.4 → 1.35 / 25.4 |
| 23 | 46 (day 7) | 175 → 361 (+106%) | 5.71 → 2.77 | 9.8 → 3.4 | 53% → 35% | 25.4 → 8.3 | 1.73 / 43.0 → 1.15 / 24.9 |
| 7 | 51 (day 28) | 244 → 454 (+86%) | 4.10 → 2.20 | 7.5 → 2.6 | 41% → 25% | 16.8 → 4.9 | 1.62 / 39.1 → 1.12 / 30.9 |
| 11 | 59 (day 28) | 206 → 276 (+34%) | 4.85 → 3.63 | 9.9 → 5.2 | 57% → 38% | 24.5 → 13.0 | 1.03 / 41.4 → 1.28 / 24.2 |
| 23 | 55 (day 28) | 173 → 412 (+138%) | 5.78 → 2.43 | 12.5 → 3.3 | 49% → 33% | 40.5 → 10.8 | 1.87 / 53.2 → 0.89 / 18.2 |

`think.start` share: the leaf starts, of which the supplier lookup is most. `village.economy` max is the worst island-tick pass in the window (plan, market round or ledger).

## Real time

20 ticks a second at 1x (`island_game` `TICK_HZ`); per 60 Hz frame (16.7 ms) the sim takes mean ms × 20 × speed / 60.

| seed | residents | side | 1x ms per frame | 3x | 10x | 30x | top speed the sim alone sustains |
|---|---|---|---|---|---|---|---|
| 7 | 46 | before | 1.5 (9%) | 4.4 (26%) | 14.5 (87%) | 43.5 (261%) | 11x |
| 7 | 46 | after | 1.1 (7%) | 3.3 (20%) | 11.1 (67%) | 33.4 (200%) | 15x |
| 11 | 46 | before | 2.0 (12%) | 6.1 (37%) | 20.4 (122%) | 61.3 (367%) | 8x |
| 11 | 46 | after | 1.0 (6%) | 3.1 (18%) | 10.2 (61%) | 30.6 (183%) | 16x |
| 23 | 46 | before | 1.9 (11%) | 5.7 (34%) | 19.0 (114%) | 57.1 (342%) | 9x |
| 23 | 46 | after | 0.9 (6%) | 2.8 (17%) | 9.2 (55%) | 27.7 (166%) | 18x |
| 7 | 51 | before | 1.4 (8%) | 4.1 (25%) | 13.7 (82%) | 41.0 (246%) | 12x |
| 7 | 51 | after | 0.7 (4%) | 2.2 (13%) | 7.3 (44%) | 22.0 (132%) | 23x |
| 11 | 59 | before | 1.6 (10%) | 4.9 (29%) | 16.2 (97%) | 48.5 (291%) | 10x |
| 11 | 59 | after | 1.2 (7%) | 3.6 (22%) | 12.1 (72%) | 36.3 (217%) | 14x |
| 23 | 55 | before | 1.9 (12%) | 5.8 (35%) | 19.3 (115%) | 57.8 (346%) | 9x |
| 23 | 55 | after | 0.8 (5%) | 2.4 (15%) | 8.1 (49%) | 24.3 (146%) | 21x |

- **Honest top speed** (mean cost only, under the load above): 14-23x after, 8-12x before, at 46-59 residents. On the quiet machine of the closing
  run the before tree reached 11-20x, so expect the after tree higher there; this was not measured quiet.
- 10x now takes 44-72% of a frame on the mean. The AI (`think`) p95 tick fell from 7.4-12.5 ms to 2.6-5.2 ms, so a 10x frame of 3-4
  ticks with one p95 tick stays inside 16.7 ms.
- **30x is still not reached** at 51-59 residents by the sim alone under this load.

## The remaining 10x hitch: the market and plan passes

`village.economy` max is still 22-31 ms (bench). By part (profile, seed 11 day 7, after):
`production::refresh` (the plan, every 300 ticks, three settlements) up to 19 ms, `market::tick` (a round, every
300 ticks) up to 11 ms, the season ledger up to 11 ms on its day. The plan's own phases are spread out
(`plan.e_run` 1.5 ms, `plan.f_standing` 1.5 ms, `plan.g_shelves` 0.9 ms, `plan.d_materials` 0.7 ms,
`plan.a_inventory` 0.5 ms per settlement); no single lookup dominates, so there is no cheap hash-neutral cut
left there. The plan got cheaper with the stock count (worst 36.8 → 19.1 ms in the paired profile).

Spreading the passes (one settlement's plan per tick, or the market round on a different tick from the plan)
would remove the spike but changes when plans and prices update, so it changes behaviour and every island hash.
Not done; it needs approval and a re-baseline.

## Hash neutrality

- `npcctl replay verify` (after build) of `tests/replays.next/island_living_world_s{7,11,23}.json`: each
  `{"ok":true,"checked":40,"first_mismatch":null}`; every `tests/replays/*.json`: ok.
- `island_lookup_neutrality`: `offer_index_equals_brute_force` (every 50 ticks of 6,000, four residents in turn ×
  every traded good: supplier and stock count equal with and without `BruteForce`; the reused query equals a
  fresh one), `hashes_unchanged_by_offer_index` (one shipped day, 24,000 ticks, hash equal every tick with and
  without `BruteForce`), `save_load_mid_round_keeps_hashes` (save in the middle of a market round, strict
  load, equal hash every tick across four rounds and plan refreshes).
- The profile test's warm-up hash is the same before and after (`c4065c40008a5591`, seed 11, 7 days).

## Host: what "smooth" still needs (`apps/island_game`, not changed here)

`island_game` adds `NpcDriverPlugin::new(TICK_HZ, Update)` (`apps/island_game/src/lib.rs`, the app builder),
so the driver keeps its default `max_ticks_per_run: 100`: after one long frame (a 20-30 ms plan or market tick,
a window drag, a clock jump) the next frame runs up to 100 ticks back to back (100 × 2.2-3.6 ms = 220-360 ms),
which is itself a long frame, so the stall grows instead of being absorbed. At 10x a frame needs only 3-4 ticks.

Recommended (host only, hash-neutral: it changes how many ticks a frame runs, never what a tick does):
cap the ticks per frame at about twice the speed's need and drop the rest of a backlog, i.e. set
`npc_framework::kernel::driver::DriverSettings.max_ticks_per_run = max(4, 2 × ceil(TICK_HZ × multiplier / 60))` (1x and 3x: 4, 10x: 8, 30x: 20)
whenever `SimControl.multiplier` changes (`speed_input` and the night-speed switch in `frontend/mod.rs`), and
at startup. A 20 ms tick then costs one late frame (about 25-30 ms at 10x) and at most 8 ticks of catch-up
(about 20-30 ms) instead of a cascade; the clock loses those few ticks of backlog instead of stalling. A
wall-time budget per frame in the driver (`npc_kernel::driver`) would be smoother still, but that is a
generic-crate change.

## S6: the 10x hitches and the view's frame floor

Fixture `village-life` (41 residents, three villages, from 10:05 of day 2), developer host
(`--island-save` copy, `--island-mode inspector`, island overview camera, 1280x720, muted),
`ISLAND_FRAME_LOG`, the first 10 s of running frames dropped; the same script and fixture as the S5
capture pass (`target/npc-out/s5/captures.md`). Before = S5's numbers (HEAD `d8e43979`, the S5
release and dev builds); after = this tree. Output: `target/npc-out/s6/` (`frames/*/frames.csv`,
`profile-*.json`, `before/`, `after/` profiler reports from `npc/perf`).

### What the 200-330 ms frames were

None of them was a simulation tick. The whole-day headless profile of the same save (the profile
test with `PROFILE_SAVE`, `PROFILE_TICKS=24000`, `PROFILE_WORST`; `target/npc-out/s6/profile-before-day.json`)
has its worst tick at 21.5 ms; the frame log puts the long frames at the host and the view:

| Long frame (release, 10x) | When | Cause | Cost |
|---|---|---|---|
| 240-330 ms | every 60 s of real time (59.7, 120.0, 180.2 s; any time of day) | the minute timer's save on the frame: `snapshot::save` 206 ms (two dense grids, the spatial grid's statics 5.6 MB and the terrain 3.9 MB, are 98% of the 9 MB save and most of its cost), JSON 35-53 ms, `sync_all` and rename | 290 ms on one frame |
| 240-290 ms | at 00:00 of the day the season changes (tick of day 14-20) | `living_scene::sync_terrain` re-meshed every land chunk of the island for the new season's colours in one frame | 168 ms, plus 18 ms of mesh uploads |
| ~100 ms (rare) | 02:49, 05:35, 15:37 in S5 and again in S6 | not identified; one frame each, view side (the sim's worst tick is 21 ms) | |

Sim ticks, whole day, release, headless (unchanged by this pass; no sim code changed):

| tick | time of day | ms | biggest probes |
|---|---|---|---|
| 44800 | 20:48 | 21.5 | `eco.frontier` 20.9 |
| 57000 | 09:00 | 17.6 | `eco.market` 7.9, `eco.frontier` 6.0 |
| 34523 | 10:31 | 14.4 | `sup.route_estimate.start` 10.6 (`start.life.hunt` 7.3) |
| every 600 ticks | | 11-15 | the market round (`eco.market` 6-8) with `eco.frontier` 2-7 |
| 49047 | 01:02 | 13.1 | `eco.production_refresh` 6.0 |

Mean 1.43 ms, p50 1.0, p95 3.8, p99 6.4 ms a tick over the day (41 residents). In the game, with the
profiler's spans and probes on, a daytime tick costs about 2.2 ms (`drive_sim` p50 13 ms at 6 ticks).

### What was changed

- The timer save leaves the frame (`content/island_content/src/village/island_save.rs`
  `begin_save` / `SaveInProgress::finish`; `apps/island_game/src/island.rs` `begin_timer_save`,
  `finish_saves`, `settle_saves`). The frame reads the world with the two grids swapped for empty
  stand-ins (without change detection) and keeps copies, 19.5 ms instead of 206 ms (release); a
  thread turns the copies into JSON by their own registered functions and writes the file. The
  finished snapshot equals `save_island`'s byte for byte (profile test `save_cost`, and
  `island_save_continue::a_save_finished_off_the_frame_is_the_whole_save_and_changes_nothing`: six
  saves 250 ticks apart, the saved world's hash equal to an unsaved twin's every tick). Ctrl+S, the
  pause menu, quitting and the title still save synchronously, and each first waits for a timer save
  in flight; removing or adopting the autosave waits too.
- A new season sweeps over the land (`content/island_view/src/living_scene.rs` `sync_terrain`,
  `SEASON_CHUNKS_PER_FRAME = 3`): three land chunks a frame are recoloured, in about a second; shape
  edits are still drawn at once. Kenney materials still turn at once.
- The paused frame wrote to UI and scene components every frame, and each write re-ran text
  measurement, layout or transform propagation: 139 world-label fonts (`village_world::label_typography`),
  the save label's text and display (`island_game` `refresh_label`), 40 patch chips and the cursor tag
  of the closed shape tools (`shape_tools/resources.rs`, `shape_tools/palette.rs`), the death and
  spectator roots (`player_hud/life.rs`), the panel thumb colour (`panel_scroll.rs`), and the height of
  about 130 static models (`living_scene::elevation`). Each now writes only a value that differs.
  After: no `Text`/`TextFont` changes on a paused frame; 2,819 bone and 484 wind-plant transforms
  still change (animations and wind, by design).
- Measurement: `probe::end_tick_rows` and the profile test's `PROFILE_SAVE`, `PROFILE_SKIP`,
  `PROFILE_WORST` (slowest ticks with time of day and probes) and `PROFILE_SAVE_COST`.

### Before and after

Paused frame (clock stopped, same camera):

| Build | Frames | Mean ms | p50 | p95 | Max | > 18 ms | > 20 ms | > 25 ms |
|---|---|---|---|---|---|---|---|---|
| release before | 2544 | 18.45 | 17.93 | 22.0 | 38.3 | 1217 (48%) | 569 | 13 |
| release after | 1907 | 16.70 | 16.64 | 18.1 | 68.3 | 105 (6%) | 32 | 5 |
| dev before | 989 | 21.42 | 20.70 | 26.5 | 40.7 | 942 (95%) | 605 | 96 |
| dev after | 1824 | 17.43 | 16.50 | 22.6 | 52.0 | 600 (33%) | 302 | 37 |

The release paused frame is now at the display's 60 Hz (16.67 ms; "> 16.7 ms" only counts vsync
jitter there, so the table counts > 18 ms). Main-thread work on a paused frame (`npc/perf`, release):
Bevy systems 8.9 → 6.0 ms, island view 3.8 → 3.4 ms, `measure_text_system` 2.0 → 0 ms; the render
thread now waits about 6 ms a frame for the swap chain.

10x:

| Build | Frames | Game days | Mean ms | p95 | p99 | Max | > 16.7 ms | > 33 ms | > 50 ms | Sim speed |
|---|---|---|---|---|---|---|---|---|---|---|
| release before | 5897 | 1.64 | 36.3 | 52.4 | 62.7 | 332.1 | 5897 (100%) | 3284 (56%) | 494 | 9.2x |
| release after | 6836 | 1.72 | 31.7 | 47.9 | 57.6 | 103.6 | 6836 (100%) | 3074 (45%) | 212 | 9.6x |
| dev before | 4161 | 1.34 | 51.9 | 74.4 | 97.4 | 351.7 | 4161 (100%) | 3860 (93%) | 2280 | 7.5x |
| dev after, run 1 | 5008 | 1.47 | 43.1 | 66.2 | 92.9 | 2063.9 | 5008 (100%) | 3266 (65%) | 1509 | 8.2x |
| dev after, run 2 | 5170 | 1.52 | 41.8 | 63.1 | 79.0 | 162.3 | 5170 (100%) | 3371 (65%) | 1427 | 8.5x |

Release 10x frames by ticks run: 3 ticks 18.8 ms, 4 ticks 21.0, 5 ticks 24.2, 6 ticks 30.4,
7 ticks 35.8, 8 ticks 44.0 ms (before: 4 ticks 23.0 ... 8 ticks 46.3). The 240-330 ms frames are gone
(worst 104 ms). Dev run 1 had one 2.06 s frame at
110 s of real time (04:26), not at a minute mark and with nothing in the log; run 2 of the same
build had none (worst 162 ms), so it is noted, not explained.

### What remains

- 10x is still not smooth: the steady cost is the simulation itself. A 10x frame at 60 Hz needs
  3.3 ticks; at about 2.2 ms a daytime tick in the game (13-14 ms per frame at the 6 ticks a 30 ms
  frame needs) plus the view's 9-10 ms of main-thread work, a frame takes 21-30 ms, so the next one
  needs more ticks. Closing that needs the sim cheaper per tick (the leaf starts' supplier and route
  lookups are still 25-40% of a tick; the market round and `eco.frontier` 6-21 ms every 600 ticks) or
  run beside the frame; neither is a view or host change.
- Turning the profiler's spans off (`RUST_LOG=warn`) gave only 31.7 → 30.4 ms mean at 10x: the
  debug-tools build's tracing is not the cost.
- Rare single ~100 ms frames at fixed-looking times (02:49, 05:35, 15:37), not caught by the
  profiler in a short run; `island_world::boat::sync` costs 2.5 ms four times a second
  (`merchant::status` prices every good the boat trades).

### Hash neutrality and checks (S6)

- `target/release/npcctl replay verify tests/replays/island_living_world_s{7,11,23}.json`: each
  `{"ok":true,"checked":40,"first_mismatch":null}`.
- `cargo test --locked -p island_content --no-fail-fast`: 70 test binaries ok, 307 passed, 0 failed,
  41 ignored; `cargo test --locked -p island_view -p island_game`: 586 + 52 + 1 passed, 0 failed.
- `cargo clippy --locked -p island_content -p island_view -p island_game --all-targets -- -D warnings`
  and `cargo fmt --all -- --check`: clean.
