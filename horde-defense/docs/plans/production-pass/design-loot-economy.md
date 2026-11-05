# Loot economy: merged spec (base: design A; grafts from B)

All paths are relative to `horde-defense/`. Two of three judges picked A, so A is the base. Per-decision majority preferences and every listed violation are applied below. I checked these facts against HEAD:

- The drop code at `sim/combat.rs:368-395`.
- The two filters at `campaign/ledger.rs:52-128`. The Campaign filter has no roster check.
- `strategy.rs:196-258`.
- `settle_parallel` at `sim/parallel.rs:201-340`.
- `finish()` at `sim/lifecycle.rs:275-372`. `finish_attempt` is at :306, the RunRecord at :307, the Report at :365.
- The recommender `score` closure and its `+ 1.0` margin at `campaign/loadout.rs:396-429`.
- `QualityCatalog::roll` at `quality.rs:111-136`. `mix` is an inner fn there; tag `0x5155_414C_4954_5901`.
- `Report` has `loot_events` and `loot` but no quality counts (`sim.rs:673-697`). `RunRecord.loot_by_quality` exists (`stats.rs:176`).
- `DefenderProfile.modifiers.weapon_classes` and `.weapon.class` exist (`campaign/loadout.rs:94-121`).
- `strength_adjusted_swing` accepts every dice>=1 combo. `rescale_swing` shifts dice by 0..+2 and adds by -5..+2 over ST 1..20 (`rules.rs:206-238`), so A's melee rows are valid. I removed that risk.

## 1. One sale rule (wave 1)

New file `crates/horde_core/src/campaign/sale.rs`:

```rust
pub enum SaleOrigin { Player, Settlement }          // Settlement skips only InBattle
pub enum SaleBlock {                                 // checked in this order, first match wins
    Unknown, InBattle, Reserved, Protected, Locked, Favorite, NpcOnly,
    EquippedPersonal, EquippedMara, Preset { names: Vec<String> }, RecruitEquipped { name: String },
}
impl SaleBlock { pub fn label(&self) -> String; pub fn resolution(&self) -> SaleResolution }
pub enum SaleResolution { Unlock, Unfavourite, EquipStarter { mode: Mode, slot: Slot }, OpenPreset { index: usize }, OpenRecruit { recruit: u64 }, None }
pub struct SaleRefs { recruit_equipped: BTreeMap<u64 /*item*/, String /*recruit name*/> } // pub(crate) ctor only
pub struct SalePlan { pub ids: Vec<u64>, pub coins: u64, pub kept_on_overflow: u32 }
impl Campaign {
    pub fn sale_block(&self, id: u64, refs: &SaleRefs, origin: SaleOrigin) -> Option<SaleBlock>;
    pub(crate) fn plan_batch(&self, refs: &SaleRefs, origin: SaleOrigin, ids: impl Iterator<Item=u64>, overflow: OverflowPolicy) -> Result<SalePlan, CommandError>;
    pub(crate) fn execute_plan(&mut self, plan: &SalePlan);   // removes ids, coins += plan.coins (already checked)
}
```

- `OverflowPolicy`:
  - `RejectBatch` for player batches: the whole batch fails with `WalletLimit`.
  - `KeepItem` for settlement: a per-item `checked_add`. An item that overflows is skipped and counted in `kept_on_overflow`.
- The quote and the sale share one `SalePlan`, so the quote always equals the coins realised.
- `StrategyState::sale_refs(&self) -> SaleRefs` replaces `excluded_equipment` and `extra_ids` (`strategy.rs:231-258`), which are deleted. Player, Mara, preset and reserved ids are read from `Campaign` inside `sale_block`. Campaign-only tests use `#[cfg(any(test, feature="test-support"))] SaleRefs::empty()`.
- `Game::sale_eligibility(id) -> Result<SaleQuote { coins }, SaleBlock>` is the single public entry point. It replaces `Game::sale_preview`.
- Routing:
  - `Command::SellItem` goes to `Game::sell_item`, now strict. It calls `plan_batch([id], Player, RejectBatch)` and then `execute_plan`.
  - Removed: the unequip-and-replace code at `ledger.rs:78-85`, the `reconcile_loadouts` call at `sim.rs:886` (the function stays for save repair), and the warn-only `affected_*` lists at `ledger.rs:63-68` and `strategy.rs:154-166`.
  - `Campaign::sell_quote(&self, &SaleRefs) -> u64` and `Campaign::sell_unprotected(&mut self, &SaleRefs) -> Result<u64, CommandError>` keep their names and go through `plan_batch`.
  - `sell_unprotected` keeps today's in-battle `Err`, now typed `SaleBlocked(InBattle)`; this fixes A's undocumented Ok(0) change. `sell_quote` returns 0 in battle.
  - `StrategyState::sell_extras_quote` and `sell_extras` become thin wrappers. They keep today's in-battle 0 / `Ok(0)`, so `Command::SellUnprotected` still returns `Amount(0)` in battle.
- New `Command::DeletePreset { index }`, because presets today can only be overwritten (`armory.rs:187-203`). It requires preparation and gives the `Preset` block a real way out.
- npc_only: a legacy unprotected `add_item(0)` is blocked with `NpcOnly`.

## 2. Saved schema (wave 1, digest-neutral)

New file `crates/horde_core/src/campaign/auto_sell.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] #[serde(default)]
pub struct AutoSellRule { pub enabled: bool, pub sell_below: Quality, pub slots: [bool; 4] /*Slot::index: Ranged, Melee, Outfit, Accessory*/, pub keep_upgrades: bool, pub apply_to_npc_runs: bool }
// Default { false, Uncommon, [true;4], true, true }; is_default()
pub const MAX_AUTO_SOLD_LINES: usize = 12;
#[derive(Default, ...)] #[serde(default)]
pub struct AutoSoldReceipt { pub count: u32, pub coins: u64, pub by_quality: KindMap<Quality,u32>, pub lines: Vec<AutoSoldLine>, pub kept_on_overflow: u32 } // is_empty()
pub struct AutoSoldLine { pub definition: ItemDefId, pub level: u32, pub quality: Quality, pub coins: u64 } // no item_id by design
```

