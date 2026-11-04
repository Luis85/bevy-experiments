# Living Island Village

Choose **Living Island Village** (`living_island`) when creating a fresh human explorer world. It adds a needs-driven economy to the island: twelve self-employed residents gather, hunt, manufacture, buy inputs and offer physical goods. The existing **Island Village** preset remains the fixed-price scenario.

For the expanded island and player-owned cottage/garden loop, choose the new **Living Island Homestead** (`living_homestead`) preset. It installs alongside older world files, while their saved compact construction profile remains intact. See [Your Living Island homestead](living-homestead.md).

For a standalone view, run inside `npc-framework/`:

```powershell
cargo run --locked --release -p actor_testbed -- --scenario living_village
```

For a persistent headless actor, use a new disposable folder/name while testing:

```powershell
cargo run --locked --release -p npcctl -- serve --explorer-home C:/NPC/living-playtest --agent Herp --world Island --preset human --world-preset living_island
```

The normal native AI client uses the selected world's briefing and registered game tools. Starting a provider remains an explicit operator action; this economy increment does not start one automatically. The browser follows the same native/headless export and remains read-only.

To stream both views, use one native explorer host in place of the headless host above:

```powershell
cargo run --locked --release -p actor_testbed -- --explorer-home C:/NPC/living-playtest --agent Herp --world Island --preset human --world-preset living_island --world-state-out C:/NPC/living-playtest/world-state.json --world-state-interval-ms 500
```

From the sibling `npc-world-viewer/` folder, run `npm run dev -- --state C:/NPC/living-playtest/world-state.json` and open the displayed local URL. Start Codex or Claude from the native **Your AI agent** controls when ready. Both views then observe that one simulation; the browser does not create another hero or advance its clock.

## Needs drive work

Hunger and thirst generate food/water targets. Upcoming cold and actual Warmth generate clothing targets. Real recipes and hunting determine the materials and tools needed to meet those targets. Stocks and outputs already being made are credited once, rather than ordered repeatedly. A satisfied need stops generating additional industrial work.

NPCs protect actual urgent survival first, plan complete input bundles from real stock/cash, and distinguish future household wishes from funded purchases. Producers can make for themselves using owned materials without paying an imaginary finished-product purchase price. These are planning allocations, not money creation, credit or remote inventory transfers.

When current prices make an entire needed bundle unaffordable, bounded conditional bids communicate its actual cash ceiling to suppliers. The planner reserves complementary inputs together, rather than spending the entire wallet on one ingredient. An intermediate stage needed for an actor's own final good can remain useful even when selling that intermediate alone would lose money.

Current reference prices still rank jobs, but owned materials do not require a second cash payment when work begins. A carpenter with real planks can complete a funded bowl order even when those planks' marked value exceeds the bowl's sale price. Admission uses actual missing-input cash cost while retaining real demand, work duration and physical settlement. This keeps replacement chains running after initial stock is consumed, without adding gifts, loans or production quotas.

The new hunter Rhea, tanner Sage and tailor Lina join Ada, Rowan, Mira, Nell, Oak, Jun, Bram, Ivo and Tess. Professions define their normal production work; visiting actors can use all the same gathering, hunting, crafting, equipment and trade actions without taking a profession. Visitors start without coins and receive no automatic stipend.

## Physical production chains

| Finished good/use | Inputs and earlier stages |
|---|---|
| Bread restores Hunger | Gather grain → mill flour; combine flour + clean water + wood at the bakery |
| Cooked meals restore Hunger | Hunt deer with a spear → raw meat; cook meat + clean water + wood |
| Warm clothes protect Warmth | Hunt → hides → tan leather; gather fiber → spin thread; sew leather + thread |
| Hunting spear | Gather ore/wood → smelt ingots + saw planks → forge a spear |
| Tanning bowls | Gather wood → saw planks → carve bowls; tanning consumes a real bowl |

Recipes publish their exact quantities, work duration and required workstation in observations and in the observer's workplace inspector. They use the framework's ordinary crafting, material reservations and inventory checks. No recipe produces goods before work completes.

Hunting takes 240 simulation ticks and requires a carried, unreserved spear and a living wild animal within 1.5 m. It claims both prey and tool. Success consumes that spear and animal and creates two raw meat and one hide; failure/cancellation creates no loot. Finite habitats replenish missing animals at day boundaries with fresh identities and distinct free grazing positions. Fiber and berries use the same finite gathering action as grain, ore, wood and water.

