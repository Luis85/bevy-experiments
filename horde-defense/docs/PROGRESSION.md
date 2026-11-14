# Progression and authored content

`crates/horde_core/content.json` is the encounter/equipment tuning source. It contains five encounter templates, five legacy enemy profiles, three boss concepts, three structure definitions and sixty-eight equipment definitions: the 29 of v1 and the 39 loot-only definitions appended at indices 29-67 (W2-08; [ITEM_QUALITY.md](ITEM_QUALITY.md), "Loot-only definitions"). Every level, item and structure carries a required stable `id` (a lowercase slug, unique per table); saves reference equipment by item `id`, never by array position, so reordering or appending items keeps existing saves meaning the same gear. Enemy and structure profiles are keyed by their `kind` and may appear in any order, but every kind needs exactly one profile. `content.rs` parses and validates the bundled data once. Changing content does not require changing simulation logic. `LASTLIGHT_CONTENT` selects a validated external JSON override at startup without rebuilding; otherwise the bundled file is used.

Ordinary component-authored archetypes and weighted waves live in the separate [enemy composition catalog](ENEMY_COMPOSITION.md); skill, firearm and throwable authoring is documented in [SKILLS_AND_WEAPONS.md](SKILLS_AND_WEAPONS.md). The legacy enemy array is not a five-archetype limit.

The five encounter templates are The Old Road (180 seconds), Ashwood Crossing (180), Depot Nine (210), The Iron Yard (210) and Lastlight Checkpoint (240). Arrival pressure grows to its authored cap. These templates supply encounter identities and mechanics throughout the country campaign; a saved campaign-threat snapshot scales pressure and new equipment beyond the fifth defense rather than resetting difficulty when the template index wraps. Country borders, population-derived local stage counts and scenario modifiers are authored independently in `world.json`; the world progress record belongs to the same campaign save as the armory and wallet.

## Personal survivor and campaign threat

Fresh profiles choose Rook or June before the first-country command. The choice changes the personal appearance, not initial attributes, pistol or combat rules. The preview is presentation state until explicitly confirmed. Mara remains the independent NPC companion. Existing saves without the identity field receive the legacy Rook selection.

Campaign threat is the committed victory count. Every victory—including a cleared-stage replay, outbreak response and worker/country-operation success—raises future threat exactly once; defeat, withdrawal, XP and world-ledger edits do not. Old unique-clear high-water records are only a one-time migration floor. Each attempt freezes difficulty and lane geometry. Threat 50 is the bounded-growth knee for speed/groups/population/economy, not a difficulty ceiling: health and damage continue to the explicit numerical guard of 10,000. Independent item levels reach 27 at victory 52 and their cap 30 at victory 58.

Equipment instances carry stable IDs, saved levels and optional six-tier quality separately from their definition’s unlock tier. Current-level shop offers use fixed base quality; drops roll quality with a separate counter key and do not consume additional combat RNG. Owned levels/qualities do not silently upgrade. Effective stats and sale values apply the rolled/base quality ratio after level scaling; missing quality or explicit base quality preserves old statistics exactly. See [ITEM_QUALITY.md](ITEM_QUALITY.md) for the complete schema and receipts. Template milestones still govern existing body/NPC allowances and blueprint gates.

See [BALANCE_AND_RUN_STATS.md](BALANCE_AND_RUN_STATS.md) for presets, frozen geometry, DPS and history. See [PROGRESSION_SCALING.md](PROGRESSION_SCALING.md) for the detailed JSON schema, shipped curve, snapshot rules and save compatibility. Its actual defaults and validation bounds, rather than this overview, define the numerical curve.

## Ownership and transactions

The campaign owns one shared wallet and armory. Items have persistent monotonically assigned IDs; loadouts and presets hold references to those IDs. Personal, Mara, recruit and guard loadouts reference the same armory. Active deployments and guard duties reserve their actual item IDs; equipment cannot serve two active assignments. Fresh personal characters must acquire an eligible weapon separately from Mara’s NPC-issued carbine, even after learning its class. Historical pre-proficiency saves retain their explicit legacy access. Deployment reserves the loadout and incompatible preparation mutations reject while it is reserved. The simulation owns per-attempt reward identities and calls `Campaign::reward` only after accepting a new reward transaction. The results screen never pays rewards.

