# Victory threat, character levels and arena authoring contract

`crates/horde_core/progression.json` defines progression independently of the five encounter templates. `LASTLIGHT_PROGRESSION` can select another JSON file at startup. Parsing rejects unknown keys, non-finite/out-of-range values, zero divisors, and decreasing pressure floors. The override is read once; no configuration is reloaded during an attempt.

## Rank and encounter snapshots

Threat rank is `Campaign.victories`: **every successful run raises future threat**, including cleared-stage replays, outbreak defenses and worker/country-operation victories. Defeat and withdrawal do not raise it. Country completion, currency, kills and XP do not independently raise threat. Active attempts retain their existing snapshot while victories in another battlefield change future deployments. A recruit's expedition is the one deployment that does not play this rank: since W4-12 it plays a rank derived from the recruit (see "Expedition threat"), although its victory still raises the campaign's.

`Campaign.threat_revision` distinguishes the old model. Missing/zero revisions migrate once by taking the greater of recorded victories and historical unique-clear high-water. Migration awards no retroactive completion XP. Current games use only victory count; editing or reopening the world ledger is not a new victory. Parallel settlement commits the victory and completion XP once, then synchronizes the next chained deployment to shared threat and character progression.

Characters have a configurable maximum level, shipped at **50**. Each victory grants **120 Personal completion XP** before the cap in either control mode; NPC kills still grant no Personal kill XP. Personal kills retain their existing XP. `Report.completion_xp` records the actual credited completion amount, including a final partial amount at the cap. Reopening results cannot award it again. At MAX, XP stops accumulating. Older characters above the new cap keep their level, body investments and points; their pending XP is cleared and no further level-up occurs.

Fresh geometry is now controlled by the selected next-run balance settings: Normal starts at **3 lanes / 12 yards** and adds two lanes/two yards every three character levels, capped at 17 lanes / 30 yards. `Attempt.lane_count` and `width_yards` freeze at deployment. Legacy checkpoints and explicit `BalanceSettings::legacy()` retain their former 11-lane / 30-yard profile. See [BALANCE_AND_RUN_STATS.md](BALANCE_AND_RUN_STATS.md) for presets, advanced controls, shared build geometry, DPS and persistent run records.

`Attempt.progression` freezes rank, simultaneous batch size, item level, population allowance, pressure floors, supply bonus and all encounter multipliers. The encounter has two axes: the threat rank, and the stage's depth on its country's front (`FrontPosition`, `stage/(count − 1)`). Deploying a country stage multiplies the rank's enemy health and arrival by the front ramp (`front_ramp_max` below), stamps the stage's boss rank (Sector, Regional on every sixth stage, Capstone on the final stage) and freezes both in the snapshot. The briefing forecast is `Game::stage_preview` of the selected stage: that stage's front ramp and boss rank, folded in as the deploy stamps them (W3-23, W4-01). The defender power number stays the rank-only reading (no front position), which equals stage 0 of a multi-stage front; stage 0 of a one-stage country is its Capstone, so it is not that. The authoritative `Attempt.arrival_rate()`, `boss_time()`, `enemy_cap()` and `defender_position()` include the frozen arena values. UI and tooling must use those methods instead of the raw template values.

