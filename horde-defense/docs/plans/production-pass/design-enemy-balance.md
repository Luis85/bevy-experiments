# Enemy balance: merged design spec (production pass, goal 6 plus decisions 12 and 13)

**Base.** Design A (minimal-risk; judges 2 of 3). The rule is that every new saved field defaults to a legacy-neutral value and is skipped at that value. Ranks 0-4 are numerically unchanged on Normal. Templates 0-4 are never edited.

**Grafts from B.** Shield ranged armor, the spitter model with wall interception, the dual-trigger exploder, split-child drop exclusion, load-time affix pair validation, the 5-affix table, the BossRank enum with Regional bosses, the 90-tick phase hold, saved per-archetype telemetry, the generator hardening, and the effective-difficulty UI accessor.

**Violations fixed.**
- No wave-1 GOLDEN move.
- No silent Custom loss of the Hard extras (new `Campaign.balance_extras_basis`).
- The streak rule is per seed, not a median.
- Threat-guard rows are mutation-checked.
- Step ordering is self-contained: extras before the ramp, and all 7 archetype records plus waves 5-11 in one commit.
- Boss phases are gated above rank 4 on Normal.
- The start clamp is two-sided.

All rules live in `horde_core` as tested pure functions. `horde_game` only renders. All paths are relative to `horde-defense/`.

## 0. Baselines (wave 0, `target/production-before/`)

Pacing (60 runs):

| Profile | Outcomes |
|---|---|
| expert DEU | DDDVVVVDVDVV, then 48 V |
| novice DEU | DDVDVVDDVV, then all V |
| novice LUX | DVDV, then all V |

- Defeats after the first victory: 2 / 3 / 1.
- Longest win streak after run 10: 50 in every profile.
- Boss TTK: median 6-10 s; late wins about 6 s (victory near 246 s against boss_time 240).
- The tree is exhausted by run 26.

Smoke:
- Personal: stage 1 won on attempt 3, stages 2-5 on attempt 1.
- NPC: stage 1 won on attempt 2.

Root causes:
- From rank 4 the floors equal the template maxima and composition is stuck at tier 4 (progression.rs:134, :247; director.rs:47-50).
- Gear outgrows enemy HP: the gear/HP ratio is 1.26 at rank 10 and 1.93 at rank 58.

## 1. Two-axis threat (resolves the critic gap "two-axis threat is undefined")

**Axis 1, threat rank `r`.**
- `r` = committed victories (`Campaign::progression_rank`, campaign/progression.rs:449). It is frozen at deploy.
- It drives growth, floors, composition tier, elite rate, boss armor and the phase gate.

**Axis 2, front depth.**
- New `progression::FrontPosition { stage: u32, count: u32 }`.
- `depth() = stage / (count - 1)`, or 0 when `count <= 1`.
- `boss_rank()`:
  - `Capstone` if `stage + 1 == count`;
  - else `Regional` if `(stage + 1) % 6 == 0`;
  - else `Sector`.
- `is_checkpoint() = boss_rank() == Regional`. USA (24 stages) gets Regional bosses at 0-based stages 5, 11 and 17, and the Capstone at 23. 23 of 242 countries have one or more checkpoints.
- It is computed in `deploy_internal` from `(Attempt.stage, Country::level_count())` and frozen in `ProgressionSnapshot`.
- `deploy(level)` without a country (GOLDEN and smoke) passes `None`: front ramp 1.0, `Sector`.

**What is not an axis.**
- Static country difficulty is a modifier, already frozen into `Attempt.modifiers` (lifecycle.rs:34-38).
- Player power is rejected as an axis.
- Liberation count is rejected as an axis.

## 2. Axis 1 data

### Growth knee

New keys in `progression.json` and `ProgressionCatalog`, each serde-defaulted so legacy `LASTLIGHT_PROGRESSION` overrides behave as today:

| Key | Default (legacy) | Bundled |
|---|---|---|
| `growth_knee_rank` | `u32::MAX` | 4 |
| `health_per_rank_late` | 0 | 0.04 |
| `damage_per_rank_late` | 0 | 0.018 |

`snapshot()` (progression.rs:174-175) becomes:
- `enemy_health = 1 + .015·min(r,k) + late_h·max(r-k, 0)`
- `enemy_damage = 1 + .008·min(r,k) + late_d·max(r-k, 0)`

`threat_growth` scaling (campaign/progression.rs:524-527) is unchanged.

Gear/HP ratio (gear = `1 + .09·(min(30, 1 + r/2) - 1)`):

| Rank | Old | New |
|---|---|---|
| 4 | 1.11 | 1.11 |
| 5 | 1.10 | 1.07 |
| 10 | 1.26 | 1.12 |
| 20 | 1.46 | 1.12 |
| 40 | 1.75 | 1.12 |
| 58 | 1.93 | 1.12 |

HP × arrival against gear (`arrival_per_rank` .01) at ranks 10 / 20 / 40: old .87 / .82 / .80, new .99 / 1.07 / 1.25.

### Floors and composition tiers

- Append `pressure_floors` rows for ranks 5..=28. The index stays the rank (progression.rs:167), so no lookup change.
  - `start_rate = .44 + .0025(r-4)`
  - `rate_cap = 1.76 + .01(r-4)`
  - `boss_health = 6800 + 600(r-4)` (21200 at rank 28)
  - `npc_boss_health = 67 + 2(r-4)`
  - `boss_time = 240`
- `wave_tier` map:

| Ranks | Tier |
|---|---|
| 5 | 4 |
| 6-7 | 5 |
| 8-9 | 6 |
| 10-12 | 7 |
| 13-15 | 8 |
| 16-19 | 9 |
| 20-27 | 10 |
| 28+ | 11 |

  This gives tiers 7 / 10 / 11 at ranks 10 / 20 / 40, so the "differ" acceptance holds.
- Lift `wave_tier > 4` to `>= MAX_WAVE_TIERS` (64) at progression.rs:134 and :247.
- 12 waves with a maximum tier of 11, and `a.level <= 4` (sim.rs:910), so `level % len` (enemy_catalog.rs:199) never wraps.
- Load-time check `every_floor_tier_exists_in_enemy_catalog` replaces B's saturation change, which would have altered override semantics.

### Boss armor

New catalogue keys, defaulting to 0 when missing:
- `boss_armor_per_rank` (.02)
- `npc_boss_armor_rank_step` (8)

Snapshot fields:
- `boss_armor_multiplier = 1 + .02·max(0, min(r,50) - 4)`, clamped to 1..=4
- `npc_boss_armor_add = max(0, r-4) / 8`, clamped to 0..=20

They apply at director.rs:195-205. Exposure still zeroes armor (sim.rs:1203-1206). Boss damage already scales through `progression.enemy_damage` (combat.rs:107).

### Elite curve

`progression.json` block `elite { min_rank 3, start_bp 200, end_rank 23, end_bp 1500 }`.
- Serde default: `end_bp 0`, which means no elites for legacy overrides.
- Pure function `progression::elite_bp(cfg, r)`: 0 if `r < 3`, else `200 + 65·(min(r,23) - 3)`. Held at 1500 above rank 23.
- Validation requires a linear curve.

## 3. Axis 2: front ramp, Regional and Capstone (decisions 12 and 13)

