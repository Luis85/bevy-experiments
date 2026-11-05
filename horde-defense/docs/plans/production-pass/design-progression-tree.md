# Progression tree: synthesized spec (goal 5, decisions 14 and 19)

## Synthesis
**Base:** design B (3 of 3 judges). B pays the level grant and milestones once, by the root game at settlement, against saved ledgers. It has an explicit v6->v7 step, a typed CommandError, an honest tier metric and wider counter coverage.

**Grafted from A (per-decision majorities):**
- Trap +1 charge per rank, strictly monotone (`charges_per_rank`), replacing B's div_ceil step 2.
- Pacing milestones keep the "runs completed" semantics (2 of 3 judges). The doc comment is fixed and a test pins it.
- Recommender: only the N2 effect retune plus a ClassModifier arm (2 of 3). B's breakpoint-aware N1 is rejected.
- The continuous value model with overkill clipping, A's dominance axis groups, and the exemption that keeps ladder ranks of 2 or more out of pairwise dominance.
- `HeavyTargetDamageMultiplier` "Big-game rounds" as the drop-in 8th node if enemy-balance's hazard paths slip.
- A report-only `TierOwned(d)` pacing row.
- Telemetry validation does not assert kills <= spawned per archetype.
- The fixed wave-4 lever order, with tests/progression_tree.rs rerun after each lever.
- An append-only FNV pin, the threaded == sequential sweep test, and save_v6 coins unchanged after load plus 600 idle ticks.

**Violations fixed:**
- Grant double-pay: verified in code. `lifecycle.rs:294` calls `first_clear` in worker clones, and `world.rs:41` awards XP whatever the mode. `parallel.rs:205-211` syncs the clone's coin delta, then `parallel.rs:236/270` calls `first_clear` again. The fix is root-only settlement against `Player.level_grant_paid`.
- Untyped training errors become `CommandError::TrainingUnsupported`.
- Price-pinned tests are rewritten inside the repricing commit, so every commit stays green.
- The frozen v6 rank gets a parity test against runtime `progression_rank()` on every legacy fixture.
- The milestone item uses a fixed authored quality, so there is no loot-economy roll and no RNG.
- The hazard field has a guaranteed consumer, because decision 12 binds exploder and spitter. If not, there is a documented fallback and removal rule.
- Trap rank prices go from 700 to 300.

## 1. Effect retunes (wave 4; ids, prerequisites and positions unchanged)
| Node | Today | New effect | New price | Reason |
|---|---|---|---|---|
| steady_hands | DamageAdd 8 @40 | DamageAdd 4 | 120 | Pistol 12->16 keeps the rank-0 two-shot on 32-HP Shamblers. The pump gains +40% per trigger, not +80%. Today it is 7.6x branch value per coin. |
| hardened | ArmorAdd 3 @100 | ArmorAdd 2 | 450 | Work jacket 1 + 2 = 3 < Runner damage 4, so contact still hurts. Today it gives x5.7 EHP. |
| trap_rank_1..10 | StructureRank, charges fixed 3 (sim.rs:979) | + content `StructureDefinition.charges_per_rank = 1` on shrapnel_trap; rank r places 3+r charges | 300+75(r-1) | Every rank changes kills. Value per coin is strictly decreasing. |
| quick_throw | OrdnanceCooldownMultiplier 0.7 @165 (value 0) | [GrenadeCharges 1, MolotovCharges 1, OrdnanceCooldownMultiplier 0.85] | 900 | Gives it a nonzero value; it stays below wide_blast (1,200). |
| combat_reload | ReloadMultiplier 0.85 @180 | 0.75 | 1,200 | N2. Not dominated by Reload drill. |
Descriptions are updated, including all 10 trap-rank texts ("+1 charge").

## 2. Value model (new `crates/horde_core/src/campaign/tree_value.rs`, analysis only; no rule reads it)
- `pub enum ValueAxis { Firepower, Survival, Melee, Ordnance, Fortification }`
- `pub enum DominanceGroup`. Groups follow A's grouping:
  - Health = HealthAdd + BodyDevelopment(Vitality)
  - Reload = ReloadMultiplier + Coordination
  - Melee = MeleeAdd + Strength
  - Stamina = StaminaAdd + Endurance
  - Damage = DamageAdd + DamageMultiplier
  - Weak = Perception
  - one class-scoped group per WeaponClass
  - Ordnance, Fortification and Armor
- `pub struct NodeValue { axis: ValueAxis, group: DominanceGroup, ln_gain: [f32; 2] }`.
- `pub fn reference_ranks(node) -> [u32; 2] = [g, g + 4]`, where `g = node.minimum_victories.unwrap_or(0)`.
- `pub fn node_value_at(node: &SkillNode, rank: u32) -> Option<f32>` evaluates the node on a synthetic campaign that owns the node's prerequisite ancestors (and lower ladder ranks).
- `pub fn node_value(node) -> Option<NodeValue>`.
- `pub fn branch_value_per_coin(branch, rank) -> f32` = Σ ln_gain / Σ price over the valued non-ladder nodes of that branch with gate <= rank.
- The enemy pool comes from the director's own `wave_pool(progression().snapshot(rank).wave_tier)` (director.rs:47-50), weighted by spawn weight. It therefore follows enemy-balance's tiers automatically.
- Constants:
  - `REFERENCE_WEAK_HIT_SHARE = 0.25`
  - `ZERO_GAIN = 0.001`
  - `CHEAP_NODE_PRICE = 100`
  - `CHEAP_NODE_LIMIT = 2.0`
  - `DEPTH_PEER_LIMIT = 2.5`
  - `BENCHMARK_GUNS = [compact_service_pistol, compact_smg, pump_shotgun, patrol_rifle_tier1, scout_marksman_rifle, belt_fed_machine_gun]`
