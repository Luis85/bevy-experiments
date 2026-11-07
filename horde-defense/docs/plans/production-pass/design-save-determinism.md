# Save and determinism spec: SAVE_VERSION 7, merged design

All paths are relative to `horde-defense/`. HEAD is 7bd5de8 (v6).

**Base design.** Design B, which won 2 of 3 judges. The per-decision winners from the judges replace B's choice wherever a majority preferred A, and every listed violation is fixed. The rejected options and their reasons are listed in §12.

## 0. Governing rules

1. **Wave 1 is digest-neutral.** The user's invariant allows GOLDEN and TRACE moves only in waves 2 and 4.
   - Wave 1 therefore moves 0 of 6 GOLDEN (`determinism.rs:33-54`) and 0 of 6 GAMEPLAY_TRACE.
   - `state_digest` hashes the whole payload (`persistence.rs:95-98`). So every new saved field takes `#[serde(default)]` at its legacy-neutral value, plus `skip_serializing_if` at exactly that value.
   - The neutral value equals the type's `Default`. New bonus knobs are integer basis points (`*_bp`) or `*_add` integers. No new f32 multiplier defaults to 1.0.
   - The f32 fields that remain have neutral 0.0 (for example `reach_yards`). They use the bit-exact helper `is_zero_f32`, which tests `to_bits() == 0`, so `-0.0` is still written.
2. **The migration is catalogue-free.** `v6_to_v7` is path-based and idempotent. It touches only `battles[*]`. Catalogue-aware fills stay in `reconcile_saved_game` (the contract at `migrations.rs:1-3`). This design needs no new reconcile fill.
3. **Legacy continuation pins are never re-blessed.** This covers V1-V5, V6 (recorded in wave-1 commit 1) and V7 (pinned at the wave-4 close).
   - Behaviour that ticks a saved encounter may read only two kinds of input:
     - frozen per-encounter state: ProgressionSnapshot, Attempt.balance, Attempt.modifiers, Attempt.profile.modifiers, ResolvedEnemy and Enemy fields. Their defaults reproduce v6 bit-for-bit.
     - catalogue records pinned as frozen: the 11 existing archetype resolutions, the enemy_catalog.json `defaults` mapping of the 5 legacy enemy kinds (boss and event spawns and snapshot-less saved enemies resolve through it), wave tiers 0..=4 in enemy_catalog.json, and the 5 content.json levels.
   - New content enters only through new records and new wave tiers numbered 5 and up.
   - No new `Attempt.rng` draw may occur on a path that is reachable with neutral snapshot values.
   - If a pin moves, the change is wrong. Fix it or stop and ask the user.
4. **Presentation never reads core RNG.** A cargo test scans the horde_game sources to enforce this (§9).

## 1. New modules (wave 1)

- `crates/horde_core/src/schema.rs` provides the skip predicates, all `pub(crate)`:
  - `is_default<T: Default + PartialEq>(&T)`
  - `is_false(&bool)`
  - `is_zero_f32(&f32)` (bit-exact)
- `crates/horde_core/src/keyed.rs` holds the stateless keyed roll, extracted byte-identically from `quality.rs:111-136`:
  - `pub(crate) fn mix(u64) -> u64`
  - `pub(crate) fn roll(seed, a, b, tag) -> u64 = mix(seed ^ mix(a) ^ mix(b) ^ tag)`
  - `pub(crate) mod tags`:
    - `QUALITY = u64::from_be_bytes(*b"QUALITY\x01")`, which equals today's 0x5155_414C_4954_5901
    - `ELITE = from_be_bytes(*b"ELITE\0\0\x01")`
    - `AFFIX = *b"AFFIX\0\0\x01"`
    - `RECRUIT_OFFER = *b"OFFER\0\0\x01"`
  - `quality.rs` calls into this module.
- `crates/horde_core/src/defender.rs` defines the defender types:
  - `pub enum Defender { Player, Companion, Recruit(u64) }` is the public identity. It is used by `Campaign.next_defender`, `Command::SetNextDefender`, `available_defenders` and `defender_power`. The variant is named Companion, not Mara, because it is a stable id. The display name comes from `npc.name`, following the `CampaignFeature::Companion` precedent at `skill_catalog.rs:92`.
  - `pub enum DefenderSeat { #[default] ModeOwner, Recruit(u64) }` is the saved seat, with `fn is_mode_owner(&self) -> bool`.
  - Accessor `defender(mode, seat) -> Defender`:
    - (Personal, ModeOwner) gives Player.
    - (Npc, ModeOwner) gives Companion.
    - (Npc, Recruit(id)) gives Recruit(id).
    - (Personal, Recruit) is rejected by validate_game.

## 2. Saved fields

Every field below follows the rules in §0. The authoritative list is the `saved_fields` output, and the `V7_SAVED_KEYS` registry in `tests/support/mod.rs` lists them all.

**Defender seat (decision 3).**
- `Attempt` (`sim.rs:323`) and `Report` (`sim.rs:674`) each gain this field:

  ```rust
  #[serde(rename = "defender", default, skip_serializing_if = "DefenderSeat::is_mode_owner")]
  pub(crate) seat: DefenderSeat
  ```

  The accessor is `fn defender(&self) -> Defender`.
- The Report literal at `lifecycle.rs:365` copies the seat from the attempt.
- The seat serializes as `{"defender":{"Recruit":3}}`. It is absent for a mode owner, so the 3 Npc GOLDEN runs, which use the Companion, serialize nothing new.
- **Stamping sites.** One helper, `fn stamp_recruit_seat(game: &mut Game, recruit_id: u64)`, is called in two places:
  - in `deploy_survivor`, after the worker deploy at `parallel.rs:117`;
  - after a successful country-operation re-deploy at `parallel.rs:317`. That re-deploy runs on the worker game, which does not know `recruit_id`. Missing this site was violation A1.
  - A takeover deploy in wave 2 stamps the depth-0 attempt.
- **Validation in validate_game.** These rules carry a depth qualifier, which fixes violation A2:
  - Personal + Recruit is invalid.
  - **Depth 0:** a seat of Recruit(id) requires that roster recruit id exists, has status `Defending`, and that `mode == Npc`.
  - **Depth 1:** every `battles[i].game.attempt` and `.last_result` seat must equal `Recruit(battles[i].recruit_id)`. A worker game has an empty roster (`parallel.rs:95-104`, `..Game::default()`), so the depth-0 roster rule never applies there.