**Front ramp.**
- `front_ramp = 1 + front_ramp_max(.15) · extras.front_ramp · depth`.
- It is applied inside `Campaign::progression_for(mode, rank, lanes, Option<FrontPosition>)`, which multiplies `snapshot.enemy_health` and `snapshot.arrival_multiplier`.
- Zero read-site changes: lifecycle.rs:41 and sim.rs:552 already read them.
- Final/first EHP × arrival at equal rank: Normal 1.15² = 1.32; Hard 1.225² = 1.50; Easy 1.1125² = 1.24.
- No tier offset (B's tier offset is rejected).

**Boss rank multipliers.** `progression.json` block `boss_ranks`, folded into snapshot fields at deploy:

| Rank | HP | Armor | NPC armor | Reward | Extra transitions |
|---|---|---|---|---|---|
| Regional | ×1.2 | ×1.0 | +0 | ×1.2 | 0 |
| Capstone | ×1.4 | ×1.25 | +2 | ×1.4 | 1 |

- The capstone reward equals its HP multiplier, so replays are not a farm.
- The capstone keeps its own template's BossKind; there is no remap at sim.rs:910.
- campaign-defenders owns the checkpoint coin grant and notice.
- Recommendation to vfx: force the night theme on Regional stages, which also fixes QA19.

## 4. Archetypes (6 plus 1 child; `EnemyKind` stays 5 variants)

### New optional fields

Each field gets a serde default, `skip_serializing_if` its default, and a bound in BOTH `AuthoredCatalog::resolve` and `ResolvedEnemy::validate`.

| Location | Field | Default | Bound |
|---|---|---|---|
| `attacks` | `reach` (yd) | 0 | 0..=12 |
| `defenses` | `ranged_armor` | 0 | 0..=1000 |
| `defenses` | `npc_ranged_armor` | 0 | 0..=100 |
| `bodies` | `stagger_immune` | false | – |
| archetype | `death_effect` | None | see below |
| archetype | `group_size` | 1 | 1..=6 |
| archetype | `tags` | [] | ≤8 entries, `valid_id`, never used for numbers |
| archetype | `elite_eligible` | true | catalogue only, not snapshotted |

`DeathEffect` variants:
- `Burst { radius f32 (0,3], damage i32 1..=10000, structure_damage i32 0..=10000, npc_dice u8 1..=20, npc_add i32 -30..=100, fuse bool }`
- `Split { archetype_id, count u8 1..=4 }`. The target must exist, have no `unlocked_by`, not be itself, have no `death_effect`, and have `group_size` 1. Depth 1 is checked at load.

### Records

Appended to `enemy_catalog.json`. Component rows are new; there are no edits to existing rows.

| id | name | kind | sprite / tint / scale | Personal stats | NPC stats (hp / ht / skill / dodge / armor / dice / add) | special | tags |
|---|---|---|---|---|---|---|---|
| exploder | Bloater | Shambler | Shambler [.72,.9,.5] 1.15 | hp 40, spd .42, dmg 0, windup 1.5, reward 3/2 | 10 / 10 / 9 / 6 / 0 / 1 / 0 | Burst{1.6, 30, 40, 3, 0, fuse true}; urgency -4 | volatile, hazard |
| splitter | Brood husk | Brute | Brute [.85,.78,.9] 1.05 | hp 130, spd .33, dmg 12, windup 1.8, armor 2, reward 9/6 | 18 / 12 / 11 / 6 / 2 / 2 / 0 | Split{splitter-brood, 3} | splitting |
| splitter-brood | Broodling | Runner | Runner [.85,.78,.9] .6 | hp 14, spd 1.0, dmg 3, windup .8, reward 1/1 | 5 / 9 / 9 / 8 / 0 / 1 / -2 | elite_eligible false; in no wave | swarm, brood |
| spitter | Spitter | Runner | Runner [.6,.9,.85] .95 | hp 30, spd .55, dmg 6, windup 1.6, reward 5/3 | 8 / 10 / 11 / 8 / 0 / 1 / 1 | reach 4.0; urgency -2 | ranged, hazard |
| shield-bearer | Riot shield | Armored | Armored [.8,.85,.95] 1.05 | hp 80, spd .40, armor 6, cAF .5, dmg 8, windup 1.4, reward 8/5 | 12 / 11 / 11 / 7 / 3 / 1 / 2 | ranged_armor 14, npc_ranged_armor 4 | shielded |
| sprinter | Sprinter pack | Runner | Runner [1,.85,.7] .85 | hp 16, spd 1.45, dmg 3, windup .8, reward 2/1 | 6 / 9 / 10 / 9 / 0 / 1 / -1 | group_size 4 | swarm |
| juggernaut | Juggernaut | Demolisher | Brute [.7,.7,.75] 1.4 | hp 420, spd .24, armor 6, dmg 22, windup 2.2, structure ×3, reward 20/12 | 30 / 14 / 12 / 5 / 5 / 3 / 2 | stagger_immune | heavy |

### Waves 5-11

Appended in the same commit as the records. Each wave inherits the previous wave and overrides the listed weights.

| Wave | Weights changed or added | Mean HP per pick (boss variants locked / all) |
|---|---|---|
| 4 (existing) | – | 51.7 / 68.7 |
| 5 | shambler 12, skitter 8, sprinter 10, spitter 8 | 53.3 / 69.4 |
| 6 | shambler 10, runner 20, skitter 6, exploder 10, shield-bearer 8 | 55.4 / 70.4 |
| 7 | splitter 8, juggernaut 3 | 67.6 / 79.6 |
| 8 | armored 22, plated-runner 18, hungry 10, sprinter 12, spitter 10, juggernaut 4 | 69.1 / 80.4 |
| 9 | runner 18, exploder 12, shield-bearer 12, splitter 10, juggernaut 6 | 74.6 / 84.5 |
| 10 | shambler 8, demolisher 18, sprinter 14, spitter 12, juggernaut 8 | 78.3 / 87.4 |
| 11 | runner 16, brute 24, shield-bearer 14, splitter 12, juggernaut 10 | 84.0 / 91.9 |

- Mean HP per pick counts group members and split children.
- New fn `EnemyCatalog::mean_pool_health(tier, unlocked, mode)`.
- The new archetypes appear only in waves 5 and above.
- Item drop tiers still filter on `a.level` (combat.rs:374). This is a note for loot-economy.

### Mechanics

All mechanics are deterministic. `a.rng` is drawn only when an effect triggers, which never happens at rank 0, template 0 or without a country.

**Spitter.**
- `ranged_stop(a,e) = clamp(DEFENSE_LINE + reach, DEFENSE_LINE, 0.5·a.width_yards)`, using the frozen width of 8-30 yd (balance.rs:117-118). That gives 2.5 yd at width 8 and 4.0 at 12.
- In `move_enemies` (movement.rs:13; the stop and contact block is movement.rs:34-108) the stop is `max(structure blocker, ranged_stop)`. A live non-trap structure ahead of the stop is met in melee. Otherwise the spitter stations at the stop.
- Target: the nearest live non-trap structure between the station and the line (walls intercept spit), else the defender.
- Integration ruling (W1-69, user decision 2026-11-06): enemies never block each other, so a stationed spitter needs no queue or passing exemption, and followers simply walk through it. W1-69 removed the lane queue, `queue_front` and the passing gate.
- The contact test (`blocked`, movement.rs:71) uses `ranged_stop` (reach 0 is identical to today).
- The attack uses the existing Windup→attack path (combat.rs:102-245).
  - Personal: `damage - armor`, halved by brace.
  - NPC: `npc_skill + rules::range_penalty(distance)` against the defender's dodge.
  - It emits the new `EventKind::Spit` (from = spitter, to = target).
- Resolution guard: no damage if `x > width/2 + .01`. Tested; it cannot trigger in valid play.

**Exploder (dual trigger).**
- Fuse: when its windup completes in contact, it detonates instead of attacking.
  - Damage hits its target (defender or structure), and flat `structure_damage` hits structures within the radius. Other enemies are not hit.
  - Personal: `(damage × enemy_damage × balance.enemy_damage) - armor`, halved by brace.
  - NPC: new `rules::resolve_blast(rng, dice, add, dodge: Option<i32>, dr, ht)` with no attack roll and dodge allowed.
  - The exploder is removed in the same pass. Its id goes into `reward_ledger`, so reap pays no reward and writes no corpse or Kill event.
  - Emits `EventKind::Detonation` with the target's `target_id` (a structure id, or None for the defender) and `SimEvent::Detonation { trigger: DetonationTrigger::Fuse { target_id, to } }` (W1 review: `target_id` alone cannot tell a defender fuse from a death burst).
- Death: when killed, it bursts on enemies and structures within the radius, never the defender. The kill pays its reward normally.
  - Bursts are collected in `reap` (combat.rs:308) and applied after the loop. Enemies killed by a burst are reaped next tick, so chains resolve one tick later and terminate (each enemy bursts once).
  - Emits `Detonation` with `target_id` None and `trigger: DetonationTrigger::Death`.

**Splitter.**
- On death it spawns `min(count, enemy_cap - non_boss_count)` children.
- The child id is resolved from the current catalogue at split time, falling back to `resolve_kind(Runner)`.
- Placement: `max(parent.x, DEFENSE_LINE) + 0.35k`, same lane.
- Children get `Enemy.split_child = true`. They are never elite and never split. They are excluded from the 3% roll and the every-20th-kill drop, but count as kills and pay their own reward once.
- Emits `EventKind::Split`.

**Shield-bearer.**
- `enemy_armor_vs(a, e, HitSource)` replaces `enemy_armor` (sim.rs:1203) at manual.rs:79, :174, structures.rs:65 and npc.rs:251.
- `HitSource { Shot, Turret, Melee, Area, Trap, Fire }`. `ranged_armor` (NPC: `npc_ranged_armor` added to DR) applies only for Shot and Turret, and only while not in contact.
- Counters: grenades, traps, or letting it close for melee.

**Juggernaut.**
- `stagger_immune` skips the HitStop refresh (sim.rs:175-178) and NPC prone/stun movement and attack loss (movement.rs:50, sim.rs:1187-1189).
- Injury rolls are unchanged, so RNG use is identical.

**Sprinter.**
- One weighted pick spawns `group_size` members in the chosen lane at `spawn_x + 0.65k`. There is no extra RNG draw.
- Each member respects the cap, through `spawn_resolved_enemy(a, profile, lane, x, elite_eligible)`.

**Counter seams for progression-tree (decision 19).**
- Spit and blast damage to the defender goes through `Attempt::hazard_damage(raw)`, which multiplies by a skill modifier (default 1.0; progression-tree owns the field).
- `armor_penetration` reduces `ranged_armor`.
- The `swarm` and `heavy` tags are exposed for perk targeting.
- Telemetry `hazard_damage_taken` gives the counter's value.

## 5. Elites

**Roll.**
- Keyed hash with no combat RNG: `hash::keyed_mix(seed ^ mix(attempt.id) ^ mix(enemy.id) ^ ELITE_ROLL) % 10000 < snapshot.elite_rate_bp`. The SplitMix moves from quality.rs:113-118 to `hash.rs`, with a pin that quality outputs are unchanged.
- The affix is picked by a second tag, `ELITE_AFFIX`, weighted over the eligible affixes.
- Only director wave spawns are eligible: not bosses, not Caller summons (director.rs:99), not split children, not `elite_eligible:false`.
- Worker games differ through `seed ^ recruit` (parallel.rs:97).

**Rate composition.** `elite_rate_bp = min(10000, round(elite_bp(r) × extras.elite_rate))`, frozen at deploy.

**Base multipliers.** `enemy_catalog.json` `elite_base { health 1.5, reward 4, xp 3, scale 1.15 }`.

**Affixes.** `elite_affixes` table; serde default empty, which means no elites:

| id | weight | Personal | NPC |
|---|---|---|---|
| hardened | 30 | HP ×1.6, armor +4 | hp +4, armor +2 |
| frenzied | 25 | speed ×1.3, windup ×.75 | skill +2, dodge +1 |
| vicious | 20 | damage ×1.5 | damage_add +3 |
| shielded | 15 | ranged_armor +10 | npc_ranged_armor +3 |
| volatile | 10 | HP ×1.2 plus Burst{1.4, 24, 30, 2, 0, fuse false} | – |

- `volatile` is offered only to archetypes without a `death_effect`.
- Bounds: health and damage ×1..=4, speed 1..=2, windup .5..=1.
- Every archetype × affix pair must pass `ResolvedEnemy::validate` at catalogue load, or the load is rejected. There are no runtime clamps.

**Bake.** `ResolvedEnemy::with_affix(base, affix)` bakes the stats into the spawned snapshot and sets `elite: Some(EliteMark { affix_id, name })`. Every sim read through `enemy_profile` is unchanged. The spawn emits the existing `Spawn` event; there is no EliteSpawn kind.

**Drop.**
- Rule: `e.boss || elite || (!e.split_child && (a.rng.below(100) < 3 || a.kills.is_multiple_of(20)))`.
- Quality: loot-economy's `QualityCatalog::roll_with_floor(.., floor: Uncommon)`, which zeroes the weights whose final ABSOLUTE quality is below Uncommon. This matches decision 6's absolute auto-sell, so elite drops are never auto-sold by the "below Uncommon" rule.

**Health guard.**
- One shared `encounter_health_bound(level, mode, progression, modifiers)`, used by lifecycle.rs:76-89 and persistence.rs:360-370.
- It includes `elite_base` × the maximum affix health multiplier, `boss_health_multiplier`, and the front ramp (already inside `enemy_health`).
- Gameplay caps stay at 224 plus 1 boss.

## 6. Bosses: phases only, no new BossKind

**Phase table.** `enemy_catalog.json` `boss_phases`: exactly ONE profile per BossKind ("one phase per BossKind").

`BossPhaseProfile { name 1..=40, speed_mult 1..=2, armor_mult .25..=1, call_interval_mult .3..=1, siege_recovery_mult .3..=1, hold_ticks 0..=180 }`.

| BossKind | Profile |
|---|---|
| Breacher | "Breach": speed ×1.4, armor ×.6, hold 90 |
| Caller | "Chorus": call interval ×.6, hold 90 |
| SiegeBrute | "Enrage": siege recovery ×.6, speed ×1.2, hold 90 |

**Transitions.**
- Thresholds are the code constant `BOSS_PHASE_THRESHOLDS = [50, 25]` (% of max HP).
- Transition n applies `mult^n`, clamped at the profile bounds (speed ≤ 2, armor ≥ .25, interval ≥ .3, recovery ≥ .3).
- At most one transition per tick. Each emits `EventKind::BossPhase` (value = n) and pushes `telemetry.boss_phase_ticks`.
- During the hold (90 ticks = 1.5 s) the boss neither moves nor counts down its attack. That is the readable tell that pairs with ux's ≥1.5 s banner (the visual twin).

**Transition count.** `snapshot.boss_phases = min(2, (r > 4) as u8 + extras.boss_phase_bonus + rank.extra_transitions)`:

| Preset | Sector or Regional | Capstone |
|---|---|---|
| Easy / Normal, r ≤ 4 | 0 | 1 |
| Easy / Normal, r > 4 | 1 | 2 |
| Hard | 0 at r ≤ 4, 2 above (W2-36, see below) | 1 at r ≤ 4, 2 above |

W2-36 amended the Hard row on the plan §2.5 default (the W2-26 pin reading): `boss_phase_bonus` counts only past rank 4, so Hard at r ≤ 4 has Normal's count. The original row read "1 at r ≤ 4, 2 above" and Capstone 2.

- Smoke (ranks 0-4, no country) and the first-victory fights are unchanged on Normal.

**Saved state.** `Enemy.boss_state: Option<BossState { profile: BossPhaseProfile, transitions: u8, entered: u8, hold_until: u64 }>`, snapshotted at spawn like `boss_timing` (director.rs:212). None means a legacy boss, which behaves as today.

**Code touch points.**
- `const BOSS_BASE_SPEED = 0.6` and `fn boss_speed(e)` replace the literals at movement.rs:47 and sim.rs:1201.
- `armor_mult` applies in `enemy_armor_vs`.
- `call_interval_mult` applies at director.rs:96.
- `siege_recovery_mult` applies at combat.rs:120.

**TTK.** The 20-60 s target is reached through the floor `boss_health` slope, the capstone HP and the holds, tuned in wave 4 from the 6 s late baseline.

## 7. Hard extras

**Data.**
- `balance.json` `BalancePresetDefinition.extras: PresetExtras { elite_rate f32 0..=3, front_ramp f32 0..=3, boss_phase_bonus u8 0..=2 }`.
- Serde default is Normal's values.

| Preset | elite_rate | front_ramp | boss_phase_bonus |
|---|---|---|---|
| Easy | .5 | .75 | 0 |
| Normal | 1 | 1 | 0 |
| Hard | 1.5 | 1.5 | 1 |

**`BalanceSettings` is UNCHANGED.** Its struct, `deny_unknown_fields`, `for_preset` equality and the three saved copies are all untouched. There is no migration step and no digest bytes.

**Custom fix (judge violation).**
- New saved `Campaign.balance_extras_basis: BalancePreset`, serde default Normal, skipped if Normal. It must never be Custom (validated on load).
- In `Campaign::set_balance_settings` (campaign.rs:196-208), after normalisation:
  - if the result is a real preset, the basis is reset to Normal;
  - if it is Custom, the basis becomes the requested preset when that was not Custom (the player was editing Hard), else the old preset when that was not Custom, else the old basis.
- `Campaign::balance_extras() = extras_for(preset == Custom ? basis : preset)`.
- A Hard player who changes lanes or width keeps the Hard elite rate, ramp and phases.
- The UI shows "Custom (Hard rules)" through a core accessor.
- `clone_for_worker` copies the basis.
- v6 Custom campaigns get basis Normal, which matches today's behaviour.

**Timing.** The extras are frozen into the snapshot at deploy, so SetBalance affects only the next deployment.

## 8. Country difficulty 0.85-1.60, generator and start clamp

**Formula.** With `h = fnv1a64("difficulty:" + code)`, `difficulty = (lo + h % (hi - lo + 1)) / 100`, in integer hundredths:

| Stages | Band |
|---|---|
| ≤3 | 85-115 |
| 4-9 | 100-135 |
| ≥10 | 120-160 |

- Measured over 242 countries: min .85, max 1.60, 50 distinct values, mean 1.049.
- Examples: LUX .92, DEU 1.04, BEL .97, USA 1.27, CHN 1.52, AUT .85, BGD 1.60.
- The band follows size (population already sets stage count), not national traits. WORLD.md must say so.

**Start clamp.**
- `Country::effective_difficulty(&WorldProgress)` clamps the campaign's start country (world.rs:447-449) to [0.95, 1.05]. Other countries use the raw value. No saved field.
- Used by `deploy_internal` (lifecycle.rs:34-38) and exposed as `Game::effective_country_difficulty(id)`. briefing.rs:103 and camp.rs:200 must display it.
- Front ordering (world.rs:521), the recommendation (loadout.rs:364) and the pacing next-country (pacing.rs:215) keep the raw value. The recommended start becomes AUT (.85 raw, .95 effective).
- tests/simulation.rs:372 switches to `effective_difficulty`; reason: the start clamp.

**`scripts/generate_world.py`.**
- Default mode `--from-existing`: offline. It loads the committed world.json, recomputes only `difficulty`, and merges `stage_themes` from a new authored `scripts/world_stage_themes.json` (initially a byte copy of today's 10). vfx's storm theme is appended there.
- The writer becomes `json.dumps(out, indent=2, ensure_ascii=False) + "\n"`, replacing the compact separators at :115. Landed first in a writer-only commit, where `--check` must exit 0 with the old formula.
- `--check` exits 1 if the output differs from the committed bytes.
- `--rebuild`: needs explicit GeoJSON paths or `--download`. It verifies sha256 against world.json `sources` (refuses unless `--allow-source-drift`) and refuses any stage-count change (unless `--allow-stage-count-change`).
- Update the comment at :98.

## 9. Telemetry, pacing harness, pacing_targets

**Saved telemetry.** Every field below is frozen in wave 1, populated in wave 2, serde-defaulted and skipped when empty or zero.

| Owner | Field | Type |
|---|---|---|
| `CombatTelemetry` (telemetry.rs:5) | `spawned_by_archetype` | `BTreeMap<String,u32>`, ≤256 keys, validated |
| | `kills_by_archetype` | `BTreeMap<String,u32>`, ≤256 keys, validated |
| | `elites_spawned` | u32 |
| | `elites_killed` | u32 |
| | `hazard_damage_taken` | u64 |
| | `boss_phase_ticks` | `Vec<u64>`, ≤2 |
| `RunRecord` (stats.rs:143, per-field defaults) | `elite_kills` | u32 |
| | `boss_ttk_ticks` | `Option<u64>` = defeat - arrival |
| | `boss_rank` | `BossRank` |
| `Report` | `boss_rank` | `BossRank` |

- Bosses are not keyed: schema-v7 §3.12 (final) excludes bosses and unresolved legacy enemies from every per-archetype map, and a `boss:<BossKind>` key would fail `valid_id` (`:` is not an id byte).
- The third map is `damage_taken_by_archetype` (`BTreeMap<String,u64>`, the defender's damage taken per archetype). It uses the same keys and bounds; schema-v7 §3.12 is its single definition.

**Pacing harness** (sim/pacing.rs):
- `PacingProfile.preset`, plus `standard()` and `lux()` constructors.
- `sweep(profile, seeds)` uses `std::thread::scope`, deterministic per seed.
- `PacingRun` gains `threat_rank`, `boss_ttk_ticks`, per-archetype maps, `elite_kills` and `defeat`.
- `PacingSummary` reports:
  - pooled defeat rate per sliding 20-run window after the first victory;
  - max win streak in runs 11-60, per seed;
  - TTK median;
  - archetype coverage;
  - LUX runs to liberation per preset.
- examples/pacing.rs gains `--seeds`, `--preset` and `--csv`.

**`tests/pacing_targets.rs`**: `#[ignore = "release-only"]`, Normal, seeds 1..=20, 60 runs.
- Pooled defeat rate in [.10, .25] in every 20-run window after the first victory.
- NO seed has a 15-win streak in runs 11-60.
- Every catalogue archetype id (splitter-brood included) is spawned and killed.
- Boss TTK median in [20, 60] s.
- LUX median runs to liberate: Easy ≤ 6, Hard ≤ 1.5 × Normal.
- A BGD start (clamped to 1.05) gets its first victory by run 4.
- An AUT start (clamped to .95) has no run-1 victory and a first victory on runs 2-4.
- Documented in horde-defense/AGENTS.md. Run it after the debug builds.

**Cheap acceptance checks** that run as normal tests: ramp ≥ 1.3, tiers differ at 10 / 20 / 40, the elite curve.

## 10. Victory→threat guard (test it, do not re-implement it)

`tests/threat_guard.rs` is table-driven:
- Positive rows: Personal first clear, Personal replay, NPC replay, reopened outbreak, worker `last_result` (parallel.rs:270), worker `pending_runs` from a chained operation (parallel.rs:236), Regional, Capstone, Hard win, elite-kill run, and takeover (added when campaign-defenders lands `SetNextDefender`).
- Negative rows: defeat, withdraw, Retreat & train, abstract garrison containment.
- Each positive row asserts +1 exactly once and `next_victory_progression().rank == rank + 1`.
- Property test in tests/properties.rs: rank == committed victories over 120 seeds.
- Existing-path rows are mutation-checked: removing `saturating_add` at campaign/world.rs:47 makes them fail. Record that in the commit message.
- campaign/world.rs:35-54 stays unchanged.

## 11. 400-enemy stress (perf-owned seam)

- Gameplay caps stay at 224 global, 145 per rank, plus 1 boss.
- review/test-support `sim::fixtures::stress_battle(400)` builds a tier-11, 17-lane, boss, night attempt and sets `attempt.progression.enemy_cap = 400`. All 400 enemies run the real movement, combat and reap paths.
- Its save is rejected by the existing validation (progression.rs:238). A test pins that `Game::to_save_json` returns Err, so autosave cannot persist it.
- The fixture includes a splitter and exploder cluster.

## 12. Determinism

- Elite and affix rolls are keyed hashes.
- Spawn placement is RNG-free.
- Effect RNG is drawn only when an effect triggers.
- Waves 0-4 and the ranks 0-4 numbers are untouched.
- Phase transitions need r > 4 on Normal; elites need r ≥ 3; the ramp and Regional/Capstone ranks need a country.
- Therefore enemy-balance never moves `GAMEPLAY_TRACE`.
- GOLDEN moves only once, in wave 2, when telemetry starts populating (see digest moves).
- Run tests/determinism.rs after every enemy-balance commit.

## 13. Cross-remit handoffs

**New `EventKind` and `SimEvent` variants** (appended): `Spit`, `Detonation`, `Split`, `BossPhase`.
- They land with stub `horde_game` match arms in the same commit.
- audio adds cues (banks keyed by archetype id; Windup and Contact cues).
- vfx adds visual twins and reduced-effects variants, and draws reach and burst radius in world units so they read at 3-17 lane zoom-to-fit. Construction mode must not rescale.
- ux adds the BossPhase banner (≥1.5 s).

**Naming adopted for save-determinism's wave-1 freeze:**
- `ResolvedEnemy.elite` (not `Enemy.elite` or `elite_affix`);
- the snapshot fields listed in the saved-fields table;
- `Campaign.balance_extras_basis`;
- `BalanceSettings` gets NO new fields.

**Other remits:**
- `CorpseRecord.archetype_id` and `CorpseRecord.elite` are audio/vfx/save-determinism-owned. If added, they need per-field defaults.
- loot-economy owns `roll_with_floor`, and must accept that splitter children are excluded from drops.

## 14. Risks and numbers to measure

**Risks.**
- Bots aim almost perfectly, so the 10-25% defeat window is a floor, not proof of human difficulty. A human playtest is logged as pending.
- After the tree is exhausted, variance comes from the front-ramp sawtooth and the capstone. `front_ramp_max` may need to rise.
- The defeat window can stay under 10% if progression-tree's repricing still lets the tree exhaust by run 26. Tune jointly in wave 4, progression first.
- tests/world_progression.rs:285-330 (USA 8 stages, 6 retries) now meets the late slope, tiers 5-6, elites, phases at rank 5 and the 1.05 clamp. Any tuning or test change needs a written reason.
- The capstone at LUX stage 2 (rank about 2): HP ×1.4 plus 1 transition can delay the first liberation and the unlock order Fortifications → Mara → Operations. The capstone HP is the knob.
- Smoke stages 4-5 (ranks 3-4) now see 2% / 2.65% elites with forced drops. Re-measure and log.
- Country difficulty up to 1.6 also means reward ×1.6. Watch coins per minute in progression-tree's farm profile, including the capstone replay check (≤ 1.1× frontier).
- Exploder chains and splits near the cap add per-tick work. Cover them in PERF_BUDGET.
- The pacing_targets runtime can exceed 10 minutes. Parallelise per seed and stop LUX profiles at liberation.

**Numbers to record before and after:**
- defeat rate per window;
- max streak per seed;
- TTK median and p90;
- LUX runs per preset;
- final/first pressure ratio (baseline 1.0 plus the rank drift);
- smoke attempts, kills and coins per stage in both modes;
- elite fraction at rank 13;
- mean pool HP per tier;
- coins per minute (frontier vs capstone replay).

---

## Integration rulings

This section was added on 2026-11-05 when the synthesized spec above was copied into the plan. The spec above is kept verbatim as the design record. **Where this section and the spec disagree, this section wins.** The rulings come from four sources, applied in this precedence order: user decisions (U), orchestrator rulings (R, RL), master-planner reconciliations (P) and the five integration cluster rulings (C, D, E, S, W). The final saved-field table is `schema-v7.md`, the commit order is `waves.md` and the proof for every criterion is `acceptance.md`, all in this folder. The plan itself is `docs/plans/2026-11-05-production-pass.md`; any reference above to `docs/plans/2026-11-04-production-pass.md` means that file.

Each entry gives the binding ruling and then what it supersedes in **this** spec.

### Orchestrator rulings

#### R7. GOLDEN / GAMEPLAY_TRACE re-bless cadence

- **Owner / wave:** save-determinism, wave 2, 4.
- **Ruling:** save-determinism re-blesses once per wave in a dedicated commit with a written reason per digest; expected moves in waves 2 and 4; a wave-1 format-only move is allowed only if the v7 schema makes it unavoidable, with a reason. Implemented by the provisional-pin ledger (P3).
- **Supersedes in this spec:** "The single enemy-balance GOLDEN move" wording: telemetry is a provisional cause T folded in W2-49.

#### R8. Single owners

- **Owner / wave:** progression-tree, wave 1-4.
- **Ruling:** Pacing harness v2 = progression-tree (wave 2; CD and EB add profiles on its API); per-archetype telemetry = enemy-balance; SHA-256 / provenance / orphan helpers and scripts/lib/Native.ps1 = qa-captures (wave 1; audio, vfx and perf reuse them); measure_performance.ps1 = perf-architecture on top of Native.ps1; storm tagging = vfx presentation table only (no world.json change); regional checkpoints are derived (no saved field), presentation lines by CD plus EB's Regional boss rank (HP and reward x1.2) in wave-4 balance, no theme-rule change; save_v7.json is cut at the wave-1 close and frozen, later cases use inline JSON or new fixtures; stress scene raises the staged enemy cap only behind a review/test-only seam (no production save accepts &gt; cap+1), owner perf.
- **Supersedes in this spec:** Own pacing harness v2, PacingSummary, standard()/lux().

### Master-planner reconciliations

#### P1. Wave-tier bound

- **Owner / wave:** enemy-balance, wave 1 (bound), 2 (catalogue check).
- **Ruling:** MAX_WAVE_TIER: usize = 15, inclusive, at progression.rs:134 and :247, in the wave-1 schema commit W1-28; every_floor_tier_exists_in_enemy_catalog lands with the content in W2-31. The 64 bound is withdrawn.
- **Supersedes in this spec:** "&lt; 64 / MAX_WAVE_TIERS"; the economy cluster's wave-2 placement of the bound.

#### P3. Provisional-pin ledger

- **Owner / wave:** save-determinism, wave 1 (W1-07).
- **Ruling:** determinism.rs holds GOLDEN, TRACE and PROVISIONAL: Option&lt;([u64;6],[u64;6])&gt; plus PROVISIONAL_CAUSES (commit tag, class F/C/G/T, reason). A digest-moving commit sets PROVISIONAL and appends its cause; the wave's SD re-bless folds it and resets it to None; the wave-close critic checks None.
- **Supersedes in this spec:** Telemetry re-pin wording.

#### P6. Regional checkpoint ownership

- **Owner / wave:** enemy-balance, wave 2, 4.
- **Ruling:** enemy-balance owns FrontPosition and Country::regional_checkpoints() (wave 2); campaign-defenders owns only the presentation lines (wave 4, W4-02). No theme change.
- **Supersedes in this spec:** Confirms the derivation.

#### P7. Pacing harness owners

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** PT owns sim/pacing.rs v2; EB, CD and LE add registry rows and pure evaluators in their own commits; EB owns per-archetype telemetry population.
- **Supersedes in this spec:** Harness rewrite.

### Cluster: economy-progression

#### E3. Hard extras and Custom semantics

- **Owner / wave:** enemy-balance, wave 1 (schema), 2 (fold).
- **Ruling:** Catalogue BalancePresetDefinition.extras: PresetExtras{elite_rate_pct: u16, front_ramp_pct: u16, boss_phase_bonus: u8}, default Normal {100,100,0}, Easy {50,75,0}, Hard {150,150,1}; BalanceSettings unchanged. Saved Campaign.balance_extras_basis (skip Normal, never Custom, Normal unless preset is Custom); set_balance_settings keeps the requested/previous named preset as the basis on a Custom edit. Accessors balance_extras_preset() / balance_extras(). Worker clone and op-chain sync copy the basis. Wave-2 fold is integer math with literal-1.0 short-circuits.
- **Supersedes in this spec:** f32 PresetExtras{elite_rate, front_ramp, boss_phase_bonus}.

#### E7. Keyed hash module and tags

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** Only crate::hash: pub(crate) mix (moved byte-identically from quality.rs:112-117), keyed(seed, a, b, tag) = mix(seed ^ mix(a) ^ mix(b) ^ tag), mod tags {QUALITY (unchanged), ELITE, AFFIX, ELITE_PICK, RECRUIT_OFFER, MILESTONE} as big-endian ASCII. Users: elite spawn keyed(seed, a.id, e.id, ELITE) % 10000 &lt; elite_rate_bp (director wave spawns only); affix keyed(.., AFFIX); elite-only drop keyed(.., ELITE_PICK); recruit offer keyed(seed, next_recruit_id, fnv1a(country), RECRUIT_OFFER); milestone keyed(seed, rank, 0, MILESTONE). The presentation scan forbids hash::keyed.
- **Supersedes in this spec:** hash::keyed_mix, ELITE_ROLL, ELITE_AFFIX.

#### E8. Elite drop: draw counts, split children and quality floor

- **Owner / wave:** loot-economy, wave 2.
- **Ruling:** Elite and split_child flags never change how many a.rng draws a kill consumes: rolled = !e.boss && below(100) &lt; 3; normal = e.boss || (!split_child && (rolled || kills % 20 == 0)); elite = resolved.elite.is_some(); if normal || elite pick from DropTable: normal uses a.rng.below(total), elite-only uses keyed ELITE_PICK. Split children take and discard the below(100) draw and never drop. Quality via roll_relative(base, elite.then_some(Uncommon), ..) with sub-floor zeroing; bosses keep their forced drop with no floor. drop_tier Some path only.
- **Supersedes in this spec:** `e.boss || elite || (!e.split_child && ..)` (skips the draw); roll_with_floor alias.

#### E9. Wave-tier validation bound

- **Owner / wave:** enemy-balance, wave 2 (superseded by P1: wave 1).
- **Ruling:** MAX_WAVE_TIER: usize = 15 at progression.rs:134 and :247 (wave_tier &gt; MAX_WAVE_TIER rejected); load-time every_floor_tier_exists_in_enemy_catalog (EnemyCatalog::wave_pool wraps silently today). P1 moves the bound into W1-28; the catalogue check stays with the content (W2-31).
- **Supersedes in this spec:** "&lt; 64 / MAX_WAVE_TIERS 64".

#### E13. Skip predicates for neutral-1.0 f32 multipliers

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** crate::schema: is_default, is_false, is_zero_f32 (bits, -0.0 written), is_one_f32 (bits), one_f32. Neutral-1.0 f32 fields allowed with skip_serializing_if is_one_f32: ProgressionSnapshot.{front_ramp, boss_armor/health/reward/damage_multiplier}, SkillModifiers.hazard_damage_multiplier, and later heavy_target_damage_multiplier. Writers produce the literal 1.0 at neutral inputs; no tolerance compare; validation rejects non-finite.
- **Supersedes in this spec:** Snapshot multipliers stay f32 with is_one_f32.

### Cluster: enemies-world-pacing

#### W-1. Pacing harness: one API, one acceptance file, named profiles

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** PT owns sim/pacing.rs v2 and examples/pacing.rs: PacingProfile{id, name, seed, country, aim, spend, max_runs, difficulty, plan: PacingPlan, stop_after}, PacingPlan{equip_drops, sell, defender, farm, train_companions}, by_id, DEFAULT_PROFILE_ID "expert-deu", pacing_seed, pacing_sweep (thread::scope), MilestoneStats (lower median, p25, p75; missing = max_runs+1). EB adds rows lux-easy/-normal/-hard (stop_after Liberation(1), max_runs 20) and start-aut/start-bgd; CD adds "takeover" with PacingDefender::RecruitWhenReady. Goal-5 and goal-6 acceptance use expert-deu, Normal, 20 seeds, 60 runs. One crates/horde_core/tests/pacing_targets.rs, all #[ignore = "release-only"], OnceLock sweep cache; run command in AGENTS.md. Default pacing 60 --runs reproduces the post-W1-69 baseline target/production-after/w1-69/pacing.txt byte-for-byte (UTF-8, no BOM, LF; W1-69 moved pacing on purpose, so the wave-0 target/production-before/pacing.txt is not the pin, plan §9.6).
- **Supersedes in this spec:** §9 harness (PacingProfile.preset, standard()/lux(), PacingSummary, own example flags) and the w2 harness step.

#### W-4. LUX Easy/Hard runs-to-liberate and start-country checks

- **Owner / wave:** enemy-balance, wave 2 (evaluators), 4 (gate).
- **Ruling:** runs_to_liberate = median Liberation(1) over 20 seeds of lux-*; asserts median(lux-easy) &lt;= 6 and median(lux-hard) &lt;= 1.5 x median(lux-normal) (missing = 21). start-aut / start-bgd: median FirstVictory in 2..=4 asserted; per-seed "no run-1 victory" only reported.
- **Supersedes in this spec:** §9 pacing_targets bullets on LUX/AUT/BGD (unnamed profiles, per-seed AUT rule).

#### W-5. Front ramp value

- **Owner / wave:** enemy-balance, wave 2.
- **Ruling:** front_ramp = 1 + front_ramp_max x extras.front_ramp x depth, folded into enemy_health and arrival inside progression_for(mode, rank, lanes, Option&lt;FrontPosition&gt;). progression.json front_ramp_max (serde default 0.0, bounds 0..=0.5) bundled 0.18 (not 0.15); Normal final/first = 1.3924, Hard 1.6129, Easy 1.2882 at equal rank. Guard floor 1.35 for Normal; wave-4 tuning only within [0.17, 0.25]. Without a country front_ramp is exactly 1.0 and skipped.
- **Supersedes in this spec:** §3 front_ramp_max .15 (1.3225) and the unstated Normal extras value.

#### W-6. Boss damage multiplier

- **Owner / wave:** enemy-balance, wave 2 (rank term), 4 (Capstone).
- **Ruling:** boss_damage_multiplier = clamp((1 + boss_damage_per_rank x max(0, min(r,50)-4)) x rank_damage, 1, 4); boss_damage_per_rank bundled 0.01 (0..=0.05); boss_ranks.*.damage Sector 1.0, Regional 1.0, Capstone 1.10. Read sites: Personal combat.rs:102-108 boss branch (before .round()); NPC combat.rs:166-176 adds floor((m-1) x 6) to adds. Non-boss enemies never read it.
- **Supersedes in this spec:** freeze note "enemy-balance may leave it at 1.0".

#### W-7. NPC (3d6) resolution for new archetypes

- **Owner / wave:** enemy-balance, wave 2.
- **Ruling:** rules.rs resolve_blast(rng, damage, defense: Option&lt;i32&gt;, dr, injury_scale, &mut InjuryState, ht) and resolve_object_blast(rng, damage, dr): no attack roll; one defense roll when Some; damage, penetration, injury, scale (1.0 identity), apply_injury. Exploder fuse at a normal NPC turn (defender: dodge + defense_bonus, dr = npc armor, scale = hazard multiplier; structures: object blast and flat structure_damage), removed without reward. Death burst hits other enemies in radius in ascending id, never the defender, chains next tick. Split children spawn with their own NPC stats and zero draws. Sprinter groups: group_size records, zero extra draws. Juggernaut stagger_immune clears prone/stunned/weapon_dropped after every injury, same draw count. Each lands with its Personal behaviour.
- **Supersedes in this spec:** §4 mechanics where only spitter and shield had NPC paths.

#### W-8. Storm tagging, reduced storm grade and wall-clock lightning

- **Owner / wave:** vfx-lighting, wave 5.
- **Ruling:** art::lighting::STORM_THEME_IDS = ["depot"]; is_storm(theme) = weather == "storm" (case-insensitive) or id in the table; no StageThemeDefinition.storm, no world.json or theme-order change. Reduced effects on any storm stage: sky_flash 0 and the static Lighting::Storm grade. sky_flash takes wall_ms from an unscaled real-time accumulator (paused frames excluded): SKY_SLOT_MS 8000, SKY_OFFSET_RANGE_MS 4000, SKY_RISE_MS 33, SKY_DECAY_MS 233, SKY_QUIET_BOSS_MS 1000, SKY_QUIET_CUE_MS 750; pulse starts &gt;= 4000 ms apart at 1x/2x/4x; schedule hash splitmix(variation ^ slot ^ SKY_TAG), never core RNG.
- **Supersedes in this spec:** §8 "vfx's storm theme is appended there".

#### W-9. Regional checkpoints and Capstone

- **Owner / wave:** enemy-balance, wave 2, 4.
- **Ruling:** One derivation: FrontPosition{stage, count}::boss_rank() (Capstone if stage+1 == count, else Regional if (stage+1) % 6 == 0, else Sector); Country::regional_checkpoints() from it (&lt;= 3 for 24 stages). Nothing saved per stage; only attempt-scoped snapshot.boss_rank and Report/RunRecord.boss_rank. No theme change, no checkpoint coin grant. Wave 2: FrontPosition, stamping, Capstone +1 transition. Wave 4: Regional HP x1.2, reward x1.2; Capstone HP x1.4, armor x1.25, NPC armor +2, reward x1.4, damage x1.10. CD owns presentation lines in wave 4 reading Game::stage_boss_rank.
- **Supersedes in this spec:** §3 vfx recommendation and "CD owns the checkpoint coin grant".

#### W-10. generate_world.py reproducibility, difficulty bands, start clamp

- **Owner / wave:** enemy-balance, wave 2.
- **Ruling:** Commit A: indent-2 writer (byte-identical today), default --from-existing (offline, carries every key, recomputes only difficulty), --check (exit 1 on byte diff; exits 0 with the old formula), --rebuild with sha256 pins (exit 3) and stage-count guard (exit 4), --self-test; no scripts/world_stage_themes.json. Commit B: difficulty = (lo + fnv1a64("difficulty:"+id) % (hi-lo+1)) / 100 with bands &lt;= 3 stages 85-115, 4-9 100-135, &gt;= 10 120-160 (LUX .92, DEU 1.04, AUT .85, BGD 1.60). Start clamp Country::effective_difficulty -&gt; [0.95, 1.05] for the start country (no saved field), used for deploy terms and exposed via Game::effective_country_difficulty.
- **Supersedes in this spec:** §8 scripts/world_stage_themes.json and theme append; hard-coded strategy_config; unpinned download.

#### W-11. 400-enemy stress seam

- **Owner / wave:** perf-architecture, wave 1.
- **Ruling:** GAMEPLAY_ENEMY_CAP: usize = 224 replaces the literal at progression.rs:238; production validation does not widen. horde_core feature review = []; horde_game review = ["horde_core/review"]. Behind cfg(any(test, feature = "test-support", feature = "review")): STRESS_ENEMY_CAP = 399, sim/fixtures.rs stress_render_fixture() (17 lanes, 30 yd, night stage, MG Npc, structures, 3 fire zones, 1,500 corpses, 1,200 stash items, 150 run records, 3 workers) validated at the authored cap, then Attempt::stage_review_enemy_cap(399): unsaveable by construction. Review screen "stress" rejects LASTLIGHT_PRACTICE and never autosaves. Wave 1 uses tier 4; perf updates it in W2-47.
- **Supersedes in this spec:** §11 "save rejected by existing validation" trivial pin.

#### W-13. Visual twins for EnemyWindup and BossPhase

- **Owner / wave:** vfx-lighting, wave 2.
- **Ruling:** The existing windup ellipse and bar already draws for every Windup enemy, defender-targeted included (critic claim incorrect). Added: EnemyView.aim_target dashed aim line for ranged windups (W2-28); BattleView.boss_phase: Option&lt;BossPhaseView{transition, name, hold_left_ticks}&gt; with label "PHASE n - NAME" and a ring for the whole 90-tick hold, static in reduced mode (W2-37). They precede the audio cues; the UX boss banner is not the twin.
- **Supersedes in this spec:** §6 "pairs with ux's &gt;= 1.5 s banner (the visual twin)".

#### W-14. Wave-tier bound (superseded by P1)

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** This cluster set MAX_WAVE_TIERS = 64 with wave_tier &lt; 64. The master planner (P1) withdrew it: the bound is MAX_WAVE_TIER = 15 inclusive in W1-28.
- **Supersedes in this spec:** Same.

#### W-15. Enemy and pacing documentation ownership

- **Owner / wave:** enemy-balance, wave 4.
- **Ruling:** enemy-balance writes ENEMY_COMPOSITION.md, PROGRESSION_SCALING.md, the goal-6 section of BALANCE_AND_RUN_STATS.md and the WORLD.md band paragraph at the wave-4 close (W4-10); progression-tree writes the goal-5 section and the AGENTS.md pacing_targets section (W2-03, W4-11); qa's docs inventory checks presence only.
- **Supersedes in this spec:** No w7 docs step.

### Wave-1 close rulings (2026-11-07)

Orchestrator rulings R12-R23, recorded verbatim in plan §2.4. The ones that touch this remit:

#### R15. No ruleset bump for W1-69

- **Owner / wave:** save-determinism, wave 2 (W2-49).
- **Ruling:** RULESET_VERSION is not bumped for W1-69 now; the planned W2-49 bump (2 -> 3) must name both W1-69 (no lane queue) and its own change. Document that pre-W1-69 recordings replay without a ruleset warning until then.
- **Supersedes in this spec:** P4 for wave 1 only: RULESET stays 2 through W1-69, and W2-49's 2 -> 3 bump names both changes. VALIDATION.md "Strict sale and ruleset 2" records the replay behaviour.

#### R21. Custom chip keeps the preset it was clicked from

- **Owner / wave:** enemy-balance (E3 semantics) and ux-flow (the settings screen), wave-1 close.
- **Ruling:** The Custom difficulty chip keeps the basis of the preset it was clicked from (clicking Hard then Custom without edits sends Custom with the Hard basis). Fix in code with TDD.
- **Supersedes in this spec:** Sending an unedited named draft as that named preset after the Custom chip. The core stays one `SetBalance` per step: the screen sends the named preset, then the same values as Custom, so the named preset is the previous preset (as the new-journey reset already does).

### Rulings owned by this remit

P1, P6, E3, E9, W-4, W-5, W-6, W-7, W-9, W-10, W-15, R21

### Writer notes (wave 2)

- **W2-26, boss phase bonus.** Section 6's transition count and schema-v7 §3.10 give Hard one boss phase at r ≤ 4 (and two on a Capstone). W2-26's pin `neutral_snapshot_serializes_no_new_keys_for_ranks_0_to_4_on_every_preset_at_depth_0` forbids a `boss_phases` key at ranks 0-4 on every preset. W2-26 followed the pin: `ProgressionSnapshot::fold_extras` adds `extras.boss_phase_bonus` only past rank 4, capped at 2. W2-36 adds only the `(r > 4)` and `rank.extra_transitions` terms, because the bonus is already added and adding it again would count it twice. Whether Hard at r ≤ 4 gets its phase is open in plan §2.5. Until it is ruled, the section 6 table's "Hard, 1 at r ≤ 4" row does not hold.
- **W2-29, exploder and splitter.** Section 4 and ruling W-7 say enemies killed by a burst are reaped next tick. `reap` runs several times per tick, and save validation rejected any defeated hostile, which a save between ticks would then hold. W2-29 kept the next-tick rule. A never-saved `Attempt.burst_victims` hold keeps a burst's victims out of the reaps later in the same tick. A reap at the start of every tick removes them before anything acts; without a victim it finds nothing and draws nothing. Save validation accepts a defeated hostile only within the radius of a death burst published on the save's tick: an untargeted `Detonation` with `from == to` whose actor left a corpse on that tick (a defender fuse publishes `to` = the defender and leaves no corpse). When the 512-event window is full and its oldest event is from the save's tick, the Detonation may have been evicted, and the bound falls back to any corpse of that tick within 3 yards, the schema's largest radius (W2-29 review). So a save between the burst and the reap is valid, and a resumed attempt reaps the victims exactly as an uninterrupted one. Melee and its sweep skip a defeated hostile (shots, ordnance, turrets and the NPC already did). The other readings W2-29 took:
  - A fuse's own target takes the blast. The flat `structure_damage` hits the other structures in the radius.
  - The NPC fuse on the defender uses scale 1.0, because `hazard_scaled` is Personal-only (W2-24 review). The Personal fuse runs through `hazard_scaled`.
  - A death burst hits hostiles with its authored damage (Personal: `damage - armor`; NPC: `npc_dice` and `npc_add`), without threat scaling. Its Personal hits publish no per-target events.
  - Fuse damage to the defender counts in `hazard_damage_taken`.
  - Split children publish no `Spawn`; the parent's `Split` carries the count spawned. A target that is missing, or could not be a child (it has a death effect, a group, an unlock or an elite mark of its own), falls back to the Runner default.
  - The ruling E8 drop rule for split children lands with them: a child takes and discards the 3% roll and never drops.
- **W2-30, shield, juggernaut, sprinter and the hazard seam.** The readings W2-30 took:
  - `enemy_armor_vs(a, e, HitSource)` serves every hit on a hostile. `HitSource::Shot` is the defender's gunfire (Personal shots and NPC shooting), `Turret` a turret, `Melee` the strike and its sweep (and an NPC melee strike), `Area` grenade impacts and death bursts, `Trap` a trap and `Fire` a Molotov's fire pulse. Ranged armor applies to Shot and Turret only. "Not in contact" is `Enemy::contact()`, the defense line, the same test that halves armor by `contact_armor_fraction`; a shield-bearer at a wall face still carries its shield. Bosses carry no ranged armor. Penetration comes off the sum, `max(armor + ranged_armor - penetration, 0)`, as `tree_value` already models it. `Attempt::enemy_protection` (the inspector) stays the base armor.
  - Stagger immunity lives in `Enemy::apply_damage`, which every hit on a hostile reaches right after its injury roll. It skips the hit-stop refresh in both modes and, in NPC mode, clears `stunned`, `prone` and `weapon_dropped`. Unconsciousness and shock are kept. The combat log line still names the knockdown the roll produced.
  - `Game::spawn_pick` spawns a pick's `group_size` members at the lane's tail, so each stands 0.65 yd behind the last. The first member is the pick the director made room for. Every later member checks the cap, and one past it counts as a skipped spawn. `spawn_archetype` goes through it. Caller summons, bosses and split children stay single.
  - A spit on the defender is hazard damage: the Personal spit runs through `hazard_scaled` before armor and Brace. The NPC spit keeps scale 1.0, like the NPC fuse. Spit damage to the defender counts in `hazard_damage_taken` in both modes; a spit on a wall does not.
- **W2-31, composition.** The 7 records and waves 5-11 are appended exactly as the section 4 tables give them (each wave copies the previous one and changes the listed weights; "hungry" is `hungry-shambler`). The Personal mean HP per pick matches the "Waves 5-11" table to one decimal. The readings W2-31 took:
  - The exploder's attack damage is 1, not the table's 0. The attack bound (`1..=10000`, in both `AuthoredCatalog::resolve` and `ResolvedEnemy::validate`) is unchanged, and the simulation never reads the value, because the fuse replaces the attack with its burst. (W2 review: the analysis-only `campaign/tree_value.rs` does read it, for the exploder's structure hit; its survivor hit comes from the burst.)
  - The new authored keys sit where section 4's field table puts them: `reach` on attacks, `ranged_armor` and `npc_ranged_armor` on defenses, `stagger_immune` on bodies, and `death_effect`, `group_size`, `tags` and `elite_eligible` on the archetype. `elite_eligible` stays in the catalogue (`EnemyCatalog::elite_eligible`) for W2-33.
  - The split target is checked at load as the section 4 `Split` bullet says. Waves 5-11 are not reachable yet: the floors stop at tier 4 until W2-32, so no digest and no pacing output moves.
  - `every_floor_tier_exists_in_enemy_catalog` is `ProgressionCatalog::check_wave_tiers`, run by the strict `init_catalogs` after both catalogues are installed. The lazy accessors keep their wrap.
  - The monotone check covers Personal HP over tiers 0-11. NPC hit points already dip from tier 3 to the frozen tier 4 (10.46 to 10.35 with the variants locked), so the NPC check starts at tier 4.
- **W2-32, growth knee, floors and boss armor.** The keys and rows are as section 2 gives them. The readings W2-32 took:
  - The knee applies to the unbounded threat rank, as the single slope did: `enemy_health = 1 + health_per_rank × min(r, k) + health_per_rank_late × (r - k)` past the knee, and the same for damage. `threat_growth` still scales the whole growth term on the campaign snapshot. Without the knee keys (`growth_knee_rank` absent = `u32::MAX`, late slopes 0) a legacy `LASTLIGHT_PROGRESSION` override keeps the single slope bit for bit. The late slopes share the early slopes' bounds (0..=0.25).
  - `growth_matches_gear_within_band` reads gear as the shop item level's damage multiplier, `1 + damage_per_level × (item_level - 1)`, and holds gear / enemy health in [1.05, 1.15] for ranks 4..=58 (the section 2 table).
  - Boss armor lands here, not in W2-36, because `rank_20_boss_armor_exceeds_rank_0_both_modes` is a W2-32 fail-first test. New catalogue keys `boss_armor_per_rank` (0..=0.05, bundled 0.02) and `npc_boss_armor_rank_step` (0..=100, 0 = none, bundled 8), both serde default 0. The snapshot writes `boss_armor_multiplier = clamp(1 + 0.02 × (min(r, 50) - 4), 1, 4)` and `npc_boss_armor_add = min((r - 4) / 8, 20)`, the literal 1.0 and 0 through rank 4. `place_enemy` applies them to the boss's spawn armor: Personal `round(boss_armor × multiplier)` (exactly the level's armor at 1.0), NPC `boss_npc_armor + add`. The armor then lives on the saved `Enemy.armor`, so exposure still zeroes it and a resumed attempt keeps it.
  - Ranks 0-4 serialize byte for byte as before (`ranks_zero_to_four_snapshot_pinned`, on every preset, both modes, with and without a country). Past rank 4 the snapshot writes `boss_armor_multiplier`, and from rank 12 `npc_boss_armor_add`.
- **W2-33, elites.** The curve, roll, affix table and bake are as section 5 and rulings E3 and E7 give them. The readings W2-33 took:
  - `progression.json` gains the block `elite { min_rank 3, start_bp 200, end_rank 23, end_bp 1500 }` (`EliteCurve`, serde default the zero curve: no elites). Validation accepts the zero curve or a linear one: `min_rank` at least 3, a later `end_rank`, `1 <= start_bp <= end_bp <= 10000` and a whole number of basis points per rank. `progression::elite_bp(cfg, r)` feeds `snapshot.elite_rate_bp`, and the existing `fold_extras` scales it by `elite_rate_pct` in integer math (Easy floor of half, Hard floor of 1.5x).
  - `enemy_catalog.json` gains `elite_base { health 1.5, reward 4, xp 3, scale 1.15 }` and the five `elite_affixes` of section 5, each with a display `name`. The base health multiplies NPC hit points as well as Personal health (the base row has no mode); an affix's Personal health multiplier does not touch NPC hit points, which take the NPC column's addition. Reward and xp are integer factors.
  - Only director wave picks roll (`Arrival::Wave`); `Game::spawn_archetype`, `spawn_enemy`, bosses, Caller summons and split children never do. Each member of a sprinter group rolls on its own id. The roll is `keyed(seed, a.id, e.id, ELITE) % 10000 < elite_rate_bp`, the affix `keyed(.., AFFIX)` modulo the weight offered to the archetype in table order. `ResolvedEnemy.name` keeps the record's name; `EliteMark.name` is "<Affix> <archetype name>", or the affix alone past 60 bytes.
  - Telemetry: the spawn counts `elites_spawned` and the reap counts `elites_killed`, only while it is below `elites_spawned` (the archetype-map rule), so `RunRecord.elite_kills` is populated.
  - The shared health guard is `sim::encounter_health_bound`, used by deploy and save validation. It counts the largest baked elite (precomputed at catalogue load) only when the snapshot's `elite_rate_bp` is above 0, and the boss with `boss_health_multiplier`, so every encounter without elites gets exactly the old bound.
  - Ranks 3 and 4 now write `elite_rate_bp`, so the W2-26 and W2-32 rank 0-4 pins were amended on purpose: `neutral_snapshot_serializes_no_new_keys_for_ranks_0_to_4_on_every_preset_at_depth_0` and enemy_schema's legacy-key test expect the key from rank 3, and `ranks_zero_to_four_snapshot_pinned` pins the rates on their own and hashes the lines without that key to the unchanged W2-32 value.
- **W2-36, bosses.** The phase table, transitions, hold and touch points are as section 6 gives them. The readings W2-36 took:
  - The transition count is three terms in three places: `ProgressionCatalog::snapshot` writes the rank term `(r > 4)`, the existing `fold_extras` adds Hard's `boss_phase_bonus` past rank 4, and `Campaign::progression_for` adds `BossRank::extra_transitions()` (Capstone 1) after stamping the boss rank, all capped at `MAX_BOSS_PHASES` (2). Hard at ranks 0-4 follows the plan §2.5 default (the W2-26 pin reading): Normal's count, 0 on a Sector or Regional stage and 1 on a Capstone (`hard_ranks_zero_to_four_have_normals_phase_count`); schema-v7 §3.10 and the section 6 table are amended to match. A one-stage country's only stage is its Capstone, so from W2-36 it writes `boss_phases` 1 at ranks 0-4; the W2-26 and W2-32 rank 0-4 pins pin that key on its own.
  - `enemy_catalog.json` `boss_phases` is a list of `{ boss, name, speed_mult, armor_mult, call_interval_mult, siege_recovery_mult, hold_ticks }` records, every field written (the saved `BossPhaseProfile` has no defaults). Absent means no phases; a partial table, a duplicate kind, an unknown key or a profile out of bounds is rejected at load.
  - `place_enemy` snapshots the profile into `boss_state` only when the snapshot's `boss_phases` is above 0, so every boss of ranks 0-4 off a Capstone, every legacy boss and every boss of a catalogue without the table keeps None and behaves as before.
  - A transition is entered at the start of every `reap` (every damage step reaps), so it follows the hit that crossed the threshold; it compares `hp × 100 <= max_hp × threshold` on `Enemy.hp` in both modes (the NPC copy of the injury hit points). "At most one per tick": a boss whose `hold_until` equals this tick plus `hold_ticks` entered this tick. A boss dropped below 25% in one blow enters phase 1 on that tick and phase 2 on the next, restarting the hold.
  - The hold covers ticks `entry .. entry + hold_ticks` (`BossState::held(tick) = tick < hold_until`): movement treats the boss as slowed to 0, and `enemy_attacks` neither counts down nor strikes, while the NPC turn bookkeeping (shock, consciousness, stun recovery) still runs.
  - The multipliers read `mult^entered`, exactly 1.0 before the first transition. Speed scales `BOSS_BASE_SPEED` (0.6, the movement.rs literal; sim.rs:1201 no longer had one). Armor scales `enemy_armor_vs` for a boss on every hit source, `round(armor × m)` after exposure (so an exposed boss stays at 0). The call interval and siege recovery scale their integer ticks, `round(ticks × m)` (540 and 144 ticks for 0.6), not the seconds, so f32 rounding never adds a tick.
  - `BossPhase` uses the event's `value` for the transition number and `duration_ticks` for the hold, `actor_id` the boss; the combat log notes "<boss> enters phase n: <name>". The visual twin is W2-37.
