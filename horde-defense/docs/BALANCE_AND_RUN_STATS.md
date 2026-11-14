# Balance settings, arena snapshots and run statistics

`crates/horde_core/balance.json` is the authoring source for Easy, Normal and Hard. `LASTLIGHT_BALANCE` selects another startup catalogue. `balance::catalog().presets` exposes names, descriptions and complete `BalanceSettings`; Custom uses the same validated fields. `Campaign.set_balance_settings` validates atomically and normalizes edited preset values to Custom. It is legal during a run because settings apply only to the **next deployment**. Active main and worker attempts keep their copied settings and geometry; chained worker operations receive current shared settings when starting their next battle.

## Shipped settings

| Factor | Easy | Normal | Hard |
| --- | ---: | ---: | ---: |
| Enemy health | .80 | 1.00 | 1.20 |
| Enemy basic damage | .75 | 1.00 | 1.20 |
| Enemy movement speed | 1.10 | 1.25 | 1.45 |
| Spawn rate | .70 | 1.00 | 1.15 |
| Defender damage | 1.20 | 1.00 | .90 |
| Defender health | 1.50 | 1.25 | 1.00 |
| Coin reward | 1.00 | 1.00 | 1.15 |
| Boss-time multiplier | .85 | 1.00 | 1.10 |
| Victory threat growth | .80 | 1.00 | 1.25 |
| Additional NPC spawn adapter | .70 | .75 | .90 |
| Construction supply adjustment | +5 | 0 | -3 |

Multipliers stack with existing authored country and victory progression. Defender damage/health apply to the selected defender's numerical profile in either mode. NPC basic damage scales through numeric adds; attack rolls, active defense, one-second maneuvers and injury procedures remain unchanged. The additional NPC arrival adapter stacks with the existing progression adapter. No preset silently changes rewards after a defeat or resets victory threat.

All three presets start with **3 lanes and a 12-yard crossing**. Every three character levels, starting after level three, add two lanes and two yards: level 4 gives 5/14, level 7 gives 7/16, level 10 gives 9/18. Lanes cap at 17; width caps at 30 yards. Both freeze at deployment, including when XP earned during combat raises character level.

Advanced fields are `enemy_health`, `enemy_damage`, `enemy_speed`, `spawn_rate`, `player_damage`, `defender_health`, `reward`, `boss_time`, `threat_growth`, `npc_spawn_rate`, `supply`, `start_lanes`, `max_lanes`, `lanes_per_step`, `lane_level_step`, `start_width`, `max_width`, `width_per_step`, `width_level_step`. Scalar factors must be finite in .25-3; supply is -30 to +100; odd lane counts are 3-17, with an even increment 0-6; widths are 8-30 yards, width increment 0-8; level steps are 1-100. The catalogue requires distinct Easy/Normal/Hard entries and a non-Custom default preset.

## Frozen geometry and compatibility

`Attempt.balance`, `lane_count` and `width_yards` are authoritative snapshots. The defender remains at `(0.5, lane_count / 2)`. Enemy spawn/view bounds, Caller cues, ranged targeting, ordnance columns, collision stops and presentation projections use the actual width. `Enemy.arena_width` freezes the view boundary. Existing `Enemy.y()` includes its saved within-lane passing offset.

Construction always has five columns in the actual left half. `construction_bounds()` is `[2,6]` in the 12-yard clearing and `[3,15]` in a 30-yard arena; `construction_cell_width()`, `build_cell_center(lane,column)` and `build_cell_at(world_point)` are the shared input/rendering helpers. Structures freeze `position_x` and `build_width`; `x()`, `footprint_width()` and `blocking_radius()` read those snapshots. Unavailable stored plan cells are omitted without spending supply or deleting the plan.

Missing checkpoint geometry defaults to exactly **11 lanes / 30 yards**, not the new fresh profile. Missing active balance defaults to `BalanceSettings::legacy()`, preserving old multipliers. Missing structure positions/widths retain the old 4.2-yard origin and 2.4-yard column spacing. Older enemies retain a 30-yard view bound. Future deployments may use the newly selected settings; restoring an existing attempt never resizes it.

## DPS convention

`Attempt.live_dps()` is resolved injury in the most recent **five simulation seconds**, divided by five. The opening window is zero-padded; it does not divide the first shot by one tick. `average_dps()` divides the whole-run total by elapsed simulation time, and `peak_dps()` stores the highest five-second rolling result. A 4x observation rate does not multiply these values by four, and pause freezes both numerator and simulation clock.