- **Firepower** = ln of the pool-weighted continuous kill rate (A's form, not ceil breakpoints).
  - Per pellet: `min(max(dmg_eff - armor_eff, 0), target_hp)`, with overkill clipped. `armor_eff = armor + ranged_armor - penetration`, clamped at 0.
  - Weak share 0.25. The hit cap is weapons.rs:56.
  - time = hp / dps_eff + reload overhead (shots / magazine × reload).
  - Generic firearm nodes average over BENCHMARK_GUNS. ClassModifier nodes use their class gun.
- **Survival** = ln(health / max(mean hit after armor and hazard multiplier, 0.25)).
- **Melee** = ln(kill fraction per strike × strikes per stamina pool).
- **Ordnance** = ln Σ charges × damage (clipped) × radius² (+burn).
- **Fortification** = ln of structure charges × kills per charge, plus the engineering multiplier.
- **Capability nodes return None, listed explicitly:** Unlock, FeatureUnlock, Blueprint, WeaponTraining, or any node carrying one.

## 3. Static tree rules (new `crates/horde_core/tests/progression_tree.rs`)
1. **`no_valued_node_is_dominated_at_its_reference_states`.** A valued node A is dominated if:
   - (i) ln_gain <= ZERO_GAIN at both refs; or
   - (ii) some B meets all of the following:
     - B has the same DominanceGroup;
     - neither is a prerequisite-ancestor of the other;
     - price_B <= price_A;
     - B.minimum_victories <= A's, and B.minimum_liberated <= A's;
     - B's gain >= A's at both of A's refs, strictly at one.

   Ladder ranks of 2 or more are neither dominated nor dominators under (ii). Rank 1 takes part.
2. **`cheap_nodes_stay_within_twice_branch_value_per_coin`.** Every valued node with price < 100 has `ln_gain / price <= 2.0 × branch_value_per_coin` at each of its refs. A ladder rank 1 under 100 is checked against its dedicated peer's branch: Vitality, Strength and Endurance against Survival; Coordination and Perception against Firearms; structure ranks against Fortification.
3. **`no_node_outvalues_its_depth_peers_by_more_than_two_and_a_half_x`.** At any price, value per coin <= 2.5 × the median of valued nodes in the same branch at depth ±1. This fails today on Hardened.
4. **`ladder_value_per_coin_never_rises`.** Body and structure ladders.
5. **`body_rank_one_is_cheaper_than_its_dedicated_peer`.** body_vitality_1 < field_fitness, body_coordination_1 < quick_reload, body_strength_1 < heavy_strike, body_endurance_1 < endurance_drill.
6. **`tier_costs_sit_in_design_bands`.** T1 in [1,700, 2,300], T2 in [7,800, 9,800], T3 in [18,500, 22,500]. Today 930/2,790/4,975.
7. **Mechanic tests:**
   - hardened_leaves_runner_contact_damaging
   - quick_throw_adds_throws_per_deployment
   - trap_mastery_1_adds_a_trap_charge
   - steady_hands_keeps_rank_zero_shambler_two_shot_and_adds_at_most_40_percent_per_shotgun_trigger
   - combat_reload_cuts_reload_by_a_quarter
8. **Structural tests:**
   - skill_catalog_is_append_only_with_eight_weapon_mastery_nodes: FNV of the first 169 ids in order, pinned to the wave-0 value; len == 177.
   - every_milestone_rank_opens_a_new_non_ladder_node
   - counters_name_catalog_archetypes
   - counter_nodes_open_no_later_than_their_archetypes: gate <= the first rank at which any listed archetype has spawn weight.
   - milestone-gated nodes do not collide in position: validator plus capture.

## 4. Repricing (static skills.json, wave 4; first pass computed against the shipped catalogue)
- **d1:** quick_reload 120, steady_hands 120, smg_training 100, piercing 200, field_fitness 120, second_wind 130, brace 140, sweep 150, engineering 120, grenade_training 120, molotov_training 130.
- **d2:** shotgun_training 300, rifle_training 350, rapid_cycle 450, stopping_power 500, endurance_drill 300, hardened 450, heavy_strike 350, reinforced_frames 450, blast_packing 350, grenade_bandolier 400, bottle_carrier 400, slow_burn 350, trap_blueprint 150, wall_blueprint 200, turret_blueprint 250, companion_contact 400.
- **d3:** marksman_training 1,300, machine_gun_training 1,400, combat_reload 1,200, veteran_vitality 1,300, defensive_fabrication 1,300, wide_blast 1,200, quick_throw 900, recruitment_network 1,150.
- **d4+:** outpost_mastery 1,500, parallel_command 1,100, country_operations 1,500.
- **Ladders:** body rank r = 100 + 30(r-1), so 100..670. Structure rank r = 300 + 75(r-1), so 300..975. That is B's ladder with A's lower structure start, per judge 3's "well below 700".
- **Tier definition change** in `sim/pacing.rs::tier_cost`: exclude nodes with `minimum_victories >= 5`. These are milestone-gated and cannot be bought inside the window. No existing node at depth <= 3 is affected (gate 5 is only on body ranks 18-20). This must be stated in BALANCE_AND_RUN_STATS.md, and the report-only `TierOwned(d)` row is printed beside it.
- **Result** (pt_prices2.py, scratchpad):
  - T1 1,950 / T2 8,250 / T3 19,700, against wave-0 930 / 2,790 / 4,975.
  - Base tree 78,575 + 8 new nodes 9,000 = 87,575 (today 52,180).
  - Expected: affordable at about runs 4 / 8 / 11-12, with wave-0 expert earnings plus grants and milestones. The tree runs out around runs 32-36, well past Liberation(2) at about run 17.
- **Prediction caveat:** today's income is assumed. Goal-6 changes income, so prices are fitted with `pacing --fit-tiers 4,8,12` after enemy-balance's wave-4 changes.
- **Wave-4 lever order** (A's, adapted). After each lever, rerun progression_tree and the 20-seed default sweep.
  1. Expert first-victory median > 4:
     - steady_hands -10 down to 100;
     - quick_reload/field_fitness -10 down to floors of 60/65. When a peer drops below 110, the matching body rank 1 goes to peer price -10 and then must pass rule 2;
     - grenade 50, molotov 60, smg 60;
     - move the removed T1 mass to the capability nodes piercing, brace, sweep and second_wind, which have no value-rule exposure.
  2. T1 outside [3,5]: piercing, brace and sweep ±20.
  3. T2 outside [7,9]: body step ±10, then blueprints ±50, then companion_contact ±100.
  4. T3 outside [11,13]: structure rank start ±50, then d3 nodes ±100.
  5. TreeExhausted <= Liberation(2): body step +10.

## 5. Decision-19 nodes (append-only ids 170-177, existing icons; prices provisional, then fitted by rules 1 and 3)
| id | effects | prereq | position | gate | price | counters |
|---|---|---|---|---|---|---|
| shotgun_pump_drill | ClassModifier Shotgun Cadence 0.8 + Reload 0.8 | shotgun_training | [-1780,190] | V5 | 800 | sprinter-swarm |
| marksman_steady_aim | Marksman Damage 1.15 + WeakBonus 0.15 | marksman_training | [-940,760] | V5 | 800 | shield-bearer, juggernaut |
| shield_breaker | ArmorPenetration(4), personal shots only | stopping_power | [-1500,760] | V10* | 1,000 | shield-bearer, juggernaut |
| blast_discipline | HazardDamageMultiplier(0.6) | hardened | [260,380] | V10* | 1,000 | exploder, spitter |
| pistol_double_tap | Pistol Cadence 0.8 | quick_reload | [-1780,0] | V15 | 1,200 | - |
| smg_spray_control | Smg Reload 0.7 | smg_training | [-660,0] | V15 | 1,200 | splitter |
| rifle_breath_control | Rifle Damage 1.15 | rifle_training | [-1780,380] | V20 | 1,500 | - |
| mg_belt_feeder | MachineGun Reload 0.6 (5.1 s->3.06 s) | machine_gun_training | [-660,380] | V20 | 1,500 | sprinter-swarm, splitter |

\* A counter's gate is the largest of 5/10/15/20 at or below the first rank at which any listed archetype has spawn weight in enemy-balance's tiers. It may move to V5, and the test enforces this.

The weak classes (shotgun x1.20 and marksman x1.39 at R0) get their perks first.

**Fallback (A's graft):** if enemy-balance's burst/reach paths have not landed by the time step W2-H starts, replace blast_discipline with `big_game_rounds`:
- `HeavyTargetDamageMultiplier(1.25)` when Enemy.max_hp >= `HEAVY_TARGET_HEALTH = 150`;
- prereq piercing, [-660,950], V20, 1,500.

Its `SkillModifiers.heavy_target_damage_multiplier` field is then added in that commit. This is format-neutral because SkillModifiers has a container `#[serde(default)]` and the field is skip-if-default. The `HazardDamageMultiplier` variant and field are removed in the same commit. That is safe because nothing could have serialized a non-default value without the node.

**New SkillEffect variants** (skill_catalog.rs:64-88):
- `ClassModifier { class: WeaponClass, stat: ClassStat, value: f32 }` with `enum ClassStat { Damage, Reload, Cadence, WeakBonus }`. Effect bounds: Damage 1.0..=2.0, Reload/Cadence 0.5..=1.0, WeakBonus 0.01..=1.0.
- `ArmorPenetration(i32)` 1..=20.
- `HazardDamageMultiplier(f32)` 0.5..=1.0.

Each also needs: EffectCategory (Firearms; Survival for hazard), keywords (class name, "perk", "armor", "shield", "blast", "spit"), `PreviewStat::{ClassStat(WeaponClass, ClassStat), ArmorPenetration, HazardDamage}` and the aggregate bounds at skill_catalog.rs:367-399, 641-663.

**Folding:**
- In `profile_for_loadout` (loadout.rs:202-218, Personal only), after the generic modifiers: `if let Some(cm) = modifiers.class_modifiers.get(&weapon.class)` then
  - damage ×= damage_multiplier (round, clamp 1..100000);
  - reload ×= reload_multiplier (.max(0.65));
  - cadence ×= cadence_multiplier (.max(0.08));
  - weak_bonus += weak_bonus_add (.min(1.0)).

  Profiles without perks are bit-identical.
- Penetration at manual.rs:174: `let damage = (raw - (enemy_armor(a, e) + ranged_armor(e) - a.profile.modifiers.armor_penetration).max(0)).max(0);`. The `ranged_armor` term arrives with enemy-balance's field (0 before it). This applies to personal shots only, not melee, ordnance or turrets.
- Hazard: new `pub(crate) fn hazard_scaled(a: &Attempt, raw: i32) -> i32` in sim/combat.rs. It returns raw unchanged when the multiplier is 1.0, and otherwise `(raw as f32 * m).round() as i32`. Enemy-balance's death_effect burst and reach-attack paths call it before armor and Brace.
- **Recommender** (tree_view.rs:538-599): a ClassModifier node "acts now" only with an owned non-npc_only Ranged item of that class, mirroring :569-580. Counters use the default personal arm. No other recommender change.

## 6. Level-up grant (decision 14)
- `crate::progression::level_grant(level: u32) -> u64 = round(catalog().rewards.level_grant_xp_fraction × xp_to_advance(level - 1))`. `xp_to_advance(L) = round(20 + 15(L-1)^1.3)` is the existing `Player::next_level_xp` curve, extracted as `pub fn xp_to_advance_from(level)`. The fraction ships at 0.25.
- Values: L2 5, L3 9, L4 14, L5 21, L6 28, L8 44, L10 61, L12 80, L15 110, L20 166, L30 290, L40 430, L50 580. Run 1 (L1->6) pays 77 coins.
- `Game::settle_progression_rewards(&mut self) -> ProgressionRewards`:
  - pays Σ level_grant(l) for l in (level_grant_paid.max(1)+1)..=player.level, using saturating add;
  - sets `level_grant_paid = player.level`;
  - then pays milestones (section 7).
- It is called from `finish` (lifecycle.rs, after first_clear, after loot-economy's auto-sell) only when `!self.worker`.
- It is also called at the end of `settle_parallel` (parallel.rs:200-290) only when `self.attempt.is_none()`. Otherwise the next main finish settles it. A receipt paid from settle_parallel goes on the settled worker battle's Report and on the RunRecord just appended.
- The grant is never paid mid-battle and never in a clone.
- Takeover (decision 4): only player XP raises player.level, so a Mara or recruit win pays no player grant.

## 7. Rank milestones 5/10/15/20
- **Content:** the section-5 gates. Results "Now open" lists the nodes (milestone_opened_nodes, tree_view.rs:665-687).
- **Reward:** `progression.json rewards.milestones = [{rank 5, coins 500, item_quality Uncommon}, {10, 1000, Uncommon}, {15, 1500, Rare}, {20, 2000, Rare}]`. It is paid once when progression_rank() >= rank and the rank is not in `Campaign.milestones_claimed`.
- **Item:** `Campaign::grant_milestone_item(seed, rank) -> Result<(u64, ItemDefId), String>`.
  - Candidates: non-npc_only Ranged definitions whose class is in modifiers().weapon_classes (Pistol if empty), at tier min(current unlocked item tier, 4), falling back to the highest lower tier that has candidates.
  - Pick: `splitmix64(game.seed ^ fnv64("milestone") ^ rank)`. There is no RNG draw.
  - Level shop_item_level(). Quality is the authored fixed quality, an absolute floor like elites (decision 12). Decision 7's relative roll applies to drops; this is not a drop.
  - Granted after auto-sell, so auto-sell never sees it.
  - An inventory failure records `SettlementIssue::MilestoneItemLost { rank, definition, error }`. The coins and the claim still apply.
- **Results:** a headline above "Now open", "Rank 10 milestone · +1,000 coins · <item name>", taken from the receipt.
- NPC allowance is not changed here; that belongs to campaign-defenders.

## 8. Camp prices
- `crate::progression::camp_price(base: u64, rank: u32) -> u64 = round5(base × min(1 + per_rank × rank, max_multiplier))`, with `rewards.camp_prices { npc_training_base: 25, per_rank: 0.10, max_multiplier: 6.0 }`.
- `Campaign::npc_training_price()` is 25 / 30 / 50 / 75 / 150 at rank 0 / 1 / 10 / 20 / 50+. It also serves `StrategyState::buy_training_point` (strategy.rs:425-437).
- `Campaign::recruit_price()` uses base `world().strategy_config.recruit_cost` (100), giving 100 / 110 / 200 / 300 / 600.
- It replaces the literals at progression.rs:726, strategy.rs:296, loadout.rs:551, smoke.rs:170-175, ui/character.rs:193 and ui/operations.rs:158-160, 206.
- world.json is not regenerated.

## 9. Trainable rows
- `NpcTraining::TRAINABLE: [Self; 5] = [Strength, Dexterity, Health, Guns, Melee]` and `fn is_trainable(self)`.
- `Game::apply(TrainNpc | TrainRecruit)` returns the new `CommandError::TrainingUnsupported(NpcTraining)` before reaching the campaign. The variant needs a Display arm.
- The UIs iterate TRAINABLE.
- Remove "IQ {} (informational)" (character.rs:242-243) and PROGRESSION.md:43.
- The variants and their serde names stay (migrations.rs:73-77).

## 10. Per-archetype telemetry
- `CombatTelemetry.{spawned_by_archetype, kills_by_archetype}: BTreeMap<String, u32>` and `damage_taken_by_archetype: BTreeMap<String, u64>`, keyed by `resolved.archetype_id`.
- Increment points:
  - director.rs:218, next to spawned += 1;
  - combat.rs:351 for kills;
  - combat.rs:252-256 and the NPC injury path in sim/npc.rs for damage taken.
- Bosses and legacy unresolved enemies are skipped.
- Validation: <= 256 keys, valid_id, Σ kills_by_archetype <= kills. There is no per-archetype kills <= spawned check (A), because v6 attempts resume with enemies spawned before the maps existed.
- The maps are not copied into RunRecord.

## 11. Pacing v2 (`sim/pacing.rs`, `examples/pacing.rs`)
- **Profile:** `PacingProfile { id: &'static str, name, seed, country, aim, spend, max_runs, difficulty: BalancePreset, plan: PacingPlan }`, with `PacingPlan` holding:
  - `equip_drops: bool`
  - `sell: PacingSell { Never, ManualExtras, Auto }`
  - `defender: PacingDefender { Survivor, MaraWhenReady, RecruitWhenReady }`
  - `farm: PacingFarm { None, ReplayAfterDefeats(u32) }`
  - `train_companions: bool`
- **Profile ids:** expert-deu (the "default" acceptance profile, first column of the wave-0 table), novice-deu, novice-gear-lux, equip-drops, manual-sell, auto-sell, takeover, farm, easy, hard, long (150 runs). `by_id(&str)`, `DEFAULT_PROFILE_ID = "expert-deu"`.
- **Actions** all go through `Game::apply`: SetBalance before ChooseStart, BuySkillNode, BuyItem/Equip, SellUnprotected/SetAutoSell, BuyNpcPoint/TrainNpc, Recruit/RecruitAs, SetNextDefender, Deploy.
- **Seeds:** `pacing_seed(base, i)` returns base when i == 0, otherwise `splitmix64(base ^ i)`. For i > 0 the strategy RNG is `Rng::new(seed ^ 0x504C41475545)`, through a pub(crate) seam in strategy.rs.
- **Sweep:** `pacing_sweep(&[profile], seeds)` uses std::thread::scope with available_parallelism and sorts results by (profile, seed index).
- **Statistics:** `MilestoneStats { median, p25, p75, reached, seeds }` with the lower median. A missing milestone counts as max_runs+1 and prints ">N".
- **Milestone semantics: unchanged "runs completed when first held".** Pre-run Feature and TierFirstNode use `index`; post-run milestones use `index + 1`. The PacingReport doc (pacing.rs:62) is rewritten to say this, and a test pins it.
- **New milestones:** TreeExhausted, Liberation(u32), Rank(u32), FirstTakeover, and `TierOwned(usize)` (report-only).
- **`earned`** now adds `r.coins + r.progression.level_coins + Σ milestone coins + auto_sold_coins`.
- **`PacingRun` gains:** seed, difficulty, rank, level, lanes, defender, kills, level_grant_coins, milestone_coins, cumulative_coins, spent_tree, spent_gear, spent_training, spent_recruit, auto_sold_count, auto_sold_coins, nodes_owned, nodes_left, wallet, boss_ttk_s, and `archetypes: BTreeMap<String, ArchetypeRun { spawned, killed, damage_taken }>`.
- **CSV header:** profile,seed,difficulty,run,country,stage,outcome,elapsed_s,coins,level_grant_coins,milestone_coins,cumulative_coins,rank,level,lanes,defender,kills,learned_count,learned_ids,spent_tree,spent_gear,spent_training,spent_recruit,auto_sold_count,auto_sold_coins,nodes_owned,nodes_left,wallet,boss_ttk_s, then `<id>_spawned,<id>_killed,<id>_damage_taken` in catalog order.
- **CLI:**
  - `[max_runs] --runs --csv --out PATH` (UTF-8, no BOM; avoids PowerShell `>`);
  - `--profile ID|all` (repeatable), `--seeds N`, `--difficulty easy|normal|hard`, `--country ISO3`;
  - `--fit-tiers a,b,c`, which prints median earned at each target run ÷ tier cost;
  - `--values`, which prints the tree_value table;
  - strict parsing: unknown arguments exit 2.
- The default `pacing [N]` prints the 3 legacy profiles at 1 seed, which reproduces target/production-before/pacing.txt.
- **Acceptance** (ignored `tests/pacing_targets.rs`, release, expert-deu, Normal, 20 seeds):
  - goal5_first_victory_run_2_to_4;
  - goal5_tiers_affordable_3_5_7_9_11_13;
  - goal5_tree_outlasts_second_liberation: TreeExhausted > Liberation(2), and at least 1 node bought after it;
  - goal5_unlock_order_fortifications_mara_operations: median order holds, and <= holds in every seed.

  The other two legacy profiles are reported. long runs at 5 seeds and is informational.

## Saves and digests
- Every new saved field is skip-if-default, so wave 1 moves no digest.
- Wave 2 moves GOLDEN (all 6) once, for the telemetry maps.
- GAMEPLAY_TRACE never moves from this remit.
- Wave 4 moves no digest from this remit.
- save-determinism owns the SAVE_VERSION 7 envelope and the fixtures. This remit adds its v6_to_v7 sub-step and the fixture content.

## Risks and numbers to measure
- **First-victory slip** on expert (run 4 today). Mitigation: Steady hands +4 keeps the two-shot, the grant adds +77 coins in run 1, and the lever order applies. Measure the 20-seed median and p75.
- **T3 margin:** 19,700 against an estimated earned(11) of about 19.5-20k. Measure with --fit-tiers.
- **Trap +1 per rank** reaches 13 charges at rank 10. Watch trap_damage share against goal 6's 10-25% defeat window. The lever is trap-ladder price, never 0.5 per rank.
- **Value model is a guard, not proof.** Bots always weak-hit (smoke.rs:262-281), which pacing overstates; this is documented.
- **Cross-remit ordering:**
  - the hazard hook depends on enemy-balance's burst and reach paths;
  - the auto-sell and takeover profiles depend on loot-economy's SetAutoSell and campaign-defenders' SetNextDefender/RecruitAs;
  - auto-sell must run before milestone item grants.
- **Smoke:** Personal stage 1 already falls twice (attempt 3). Repricing may push it toward the 6-retry gate (simulation.rs:506). The route can be reordered toward gear, with a written reason.
- **Review fixture** ui/review/fixtures.rs:138 (allocate_body panic): qa-captures switches it to BuySkillNode body nodes funded from skills() prices.
- **Pacing runtime:** 20 seeds × 11 profiles is unmeasured. If pacing_targets exceeds about 10 minutes, keep 20 seeds only for expert-deu.
- **Numbers to record:**
  - wave-0 pacing-v2 20-seed medians with p25/p75 (the variance the critic asked for);
  - TierAffordable and TierOwned;
  - Liberation(2) and TreeExhausted;
  - per-archetype spawn/kill/damage-taken;
  - smoke before/after at waves 2 and 4;
  - the --values table at both refs for every node.

---

## Integration rulings

This section was added on 2026-11-05 when the synthesized spec above was copied into the plan. The spec above is kept verbatim as the design record. **Where this section and the spec disagree, this section wins.** The rulings come from four sources, applied in this precedence order: user decisions (U), orchestrator rulings (R, RL), master-planner reconciliations (P) and the five integration cluster rulings (C, D, E, S, W). The final saved-field table is `schema-v7.md`, the commit order is `waves.md` and the proof for every criterion is `acceptance.md`, all in this folder. The plan itself is `docs/plans/2026-11-05-production-pass.md`; any reference above to `docs/plans/2026-11-04-production-pass.md` means that file.

Each entry gives the binding ruling and then what it supersedes in **this** spec.

### User decisions

#### U1. SAVE_VERSION 7

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** SAVE_VERSION 7 bump approved before wave 1. v6_to_v7 is path-based, idempotent and catalogue-free; v1-v6 saves keep loading; save_v6.json is cut on untouched v6 code (W1-01) and save_v7.json is frozen at the wave-1 close (W1-63).
- **Supersedes in this spec:** Ledger sub-steps (c)(d) are confirmed in W1-24.

#### U5. Skill nodes

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** Up to 8 new append-only skill nodes (ids 170-177) are approved.
- **Supersedes in this spec:** Any "repricing only" fallback.

### Orchestrator rulings

#### R4. Legacy attempts and rules_revision

- **Owner / wave:** save-determinism, wave 2, 4.
- **Ruling:** No rules_revision field. Live legacy attempts keep their frozen snapshot fields; content-driven changes (drop pools, weights, trap charges) may affect continued legacy attempts; any continuation pin that moves is re-blessed together with that wave's GOLDEN re-bless with a written reason. Prefer a byte-identical legacy path when cheap (drop_tier None).
- **Supersedes in this spec:** trap charges_per_rank may reach live legacy attempts (E12).

#### R5. heavy_target_damage_multiplier

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** SkillModifiers.heavy_target_damage_multiplier may arrive later as a serde-defaulted field (is_one_f32 skip); it is the only pre-approved post-freeze field.
- **Supersedes in this spec:** The need to decide blast_discipline vs big_game_rounds before the freeze.

#### R8. Single owners

- **Owner / wave:** progression-tree, wave 1-4.
- **Ruling:** Pacing harness v2 = progression-tree (wave 2; CD and EB add profiles on its API); per-archetype telemetry = enemy-balance; SHA-256 / provenance / orphan helpers and scripts/lib/Native.ps1 = qa-captures (wave 1; audio, vfx and perf reuse them); measure_performance.ps1 = perf-architecture on top of Native.ps1; storm tagging = vfx presentation table only (no world.json change); regional checkpoints are derived (no saved field), presentation lines by CD plus EB's Regional boss rank (HP and reward x1.2) in wave-4 balance, no theme-rule change; save_v7.json is cut at the wave-1 close and frozen, later cases use inline JSON or new fixtures; stress scene raises the staged enemy cap only behind a review/test-only seam (no production save accepts &gt; cap+1), owner perf.
- **Supersedes in this spec:** Telemetry population (PT W2-B is struck).

#### R10. UI prices read core

- **Owner / wave:** progression-tree, wave 2-3.
- **Ruling:** Recruit and training prices in the UI read the core price function (Campaign::recruit_price / npc_training_price, Game::recruit_offer().price).
- **Supersedes in this spec:** Leaving UI literals to a later wave: they change in the same commit (W2-18).

### Master-planner reconciliations

#### P3. Provisional-pin ledger

- **Owner / wave:** save-determinism, wave 1 (W1-07).
- **Ruling:** determinism.rs holds GOLDEN, TRACE and PROVISIONAL: Option&lt;([u64;6],[u64;6])&gt; plus PROVISIONAL_CAUSES (commit tag, class F/C/G/T, reason). A digest-moving commit sets PROVISIONAL and appends its cause; the wave's SD re-bless folds it and resets it to None; the wave-close critic checks None.
- **Supersedes in this spec:** Any per-commit re-pin.

#### P7. Pacing harness owners

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** PT owns sim/pacing.rs v2; EB, CD and LE add registry rows and pure evaluators in their own commits; EB owns per-archetype telemetry population.
- **Supersedes in this spec:** PT W2-B telemetry population.

### Cluster: defenders

#### D15. Camp prices in core and existing UI labels

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** PT W2-D adds Campaign::recruit_price() and npc_training_price(); StrategyState::recruit (strategy.rs:296) and training (progression.rs:726) charge them; the same commit replaces ui/operations.rs:158/160/206 and ui/character.rs:193 literals, enable checks and hover text. Test camp_price_labels_follow_core_price_at_rank_10 (200 / 50 coins).
- **Supersedes in this spec:** Leaving the UI literals for later.

### Cluster: economy-progression

#### E1. Level grant and rank milestones: ledger design

- **Owner / wave:** save-determinism, wave 1 (schema), 2 (writer).
- **Ruling:** Ledger, not transition: Player.level_grant_paid (skip 0, &lt;= level); Campaign.milestones_claimed (skip empty, &lt;= 64, each &lt;= progression_rank()); Report.progression_rewards (renamed from PT's Report.progression); RunRecord.level_grant_coins / milestone_coins. v6_to_v7 (c) inserts level_grant_paid = level only when level &gt;= 2; (d) milestones_claimed = {r in [5,10,15,20] : r &lt;= v6_rank}, copied into battles. clone_for_worker and the op-chain level sync copy both.
- **Supersedes in this spec:** The name Report.progression; unconditional or_insert.

#### E2. Grant and milestone settlement writer and finish() order

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** Game::settle_progression_rewards: returns default in workers; pays level_grant(l) for l in (paid.max(1)+1)..=level and unclaimed reached milestones; milestone item pick keyed(seed, rank, 0, MILESTONE), authored fixed quality. Called in finish() at depth 0 in the order first_clear -&gt; stage clear -&gt; outbreak -&gt; finish_attempt -&gt; takeover release -&gt; H1 auto-sell -&gt; settle_progression_rewards -&gt; RunRecord -&gt; Report; and once at the end of settle_parallel when idle at depth 0. Never paid mid-battle or in a clone. Expected GOLDEN move 0; smoke and pacing move with recorded reasons.
- **Supersedes in this spec:** Any placement not after H1.

#### E7. Keyed hash module and tags

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** Only crate::hash: pub(crate) mix (moved byte-identically from quality.rs:112-117), keyed(seed, a, b, tag) = mix(seed ^ mix(a) ^ mix(b) ^ tag), mod tags {QUALITY (unchanged), ELITE, AFFIX, ELITE_PICK, RECRUIT_OFFER, MILESTONE} as big-endian ASCII. Users: elite spawn keyed(seed, a.id, e.id, ELITE) % 10000 &lt; elite_rate_bp (director wave spawns only); affix keyed(.., AFFIX); elite-only drop keyed(.., ELITE_PICK); recruit offer keyed(seed, next_recruit_id, fnv1a(country), RECRUIT_OFFER); milestone keyed(seed, rank, 0, MILESTONE). The presentation scan forbids hash::keyed.
- **Supersedes in this spec:** splitmix64(seed ^ fnv64("milestone") ^ rank).

#### E12. Legacy continuation: trap charges_per_rank

- **Owner / wave:** progression-tree, wave 4 (mechanism W2).
- **Ruling:** No SkillModifiers.structure_charges. StructureDefinition.charges_per_rank (serde default 0; shrapnel_trap 1 in wave 4). Placement (sim.rs:979) sets charges = d.charges + d.charges_per_rank * structure_upgrades[kind], like the durability bonus at sim.rs:967. Live legacy attempts may get the new charges (R4); a pin move would be re-blessed in W4-08 with a reason.
- **Supersedes in this spec:** Confirms the content field.

#### E13. Skip predicates for neutral-1.0 f32 multipliers

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** crate::schema: is_default, is_false, is_zero_f32 (bits, -0.0 written), is_one_f32 (bits), one_f32. Neutral-1.0 f32 fields allowed with skip_serializing_if is_one_f32: ProgressionSnapshot.{front_ramp, boss_armor/health/reward/damage_multiplier}, SkillModifiers.hazard_damage_multiplier, and later heavy_target_damage_multiplier. Writers produce the literal 1.0 at neutral inputs; no tolerance compare; validation rejects non-finite.
- **Supersedes in this spec:** hazard_damage_multiplier uses is_one_f32.

### Cluster: enemies-world-pacing

#### W-1. Pacing harness: one API, one acceptance file, named profiles

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** PT owns sim/pacing.rs v2 and examples/pacing.rs: PacingProfile{id, name, seed, country, aim, spend, max_runs, difficulty, plan: PacingPlan, stop_after}, PacingPlan{equip_drops, sell, defender, farm, train_companions}, by_id, DEFAULT_PROFILE_ID "expert-deu", pacing_seed, pacing_sweep (thread::scope), MilestoneStats (lower median, p25, p75; missing = max_runs+1). EB adds rows lux-easy/-normal/-hard (stop_after Liberation(1), max_runs 20) and start-aut/start-bgd; CD adds "takeover" with PacingDefender::RecruitWhenReady. Goal-5 and goal-6 acceptance use expert-deu, Normal, 20 seeds, 60 runs. One crates/horde_core/tests/pacing_targets.rs, all #[ignore = "release-only"], OnceLock sweep cache; run command in AGENTS.md. Default pacing 60 --runs reproduces pacing.txt byte-for-byte.
- **Supersedes in this spec:** Confirms ownership.

#### W-2. Operations unlock event for the unlock-order check

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** Operations unlock = the Operations page becoming reachable: Game::operations_available() moved verbatim from ui/context.rs:230-240 (ui delegates). Milestone::OperationsOpen appended. Fortifications = Feature(Construction), Mara = Feature(Companion). goal5_unlock_order_fortifications_mara_operations: per seed Fort &lt;= Mara &lt;= OperationsOpen and medians strictly increasing.
- **Supersedes in this spec:** Undefined Operations event in §11.

#### W-3. Tree outlasts the 2nd liberation

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** TreeExhausted = first post-run count with no unowned, non-retired node; Liberation(n) = first post-run count with world.cleared.len() &gt;= n. goal5_tree_outlasts_second_liberation: median(TreeExhausted) &gt; median(Liberation(2)) and median nodes bought after Liberation(2) &gt;= 1; a seed that never reaches Liberation(2) fails and is printed.
- **Supersedes in this spec:** goal5_tree_outlasts_second_liberation wording (median vs per-seed unstated).

#### W-10. generate_world.py reproducibility, difficulty bands, start clamp

- **Owner / wave:** enemy-balance, wave 2.
- **Ruling:** Commit A: indent-2 writer (byte-identical today), default --from-existing (offline, carries every key, recomputes only difficulty), --check (exit 1 on byte diff; exits 0 with the old formula), --rebuild with sha256 pins (exit 3) and stage-count guard (exit 4), --self-test; no scripts/world_stage_themes.json. Commit B: difficulty = (lo + fnv1a64("difficulty:"+id) % (hi-lo+1)) / 100 with bands &lt;= 3 stages 85-115, 4-9 100-135, &gt;= 10 120-160 (LUX .92, DEU 1.04, AUT .85, BGD 1.60). Start clamp Country::effective_difficulty -&gt; [0.95, 1.05] for the start country (no saved field), used for deploy terms and exposed via Game::effective_country_difficulty.
- **Supersedes in this spec:** No world regeneration by PT.

#### W-15. Enemy and pacing documentation ownership

- **Owner / wave:** enemy-balance, wave 4.
- **Ruling:** enemy-balance writes ENEMY_COMPOSITION.md, PROGRESSION_SCALING.md, the goal-6 section of BALANCE_AND_RUN_STATS.md and the WORLD.md band paragraph at the wave-4 close (W4-10); progression-tree writes the goal-5 section and the AGENTS.md pacing_targets section (W2-03, W4-11); qa's docs inventory checks presence only.
- **Supersedes in this spec:** AGENTS.md section assigned.

### Rulings owned by this remit

U5, R5, R8, R10, P7, D15, E2, E12, W-1, W-2, W-3
