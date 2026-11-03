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