- Lines keep the 12 sales with the highest coins (ties go to ascending id). Totals are always exact. Results shows "and N more".
- All saved fields are listed in the saved-fields table. Every one is `serde(default)` plus `skip_serializing_if` default, so the wave-1 commit moves no GOLDEN digest. `SAVE_VERSION` becomes 7 (user decision 8).
- The v6_to_v7 loot entries in `sim/migrations.rs` are owned by save-determinism and ordered with the other remits' entries:
  - Root `last_result` and every `battles[].game.last_result` get `drop_ids` set to `loot_events[].item_id` in event order.
  - `loot_by_quality` comes from the matching `run_history` record (`attempt_id` equal and `worker_id` null) when one exists; otherwise it is counted from `loot_events`.
  - Every other new field relies on its serde default. This is documented in the step's doc comment.
- `Campaign::clone_for_worker` (`campaign.rs:212`) is a struct literal, so the compiler forces `auto_sell: AutoSellRule::default()`. Validation rejects an enabled rule on a worker game. That prevents a double payment through `synced_coins` (`parallel.rs:206-212`).
- `save_v6.json` is produced on the unchanged v6 build first; save-determinism owns that step. `save_v7.json` has non-default values in every loot field.

## 3. Drops (wave 2)

New file `crates/horde_core/src/loot.rs`. Constants stay in code; loot.json is rejected.

```rust
pub const LOOT_DROP_PERCENT: u64 = 3; pub const LOOT_EVERY_NTH_KILL: u32 = 20;   // today's values, now named
pub const DROP_TIER_WEIGHTS: [u64; 4] = [6, 3, 1, 1];   // index = drop_tier - item.tier; tiers above drop_tier weigh 0
pub const TRAINED_CLASS_WEIGHT: u64 = 3;                // Ranged item whose WeaponClass is trained
pub const SLOT_WEIGHTS: [u64; 4] = [1, 1, 1, 1];        // wave-4 tuning lever, neutral now
pub const ELITE_PICK_TAG: u64 = 0x454C_4954_455F_5049;  // "ELITE_PI"
pub const ELITE_QUALITY_FLOOR: Quality = Quality::Uncommon;
pub fn max_drop_tier() -> u8;                           // max tier over droppable content (4)
pub fn keyed(seed: u64, attempt: u64, enemy: u64, tag: u64) -> u64; // quality::mix(seed ^ mix(attempt) ^ mix(enemy) ^ tag)
pub struct DropTable { cumulative: Vec<(usize, u64)>, total: u64 }
impl DropTable { pub fn for_attempt(a: &Attempt) -> Self; pub fn total(&self) -> u64; pub fn pick(&self, roll: u64) -> usize }
pub fn drop_quality(base: Quality, seed: u64, attempt: u64, enemy: u64, rank: u32, elite: bool) -> Quality;
```

- `quality::mix` is lifted from an inner fn to `pub(crate) fn mix`, with identical code.
- Pool: index >= 4, `tier` in `1..=drop_tier`, `!npc_only`. `loot_only` items **are** droppable.
- Trained classes come from the attempt's frozen profile:
  - Personal: `a.profile.modifiers.weapon_classes`.
  - Npc: `{a.profile.weapon.class}`.
- Weight = `DROP_TIER_WEIGHTS[d - t] * SLOT_WEIGHTS[slot] * (TRAINED_CLASS_WEIGHT if trained Ranged else 1)`, walked cumulatively in catalogue index order.
- `Attempt.drop_tier: Option<u8>` is set in `deploy_internal` to `Some(max(level + 1, campaign.unlocked()).clamp(1, max_drop_tier()))`. This fixes the `stage % 5` wrap (`sim.rs:910`) and the mid-run changes from worker `first_clear`. `None` (a v6 in-flight attempt) uses `min(level + 1, max_drop_tier())` through the same weighted table. There is no promise of byte-exact legacy picks.
- New drop site at `combat.rs:368`:

```rust
let normal = e.boss || a.rng.below(100) < LOOT_DROP_PERCENT || a.kills.is_multiple_of(LOOT_EVERY_NTH_KILL); // unchanged short-circuit
let elite = e.elite.is_some();                      // enemy-balance's field
if normal || elite {
    let table = DropTable::for_attempt(&a);
    if table.total() > 0 {                           // same guard as today's !available.is_empty()
        let roll = if normal { a.rng.below(table.total()) }            // exactly one draw, as today
                   else { keyed(self.seed, a.id, e.id, ELITE_PICK_TAG) % table.total() }; // elite-only: no a.rng
        let definition = table.pick(roll);
        let quality = drop_quality(content().items[definition].base_quality, self.seed, a.id, e.id, a.progression.rank, elite);
        ... add_item_with_quality; a.drop_ids.push(item_id); (existing loot/loot_events code unchanged)
```

- This fixes A's determinism violation. The `|` form makes an elite-only drop take an extra `a.rng.below(len)`. Here elite status never changes combat-RNG consumption.
- Elites always drop, as the brief says and EB7 recommends. Bosses keep today's forced drop and get no quality floor.
- GAMEPLAY_TRACE stays put: `below(total)` consumes one `next_u64` exactly as `below(len)` does. The only difference is rejection, below `2^64 mod n`, with p ~ n/2^64.

## 4. Relative quality (b1) with an absolute floor (wave 2)

`QualityCatalog::roll_relative(&self, base: Quality, floor: Option<Quality>, seed, attempt, enemy, rank) -> Quality`:

- It uses the same key and tag as `roll`.
- Each tier's weight is read as a step from Common: Poor -1, Common 0, Uncommon +1, Rare +2, Epic +3, Legendary +4.
- Outcome = `clamp(base + step, Poor, Legendary)`.
- With `floor`, tiers whose outcome is below the floor get weight 0, and the roll is `key % renormalised_total`. If the total is 0, the result is `max(base, floor)`.
- A Common base with no floor equals `roll` exactly.
- `pub fn shift(base, step) -> Quality`.
- `drop_quality` = `roll_relative(base, elite.then_some(ELITE_QUALITY_FLOOR), ...)`.
- `Item.quality` and `LootEvent.quality` stay absolute, so existing items keep their stats and value through `stat_ratio`/`value_ratio` (`quality.rs:147-157`). There is no item migration and no change to the `quality.json` schema (the catalogue is `deny_unknown_fields`).
- Sub-floor zeroing (from B) is used instead of `max()` (A), which would pile about 95% onto Uncommon. B measured, at rank 3: Common base gives Uncommon 80.1% / Rare 17.0% / Epic 2.8%; Uncommon base gives Uncommon 69.7% / Rare 24.2% / Epic 5.2% / Legendary 0.9%.