| JSON setting | Shipped value / rule |
| --- | --- |
| `max_rank` | 50; bounded movement/batch/population/economy growth only, **not** a threat plateau |
| `threat_guard_rank` | 10,000; explicit numerical support guard, independent of character MAX |
| `player_level_cap`, `victory_player_xp` | 50 / 120 |
| `batch_rank_step`, `max_batch` | `min(4, 1 + floor(min(victories,50) / 3))` enemies in each group |
| `item_level_rank_step`, `max_item_level` | `min(30, 1 + floor(victories / 2))`; victory 52 offers level 27, victory 58 reaches level 30 |
| `population_base`, `population_per_rank`, `population_cap`, `population_global_cap` | Base `min(145,72 + 2 * min(victories,50))`; scale by `lane_count / 11`, round up, then cap at 224 ordinary actors, plus one reserved boss |
| `health_per_rank`, `health_per_rank_late` | `1 + .015 * min(victories, 4) + .04 * max(victories − 4, 0)`, continuing past 50 |
| `damage_per_rank`, `damage_per_rank_late` | `1 + .008 * min(victories, 4) + .018 * max(victories − 4, 0)`, continuing past 50 |
| `growth_knee_rank` | 4: the rank where health and damage switch to the `_late` slopes (each 0..=0.25), so shop gear stays within about 5-15% of enemy health from rank 4 to 58. Missing knee and late keys keep one slope (`1 + .015 * victories`, `1 + .008 * victories`) |
| `speed_per_rank` | `1 + .002 * min(victories,50)` |
| `arrival_per_rank` | `1 + .01 * min(victories,50)` |
| `npc_arrival_factor` | Additional `.5` multiplier for NPC encounter arrivals (0.5..=2.0; `.9` until W4-14, R59): one action per turn caps an NPC defender's kill rate, so its arrivals stay under that cap. On Normal the NPC meets 0.375 x the personal arrivals |
| `reward_per_rank` | `1 + .03 * min(victories,50)` |
| `fortification_per_rank` | `1 + .025 * min(victories,50)` |
| `supply_per_rank`, `max_supply_bonus` | Renewed construction supply bonus `min(40, min(victories,50))` |
| `front_ramp_max` | 0..=0.5, shipped .18 (missing key: 0, no ramp). Enemy health and arrival × `1 + front_ramp_max × front_ramp_pct/100 × stage/(count − 1)`, at most 2; `front_ramp_pct` is the preset's extra (Easy 75, Normal 100, Hard 150). Stage 0, a one-stage country and a deployment without a country stay at exactly 1 |
| `boss_damage_per_rank` | 0..=0.05, shipped .01 (missing key: 0). Boss multiplier `clamp(1 + boss_damage_per_rank × (min(victories,50) − 4), 1, 4)`, exactly 1 through rank 4. Personal boss contact damage is multiplied by it before rounding; NPC boss attacks gain `floor((m − 1) × 6)` additional basic damage. Regular enemies never read it |
| `boss_armor_per_rank` | 0..=0.05, shipped .02 (missing key: 0). A Personal boss spawns with `round(boss_armor × clamp(1 + boss_armor_per_rank × (min(victories,50) − 4), 1, 4))`, exactly the template armor through rank 4. Exposure still drops it to 0 |
| `npc_boss_armor_rank_step` | 0..=100, shipped 8 (missing key or 0: none). An NPC boss spawns with `boss_npc_armor + min((victories − 4) / step, 20)` armor, the template armor through rank 11 |
| `elite { min_rank, start_bp, end_rank, end_bp }` | Shipped `{3, 200, 23, 1500}` (missing block: no elites). The snapshot's `elite_rate_bp` is `progression::elite_bp`: 0 below rank 3, `200 + 65 × (min(victories, 23) − 3)` basis points from rank 3 (2% at 3, 8.5% at 13, 15% from 23), times the preset's `elite_rate_pct` (Easy 50, Normal 100, Hard 150) in integer math, at most 10000, frozen at deploy. Validation keeps the curve linear (a whole number of basis points per rank), starting at rank 3 or later. See ENEMY_COMPOSITION.md, "Elites" |
| `boss_ranks { regional, capstone }` | Shipped Regional `{health 1.2, reward 1.2}`, Capstone `{health 1.4, armor 1.25, reward 1.4, damage 1.1, npc_armor_add 2, extra_transitions 1}` (missing block: neutral terms and the Capstone's one transition). Folded into the boss fields of a country stage's snapshot at deploy; see "Regional checkpoints and the Capstone" and ENEMY_COMPOSITION.md, "Regional and Capstone bosses" |
| (code) boss phase transitions | No catalogue key. The snapshot's `boss_phases` is `min(2, (victories > 4) + boss_phase_bonus + extra_transitions)`: one past rank 4, the preset's `boss_phase_bonus` (Hard 1) also only past rank 4, and the boss rank's `extra_transitions` (Capstone 1); frozen at deploy. Absent at 0. See ENEMY_COMPOSITION.md, "Boss phases" |

The NPC arrival adapter accounts for the discrete attack/defense/Ready/Aim procedures. It does not accelerate NPC turns, change attack rolls, or grant player skills to NPCs. Personal damage scales directly; NPC hostile attack damage receives `floor((enemy_damage − 1) × 6)` authored additional basic damage. Existing armor, active defense and injury stages still resolve normally.

Spawn credits count groups. Each tick adds `arrival_rate / (60 × batch_size)`, so larger groups change how many enemies arrive at once without secretly multiplying the displayed mean enemy rate. Each admitted member independently selects a legal lane and eligible weighted archetype through the existing RNG. A full population discards that group's missing members; it accumulates no deferred catch-up burst. Population and group size remain bounded. A hard scheduling ceiling of 30 enemies/second protects fixed-tick credit invariants even for aggressive overrides.

## Two-axis threat

An encounter's threat has two axes, both frozen at deploy (design
enemy-balance §1, rulings W-5 and W-9):