**`Game::defender_npc(&self) -> &Npc`.**
- It returns `&campaign.npc` for a worker game (`self.worker`, because `parallel.rs:104` copies the recruit into the worker's `campaign.npc`) or when the seat is ModeOwner.
- At depth 0 with Recruit(id), it returns that roster entry's `npc`.
- It replaces the live reads at `npc.rs:72, 102, 125, 181`, `sim.rs:1106-1107` and `lifecycle.rs:318`.
- The Companion path in the Npc GOLDEN runs and in the v1 live-NPC continuation therefore stays byte-identical.
- No Npc snapshot is stored on Attempt.

**ParallelBattle (`sim.rs:746`).**
- `acknowledged: bool`, skip if false. The migration sets it to true where `settled == true`.
  - Validation: `acknowledged` implies `settled`.
  - New `Command::AcknowledgeExpedition(u64)`, which is idempotent.
  - Pruning at `parallel.rs:143-150` removes the oldest battle that is settled and acknowledged first, then falls back to the oldest settled battle.
- `drop_ids: Vec<u64>`, skip if empty. It holds parent-side ids:
  - Pushed in `settle_parallel` when `add_item_with_quality` returns `Ok(id)` (`parallel.rs:217-228`).
  - Drained by worker auto-sell at `battle.settled = true` (`parallel.rs:334`).
  - Validation: empty when settled, unique, and every id below the parent `next_item_id`.
  - A country operation accumulates drops across its stages and sells at final settlement.

**Attempt.**
- `drop_ids: Vec<u64>`, skip if empty.
  - Pushed at `combat.rs:381-399` on every successful insert, always, whether the auto-sell rule is on or off.
  - Depth 0 only. Worker games push nothing; the parent records ParallelBattle.drop_ids instead.
  - Validation: unique, ascending, each below `next_item_id`, and `len <= a.loot`.
  - There is no fixed cap, which removes B's 4096 never-sold exception.
- The seat, described above.

**Report.**
- The seat, described above.
- `drop_ids: Vec<u64>`, skip if empty. It is moved from Attempt.drop_ids at depth-0 settlement. It feeds the Results "better than equipped / Equip" view and "drops by quality" for every drop, so the view is not limited to the 128 loot_events. Validation: `len <= loot`, each id below `next_item_id`.
- `auto_sold: AutoSoldReceipt`, which takes a container default and skips if default.
  - Fields:
    - `count: u32`
    - `coins: u64`
    - `by_quality: KindMap<Quality, u32>`
    - `kept_on_overflow: u32`
    - `lines: Vec<AutoSoldLine { item_id: u64, definition: ItemDefId, quality: Quality, level: u32, coins: u64 }>`
  - Bound: `MAX_AUTO_SOLD_LINES = 128`.
  - Validation: `lines <= min(count, 128)`; `sum(lines.coins) <= coins`; line ids are no longer in `campaign.items`.
  - For worker runs the parent writes this receipt into `battle.game.last_result`, following the completion_xp precedent at `parallel.rs:253-257`.
- `grants: GrantReceipt`, skip if default.
  - Fields:
    - `level_ups: u32`
    - `level_coins: u64`
    - `milestones: Vec<MilestoneGrant { rank: u32, coins: u64, item_id: Option<u64> }>`
  - Bound: at most 4 milestones, each rank in {5, 10, 15, 20}.

**Campaign (`campaign.rs:31`).**
- `auto_sell: AutoSellRule`, which takes a container default, derives PartialEq and skips if default.
  - Fields:
    - `enabled: bool`, default false
    - `sell_below: Quality`, default Uncommon
    - `slots: [bool; 4]` in Slot order, default all true
    - `keep_upgrades: bool`, default true
    - `apply_to_npc_runs: bool`, default true
  - `slots` keeps the brief's literal `[bool; 4]`. Its order is pinned by `slot_positional_arrays_keep_their_frozen_order` (Slot::ALL at `content.rs:20`).
  - `clone_for_worker` resets it to the default.
- `next_defender: Option<Defender>`, skip if None.
  - None means continue with the defender of the last report, which preserves `continue_journey` at `lifecycle.rs:255`.
  - Validation: a Recruit value must exist in the roster.
  - `clone_for_worker` resets it to None.

**RunRecord (`stats.rs:143`).** It has no field defaults today, so each new field needs its own `#[serde(default, skip_serializing_if)]`. That is mandatory.
- New fields:
  - `auto_sold_count: u32`
  - `auto_sold_coins: u64`
  - `level_grant_coins: u64`
  - `elite_kills: u32`
  - `boss_ttk_ticks: Option<u64>`
- Validation:
  - `auto_sold_count <= loot`
  - `boss_ttk_ticks <= duration_ticks`
  - `worker_id == None && recruit_id == Some(_)` means a main-run takeover. It requires mode Npc and is documented in the `stats.rs` doc comment. This fixes violation A8.
- There is no capstone field. Capstone is derived from country, stage and level_count.

**CombatTelemetry (`sim/telemetry.rs:5`).** It has no container default, so each new field takes a field-level default plus skip.
- `spawned_by_archetype: BTreeMap<String, u32>`
- `kills_by_archetype: BTreeMap<String, u32>`
- `defender_damage_by_archetype: BTreeMap<String, u64>`
- `elites_spawned: u32`
- `elites_killed: u32`
- `boss_phase_ticks: Vec<u64>`, at most 3

**Recruits (`strategy.rs`).**
- `RecruitStatus::Defending` is appended at `strategy.rs:30`.
- `Recruit.service: ServiceRecord { liberated: Vec<String> /* country ids, ordered, unique, <=242 */, kills: u64, containments: u32 }`.
  - It takes a field-level default plus skip, which is mandatory because Recruit has no defaults today.
  - Defenses and victories are read from the existing `npc.attempts` and `npc.victories`.
- The origin trait is derived, not stored: `fn origin_trait(country_id) -> OriginTrait` uses an explicit append-only table keyed by stable region or biome id. The table is pinned by `origin_trait_mapping_is_frozen`, an FNV over all 242 countries.

**Enemy (`sim.rs:94`).**
- `boss_phase: u8`
- `boss_phases: Vec<BossPhaseProfile>`, at most 3, frozen at spawn so that later catalogue edits cannot reach an in-flight boss
- `split_from: Option<u64>`

All three skip at default.

**ResolvedEnemy (`enemy_catalog.rs:49`, `deny_unknown_fields`).** Each field takes a field-level default plus skip. Bounds go in `ResolvedEnemy::validate` (`enemy_catalog.rs:80-110`).
- `reach_yards: f32` (0 means contact; range 0..=15; uses `is_zero_f32`)
- `ranged_armor: i32` (0..=1000)
- `npc_ranged_armor: i32` (0..=100)
- `stagger_immune: bool`
- `death_effect: Option<DeathEffect>`, with variants:
  - `Burst { radius_yards, damage, structure_damage, npc_damage_dice, npc_damage_add, on_contact }`
  - `Split { archetype_id, count 1..=4 }`
- `group_size: Option<u8>` (None means a single enemy; 1..=6)
- `tags: BTreeSet<String>` (at most 8, each a valid id)
- `elite: Option<EliteMark { affix_id: String, name: String }>`

**CorpseRecord (`sim.rs:132`).**
- `elite_affix_id: Option<String>`
- `archetype_id: Option<String>`

Both are skipped when None.

**ProgressionSnapshot (`progression.rs:194`, container default).** The Default impl stays legacy-neutral, with every field 0 or false, and each field skips at default.
- `elite_rate_bp: u16`
- `stage_ramp_bp: u16`
- `ramp_tier_offset: u8`
- `boss_phase_count: u8`
- `boss_armor_bonus_bp: u32`
- `boss_damage_bonus_bp: u32`
- `npc_boss_armor_add: i32`
- `capstone: bool`

The `wave_tier` validation bound at `progression.rs:247-249` rises from 4 to `MAX_WAVE_TIER: u32 = 15`.

**SkillModifiers (`skill_catalog.rs:310`, container default; saved inside Attempt.profile.modifiers).** Each field skips at default.
- `class_modifiers: BTreeMap<WeaponClass, ClassModifiers { damage_bonus_bp: i32, reload_bonus_bp: i32, cadence_bonus_bp: i32, spread_bonus_bp: i32, weak_bonus_bp: i32 }>`
- `armor_penetration_add: i32`
- `hazard_damage_bonus_bp: u32`
- `structure_charges: KindMap<StructureKind, u32>`

**BalancePresetDefinition (`balance.rs:139-146`).** This is catalogue only and is not saved.
- `threat: PresetThreat { elite_rate_pct: u16 /*100 = Normal*/, stage_ramp_bp: u16, boss_phase_count: u8 }` with `#[serde(default)]`.
- At deploy, the frozen `Attempt.balance.preset` resolves to its PresetThreat (Custom uses Normal's) and is folded into the frozen ProgressionSnapshot.
- **BalanceSettings gains no fields.** It is `deny_unknown_fields` and is saved in 3 places plus `Command::SetBalance`.

**Constants.**
- `SAVE_VERSION = 7` (`migrations.rs:8`).
- `RULESET_VERSION` (`persistence.rs:11`) is 1 until wave 2, 2 at the wave-2 behaviour commit that first changes outcomes, and 3 at the wave-4 close.

**Conditional fields.** These are added only if their owner shows a need. Their names and defaults are fixed here, each with a field-level default plus skip:
- `StrategyState.overruns: u32`
- `WorldProgress.selected: Option<String>`, only if SelectCountry cannot reuse `active`

**Explicitly not added.** The reasons are in §12.
- Player.level_rewarded, Campaign.claimed_milestones, Campaign.seen_callouts
- Attempt.defender_npc, Campaign.benched_companion
- Item.origin, Recruit.origin_trait, Npc.service_xp
- Enemy.capstone, Report.capstone, RunRecord.capstone
- BalanceSettings.elite_*, stage_ramp, boss_phase_count
- StrategyState.notified_expedition_through
- Event fields: the saved `Event` at `sim.rs:269` gains nothing, because its Debug output is hashed by TRACE.

## 3. Migration `v6_to_v7` (`migrations.rs`)

Add the arm `6 => v6_to_v7(value)?` at `migrations.rs:15-23`. For each object in `value["battles"]` (a missing array means nothing to do):

1. Require a u64 `recruit_id`, otherwise return `SaveError::Migration`.
2. For `key` in `["attempt", "last_result"]`, where `game[key]` is an object: `entry("defender").or_insert(json!({"Recruit": recruit_id}))`.
3. If `settled == true`: `entry("acknowledged").or_insert(true)`.

The step never touches top-level attempts or reports. A v6 Npc root reads as Companion, and a v6 Personal root reads as Player. The step is idempotent and consults no catalogue.

## 4. Settlement rules (wave 2 behaviour, wave 1 types)

**Auto-sell, exactly once.**
- **Main run.** The sale happens in the same call that releases `campaign.reserved` (lifecycle finish, about `lifecycle.rs:275`). It applies the rule current at that moment to Attempt.drop_ids, writes Report.auto_sold, and moves drop_ids into Report.drop_ids. The wallet is credited with `checked_add`; on overflow the item is kept and counted in `kept_on_overflow`.
- **Workers.** The sale happens at `battle.settled = true`. It sells from ParallelBattle.drop_ids, then clears the list.
- **Why exactly once.** No persisted state exists between selling and recording, and `next_item_id` only grows. A v6 in-flight attempt or expedition starts with empty lists, so nothing from before the upgrade is ever sold.

**Grants, transition-based with no ledger.**
- The level grant is paid where `player.level += 1` (`campaign/progression.rs:694`). The milestone grant is paid in `Campaign::first_clear` (`campaign/world.rs:35-53`) when `before < m <= after`, with `before` measured after `migrate_threat_history()`.
- Both take `GrantPolicy { Depth0, Worker }`. Workers never pay. Worker coins sync by delta (`parallel.rs:206-212`), and the parent's own `first_clear` for a worker victory (`parallel.rs:236/270`) pays once.

**Takeover.**
- `SetNextDefender(Recruit(id))` followed by deploy sets status to Defending, stamps the seat, and reserves `recruit.npc.loadout`.
- Validation:
  - Exactly one recruit is Defending if and only if a depth-0 Recruit seat is live.
  - That recruit's loadout is in `expected_reservations` (`persistence.rs:565-610`).
  - A Defending recruit is neither in `guards` nor in an unsettled battle.
  - `StrategyState::validate` (`strategy.rs:896`) receives the set of defending ids from validate_game.
  - The depth-0 rule that a Deployed recruit has an expedition is unchanged.
- Settlement credits XP and claims to the roster recruit's npc, sets status to Ready, and calls `release_external`.

**Hard preset after upgrade.**
- PresetThreat lives in the catalogue, so a v6 Hard campaign gets the current Hard threat at its next deployment. An in-flight v6 attempt keeps snapshot `elite_rate_bp` 0.
- If wave 4 changes the values of an existing named preset's BalanceSettings (for example Hard's enemy_health), that wave-4 commit adds a narrow reconcile, `Campaign::follow_named_preset`:
  - It runs only when `preset != Custom && settings != for_preset(preset)`.
  - It touches `campaign.balance_settings` only, never the frozen Attempt.balance or RunRecord.balance copies.
  - Its test is `named_preset_follows_the_catalogue_on_load_but_frozen_attempt_balance_does_not`.
- Otherwise no reconcile is added.

## 5. RNG keying

- **Elite.** At spawn, after the enemy id is assigned:

  ```
  snapshot.elite_rate_bp > 0 && enemy.split_from.is_none()
      && keyed::roll(game.seed, a.id, e.id, tags::ELITE) % 10_000 < elite_rate_bp
  ```

  The roll is skipped entirely at rate 0.
- **Elite rate.** 0 below rank 3. Otherwise `min(200 + 65*(rank-3), 1500) * elite_rate_pct / 100`, clamped to 10_000 and frozen at deploy. The values are owned by enemy-balance.
- **Affix.** `keyed::roll(.., tags::AFFIX) % total_eligible_weight` over the append-only affix table. The affix id and name are frozen into `ResolvedEnemy.elite`.
- **Drops.**
  - The drop check becomes `e.boss || e.elite.is_some() || a.rng.below(100) < 3 || kills % 20 == 0`. The elite term short-circuits before the RNG draw.
  - The elite quality floor is `max(keyed quality, Uncommon)`, with no extra draw.
  - The definition pick stays exactly one `a.rng.below(total_weight)` over a cumulative table. Uniform weights reproduce `available[below(len)]` exactly.
- **Split, burst and boss phases.** These draw no RNG. Child ids come from `a.next_id` in the order of the enemies vec, and phases are driven by HP thresholds.
- **Recruit offers.** A pure `&Game` function, `keyed::roll(seed, strategy.next_recruit_id, fnv(country_id), tags::RECRUIT_OFFER)`. It never reads `strategy.rng`.
- **Pacing seeding.** The 20-seed pacing example reseeds per seed: `strategy.rng = Rng::new(seed ^ u64::from_be_bytes(*b"STRATEGY"))` through `Game::staging()` (`sim/fixtures.rs:176-205`). There is no production change, and `set_seed` and `strategy.rs:94` are untouched.

## 6. Commands and errors

- The 41 existing variants are frozen.
  - `crates/horde_core/tests/fixtures/recording_vocabulary_v1.json` is generated in wave-1 commit 1. It holds one JSON sample per command plus 2 Input samples.
  - The test is `every_v1_recorded_command_still_parses_and_reserializes_identically`.
  - Recording FORMAT stays 1.
- New variants are appended in this order:
  1. `SetAutoSell(AutoSellRule)`
  2. `ApplyAutoSellToStash { count: u32, coins: u64 }`
  3. `SetNextDefender(Defender)`
  4. `SelectCountry(String)`
  5. `RecruitAs { country: String, archetype: String }`
  6. `AcknowledgeExpedition(u64)`

  `Recruit(String)` keeps its archetype-0 meaning.
- **Stale-quote guard.** `ApplyAutoSellToStash` recomputes the pure `Game::auto_sell_quote(&rule)`. On a mismatch it returns `AutoSellQuoteChanged { count, coins }` with no mutation, staging on a clone and committing on success like ChooseStart (`command.rs:201-212`). The confirmation modal is UI-only.
- **New CommandError variants:**
  - `DefenderLocked(Defender)`
  - `DefenderBusy(RecruitStatus)`
  - `DeploymentInProgress`
  - `AutoSellQuoteChanged { count: u32, coins: u64 }`
  - `NothingToSell`
  - `AutoSellUnavailableInBattle`
  - `WalletFull`
  - `UnknownCountry`
  - `CountryNotReachable`
  - `ArchetypeNotOffered`
  - `ExpeditionStillRunning`

## 7. Persistence, replay and backup

- **`Game::decode_payload`.** `pub fn Game::decode_payload(value: serde_json::Value, version: u32) -> Result<Game, SaveError>` in `persistence.rs`:
  - It returns `UnsupportedVersion` for 0 or anything above SAVE_VERSION.
  - Otherwise it runs migrate (if older), `from_value`, `reconcile_saved_game` and `validate_game(0)`.
  - `decode_generation` uses it.
- **Replay.** `debug_tools/replay.rs:91` uses `decode_payload` only when `header.version < SAVE_VERSION`. Current-version recordings keep plain `from_value`.
- **Backup decision.** This applies only if perf measures save p99 above 1 ms with 1,000+ items and 3 workers. `commit_generation` (`persistence.rs:148-165`) then skips the full `read_save` when the primary's envelope checksum equals the checksum this process last committed to that path. Otherwise it keeps the full decode. This keeps the rule that `.bak` is only replaced after a validated decode. Test: `backup_decision_matches_full_decode_on_corrupt_and_valid_primaries`.

## 8. Settings v2 (horde_game, `settings.rs:44`; not in any digest)

- `bindings: [String; 8]` is unchanged, and `Action::ALL` stays at 8.
- **New fields** (container default):
  - `version: u32`: missing means 1, and the file is written as 2.
  - `extra_bindings: BTreeMap<String, String>`, keyed by `ExtraAction::id()`, which are stable snake_case ids. Unknown keys round-trip. Missing keys are filled from the defaults, but a default never takes a key the user has bound in `bindings`; that action is left unbound with a notice.
  - `window_mode: WindowModeSetting { Windowed, BorderlessFullscreen }`, default Windowed. Variants are append-only.
  - `vsync: bool`, default true.
  - `low_latency: bool`, default false.
  - `music: f32`. The default value is owned by audio and gated by the -23 LUFS capture at default sliders.
  - `ui: Option<f32>`. At load, None becomes `Some(effects)`, which preserves today's mix (UI cues ride the effects bus at `sfx.rs:146, 208`). After that the two volumes are independent.
  - `quality_markers: QualityMarkers { ColorOnly, Text, Shape, TextAndShape }`, default ColorOnly.
  - `palette: Palette { Standard, ColorBlind }`, default Standard.
  - `seen_callouts: Option<BTreeSet<String>>`, stable feature ids such as `feature:companion` and `blueprint:trap`. None means a pre-pass file; the horde_game profile load then sets `Some(features the loaded campaign already has)` via a new pure core `CampaignFeature::id()`. New game sets `Some(empty)`. There is no core field and no command.
- **Loader.** `Settings::load(path) -> SettingsLoad { settings, warnings: Vec<String>, preserved: Option<PathBuf> }` replaces `settings.rs:78-84`:
  - A full typed parse is tried first.
  - If that fails, the file is parsed as a JSON object and each key is overlaid onto `to_value(Settings::default())`. A key is kept only if the merged object still deserializes.
  - `bindings` is accepted element-wise when its length is 8, and falls back as a whole field otherwise.
  - Volumes are clamped to 0..=1, and NaN becomes the default.
  - A non-JSON file is copied to `settings.json-unreadable-<stamp>.json` before defaults are used.
  - Warnings are logged with `tracing::warn`.
- **Rebinding.** Conflict detection and the swap (`ui/preferences.rs:155-168`) span both tables.
- **Not added by this remit:** sky_flashes and mono_audio. They are vfx and audio scope; if those remits add them, they follow these rules.

## 9. Determinism protocol

**Tooling (wave 1, `crates/horde_core/tests/determinism.rs`):**
- The TRACE test at `determinism.rs:80-101` prints all six `({seed:#x}, Mode::{mode:?}, {hash:#018x}),` lines and collects mismatches, matching the GOLDEN test.
- `outcome_summary(seed, mode)` reports kills, coins, hp, loot, spawned and `rng.state` at ticks 600, 1200 and 1800.
- The ignored test `determinism_anatomy` prints for each run:
  - golden;
  - trace;
  - `keys`: FNV of the sorted JSON-pointer paths with array indices stripped, via the `support::json_key_paths_fnv` helper;
  - `items`: FNV of the definition, quality and level of campaign.items;
  - `outcome`.
- A unit-tested classifier assigns each move a class:
  - **F**, format: only keys moved.
  - **C**, content: items moved, outcome did not.
  - **T**, telemetry: only telemetry and receipt subtrees moved.
  - **G**, gameplay: outcome or TRACE moved.
- `const TRACE_RULESET: u32` with `assert_eq!(TRACE_RULESET, RULESET_VERSION)`. A ruleset bump therefore forces an explicit statement on TRACE.
- The file header carries a re-bless log table with columns: wave | commit | seed/mode | GOLDEN old->new | TRACE old->new | class | cause.

**Rules:**
- One cause per commit. A commit that moves digests re-blesses exactly the digests it explains, in that commit, with the outcome-summary diff in its message, so every commit stays green.
- At each wave close, save-determinism re-runs from clean and writes the signed wave table into `docs/plans/2026-11-04-production-pass.md`.
- Every G-class row attaches pacing (60 runs) and `--smoke`, before and after, against `target/production-before`.
- The legacy pins V1-V7 are never re-blessed.
- Waves 1, 3, 5, 6 and 7 move nothing in core. A core move in those waves means stop and ask the user.

**Extra coverage (wave 4):** `PROGRESSED_GOLDEN` and `PROGRESSED_TRACE`, 4 runs each:
- seeds {1, 0xDEAD_BEEF_2026} × {Personal Hard capstone, Npc recruit takeover};
- rank 12, 3600 ticks;
- built by `support::progressed_determinism_game`, not by the review render fixtures.
- They assert `elites_spawned > 0`, non-empty `boss_phase_ticks`, and at least one new archetype killed.

**Presentation RNG guard.** `crates/horde_game/src/architecture_tests.rs` is a `#[cfg(test)]` scan of `CARGO_MANIFEST_DIR/src`, excluding `debug_tools/`. It forbids `.rng`, `Rng::`, `keyed::`, `catalog().roll(` and `.tick(` outside `runner/`. It includes a self-test against a planted string. Audio and VFX variation is keyed by `SimEventRecord.seq` or a frontend counter.

## 10. Fixtures

**save_v6.json** is written in wave-1 commit 1, on untouched v6 code.
- **Generator.** `crates/horde_core/tests/save_fixtures.rs::write_save_fixture` is `#[ignore]` and gated by the env var `LASTLIGHT_WRITE_SAVE_FIXTURE=<path>` (precedent `audio.rs:546`). It refuses to overwrite an existing file, asserts `SAVE_VERSION == 6` (later `== 7` for the v7 recipe), and drives `Game::apply`. Staging is used only for coins, XP and the strategy seed, and that is documented in the `save_migration.rs` header.
- **Content** (at most 64 KB):
  - seed 0x5AFE_0006;
  - Hard preset;
  - player level at least 3 and victories at least 5;
  - one country liberated;
  - Companion, Recruitment and Parallel owned;
  - 3 recruits:
    - Guarding, with a guard assignment;
    - Ready, released by a settled expedition whose worker last_result is a Victory and whose RunRecord carries recruit_id;
    - Deployed on a live expedition with at least 1 synced drop, which must not settle within the next 600 ticks;
  - one recruit's investments at the old allowance 12+8n;
  - run_history with at least 1 main and at least 1 worker record;
  - a stash with Rare and Epic drops, a favourite, a locked item and one preset;
  - root: a live Mode::Npc Companion attempt at about tick 420, with enemies, at least 1 corpse and at least 1 loot_event.
- **Fallback.** If DeploySurvivor is refused while the main attempt is reserved, the generator deploys the expeditions before the Companion.
- **Root Npc report case.** It is covered by inline migration JSON, because deploy clears last_result (`lifecycle.rs:234`).
- **V6_CONTINUATION_FNV** covers 600 idle ticks, sampled every 50 (12 lines). Each line uses the v5 line format (`save_migration.rs:541-552`) for the root attempt, plus each battle's settled, tick, hp, rng.state and enemy count.
  - It excludes campaign coins, item counts and strategy RNG, because reward and strategy policy may legitimately move in waves 2 and 4.
- **Perturbation proof.** Each characterization pin is run once with FNV+1 and once with one fixture byte flipped. Both must fail, and this is recorded in the commit message.

**save_v7.json** is cut at the wave-1 close, from the same generator with `stage_v7_scenario`, as the schema-freeze file.
- **Staging.** It uses `Game::apply` plus `Game::staging()`. Every V7_SAVED_KEYS key is present at a non-default value and the state validates:
  - a depth-0 Recruit seat with a Defending recruit and its reserved loadout;
  - auto_sell enabled with a non-default rule;
  - next_defender set;
  - one settled, unacknowledged battle with an auto_sold receipt;
  - one live battle with drop_ids;
  - a recruit service record;
  - RunRecord v7 fields;
  - ProgressionSnapshot and SkillModifiers non-default fields;
  - an elite enemy and a boss with phases.
- **Pins.** The file must load, validate and resave byte-identically from then on. Its `V7_CONTINUATION_FNV` is pinned at the wave-4 close and guards waves 5-7.

## 11. Steps

The ordered steps are in the `steps` output.

- **Wave 1:**
  - commit 1, v6 evidence (on untouched v6 code);
  - determinism tooling;
  - schema and keyed helpers plus the freeze pins;
  - SAVE_VERSION 7 and the migration;
  - the remaining fields and validation;
  - settings v2;
  - the presentation RNG guard;
  - the save_v7 freeze and docs.
- **Wave 2:**
  - elites and RNG isolation;
  - fixture behaviour tests;
  - per-cause re-bless commits and RULESET 2.
- **Wave 4:**
  - balance re-bless and RULESET 3;
  - PROGRESSED families and the V7 continuation.
- **Waves 5-7:** zero-move verification and docs.

## 12. Rejected options (summary; full list in `rejected`)

- **Defender as a literal enum on Attempt and Report.** Rejected: it moves GOLDEN in wave 1.
- **B's `recruit_id` naming.** Rejected: it departs from decision 3's `defender` key.
- **Npc snapshot on Attempt.** Rejected.
- **Reusing Deployed for takeover.** Rejected.
- **Item.origin.** Rejected.
- **Drops tracked only while the rule is on.** Rejected: a mid-run toggle would sell part of a run.
- **MAX_DROP_IDS 4096.** Rejected: drops past the cap would never be sold, against the acceptance criterion.
- **BalanceSettings f32 fields and a reconcile on every load.** Rejected.
- **Campaign.seen_callouts and MarkCalloutSeen.** Rejected: UI state does not belong in core.
- **Stored origin_trait.** Rejected.
- **Ledger fields for grants.** Rejected.
- **High-water notice tracking.** Rejected.
- **Envelope-only backup check.** Rejected.
- **Game::seed_new_campaign.** Rejected.
- **A save_v7 cut at wave 4 only.** Rejected.
- **A single lumped re-bless commit per wave.** Rejected.
- **A duplicate check_views.py RNG scan.** Rejected.
- **The KindMap shape for slots.** Rejected.

## 13. Risks

1. **Missing default.** If one `#[serde(default)]` is missing on RunRecord, CombatTelemetry, Recruit, CorpseRecord, StrategyState, WorldProgress or ResolvedEnemy (`deny_unknown_fields`), every save becomes unreadable, and `load_profile` silently starts fresh (`profile.rs:83-92`). Guards: `v6_payload_round_trips_unchanged_through_v7_types` plus the per-pointer table.
2. **Frozen catalogue records.** Wave-2 and wave-4 edits to frozen catalogue records move the V1-V6 continuation pins. The guard is `legacy_catalogue_records_that_tick_saved_encounters_are_frozen` (W2-53, before the first wave-2 catalogue edit W2-08 and the enemy catalogue rows from W2-26). Per Ruling R4 there is no `rules_revision` escape hatch: an edit to a frozen record is fixed, and a content-driven change meant to reach legacy attempts re-blesses the continuation pins it moves in that wave's GOLDEN re-bless with a written reason.
3. **Missed stamping site.** A missed seat-stamping site makes depth-1 validation reject the save, and the generation silently does not commit. Guard: `country_operation_next_stage_attempt_is_still_credited_to_its_recruit`.
4. **Grant double-pay.** Without `GrantPolicy::Worker`, grants are paid twice.
5. **v6 fixture timing.** `save_v6.json` must land before any schema edit. After that, no genuine v6 generation can be produced.
6. **World regeneration.** Regeneration under decision 12 can drift stage counts. Guard: `country_level_counts_are_pinned`.
7. **Pruning loss.** If more than 32 battles have settled unacknowledged, a notice can still be pruned (the cap at `persistence.rs:333`).
8. **Not verified here (read-only):**
   - whether GOLDEN runs drop items within 1800 ticks, which decides the wave-2 C and F moves;
   - whether DeploySurvivor is allowed during a live main attempt;
   - the fixture byte size;
   - whether `add_item_with_quality` returns the new id;
   - exact test counts.

## 14. Numbers to measure

- **GOLDEN and TRACE per wave.** Expected:

  | Wave | GOLDEN | TRACE |
  |---|---|---|
  | 1 | 0/6 | 0/6 |
  | 2 | 6/6 (F, C, T) | 0/6 |
  | 4 | 6/6 (G, only if rank-0 inputs move) | 6/6 (same condition) |
  | 3, 5-7 | 0 | 0 |

- **PROGRESSED:** 0/4 after wave 4.
- **Fixture sizes:** save_v6 at most 64 KB; save_v7 at most 96 KB.
- **Save cost:** p99 with 1,000+ items and 3 workers, measured before the backup-cache decision.
- **Pacing and smoke** against `target/production-before`. Baselines: first victory on run 4, tree exhausted by run 26, Personal stage 1 on attempt 3, Npc on attempt 2. The 20-seed medians use the staged strategy seed.
- **Determinism anatomy:** class per move.

---

## Integration rulings

This section was added on 2026-11-05 when the synthesized spec above was copied into the plan. The spec above is kept verbatim as the design record. **Where this section and the spec disagree, this section wins.** The rulings come from four sources, applied in this precedence order: user decisions (U), orchestrator rulings (R, RL), master-planner reconciliations (P) and the five integration cluster rulings (C, D, E, S, W). The final saved-field table is `schema-v7.md`, the commit order is `waves.md` and the proof for every criterion is `acceptance.md`, all in this folder. The plan itself is `docs/plans/2026-11-05-production-pass.md`; any reference above to `docs/plans/2026-11-04-production-pass.md` means that file.

Each entry gives the binding ruling and then what it supersedes in **this** spec.

### User decisions

#### U1. SAVE_VERSION 7

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** SAVE_VERSION 7 bump approved before wave 1. v6_to_v7 is path-based, idempotent and catalogue-free; v1-v6 saves keep loading; save_v6.json is cut on untouched v6 code (W1-01) and save_v7.json is frozen at the wave-1 close (W1-63).
- **Supersedes in this spec:** Any "stay on v6" fallback or conditional wording; the bump is confirmed.

### Orchestrator rulings

#### R1. Camera formula, owner and construction overlay

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** Base = today's per-geometry clearing_rect for (lane_count, width_yards) including its clamps; uniform zoom z = min(arena.w/base.w, arena.h/base.h) (&gt;= 1), centred; no texel or magnification clamp; 17x30 and every other already-filling field stays pixel-identical; the worker observer window uses the same fit function. Owner vfx-lighting, implemented in WAVE 1 because later captures depend on it. Construction is an overlay panel drawn over the field (no arena rescale), with an INCOMING occlusion chevron when the panel covers spawns; Save/Clear plan stay in the panel.
- **Supersedes in this spec:** None (SD18 already presentation-only).

#### R2. Defender identity

- **Owner / wave:** campaign-defenders, wave 1-2.
- **Ruling:** Serde key "defender" on Attempt and Report; an absent value means the mode owner (Player for Personal, Companion/Mara for Npc), so existing GOLDEN digests do not move for this reason. Mara stays selectable from the Companion node; recruits from the Recruitment node. One status model (RecruitStatus::Defending appended) and one npc source of truth (the roster entry, justified for determinism in D6). Takeover deploy stamps the recruit's appearance identity, not Mara's Female.
- **Supersedes in this spec:** Defender{Player,Companion,Recruit} naming (now DefenderId{Player,Mara,Recruit}); the needs_user item on the Npc default is answered yes.

#### R4. Legacy attempts and rules_revision

- **Owner / wave:** save-determinism, wave 2, 4.
- **Ruling:** No rules_revision field. Live legacy attempts keep their frozen snapshot fields; content-driven changes (drop pools, weights, trap charges) may affect continued legacy attempts; any continuation pin that moves is re-blessed together with that wave's GOLDEN re-bless with a written reason. Prefer a byte-identical legacy path when cheap (drop_tier None).
- **Supersedes in this spec:** ProgressionSnapshot.rules_revision escape hatch and "V1-V6 pins can never be re-blessed"; the needs_user item is answered.

#### R5. heavy_target_damage_multiplier

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** SkillModifiers.heavy_target_damage_multiplier may arrive later as a serde-defaulted field (is_one_f32 skip); it is the only pre-approved post-freeze field.
- **Supersedes in this spec:** The conditional freeze row.

#### R7. GOLDEN / GAMEPLAY_TRACE re-bless cadence

- **Owner / wave:** save-determinism, wave 2, 4.
- **Ruling:** save-determinism re-blesses once per wave in a dedicated commit with a written reason per digest; expected moves in waves 2 and 4; a wave-1 format-only move is allowed only if the v7 schema makes it unavoidable, with a reason. Implemented by the provisional-pin ledger (P3).
- **Supersedes in this spec:** One re-bless commit per cause.

#### R8. Single owners

- **Owner / wave:** progression-tree, wave 1-4.
- **Ruling:** Pacing harness v2 = progression-tree (wave 2; CD and EB add profiles on its API); per-archetype telemetry = enemy-balance; SHA-256 / provenance / orphan helpers and scripts/lib/Native.ps1 = qa-captures (wave 1; audio, vfx and perf reuse them); measure_performance.ps1 = perf-architecture on top of Native.ps1; storm tagging = vfx presentation table only (no world.json change); regional checkpoints are derived (no saved field), presentation lines by CD plus EB's Regional boss rank (HP and reward x1.2) in wave-4 balance, no theme-rule change; save_v7.json is cut at the wave-1 close and frozen, later cases use inline JSON or new fixtures; stress scene raises the staged enemy cap only behind a review/test-only seam (no production save accepts &gt; cap+1), owner perf.
- **Supersedes in this spec:** save_v7.json re-cuts after wave 1.

### Master-planner reconciliations

#### P1. Wave-tier bound

- **Owner / wave:** enemy-balance, wave 1 (bound), 2 (catalogue check).
- **Ruling:** MAX_WAVE_TIER: usize = 15, inclusive, at progression.rs:134 and :247, in the wave-1 schema commit W1-28; every_floor_tier_exists_in_enemy_catalog lands with the content in W2-31. The 64 bound is withdrawn.
- **Supersedes in this spec:** The enemies cluster's MAX_WAVE_TIERS = 64 ruling (owner SD).

#### P2. Auto-sell command handlers in wave 1

- **Owner / wave:** loot-economy, wave 1.
- **Ruling:** DeletePreset, SetAutoSell and ApplyAutoSellToStash land in wave 1 with real handlers (W1-20, W1-23). The upgrade-metric extraction moves to wave 1 (W1-22). Wave 2 adds only the settlement hooks H1 and W-a and results_loot.
- **Supersedes in this spec:** Variant registry placement unchanged; handlers are not stubs.

#### P3. Provisional-pin ledger

- **Owner / wave:** save-determinism, wave 1 (W1-07).
- **Ruling:** determinism.rs holds GOLDEN, TRACE and PROVISIONAL: Option&lt;([u64;6],[u64;6])&gt; plus PROVISIONAL_CAUSES (commit tag, class F/C/G/T, reason). A digest-moving commit sets PROVISIONAL and appends its cause; the wave's SD re-bless folds it and resets it to None; the wave-close critic checks None.
- **Supersedes in this spec:** Separate re-bless commits per cause.

#### P4. RULESET numbering

- **Owner / wave:** save-determinism, wave 1, 2, 4.
- **Ruling:** RULESET_VERSION 1 to 2 at the LE strict-sale commit (W1-20), 3 at the wave-2 re-bless, 4 at the wave-4 re-bless; TRACE_RULESET follows in the same commits.
- **Supersedes in this spec:** RULESET 2 at the wave-2 re-bless and 3 at wave 4.

### Cluster: defenders

#### D1. Defender identity types

- **Owner / wave:** campaign-defenders, wave 1.
- **Ruling:** New horde_core defenders.rs: pub enum DefenderId { Player, Mara, Recruit(u64) } (the only public identity; display names from npc.name / survivor name); saved pub enum DefenderSeat { #[default] ModeOwner, Recruit(u64) } serialising like DefenderId::Recruit; resolve(mode, seat); RECRUIT_IDENTITIES: [(&str, Appearance); 12] moved from strategy.rs:309-322 in order; recruit_identity(id); RecruitStatus::label(). Types only, no behaviour change.
- **Supersedes in this spec:** Defender { Player, Companion, Recruit } (renamed DefenderId::Mara).

#### D2. "defender" key on Attempt and Report (R2)

- **Owner / wave:** campaign-defenders, wave 1.
- **Ruling:** Attempt and Report gain #[serde(rename = "defender", default, skip_serializing_if = "DefenderSeat::is_mode_owner")] pub(crate) seat: DefenderSeat. Absent = mode owner. No GOLDEN or TRACE move. Attempt::defender() / Report::defender() via resolve. The Report literal at lifecycle.rs:365 copies the seat; settle_parallel does not overwrite worker reports. No Report.recruit_id. RunRecord.recruit_id is written Some(id) for a takeover.
- **Supersedes in this spec:** freeze §3.1 Report.defender: DefenderId default Player and the root "Mara" migration write; "settle_parallel overwrites every worker report".

#### D3. v6_to_v7 for defender data

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** For each battles[*] (missing array = nothing): require u64 recruit_id else SaveError::Migration; or_insert "defender": {"Recruit": id} on game.attempt and game.last_result. The root attempt/last_result are never touched. Npc attempts get profile.or_insert("appearance","Female"). No appearance re-stamp for v6 in-flight expeditions (documented). No acknowledged key.
- **Supersedes in this spec:** freeze §4(a) root last_result "Mara" insert; step 3 acknowledged = true.

#### D4. Worker seats and appearance stamping

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** stamp_recruit_defender(attempt, recruit_id) sets seat Recruit(id) and appearance recruit_identity(id).1. Called in deploy_survivor after the worker deploy (parallel.rs:113-117) and after the chained operation stage deploy (parallel.rs:317). Inside a worker, defender_npc still returns the worker's campaign.npc copy.
- **Supersedes in this spec:** Confirms seats; fixes worker last_result deriving as Mara.

#### D5. Takeover status: append RecruitStatus::Defending

- **Owner / wave:** campaign-defenders, wave 1.
- **Ruling:** RecruitStatus { Ready, Guarding, Deployed, Defending }. Deployed = on an expedition (unchanged). Defending = depth-0 main-run defender. reserve_defender(id) (Ready -&gt; Defending) and release_defender(id) (Defending -&gt; Ready); reserve_recruit/release_recruit stay Deployed-only. tick_world skips claim_objectives for Deployed | Defending. Existing Ready-gated mutators reject Defending. horde_game adds a "Defending" label arm.
- **Supersedes in this spec:** freeze "RecruitStatus: no new variant".

#### D6. Battle npc source of truth: the roster entry

- **Owner / wave:** campaign-defenders, wave 2.
- **Ruling:** No Npc snapshot on Attempt (Attempt.takeover withdrawn). The defending recruit's live Npc is strategy.roster[id].npc. defender_npc(&self, seat) / defender_npc_mut take the seat explicitly (advance_tick takes the attempt out of self). record_defender_progress(seat, hits, ticks). Reads at sim.rs:1106-1107, npc.rs:72/102/125/181, writes at sim.rs:1117, npc.rs:272 and lifecycle.rs:318 use a.seat. Justification: one serialised copy, no copy-back at finish, the mode-owner path is byte-identical, other roster writers skip or reject Defending, workers keep isolated copies.
- **Supersedes in this spec:** Game::defender_npc(&self) reading the seat from self.attempt.

#### D7. Seat, Defending, reserved_recruit and next_defender validation

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** validate_game rules 1-8 as in schema-v7 §5 (Personal + Recruit invalid; depth-0 live Recruit seat requires exactly one Defending recruit with that id, reserved Some(Npc), reserved_recruit Some(id), reserved loadout, no battle, no guard; ModeOwner/no attempt means no Defending and reserved_recruit None; worker seats equal Recruit(b.recruit_id); worker games have no reserved_recruit, next_defender or Defending; next_defender must name a roster recruit; RunRecord worker_id None + recruit_id Some requires Npc). Campaign.reserved_recruit: Option&lt;u64&gt;; item_reserved / reserve_external cover Mara's loadout only when reserved_recruit is None.
- **Supersedes in this spec:** freeze takeover validation ("roster recruit is Deployed", equality with Attempt.takeover).

#### D8. Takeover deploy and finish order

- **Owner / wave:** campaign-defenders, wave 2.
- **Ruling:** deploy_internal becomes deploy_as(level, DefenderId, stage); public deploy keeps resolve(mode, ModeOwner). Recruit preflight (UnknownSurvivor, Ready, recruitment, preparation, validate_loadout, weapon access) then commit: reserve_takeover, reserve_defender, profile_for_loadout(Npc, recruit loadout, &recruit.npc), stamp_recruit_defender; rollback like parallel.rs:118-125. finish(): (1) victory credit (commit_victory + credit_npc_victory(defender_npc_mut)), (2) Npc attempts bump moves out of finish_attempt, (3) takeover release, (4) LE H1, (5) RunRecord recruit_id, (6) Report seat.
- **Supersedes in this spec:** Names fixed for "SetNextDefender then deploy sets Defending".

#### D9. next_defender location and commands

- **Owner / wave:** campaign-defenders, wave 2.
- **Ruling:** Campaign.next_defender: Option&lt;DefenderId&gt; (skip None, None in workers); Game gets no field. Game::next_defender() = campaign.next_defender.unwrap_or(last_result.map_or(Player, Report::defender)). Appended SetNextDefender(DefenderId) and BeginDefense{stage} (deploys next_defender, unpauses like Deploy, clears it). Deploy{stage, mode} keeps its recorded meaning and never touches next_defender. ContinueJourney deploys next_defender(); retry/repeat use report.defender(). Dispatching the next defender elsewhere resets it to Player with a note. Mara via Companion, recruits via Recruitment.
- **Supersedes in this spec:** freeze "Deploy{mode:Npc} uses next_defender".

#### D10. Expedition notice persistence and commands

- **Owner / wave:** campaign-defenders, wave 2 (types W1).
- **Ruling:** ParallelBattle.summary: Option&lt;ExpeditionSummary{outcome, stages_cleared, liberated, fronts (&lt;=32), coins, loot_count, acknowledged}&gt; plus StrategyState.folded_outcomes. Written in settle_parallel and finalised at settled = true. Pruning drops summary-None or acknowledged settled battles first; otherwise folds. Game::unacknowledged_outcomes() -&gt; OutcomeNotices. Appended AcknowledgeOutcomes{battles, folded}: idempotent, returns Applied. v6 settled battles are never announced. Types ship in wave 1 (W1-16).
- **Supersedes in this spec:** ParallelBattle.acknowledged migrated true and ExpeditionStillRunning.

#### D11. Defender command errors

- **Owner / wave:** campaign-defenders, wave 1.
- **Ruling:** CommandError appends DefenseInProgress and DefenderUnavailable{defender: DefenderId, blocker: DefenderBlocker{Locked{node_id}, Guarding{country_id}, OnExpedition{battle}, Defending, InvalidLoadout{reason}}}. Unknown recruit = existing UnknownSurvivor. Plain Display. Typed pre-check for AssignGuard, DeploySurvivor, CountryOperation, TrainRecruit, EquipRecruit, BuyRecruitPoint and SetRecruitPolicy on the Defending recruit, with no mutation.
- **Supersedes in this spec:** DefenderLocked(Defender), DefenderBusy(RecruitStatus), DeploymentInProgress.

#### D14. save_v7.json defender content

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** save_v7.json is cut at the wave-1 close and frozen. Defender content is staged through Game::staging().stage_takeover(recruit_id), which must pass validate_game: a live depth-0 Npc attempt with seat Recruit(id) and that recruit Defending; next_defender Some(Recruit(other)); a live and a settled expedition with Recruit seats; the settled summary acknowledged false; folded_outcomes &gt; 0; a RunRecord with worker_id None and recruit_id Some. Wave-2 cases use the frozen file, inline JSON or new save_v7_w2_*.json fixtures.
- **Supersedes in this spec:** freeze "save_v7.json is cut at the wave-2 close".

### Cluster: economy-progression

#### E1. Level grant and rank milestones: ledger design

- **Owner / wave:** save-determinism, wave 1 (schema), 2 (writer).
- **Ruling:** Ledger, not transition: Player.level_grant_paid (skip 0, &lt;= level); Campaign.milestones_claimed (skip empty, &lt;= 64, each &lt;= progression_rank()); Report.progression_rewards (renamed from PT's Report.progression); RunRecord.level_grant_coins / milestone_coins. v6_to_v7 (c) inserts level_grant_paid = level only when level &gt;= 2; (d) milestones_claimed = {r in [5,10,15,20] : r &lt;= v6_rank}, copied into battles. clone_for_worker and the op-chain level sync copy both.
- **Supersedes in this spec:** Transition-based GrantPolicy / GrantReceipt / Report.grants / MilestoneGrant.

#### E2. Grant and milestone settlement writer and finish() order

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** Game::settle_progression_rewards: returns default in workers; pays level_grant(l) for l in (paid.max(1)+1)..=level and unclaimed reached milestones; milestone item pick keyed(seed, rank, 0, MILESTONE), authored fixed quality. Called in finish() at depth 0 in the order first_clear -&gt; stage clear -&gt; outbreak -&gt; finish_attempt -&gt; takeover release -&gt; H1 auto-sell -&gt; settle_progression_rewards -&gt; RunRecord -&gt; Report; and once at the end of settle_parallel when idle at depth 0. Never paid mid-battle or in a clone. Expected GOLDEN move 0; smoke and pacing move with recorded reasons.
- **Supersedes in this spec:** Payment at player.level += 1 and in Campaign::first_clear.

#### E3. Hard extras and Custom semantics

- **Owner / wave:** enemy-balance, wave 1 (schema), 2 (fold).
- **Ruling:** Catalogue BalancePresetDefinition.extras: PresetExtras{elite_rate_pct: u16, front_ramp_pct: u16, boss_phase_bonus: u8}, default Normal {100,100,0}, Easy {50,75,0}, Hard {150,150,1}; BalanceSettings unchanged. Saved Campaign.balance_extras_basis (skip Normal, never Custom, Normal unless preset is Custom); set_balance_settings keeps the requested/previous named preset as the basis on a Custom edit. Accessors balance_extras_preset() / balance_extras(). Worker clone and op-chain sync copy the basis. Wave-2 fold is integer math with literal-1.0 short-circuits.
- **Supersedes in this spec:** PresetThreat{elite_rate_pct, stage_ramp_bp, boss_phase_count} and "Custom uses Normal".

#### E4. Auto-sell command, outcome and error names

- **Owner / wave:** loot-economy, wave 1.
- **Ruling:** SetAutoSell(AutoSellRule), ApplyAutoSellToStash{expect_count, expect_coins}, DeletePreset{index}; CommandOutcome::Sold{count, coins} (Copy); CommandError SaleBlocked(SaleBlock), AutoSellRuleInvalid(Quality), NothingToSell, SaleQuoteChanged{count, coins}, WalletLimit. In-battle stash apply = SaleBlocked(InBattle). Error order InBattle -&gt; NothingToSell -&gt; SaleQuoteChanged (no mutation) -&gt; WalletLimit. The confirmation is UI state only. (P2: handlers also land in wave 1.)
- **Supersedes in this spec:** ApplyAutoSellToStash{count, coins}, AutoSellQuoteChanged, AutoSellUnavailableInBattle, WalletFull.

#### E5. Drop tracking names and receipt line shape

- **Owner / wave:** loot-economy, wave 1.
- **Ruling:** Attempt.drop_ids (depth 0, drop_tier Some path only), Attempt.drop_tier, Report.drop_ids, Report.loot_by_quality, ParallelBattle.pending_auto_sell, Report.auto_sold, RunRecord.auto_sold_count/coins. AutoSoldLine{definition, level, quality, coins} with no item_id; MAX_AUTO_SOLD_LINES = 12 (highest coins, ties by ascending id). Receipt validation as schema-v7 §3.4. Exactly-once by W-a plus the final sweep.
- **Supersedes in this spec:** ParallelBattle.drop_ids and MAX_AUTO_SOLD_LINES 128.

#### E6. Recruit history: containments counter only

- **Owner / wave:** campaign-defenders, wave 2 (schema W1).
- **Ruling:** The only saved field is Recruit.containments (skip 0), incremented where a guarded outbreak becomes Contained. Game::service_record(recruit_id) is derived from run_history (RunRecord.recruit_id) plus containments. If run_history is ever compacted it must keep per-recruit summaries.
- **Supersedes in this spec:** Saved Recruit.service: ServiceRecord{liberated, kills, containments}.

#### E7. Keyed hash module and tags

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** Only crate::hash: pub(crate) mix (moved byte-identically from quality.rs:112-117), keyed(seed, a, b, tag) = mix(seed ^ mix(a) ^ mix(b) ^ tag), mod tags {QUALITY (unchanged), ELITE, AFFIX, ELITE_PICK, RECRUIT_OFFER, MILESTONE} as big-endian ASCII. Users: elite spawn keyed(seed, a.id, e.id, ELITE) % 10000 &lt; elite_rate_bp (director wave spawns only); affix keyed(.., AFFIX); elite-only drop keyed(.., ELITE_PICK); recruit offer keyed(seed, next_recruit_id, fnv1a(country), RECRUIT_OFFER); milestone keyed(seed, rank, 0, MILESTONE). The presentation scan forbids hash::keyed.
- **Supersedes in this spec:** crate::keyed / keyed::roll / keyed.rs.

#### E8. Elite drop: draw counts, split children and quality floor

- **Owner / wave:** loot-economy, wave 2.
- **Ruling:** Elite and split_child flags never change how many a.rng draws a kill consumes: rolled = !e.boss && below(100) &lt; 3; normal = e.boss || (!split_child && (rolled || kills % 20 == 0)); elite = resolved.elite.is_some(); if normal || elite pick from DropTable: normal uses a.rng.below(total), elite-only uses keyed ELITE_PICK. Split children take and discard the below(100) draw and never drop. Quality via roll_relative(base, elite.then_some(Uncommon), ..) with sub-floor zeroing; bosses keep their forced drop with no floor. drop_tier Some path only.
- **Supersedes in this spec:** "Elite term short-circuits BEFORE a.rng.below(100)" and the max(keyed, Uncommon) floor.

#### E9. Wave-tier validation bound

- **Owner / wave:** enemy-balance, wave 2 (superseded by P1: wave 1).
- **Ruling:** MAX_WAVE_TIER: usize = 15 at progression.rs:134 and :247 (wave_tier &gt; MAX_WAVE_TIER rejected); load-time every_floor_tier_exists_in_enemy_catalog (EnemyCatalog::wave_pool wraps silently today). P1 moves the bound into W1-28; the catalogue check stays with the content (W2-31).
- **Supersedes in this spec:** MAX_WAVE_TIER: u32 (the type is usize).

#### E11. Legacy continuation: byte-identical drop path (R4)

- **Owner / wave:** loot-economy, wave 2.
- **Ruling:** From wave 2 deploy_internal always writes drop_tier = Some(max(level+1, unlocked()).clamp(1, max_drop_tier())), so None means "deployed before v7". When drop_tier is None the drop site runs today's code verbatim (combat.rs:368-395): the uniform pick, the absolute quality roll, no elites, nothing pushed to drop_ids (never auto-sold). The marker is loot-only. V1-V6 pins are expected not to move for loot. V6_DROP_TRACE_FNV is recorded in W1-01 and asserted from W2-06.
- **Supersedes in this spec:** freeze "None equals the fallback" and the rules_revision escape hatch.

#### E12. Legacy continuation: trap charges_per_rank

- **Owner / wave:** progression-tree, wave 4 (mechanism W2).
- **Ruling:** No SkillModifiers.structure_charges. StructureDefinition.charges_per_rank (serde default 0; shrapnel_trap 1 in wave 4). Placement (sim.rs:979) sets charges = d.charges + d.charges_per_rank * structure_upgrades[kind], like the durability bonus at sim.rs:967. Live legacy attempts may get the new charges (R4); a pin move would be re-blessed in W4-08 with a reason.
- **Supersedes in this spec:** SkillModifiers.structure_charges: KindMap&lt;StructureKind,u32&gt;.

#### E13. Skip predicates for neutral-1.0 f32 multipliers

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** crate::schema: is_default, is_false, is_zero_f32 (bits, -0.0 written), is_one_f32 (bits), one_f32. Neutral-1.0 f32 fields allowed with skip_serializing_if is_one_f32: ProgressionSnapshot.{front_ramp, boss_armor/health/reward/damage_multiplier}, SkillModifiers.hazard_damage_multiplier, and later heavy_target_damage_multiplier. Writers produce the literal 1.0 at neutral inputs; no tolerance compare; validation rejects non-finite.
- **Supersedes in this spec:** "No new f32 multiplier defaults to 1.0; *_bp/_add integers only" and the *_bp snapshot names.

#### E14. Old debug recordings under strict SellItem

- **Owner / wave:** loot-economy, wave 1.
- **Ruling:** Recordings with header.version &lt; 7 replay through the decode seam under current rules. A recorded SellItem on an equipped, preset or recruit-equipped item now returns SaleBlocked; replay continues and may diverge. Documented as "pre-v7 recordings replay under v7 sale rules". FORMAT stays 1. Test v6_recording_selling_an_equipped_item_replays_as_a_typed_rejection.
- **Supersedes in this spec:** Fixture recording_v6_equipped_sale.jsonl is cut in W1-01.

### Cluster: settings-audio-tooling

#### S1. Settings v2 struct

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** Keep the 13 keys and bindings: [String; 8]. Add #[serde(default)] version: u32 (absent = 0), extra_bindings: ExtraBindings{world_clock "P", cycle_attack "Q"} (struct), window_mode: WindowModeSetting{Windowed, BorderlessFullscreen}, vsync true, low_latency false, music f32 ("Music & stingers"), ui f32 ("Interface sounds"), mono_audio false, quality_markers: QualityMarkers{ColorOnly, Text, Shape, TextAndShape}, palette: Palette{Standard, ColorBlind}, seen_callouts: Option&lt;BTreeSet&lt;String&gt;&gt;. Enums are append-only and safe under per-key salvage. SETTINGS_VERSION 1 from wave 1, 2 from wave 6. Defaults in settings::defaults (audio owns values; wave-1 0.6/0.7/0.15/0.7/0.7). Lands before perf's render_plan.
- **Supersedes in this spec:** BTreeMap extra_bindings, ui: Option&lt;f32&gt;, "missing version means 1", not adding mono_audio.

#### S2. Settings loader, salvage and migration order

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** Pure Settings::decode(bytes) -&gt; SettingsDecode{settings, dropped_keys, not_json} with per-key salvage; IO profile::read_settings(path) -&gt; SettingsLoad{settings, warning, preserved} copies the original to settings.unreadable.json before returning. Order: salvage; version &lt; 1 sets ui = effects; (wave 6) per-field default migration; clamp / non-finite to default; normalise() unbinds extra keys equal to core keys; version = SETTINGS_VERSION. Wave-3 modal text: "Settings could not be read; defaults are in use. The original is kept as settings.unreadable.json."
- **Supersedes in this spec:** settings.json-unreadable-&lt;stamp&gt;.json and Settings::load returning SettingsLoad.

#### S4. seen_callouts seeding, New game and Restore defaults

- **Owner / wave:** ux-flow, wave 3.
- **Ruling:** Feature callout ids are "feature:" + CampaignFeature::id() (core, append-only: construction|companion|recruitment|parallel|operations); others "&lt;area&gt;:&lt;name&gt;". ui::onboarding::seed_seen_callouts sets None to Some(every unlocked CALLOUTS id) once in FrontendParts::new. New game sets Some(empty). Settings::restore_defaults resets preferences but keeps seen_callouts.
- **Supersedes in this spec:** "Features the loaded campaign already has" only.

#### S6. Extra bindings and rebinding

- **Owner / wave:** ux-flow, wave 3.
- **Ruling:** ExtraAction{WorldClock, CycleAttack} with ALL, name() and accessors; one binding_pressed(ctx, binding, held) used by Settings::pressed and pressed_extra; conflict detection and swap span both tables; an unbound ("") extra action never fires.
- **Supersedes in this spec:** "Default never takes a key the user bound; left unbound with a notice" (replaced by normalise() plus the swap).

#### S17. Strict SellItem sequence

- **Owner / wave:** loot-economy, wave 1.
- **Ruling:** Three LE commits: (a) characterisation old_campaign_filter_sells_recruit_equipped_item (passes on old code, mutation proof recorded); (b) route every sell path through Game::sale_eligibility (no behaviour change); (c) strict SellItem with typed SaleBlocked, removal of the unequip-and-replace at ledger.rs:78-85 and of reconcile_loadouts at sim.rs:886 (kept for save repair at persistence.rs:264), the characterisation test inverted to strict_sell_rejects_recruit_equipped_item, ui/armory.rs:1024 Sell disabled with SaleBlock::label(), RULESET 1 -&gt; 2 with a doc reason. GOLDEN and TRACE unchanged.
- **Supersedes in this spec:** "RULESET 2 at the wave-2 re-bless" (now 3).

#### S18. Old debug recordings across v7

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** W1-01 also cuts crates/horde_game/tests/fixtures/recording_v6_equipped_sale.jsonl (fresh game: buy, Equip, SellItem the equipped item, End digest). The SAVE_VERSION 7 commit makes replay call Game::from_payload(initial, header.version) when header.version &lt; SAVE_VERSION. LE's strict commit adds pre_strict_recording_diverges_with_ruleset_warning. VALIDATION.md states that ruleset &lt; 2 recordings are not expected to reproduce across the strict-sale change.
- **Supersedes in this spec:** The name decode_payload (from_payload wins).

### Cluster: enemies-world-pacing

#### W-11. 400-enemy stress seam

- **Owner / wave:** perf-architecture, wave 1.
- **Ruling:** GAMEPLAY_ENEMY_CAP: usize = 224 replaces the literal at progression.rs:238; production validation does not widen. horde_core feature review = []; horde_game review = ["horde_core/review"]. Behind cfg(any(test, feature = "test-support", feature = "review")): STRESS_ENEMY_CAP = 399, sim/fixtures.rs stress_render_fixture() (17 lanes, 30 yd, night stage, MG Npc, structures, 3 fire zones, 1,500 corpses, 1,200 stash items, 150 run records, 3 workers) validated at the authored cap, then Attempt::stage_review_enemy_cap(399): unsaveable by construction. Review screen "stress" rejects LASTLIGHT_PRACTICE and never autosaves. Wave 1 uses tier 4; perf updates it in W2-47.
- **Supersedes in this spec:** freeze "enemy_cap &lt;= 224 becomes &lt;= STAGED_ENEMY_CAP_MAX = 400".

#### W-14. Wave-tier bound (superseded by P1)

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** This cluster set MAX_WAVE_TIERS = 64 with wave_tier &lt; 64. The master planner (P1) withdrew it: the bound is MAX_WAVE_TIER = 15 inclusive in W1-28.
- **Supersedes in this spec:** MAX_WAVE_TIERS = 64 is withdrawn; use MAX_WAVE_TIER = 15.

### Wave-1 close rulings (2026-11-07)

Orchestrator rulings R12-R23, recorded verbatim in plan §2.4. The ones that touch this remit:

#### R12. save_v6.json as cut is accepted

- **Owner / wave:** save-determinism, wave-1 close.
- **Ruling:** save_v6.json deviation (120,842 B, player level 33, 14 run records vs schema-v7 §11's 64 KB / level 12 / 2 records) is accepted: the fixture is frozen evidence and §11's numbers were estimates. Amend schema-v7.md §11 with a note.
- **Supersedes in this spec:** Section 11's size, player level and run-record counts for save_v6.json (64 KB, level 12, 1 main and 1 worker record). The fixture is not re-cut and its pins stay (schema-v7.md §11 note).

#### R13. Review recipes may advance their own Game

- **Owner / wave:** save-determinism and qa-captures, wave-1 close.
- **Ruling:** Review recipes (crates/horde_game/src/ui/review/fixtures.rs, ui/review/fixtures/production.rs, ui/review/matrix.rs; review feature only) may call .tick(; record the exception in design-save-determinism.md Integration rulings and cite it in the architecture_tests.rs allowlist comment.
- **Supersedes in this spec:** Any reading that only the runner may call `.tick(` in horde_game. The `.tick(` rule in `crates/horde_game/src/architecture_tests.rs` allows `runner/` and the three review recipe files and cites this ruling.

#### R15. No ruleset bump for W1-69

- **Owner / wave:** save-determinism, wave 2 (W2-49).
- **Ruling:** RULESET_VERSION is not bumped for W1-69 now; the planned W2-49 bump (2 -> 3) must name both W1-69 (no lane queue) and its own change. Document that pre-W1-69 recordings replay without a ruleset warning until then.
- **Supersedes in this spec:** P4 for wave 1 only: RULESET stays 2 through W1-69, and W2-49's 2 -> 3 bump names both changes. VALIDATION.md "Strict sale and ruleset 2" records the replay behaviour.

#### R16. level_grant_paid shim in wave 2

- **Owner / wave:** progression-tree, wave 2 (W2-52, before the W2-49 re-bless).
- **Ruling:** The level_grant_paid shim (neutral level-ups keep the grant ledger paid; test neutral_level_ups_keep_the_grant_ledger_paid) moves three Personal state digests, so it is scheduled as a new wave-2 commit before the wave-2 re-bless: append a row with the next free W2 id in waves.md (owner progression-tree) and a matching entry in the plan.
- **Supersedes in this spec:** Fixing the level-grant half of the W1 review finding in wave 1 (3df1242 fixed only the milestone half). The three Personal GOLDEN digests move provisionally in W2-52 and are folded by W2-49.

### Rulings owned by this remit

U1, R4, R7, P3, P4, D3, D4, D7, D14, E1, E7, E13, S1, S2, S18, W-14, R12, R13, R15