## 5. Content: 39 `loot_only` ids, indices 29-67 (wave 2)

- New `ItemDefinition.loot_only: bool` (`#[serde(default)]`; `ItemDefinition` is not deny_unknown_fields).
- `ItemDefinition::purchasable()` = `tier > 0 && !loot_only`. It is applied at:
  - `Campaign::buy_price`/`buy_item` (`ledger.rs:35-43`), with the message "This equipment is only recovered as loot"
  - `shop_offer`/`shop_offers` (`view.rs:136,156`)
  - the recommender (`loadout.rs:525-535`)
  - `smoke_buy_gear` (`smoke.rs:203-210`)
  - `fixtures.rs:28,118`
  - `campaign.rs:733`
- The shop stays at 24 rows, so no Quartermaster grouping is needed.
- Content validation: `loot_only` implies `tier` in 1..=4.

**Ranged, tiers 2-4 (15 items)**:
- Each is `quality::apply(items[exemplar].clone(), Some(B_T))` with B_T = Rare (t2), Epic (t3), Legendary (t4). Exemplars: Pistol 21, Smg 22, Shotgun 23, Marksman 25, MachineGun 26.
- Weapon block, magazine, cadence, reload and visual_style are copied from the exemplar.
- sell = round(exemplar.sell × {36, 56, 80}/23); buy = round(exemplar.buy × {120, 220, 340}/70).
- Rarity labels: Rare, Exceptional, Masterwork. The JSON values are the output of `apply` and are pinned by a test.

| Tier | Ids, with Personal damage and NPC dice, sell/buy |
|---|---|
| 2 | 35 duty_pistol 26, 2d+3, 17/60 · 36 patrol_smg 21, 3d+3, 28/94 · 37 riot_shotgun 13, 5d+5, 31/103 · 38 designated_marksman_rifle 108, 7d+9, 44/146 · 39 squad_machine_gun 32, 5d+5, 49/163 |
| 3 | 46 match_pistol 31, 2d+5, 27/110 · 47 suppressed_smg 25, 3d+6, 44/173 · 48 combat_shotgun 16, 5d+10, 49/189 · 49 precision_marksman_rifle 129, 7d+15, 68/267 · 50 general_purpose_machine_gun 39, 5d+10, 75/299 |
| 4 | 57 custom_pistol 38, 2d+8, 38/170 · 58 elite_smg 30, 3d+9, 63/267 · 59 auto_shotgun 19, 5d+16, 70/291 · 60 longshot_marksman_rifle 158, 7d+24, 97/413 · 61 heavy_machine_gun 48, 5d+16, 108/461 |

**Non-ranged (24 items)**:
- Each is a sidegrade of its slot anchor, the lowest-index item of that slot and tier.
- It copies the anchor's base_quality, rarity label and sell/buy.
- It must be Pareto non-dominated against the anchor over its non-zero Personal and NPC stat fields.
- Values below are Personal / NPC.

| Tier | Items |
|---|---|
| t1 (anchors 5/6/7; Uncommon; Melee 20/55, Outfit 18/60, Accessory 18/50) | 29 field_hatchet melee.axe 62 / 2d+1 Cutting · 30 pipe_wrench melee.hammer 54 / 2d+3 Crushing · 31 canvas_coat armor.cloth arm 2 hp 15 st 10 / arm 2 · 32 leather_jacket armor.leather arm 4 hp 5 / arm 4 · 33 ammo_pouch accessory.pouch st 25 wb 0.10 / acc 1 · 34 signal_radio accessory.radio hp 10 wb 0.20 / acc 1 |
| t2 (anchors 9/10/11; Rare; 32/100, 35/130, 28/95) | 40 reinforced_axe melee.axe 78 / 2d+3 Cutting · 41 sledgehammer melee.hammer 66 / 3d+1 Crushing · 42 kevlar_jacket armor.leather arm 4 hp 25 st 10 / arm 4 · 43 plated_vest armor.vest arm 6 hp 8 / arm 6 · 44 medic_satchel accessory.medic hp 30 st 15 / arm 1 · 45 scout_satchel accessory.satchel st 25 wb 0.25 / acc 1 |
| t3 (anchors 13/14/15; Epic; 48/180, 54/210, 45/160) | 51 war_machete melee.machete 88 / 2d+5 Cutting · 52 breaching_axe melee.axe 102 / 3d+0 Cutting · 53 tactical_vest armor.vest arm 7 hp 35 st 10 / arm 6 · 54 ceramic_plate_carrier armor.plate arm 9 hp 15 / arm 8 · 55 trauma_kit accessory.medic hp 45 st 15 / arm 1 · 56 command_radio accessory.radio hp 10 st 10 wb 0.40 / acc 2 |
| t4 (anchors 17/18/19; Legendary; 70/290, 80/350, 65/260) | 62 officer_saber melee.saber 110 / 3d+5 Cutting · 63 demolition_hammer melee.hammer 128 / 3d+3 Crushing · 64 bastion_armor armor.plate arm 12 hp 30 / arm 11 · 65 reactive_vest armor.vest arm 10 hp 55 st 10 / arm 9 · 66 quartermaster_rig accessory.rig hp 50 st 50 / arm 2 · 67 expedition_satchel accessory.satchel hp 30 st 30 wb 0.40 / acc 1 |

- The catalogue goes from 29 to 68 definitions.
- Droppable definitions per drop tier: 18 / 33 / 48 / 63 (today 12 / 16 / 20 / 24).
- Only existing visual styles are used, so `player_model/tests.rs:455` passes unchanged.

## 6. Upgrade judge (wave 2; B's design, adopted by 3/3 judges)

Extract from `campaign/loadout.rs`:
- `pub(crate) fn slot_score(mode: Mode, m: &LoadoutMetrics, slot: Slot) -> f32`, the closure at :396-418, verbatim.
- `pub(crate) fn loadout_metrics(&self, mode, loadout: [u64;4], npc: &Npc) -> LoadoutMetrics`, from the closure inside `compare_item` at :263-341.
- `pub const UPGRADE_MARGIN: f32 = 1.0`.