The four legacy starter items and the fresh campaign’s separate personal pistol cannot be sold or unlocked. They are sufficient to start another attempt after all optional loot is sold. The armory has no gameplay capacity ceiling. Automatic collection never silently discards an item. Nothing is sold without the player: a sale is a command, or the auto-sell rule the player turned on (below).

Sales are strict since ruleset 2 (W1-20): one rule (`Campaign::sale_block`) decides every single and batch sale, and an item sells only when nothing holds it. Protected starter, locked, favorited and NPC-issue items, equipment worn by you, Mara or an idle recruit, items a saved preset references and items reserved by a deployment or a country guard cannot be sold: `SellItem` answers `CommandError::SaleBlocked` and the armory disables the sale and states the first reason. A sale never rewrites a loadout and never leaves a preset incomplete (GDD.md, "Selling and loadout safety"). Batch sale quotes and sales apply the same rule.

### Auto-sell

The campaign stores one auto-sell rule (`Campaign.auto_sell`, `campaign::AutoSellRule`). It is off in a fresh campaign and in every save older than v7, and a worker copy always carries the default rule (save validation rejects any other rule in a worker, so an expedition's sales are never paid twice). `Command::SetAutoSell` stores a new rule in or out of battle; it is used at the next settlement. The fields and their defaults:

| Field | Default | Meaning |
| --- | --- | --- |
| `enabled` | off | Settlement sells this run's matching drops |
| `sell_below` | Uncommon | A drop sells when its rolled quality is strictly below this, so the default sells Poor and Common. Common to Legendary are accepted; Poor would sell nothing and is refused with `CommandError::AutoSellRuleInvalid` |
| `slots` | all four | Ranged, Melee, Outfit, Accessory (`Slot::index` order) |
| `keep_upgrades` | on | Keeps a drop that beats what the judged defender wears in its slot by more than `UPGRADE_MARGIN` (1.0 slot score, the equipment recommender's measure) |
| `apply_to_npc_runs` | on | Also sells the drops of expeditions and country operations |

Settlement sells at two points and nowhere else. Neither draws randomness or emits a simulation event, and nothing is sold during a battle.

- **Main run (hook H1).** When a main run finishes (victory, defeat or withdrawal), after its reservation and any recruit takeover are released and before the level-up grant and rank milestone are paid, the rule sells the run's own drops (`Attempt.drop_ids`). `keep_upgrades` judges them for the defender who fought: you, Mara or the recruit (`UpgradeJudge::Defender`); a recruit the roster no longer resolves keeps everything. A Mara or recruit main run is a main run, so `apply_to_npc_runs` does not gate it.
- **Expedition and operation (hook W-a).** An expedition's drops queue in `ParallelBattle.pending_auto_sell`. At each of its run records (each stage of an operation) the queue is drained once: sold when the rule is enabled and `apply_to_npc_runs` is on, judged against the expedition's own loadout and NPC, and otherwise discarded from the queue (the items stay), so enabling the rule later never sells them. A final sweep just before the battle is marked settled sells what is left. These sales may happen while a main run is deployed; only the in-battle guard is skipped, every other guard applies.

Every auto-sale goes through the one sale rule, so a starter, locked, favourite or NPC-issue item, worn equipment, a preset's or an idle recruit's gear and a reserved item are never auto-sold. An item without a rolled quality (shop stock, starters) is never a candidate, and neither is a drop banked by an attempt deployed before v7 (`drop_tier` None, ruling E11). A sale that would overflow the wallet keeps its item and is counted, not lost.

Each sale leaves a receipt. `Report.auto_sold` (`AutoSoldReceipt`) holds the exact count and coins, the count per quality, the 12 sales with the highest coins (`MAX_AUTO_SOLD_LINES`; Results shows "and N more") and the drops kept because the wallet was full; the run record keeps `auto_sold_count` and `auto_sold_coins` (BALANCE_AND_RUN_STATS.md). Results shows "Auto-sold N · +C" with the lines on hover, or "Auto-sell is off · Equipment › Auto-sell", and History adds the line under the run's coins. The battle screen and the expedition observer never show sale text.

**Applying the rule to the stash.** "Apply rule to stash now…" in Equipment › Manage equipment › Auto-sell sells what the stored rule matches in the whole stash now, whether or not the rule is enabled. Here `keep_upgrades` keeps anything that would upgrade any defender: you, Mara once she has joined, and every roster recruit. A confirmation shows the count and coins first, and `Command::ApplyAutoSellToStash { expect_count, expect_coins }` sells only that exact quote. It is refused, changing nothing, in this order: `SaleBlocked(InBattle)` while a main run is deployed, `NothingToSell`, `SaleQuoteChanged { count, coins }` when the stash changed (the dialog shows the new quote), and `WalletLimit` when the whole batch would overflow the wallet. A sale answers `CommandOutcome::Sold { count, coins }`. The pure reads behind the panel are `Game::auto_sell_preview` (the stash, or the last run's drops still owned) and `Game::results_loot`.

## Personal development

Personal kills and structures grant Personal kill XP; NPC kills do not. Every victory grants the actual capped completion XP in either mode. Fresh profiles use XP for level/geometry without minting personal CP; permanent body ranks are coin-funded nodes in the same graph. Existing legacy points/allocations retain their budget through the pure-body node payment adapter and spent-point ledger. Coin-trained body ranks use highest owned target rank and are permanent; the old unique-template body_cap applies to legacy CP eligibility, not the fresh coin tree’s authored twenty-rank paths. Character MAX 50, saved geometry and every-victory threat remain separate.

The authored personal progression DAG contains 177 nodes rooted at stable pistol_training, including body development, firearm/ability paths, fortifications and feature access. The seven branch labels focus one complete graph rather than separate tabs. Purchases require the actual prerequisites and coins; innate pistol training is granted at creation. Class training permits eligible owned equipment and never grants a gun. Personal abilities and ordnance affect personal play; shared engineering effects improve structures in either mode. Mara and recruits retain independent NPC advancement. See [SKILLS_AND_WEAPONS.md](SKILLS_AND_WEAPONS.md) for stable IDs, effects, classes, charges and startup overrides. Personal body investment never converts into NPC skill or damage modifiers.

### Level-up grant and rank milestones

Each player level-up earns a coin grant of a quarter of the XP needed to reach it: `progression::level_grant(l) = round(level_grant_xp_fraction × xp_to_advance_from(l − 1))`, with `progression.json` `rewards.level_grant_xp_fraction` 0.25 and the player's XP curve `xp_to_advance_from(L) = round(20 + 15 × (L − 1)^1.3)`. That is 5 coins at level 2, 28 at level 6, 77 over levels 2-6 (about one first run), 166 at level 20 and 580 at level 50. Only the player's level counts. The completion XP of every victory goes to the player, whoever defended, so a Mara, takeover or expedition win can level the player and pay the grant; the defender's own XP and points stay its own.

Threat ranks 5, 10, 15 and 20 (`Campaign::progression_rank`, the committed victories) each pay one milestone (`rewards.milestones`):

| Rank | Coins | Item quality |
| ---: | ---: | --- |
| 5 | 500 | Uncommon |
| 10 | 1,000 | Uncommon |
| 15 | 1,500 | Rare |
| 20 | 2,000 | Rare |

The milestone item is a Ranged definition that is not NPC-only, of a weapon class the survivor is trained in (Pistol when none), at item tier min(unlocked tier, 4) or the highest lower tier that has one. Loot-only definitions count, since a milestone is not a shop offer. `Campaign::milestone_item_definition` picks it with the keyed hash `keyed(seed, rank, 0, MILESTONE)` over the candidates in catalogue order, so it draws no randomness and the same seed and rank always give the same definition. It is stored at the shop item level (`shop_item_level`) with the authored quality, or the definition's base quality if that is higher; it is not a drop, so the bad-luck loot counter does not move. An item that cannot be stored records `SettlementIssue::MilestoneItemLost`, and the coins and the claim still apply; with no candidate the milestone pays its coins and no item.

`Game::settle_progression_rewards` pays both from saved ledgers, `Player.level_grant_paid` (the highest level whose grant was paid) and `Campaign.milestones_claimed`. It pays every grant above the ledger, then every reached, unclaimed milestone in rank order. It runs in the root game only: when a main run finishes (victory, defeat or withdrawal), after the auto-sell hook H1 so a milestone item is never auto-sold, and at the end of an expedition settlement pass that credited a victory while no main run is live. It never pays mid-battle and never in a worker copy, so an expedition victory is paid once, by the parent. A frozen rank 5/10/15/20 the catalogue authors no reward for is claimed unpaid, so authoring it later pays no backlog. A save older than v7 loads with `level_grant_paid` at its level (from level 2) and the ranks it had reached claimed, so loading pays nothing ([SAVE_FORMAT.md](SAVE_FORMAT.md), "Progression ledgers").

Each payment leaves a receipt: the run's report keeps `progression_rewards` and its run record `level_grant_coins` and `milestone_coins`. Results shows each paid milestone as an amber headline, "Rank 10 milestone · +1,000 coins · <item name>", then the grant, "Level 6 reached · +77 coins", both above "Now open in the tree"; a receipt that paid nothing adds no line.

### Skill tree prices

The W4-p repricing (production pass, wave 4) sets every node price. Prices follow the analysis-only tree value model and six static tree rules, then the goal-5 pacing acceptance (SKILLS_AND_WEAPONS.md, "Prices and the tree rules"). The whole tree costs 96,720 coins, and tiers 1-3 cost 2,030 / 8,700 / 19,630 coins, where a tier is every node up to that prerequisite depth that is not gated behind 5 or more victories (BALANCE_AND_RUN_STATS.md, "Goal 5: progression pacing acceptance"). Body ranks cost 100 + 30 × (r − 1) coins (100 to 670); the trap, wall and turret ranks start at 400, 320 and 240 and rise 140 a rank. The three blueprints cost 150 / 200 / 250 coins, Meet Mara 400, Survivor network 1,500 (1,700 from W4-p until W4-15 lowered it so the recruits join before Parallel opens), Parallel command 1,100 and Country operations 1,500. A blueprint is bought as its tree node, and `Campaign::blueprint_price` is the one price every offer quotes; the content `blueprint_price` (25 / 35 / 50) is only what a legacy-features campaign pays.

History: the foundation pass scaled the original prices with prerequisite depth (depth 1 ×2, depth 2 ×2.5, depth 3 and deeper ×3, rounded to 5 coins) and left blueprints and feature unlocks as they were; the tree then cost 52,180 coins (was 18,125), and 61,180 once W2-41 appended the eight decision-19 nodes. It also cut the first encounter's boss to 1,100 HP (was 2,400). That reason and its before/after pacing table are in [the foundation pass plan](plans/2026-11-04-foundation-pass.md). Re-measure pacing with the `pacing` example (BALANCE_AND_RUN_STATS.md, "Pacing CLI").

## Mara's advancement

Mara starts with ST 10, DX 12, IQ 10, HT 12, Guns (Rifle) 16 and Broadsword 13. The creation build costs 76 points: 60 in attributes, 12 in DX/E Guns and 4 in DX/A Broadsword. That creation budget is separate from campaign advancement. Supported melee uses an explicit original unified-melee skill adapter mapped to the purchased Broadsword progression: the same trained melee value controls authored crowbar, axe, machete, hammer and saber profiles. Damage type is individually authored (crowbar/hammer crushing; bladed weapons cutting). Muscle-powered weapon profiles author their original ST-10 baseline. The rules module now applies the pinned Lite swing-table delta for the survivor's actual ST, and low-FP strength adjustments use that same procedure. ST improves both HP and melee force while firearm bullet damage remains unchanged. Supported human ST ends at 20; detailed per-weapon tabletop skill taxonomies remain outside the unified-melee adapter. HP derives from ST, FP from HT, Basic Speed from `(DX + HT) / 4`, Dodge from rounded-down Basic Speed plus 3, and Will/Perception from IQ. Will and Perception remain informational in the supported fixed-position encounter scope.

Mara and every recruit share the campaign's advancement allowance ("Recruit advancement allowance" below). It counts all awarded points, whether spent or unspent. One paid training-service point costs `Campaign::npc_training_price()` ("Camp prices" below). Twenty successful ranged hits and 120 cumulative simulated combat seconds each award one point once. A first NPC clear of an encounter template awards two points once. Earned awards take priority over paid training; awards blocked by a full allowance remain pending until capacity opens. Respecialization refunds spent points without changing the historical awarded total.

ST or HT improvements cost 10 points, DX costs 20, an additional Guns/Broadsword level costs 4. Independent HP/FP purchases are excluded from the pinned Lite subset; improving ST or HT improves derived HP or FP respectively. DX also improves both DX-based skills. IQ is not trainable because it currently has no useful combat benefit. The trainable rows are `NpcTraining::TRAINABLE` (ST, DX, HT, Guns, Broadsword); `TrainNpc` or `TrainRecruit` for IQ, HP or FP answers `CommandError::TrainingUnsupported` and changes nothing (the variants stay so old saves load). Mara's page and the recruit rows in Operations draw the same rows from that list and name no IQ, HP or FP training. No movement upgrade is sold. NPC health/fatigue and injuries are restored between attempts under the GDD's campaign recovery rule.

### Recruit advancement allowance

The allowance is the most character points the campaign lets Mara or any recruit be awarded (W2-10, design-campaign-defenders §1.7): `Campaign::allowance() = 12 + 8 × (cap level − 1)`, where `Campaign::advancement_cap_level() = min(10, max(1 + first clears, 1 + ⌈3L / 2⌉))`, first clears counts the campaign's first-cleared catalogue levels and L is `Campaign::liberated_count()`. L is a high-water count: a liberated country that an outbreak reopens still counts, so the allowance never falls. With the first-clear term below the liberation term, as in most journeys:

| Liberated countries (L) | 0 | 1 | 2 | 3 | 4 | 5 | 6 and more |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Advancement cap level | 1 | 3 | 4 | 6 | 7 | 9 | 10 |
| Allowance (CP) | 12 | 28 | 36 | 52 | 60 | 76 | 84 |

So a defender can reach advancement level 10 by the sixth liberation, and the cap stops at level 10 (84 points) whatever the first clears. The first-clear term can carry the cap above the table on a short route (a route of three-stage countries first-clears three levels and holds 36 through the second liberation, then follows the table), and the allowance is never below the earlier rule, 12 plus 8 per first clear, up to 9 first clears, which covers the shipped five-level catalogue; at L = 0 it equals that rule, so a fresh campaign's numbers did not move. A defender's advancement level (`Npc::advancement_level`) is the smallest level n from 1 to 10 whose `12 + 8 × (n − 1)` covers the points it has been awarded; defender cards show it with the campaign cap level, and Mara's page and the Operations roster show "awarded / allowance". A disabled "Buy 1 training CP" button's hover ends with `Campaign::allowance_raise_hint`, which names what would raise the cap next (a liberation, a new stage clear or either) or says the allowance is at its highest. The same advancement level sets an expedition's threat (PROGRESSION_SCALING.md, "Expedition threat").

### Camp prices

Camp services scale with the threat rank through one function, `progression::camp_price(base, rank) = round5(base × min(1 + per_rank × rank, max_multiplier))`, where round5 rounds to the nearest 5 coins and the shipped `rewards.camp_prices` are `per_rank` 0.1 and `max_multiplier` 6 (a multiplier of 1 returns the base unchanged). Two prices use it:

| Service | Core price | Base | Rank 0 | Rank 1 | Rank 10 | Rank 20 | Rank 50 and above |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| One NPC training point (Mara or a recruit) | `Campaign::npc_training_price` | `camp_prices.npc_training_base` 25 | 25 | 30 | 50 | 75 | 150 |
| One recruit | `Campaign::recruit_price` | `world.json` `strategy_config.recruit_cost` 100 | 100 | 110 | 200 | 300 | 600 |

The strategy layer charges the same prices (`StrategyState::buy_training_point` and `recruit_as`; `Game::recruit_offer` quotes the recruit price), and every camp button, enable check and hover text reads the core price, never a literal: Mara's page, the recruit rows in Operations and the recruit offer. `world.json` is not regenerated for this; only its `recruit_cost` base is read.

## Fortifications

Blueprint ownership and ranks now use the shared node preflight; authored victory/liberation requirements and learned construction access apply, while legacy saves retain earned capabilities. Blueprints are permanent; attempts spend renewed build supply, never permanent coins, on placement. Costs are trap 4, wall 3 and turret 10 supply. Base supply ranges from 30 to 45 by encounter template, with authored country and progression adjustments applied at deployment. Construction stays live unless explicitly paused. Save country plan records surviving placements for that country; plans require owned blueprints, unique cells, rows inside the frozen deployed lane count and columns 0–4, but may exceed a particular deployment’s budget. A new attempt applies the ordered affordable prefix, reports omissions and retains the complete plan. Clearing a stored plan does not remove active structures. The simulation revalidates cells, footprints and supply when deploying or constructing. The older single-layout API remains a compatibility path with its own template budget check.

Fortifications now have ten authored rank nodes per type, with their actual node costs, prerequisites and milestones. Old blueprint/rank fields count as ownership and retain their saved floors. The simulation applies their authored durability/damage effects together with shared engineering. Each trap rank also adds one charge to every trap placed afterwards (`charges_per_rank`, below), so at trap mastery rank 10 a trap places 13 charges instead of 3. Salvage and action clocks are simulation responsibilities.

## Overtaking, critical injury and loot

[OVERTAKING_AND_CRITICAL.md](OVERTAKING_AND_CRITICAL.md) records the user’s explicit passing and retained GURPS injury semantics. Enemy.y() includes saved lateral displacement for hit tests, range, corpses and loot. Since W1-69 enemies never queue: they overlap, and that displacement only eases back to 0. A conscious NPC enemy at zero/negative HP is Critical, not a paid defeat. Actual terminals stop/reap/pay in the same fixed tick. Loot receipts are bounded informational history for already-owned items, never a second reward path. An auto-sold drop still has its loot receipt; the auto-sold receipt (above) records its sale.

## Validation boundary

Automated tests cover XP ownership, advancement conservation across respecialization, pending earned awards, equipment reservation, protected starter recovery, preset identity, unlimited loot retention, invalid layout safety, profile separation and content authoring constraints. Balance numbers are authored prototype hypotheses. They are not evidence of equal clear rates between geometric personal play and rules-based NPC play, and the scope of seeded campaign/country checks and remaining player testing is recorded in [VALIDATION.md](VALIDATION.md).


## Playtest preparation assistance and identity safety

`Campaign::recommendation(mode)` is a read-only, deterministic next-action suggestion. After country liberation it points to an accessible new front instead of repeating the completed final stage. During preparation it prioritizes an unspent legal body point, useful equipment already owned, an earned NPC skill increase, newly available blueprints, the cheapest affordable node that acts now (`Campaign::node_helps_now`; body ranks only after those), then guaranteed shop equipment or the next defense. Its structured action, coin price and affordability are derived from the actual campaign; it performs no purchase or allocation. A recommendation never inspects future random outcomes. Body-cap checks keep its allocation action legal.

`compare_item(mode, id)` compares the complete current and hypothetical loadout without changing ownership. Personal comparisons expose damage, magazine, cadence, reload, armor, health, stamina, weak-point bonus and a body-hit Shambler breakpoint. NPC comparisons expose the actual purchased skills, weapon dice/adds/type, accuracy, protection, HP/FP, Dodge and melee profile. NPC mean basic damage is explicitly **before** attack rolls, defense, armor and wounding; it is not a probability of injury.

Armory identities remain sorted by monotonically increasing creation ID. Binary lookup makes large-stash comparisons efficient without imposing a gameplay item cap. A failed purchase at identity exhaustion and a sale that would overflow the wallet leave both inventory and currency untouched. Save validation checks the actual frontier, body cap, processed XP, unique skill ownership, legal blueprint upgrades, layouts and preset slot references. A preset saved under ruleset 1, when a sale could leave it pointing at a sold ID, still loads with that ID and is refused when applied ("Preset references sold equipment"); no ruleset-2 sale creates one.

## Individual recruit creation

Legacy NPC records without a `creation` object deserialize to Mara's existing 76-point build. Each new recruit joins as one of four independently validated, equal-budget creation profiles: the rotation by recruit id, or the second archetype its offer names (`Game::recruit_offer` and `RecruitAs`, STRATEGY.md); respecialization restores that survivor's own baseline and never changes historical advancement awards.

| Background | ST / DX / IQ / HT | Guns / Melee | Creation points | Initial policy |
|---|---|---|---:|---|
| Marksman | 10 / 12 / 10 / 12 | 16 / 13 | 76 | Marksman |
| Guardian | 12 / 11 / 10 / 12 | 13 / 14 | 76 | Guardian |
| Scout | 10 / 13 / 10 / 10 | 16 / 15 | 76 | Balanced |
| Steadfast | 11 / 11 / 10 / 13 | 14 / 13 | 76 | Boss focus |

The creation cost is checked against the pinned attribute and DX/E Guns, DX/A unified-melee skill costs. Both purchased skill levels are already in the four-point-per-level advancement range. Creation budgets and advancement ledgers are separate. Names are original fictional survivor identities with an explicit country of recruitment; they are not a claim of researched national naming or culture.


Offensive structures now declare an explicit NPC skill: trap 14 and turret 15. Walls do not attack. The generic equipment skill-bonus field no longer accidentally raises a turret to skill 29; range, active defense, protection and the authored damage profile still resolve normally.


### Supported authoring bounds

The initial schema deliberately keeps five encounter templates, the canonical five-enemy order and three-structure order, the legacy protected starter definitions, a personal pistol and guaranteed shop/loot options for the supported slots. Accepted values are finite: boss timers 10–3,600 seconds, at most 512 ordinary enemies plus a reserved boss, arrival rates up to 30 per second, enemy movement 0.01–10 yards/second, windups 0.5–60 seconds, personal firearm cadence at least 0.08 seconds, and NPC magazine/reload bounds of 1–1,000 rounds and 1–60 turns. A trap may declare `charges_per_rank` (absent means 0; walls and turrets must leave it 0), bounded so its charges at mastery rank 10 stay at or below 1,000. A trap placed at structure mastery rank r starts with `charges + charges_per_rank × r`. The shipped shrapnel trap authors `charges_per_rank` 1 (W4-03), so it places 3 + r charges; the wall and the turret leave it 0. `progression.json` may carry an optional `rewards` block with `level_grant_xp_fraction` (0–1), up to four `milestones` (`{rank, coins, item_quality}`, ranks 1–10,000 strictly increasing, coins up to 1,000,000) and `camp_prices` (`npc_training_base` 1–1,000,000, `per_rank` 0–1, `max_multiplier` 1–10). Absent, it is neutral: no level grant, no milestones and flat camp prices. These are implementation safety limits, not recommendations for balance. Current authored values are the checked-in JSON; earlier prototype values are not a second source of truth. The shipped block authors all three: `level_grant_xp_fraction` 0.25 and the four milestones at ranks 5, 10, 15 and 20 (W2-54; see "Personal development" above), and `camp_prices` (`npc_training_base` 25, `per_rank` 0.1, `max_multiplier` 6): `camp_price(base, rank) = round5(base × min(1 + 0.1 × rank, 6))` scales NPC training and the recruit price from the world's `recruit_cost`.

NPC damage profiles use the supported 1–20 dice and modifier range, and authored melee profiles must remain valid over the declared ST/fatigue table. Displayed NPC basic-damage means apply minimum damage to each possible roll before averaging, including negative modifiers. The comparison consumes no random state. Reordered profiles, missing guaranteed starter/shop definitions, overflowed spawn weights and nonfinite values fail validation with the named profile.

Bulk sales now preflight the complete wallet transaction before selling any item; exceeding the wallet's integer limit cannot leave a half-sold batch. Their quote uses one inventory pass with a precomputed set of protected loadout/preset/reservation references, avoiding repeated full-armory searches on the native screen.

History retains every newly settled outcome through parent-only worker outboxes; the existing64MiB save ceiling remains explicit. Charts show the last 40 matching records without deleting older ones. DPS is resolved injury over five simulation seconds, zero-padded initially, with whole-run average/peak and defender/structure/ordnance totals; playback speed never multiplies it.

The main incremental route returns every completed main round to Journey upgrades, then explicitly continues/retries or leaves with a durable save. New game protects an existing full generation before replacement. See [JOURNEYS.md](JOURNEYS.md) and [CHARACTER_COMPONENTS.md](CHARACTER_COMPONENTS.md); the individual NPC attribute/skill advancement described above remains separate.

Legacy allocation refunds must reject atomically if removing their ownership floors would orphan an explicitly learned node. No CP/coin/rank/ledger change or free ancestor grant occurs; successful unrelated legacy refunds retain their old semantics. This does not make coin-trained or already graph-spent ranks refundable.