Raw meat must be cooked. `eat` defaults to bread; `kind` can select `village_cooked_meal` or `village_berries` in this world. Cold nights/winter reduce Warmth. Equip `village_warm_clothes` in the body slot: carrying sale stock provides no protection. Equipped garments have finite durability and wear during outdoor cold exposure. Home/paid-bed recovery offers shelter; ground rest is slower and exposed.

A new garment lasts 96,000 exposed cold ticks: four default simulation days. At continuous cold exposure, thirteen actors need 3.25 replacement garments per day. The eight-animal habitat can supply hides for four garments per day through the actual tanning/sewing recipes. This gives production a feasible material budget; obtaining the tools, fuel, water and thread still takes real work and trade.

The preset begins in late autumn so preparation and the approach of winter matter during play. All time is simulation time; the host controls its advance.

## Trade and player offers

Observe `village.economy.market` for current published prices, offered quantities and financed demand. Prices retain integer milli-coin learning precision, settle in whole coins, and adapt only during simulation market rounds using actual trade and cash-backed offers. An affordability cap is conditional buying capacity, not a promised escrowed order. Full bounded history remains available in the operator export; the default agent observation omits that history to keep repeated tool responses compact.

All trades require physical reach, a living counterparty, real unreserved goods, capacity and sufficient cash. NPC buyers accept their actual planned purchasing need and reservation price; published wishes alone are not an unlimited purchase guarantee. Rejections and read-only requests never train prices.

Use MCP `act` or the registered CLI action interface:

- `gather`, target a perceived resource patch.
- `hunt`, target a perceived living deer, carrying a spear.
- Advertised `craft:<recipe>`, target its known workstation.
- `equip`, target your carried clothes.
- `buy` / `sell`, target a nearby trader and supply `kind` / `quantity`. Optional `limit_price` is maximum unit price for buying and minimum for selling, checked again when the command applies.
- `offer_goods`, supply `kind` / `quantity` and optionally a minimum `limit_price`. It lists actual carried goods for other actors to purchase; listing alone pays nothing.
- `withdraw_offer`, supply `kind` to remove your listing.
- `stop_work` cancels gathering, crafting, hunting or rest and releases work claims.

For example, after crafting a real spear:

```json
{"verb":"offer_goods","args":{"kind":"village_spear","quantity":1,"limit_price":1}}
```

The offer can survive save/resume, but a buyer still has to need it, afford its quote, travel to the seller and complete the exchange. Inspect your coins, inventory and sale receipt before assuming it sold. NPC purchases use the same quote and ownership rules.

Current affordances replace earlier offers. Remembered workplaces describe static products and accepted goods; they are useful search destinations when a trader has moved, not live tracking. Reacquire a sensed trader and check the actual nearby offer before trading.

Hunting outcomes, listing changes, worn-out clothing, rest completion and sales produce closed game-event summaries for the connected AI scheduler. Summaries contain no private notebook text, credentials or arbitrary game instructions.

## Observer and compatibility notes

Native captions show the current physical job; deer and new gathering patches have distinct visuals. The browser exposes warmth/clothing, recipes and a published market table. Both read the same authoritative simulation and cannot create goods or update prices by rendering.

Create a **fresh living world** for this increment. Its new registrations/state intentionally have a separate content hash. Fixed-mode scenarios remain available; intermediate development saves are not a migration contract. See the [implementation plan](superpowers/plans/2026-11-01-living-economy.md) and [design decision](adr/0030-needs-driven-living-economy.md).

This is a physical spot-market village foundation: jobs are producer professions, not salaried employment contracts. It does not add loans, property markets, remote auction settlement, automatic bankrupt-agent replacement or a full moving predator/herd ecosystem. NPC survival under ordinary/shock test conditions is measured; endless survival after arbitrary player destruction or hoarding is not promised.

The focused unchanged-world replacement test now spans 192,000 ticks and requires new late garment production/sales and replacement of a destroyed equipped item. The final release rebuild and unchanged 192,000-tick physical-host probe are green, with ongoing late trade and real garment replacement. The final full workspace gate passes with `ok:true` and 1,695 tests passed; the earlier legacy-status failure was corrected while preserving its original assertion. See [validation evidence](./_notes/2026-11-01-living-economy-validation.md) for current results.

The design is independently inspired by [BazaarBot](https://github.com/larsiusprime/bazaarBot) and [Doran/Parberry's research](https://ianparberry.com/pubs/econ.pdf), adapted to this project's integer simulation and physical world.