Damage is separated into defender, structures and ordnance through `stats::DamageTotals`. It is the positive post-protection injury actually resolved by combat, including overkill as represented in the combat log; cursor input, animation, armor absorption and missed/defended attacks add nothing. Existing legacy telemetry stays intact. Counts such as shots and target-hit results retain their existing meanings; piercing can create multiple hit results from one shot.

The saved `DpsTracker` stores source totals, a bounded ordered bucket window, and peak. Validation checks tick order/future ticks, finite peak, overflow-safe aggregate window sums, and that each window source sum does not exceed its cumulative total. Legacy version-zero checkpoints hydrate the known old defender/structure aggregates for whole-run averages. Their old rolling window and precise ordnance split are unknown, so no historical five-second buckets or peak are invented. Such completed records carry `legacy: true`.

## Untruncated run history

Every newly finished victory, defeat and withdrawal appends one compact `stats::RunRecord`. There is no record-count limit or silent truncation. The existing explicit 64 MiB save-generation limit still protects storage; exceeding it reports a save failure rather than deleting history. Records use deterministic sequence/attempt IDs and simulation ticks, not real-clock timestamps.

Each record captures outcome, country/stage/template, control mode, defender identity, actual geometry, victory threat, full applied balance settings, four equipped gear definitions/qualities/levels/classes, duration, kills/shots/hits/misses, damage by source, average/peak DPS, retained coins, actual credited kill/completion XP, loot count and six quality counts, final/max HP and cause. `duration_seconds()` converts the saved tick duration. These fields describe already-banked transactions and cannot pay them again.

The parent campaign is the sole persistent history owner. Worker campaign copies exclude the parent's history entirely. Finished workers emit records through a saved outbox; parent settlement drains it once, assigns a global history ID, normalizes actual shared completion XP, and tags worker/recruit IDs. This also records recalls/defeats, not just successful operations. Reopening results or restoring a generation cannot append or award the same finished run again. Older saves start with an empty history; prior finalized runs are not fabricated retroactively.

## Verification

The Normal opening controller uses current enemy positions and only earned, guaranteed purchases. Across DEU/BRA/EGY/USA/ISL and three seeds, every fresh run failed; fourteen purchase routes cleared on run three and one on run four, without equipping loot. First runs banked 282-464 coins. The actual eight-defense USA route and all five templates remained achievable in both modes. These are deterministic simulation proofs, not human playtest or GUI evidence.

Dedicated tests cover atomic/frozen settings, all three presets, actual damage-source integration, fixed-window DPS and corrupt aggregate rejection, pause/checkpoint equivalence, legacy 11/30 geometry, 302 retained all-outcome records, and saved worker-outbox settlement with one parent owner.

## Goal 6: enemy difficulty acceptance

Goal 6 of the production pass gives the enemy side a measured shape (acceptance G6.1-G6.18). The rules (two-axis threat, composition, elites, boss phases and ranks, the country band, the victory-to-threat guard) are ordinary tests and are described in PROGRESSION_SCALING.md and ENEMY_COMPOSITION.md. Six rows are pacing measurements: bots play whole campaigns through `Game::apply`, and `crates/horde_core/tests/pacing_targets.rs` asserts on the result. Every test there is `#[ignore = "release-only"]` (G6.10) and runs after the debug builds:

```powershell
cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture
cargo test --locked --release -p horde_core --test pacing_targets goal6 -- --ignored --nocapture --test-threads 1
```

The second line runs only the goal-6 rows and their sweeps. Each test prints a PASS or FAIL summary with the numbers it read. The evaluators are pure functions in `sim/pacing/goal6.rs`, pinned on synthetic reports in `tests/pacing_harness.rs` (normal suite).

### Profiles

Every sweep is 20 seeds, deterministic per seed. The profiles are rows of the `sim/pacing.rs` registry:

| Profile | Player | Feeds |
| --- | --- | --- |
| `expert-deu` | The acceptance profile: Germany start, expert aim, tree-first spending, Normal, the solo plan, 60 runs | G6.16, G6.17; G6.11 and G6.12 printed as information only |
| `goal6-novice` | `expert-deu` with the novice aim and nothing else (W4-13, user decision R56) | G6.11, G6.12 |
| `lux-easy`, `lux-normal`, `lux-hard` | Expert, tree first, Luxembourg start on each preset; stops at the first liberation, at most 20 runs | G6.15 |
| `start-aut`, `start-bgd` | Expert, tree first, Austria (0.85, plays 0.95) and Bangladesh (1.60, plays 1.05) starts; stops at the first victory, at most 20 runs | G6.18 |