`recommendation` and `compare_item` delegate to these. Then:

```rust
pub enum UpgradeJudge { Defender(Defender), Loadout { mode: Mode, loadout: [u64;4], npc: Npc }, AnyDefender }
pub fn upgrade_gain(&self, mode, loadout: [u64;4], npc: &Npc, id: u64) -> Option<f32>
// Some(after-before) iff can_use_weapon(mode, def) && slot_score(after) > slot_score(before) + UPGRADE_MARGIN
pub fn is_upgrade_for(&self, id: u64, judge: &UpgradeJudge, strategy: &StrategyState) -> bool
```

- `Defender::Player` maps to (Personal, player.loadout, npc); `Mara` to (Npc, npc.loadout, npc); `Recruit(id)` to (Npc, recruit.npc.loadout, recruit.npc). An unresolvable recruit means everything is kept.
- `Report.defender` is owned by campaign-defenders. Until it lands, the judge is derived from `a.mode`: Personal maps to Player, Npc to Mara.
- `AnyDefender` means Player, Mara when `npc_available`, and every roster recruit.

## 7. Settlement (wave 2)

**Main run, hook H1**, in `finish()`:
- Order: after `self.campaign.finish_attempt()` (`lifecycle.rs:306`) and campaign-defenders' takeover release; before the `RunRecord` (:307) and the `Report` (:365).
- Gate: `!self.worker && campaign.auto_sell.enabled`.
- `let drops = mem::take(&mut a.drop_ids)`.
- Candidates, in ascending id order:
  - still owned;
  - `quality.is_some()`;
  - `quality < sell_below`, compared on absolute quality;
  - `slots[slot]`;
  - `sale_block(.., Settlement)` is None;
  - if `keep_upgrades`, not `is_upgrade_for(Defender)`.
- Then `plan_batch(.., KeepItem)`, `execute_plan`, and build the receipt.
- Stamp `record.auto_sold_count/coins` and `Report.auto_sold`. `Report.drop_ids = drops` (sold ids included). `Report.loot_by_quality = a.loot_quality_counts.clone()`.
- No RNG and no new SimEvent.

**Workers, hook W-a**, in `sim/parallel.rs`:
- At :217, every `Ok(id)` from `add_item_with_quality` is pushed to `battle.pending_auto_sell`.
- At the **last** pending record of this call (:233-260, before `append_run`): `let ids = mem::take(&mut battle.pending_auto_sell)`.
  - If `enabled && apply_to_npc_runs`, sell with `UpgradeJudge::Loadout { Npc, battle.loadout, battle.game.campaign.npc }`.
  - Otherwise drop the list. This keeps rule changes from acting retroactively.
- Stamp the record. Mirror into `battle.game.last_result.auto_sold` when `attempt_id` matches, following the `completion_xp` precedent at :251-254.
- **Final sweep (from B):** immediately before `battle.settled = true` (:334), sell anything left under the same gate. This covers the legacy `!had_pending` path, and the receipt goes only to the worker `last_result`. Validation: settled implies an empty list.
- `Settlement` origin skips only `InBattle`, so expedition loot sells while the main run is deployed. All other guards still apply. This was B's hole; A's fix is kept.
- Exactly-once holds because ids are monotonic (`armory.rs:131-136`), each list is drained once with `mem::take`, and the root `drop_ids` and `pending_auto_sell` are disjoint (validated). Disjointness from `reserved_items` (validated) makes the drain position relative to `release_external` (:325) irrelevant.

**Stash apply:**
- `Command::ApplyAutoSellToStash { expect_count: u32, expect_coins: u64 }`.
- Core recomputes `auto_sell_preview(&rule, Stash)` with Player origin and `AnyDefender`. It ignores `enabled` and never sells quality None.
- Errors, in order: `SaleBlocked(InBattle)`, then `NothingToSell`, then `SaleQuoteChanged { count, coins }` (no mutation), then `WalletLimit` (whole batch).
- Success returns `CommandOutcome::Sold { count, coins }`.
- The confirmation is UI modal state only; nothing is saved.

## 8. Commands and read API

- `SetAutoSell(AutoSellRule)` returns Applied. It is allowed in or out of battle and takes effect at the next settlement.
  - `sell_below` must be in {Common, Uncommon, Rare, Epic, Legendary}; Poor gives `AutoSellRuleInvalid(Quality)`. This is B's range, 2 of 3 judges.
  - The UI shows an amber "sells Rare/Epic finds" warning for thresholds above Uncommon.
- `ApplyAutoSellToStash{..}` and `DeletePreset{index}`.
- New CommandError variants: `SaleBlocked(SaleBlock)`, `AutoSellRuleInvalid(Quality)`, `NothingToSell`, `SaleQuoteChanged{count,coins}`, `WalletLimit`.
- New CommandOutcome: `Sold{count,coins}`.
- All are append-only and covered by the command serde round-trip test (`command.rs:618`).
- Pure reads (no IO, no RNG):
  - `Game::sale_eligibility(id)`
  - `Game::auto_sell_preview(&AutoSellRule, AutoSellScope::{Stash, LastRun}) -> AutoSellPreview{count, coins, by_quality, by_slot:[u32;4], kept_upgrades, blocked, ids}`
  - `Game::results_loot() -> Option<ResultsLoot{attempt_id, total, by_quality, kept: Vec<KeptDrop{item_id, definition, quality, level}>, upgrades: Vec<UpgradeOffer{item_id, slot, gain, command}> (≤1 per slot, UI shows ≤2), auto_sold}>`
  - For worker reports, `kept` and `upgrades` are empty because their ids are worker-local.

## 9. UI (wave 3, render only)

- **Equipment Auto-sell panel** (`ui/armory.rs:569`, split into `ui/armory/auto_sell.rs`):
  - Every toggle dispatches `SetAutoSell`.
  - Threshold chips carry a swatch, the name and a decision-18 shape marker.
  - Labels: "Poor only" / "Common and below (default)" / "Uncommon and below" / "Rare and below" / "Epic and below".
  - "Would sell N now · +C" reads from the preview.
  - The hint "Your recent drops are Rare or better; raise the threshold to keep selling" shows when the LastRun preview is 0 and the stash preview at the next threshold is above 0.
