# Living economy implementation plan

Date: 2026-11-01. Objective: an island where NPC jobs, materials, production and trade respond to actual needs, and a player/AI actor can earn a living through the same physical actions.

## Research and constraints

BazaarBot ([Agent](https://github.com/larsiusprime/bazaarBot/blob/master/bazaarbot/Agent.hx), [Market](https://github.com/larsiusprime/bazaarBot/blob/master/bazaarbot/Market.hx)) and Doran/Parberry's [paper](https://ianparberry.com/pubs/econ.pdf) motivate shortage/surplus offers, adaptation from trade and unmet demand, and bounded history. This is an independent integer implementation. It retains physical movement and settlement rather than copying remote auction transfers, floating-point arithmetic, shuffled order or agent replacement. BazaarBot's MIT license/copyright remain with its source; attribution is recorded in the ADR.

Preserve deterministic sorted SimIds, integer state, host-owned ticks, recorded client commands, private notebooks and protected non-island oracles. No dependency upgrade, new core capability or generated lint/policy change is required. Implement the market/production algorithms as isolated content modules using existing facade APIs.

## Increments and ownership

1. **Mode and construction — primary.** Add opt-in `economy: needs_driven`, the `living_island` preset and `living_village` scenario. Legacy fixed-price presets/scenarios remain available. Append hunter/tanner/tailor and new resources after incumbent spawning. Add cold/Warmth and actual equipped garment wear. Begin in late autumn to make seasonal preparation relevant.
2. **Market engine — action_parity.** Save/hash bounded market history, financed demand, physical offers and adaptive integer prices. One quote path serves NPCs, client affordances and settlement. Buy/sell limit prices are checked again at application. Player `offer_goods` and `withdraw_offer` post real carried goods; NPCs can purchase those goods physically. No currency/item creation, remote filling or price manipulation by observation/rejected-action spam.
3. **Needs-driven jobs — npc_life_tree.** Save/hash a bounded production plan. Forecast food/water needs over two simulation days, clothing for upcoming cold, and tools/materials needed for actual work. Propagate demand backward through an acyclic recipe graph, credit stocks once, and allocate deterministic physical jobs. Retain book/reference margins for ranking while admitting genuinely financed owned-input work on its actual missing-input cash cost. Production stops when demand is met. Use real shared gather/hunt/equip/craft/trade actions and current quotes, preserving external control and emergency survival priorities.
4. **Resources and chains — environment_contract.** Add finite huntable wildlife, habitat renewal with fresh identities, fiber and berries gathering. Chains join grain/flour/bread and water/logs with hunting/meat/meals and hide/leather + fiber/thread → warm clothes. Smithing supplies finite hunting spears; tanning consumes physical bowls. Hunting reserves prey/tool, takes time, rechecks reach/capacity and cannot duplicate loot on cancellation/contention.
5. **Player/observer integration — primary.** Expose perceived hunt/gather actions, safe market aggregates, current quotes/limits, own listings, recipe requirements, production jobs and warmth. Preserve fog/privacy. Native/browser observers explain current jobs, prices and wildlife using the existing visual language. Document creating a fresh Living Island Village and participating through MCP/CLI.
6. **Verification and polish — all, primary owns final gate.** Write failing regressions first, then focused checks for conservation, scarcity/oversupply price response, demand propagation, production stalls/recovery, hunt contention/cancellation, clothing use/wear, player listings and snapshot determinism. Run multiple-seed long soaks and resource/supply shocks, recurring garment replacement over two configured lifetimes (192,000 ticks), actual registered HTTP/MCP production/market rehearsals, release builds and bounded background native/browser captures. Finish with `cargo xtask verify --pretty`; never change protected baselines to get green.

## Acceptance

- Needs create demand; demand creates input procurement and production jobs; real completed outputs become market stock/offers.
- At least two joining, multi-stage chains run end to end with actual downstream consumption/use, including hunting and gathering.
- NPC and client settlement use the same prices, limits, capacity and ownership rules. Listings cannot sell absent/reserved goods or fill twice.
- Prices respond to financed scarcity, supply and completed trades while remaining positive and bounded. Read-only requests and failed requests never train prices.
- No magic restock, item/currency injection, dead animal resurrection, arbitrary NPC respawn or hidden provider dispatch.
- Ordinary runs sustain the resident economy and offer paid work to a zero-cash visiting actor. Shocks produce observable shortage/recovery rather than silent deadlock.
- Funded work with real owned inputs continues on actual missing-input cash cost even when marked book costs exceed the immediate output bid. No loan, gift or forced industrial quota substitutes for that work.
- A mature unchanged world produces and sells replacement garments after the first lifetime and equips a different real item after an older garment is destroyed, with residents alive and currency conserved.
- Same seed/commands and save/resume produce equal hashes. Legacy fixed-mode behavior and protected evidence remain intact.
- Final report states exact checks, visual evidence, skips/warnings, fresh-world compatibility and any remaining limits. No paid AI CLI run or provider-history inspection is authorized in this chat.

## Maturation closeout status

The first full gate returned `ok:false` on legacy client-status formatting; its original assertion is preserved and all eight focused status tests now pass. A longer pre-fix physical-host run also exposed stalled clothing replacement. The owned-plank `CARVE` regression reproduced that admission bug before its fix, and the current production/market/survival/replacement batch is green, including an unchanged 192,000-tick simulation with real late garment production and sales.

Final focused Clippy is clean; the registered Herp rerun passes with 23 coins of income followed by an earned-income water purchase/drink; and the all-five release rebuild passes. The unchanged rebuilt release-host probe is also green to 192,000 ticks: twelve NPCs remain alive, late traded units increase 417→655, food units 44→77 and garment sales 11→13, with destroyed original garments replaced on Mira, Ivo and Lina. No state, money, item or provider intervention funds those outcomes, and the owned host exits cleanly.

All six increments and the closing improvement/polishing pass are complete. The final `cargo xtask verify --pretty` gate is **`ok:true`, exit 0**: 1,695 tests passed, 12 skipped; documentation 1 passed/10 ignored; architecture 7; replays 8; golden snapshots 6; dependencies pass. Nextest reports 1 slow and 4 leaky outcomes; no optional tool was skipped. All 501 candidate source files retain the recorded final fingerprint. The earlier internal-disjunct negative control remains historical evidence, not causal proof of the final admission policy. Exact artifacts, final candidate fingerprint and limitations are maintained in [the validation note](../../_notes/2026-11-01-living-economy-validation.md).
