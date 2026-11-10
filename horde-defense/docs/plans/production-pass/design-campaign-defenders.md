# campaign-defenders: synthesized spec

**Base:** Design A ("minimal-risk"), the winner on all 3 judges (8.5/6.5, 7.6/6.4, 8.0/6.5).

**Grafts from B,** taken where a per-decision majority preferred B or a judge listed the graft:
- expedition notices saved with acknowledgement (all 3 judges preferred B);
- a takeover-aware reservation using `Campaign.reserved_recruit` (judges 1 and 2);
- `BeginDefense` as the picker's deploy command (judge 3);
- live tactics for the active recruit (judge 3);
- worker identity and appearance (judge 2);
- the `liberated_count` performance rewrite;
- a country-hashed second archetype and an `ArchetypeChoice` stat preview;
- Mara listed as Locked before Companion is owned;
- the presentation-only checkpoint night theme;
- the `!had_pending` legacy-path guard;
- the worker-credit test;
- a saved `Recruit.containments` counter;
- a test that defender_power inputs equal the deployed inputs;
- a wave-7 docs step, a strategy baseline file, and a headless loop that ends with "the recruit defends".

**Rejected from B:** home-ground stat bonus, checkpoint coin grant, one recruit per country, `RecruitStatus::Defending`, `npc_slot`, nested-worker and appearance migrations, `RunRecord.bonus_coins`, `Report.checkpoint`, the `power_reference` catalog block, and the DeploymentPlan mega-extraction.

**Invariants kept:**
- No GOLDEN or GAMEPLAY_TRACE move from this remit in any wave.
- No balance change before wave 4.
- Every new saved key is `#[serde(default, skip_serializing_if = ...)]`.
- The v6→v7 step is a documented no-op for every field of this remit.

## 1. Design

