# Victory threat, character levels and arena authoring contract

`crates/horde_core/progression.json` defines progression independently of the five encounter templates. `LASTLIGHT_PROGRESSION` can select another JSON file at startup. Parsing rejects unknown keys, non-finite/out-of-range values, zero divisors, and decreasing pressure floors. The override is read once; no configuration is reloaded during an attempt.

## Rank and encounter snapshots

Threat rank is `Campaign.victories`: **every successful run raises future threat**, including cleared-stage replays, outbreak defenses and worker/country-operation victories. Defeat and withdrawal do not raise it. Country completion, currency, kills and XP do not independently raise threat. Active attempts retain their existing snapshot while victories in another battlefield change future deployments.

`Campaign.threat_revision` distinguishes the old model. Missing/zero revisions migrate once by taking the greater of recorded victories and historical unique-clear high-water. Migration awards no retroactive completion XP. Current games use only victory count; editing or reopening the world ledger is not a new victory. Parallel settlement commits the victory and completion XP once, then synchronizes the next chained deployment to shared threat and character progression.

Characters have a configurable maximum level, shipped at **50**. Each victory grants **120 Personal completion XP** before the cap in either control mode; NPC kills still grant no Personal kill XP. Personal kills retain their existing XP. `Report.completion_xp` records the actual credited completion amount, including a final partial amount at the cap. Reopening results cannot award it again. At MAX, XP stops accumulating. Older characters above the new cap keep their level, body investments and points; their pending XP is cleared and no further level-up occurs.

Fresh geometry is now controlled by the selected next-run balance settings: Normal starts at **3 lanes / 12 yards** and adds two lanes/two yards every three character levels, capped at 17 lanes / 30 yards. `Attempt.lane_count` and `width_yards` freeze at deployment. Legacy checkpoints and explicit `BalanceSettings::legacy()` retain their former 11-lane / 30-yard profile. See [BALANCE_AND_RUN_STATS.md](BALANCE_AND_RUN_STATS.md) for presets, advanced controls, shared build geometry, DPS and persistent run records.

`Attempt.progression` freezes rank, simultaneous batch size, item level, population allowance, pressure floors, supply bonus and all encounter multipliers. The encounter has two axes: the threat rank, and the stage's depth on its country's front (`FrontPosition`, `stage/(count − 1)`). Deploying a country stage multiplies the rank's enemy health and arrival by the front ramp (`front_ramp_max` below), stamps the stage's boss rank (Sector, Regional on every sixth stage, Capstone on the final stage) and freezes both in the snapshot. The briefing forecast (`deployment_progression`, `next_victory_progression`) and the defender power number are the rank-only reading, which equals stage 0 of any front. The authoritative `Attempt.arrival_rate()`, `boss_time()`, `enemy_cap()` and `defender_position()` include the frozen arena values. UI and tooling must use those methods instead of the raw template values.

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
| `npc_arrival_factor` | Additional `.9` multiplier for NPC encounter arrivals |
| `reward_per_rank` | `1 + .03 * min(victories,50)` |
| `fortification_per_rank` | `1 + .025 * min(victories,50)` |
| `supply_per_rank`, `max_supply_bonus` | Renewed construction supply bonus `min(40, min(victories,50))` |
| `front_ramp_max` | 0..=0.5, shipped .18 (missing key: 0, no ramp). Enemy health and arrival × `1 + front_ramp_max × front_ramp_pct/100 × stage/(count − 1)`, at most 2; `front_ramp_pct` is the preset's extra (Easy 75, Normal 100, Hard 150). Stage 0, a one-stage country and a deployment without a country stay at exactly 1 |
| `boss_damage_per_rank` | 0..=0.05, shipped .01 (missing key: 0). Boss multiplier `clamp(1 + boss_damage_per_rank × (min(victories,50) − 4), 1, 4)`, exactly 1 through rank 4. Personal boss contact damage is multiplied by it before rounding; NPC boss attacks gain `floor((m − 1) × 6)` additional basic damage. Regular enemies never read it |
| `boss_armor_per_rank` | 0..=0.05, shipped .02 (missing key: 0). A Personal boss spawns with `round(boss_armor × clamp(1 + boss_armor_per_rank × (min(victories,50) − 4), 1, 4))`, exactly the template armor through rank 4. Exposure still drops it to 0 |
| `npc_boss_armor_rank_step` | 0..=100, shipped 8 (missing key or 0: none). An NPC boss spawns with `boss_npc_armor + min((victories − 4) / step, 20)` armor, the template armor through rank 11 |

The NPC arrival adapter accounts for the discrete attack/defense/Ready/Aim procedures. It does not accelerate NPC turns, change attack rolls, or grant player skills to NPCs. Personal damage scales directly; NPC hostile attack damage receives `floor((enemy_damage − 1) × 6)` authored additional basic damage. Existing armor, active defense and injury stages still resolve normally.

Spawn credits count groups. Each tick adds `arrival_rate / (60 × batch_size)`, so larger groups change how many enemies arrive at once without secretly multiplying the displayed mean enemy rate. Each admitted member independently selects a legal lane and eligible weighted archetype through the existing RNG. A full population discards that group's missing members; it accumulates no deferred catch-up burst. Population and group size remain bounded. A hard scheduling ceiling of 30 enemies/second protects fixed-tick credit invariants even for aggressive overrides.

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