1. **Threat rank** `r`, the committed victories (`Campaign::progression_rank`;
   an expedition plays its own, below). It drives the growth knee, the
   pressure floors and composition tier, the elite rate, boss armor and damage
   and the boss phase gate: every row of the table above that names
   `victories`.
2. **Front depth**, the stage's place on its country's front:
   `FrontPosition { stage, count }` with `depth = stage / (count − 1)` (0 for a
   one-stage country). It drives the front ramp and the boss rank. A
   deployment without a country (GOLDEN, smoke, practice) has no front
   position: ramp exactly 1.0 and a Sector boss.

The front ramp multiplies the rank's enemy health and arrival by `1 +
front_ramp_max × front_ramp_pct/100 × depth` inside
`Campaign::progression_for`. At equal rank a country's last stage over its
first is, in enemy health × arrival, 1.18² = 1.3924 on Normal, 1.27² = 1.6129
on Hard and 1.135² = 1.2882 on Easy (`front_ramp_final_over_first_normal_is_at_least_1_35`,
`front_ramp_orders_hard_over_normal_over_easy`; acceptance G6.13 asks at
least 1.3). The composition tier does not follow depth. Wave tiers at ranks
10 / 20 / 40 are 7 / 10 / 11 (`snapshot_wave_tiers_at_10_20_40_are_7_10_11`,
G6.14).

Not an axis: the country's difficulty is a modifier already frozen into
`Attempt.modifiers` ("Country difficulty band" below), and neither the
survivor's power nor the liberation count feeds threat. The preset's extras
(`PresetExtras`: `elite_rate_pct`, `front_ramp_pct`, `boss_phase_bonus`;
Easy 50 / 75 / 0, Normal 100 / 100 / 0, Hard 150 / 150 / 1) scale the axes'
terms and, like every other balance setting, reach only the next deployment.
A Custom edit keeps the extras of the preset it was edited from
(`Campaign.balance_extras_basis`, shown as "Custom (Hard rules)").

## Elite rate

`snapshot.elite_rate_bp` is the chance, in basis points, that a director wave
spawn is elite: `min(10000, elite_bp(r) × elite_rate_pct / 100)` in integer
math, frozen at deploy. On Normal it is 0 below rank 3, 2% at rank 3, 8.5% at
rank 13 and 15% from rank 23 (`elite_bp_pins`,
`elite_fraction_at_rank_13_is_8_5_pm_0_5_over_20k`); Hard is 1.5× and Easy
0.5× (`hard_elite_rate_1_5x_normal_easy_0_5x`). The rate depends on the rank
only, never on the front depth or the boss rank. The roll itself, the affixes
and the forced Uncommon-or-better drop are in ENEMY_COMPOSITION.md, "Elites".

## Regional checkpoints and the Capstone

Every sixth stage of a country short of its last, 0-based stages 5, 11 and
17, is a Regional checkpoint, and the last stage is the Capstone
(`FrontPosition::boss_rank`, `Country::regional_checkpoints`). They are
derived from the stage count alone, so nothing is saved per stage: only the
attempt-scoped `snapshot.boss_rank` and the finished run's `Report.boss_rank`
and `RunRecord.boss_rank` record them. A country needs 7 stages for one
checkpoint, 13 for two and 19 for three; 219 of the 242 countries have none,
12 have one, 4 two and 7 three (`regional_checkpoint_distribution_is_219_12_4_7`;
the 24-stage USA has Regional bosses at stages 5, 11 and 17 and its Capstone
at 23, `boss_rank_derivation_usa_24`).

A Regional boss has ×1.2 health and coins; a Capstone ×1.4 health and coins,
×1.25 armor, +2 NPC armor, ×1.10 damage and one extra phase transition
(`boss_ranks` above; the numbers and read sites are in ENEMY_COMPOSITION.md,
"Regional and Capstone bosses"). The rank does not change the threat rank,
the ramp, the regular enemies, the stage theme or the victory's +1 threat.
The briefing, the world panel and Results name the checkpoints from the same
terms (W4-02).

## Expedition threat

A recruit's expedition plays a threat rank that follows the recruit, not the
campaign (W4-12, user decision R57; design-campaign-defenders §1.14):

`expedition_threat_rank = (advancement level − 1) × 1 + floor(Σ (item level − 1) over the four loadout slots × 1 / 4)`,
bounded by the campaign's `progression_rank()`.

The advancement level (1-10) is the smallest level whose allowance `12 + 8 ×
(level − 1)` covers the recruit's awarded points; a slot whose item is not in
the stash counts as item level 1. Both factors are the constants
`EXPEDITION_RANKS_PER_ADVANCEMENT_LEVEL` and `EXPEDITION_RANKS_PER_ITEM_LEVEL`
(1 each). A starter recruit plays rank 0, and an expedition is never harder
than the main run. The rank is computed when the expedition attempt is
created (`Mode::Npc` deploys in a worker game: `deploy_survivor` and a country
operation's next stage) and saved in `Attempt.progression`, where every
attempt's threat already lives, so the schema is unchanged. Main runs (the
survivor, Mara, a recruit takeover) keep the campaign rank, and every
expedition victory still adds one to the campaign rank.

Measured at the wave-4 close (release, 20 seeds; plan §9.6): the bot's
recruits left on their starter loadout, played rank 0 and cleared no stage,
so G3.11 (`parallel_frees_three_stage_neighbours_within_two_thirds_of_solo_runs`)
read 1.00 against its 0.67 target. W4-14's Npc pass let them clear stages 1
and 2, and W4-15 (Survivor network at 1,500 coins and the strategy bot's
`train_companions` step, which trains each recruit to 12 points and equips
the best stash items that keep its rank) lets them clear every stage: G3.11
reads 0.67 and passes. The rule reads item levels, not tiers, and floors
the excess levels over the four slots: a recruit wearing tier-4 drops with
at most three item levels above 1 in all (for example one level-4 item and
three level-1 items) still plays rank 0, while four level-2 items play
rank 1 and four level-4 items rank 3. The user accepted this reading of
gear by item level, not tier (decision R66, 2026-11-17), and the human
difficulty playtest judges it.

## Country difficulty band

Country difficulty is 0.85-1.60 in whole hundredths, set by
`scripts/generate_world.py` from the country's stage count: up to 3 stages
0.85-1.15, 4-9 stages 1.00-1.35, 10 or more 1.20-1.60 (acceptance G6.7,
`world_difficulty_matches_band_formula`). A deployment multiplies enemy health
and reward by it and enemy speed by its square root, frozen in
`Attempt.modifiers`, on top of the rank and the ramp. The campaign's start
country plays clamped to 0.95-1.05 (`Country::effective_difficulty`), so the
clamped AUT and BGD starts reach their first victory on runs 2-4 like the
rest (G6.18). The band, the formula and who reads the raw value are in
WORLD.md, "Country difficulty band".

## Floors across template cycles and country changes

`pressure_floors` is indexed by victory threat rank, holding its last entry beyond the authored list. Entries must be nondecreasing. Zeroes in rank zero preserve the original first-attempt template profile.

| Rank | Minimum start rate | Minimum rate cap | Minimum boss time | Personal boss HP floor | NPC boss HP floor | Minimum composition tier |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| 1 | .35 | 1.40 | 180 s | 3,000 | 43 | 1 |
| 2 | .38 | 1.52 | 210 s | 4,000 | 51 | 2 |
| 3 | .41 | 1.64 | 210 s | 5,200 | 59 | 3 |
| 4 | .44 | 1.76 | 240 s | 6,800 | 67 | 4 |
| 5-28 | `.44 + .0025 × (r − 4)` | `1.76 + .01 × (r − 4)` | 240 s | `6,800 + 600 × (r − 4)` | `67 + 2 × (r − 4)` | 4 at rank 5, 5 at 6-7, 6 at 8-9, 7 at 10-12, 8 at 13-15, 9 at 16-19, 10 at 20-27, 11 at 28 |
| 28+ | .50 | 2.00 | 240 s | 21,200 | 115 | 11 |

For each field, the encounter uses the greater of the template value and the frozen floor before applying rank multipliers. The weighted composition uses the greater of the template index and the composition floor. Thus defense six does not return to introductory enemies, a 180-second timer or the original boss health. New countries retain the same global victory threat. Authored country difficulty, buffs and biome modifiers still apply, so geographical variations remain meaningful.

The tactical equipment multiplier strengthens structure durability and attacks through the existing structure-bonus path. Grenade impact damage and Molotov impact/fire damage also include it, then freeze their resolved values into each projectile/fire zone. Charges and skill requirements stay independent; rank grants no free skill unlocks.

## Item-instance levels

`Item.level` is authoritative instance metadata, not an equipment tier. New drops receive the encounter's frozen item level; shop purchases receive the current victory-based offer level. Protected starter and recruit-issued sets remain level one. Existing `add_item(definition)` remains a level-one loot/fixture adapter; `add_item_at_level(definition, level)` is the explicit leveled acquisition API.

For `d = level − 1`, the shipped `gear` settings apply:

| Field | Effective result |
| --- | --- |
| `damage_per_level: .09` | Personal ranged/melee basic damage × `(1 + .09d)`, rounded |
| `protection_per_level: .04` | Personal and NPC armor × `(1 + .04d)`, rounded |
| `vitality_per_level: .05` | Personal item health/stamina bonuses × `(1 + .05d)`, rounded |
| `npc_damage_add_per_level: 1` | Ranged/melee NPC basic-damage adds + `d`, within supported numeric bounds |
| `npc_health_per_level: .35` | Equipped outfit/accessory level increments jointly add `floor(.35 × total increments)` NPC HP |
| `price_per_level: .05` | Purchase and resale values × `(1 + .05d)`, rounded |

Item multipliers have an engine safety ceiling of 8. Numeric profile limits still apply. Level one produces identical old statistics. NPC attributes, creation/advancement budgets, legal cadence and combat procedures remain separate from equipment bonuses. Personal skill effects still apply only where their existing rules permit. The pistol cap applies once to the whole trigger pull after leveled damage, pellets and weak-point bonuses.

Use `Campaign.effective_item(id)` for a scaled definition, `profile`/`compare_item` for complete loadout effects, `buy_price(definition)` for the current offer, and `item_sale_value(id)` for the owned instance. These helpers also support guards and shared-inventory sale quotes. `shop_item_level()` exposes the offer's level. `deployment_progression(mode)` returns the exact upcoming NPC adapter and lane-scaled population. `next_victory_progression(mode)`, `level_after_victory()` and `lane_count_after_victory()` forecast known completion XP only, excluding unknown future kill XP. Catalog `buy_value`/`sell_value` are level-one bases.

The existing `npc_only` and proficiency rules remain authoritative. A higher level never unlocks a weapon class, manufactures an item, or lets a fresh Personal defender borrow NPC-issued starters.

## Save compatibility and validation

- Missing item levels default to one. Valid instance levels are 1–64; protected starters must remain level one.
- Missing progression snapshots default to the legacy unscaled profile; a zero population snapshot cap resolves to the old template cap.
- Old threat revisions use unique country completion/high-water only as a one-time migration floor; current threat grows once per victory.
- Missing `lane_count` defaults to exactly 11, including older checkpoints whose character has already passed a future lane-unlock threshold. Lane telemetry accepts the old JSON arrays and freezes the active arena length.
- Snapshots validate finite bounded factors, population, group/item levels and floor metadata independently of current authoring values; their rank cannot exceed the campaign's recorded victory threat.
- `SurvivorId::{Rook, June}` is cosmetic. A fresh campaign starts with no confirmed survivor; the app confirms the choice before country selection. Missing older selections become Rook, and missing appearance snapshots become Male. Direct core/practice fixtures retain the original male fallback. Both choices have identical starting equipment and combat statistics.

`peak_render_fixture()` fills its actual frozen population at victory threat four. `maximum_progression_render_fixture()` stages bounded-growth threat 50, character MAX 50, 17 lanes, funds and leveled equipment, filling all 224 ordinary slots plus its reserved boss with visible enemies. Threat 50 is not the maximum supported difficulty. It is a synthetic rendering/performance fixture, never evidence of earned progression. Earned progression has separate simulation tests. Both fixtures deploy stage 4 of their active country on Normal, so since W2-26 their encounters, and the practice scenes built on them, carry that stage's front ramp `1 + .18 × 4/(count − 1)` (about 1.031 on peak's 24-stage USA front); `render_fixtures_carry_their_stage_front_ramp` pins it.

## Numerical safety is not a progression reset

Health and damage continue increasing after character level 50, maximum lanes and bounded-growth rank 50. The shipped threat guard is 10,000 victories. Deployments beyond the configured support guard reject explicitly instead of silently flattening or resetting threat. Generated enemy health also rejects profiles above 10,000,000 HP before deployment/load; generated damage adds have an engine safety range up to 1,000,000. These bounds protect fixed integer injury arithmetic and are not new attacks, death rules or turn opportunities. Item quality, item level, speed, batches, lanes and population have their own separate caps.