### 1.1 Identity (new `crates/horde_core/src/defenders.rs`, types and pure functions only)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DefenderId { Player, Mara, Recruit(u64) }   // externally tagged: "Player" | "Mara" | {"Recruit":3}
impl DefenderId {
    pub fn mode(self) -> Mode;                 // Player→Personal, else Npc
    pub fn recruit(self) -> Option<u64>;
    pub fn from_mode(mode: Mode) -> Self;      // Personal→Player, Npc→Mara
    pub fn from_parts(mode: Mode, recruit: Option<u64>) -> Self; // (Personal,_)→Player,(Npc,None)→Mara,(Npc,Some)→Recruit
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Takeover { pub recruit_id: u64, pub npc: Npc }
pub enum DefenderRole { Survivor, Companion, Recruit }
pub enum DefenderDuty { Ready, Guarding { country_id: String }, Expedition { battle: u64 }, Defending }
pub enum DefenderBlocker {
    Locked { node_id: String, node_name: String, price: u64 },
    Guarding { country_id: String },
    OnExpedition { battle: u64 },
    Defending,
    InvalidLoadout { reason: String },
    UnknownRecruit,
}
pub enum Archetype { Marksman, Guardian, Scout, Steadfast }   // ALL, label(), creation(), default_policy(), from_creation()
pub enum OriginTrait { MonsoonHardened, HedgerowForager, DustWalker, FrostSteady, HighCountryClimber } // for_biome(&str), label(), blurb(), icon_id()
pub enum PowerBand { Weak, Even, Strong }                      // label()
pub const RECRUIT_IDENTITIES: [(&str, Appearance); 12] = [
    ("Alex Rowan", M), ("Sam Okoro", M), ("Noor Aziz", F), ("Ren Mori", M),
    ("Lena Fischer", F), ("Ari Silva", F), ("Mika Chen", F), ("Imani Cole", F),
    ("Sasha Volk", M), ("Kai Morgan", M), ("Dara Nouri", F), ("Morgan Reyes", M),
];
// The table moves from strategy.rs:309-322, keeping the same order and names; the index is (id−1)%12.
pub const MAX_ADVANCEMENT_LEVEL: u8 = 10;
```

`Archetype::default_policy()` maps:
- Marksman → Marksman;
- Guardian → Guardian;
- Scout → Balanced;
- Steadfast → BossFocus.

This is the strategy.rs:339-344 mapping. `Archetype::creation()` = `NpcCreation::archetype(index)` (progression.rs:199-230).

`OriginTrait::for_biome` maps:

| Biome | Countries | Trait |
|---|---|---|
| Tropical | 121 | Monsoon-hardened |
| Temperate | 62 | Hedgerow forager |
| Arid | 28 | Dust-walker |
| Boreal | 17 | Frost-steady |
| Alpine | 14 | High-country climber |

An unknown biome falls back to Temperate. A test pins that no world.json country hits the fallback. The trait is flavour, text and icon only, with no stat effect.

Labels via `label()`:
- in defenders.rs: Archetype, OriginTrait, PowerBand, DefenderBlocker, DefenderDuty;
- in their own modules: RecruitStatus (strategy.rs) and Policy (content.rs).

`RecruitReadiness.summary` (strategy.rs:128-137) uses `status.label()` instead of `{:?}`.

### 1.2 Saved shapes

All of these are skip-if-default. Section 4 lists them as saved_fields.

- `Attempt.takeover: Option<Takeover>`.
- `Report.recruit_id: Option<u64>`.
- `Attempt::defender()` / `Report::defender()` = `DefenderId::from_parts(mode, recruit)`. Brief decision 3's `defender` is exposed as this accessor; see needs_user.
- `Game.next_defender: Option<DefenderId>`. It lives on `Game`, not `Campaign`, so it never leaks into `clone_for_worker` (campaign.rs:212).
- `Campaign.reserved_recruit: Option<u64>`: Some(id) only while recruit `id` defends the main run, with `reserved == Some(Npc)`.
- `ParallelBattle.summary: Option<ExpeditionSummary>`.
- `Recruit.containments: u32`.
- `RunRecord.recruit_id`: an existing field with a new writer.

`Game::next_defender() -> DefenderId` = `self.next_defender.unwrap_or_else(|| self.last_result.as_ref().map_or(Player, Report::defender))`. None means "the defender of the last report", which is exactly today's continue behaviour (journey.rs:43 reuses `report.mode`).

### 1.3 Takeover runtime (CD1, CD2)

**Deploy.** `deploy_internal(level, mode, stage)` becomes `deploy_as(level, DefenderId, stage)`:
- Public `deploy(level, mode)` and `deploy_country_stage(stage, mode)` keep `DefenderId::from_mode`, so Npc means Mara. Smoke, pacing, GOLDEN (determinism.rs:73) and worker deploys are unchanged.
- New `deploy_country_stage_as(stage, DefenderId)`.

**Shared inputs.** `fn deployment_inputs(&self, defender, level, stage) -> Result<DeploymentInputs { profile, snapshot, max_hp, balance factors }, CommandError>` is a pure `&self` helper. Both `deploy_as` and `defender_power` call it, so the power number cannot drift from what deploys.
- Profile for Recruit(id): `campaign.profile_for_loadout(Mode::Npc, r.npc.loadout, &r.npc)` (loadout.rs:155) with `profile.appearance = RECRUIT_IDENTITIES[(id−1)%12].1`.
- Mara and Player keep their current profiles. Mara is already Female (loadout.rs:222).
- appearance is never read by core rules (verified: only loadout.rs and survivors.rs).

**Preflight for Recruit(id), no mutation:**
1. The recruit exists, else `UnknownSurvivor`.
2. Its status is Ready, else `DefenderUnavailable{Guarding|OnExpedition}`.
3. `recruitment_unlocked`.
4. `campaign.preparation()`.
5. `validate_loadout(r.npc.loadout)`.
6. `weapon_access(Npc, ..)` for every item.

**Commit:**
1. `campaign.reserve_external(r.npc.loadout)?`.
2. `campaign.reserved = Some(Npc)` and `campaign.reserved_recruit = Some(id)`.
3. `strategy.reserve_recruit(id)` (status Deployed).
4. `attempt.takeover = Some(Takeover { recruit_id: id, npc: r.npc.clone() })`.

It rolls back exactly like parallel.rs:118-125.

**Reservation semantics.** `item_reserved` and `reserve_external` (armory.rs:283-305) treat `Some(Npc)` as covering Mara's loadout only when `reserved_recruit.is_none()`. During a takeover only the recruit's items (in `reserved_items`) are reserved, so `sale_eligibility` never reports Mara's idle loadout as deployed. `preparation()` still blocks camp changes through `reserved.is_some()`. `clone_for_worker` sets `reserved_recruit: None`.

**Rules redirection.** With `takeover == None` these behave exactly as today:
- `Game::active_tactics(&Attempt) -> ActiveTactics { policy, prefer_aim, early_reload, allow_risk, recall_percent }` (Copy) replaces the `self.campaign.npc.*` reads at sim.rs:1106-1107 and npc.rs:72, 102, 125, 181.
- `Game::record_defender_progress(&mut Attempt, hits, ticks)` replaces `campaign.record_npc_progress` at sim.rs:1117 and npc.rs:272. With a takeover it does `t.npc.successful_hits += hits; t.npc.combat_ticks += ticks; t.npc.claim_objectives(campaign.allowance())`.

**Live tactics.** `Command::SetRecruitTactics { id, tactics: NpcTactics }`:
- It validates `recall_percent ≤ MAX_RECALL_PERCENT`.
- It applies to a Ready roster recruit, or to `attempt.takeover.npc` when `id` is the live takeover. This gives mid-battle parity with `SetNpcPolicy`, which still always edits Mara.
- Guarding or expedition recruits return `RecruitNotReady`.
- The HUD dispatches by `active_defender().role`.

**Credit (decision 4).** `Campaign::first_clear` (campaign/world.rs:35-54) is split:
- `pub(crate) fn commit_victory(&mut self, level) -> bool` gives the player completion XP (unchanged, every mode, VALIDATION.md:291), defeated_bosses, campaign first_clears, victories + 1, and Mara's claim_objectives.
- `pub(crate) fn credit_npc_victory(&self, npc: &mut Npc, level)` does `npc.victories += 1`, `npc.first_clears.insert(level_id)` and `npc.claim_objectives(self.allowance())`.
- `first_clear(level, mode)` = `commit_victory` + (Npc ⇒ `credit_npc_victory(&mut self.npc)`), so it is byte-identical.
- `finish_attempt(defender: DefenderId)` bumps Mara's `attempts` only for `Mara`, replacing the `reserved == Some(Npc)` check at ledger.rs:129-135. For a takeover, `t.npc.attempts += 1`.
- RunRecord gets `recruit_id: Some(id)` and `defender_name: t.npc.name` (lifecycle.rs:312, 317-326).

**Finish order.** For a takeover, `finish` runs:
1. credit;
2. `release_external(t.npc.loadout)`;
3. `reserved_recruit = None`;
4. `self.release_recruit_checked(id, t.npc)`.

Loot-economy's auto-sell hook runs after the release.

`release_recruit_checked` first runs `campaign.validate_npc(&npc)`. If validation fails, it keeps the roster's pre-deployment snapshot, sets the recruit Ready and pushes `SettlementIssue::RecruitReleaseFailed { recruit_id, reason }`. Expedition settlement also uses it.

**Commands blocked during a takeover.** `DeploySurvivor`, `AssignGuard`, `CountryOperation`, `TrainRecruit`, `EquipRecruit` and `BuyRecruitPoint` on the recruit defending the main run return `DefenderUnavailable { defender, blocker: Defending }`, with the state unchanged. `Game::recruit_duty(id) -> Option<DefenderDuty>` derives Defending from `attempt.takeover`.

### 1.4 Choosing the defender (CD3, CD13, decision 2)

Commands (appended; existing variants keep their shape for dev recordings, debug_tools/recording.rs:40-42):

**`SetNextDefender(DefenderId)`.** Rejections:
- `DefenseInProgress` while an attempt is live ("between defenses only");
- `DefenderUnavailable{Locked}` for Mara without Companion (`npc_available()`, progression.rs:368-370);
- `DefenderUnavailable{Locked}` for a recruit without Recruitment (`recruitment_unlocked`, strategy.rs:268-271);
- `DefenderUnavailable{Guarding|OnExpedition}`;
- `UnknownSurvivor`.

On success it stores `Some(id)`.

**`BeginDefense { stage: usize }`.** It deploys `next_defender()` at `stage` of the active country, with the same unpause as `Deploy` (command.rs:233-236). On success it sets `next_defender = None`. The Briefing primary action uses it (ui/context.rs:278-286 in wave 3).

**`Deploy { stage, mode }`** is frozen: Personal → Player, Npc → Mara. It never reads or writes `next_defender`.

**`continue_journey`** deploys `next_defender()` through `deploy_as` and sets `next_defender = None` on success. Errors are typed. `Game::continue_blocker() -> Option<CommandError>` is pure, so Results can disable Continue and show the picker.

**`retry()`, `start_repeat` and `continue_repeat`** (lifecycle.rs:254-265, 405-443) use `report.defender()`. Repeats still require Mode::Npc, which now includes recruits.

**Dispatching the next defender.** `AssignGuard`, `DeploySurvivor` and `CountryOperation` on the recruit that `next_defender()` resolves to still succeed. In the same call they set `next_defender = Some(Player)` and add a strategy note: "<name> left the next defense; you will defend next."

**Gates.** Mara stays gated by Companion and recruits by Recruitment. The Recruitment node requires Meet Mara. This preserves today's Mara takeover (ui/chrome.rs:31-60).

**`Game::available_defenders() -> Vec<DefenderOption { card: DefenderCard, selectable: Result<(), DefenderBlocker>, selected: bool, power: DefenderPower }>`.** The order is Player, Mara (always listed; `Locked{node: companion_contact}` until it is owned), then recruits by id. `Game::defender_loadout(DefenderId) -> [u64;4]` (loot-economy's keep_upgrades uses `defender_loadout(report.defender())`).

### 1.5 Presentation accessors (critic gap: about 28 branches)

**`Game::defender_card(DefenderId) -> Option<DefenderCard>`**, with fields:
- `id`, `name`;
- `hud_name`: first word, uppercase (ROOK/MARA/ARI);
- `appearance`, `role`;
- `archetype: Option<Archetype>`;
- `origin: Option<OriginInfo { country_id, country_name, origin_trait }>`;
- `mode`, `level: u8` (`Npc::advancement_level`), `level_cap: u8` (`Campaign::advancement_cap_level`).

*As built (W2-12):* `level` and `level_cap` are `u32`, because the player's card carries `player.level` (u32) and `player_level_cap()`; a survivor's u8 values widen. `archetype` is added by W2-20 with the `Archetype` type, and `DefenderOption.power` by W2-21 with `defender_power`. `defender_loadout` returns `Option<[u64; 4]>`, None for a recruit not on the roster (loot-economy keeps everything for an unresolvable recruit).

**Other accessors:**
- `Game::active_defender() -> Option<DefenderCard>`: the live main attempt.
- `Attempt::defender_appearance()` = `profile.appearance`.
- `ParallelBattle::defender()` = `Recruit(recruit_id)`.
- `Game::battle_defender_card(battle_id) -> Option<DefenderCard>`: the observer window uses this, never the worker's own `defender()`, because inside a worker the companion slot is the battle's recruit and derives Mara.

**Worker appearance.** The deploys at parallel.rs:116 and the chained stage deploy (around parallel.rs:317) stamp `worker.attempt.profile.appearance = RECRUIT_IDENTITIES[(recruit_id−1)%12].1`. An in-flight v6 expedition keeps Female until its next stage; this is documented and needs no migration.

**Branches that switch to the accessor in wave 3:**
- art/battlefield/mod.rs:104-108 and art/battlefield/ordnance.rs:90-94 use `a.profile.appearance`, with the Mode branch deleted;
- ui/hud/view.rs:84-91;
- ui/chrome.rs:20-60;
- ui/briefing/view.rs:80-83 (`defender_loadout(next_defender())`);
- ui/notices.rs Withdraw arm (`next_defender().mode()`);
- ui/context.rs:278-286 (BeginDefense);
- ui/operations.rs:46-57 (Plan response);
- ui/history.rs: add a defender filter;
- runner/runtime.rs:390-392: NpcShot keyed on the gear-snapshot weapon class (audio).

**Combat-model branches that stay on Mode:** animation.rs:284, 377; battle_view.rs:514; defender_presentation.rs:100, 122; inspector.rs:65, 122, 193; ui/battle.rs:370; ui/hud/view.rs:96, 150, 275; atmosphere_audio.rs:313. Mara-only pages (ui/character.rs, ui/armory.rs:351, 930) stay Mara-specific.

### 1.6 Recruitment (decisions 5, 15)

**Offer.** `StrategyState::recruit_offer_for(country_id) -> [Archetype; 2]`:
- `first = ALL[(next_recruit_id−1)%4]`, today's rotation;
- `second = ALL[(first_idx + 1 + stable_hash(country_id)%3)%4]`, using `world::stable_hash`, made `pub(crate)`.

There is no RNG draw, and the two are always distinct.

**`Command::RecruitAs { country: String, archetype: Archetype }`:**
1. `recruit_offer(country)`; an Err(b) returns `RecruitUnavailable(b)`.
2. An archetype that is not offered returns `ArchetypeNotOffered { offered }`.
3. Otherwise `strategy.recruit_as(campaign, country, a)`, which sets `npc.creation`, stats and `policy = a.default_policy()`.

`Command::Recruit(String)` = `RecruitAs` with offer[0], identical to today's `(id−1)%4`. One recruit per country is NOT enforced; today's rule is kept.

**`Game::recruit_offer(country_id) -> Result<RecruitOffer, RecruitBlocker>`:**
- `RecruitOffer { country_id, country_name, price: campaign.recruit_price(), name, appearance, origin_trait, archetypes: [ArchetypeChoice { archetype, policy, st, dx, ht, guns, melee }; 2] }`.
- `RecruitBlocker { NeedsNode { id, name, price }, NotLiberated, RosterFull, NeedCoins(u64), DefenseInProgress }`. NeedsNode is found from the catalog node whose effect is `CampaignFeature::Recruitment`.
- `Campaign::recruit_price()` today returns world.json recruit_cost 100; progression-tree owns its scaling.
- runner/apply.rs `confirms_purchase` (:103-116) adds RecruitAs.

*As built (W2-20, `sim/recruitment.rs`, types in `defenders.rs`):* the command and error follow schema-v7 §6 row 8: `Command::RecruitAs { country: String, archetype: String }`, appended after `SelectCountry`, takes the archetype by `Archetype::id()` (the `NpcCreation.archetype` name), and `CommandError::ArchetypeNotOffered(String)` carries the rejected text (an unknown name is not offered either), with `RecruitUnavailable(RecruitBlocker)` beside it. The second archetype is keyed as ruling E7 says, not with `world::stable_hash`: `ALL[(first + 1 + keyed(game.seed, next_recruit_id, fnv1a(country), RECRUIT_OFFER) % 3) % 4]`, so `StrategyState::recruit_offer_for` takes the seed. The blockers are checked in today's order (NeedsNode while the Recruitment feature is missing; NotLiberated for a country that is not cleared or unknown, or below `recruitment_unlock_freed`; RosterFull; DefenseInProgress while `campaign.reserved` is set; NeedCoins), and their labels keep today's texts where one existed. `Command::Recruit` now answers with the same typed errors. `StrategyState::recruit` keeps its string API and recruits the rotation archetype through `recruit_as`. `DefenderCard.archetype` is `Archetype::from_creation(&npc.creation)` for Mara and recruits and None for the player.

### 1.7 Recruit progression (CD5, decision "10 levels by the 6th liberation")

- `Campaign::advancement_cap_level() -> u8 = max(1 + first_clears.len(), min(10, 1 + ceil(3·L/2)))`, with `L = liberated_count()` as a high-water count.
- `Campaign::allowance() = 12 + 8·(cap_level − 1)`.

| L | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---|---|---|---|---|---|---|
| cap level | 1 | 3 | 4 | 6 | 7 | 9 | 10 |
| allowance | 12 | 28 | 36 | 52 | 60 | 76 | 84 |

The value is never below the legacy `12 + 8·first_clears`, so v1-v6 ledgers still pass `validate_npc` (progression.rs:826). At L = 0 it equals the legacy value, so GOLDEN, smoke and the 3 solo pacing profiles cannot move.

`Npc::advancement_level() -> u8` = the smallest n in 1..=10 with `12 + 8(n−1) ≥ awarded`. The UI shows "Lv 4 · cap 7 · awarded/allowance CP".

Performance: rewrite `liberated_count()` (progression.rs:407-422) over `world.cleared ∪ clear_high_water.keys()` through a `OnceLock` country-id index. `allowance()` runs every tick (strategy.rs:592), and the current function scans 242 countries. The result must be identical (test).

### 1.8 SelectCountry and Plan response (CD6)

**`Command::SelectCountry(String)`:**
- returns `DefenseInProgress` if an attempt is live;
- returns `StartNotChosen` if `world.start` is None (world.rs:493-496 would otherwise choose a start);
- returns `CountryLocked { country_id }` if `!can_enter`;
- otherwise calls `campaign.enter_country(id)` and returns `CountryLiberated` if the country is fully completed, else `Applied`.

It never deploys, and `next_attempt_id` is unchanged. `EnterCountry` keeps its legacy behaviour for recordings (it always deploys Personal unpaused, command.rs:214-232), but the UI stops sending it.

**`Game::outbreak_plan(country_id) -> Option<OutbreakPlan>`** with fields:
- `country_id`, `country_name`;
- `stage: count−1`;
- `ticks_left`, `strength`;
- `guards`, `guard_capacity`;
- `eligible_guards: Vec<u64>`;
- `recommended_defender: DefenderId`, the highest-power selectable defender.

"Plan response" = SelectCountry + Briefing at stage count−1 + BeginDefense. `deploy_country_stage` already accepts `count−1` for a liberated country (sim.rs:907), and a victory resolves the outbreak through the existing path (lifecycle.rs:297-301).

**`Game::country_choice_preview(id) -> CountryChoicePreview { stage_count, checkpoints, companion_after_liberation }`.**

*As built (W2-19, `sim/country.rs`):* `CommandError::CountryLocked(String)` follows schema-v7 §6 (an unknown id is locked too), and `SelectCountry` is appended after `AcknowledgeOutcomes`. `outbreak_plan` returns None without an active outbreak; `guards` lists the guarding recruit ids in assignment order, and `eligible_guards` is `assign_guard`'s own verdict (`StrategyState::eligible_guards`), so a full garrison lists nobody. `country_choice_preview` returns None for an unknown id; `companion_after_liberation: bool` is true while Mara is not in camp and liberating the country is what first meets the "Meet Mara" node's `minimum_liberated` (false once it is already met, for a country already counted as liberated, and always for a legacy campaign, which meets Mara after its first defense; W2 review eb93abd, `legacy_campaign_meets_mara_after_its_first_defense_not_a_liberation`). Two fields wait for their inputs: `OutbreakPlan.recommended_defender` is added by W2-21 with `defender_power` (landed, see 1.11), and `CountryChoicePreview.checkpoints` by W4-02 from `Country::regional_checkpoints()` (W2-26, ruling P6).

### 1.9 Expedition settlement and notices (CD7, CD8)

**Summary record.**
- `ParallelBattle.summary: Option<ExpeditionSummary { outcome: Option<Outcome>, stages_cleared: u32, liberated: bool, fronts: Vec<String> /*≤32*/, coins: u64, loot_count: u32, acknowledged: bool }>`.
- It is created and updated in the per-report commit block of `settle_parallel` (parallel.rs:201-284): `stages_cleared += 1` on a victory, `liberated |= ` the `Ok(true)` that `record_clear_in` returns (discarded today at :240 and :273), coins and loot_count.
- It is finalised at the single `battle.settled = true` transition (parallel.rs:334): `outcome = Some(final)` and `fronts = world.fronts_opened_by(country)` ids.
- Because it is saved incrementally, a country operation settled across several calls and a save/load are both handled.

**Notices.**
- `Game::expedition_notices() -> Vec<ExpeditionNotice { battle, recruit_id, recruit_name, country_id, country_name, summary }>` lists the battles that are settled and not acknowledged.
- `Command::AcknowledgeExpedition(u64)`: `UnknownExpedition` if the battle is missing; `ExpeditionNotSettled` if it is unsettled; Applied, idempotent, otherwise.
- v6 settled battles load with `summary None`, are never announced and stay listed in Camp home. Counts for a v6 in-flight battle start at load; this is documented.
- Pruning (parallel.rs:140-146, >32 settled) removes acknowledged settled battles first, oldest first.

**Wording.** `ExpeditionSummary::headline()` and a shared core `world::liberation_line(country_name, fronts)` produce "Belgium liberated · 2 new fronts open on the map." The main ContinueJourney notice uses the same formatter (ui/notices.rs:268-276).

**Loot hand-off.** Settlement keeps the main-stash ids returned by `add_item_with_quality` (armory.rs:119-124; discarded at parallel.rs:217) in a per-call, unsaved `Vec<u64>` handed to loot-economy's auto-sell hook after `release_recruit_checked`.

**Idempotence.** The `!had_pending` legacy path (parallel.rs:268-284) commits only if `run_history` has no record with `worker_id == Some(battle.id) && attempt_id == report.attempt_id`.

**Worker credit.** Worker wins already credit the recruit inside the worker game and `RunRecord.recruit_id` (parallel.rs:254). This is now pinned by a test.

### 1.10 Service history (derived) and containments

**`Game::service_record(recruit_id) -> Result<ServiceRecord, CommandError>`.** The counts come from `run_history` filtered by `recruit_id`:
- deployments;
- takeovers (worker_id None);
- expeditions (distinct worker_id);
- victories, defeats, withdrawals;
- kills, coins;
- stages_cleared;
- liberations (a victory where `stage+1 == level_count`);
- countries;
- last outcome.

Other sources:
- the recruit's `npc.attempts` and `victories`;
- the live GuardAssignment (country, duty_ticks);
- `Recruit.containments`, a saved counter incremented at the guard-containment site (strategy.rs:776). Without it, containments are lost when the GuardAssignment is removed.

Records before v7 are labelled "since records began". If perf-architecture compacts `run_history`, it must keep per-recruit summaries.

*As built (W2-21, `sim/defense.rs`, types in `defenders.rs`):* `ServiceRecord { recruit_id, deployments, takeovers, expeditions, victories, defeats, withdrawals, kills, coins, stages_cleared, liberations, countries, last_outcome, npc_attempts, npc_victories, guard: Option<GuardDuty { country_id, duty_ticks }>, containments }`, `UnknownSurvivor` for an id not on the roster. `stages_cleared` counts distinct (country, stage) victories, `liberations` every victory on a country's final stage, and `countries` lists the countries fought in, in first-fought order. `Recruit.containments` is credited at both places a guarded outbreak becomes Contained: the guard whose hit defeats the last group (strategy.rs:834-838 at HEAD; the design above cites the pre-W2-21 site as :776), and every guard of the country when that group instead falls to its wounds on its own turn (strategy.rs:859-870). A deployment's containment (`resolve_outbreak`) credits nobody.

### 1.11 defender_power (CD11)

**Signature.** `Game::defender_power(defender, next_threat: u32) -> Result<DefenderPower { defender, threat, kills_per_min, survival_s, rating: u16, band: PowerBand }, CommandError>`. It is pure: no RNG, no mutation. `Game::next_threat()` = `campaign.progression_rank()`.

**Inputs.**
- The profile comes from `deployment_inputs`.
- The snapshot comes from `progression_for(mode, next_threat, lane_count())`, made `pub(crate)` (progression.rs:514).
- The reference enemy is `resolve_kind(EnemyKind::Shambler)`.

**Personal model.**
- dps = `max(1, damage − armor_e)·(1 + 0.5·weak_bonus)·mag/(mag·cadence + reload)`;
- kill rate = dps/(health_e·enemy_health);
- survival = health/(`max(1, round(damage_e·enemy_damage) − armor)`/`max(windup_e, 0.5)`·arrival).

**NPC model, per 1-second turn.**
- p = `attack_success_probability(guns + skill_bonus)`·(1 − `threshold_probability(npc_dodge_e)`);
- per-hit damage = `max(0, mean_basic_damage − npc_armor_e)`;
- dps = p·per-hit·mag/(mag + reload_turns);
- survival = max_hp/(`attack_success_probability(npc_skill_e)`·(1 − `threshold_probability(dodge)`)·max(0, mean(npc_damage_e)·enemy_damage − armor)·arrival).

**Rating.** `round(100·sqrt((kill/kill_ref)·(surv/surv_ref)))`, clamped to 1..=999. The refs are the same formula for `Campaign::default()` at threat 0 per mode, in a OnceLock, so a fresh starter scores exactly 100 in both models. Bands: Weak below 85, Even 85-115, Strong above 115.

**Cost.** Recorded in release, not asserted.

*As built (W2-21, `sim/power.rs`):* the shared helper is `deployment_inputs_for(campaign, defender, loadout, character, threat, front: Option<FrontPosition>)`, public as `Game::deployment_inputs(defender, threat) -> Result<DeploymentInputs { defender, threat, profile, snapshot, max_hp, balance }, CommandError>`. W2-26 (363bfac) added the `front` parameter, the stage's place on its country's front (W2 review): `deploy_as` passes the stage's `FrontPosition` (`Country::front_position(stage)`), so the deployed snapshot carries the front ramp and boss rank, while `Game::deployment_inputs` (and through it `defender_power`) and the power reference pass None. That None is what keeps power a rank-only, country-free reading: no front ramp and the default Sector boss rank, as on stage 0 of a front of two or more stages. It takes no level, and the inputs stop before the country's modifiers (`personal_damage`, `npc_skill`), which `deploy_as` still applies after it; on a country whose modifiers leave the defender alone the deployed attempt's profile is exactly `profile`. A recruit's profile carries its identity appearance. `enemy_health` and `enemy_damage` are the snapshot's times the balance preset's, and `arrival` is the snapshot's times `spawn_rate` (and `npc_spawn_rate` for Npc). Health and arrival match the battle, and so does the Personal hit, which the battle also multiplies by both damage factors and rounds (`sim/combat.rs`). The NPC hit does not (W2 review): the model multiplies the mean enemy hit by the product, while the NPC battle adds `floor((snapshot enemy_damage − 1)·6)` to the dice adds and then the mean times `(balance enemy_damage − 1)`, rounded (`sim/combat.rs`), so NPC survival approximates the battle's damage rather than reproducing it. The NPC kill rate divides by the Shambler's `npc_hp`. A defender the reference enemy cannot hurt survives 3,600 s. `UnknownSurvivor` for a recruit not on the roster and `Rejected` past `threat_guard_rank`. `DefenderOption.power` is read at `next_threat()` held at `threat_guard_rank`, and `OutbreakPlan.recommended_defender` is the selectable option with the highest rating, the earlier row on a tie (the player is the fallback when no row is selectable; W2 review: the player and Mara carry the recruits' held-elsewhere `InvalidLoadout` blocker, so a guard or expedition holding one of their items blocks them too).

### 1.12 Regional checkpoints (decision 13): derived, no reward

- `Country::checkpoint_count() = ((level_count()−1)/6).min(3)`. Measured distribution: 219 countries have 0, 12 have 1, 4 have 2 and 7 have 3.
- `Country::region_of(stage) -> RegionPosition { region, regions, first_stage, last_stage }`.
- `Country::is_checkpoint_stage(stage)`: the 0-based stage `6k−1` with `k ≤ checkpoint_count`.
- `WorldProgress::regions_secured(id) = min(max(completed, clear_high_water)/6, checkpoints)`. It is stable under reopen, because reopen sets completed = count−1 (world.rs:578-579).

**Effects.**
- Presentation lines: the Results "Region k of n secured · N defenses to liberation", Briefing segmented progress and a map tooltip.
- `stage_theme_with` (world.rs:174) returns the existing `checkpoint` theme (lighting night, world.json) on checkpoint stages. It is verified that only horde_game reads stage themes. This serves the `night-checkpoint` capture.
- `region_of` is offered to enemy-balance as the in-country ramp input.

There is no coin grant, no feature gate and no saved field. Rewards are a wave-4 lever with evidence.

### 1.13 Strategy pacing harness (CD12)

**Plan type.** `PacingProfile.plan: PacingPlan { recruit_up_to: usize, takeover: bool, parallel: PacingParallel{Off,Single,Operations}, guards: bool, camp_ticks: u32, route: PacingRoute{Easiest,FewestStages}, seed_strategy: bool }`. `PacingPlan::SOLO` = (0, false, Off, false, 0, Easiest, false) keeps the 3 existing profiles byte-identical.

**`strategy_profiles(max_runs)`.**
- S1 "strategy, novice, gear first, Luxembourg": recruit 4, Operations, guards, camp 1,800 ticks (30 s), FewestStages, seeded, 120 runs.
- S2a "neighbours solo, Germany".
- S2b "neighbours parallel, Germany": recruit 3, Single then Operations.

S2a and S2b use the same seed and camp 1,800, restricted to DEU's seven 3-stage neighbours.

**Seeding.** With `seed_strategy`, the harness sets `g.strategy.rng = Rng::new(seed ^ 0x504c41475545)` (non-zero asserted). `StrategyState::default` (strategy.rs:94) is unchanged, and no per-journey seed ships.

**Camp phase.** The harness ticks `camp_ticks` with no attempt (sim.rs:1036-1043).

**Bot.** It acts only through `Game::apply`, using:
- RecruitAs with the offered archetype of higher `defender_power`;
- AssignGuard on active outbreaks, ReleaseGuard when an outbreak is contained;
- DeploySurvivor or CountryOperation to the next enterable 3-stage front;
- SetNextDefender when takeover is on and the best recruit's rating ≥ the Player's;
- an outbreak with `ticks_left` below the next run budget: SelectCountry + BeginDefense{count−1}.

**Metrics (`StrategyMetrics`):**
- runs and minutes at liberations 2/4/10 (`Milestone::Liberations(n)`, appended);
- `coins_total` = Σ Report.coins + Σ summary.coins + Σ sale amounts;
- `battle_min`, `camp_min`, `coins_per_min`;
- overruns, from a per-tick diff of outbreak ids turning Overrun (pruning-proof), counted from the Operations unlock over the next 10 liberations;
- `runs_to_free_neighbours(K = 3)`.

**Sweeps.** `simulate_pacing_seeds(&profile, n)` uses seeds `base ^ k·0x9E3779B97F4A7C15` and reports medians and p90. The example gains `--strategy --seeds N`. The first 20-seed table is written to `target/production-before/strategy-pacing.txt` at the end of wave 2, before tuning, and quoted in the plan.

*As built (W2-43, `sim/pacing/strategy.rs` on the progression-tree harness, rulings W-1 and P7):* the plan is `PacingPlan.strategy: Option<StrategyPlan { recruit_up_to, parallel: PacingParallel {Off, Single, Operations}, guards, camp_ticks, route: PacingRoute {Easiest, FewestStages}, countries, seed_strategy }>`; `PacingPlan::SOLO` has None, and `StrategyPlan::OFF` (everything off) plays exactly the solo profile through commands (`solo_plan_is_todays_profile`). `Operations` sends single stages until `grand_strategy_unlocked`, then country operations, so S2b's "Single then Operations" is `Operations`. The rows are `strategy` (S1, as above, from `novice-gear-lux`), `neighbours-solo` (S2a) and `neighbours-parallel` (S2b, recruit 3), the S2 rows on `expert-deu`'s seed, aim and spending with `countries` = AUT, BEL, CHE, CZE, DNK, LUX, NLD and `stop_after` Liberation(4). Seeds use the harness's `pacing_seed`, not the k·0x9E37… formula; `pacing_start` seeds the strategy RNG at every index of a seeded plan. The bot's takeover option (`SetNextDefender`) is not built: the W2-42 `takeover` row covers takeovers. Every action is a `Game::apply` command (gear and nodes too, through `smoke_buy_gear_with`); `simulate_strategy_traced` and `replay_strategy` prove it (`strategy_bot_acts_only_through_commands`). Definitions: `runs_to_free_neighbours(K)` counts runs from the start country's liberation (`Liberation(1)`) to the run after which K listed countries are liberated; reopens are overruns, seen tick by tick, while `liberated_count` is within the 10 liberations from the one at the strategy unlock (a seed whose window never completes counts as 5); coins per minute is (run earnings, the harness's `cumulative_coins`, plus expedition summary coins) over battle and camp minutes; medians are lower medians. The evaluators are `liberations_and_coins_per_minute`, `parallel_frees_neighbours` and `outbreak_reopens`, run by the ignored `pacing_targets` goal-3 tests.

## 2. Decisions

| Question | Chosen | Rejected (reason) |
|---|---|---|
| Recruit runs the main defense | `Attempt.takeover` attempt-owned Npc, Mode::Npc rules, `active_tactics` and `record_defender_progress` | Roster record plus `RecruitStatus::Defending` plus `npc_slot` (new variant, worker special case, three mirrors); swap into campaign.npc (every reader sees the recruit); foreground worker (breaks Report and Results) |
| Defender on Attempt/Report | `recruit_id` plus a derived `defender()` | Stored `defender: DefenderId` (redundant with mode; migration over nested reports); Option<DefenderId> (two representations) |
| next_defender location | `Game.next_defender` | `Campaign.next_defender` (leaks into worker clones) |
| Reservation during takeover | `reserved = Some(Npc)` plus `Campaign.reserved_recruit` | Over-reserving Mara's loadout (misleads sale_eligibility; deploy_survivor and reserve_external do not call preparation, parallel.rs:66-95); `reserved: Option<DefenderId>` (rewrites Npc→Mara, digest move) |
| Picker deploy command | `BeginDefense{stage}` with `Deploy{mode}` frozen | Overloading Deploy{mode:Npc} (one recorded command, two meanings); DeployAs{stage,defender} (two sources of truth) |
| Live tactics during takeover | `SetRecruitTactics{id,tactics}` targets the takeover npc | Hiding HUD controls (regression vs Mara) |
| Credit | `commit_victory` + `credit_npc_victory`; player completion XP in every mode | XP to the winner as CP, or 50/50 (moves lane unlocks and pacing) |
| Allowance | `max(1+fc, min(10, 1+ceil(3L/2)))`, plus the liberated_count rewrite | B's floor curve (slower on the 3-stage route); `Npc.service_xp` (saved field) |
| Archetypes | Rotation first plus a country-hashed second, `ArchetypeChoice` preview | strategy.rng (shifts outbreaks); biome weighting (opaque) |
| Origin trait | Derived from biome, flavour only | Home-ground +1 Guns/Melee (unbaselined balance change in wave 2) |
| One recruit per liberation | Not enforced | AlreadyRecruited (unrequested rule change for v6 saves) |
| Expedition notices | Saved `ParallelBattle.summary` with acknowledged, `AcknowledgeExpedition`, acknowledged-first pruning | `#[serde(skip)]` outbox (lost on save/quit) |
| Checkpoints | Derived, presentation plus the checkpoint theme | Coin grant (moves DEU pacing in wave 2); gate counting (wave-4 lever); mini-boss (enemy-balance) |
| defender_power | Analytic, starter = 100, shares `deployment_inputs` | 60 s probe (cost, cache); win rate (empty for new recruits); `power_reference` catalog block (unneeded) |
| Service history | Derived from run_history plus `Recruit.containments` | Full saved ServiceRecord (duplication) |
| Strategy RNG | Seeded in the harness only | A shipped per-journey seed (save-determinism/ux follow-up) |
| Mara gate | Companion | Recruitment (regresses today's Mara takeover) |

## 3. Commands and types added (all appended)

- `Command::SetNextDefender(DefenderId)`
- `Command::BeginDefense { stage: usize }`
- `Command::SelectCountry(String)`
- `Command::RecruitAs { country: String, archetype: Archetype }`
- `Command::SetRecruitTactics { id: u64, tactics: NpcTactics }`
- `Command::AcknowledgeExpedition(u64)`

`CommandError` additions:
- `DefenseInProgress` (shared with other remits; define once);
- `DefenderUnavailable { defender: DefenderId, blocker: DefenderBlocker }`;
- `ArchetypeNotOffered { offered: [Archetype; 2] }`;
- `RecruitUnavailable(RecruitBlocker)`;
- `StartNotChosen`;
- `CountryLocked { country_id: String }`;
- `ExpeditionNotSettled`.

Display texts are written in plain player language.

## 4. Saved fields (wave-1 freeze)

All use `#[serde(default, skip_serializing_if = ...)]`, so v1-v6 need no migration.

- **`Attempt.takeover: Option<Takeover>`.** Validation at depth 0 only:
  - mode Npc;
  - the roster recruit exists with status Deployed;
  - `r.npc.loadout == t.npc.loadout`, `r.name == t.npc.name`;
  - `validate_npc(&t.npc)`;
  - the recruit is inserted into the `recruits` duty set before the expedition loop, so a duplicate duty is rejected;
  - `expected_reservations.extend(t.npc.loadout)`;
  - `campaign.reserved == Some(Npc)` and `reserved_recruit == Some(t.recruit_id)`;
  - rejected inside workers.
- **`Report.recruit_id: Option<u64>`.** Some requires Npc mode, depth 0 and a roster id.
- **`Game.next_defender: Option<DefenderId>`.** Some(Recruit(id)) requires a roster id; rejected inside workers.
- **`Campaign.reserved_recruit: Option<u64>`.** Equals `attempt.takeover.recruit_id` at depth 0; always None in workers (`clone_for_worker`).
- **`ParallelBattle.summary: Option<ExpeditionSummary>`.** `fronts.len() ≤ 32`; `acknowledged ⇒ settled`; `outcome.is_some() ⇔ settled`, except v6-settled battles, which have None.
- **`Recruit.containments: u32`** (skip if 0).
- **`RunRecord.recruit_id`**: an existing field; takeovers now write it.
- **Explicitly NOT saved:**
  - `Game.expedition_outcomes` outbox, ParallelBattle.loot_ids and announced, `RecruitStatus::Defending`;
  - Campaign.benched_companion, Npc.service_xp, StrategyState.overrun_total;
  - checkpoint claims, origin trait, archetype offer, recruit appearance;
  - advancement level and cap, service record, strategy seed.

  This closes the critic's missing_saved_fields items for CD.

**Fixtures** (owned by save-determinism; contents required by this remit):

`save_v6.json` must hold:
- a settled Mara last_result;
- `npc.awarded == 52` with 5 first_clears;
- a roster with at least 1 Ready, 1 Guarding and 1 Deployed recruit;
- one unsettled and one settled ParallelBattle.

`save_v7.json` lands at the end of wave 2 and must hold:
- a live recruit takeover;
- `next_defender = Some(Recruit(id))`;
- a past Report with recruit_id;
- a takeover RunRecord;
- one unacknowledged summary;
- a recruit with containments > 0.

## 5. Steps

Section 6 has the structured list. Every step re-runs `--test determinism`, and the GOLDEN and GAMEPLAY_TRACE files stay unedited.

## 6. Digest moves

None from this remit in any wave:
- GOLDEN payloads (determinism.rs:57-75: fresh Game, deploy(0,mode), 1,800 ticks, no report, roster or battles) contain none of the new keys.
- The digest hashes the payload only (persistence.rs:95-98).
- With `takeover == None` every redirected read and write equals today's.
- `commit_victory` + `credit_npc_victory(&mut self.npc)` reproduces `first_clear`.
- `finish_attempt(Mara)` equals `reserved == Some(Npc)` for every existing path.
- The allowance equals legacy at L = 0.
- No RNG draws are added.
- Checkpoint themes are not core state.

A GOLDEN or GAMEPLAY_TRACE diff in a campaign-defenders commit is a defect, not a re-bless.

## 7. Risks

1. The derived `Report::defender()` deviates from the brief's literal wording (needs_user). The fallback is a literal `Report.defender` plus a v7 migration over `last_result` and `battles[].game.last_result`.
2. A liberation-based allowance makes `validate_npc` depend on `liberated_count()`. This is mitigated by `release_recruit_checked`.
3. Mara's allowance rises to 84 at L = 6, opening a 25-coin-per-CP sink. Re-measure in wave 4.
4. `RecruitStatus::Deployed` is shared by expedition and takeover. The UI must use `recruit_duty()`.
5. The auto-reset of next_defender to Player must be visible (strategy note plus picker).
6. Strategy pacing runtime is minutes in release. Keep it `#[ignore]`, release-only and queued after the debug builds, with at most 2 cargo builders.
7. The ≤ 1/1.5 neighbour ratio depends on the Parallel unlock at 2 liberations (`strategy_config.parallel_unlock_freed`). If it fails in wave 4, the fix comes through balance owners with evidence, never through harness definitions.
8. The checkpoint theme changes the background of the 23 large countries' checkpoint stages in captures. qa-captures must expect it.
9. Takeover does not change lane geometry (it follows player.level, campaign.rs:209-211), so the uniform zoom-to-fit camera needs no defender input. The observer-window fit (ui/operations.rs:277) belongs to ux-flow and vfx-lighting.

## 8. Numbers to measure

| Number | Requirement |
|---|---|
| `--smoke` | Equal to the post-W1-69 baseline target/production-after/w1-69/smoke.txt (Personal stage1 falls on attempt 2, Npc on attempt 3; wave 0's production-before/smoke.txt had 3 and 2, plan §9.6) after every wave-2 step. W2 review: W2-33, W2-39 and W2-54 moved smoke on purpose with their rows in plan §9.6, so a later step compares against the latest recorded smoke (`w2-54/smoke.txt` at the W2 review) |
| `pacing -- 60 --runs`, 3 solo profiles | Empty diff vs the post-W1-69 baseline target/production-after/w1-69/pacing.txt (expert first victory run 3, wave 0 run 4; tree exhausted run 26). Amended by R28 (plan §2.4, 2026-11-09): empty diff vs the latest recorded output, `w2-41/pacing.txt` at R28, then each output a later commit records with its before/after rows in plan §9.6 (`w2-54/pacing.txt` at the W2 review) |
| 20-seed strategy medians and p90 | runs and minutes to liberations 2/4/10; coins/min; battle and camp minutes; S2b/S2a runs-to-free-3 (target ≤ 0.667); overruns per 10 liberations after the Operations unlock (target < 5). Before tuning in `target/production-before/strategy-pacing.txt`, after in the plan |
| defender_power | Release cost per call and for 13 defenders (target < 50 µs total, recorded). *As built (W2-21, release, 2,000 rounds, from the W2-21 commit body bc85bf7):* 6.9 µs per call, 90 µs for 13 defenders (Player, Mara, 11 recruits), and 105-110 µs for `available_defenders`, all above the 50 µs target; the time is `Campaign::profile_for_loadout`, which the deploy shares. The same commit made `outbreak_plan` fill `recommended_defender` through `available_defenders`, so each `outbreak_plan` call pays that cost again and a frame that reads the picker and an outbreak plan pays it twice (not measured on its own). Recorded, not asserted; plan §2.5 holds the open item |
| liberated_count | Before and after cost (recorded) |
| Takeover save | Size delta in bytes (recorded) |

---

## Integration rulings

This section was added on 2026-11-05 when the synthesized spec above was copied into the plan. The spec above is kept verbatim as the design record. **Where this section and the spec disagree, this section wins.** The rulings come from four sources, applied in this precedence order: user decisions (U), orchestrator rulings (R, RL), master-planner reconciliations (P) and the five integration cluster rulings (C, D, E, S, W). The final saved-field table is `schema-v7.md`, the commit order is `waves.md` and the proof for every criterion is `acceptance.md`, all in this folder. The plan itself is `docs/plans/2026-11-05-production-pass.md`; any reference above to `docs/plans/2026-11-04-production-pass.md` means that file.

Each entry gives the binding ruling and then what it supersedes in **this** spec.

### User decisions

#### U1. SAVE_VERSION 7

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** SAVE_VERSION 7 bump approved before wave 1. v6_to_v7 is path-based, idempotent and catalogue-free; v1-v6 saves keep loading; save_v6.json is cut on untouched v6 code (W1-01) and save_v7.json is frozen at the wave-1 close (W1-63).
- **Supersedes in this spec:** Any "v6-&gt;v7 is a documented no-op for this remit" reading: the defender seat migration step (a) exists.

### Orchestrator rulings

#### R2. Defender identity

- **Owner / wave:** campaign-defenders, wave 1-2.
- **Ruling:** Serde key "defender" on Attempt and Report; an absent value means the mode owner (Player for Personal, Companion/Mara for Npc), so existing GOLDEN digests do not move for this reason. Mara stays selectable from the Companion node; recruits from the Recruitment node. One status model (RecruitStatus::Defending appended) and one npc source of truth (the roster entry, justified for determinism in D6). Takeover deploy stamps the recruit's appearance identity, not Mara's Female.
- **Supersedes in this spec:** Report.recruit_id with a derived defender, Attempt.takeover, Game.next_defender and the Deployed takeover status; the needs_user item is answered.

#### R8. Single owners

- **Owner / wave:** progression-tree, wave 1-4.
- **Ruling:** Pacing harness v2 = progression-tree (wave 2; CD and EB add profiles on its API); per-archetype telemetry = enemy-balance; SHA-256 / provenance / orphan helpers and scripts/lib/Native.ps1 = qa-captures (wave 1; audio, vfx and perf reuse them); measure_performance.ps1 = perf-architecture on top of Native.ps1; storm tagging = vfx presentation table only (no world.json change); regional checkpoints are derived (no saved field), presentation lines by CD plus EB's Regional boss rank (HP and reward x1.2) in wave-4 balance, no theme-rule change; save_v7.json is cut at the wave-1 close and frozen, later cases use inline JSON or new fixtures; stress scene raises the staged enemy cap only behind a review/test-only seam (no production save accepts &gt; cap+1), owner perf.
- **Supersedes in this spec:** Own PacingPlan rewrite of examples/pacing.rs; checkpoint night theme.

#### R10. UI prices read core

- **Owner / wave:** progression-tree, wave 2-3.
- **Ruling:** Recruit and training prices in the UI read the core price function (Campaign::recruit_price / npc_training_price, Game::recruit_offer().price).
- **Supersedes in this spec:** recruit_offer price comes from Campaign::recruit_price.

### Master-planner reconciliations

#### P6. Regional checkpoint ownership

- **Owner / wave:** enemy-balance, wave 2, 4.
- **Ruling:** enemy-balance owns FrontPosition and Country::regional_checkpoints() (wave 2); campaign-defenders owns only the presentation lines (wave 4, W4-02). No theme change.
- **Supersedes in this spec:** CD #11 checkpoint accessors (checkpoint_count, region_of, is_checkpoint_stage, regions_secured) and the checkpoint night theme.

#### P7. Pacing harness owners

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** PT owns sim/pacing.rs v2; EB, CD and LE add registry rows and pure evaluators in their own commits; EB owns per-archetype telemetry population.
- **Supersedes in this spec:** PacingPlan rewrite.

### Cluster: defenders

#### D1. Defender identity types

- **Owner / wave:** campaign-defenders, wave 1.
- **Ruling:** New horde_core defenders.rs: pub enum DefenderId { Player, Mara, Recruit(u64) } (the only public identity; display names from npc.name / survivor name); saved pub enum DefenderSeat { #[default] ModeOwner, Recruit(u64) } serialising like DefenderId::Recruit; resolve(mode, seat); RECRUIT_IDENTITIES: [(&str, Appearance); 12] moved from strategy.rs:309-322 in order; recruit_identity(id); RecruitStatus::label(). Types only, no behaviour change.
- **Supersedes in this spec:** DefenderId::from_parts(mode, Option&lt;u64&gt;) (now resolve(mode, seat)).

#### D2. "defender" key on Attempt and Report (R2)

- **Owner / wave:** campaign-defenders, wave 1.
- **Ruling:** Attempt and Report gain #[serde(rename = "defender", default, skip_serializing_if = "DefenderSeat::is_mode_owner")] pub(crate) seat: DefenderSeat. Absent = mode owner. No GOLDEN or TRACE move. Attempt::defender() / Report::defender() via resolve. The Report literal at lifecycle.rs:365 copies the seat; settle_parallel does not overwrite worker reports. No Report.recruit_id. RunRecord.recruit_id is written Some(id) for a takeover.
- **Supersedes in this spec:** Report.recruit_id with a derived defender; Attempt.takeover as identity source.

#### D3. v6_to_v7 for defender data

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** For each battles[*] (missing array = nothing): require u64 recruit_id else SaveError::Migration; or_insert "defender": {"Recruit": id} on game.attempt and game.last_result. The root attempt/last_result are never touched. Npc attempts get profile.or_insert("appearance","Female"). No appearance re-stamp for v6 in-flight expeditions (documented). No acknowledged key.
- **Supersedes in this spec:** "v6-&gt;v7 is a documented no-op for this remit".

#### D4. Worker seats and appearance stamping

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** stamp_recruit_defender(attempt, recruit_id) sets seat Recruit(id) and appearance recruit_identity(id).1. Called in deploy_survivor after the worker deploy (parallel.rs:113-117) and after the chained operation stage deploy (parallel.rs:317). Inside a worker, defender_npc still returns the worker's campaign.npc copy.
- **Supersedes in this spec:** "Worker appearance stamping in the wave-2 expedition step" (moved to wave 1).

#### D5. Takeover status: append RecruitStatus::Defending

- **Owner / wave:** campaign-defenders, wave 1.
- **Ruling:** RecruitStatus { Ready, Guarding, Deployed, Defending }. Deployed = on an expedition (unchanged). Defending = depth-0 main-run defender. reserve_defender(id) (Ready -&gt; Defending) and release_defender(id) (Defending -&gt; Ready); reserve_recruit/release_recruit stay Deployed-only. tick_world skips claim_objectives for Deployed | Defending. Existing Ready-gated mutators reject Defending. horde_game adds a "Defending" label arm.
- **Supersedes in this spec:** strategy.reserve_recruit sets Deployed for takeovers; DefenderDuty::Defending derived from attempt.takeover.

#### D6. Battle npc source of truth: the roster entry

- **Owner / wave:** campaign-defenders, wave 2.
- **Ruling:** No Npc snapshot on Attempt (Attempt.takeover withdrawn). The defending recruit's live Npc is strategy.roster[id].npc. defender_npc(&self, seat) / defender_npc_mut take the seat explicitly (advance_tick takes the attempt out of self). record_defender_progress(seat, hits, ticks). Reads at sim.rs:1106-1107, npc.rs:72/102/125/181, writes at sim.rs:1117, npc.rs:272 and lifecycle.rs:318 use a.seat. Justification: one serialised copy, no copy-back at finish, the mode-owner path is byte-identical, other roster writers skip or reject Defending, workers keep isolated copies.
- **Supersedes in this spec:** Attempt-owned Npc with active_tactics, record_defender_progress on t.npc and release_recruit_checked.

#### D7. Seat, Defending, reserved_recruit and next_defender validation

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** validate_game rules 1-8 as in schema-v7 §5 (Personal + Recruit invalid; depth-0 live Recruit seat requires exactly one Defending recruit with that id, reserved Some(Npc), reserved_recruit Some(id), reserved loadout, no battle, no guard; ModeOwner/no attempt means no Defending and reserved_recruit None; worker seats equal Recruit(b.recruit_id); worker games have no reserved_recruit, next_defender or Defending; next_defender must name a roster recruit; RunRecord worker_id None + recruit_id Some requires Npc). Campaign.reserved_recruit: Option&lt;u64&gt;; item_reserved / reserve_external cover Mara's loadout only when reserved_recruit is None.
- **Supersedes in this spec:** §4 "recruit status Deployed" validation.

#### D8. Takeover deploy and finish order

- **Owner / wave:** campaign-defenders, wave 2.
- **Ruling:** deploy_internal becomes deploy_as(level, DefenderId, stage); public deploy keeps resolve(mode, ModeOwner). Recruit preflight (UnknownSurvivor, Ready, recruitment, preparation, validate_loadout, weapon access) then commit: reserve_takeover, reserve_defender, profile_for_loadout(Npc, recruit loadout, &recruit.npc), stamp_recruit_defender; rollback like parallel.rs:118-125. finish(): (1) victory credit (commit_victory + credit_npc_victory(defender_npc_mut)), (2) Npc attempts bump moves out of finish_attempt, (3) takeover release, (4) LE H1, (5) RunRecord recruit_id, (6) Report seat.
- **Supersedes in this spec:** Commit step "reserve_recruit (Deployed)", "attempt.takeover = Some", release_recruit_checked.

#### D9. next_defender location and commands

- **Owner / wave:** campaign-defenders, wave 2.
- **Ruling:** Campaign.next_defender: Option&lt;DefenderId&gt; (skip None, None in workers); Game gets no field. Game::next_defender() = campaign.next_defender.unwrap_or(last_result.map_or(Player, Report::defender)). Appended SetNextDefender(DefenderId) and BeginDefense{stage} (deploys next_defender, unpauses like Deploy, clears it). Deploy{stage, mode} keeps its recorded meaning and never touches next_defender. ContinueJourney deploys next_defender(); retry/repeat use report.defender(). Dispatching the next defender elsewhere resets it to Player with a note. Mara via Companion, recruits via Recruitment.
- **Supersedes in this spec:** Game.next_defender.

#### D10. Expedition notice persistence and commands

- **Owner / wave:** campaign-defenders, wave 2 (types W1).
- **Ruling:** ParallelBattle.summary: Option&lt;ExpeditionSummary{outcome, stages_cleared, liberated, fronts (&lt;=32), coins, loot_count, acknowledged}&gt; plus StrategyState.folded_outcomes. Written in settle_parallel and finalised at settled = true. Pruning drops summary-None or acknowledged settled battles first; otherwise folds. Game::unacknowledged_outcomes() -&gt; OutcomeNotices. Appended AcknowledgeOutcomes{battles, folded}: idempotent, returns Applied. v6 settled battles are never announced. Types ship in wave 1 (W1-16).
- **Supersedes in this spec:** AcknowledgeExpedition(u64), ExpeditionNotSettled and expedition_notices().

#### D11. Defender command errors

- **Owner / wave:** campaign-defenders, wave 1.
- **Ruling:** CommandError appends DefenseInProgress and DefenderUnavailable{defender: DefenderId, blocker: DefenderBlocker{Locked{node_id}, Guarding{country_id}, OnExpedition{battle}, Defending, InvalidLoadout{reason}}}. Unknown recruit = existing UnknownSurvivor. Plain Display. Typed pre-check for AssignGuard, DeploySurvivor, CountryOperation, TrainRecruit, EquipRecruit, BuyRecruitPoint and SetRecruitPolicy on the Defending recruit, with no mutation.
- **Supersedes in this spec:** DefenderBlocker::UnknownRecruit; freeze tuple shape.

#### D12. Live tactics for the defending recruit

- **Owner / wave:** campaign-defenders, wave 2.
- **Ruling:** Appended SetRecruitTactics{id, tactics}: validates recall, applies to the roster npc when Ready or Defending (takes effect next npc turn); Guarding/Deployed give RecruitNotReady. SetRecruitPolicy stays Ready-only; SetNpcPolicy edits Mara. The HUD dispatches by active_defender().
- **Supersedes in this spec:** "Applies to attempt.takeover.npc when id is the live takeover".

#### D13. Auto-sell keep_upgrades judge vs takeover ordering

- **Owner / wave:** loot-economy, wave 2.
- **Ruling:** CD's takeover core (D6/D8) lands before LE's H1. H1 runs in finish() after the takeover release and judges with UpgradeJudge::Defender(a.defender()); the a.mode fallback is deleted. Expedition W-a keeps UpgradeJudge::Loadout{Npc, battle.loadout, battle.game.campaign.npc}. Every roster loadout is excluded from candidates.
- **Supersedes in this spec:** Commit order inside wave 2 is fixed.

#### D14. save_v7.json defender content

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** save_v7.json is cut at the wave-1 close and frozen. Defender content is staged through Game::staging().stage_takeover(recruit_id), which must pass validate_game: a live depth-0 Npc attempt with seat Recruit(id) and that recruit Defending; next_defender Some(Recruit(other)); a live and a settled expedition with Recruit seats; the settled summary acknowledged false; folded_outcomes &gt; 0; a RunRecord with worker_id None and recruit_id Some. Wave-2 cases use the frozen file, inline JSON or new save_v7_w2_*.json fixtures.
- **Supersedes in this spec:** Step "save_v7.json fixture with a live takeover (wave 2)".

#### D16. Wave-3 defender labels, picker, notices and price text

- **Owner / wave:** ux-flow, wave 3.
- **Ruling:** UI reads only core: Recruit modal and liberation block price from Game::recruit_offer(country).price; picker from available_defenders() dispatching SetNextDefender; Briefing primary dispatches BeginDefense{stage}; HUD/Results names from defender_card; HUD "MARA" and the forced female portrait (ui/hud/view.rs:84-91) removed; toasts from unacknowledged_outcomes() with AcknowledgeOutcomes in the same frame. UX does not commit red consumer tests for core contracts.
- **Supersedes in this spec:** CD writes the core contract tests in its D9/D10 commits.

#### D17. Battlefield appearance reads the stamped profile

- **Owner / wave:** vfx-lighting, wave 3.
- **Ruling:** vfx deletes the Npc =&gt; Female branches at art/battlefield/mod.rs:104-108 and ordnance.rs:90-94 and reads a.profile.appearance (Attempt::defender_appearance()). The observer draws the worker attempt's appearance and takes names from Game::battle_defender_card(battle_id). Main-run Mara stays pixel-identical (W3-17).
- **Supersedes in this spec:** CD 1.5 art/battlefield branch list (now owned by vfx).

### Cluster: economy-progression

#### E6. Recruit history: containments counter only

- **Owner / wave:** campaign-defenders, wave 2 (schema W1).
- **Ruling:** The only saved field is Recruit.containments (skip 0), incremented where a guarded outbreak becomes Contained. Game::service_record(recruit_id) is derived from run_history (RunRecord.recruit_id) plus containments. If run_history is ever compacted it must keep per-recruit summaries.
- **Supersedes in this spec:** Confirms.

#### E7. Keyed hash module and tags

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** Only crate::hash: pub(crate) mix (moved byte-identically from quality.rs:112-117), keyed(seed, a, b, tag) = mix(seed ^ mix(a) ^ mix(b) ^ tag), mod tags {QUALITY (unchanged), ELITE, AFFIX, ELITE_PICK, RECRUIT_OFFER, MILESTONE} as big-endian ASCII. Users: elite spawn keyed(seed, a.id, e.id, ELITE) % 10000 &lt; elite_rate_bp (director wave spawns only); affix keyed(.., AFFIX); elite-only drop keyed(.., ELITE_PICK); recruit offer keyed(seed, next_recruit_id, fnv1a(country), RECRUIT_OFFER); milestone keyed(seed, rank, 0, MILESTONE). The presentation scan forbids hash::keyed.
- **Supersedes in this spec:** The recruit offer key uses tags::RECRUIT_OFFER (no strategy RNG).

### Cluster: enemies-world-pacing

#### W-1. Pacing harness: one API, one acceptance file, named profiles

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** PT owns sim/pacing.rs v2 and examples/pacing.rs: PacingProfile{id, name, seed, country, aim, spend, max_runs, difficulty, plan: PacingPlan, stop_after}, PacingPlan{equip_drops, sell, defender, farm, train_companions}, by_id, DEFAULT_PROFILE_ID "expert-deu", pacing_seed, pacing_sweep (thread::scope), MilestoneStats (lower median, p25, p75; missing = max_runs+1). EB adds rows lux-easy/-normal/-hard (stop_after Liberation(1), max_runs 20) and start-aut/start-bgd; CD adds "takeover" with PacingDefender::RecruitWhenReady. Goal-5 and goal-6 acceptance use expert-deu, Normal, 20 seeds, 60 runs. One crates/horde_core/tests/pacing_targets.rs, all #[ignore = "release-only"], OnceLock sweep cache; run command in AGENTS.md. Default pacing 60 --runs reproduces the post-W1-69 baseline target/production-after/w1-69/pacing.txt byte-for-byte (UTF-8, no BOM, LF; W1-69 moved pacing on purpose, so the wave-0 target/production-before/pacing.txt is not the pin, plan §9.6). **Amended by R28 (plan §2.4, 2026-11-09):** the pin is no longer `w1-69/pacing.txt`. Default pacing 60 --runs is diffed against the latest recorded output: `target/production-after/w2-41/pacing.txt` at R28, then each output a later commit records with its before/after rows in plan §9.6 (`w2-54/pacing.txt` at the W2 review).
- **Supersedes in this spec:** w2 PacingPlan / strategy-profile rewrite of examples/pacing.rs.

#### W-9. Regional checkpoints and Capstone

- **Owner / wave:** enemy-balance, wave 2, 4.
- **Ruling:** One derivation: FrontPosition{stage, count}::boss_rank() (Capstone if stage+1 == count, else Regional if (stage+1) % 6 == 0, else Sector); Country::regional_checkpoints() from it (&lt;= 3 for 24 stages). Nothing saved per stage; only attempt-scoped snapshot.boss_rank and Report/RunRecord.boss_rank. No theme change, no checkpoint coin grant. Wave 2: FrontPosition, stamping, Capstone +1 transition. Wave 4: Regional HP x1.2, reward x1.2; Capstone HP x1.4, armor x1.25, NPC armor +2, reward x1.4, damage x1.10. CD owns presentation lines in wave 4 reading Game::stage_boss_rank.
- **Supersedes in this spec:** 1.12 checkpoint night theme in stage_theme_with.

### Rulings owned by this remit

R2, D1, D2, D5, D6, D8, D9, D10, D11, D12, E6
