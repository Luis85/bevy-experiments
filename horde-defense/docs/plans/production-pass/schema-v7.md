# Last Light: final frozen v7 schema (master planner, 2026-11-05)

Status: **FINAL. Frozen at the wave-1 close by `save_v7.json` (W1-63).** This document supersedes `freeze-v7.md` wherever the two differ. It applies the user decisions (SAVE_VERSION 7, R1-R11) and the five binding cluster rulings. After the freeze, any new saved field must be serde-defaulted, skipped at its default, and must load `save_v7.json` byte-identically. Otherwise the work stops and asks the user. The one pre-approved exception is R5: `SkillModifiers.heavy_target_damage_multiplier`.

## 0. Planner reconciliations (where two binding inputs disagreed)

| # | Conflict | Ruling applied | Reason |
|---|---|---|---|
| P1 | Wave-tier bound. The economy cluster says `MAX_WAVE_TIER = 15` (EB, wave 2). The enemies cluster says `MAX_WAVE_TIERS = 64` (SD, wave 1). | `pub const MAX_WAVE_TIER: usize = 15`, inclusive (`wave_tier > MAX_WAVE_TIER` is rejected), at progression.rs:134 and :247. It lands in the **wave-1** SD/EB schema commit (W1-28). The load-time check `every_floor_tier_exists_in_enemy_catalog` lands in wave 2 with the content (W2-31). | It is the tighter save validation, with 4 tiers of headroom over the authored maximum of 11. Wave-1 placement is needed because save_v7.json stages a tier-11 snapshot. Widening later is validation-only. The 64 bound is withdrawn. |
| P2 | Economy says the auto-sell Command variants land in wave 1 with handlers in wave 2. The brief allows only typed errors, so a placeholder handler is not allowed. | All 3 variants (DeletePreset, SetAutoSell, ApplyAutoSellToStash) land in wave 1 with **real** handlers. LE's upgrade-metric extraction moves from wave 2 to wave 1 (W1-22), so the stash preview can judge `UpgradeJudge::AnyDefender`. Wave 2 adds only the settlement hooks (H1, W-a) and `results_loot`. | This keeps the recording vocabulary fixed in wave 1 and avoids untyped stubs. |
| P3 | R7 (one re-bless commit per wave) against "every commit stays green". | **Provisional-pin ledger**, added in W1-07. `determinism.rs` holds `GOLDEN`, `TRACE` and `PROVISIONAL: Option<([u64;6],[u64;6])>` plus `PROVISIONAL_CAUSES: &[(commit_tag, class F/C/G/T, reason)]`. A digest-moving commit sets PROVISIONAL and appends its cause, and the test accepts PROVISIONAL only when the causes are non-empty. The wave's dedicated SD re-bless commit folds PROVISIONAL into GOLDEN/TRACE, writes the per-digest reason table and resets PROVISIONAL to None. The wave-close critic checks that PROVISIONAL is None. | Every commit stays green, there is exactly one deliberate re-bless per wave, and each cause is recorded where it happens. |
| P4 | RULESET numbering: freeze 2/3 vs the settings cluster 2(W1)/3(W2)/4(W4). | 1 to **2** at the LE strict-sale commit (W1-20), **3** at the wave-2 re-bless, **4** at the wave-4 re-bless. `TRACE_RULESET` follows in the same commits. | The settings cluster ruling is the later and more specific one. |
| P5 | Duplicate UI owners for the Results loot block and the Auto-sell panel (UX #22/#23 vs LE #12/#13). | **loot-economy** owns both widgets in wave 3. ux-flow signs off on the primary-action rule and the layout. | One writer per file region. |
| P6 | CD's regional-checkpoint theme and accessors (CD #11) vs the EB derivation. | EB owns `FrontPosition`/`Country::regional_checkpoints()` (wave 2). CD owns only the presentation lines (wave 4). No theme change. | Ruling R8 plus the enemies cluster. |
| P7 | Pacing harness owners. | PT owns `sim/pacing.rs` v2. EB, CD and LE add registry rows and pure evaluators in their own commits. EB owns per-archetype telemetry population (PT W2-B is struck). | R8 |
| P8 | `horde_audio` crate. The SHA ruling forbids a crate *for SHA*. The settings ruling makes `horde_audio::mix` conditional. | No crate in wave 1. A Bevy-free path crate `crates/horde_audio` is proposed for wave 6 (A6-00) and is **open for the user** (default: create it). | It adds no external dependencies. The Cargo.lock diff is one path entry. |

## 1. Serde and determinism conventions (binding for every field below)

- **C1. Defaults and skip.** Every new field has `#[serde(default)]` or `default = "fn"` with a legacy-neutral value, plus `skip_serializing_if` set to that default. This is mandatory on structs without a container default: RunRecord, CombatTelemetry, Recruit, StrategyState, Player, Report, Attempt, Enemy, ParallelBattle, Campaign, and ResolvedEnemy (which is deny_unknown_fields).
- **C2. Predicates live in the new `crate::schema` module** (pub(crate)):
  - `is_default<T: Default + PartialEq>` and `is_false`;
  - `is_zero_f32(v) = v.to_bits() == 0`, so -0.0 is still written;
  - `is_one_f32(v) = v.to_bits() == 1.0f32.to_bits()`;
  - `one_f32() -> 1.0` and `one_u8() -> 1`.

  Neutral-1.0 f32 fields are allowed. Writers must produce the literal 1.0 at neutral inputs (short-circuit) and never compare against 1.0 with a tolerance. Validation rejects non-finite values.
- **C3. Wave-1 writers.** Wave 1 adds types, migration, validation and readers. The only wave-1 writers are digest-neutral for GOLDEN:
  - worker seat and appearance stamping (GOLDEN has no workers);
  - SetAutoSell, DeletePreset and the stash apply (never dispatched in GOLDEN);
  - the balance_extras_basis writer (GOLDEN is Normal).
- **C4. `deny_unknown_fields` types stay closed.** BalanceSettings gains no field. ResolvedEnemy gains fields only under C1.
- **C5. v6_to_v7 is path-based, idempotent (or_insert), catalogue-free** and walks only the root payload and `battles[*].game`.
- **C6. Append-only enums:** Command, CommandError, CommandOutcome, RecruitStatus, BossRank, SettlementIssue, EventKind/SimEvent, ExtraAction, WindowModeSetting, QualityMarkers, Palette, hash tags. Existing Command shapes are frozen; recordings serialize them.
- **C7. `state_digest`** = FNV of `serde_json::to_string(Game)`. Every serialized core field is in it. `#[serde(skip)]` fields, settings.json, catalogues and assets are never in it.
- **C8. Keyed hashing.** Presentation and keyed rolls never draw `Attempt.rng` or the strategy RNG. Keyed rolls use `crate::hash::keyed(seed, a, b, tag) = mix(seed ^ mix(a) ^ mix(b) ^ tag)`. `mix` moves byte-identically from quality.rs:112-117. Tags are u64 big-endian ASCII, append-only:
  - `QUALITY = b"QUALITY\x01"` (unchanged, 0x5155_414C_4954_5901);
  - `ELITE = b"ELITE\0\0\x01"`;
  - `AFFIX = b"AFFIX\0\0\x01"`;
  - `ELITE_PICK = b"ELPICK\0\x01"`;
  - `RECRUIT_OFFER = b"OFFER\0\0\x01"`;
  - `MILESTONE = b"MILEST\0\x01"`.

## 2. Envelope constants

| Constant | Value | Changes |
|---|---|---|
| `SAVE_VERSION` (migrations.rs) | 7 | W1-14. Adds the arm `6 => v6_to_v7(value)?`. Version 8 is rejected. |
| `RULESET_VERSION` (persistence.rs:11) | 1 to 2 (W1-20 strict sale), 3 (W2 re-bless), 4 (W4 re-bless) | Envelope only. `TRACE_RULESET` is asserted equal. No digest effect. |
| `SETTINGS_VERSION` (settings.rs) | 1 from W1-31, 2 from wave 6 (A6-17) | settings.json only |
| Recording FORMAT | 1 (unchanged) | pinned by `recording_vocabulary_v1` |
| `GAMEPLAY_ENEMY_CAP` (progression.rs:238) | 224 (+1 boss = 225 drawn) | Replaces the literal. Production validation does **not** widen. |
| `STRESS_ENEMY_CAP` | 399 | Only under `cfg(any(test, feature="test-support", feature="review"))`. Never saveable. |
| `MAX_WAVE_TIER` | 15 (inclusive) | P1, W1-28 |
| `MAX_RUN_DROPS` | 4096 | W1-21 |
| `MAX_AUTO_SOLD_LINES` | 12 | W1-21 |
| `V7_MILESTONE_RANKS` (migration-frozen) | [5,10,15,20] | W1-24 |

## 3. Saved fields: complete v7 table

Legend:
- **Mig** is the v6 to v7 step (see section 4).
- **W** is the wave of the first non-default writer.
- **Digest** is the effect on GOLDEN (6 runs: rank 0, Personal and Npc, 1800 ticks, attempt live, `last_result` None, no workers).

### 3.1 Campaign (campaign.rs:31; no container default; `clone_for_worker` is a struct literal at :212)

| Field | Type | Default / skip | Validation | clone_for_worker / op-chain sync | Mig | W | Digest |
|---|---|---|---|---|---|---|---|
| `auto_sell` | `AutoSellRule { enabled: bool, sell_below: Quality, slots: [bool;4] (Slot order), keep_upgrades: bool, apply_to_npc_runs: bool }`, container default, PartialEq | `{false, Uncommon, [true;4], true, true}`; skip `is_default` | `sell_below != Poor`; must be default in a worker | worker = default | serde default (off) | W1 (SetAutoSell) | none |
| `next_defender` | `Option<DefenderId>` | None; skip None | `Recruit(id)` must be in the roster; None in workers | worker = None | none | W2 | none |
| `reserved_recruit` | `Option<u64>` | None; skip None | D7: at depth 0 it equals the Defending recruit id, iff a seat Recruit attempt is live; None in workers | worker = None | none | W2 (W1 staging seam) | none |
| `milestones_claimed` | `BTreeSet<u32>` | empty; skip empty | ≤ 64 entries, each ≤ `progression_rank()` | copied to workers and on the op-chain sync | (d) | W2 | none |
| `balance_extras_basis` | `BalancePreset` | Normal; skip Normal | never Custom; preset != Custom implies Normal | copied to workers and on the op-chain sync (parallel.rs ~:286) | none (v6 Hard gets Hard extras at the next deploy; v6 Custom gets Normal) | W1 (set_balance_settings) | none |

**As cut (annotation, 7 November 2026, W1 review):** the first non-default writer of `milestones_claimed` is wave 1, not W2. Besides migration step (d), 3df1242 added `Campaign::claim_neutral_milestones` (campaign/progression.rs), called from `first_clear` (campaign/world.rs) after each victory: while the catalogue authors no milestones, it inserts every `V7_MILESTONE_RANKS` entry at or below `progression_rank()`, unpaid and with no receipt. A native v7 save made by a wave-1 build can therefore carry claims at ranks 5, 10 and so on that no reward paid. W2-22's `settle_progression_rewards` removes the shim. The row above is unchanged.

### 3.2 Player (campaign/progression.rs:147)

| Field | Type | Default / skip | Validation | Mig | W | Digest |
|---|---|---|---|---|---|---|
| `level_grant_paid` | u32 (0 and 1 are equivalent) | 0; skip 0 | ≤ level | (c): inserted = level only when level ≥ 2 (root and every `battles[*].game.campaign.player`). The op-chain level sync also copies it. | W2 (root finish / settle_parallel only, never in a worker) | none |

### 3.3 Attempt (sim.rs:323)

| Field (serde key) | Type | Default / skip | Validation | Mig | W | Digest |
|---|---|---|---|---|---|---|
| `seat` (`"defender"`) | `DefenderSeat { #[default] ModeOwner, Recruit(u64) }`. A Recruit seat serialises as `{"Recruit":id}`, the same as `DefenderId::Recruit`. | ModeOwner; skip `is_mode_owner` | Personal + Recruit is invalid. Depth-0 rules: D7. A worker battle seat must equal `Recruit(battle.recruit_id)`. | (a): battles only | W1 (worker stamping, W1-15); W2 (takeover) | none (no workers in GOLDEN) |
| `drop_ids` | `Vec<u64>` | empty; skip empty | ascending, unique, len ≤ loot, ≤ 4096, each < next_item_id; disjoint from `pending_auto_sell` | none | W2 (depth 0, drop_tier Some path only) | **F move W2** |
| `drop_tier` | `Option<u8>` | None; skip None | 1..=max_drop_tier(). Every attempt deployed from W2 on is always Some. **None means a pre-v7 legacy loot path.** It is a loot-only marker. | none | W2 | **F move W2** |
| `profile.appearance` (existing field) | Appearance | existing default Male | none | (e): Npc attempts get or_insert "Female" | W1 (worker stamp from `recruit_identity(id).1`); W2 (takeover stamp) | none |

### 3.4 Report (sim.rs:674)

| Field (key) | Type | Default / skip | Validation | Mig | W | Digest |
|---|---|---|---|---|---|---|
| `seat` (`"defender"`) | DefenderSeat | ModeOwner; skip | Copied from the attempt at lifecycle.rs:365. Depth 0 with Recruit(id) requires mode Npc and the id in the roster (any status). A worker report's seat must equal Recruit(b.recruit_id). | (a): battles only; the root stays keyless | W1 | none |
| `drop_ids` | `Vec<u64>` | empty; skip | len ≤ loot, unique, each < the owning game's next_item_id (worker-local ids in worker reports) | (b): from `loot_events` ids, sorted and deduped | W2 | none (GOLDEN never finishes) |
| `loot_by_quality` | `KindMap<Quality,u32>` | all zero; skip all-zero | sum ≤ loot | (b): run_history match (root) or a count of loot_events | W2 | none |
| `auto_sold` | `AutoSoldReceipt { count: u32, coins: u64, by_quality: KindMap<Quality,u32>, lines: Vec<AutoSoldLine { definition: ItemDefId, level: u32, quality: Quality, coins: u64 }>, kept_on_overflow: u32 }`. **No item_id.** | empty; skip default | `count + kept_on_overflow ≤ loot`; `lines.len() ≤ min(count,12)` (highest coins kept, ties broken by ascending id); `Σ by_quality == count`; `Σ line.coins ≤ coins`; count 0 implies coins 0 and no lines; every definition resolves | none | W2 | none |
| `progression_rewards` | `ProgressionRewards { level_from: u32, level_to: u32, level_coins: u64, milestones: Vec<MilestoneReward { rank: u32, coins: u64, item_id: Option<u64>, item_definition: Option<ItemDefId> }> }`, container default | empty; skip default | ≤ 4 milestones with strictly increasing ranks; `level_from ≤ level_to ≤ cap` | none | W2 | none |
| `boss_rank` | `BossRank { Sector, Regional, Capstone }` | Sector; skip | none | none | W2 | none |

### 3.5 RunRecord (stats.rs:143; no defaults today, so field-level default + skip is mandatory)

| Field | Type | Default / skip | Validation | W |
|---|---|---|---|---|
| `recruit_id` (existing; new writer) | Option<u64> | unchanged | `worker_id None && recruit_id Some` implies mode Npc | W2 (takeover) |
| `auto_sold_count` / `auto_sold_coins` | u32 / u64 | 0; skip 0 | count ≤ loot; count 0 implies coins 0 | W2 |
| `level_grant_coins` / `milestone_coins` | u64 / u64 | 0; skip 0 | none | W2 |
| `elite_kills` | u32 | 0; skip 0 | ≤ kills | W2 |
| `boss_ttk_ticks` | Option<u64> | None; skip | ≤ duration_ticks | W2 |
| `boss_rank` | BossRank | Sector; skip | none | W2 |

### 3.6 ParallelBattle (sim.rs:746)

| Field | Type | Default / skip | Validation | W |
|---|---|---|---|---|
| `summary` | `Option<ExpeditionSummary { outcome: Option<Outcome>, stages_cleared: u32, liberated: bool, fronts: Vec<String>, coins: u64, loot_count: u32, acknowledged: bool }>`, container default | None; skip None | `acknowledged` implies settled; `outcome.is_some()` implies settled; fronts ≤ 32. v6 settled battles stay None and are never announced. | W2 |
| `pending_auto_sell` | `Vec<u64>` (parent-side ids) | empty; skip | unique; each < the parent's next_item_id; disjoint from `reserved_items` and the root `attempt.drop_ids`; settled implies empty | W2 |

### 3.7 StrategyState / Recruit / RecruitStatus (strategy.rs)

| Field | Type | Default / skip | Notes | W |
|---|---|---|---|---|
| `StrategyState.folded_outcomes` | u32 | 0; skip 0 | Incremented (saturating) when pruning must drop an unacknowledged settled battle | W2 |
| `Recruit.containments` | u32 | 0; skip 0 | +1 where a guarded outbreak becomes Contained (~:774) | W2 |
| `RecruitStatus::Defending` | appended variant | n/a | The only takeover status. Deployed keeps meaning "on an expedition". tick_world skips claim_objectives for `Deployed \| Defending`. | W1 (type), W2 (writer) |

### 3.8 Enemy (sim.rs:94)

| Field | Type | Default / skip | Validation | W |
|---|---|---|---|---|
| `split_child` | bool | false; skip | children are never elite and never split | W2 |
| `boss_state` | `Option<BossState { profile: BossPhaseProfile { name, speed_mult 1..=2, armor_mult .25..=1, call_interval_mult .3..=1, siege_recovery_mult .3..=1, hold_ticks 0..=180 }, transitions: u8 (0..=2), entered: u8 (≤ transitions), hold_until: u64 }>` | None; skip | bosses only | W2 |

### 3.9 ResolvedEnemy (enemy_catalog.rs:49; deny_unknown_fields; field default + skip is mandatory)

| Field | Type / bounds | Default (skip) |
|---|---|---|
| `reach` | f32 0..=12 yd; the runtime stop is clamped to `width_yards/2` (far-half rule) | 0.0 (`is_zero_f32`) |
| `ranged_armor` | i32 0..=1000 | 0 |
| `npc_ranged_armor` | i32 0..=100 | 0 |
| `stagger_immune` | bool | false |
| `group_size` | u8 1..=6 | 1 (`default = one_u8`, skip == 1) |
| `death_effect` | `Option<DeathEffect>`: `Burst { radius (0,3], damage 1..=10000, structure_damage 0..=10000, npc_dice 1..=20, npc_add -30..=100, fuse: bool }` or `Split { archetype_id, count 1..=4 }` | None |
| `tags` | `Vec<String>` ≤ 8, valid_id, unique | empty |
| `elite` | `Option<EliteMark { affix_id: String, name: String (1..=60) }>`; elite stats are baked into the other fields | None |

The 11 existing archetypes serialize nothing new. An unknown affix id or split target still loads (snapshot independence).

### 3.10 ProgressionSnapshot (progression.rs:194; container default with a manual neutral Default; every field is added to Default and skipped)

| Field | Type / bounds | Neutral | Writer rule (W2 unless noted) |
|---|---|---|---|
| `elite_rate_bp` | u32 0..=10000; 0 when rank < 3 | 0 | `min(10000, elite_bp(r) * extras.elite_rate_pct / 100)`; elite_bp: 0 below 3, 200 at 3, linear to 1500 at 23, then flat |
| `front_ramp` | f32 1..=2 (already folded into enemy_health and arrival) | 1.0 (`is_one_f32`) | `1 + front_ramp_max * (front_ramp_pct/100) * depth`; the literal 1.0 when depth == 0, pct 0 or no country |
| `boss_rank` | BossRank | Sector | `FrontPosition::boss_rank()`: Capstone if stage+1 == count, else Regional if (stage+1)%6 == 0, else Sector |
| `boss_phases` | u8 0..=2 | 0 | `min(2, (r>4) as u8 + boss_phase_bonus + rank.extra_transitions)` |
| `boss_armor_multiplier` | f32 1..=4 | 1.0 | EB rank formula (W2) × rank factor (W4) |
| `npc_boss_armor_add` | i32 0..=20 | 0 | EB (W2); Capstone +2 (W4) |
| `boss_health_multiplier` | f32 1..=2 | 1.0 | Regional 1.2, Capstone 1.4 (W4) |
| `boss_reward_multiplier` | f32 1..=2 | 1.0 | Regional 1.2, Capstone 1.4 (W4) |
| `boss_damage_multiplier` | f32 1..=4 | 1.0 | `clamp((1 + boss_damage_per_rank * max(0, min(r,50)-4)) * rank_damage, 1, 4)`; rank term W2, Capstone 1.10 W4 |
| `wave_tier` (existing) | bound becomes ≤ MAX_WAVE_TIER (15) | none | W1-28 (validation only) |
| `enemy_cap` (existing) | bound stays ≤ GAMEPLAY_ENEMY_CAP (224) | none | unchanged |

### 3.11 SkillModifiers (skill_catalog.rs:310; container default with a manual Default; saved inside Attempt.profile)

| Field | Type / bounds | Neutral (skip) |
|---|---|---|
| `class_modifiers` | `BTreeMap<WeaponClass, ClassModifiers { damage_multiplier, reload_multiplier, cadence_multiplier: f32 (finite 0.01..=100, default 1.0), weak_bonus_add: f32 (0..=1, default 0.0) }>`; ≤ 6 entries; inner fields are always written | empty |
| `armor_penetration` | i32 0..=1000 | 0 |
| `hazard_damage_multiplier` | f32 finite 0.01..=1.0 | 1.0 (`is_one_f32`) |
| *(R5, may arrive later)* `heavy_target_damage_multiplier` | f32 | 1.0 (`is_one_f32`); serde-defaulted, format-neutral |

### 3.12 CombatTelemetry (sim/telemetry.rs:5; manual Default; field default + skip)

The fields are `spawned_by_archetype`, `kills_by_archetype` (`BTreeMap<String,u32>`), `damage_taken_by_archetype` (`BTreeMap<String,u64>`), `elites_spawned`, `elites_killed` (u32), `hazard_damage_taken` (u64) and `boss_phase_ticks` (`Vec<u64>` ≤ 2).

Rules:
- Keys are valid_id, at most 256 per map.
- Bosses and unresolved legacy enemies are excluded.
- Σ kills ≤ kills; per-archetype kills ≤ spawned.
- The writer is EB in W2. This is a **T move** of all 6 GOLDEN.

### 3.13 Appended event kinds

`EventKind`/`SimEvent` gain `Spit`, `Detonation`, `Split`, `BossPhase { boss_id, kind, phase: u8, lane, at }`. Types land in W1-28; writers in W2. No horde_game wildcard remains after A6-12.

## 4. v6 to v7 transform (final; SD owns ordering; each sub-step lands in its owner's commit)

```text
fn v6_to_v7(mut v) -> Result<Value, SaveError>          // path-based, idempotent (or_insert), catalogue-free
  // (a) defender seats (W1-14, SD): workers only; the root attempt and last_result stay keyless (absent = mode owner)
  for b in v.battles[*] (a missing array means nothing to do):
      rid = b.recruit_id as u64 else Err(SaveError::Migration("battle without recruit_id"))
      for key in ["attempt","last_result"]: if b.game[key] is object: b.game[key].or_insert("defender", {"Recruit": rid})
  // (e) defensive appearance (W1-14, SD): for a in [v.attempt] + [b.game.attempt]: if a.mode=="Npc": a.profile.or_insert("appearance","Female")
  // (b) report drop tracking (W1-21, LE sub-step): for r in [v.last_result] + [b.game.last_result]:
      if !r.drop_ids and loot_events non-empty: r.drop_ids = sorted_unique(loot_events[*].item_id)
      if !r.loot_by_quality: q = root ? run_history.find(attempt_id==r.attempt_id && worker_id==null).loot_by_quality : None
                             q = q or count(loot_events by quality); insert only if any non-zero
  // (c) level-grant ledger (W1-24, PT sub-step): for p in [v.campaign.player] + [b.game.campaign.player]:
      if p.level >= 2: p.or_insert("level_grant_paid", p.level)        // no retroactive grant; level 1 writes nothing
  // (d) milestones (W1-24): rank = v6_rank(v.campaign)   // frozen copy of progression_rank()/legacy_clear_rank()
      set = {r in [5,10,15,20] : r <= rank}; if non-empty: v.campaign.or_insert("milestones_claimed", set); battles[*].game.campaign likewise (parent's set)
  // everything else: serde defaults (auto_sell off, seats ModeOwner, summary None, telemetry empty, snapshot neutral, basis Normal)
```

Rules:
- Applying it twice gives an equal Value.
- A resave is byte-identical to a second resave.
- v6 in-flight expeditions keep Female until their next chained re-deploy. SAVE_FORMAT.md documents this.
- There is no `acknowledged` insert and no notice burst.
- `Game::from_payload(value, version)` runs migrate, then reconcile, then validate. Debug replay uses it when `header.version < SAVE_VERSION`.

**As cut (annotation, 7 November 2026, W1 review):** sub-step (d) as implemented by W1-24 (24c879f, `v7_milestones_claimed` in `sim/migrations.rs`) does not give each `battles[*].game.campaign` the parent's full set. It gives the parent's claims limited to `r <= min(parent rank, v6_rank(that campaign))`, so an expedition keeps only the claims its own v6 rank reached; the root is unchanged. Without the cap, a v6 expedition cloned below the parent's rank would carry a claim above its own rank, fail the `milestones_claimed` bound (section 3 table, checked by `validate_game` per section 5: each claim <= `progression_rank()` in every game), and that v6 save would not load. The pin is `v7_ledgers_mark_levels_paid_and_claim_reached_milestones_idempotently` (a rank-9 expedition under a rank-15 parent keeps `[5]`); SAVE_FORMAT.md step 5 documents the capped rule. The pseudo-code above is unchanged.

## 5. Save validation added in v7 (persistence.rs `validate_game`)

**D7 seat and reservation rules:**
1. A Recruit seat with mode Personal is invalid.
2. At depth 0, a live seat Recruit(id) requires all of the following:
   - exactly one roster recruit is Defending, and its id is `id`;
   - `reserved == Some(Npc)` and `reserved_recruit == Some(id)`;
   - its loadout is in `expected_reservations`;
   - it is in no unsettled battle and in no guard.
3. At depth 0 with no attempt or a ModeOwner seat: no recruit is Defending, and `reserved_recruit` is None.
4. A depth-0 `last_result` with Recruit(id) requires mode Npc and the id in the roster.
5. In the parent battle loop, the attempt and last_result seats must equal `Recruit(b.recruit_id)`.
6. Worker games: `reserved_recruit` None, `next_defender` None, no Defending, auto_sell default.
7. `next_defender` Some(Recruit(id)) requires the id in the roster.
8. RunRecord with worker_id None and recruit_id Some requires mode Npc.

`item_reserved` and `reserve_external` treat Some(Npc) as covering Mara's loadout only when `reserved_recruit.is_none()`.

**Other rules:**
- Loot bounds: §3.3-3.6.
- Progression ledgers: §3.1-3.2.
- Summary bounds: §3.6.
- `balance_extras_basis`: §3.1.
- Telemetry sums: §3.12.
- Snapshot bounds: §3.10.
- Enemy bounds: §3.8-3.9.
- `boss_ttk_ticks ≤ duration_ticks`.

## 6. Commands, outcomes and errors (append-only, final order after `SalvageExpedition`)

| # | Variant | Wave / owner | Handler | Errors (appended) |
|---|---|---|---|---|
| 1 | `DeletePreset { index: usize }` | W1-20 LE | Removes the preset and unblocks its items for sale | `Rejected` (unknown index; existing string path) |
| 2 | `SetAutoSell(AutoSellRule)` | W1-23 LE | Validates and stores. It is allowed in or out of battle and takes effect at the next settlement. | `AutoSellRuleInvalid(Quality)` (Poor) |
| 3 | `ApplyAutoSellToStash { expect_count: u32, expect_coins: u64 }` | W1-23 LE | Recomputes `auto_sell_preview(rule, Stash)` on a clone: Player origin, judge AnyDefender, ignores `enabled`, never sells quality None. On success it commits clone-then-commit like command.rs:201-212. | Checked in order: `SaleBlocked(SaleBlock::InBattle)`, `NothingToSell`, `SaleQuoteChanged { count, coins }` (no mutation), `WalletLimit` |
| 4 | `SetNextDefender(DefenderId)` | W2 CD | Stores Some | `DefenseInProgress`, `DefenderUnavailable { defender, blocker }`, `UnknownSurvivor` |
| 5 | `BeginDefense { stage: usize }` | W2 CD | Deploys `next_defender()` at the active country's stage, with Deploy's unpause; clears next_defender on success | as Deploy, plus `DefenderUnavailable` |
| 6 | `SetRecruitTactics { id: u64, tactics: NpcTactics }` | W2 CD | Allowed for Ready or Defending only | `RecallThresholdOutOfRange`, `RecruitNotReady` |
| 7 | `SelectCountry(String)` | W2 CD | Sets `world.active`. It never creates an attempt; a liberated outbreak country gives `CountryLiberated`. | `StartNotChosen`, `CountryLocked(String)`, `DefenseInProgress` |
| 8 | `RecruitAs { country: String, archetype: String }` | W2 CD | Takes an offer from `recruit_offer` (keyed RECRUIT_OFFER, no RNG). `Recruit(String)` stays and takes offer[0]. | `ArchetypeNotOffered(String)`, `RecruitUnavailable(RecruitBlocker)` |
| 9 | `AcknowledgeOutcomes { battles: Vec<u64>, folded: u32 }` | W2 CD | Idempotent. It ignores unknown, unsettled and already-acknowledged ids; `folded_outcomes` saturating_sub; returns Applied. | none |

- **CommandError also appends:** `SaleBlocked(SaleBlock)` and `WalletLimit` (W1-20); `DefenseInProgress` and `DefenderUnavailable { defender: DefenderId, blocker: DefenderBlocker }` (W1-12), where `DefenderBlocker { Locked { node_id }, Guarding { country_id }, OnExpedition { battle }, Defending, InvalidLoadout { reason } }`; `TrainingUnsupported(NpcTraining)` (W2 PT). Every variant has a plain Display with no CamelCase.
- **CommandOutcome appends:** `Sold { count: u32, coins: u64 }`. The enum stays Copy.
- **Frozen meanings:**
  - `Deploy { stage, mode }` means Personal = Player and Npc = Mara, and never touches next_defender.
  - `ContinueJourney` deploys `next_defender()` and clears it on success.
  - retry, start_repeat and continue_repeat use `report.defender()`.
  - `SellItem` is strict from W1-20: a typed SaleBlocked, and no reconcile_loadouts after a sale.
  - `EnterCountry` is kept for recordings.
- **Typed pre-check (W1-12):** AssignGuard, DeploySurvivor, CountryOperation, TrainRecruit, EquipRecruit, BuyRecruitPoint and SetRecruitPolicy on the Defending recruit return `DefenderUnavailable{Recruit(id), Defending}` with no mutation.
- **Pure read APIs** (no RNG; each has a `to_value` before == after test):
  - `Game::sale_eligibility(id) -> Result<SaleQuote{coins}, SaleBlock>` (R11 facade over the internal `sale_eligibility_for(id, origin, &SaleRefs)` and `plan_sale`);
  - `auto_sell_preview`, `results_loot`;
  - `available_defenders`, `defender_card`, `active_defender`, `battle_defender_card(battle_id)`;
  - `defender_power(defender, next_threat)`;
  - `next_defender() = campaign.next_defender.unwrap_or(last_result.map_or(Player, Report::defender))`;
  - `Attempt::defender()`, `Report::defender()`, `Attempt::defender_appearance()`;
  - `unacknowledged_outcomes() -> OutcomeNotices { notices: Vec<ExpeditionNotice>, folded }`;
  - `recruit_offer(country)` (price = `Campaign::recruit_price()`), `service_record(recruit_id)`;
  - `operations_available()` (moved verbatim from ui/context.rs:230);
  - `effective_country_difficulty(id)`, `stage_boss_rank(country, stage)`, `stage_preview`;
  - `Campaign::recruit_price()`, `Campaign::npc_training_price()`;
  - `Campaign::balance_extras()`, `Campaign::balance_extras_preset()`;
  - `CampaignFeature::id()`.

## 7. Catalogue and asset schema changes (not saved; never in the digest)

- **balance.json** `BalancePresetDefinition.extras: PresetExtras { elite_rate_pct: u16 0..=300, front_ramp_pct: u16 0..=300, boss_phase_bonus: u8 0..=2 }`, serde default Normal. The values are Easy {50,75,0}, Normal {100,100,0} and Hard {150,150,1}. Lands in W1-29.
- **progression.json** (all keys serde default to legacy-neutral):
  - `front_ramp_max` (0..=0.5, bundled 0.18; tuning window [0.17,0.25]);
  - `boss_damage_per_rank` (0..=0.05, bundled 0.01);
  - `boss_ranks.{sector,regional,capstone}.{health,armor,reward,damage,npc_armor_add,extra_transitions}`;
  - growth knee keys, `pressure_floors` rows 5..=28, the elite curve block;
  - `rewards { level_grant_xp_fraction, milestones[] }`, `camp_prices`.
- **enemy_catalog.json:** 7 archetypes (exploder, splitter, spitter, shield-bearer, sprinter swarm, juggernaut, plus the 7th per EB), waves 5-11, `elite_base`, `elite_affixes`, `boss_phases` (one per BossKind).
- **content.json:** 39 `loot_only` items (indices 29-67); `ItemDefinition.loot_only: bool` (default false); `StructureDefinition.charges_per_rank: u32` (default 0; shrapnel_trap 1 in W4).
- **skills.json:** 8 appended nodes (ids 170-177); `SkillNode.counters: Vec<String>`.
- **world.json:** only `Country.difficulty` changes (W2, regenerated by `scripts/generate_world.py`). stage_themes, strategy_config and sources are carried over verbatim.
- **assets/art/enemy_visuals.json:** schema 2 with `elite_tells`. v1 still loads.
- **assets/audio/SURVIVAL_PROVENANCE.json:** schema 2 in wave 6. The v1 copy is archived.

## 8. settings.json v2 (horde_game Settings; container `#[serde(default)]`; never in the digest)

The 13 existing keys and `bindings: [String; 8]` are unchanged.

| Field | Type | Default | Notes |
|---|---|---|---|
| `version` | u32, **field-level** `#[serde(default)]` | absent = 0; written as `SETTINGS_VERSION` (1 from W1, 2 from W6) | Is the mix version (no `audio_mix_version`) |
| `extra_bindings` | `ExtraBindings { world_clock: String = "P", cycle_attack: String = "Q" }` (container default; a new action is a new field) | P / Q | `""` = Unbound and never fires |
| `window_mode` | `WindowModeSetting { Windowed, BorderlessFullscreen }` | Windowed | Applied live by `render_plan::apply_display_settings` |
| `vsync` | bool | true | Fifo |
| `low_latency` | bool | false | Mailbox (Bevy falls back to Immediate then Fifo); pipelined off; restart required |
| `music` | f32 0..=1 | W1: 0.7 | Label "Music & stingers" |
| `ui` | f32 0..=1 | W1: 0.7 | Label "Interface sounds". If version < 1, `ui = effects`. |
| `mono_audio` | bool | false | Wired in W6 |
| `quality_markers` | `QualityMarkers { ColorOnly, Text, Shape, TextAndShape }` | ColorOnly | none |
| `palette` | `Palette { Standard, ColorBlind }` | Standard | none |
| `seen_callouts` | `Option<BTreeSet<String>>` | None | Seeded at profile load in W3 with every unlocked CALLOUTS id. New game sets Some(empty). Restore defaults KEEPS it. |

- **Load order:**
  1. Per-key salvage (`Settings::decode` gives `{settings, dropped_keys, not_json}`). `profile::read_settings` copies the original to `settings.unreadable.json` BEFORE any write.
  2. If version < 1, set `ui = effects`.
  3. W6 only, per field: if version < 2 and the value is within 1e-6 of `defaults::WAVE1_*`, set the calibrated default (targets 0.8/0.8/0.5/0.7/0.7, retunable in 0.05 steps by the -23 LUFS gate).
  4. Clamp; NaN or null takes the default.
  5. `normalise()` unbinds any extra key equal to a core key.
  6. version = SETTINGS_VERSION.
- **Startup:** main.rs reads settings once, before App::new, and the same SettingsLoad feeds `FrontendParts::load`. The profile is fixed per process.
- **Constants:** default constants live in `settings::defaults` (MASTER, EFFECTS, AMBIENCE, MUSIC, UI, WAVE1_*). audio owns the values.
- **Withdrawn names:** music_volume, ui_volume, audio_mix_version, callouts_seeded, fullscreen, colour_blind_palette, a BTreeMap extra_bindings, `ui: Option<f32>`.

## 9. Derived, not saved (each has a test that adds no key)

- `Attempt::defender()`, `Report::defender()` (mode plus seat).
- `Game::next_defender()` fallback.
- `DefenderDuty` view.
- Recruit service record (run_history plus containments). If run_history is ever compacted, it must keep per-recruit summaries.
- Origin trait (biome of `Recruit.origin`).
- Archetype offer (keyed).
- `RECRUIT_IDENTITIES` table (the stamped appearance *is* saved in the profile).
- Advancement level and cap.
- Regional checkpoints and `FrontPosition` (only `boss_rank` copies are saved).
- Country effective difficulty and the start clamp [0.95, 1.05].
- `operations_available`.
- Recruit and training prices.
- Trap charges at rank (computed at placement; the structure's `charges` is saved as today).
- tree_value.
- Pacing state.
- Sale eligibility, previews and results_loot.
- OutcomeNotices.
- The upgrade metric.
- Music director state.
- BattleLayout, FieldFit, `BattleUi.{observer_arena_rect, occluded_lanes, bar_rows}`.
- RenderPlan.
- Storm tagging (`STORM_THEME_IDS=["depot"]`) and the sky-flash schedule (wall clock).
- Elite tells (asset).
- `Game.settlement_issues` (serde skip; `MilestoneItemLost` is transient).
- The expedition outbox (serde skip).
- Toasts, modals and FramePrimaries.

## 10. Withdrawn names (must not appear in code)

- **Defender:** `Attempt.takeover`/`Takeover`, `Report.defender: DefenderId` default Player, `Report.recruit_id`, `Game.next_defender`, `Defender{Player,Companion,Recruit}`, `DefenderId::from_parts`.
- **Expeditions and recruits:** `ParallelBattle.acknowledged`, `ParallelBattle.drop_ids`, `StrategyState.outcome_notices/next_notice_seq/folded_notices`, `Recruit.service`, `CorpseRecord.archetype_id/elite_affix_id`.
- **Progression:** `Report.grants/GrantReceipt/GrantPolicy`, `Report.progression`, `PresetThreat`.
- **Snapshot and modifiers:** `*_bp` snapshot/modifier encodings (`stage_ramp_bp`, `boss_armor_bonus_bp`, ...), `SkillModifiers.structure_charges`, `ProgressionSnapshot.rules_revision`, `STAGED_ENEMY_CAP_MAX`, `MAX_WAVE_TIERS=64`.
- **Commands and errors:** `AcknowledgeExpedition`, `ExpeditionNotSettled`, `ExpeditionStillRunning`, `DefenderLocked/DefenderBusy/DeploymentInProgress`, `AutoSellQuoteChanged/AutoSellUnavailableInBattle/WalletFull`, `AutoSoldLine.item_id`.
- **Modules and helpers:** `keyed.rs`, `hash::keyed_mix`, `ELITE_ROLL`, `ELITE_PICK_TAG`, `loot::keyed`, `Game::sale_preview`, `StrategySaleQuote`.

## 11. Fixtures

- **`save_v6.json` (W1-01, on untouched HEAD 7bd5de8).** It is generated through `Game::apply` by an #[ignore], env-gated generator that refuses to overwrite and asserts SAVE_VERSION == 6. It is at most 64 KB. Contents:
  - Hard preset, with one earlier Custom RunRecord;
  - player level 12; victories ≥ 10 (milestones {5,10}); npc.awarded 52 with 5 first_clears;
  - recruits: 1 Ready, 1 Guarding, 1 Deployed;
  - 1 unsettled ParallelBattle with ≥ 1 synced drop, and 1 settled with a worker last_result;
  - run_history: 1 main record and 1 worker record;
  - stash: Rare/Epic items, a favourite, a locked item, a preset, and a recruit-equipped item;
  - root last_result: a settled Mara (Npc) report with loot_events;
  - a live **Personal** root attempt at about tick 420, deployed at the current rank (≥ 10), with enemies and loot.

  Pins recorded on the v6 build, each with a perturbation proof:
  - `V6_CONTINUATION_FNV` (600 idle ticks);
  - `V6_DROP_TRACE_FNV` (ticked to ≥ 3 drops; (definition, quality, level, rng.state));
  - `country_level_counts_are_pinned`;
  - the 10k-tuple quality-roll table;
  - the v6 item stat/value table.

  **As cut (annotation, 7 November 2026, W1 review):** the fixture checked in by W1-01 (b82658a) is 120,842 bytes, has player level 33 and 14 run_history records (13 main and 1 worker), and was cut on parent 3d492cd (no `crates/` change from 7bd5de8). No ruling covers these differences. The plan above is unchanged; see VALIDATION.md "save_v6.json as cut".

  The same commit also cuts `recording_vocabulary_v1.json` (41 Command + 2 Input), `recording_v6_equipped_sale.jsonl` (a fresh new game: buy, Equip, SellItem the equipped item, End digest) and `crates/horde_game/tests/fixtures/settings_v0.json` (13 keys, custom bindings, effects 0.45).
- **`save_v7.json` (W1-63, wave-1 close, then frozen; ≤ 96 KB).** It is staged through test-support seams: `stage_takeover`, `stage_review_*` for enemy state, receipts and summaries. It must pass `validate_game`, and every `V7_SAVED_KEYS` key must occur at a non-default value:
  - a live depth-0 Npc attempt with seat Recruit(a), that recruit Defending, its loadout reserved, `reserved_recruit` Some(a), appearance from the recruit identity, drop_ids, drop_tier Some;
  - snapshot: elite_rate_bp > 0, front_ramp ≠ 1, boss_rank Capstone, boss_phases 2, every multiplier ≠ 1, wave_tier 11;
  - enemies: an elite, a stationed spitter mid-windup, a split child, a Capstone boss with `boss_state.entered = 1`;
  - non-empty telemetry maps;
  - `next_defender` Some(Recruit(b));
  - `balance_extras_basis` Hard with preset Custom;
  - auto_sell on (non-default);
  - one live expedition with `pending_auto_sell` and one settled expedition with Recruit seats, `summary.acknowledged = false` and an `auto_sold` receipt;
  - `folded_outcomes > 0`; `containments > 0`;
  - a RunRecord with worker_id None and recruit_id Some, plus grant, milestone, auto-sold, elite and boss fields;
  - `level_grant_paid`, `milestones_claimed`, a report with `progression_rewards` and `boss_rank`;
  - SkillModifiers with class_modifiers, armor_penetration and hazard ≠ 1.

  Later waves use inline JSON or new `save_v7_w2_*.json` / `save_v7_w4_*.json` files, never a re-cut.
- **`V7_CONTINUATION_FNV`** is pinned at the wave-4 close (W4-09), together with the PROGRESSED_GOLDEN/TRACE families.

## 12. Digest and determinism ledger

| Wave | GOLDEN (6) | GAMEPLAY_TRACE (6) | V1-V6 continuation | RULESET | Notes |
|---|---|---|---|---|---|
| 1 | **0 moves** (a format-only move is allowed only if the v7 schema makes it unavoidable, with a reason; none expected because every field skips at its default) | 0 | unchanged | 1 to 2 (W1-20) | `default_and_golden_games_serialize_no_v7_key` |
| 2 | Expected moves, provisional per commit, re-blessed once in W2-49: **F** drop_tier always Some plus drop_ids (W2-06); **T** archetype telemetry (W2-05); **C** 39 items enlarge the tier pool (W2-08); **G** only if the weighted or relative pick changes a GOLDEN item (W2-07/09). A written reason per digest. | 0 expected (rank 0: no elites, no new archetypes; the weighted pick makes exactly one draw). Any move needs a G reason plus smoke and pacing evidence. | unchanged (the legacy `drop_tier None` path is byte-identical; elites and phases come from neutral snapshots) | 3 | V6_DROP_TRACE asserted |
| 3 | 0 | 0 | unchanged | 3 | UI only |
| 4 | **G** balance moves, re-blessed once (W4-08) | **G** | unchanged; a trap charges_per_rank pin move would be re-blessed in W4-08 with its reason (none expected) | 4 | PROGRESSED_GOLDEN (4 runs) and PROGRESSED_TRACE plus V7_CONTINUATION pinned at the close |
| 5-7 | 0 | 0 (PROGRESSED 0/4) | unchanged | 4 | Any core move means stop and ask the user |

**As run (annotation, 7 November 2026, W1 review):** W1-69 (user decision 2026-11-06, enemies overlap) re-blessed GOLDEN 1/6 and GAMEPLAY_TRACE 3/6, class G; V1-V6 continuation, V6_DROP_TRACE_FNV and save_v7.json pins unchanged; see plan §6. The wave-1 row above is unchanged.