- **Modal**: `ArmoryUi.auto_sell_modal: Option<AutoSellPreview>`.
  - Copy: "Sell N items for C coins? Locked, favourite, equipped, preset, recruit and shop items are kept."
  - Buttons: [Sell N · +C] in amber and [Keep items]. Escape closes it.
  - On a `SaleQuoteChanged` error the modal refreshes.
- **Blocked sale**: show `label()` plus `resolution()` buttons. This replaces the warn-and-sell dialog at `armory.rs:994-1041` and the `{:?}` slot labels at :453/:459/:587.
- **Results loot block** under the outcome strip (`ui/results.rs:116`):
  - "Loot 14 · 2 Rare · 5 Uncommon · 6 Common · 1 Poor".
  - "Auto-sold 7 · +86", with the lines on hover.
  - "Better than equipped: <name> · Ranged +18 · [Equip]". This is a secondary inline control, not one of the three actions; ux-flow UX-18 signs off.
- **History**: auto-sold count and coins columns (`ui/history.rs:192`).
- **Battle**: no sale text on `Screen::Battle` or in the worker observer window (`ui/operations.rs:277`). The battle loot receipt (`ui/battle.rs:308-321`) and the fly-ups stay unchanged; they remain true because main-run items cannot be sold mid-run.
- **Audio** (with audio AUD-12): one AutoSellBatch cue per receipt, keyed by `attempt_id` on Results open or by the run id of the displayed expedition outcome. One cue on `Sold`. The loot-reveal cue plays on Results.

## Decisions (question | chosen | rejected, with reason)

| Question | Chosen | Rejected |
|---|---|---|
| Base design | A (2/3 judges) | B as base: no Settlement origin, so worker auto-sell silently fails during a main battle |
| SellItem strictness | Strict, typed SaleBlock, `DeletePreset` plus resolution shortcuts | Warn-and-replace (contradicts goal 4); `SellItemReplacing` (a 5th sell path) |
| Eligibility plumbing | `sale_block(id, &SaleRefs, SaleOrigin)` plus `plan_batch` shared by quote and sale | Mirroring recruit loadouts into Campaign (drift); deleting `Campaign::sell_quote` (brief says route) |
| In-battle worker sales | `SaleOrigin::Settlement` skips only InBattle | B's single-origin check (blocks every drain during a main run) |
| Elite drop | Forced drop, keyed pick when only the elite term fires | A's `\|` form (extra `a.rng` draw, determinism defect); B's 25% bonus (departs from the brief and EB7 without a user decision; kept as a wave-4 fallback that needs enemy-balance sign-off) |
| Elite floor | Absolute Uncommon via sub-floor zeroing | Relative floor (tier-4 elites always Legendary); `max()` (95% Uncommon pile-up) |
| Boss floor | None; bosses unchanged | B's boss floor (not in the brief; an extra balance move) |
| Shop exposure | `loot_only` (2/3 judges) | All purchasable (moves smoke/pacing in wave 2, a 63-row shop, recommender picks) |
| Report fields | Add `Report.drop_ids` and `Report.loot_by_quality` (2/3 judges) | Searching run_history from Results only (critic gap left open; Equip rows capped by 128 loot_events) |
| Upgrade metric | Extracted recommender `slot_score` plus `UPGRADE_MARGIN` 1.0 (3/3) | A's new per-slot formulas (drift from the recommender); U2 lexicographic; U3 any-field |
| Drop constants | Code constants in `loot.rs` | loot.json catalogue (extra install, validation and override surface the brief does not ask for) |
| Drop weighting | W1 single draw, `[6,3,1,1]`, ×3 trained class, neutral slot weights | W2 two draws (moves the trace); W3 counter pick; B's slot weights (unmeasured with A's table; kept as a lever) |
| drop_tier | `Option<u8>`, frozen at deploy, `max(level+1, unlocked)` | Live `unlocked()` at kill time (changes mid-run on worker first_clear); threat rank; `template+1` (wraps); A's u8 0 sentinel (an Option is clearer and skips natively) |
| Worker hook | W-a at the last pending record, plus a final sweep that sells | W-b (patches an appended record); W-c (sells mid-battle on remap); A's clear-without-sell on the legacy path |
| Receipt bound | 12 lines, highest coins first, exact totals | 32 lines (more than Results can show at 1050x700@1.35) |
| sell_below range | Common..=Legendary, Poor rejected | {Common, Uncommon, Rare} (late game sells nothing under b1) |
| Stash confirmation | UI modal plus a quote-carrying command | Core two-step token (extra state); an unconditional command (can sell more than shown) |
| apply_to_npc_runs scope | Expeditions and operations only | Gating main-run Mara or takeover battles |
| Pacing gate wave | Wave 4 (goal-5 profiles) | Wave 2 (B) |
| GOLDEN cadence | Per-commit provisional re-pins, each with a one-line reason; save-determinism's single wave-2 re-bless records final per-digest reasons | One batch re-bless with red intermediate commits (B) |

## Steps (failing-first)

The steps array has the full list. In summary:
- Wave 1: characterize, then route, then the strict rule and DeletePreset; then the schema, v7 migration and fixtures.
- Wave 2: L1 tracking fields; L2 b1 quality; L3 39 items; L4 weighted table; then upgrade-metric extraction, main settlement, worker settlement, stash apply and previews, the elite forced drop and the property test.
- Wave 3: panel and modal, Results, history, guards.
- Wave 4: pacing profiles, stash-growth measurement and tuning.
- Wave 7: docs.

Test-rule exceptions, labelled in the code:
- `old_campaign_filter_sells_recruit_equipped_item` is a characterization test the brief requires. It passes on old code and is replaced in the fix commit.
- `recommendation_unchanged_after_refactor` is a refactor pin written before the extraction.

## Digest moves

See `digest_moves`. In short:
- Wave 1: no moves.
- Wave 2: four provisional GOLDEN re-pins (format, quality, content, pick). GAMEPLAY_TRACE is never moved by loot.
- Settlement and elites: no moves at rank 0.
- Smoke and pacing must be byte-identical across the whole loot series.

## Risks