G6.11 and G6.12 moved to `goal6-novice` because the expert bot aims almost perfectly (design-enemy-balance §14): a bot's defeat share is a floor, not a measure of how hard the game is for a person. The human difficulty playtest (plan §11) still decides that.

### Rows and how they are read

Cells are the lower median over the 20 seeds, with [p25, p75] unless named.

| Row | Test | Target | Wave-4 close | Verdict |
| --- | --- | --- | --- | --- |
| G6.11 | `goal6_defeat_rate_windows` | Defeat share 10-25% in every 20-run window after the first victory | `goal6-novice`: 32 of 37 windows outside (30.5% at offset 0, 0.0% from offset 18); `expert-deu`: 32 of 38 | FAIL |
| G6.12 | `goal6_no_15_streak_after_run_10` | No seed wins 15 in a row from run 11 | `goal6-novice`: longest streak median 44 [max 50], every seed; `expert-deu`: 49 [max 50] | FAIL |
| G6.15 | `goal6_lux_easy_at_most_6_runs`, `goal6_lux_hard_at_most_1_5x_normal` | Luxembourg liberated in at most 6 runs on Easy; Hard at most 1.5× Normal | Easy 5 [5, 6], Normal 9 [7, 10], Hard 13 [12, 15] (limit 13.5) | PASS |
| G6.16 | `goal6_archetype_coverage` | Every catalogue archetype spawned and killed | 18 of 18 | PASS |
| G6.17 | `goal6_boss_ttk_median_20_60_s` | Boss time-to-kill median 20-60 s | 31.4 s [p25 25.6, p90 47.5] | PASS |
| G6.18 | `goal6_clamped_starts_first_victory_runs_2_to_4` | A clamped start's median first victory on runs 2-4 | Austria 3 [3, 5], Bangladesh 3 [3, 4] | PASS |

- **Windows (G6.11).** Each seed's runs are aligned on its own first victory. Window `offset` covers the 20 runs from that offset and pools the defeats of every seed that played all 20; a seed that never wins adds nothing and is named as a failure. No minimum seed count applies, so the summary reports how many seeds each window pooled (ruling R37): on `goal6-novice` offsets 0-35 pool 20 seeds and offset 36 pools 19.
- **Streak (G6.12).** The longest run of consecutive victories from run 11 on, per seed. The rule is per seed, not a median: every offending seed is named.
- **Runs to liberate (G6.15).** The median `Liberation(1)` run, a seed that never liberates counting 21. A Hard median that lands on such a seed fails as unmeasured, because 21 only bounds it from below (design-enemy-balance W-4 as-built note).
- **Boss time-to-kill (G6.17).** `RunRecord.boss_ttk_ticks`, the defeat tick less the boss's arrival, pooled over every seed and run.

The numbers are those of plan §9.6 ("W4-p repricing and goal-5 calibration", "W4-04..n tuning loop, resumed on the repriced tree" and "W4-13 goal-6 measurement profile"), measured after the wave-4 rules (Regional and Capstone multipliers, the repriced tree) were in. The expedition rule of W4-12 does not reach them: every goal-6 profile plays the solo plan.

### What the tuning showed

The W4-04..n loop measured every listed enemy lever on the repriced tree: `health_per_rank_late`, the `boss_health` slope past rank 4, `front_ramp_max` in 0.17-0.25, elite `end_bp` and the affix multipliers, and the Capstone health, alone and combined. Inside the G6.17 band (boss TTK p90 at most 60 s) every variant left 32-33 of 38 windows outside 10-25% and a 46-50 win streak, the shipped values' own numbers, so none was kept and `progression.json` and `enemy_catalog.json` ship the wave-2 values plus the W4-01 boss ranks. The cause is the survivor's late power, not the enemy curve: the expert survivor takes no archetype damage in the median run from run 11 and loses no run after run 20, while its power stops growing at about run 30 (level 50, tree exhausted at run 28-30). The rank rises by one per victory and never falls, so a steeper slope builds a wall at one rank (no defeats, then nearly all) instead of a steady share. `goal6-novice` loses more early (the first windows rise past 25%) and still loses no run from about run 25. No other profile the bot's knobs allow (aim, spending, start country, farming, takeover; preset Normal) passes either.

G6.11 and G6.12 are red at the wave-4 close and wait for a user decision (plan §2.5, "G6.11 and G6.12 stay red on the goal-6 measurement profile"); its default is to accept both red for this pass, with `goal6-novice` the measured profile and the human playtest deciding. The closing phase re-runs every `pacing_targets` row at HEAD.