1. Strict SellItem breaks GDD.md:395's warned sale. Mitigated by typed reasons, shortcuts, DeletePreset, and four rewritten tests with reasons.
2. With `loot_only`, trained-class tier 2-4 weapons can only be obtained by drops. Mitigated by the ×3 class weight. If progression-tree PT15 objects, flipping specific ids to purchasable is a content change, but it moves smoke and pacing and needs before/after numbers.
3. Forced elite drops at rank 23 give about 45 extra drops per 300-kill run, and the default rule sells only about 5% at tier 4. That means stash growth and save-clone cost (perf-architecture). Measured in wave 4. Levers, in order: the threshold hint, `SLOT_WEIGHTS`, then the 25% keyed bonus (needs enemy-balance).
4. Derived tier-4 NPC Marksman 7d+24 and MG 5d+16 are not a new ceiling (a Legendary-rolled scout marksman already reaches it) but appear more often. Watch NPC TTK in wave 4.
5. Worker sales raise coins mid-battle. The guard tests cover the battle screen and the observer window.
6. Two pending records in one `settle_parallel` call would attribute all sales to the last record. This is not reachable today, because the next stage deploys inside the release branch. Pinned by `chained_operation_gets_one_receipt_per_stage_with_count_le_loot`.
7. Dependency on campaign-defenders (`Report.defender`, takeover release before H1) and on enemy-balance (`Enemy.elite`). Fallbacks are specified.
8. The Results Equip row against the "exactly three actions" rule: ux-flow must classify it as secondary.
9. `tests/item_quality.rs:139-196` and the parallel-settlement quality test encode the absolute roll and the uniform pick. They are updated in L2/L4 with written reasons.

## Numbers to measure

- **Model, pre-implementation:** `C:\Users\LUISME~1\AppData\Local\Temp\claude\4857e52b395b3119\bbde60f0-dda4-4feb-894b-f43f39d19bb2\scratchpad\le_model.py`, rank 0, Pistol+Rifle trained.
  - Value per drop at tiers 1-4: today 22.2 / 20.2 / 18.6 / 16.9; after 24.0 / 33.2 / 49.3 / 57.8.
  - Share sold by the default rule: 43.2 / 17.6 / 5.7 / 5.2%.
  - Mean stat ratio: today 0.987 (t1) and 0.825 (t4); after 1.013-1.047.
  - Tier shares at drop tier 4: 52.8 / 26.4 / 8.8 / 12.0% (t4 / t3 / t2 / t1).
- **Re-measure after L4 with the same script and record in the plan:**
  - drops per run (1 + 0.0785 × kills, so 9-23 in baseline smoke);
  - elite drops per run at ranks 3, 10 and 23;
  - median stash size after 60 and 150 runs, auto-sell profile;
  - auto-sold coins per run in runs 1-6.
- **Wave-4 acceptance:** with the auto-sell profile, 20 seeds, median, tiers 1/2/3 affordable within ±1 run of the same-build default no-sale profile. Wave-0 baseline (`target/production-before/pacing.txt`): runs 3/5/6-7. Predictions: A says tier 1 may arrive one run early; B says 3→2, 5, 7→6.
- **Gates:** `--smoke` and pacing output diffed against `target/production-before/{smoke,pacing}.txt` after each loot commit.

---

## Integration rulings

This section was added on 2026-11-05 when the synthesized spec above was copied into the plan. The spec above is kept verbatim as the design record. **Where this section and the spec disagree, this section wins.** The rulings come from four sources, applied in this precedence order: user decisions (U), orchestrator rulings (R, RL), master-planner reconciliations (P) and the five integration cluster rulings (C, D, E, S, W). The final saved-field table is `schema-v7.md`, the commit order is `waves.md` and the proof for every criterion is `acceptance.md`, all in this folder. The plan itself is `docs/plans/2026-11-05-production-pass.md`; any reference above to `docs/plans/2026-11-04-production-pass.md` means that file.

Each entry gives the binding ruling and then what it supersedes in **this** spec.

### User decisions

#### U1. SAVE_VERSION 7

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** SAVE_VERSION 7 bump approved before wave 1. v6_to_v7 is path-based, idempotent and catalogue-free; v1-v6 saves keep loading; save_v6.json is cut on untouched v6 code (W1-01) and save_v7.json is frozen at the wave-1 close (W1-63).
- **Supersedes in this spec:** Loot sub-step (b) of v6_to_v7 is confirmed in W1-21.

### Orchestrator rulings

#### R4. Legacy attempts and rules_revision

- **Owner / wave:** save-determinism, wave 2, 4.
- **Ruling:** No rules_revision field. Live legacy attempts keep their frozen snapshot fields; content-driven changes (drop pools, weights, trap charges) may affect continued legacy attempts; any continuation pin that moves is re-blessed together with that wave's GOLDEN re-bless with a written reason. Prefer a byte-identical legacy path when cheap (drop_tier None).
- **Supersedes in this spec:** "No promise of byte-exact legacy picks": the drop_tier None path is verbatim today's code (E11).

#### R7. GOLDEN / GAMEPLAY_TRACE re-bless cadence

- **Owner / wave:** save-determinism, wave 2, 4.
- **Ruling:** save-determinism re-blesses once per wave in a dedicated commit with a written reason per digest; expected moves in waves 2 and 4; a wave-1 format-only move is allowed only if the v7 schema makes it unavoidable, with a reason. Implemented by the provisional-pin ledger (P3).
- **Supersedes in this spec:** Re-pinning GOLDEN provisionally inside each loot commit without a ledger.

#### R9. "50 sales produce 1 cue"

- **Owner / wave:** audio, wave 6.
- **Ruling:** One auto-sell or stash-apply batch of 50 items gives 1 cue; manual single sales may each cue.
- **Supersedes in this spec:** CommandOutcome::Sold is the batch signal.

#### R11. Game::sale_eligibility facade

- **Owner / wave:** loot-economy, wave 1.
- **Ruling:** The brief's Game::sale_eligibility exists as the public facade used by every sell path (over sale_eligibility_for / plan_sale).
- **Supersedes in this spec:** Routing only through Campaign::plan_batch / StrategyState::sale_refs without a Game facade; Game::sale_preview and StrategySaleQuote are removed.

### Master-planner reconciliations

#### P2. Auto-sell command handlers in wave 1

- **Owner / wave:** loot-economy, wave 1.
- **Ruling:** DeletePreset, SetAutoSell and ApplyAutoSellToStash land in wave 1 with real handlers (W1-20, W1-23). The upgrade-metric extraction moves to wave 1 (W1-22). Wave 2 adds only the settlement hooks H1 and W-a and results_loot.
- **Supersedes in this spec:** Handlers in wave 2 and the upgrade metric in wave 2.

#### P3. Provisional-pin ledger

- **Owner / wave:** save-determinism, wave 1 (W1-07).
- **Ruling:** determinism.rs holds GOLDEN, TRACE and PROVISIONAL: Option&lt;([u64;6],[u64;6])&gt; plus PROVISIONAL_CAUSES (commit tag, class F/C/G/T, reason). A digest-moving commit sets PROVISIONAL and appends its cause; the wave's SD re-bless folds it and resets it to None; the wave-close critic checks None.
- **Supersedes in this spec:** Per-commit GOLDEN re-pins.

#### P4. RULESET numbering

- **Owner / wave:** save-determinism, wave 1, 2, 4.
- **Ruling:** RULESET_VERSION 1 to 2 at the LE strict-sale commit (W1-20), 3 at the wave-2 re-bless, 4 at the wave-4 re-bless; TRACE_RULESET follows in the same commits.
- **Supersedes in this spec:** Strict-sale commit bumps RULESET (confirmed).

#### P5. Results loot block and Auto-sell panel owner

- **Owner / wave:** loot-economy, wave 3.
- **Ruling:** loot-economy owns the Results loot block and the Equipment Auto-sell panel (W3-20, W3-21). ux-flow signs off on the primary-action rule and layout.
- **Supersedes in this spec:** Confirms LE #12/#13.

#### P7. Pacing harness owners

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** PT owns sim/pacing.rs v2; EB, CD and LE add registry rows and pure evaluators in their own commits; EB owns per-archetype telemetry population.
- **Supersedes in this spec:** PacingProfile.auto_sell lands as a row on PT's API (W2-42, W4-l).

### Cluster: defenders

#### D1. Defender identity types

- **Owner / wave:** campaign-defenders, wave 1.
- **Ruling:** New horde_core defenders.rs: pub enum DefenderId { Player, Mara, Recruit(u64) } (the only public identity; display names from npc.name / survivor name); saved pub enum DefenderSeat { #[default] ModeOwner, Recruit(u64) } serialising like DefenderId::Recruit; resolve(mode, seat); RECRUIT_IDENTITIES: [(&str, Appearance); 12] moved from strategy.rs:309-322 in order; recruit_identity(id); RecruitStatus::label(). Types only, no behaviour change.
- **Supersedes in this spec:** UpgradeJudge::Defender(Defender) becomes Defender(DefenderId).

#### D13. Auto-sell keep_upgrades judge vs takeover ordering

- **Owner / wave:** loot-economy, wave 2.
- **Ruling:** CD's takeover core (D6/D8) lands before LE's H1. H1 runs in finish() after the takeover release and judges with UpgradeJudge::Defender(a.defender()); the a.mode fallback is deleted. Expedition W-a keeps UpgradeJudge::Loadout{Npc, battle.loadout, battle.game.campaign.npc}. Every roster loadout is excluded from candidates.
- **Supersedes in this spec:** "Until Report.defender lands, the judge is derived from a.mode" and unordered wave-2 commits.

### Cluster: economy-progression

#### E2. Grant and milestone settlement writer and finish() order

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** Game::settle_progression_rewards: returns default in workers; pays level_grant(l) for l in (paid.max(1)+1)..=level and unclaimed reached milestones; milestone item pick keyed(seed, rank, 0, MILESTONE), authored fixed quality. Called in finish() at depth 0 in the order first_clear -&gt; stage clear -&gt; outbreak -&gt; finish_attempt -&gt; takeover release -&gt; H1 auto-sell -&gt; settle_progression_rewards -&gt; RunRecord -&gt; Report; and once at the end of settle_parallel when idle at depth 0. Never paid mid-battle or in a clone. Expected GOLDEN move 0; smoke and pacing move with recorded reasons.
- **Supersedes in this spec:** H1 precedes the milestone item (finish_settlement_order_auto_sell_runs_before_milestone_item).

#### E4. Auto-sell command, outcome and error names

- **Owner / wave:** loot-economy, wave 1.
- **Ruling:** SetAutoSell(AutoSellRule), ApplyAutoSellToStash{expect_count, expect_coins}, DeletePreset{index}; CommandOutcome::Sold{count, coins} (Copy); CommandError SaleBlocked(SaleBlock), AutoSellRuleInvalid(Quality), NothingToSell, SaleQuoteChanged{count, coins}, WalletLimit. In-battle stash apply = SaleBlocked(InBattle). Error order InBattle -&gt; NothingToSell -&gt; SaleQuoteChanged (no mutation) -&gt; WalletLimit. The confirmation is UI state only. (P2: handlers also land in wave 1.)
- **Supersedes in this spec:** Wave-2 handlers (P2).

#### E5. Drop tracking names and receipt line shape

- **Owner / wave:** loot-economy, wave 1.
- **Ruling:** Attempt.drop_ids (depth 0, drop_tier Some path only), Attempt.drop_tier, Report.drop_ids, Report.loot_by_quality, ParallelBattle.pending_auto_sell, Report.auto_sold, RunRecord.auto_sold_count/coins. AutoSoldLine{definition, level, quality, coins} with no item_id; MAX_AUTO_SOLD_LINES = 12 (highest coins, ties by ascending id). Receipt validation as schema-v7 §3.4. Exactly-once by W-a plus the final sweep.
- **Supersedes in this spec:** freeze AutoSoldLine.item_id and the "line ids appear in drop_ids" rule.

#### E7. Keyed hash module and tags

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** Only crate::hash: pub(crate) mix (moved byte-identically from quality.rs:112-117), keyed(seed, a, b, tag) = mix(seed ^ mix(a) ^ mix(b) ^ tag), mod tags {QUALITY (unchanged), ELITE, AFFIX, ELITE_PICK, RECRUIT_OFFER, MILESTONE} as big-endian ASCII. Users: elite spawn keyed(seed, a.id, e.id, ELITE) % 10000 &lt; elite_rate_bp (director wave spawns only); affix keyed(.., AFFIX); elite-only drop keyed(.., ELITE_PICK); recruit offer keyed(seed, next_recruit_id, fnv1a(country), RECRUIT_OFFER); milestone keyed(seed, rank, 0, MILESTONE). The presentation scan forbids hash::keyed.
- **Supersedes in this spec:** loot::keyed and ELITE_PICK_TAG 0x454C_4954_455F_5049.

#### E8. Elite drop: draw counts, split children and quality floor

- **Owner / wave:** loot-economy, wave 2.
- **Ruling:** Elite and split_child flags never change how many a.rng draws a kill consumes: rolled = !e.boss && below(100) &lt; 3; normal = e.boss || (!split_child && (rolled || kills % 20 == 0)); elite = resolved.elite.is_some(); if normal || elite pick from DropTable: normal uses a.rng.below(total), elite-only uses keyed ELITE_PICK. Split children take and discard the below(100) draw and never drop. Quality via roll_relative(base, elite.then_some(Uncommon), ..) with sub-floor zeroing; bosses keep their forced drop with no floor. drop_tier Some path only.
- **Supersedes in this spec:** Confirms the A-graft.

#### E10. Game::sale_eligibility facade (R11)

- **Owner / wave:** loot-economy, wave 1.
- **Ruling:** pub fn Game::sale_eligibility(id) -&gt; Result&lt;SaleQuote, SaleBlock&gt; (Player origin) replaces Game::sale_preview and StrategySaleQuote; single implementation sale_eligibility_for(id, origin, &SaleRefs); plan_sale builds SaleRefs once. Every sell path (SellItem strict, SellUnprotected / sell_extras, ApplyAutoSellToStash, auto_sell_preview, H1, W-a) routes through plan_sale -&gt; Campaign::execute_plan. SaleQuote becomes {coins}.
- **Supersedes in this spec:** Leaving Game::sale_eligibility without one internal route; public Campaign-level sell paths that bypass StrategyState.

#### E11. Legacy continuation: byte-identical drop path (R4)

- **Owner / wave:** loot-economy, wave 2.
- **Ruling:** From wave 2 deploy_internal always writes drop_tier = Some(max(level+1, unlocked()).clamp(1, max_drop_tier())), so None means "deployed before v7". When drop_tier is None the drop site runs today's code verbatim (combat.rs:368-395): the uniform pick, the absolute quality roll, no elites, nothing pushed to drop_ids (never auto-sold). The marker is loot-only. V1-V6 pins are expected not to move for loot. V6_DROP_TRACE_FNV is recorded in W1-01 and asserted from W2-06.
- **Supersedes in this spec:** "None uses min(level+1, max) through the weighted table; no byte-exact legacy promise".

#### E14. Old debug recordings under strict SellItem

- **Owner / wave:** loot-economy, wave 1.
- **Ruling:** Recordings with header.version &lt; 7 replay through the decode seam under current rules. A recorded SellItem on an equipped, preset or recruit-equipped item now returns SaleBlocked; replay continues and may diverge. Documented as "pre-v7 recordings replay under v7 sale rules". FORMAT stays 1. Test v6_recording_selling_an_equipped_item_replays_as_a_typed_rejection.
- **Supersedes in this spec:** Critic gap with no written reason or test.

#### E15. H1 judge defender

- **Owner / wave:** loot-economy, wave 2.
- **Ruling:** H1 judges keep_upgrades with UpgradeJudge::Defender(attempt.defender()) (R2 accessor; absent = mode owner). A recruit seat resolves to the roster recruit's loadout and npc. An unresolvable recruit keeps everything. H1 lands after CD's takeover commit.
- **Supersedes in this spec:** §6 "Until Report.defender lands, the judge is derived from a.mode".

### Cluster: settings-audio-tooling

#### S17. Strict SellItem sequence

- **Owner / wave:** loot-economy, wave 1.
- **Ruling:** Three LE commits: (a) characterisation old_campaign_filter_sells_recruit_equipped_item (passes on old code, mutation proof recorded); (b) route every sell path through Game::sale_eligibility (no behaviour change); (c) strict SellItem with typed SaleBlocked, removal of the unequip-and-replace at ledger.rs:78-85 and of reconcile_loadouts at sim.rs:886 (kept for save repair at persistence.rs:264), the characterisation test inverted to strict_sell_rejects_recruit_equipped_item, ui/armory.rs:1024 Sell disabled with SaleBlock::label(), RULESET 1 -&gt; 2 with a doc reason. GOLDEN and TRACE unchanged.
- **Supersedes in this spec:** Leaving the armory button to wave 3.

#### S18. Old debug recordings across v7

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** W1-01 also cuts crates/horde_game/tests/fixtures/recording_v6_equipped_sale.jsonl (fresh game: buy, Equip, SellItem the equipped item, End digest). The SAVE_VERSION 7 commit makes replay call Game::from_payload(initial, header.version) when header.version &lt; SAVE_VERSION. LE's strict commit adds pre_strict_recording_diverges_with_ruleset_warning. VALIDATION.md states that ruleset &lt; 2 recordings are not expected to reproduce across the strict-sale change.
- **Supersedes in this spec:** Divergence test lands in W1-20.

### Cluster: enemies-world-pacing

#### W-1. Pacing harness: one API, one acceptance file, named profiles

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** PT owns sim/pacing.rs v2 and examples/pacing.rs: PacingProfile{id, name, seed, country, aim, spend, max_runs, difficulty, plan: PacingPlan, stop_after}, PacingPlan{equip_drops, sell, defender, farm, train_companions}, by_id, DEFAULT_PROFILE_ID "expert-deu", pacing_seed, pacing_sweep (thread::scope), MilestoneStats (lower median, p25, p75; missing = max_runs+1). EB adds rows lux-easy/-normal/-hard (stop_after Liberation(1), max_runs 20) and start-aut/start-bgd; CD adds "takeover" with PacingDefender::RecruitWhenReady. Goal-5 and goal-6 acceptance use expert-deu, Normal, 20 seeds, 60 runs. One crates/horde_core/tests/pacing_targets.rs, all #[ignore = "release-only"], OnceLock sweep cache; run command in AGENTS.md. Default pacing 60 --runs reproduces pacing.txt byte-for-byte.
- **Supersedes in this spec:** auto_sell profile is a row on this API.

### Rulings owned by this remit

R11, P2, P5, D13, E4, E5, E8, E10, E11, E14, E15, S17
