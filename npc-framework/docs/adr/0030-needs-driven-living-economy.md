# ADR 0030: Needs-driven living economy

- Status: Accepted
- Date: 2026-11-01

The existing island has physical stock, wallets, crafting and life trees, but its prices and production ceilings are fixed. That creates useful demonstrations without a general needs-to-production feedback loop.

Add a separate `living_island` world preset with `EconomyModel::NeedsDriven`; the existing `Fixed` default is omitted from old serialized presets. New market, planning, wildlife and warmth registrations are opt-in before content freeze. Fixed scenarios remain available and their protected evidence is not rebaselined. A new living world requires its own fresh save/content hash.

Use isolated island content modules through the existing facade. No generic crate names island roles/materials, and this increment does not introduce a core capability or modify protected architecture/lint policy. The market arithmetic and production planning stay deterministic integer data algorithms with stable ordering, bounded histories and hashed/saved state. Their future extraction should follow a separate portability decision.

[BazaarBot](https://github.com/larsiusprime/bazaarBot), copyright 2013 Lars A. Doucet, is MIT-licensed and is based on [Doran and Parberry, Emergent Economies for Role Playing Games](https://ianparberry.com/pubs/econ.pdf). Its shortage/surplus offers, trading feedback and history motivate this independently written adaptation. We retain physical travel, inventory, tools and cash settlement; we do not copy its floating-point/shuffled matching, fake initial trading history or bankrupt-agent replacement. This is not a claim of an identical auction implementation.

Forecast terminal survival and cold-protection demand, propagate it through a bounded acyclic recipe graph and credit physical stock once. Separate desired procurement from goods to produce. Refresh that plan before market rounds, then choose jobs using public prices and actual affordability/readiness. Financed demand and unreserved surplus drive bounded price learning; actual trades feed volume/turnover history. Observation and rejected requests never affect prices. A finite community fund may circulate existing money; visitors receive no automatic stipend.

All external mutations remain recorded commands. Existing movement, crafting, equipment and inventory operations stay authoritative. New hunting is finite, timed and claimed: successful completion consumes a real spear and prey, while later habitat replenishment creates a new identity. Actual hunger consumes food; actual cold consumes equipped garment durability. Multi-stage goods therefore have use beyond accumulating stock.

Buy/sell limits protect against quote changes at application. Player listings are backed by real unreserved goods and do not transfer items remotely. Public market aggregates and current offers are intentional market information; private wallets, notebooks and unseen actor positions are not exposed. Clients discover goods and jobs through the same action/observation surface, then physically meet traders.

Allocate money to urgent consumption, one feasible complete production bundle, then future household reserves. Preserve latent demand for an unaffordable necessary bundle using conditional, jointly cash-backed input bid limits. Lower bids can reduce an unaffordable reference price; they do not count as executable upward price pressure or guaranteed escrow. Credit owned materials and internal intermediate production without charging an imaginary finished-product purchase to its own maker.

Separate job valuation from cash admission. Public book/reference input costs retain their opportunity-cost role when ranking jobs. A genuinely financed job with owned inputs is admitted according to actual missing-input cash cost; its producer does not pay again for the same held materials at their new marked value. This applies both to self-production and to physical sales meeting another resident's backed need. Require real demand, inputs, work and settlement throughout. A negative standalone book margin alone must not stop ready owned-input work, and this rule adds no gifts, loans or forced production quotas.

Balance material throughput before tuning purchasing behavior. One garment has 96,000 cold-tick durability and wears one point per exposed tick. At the default 24,000-tick day, thirteen actors need 3.25 garments per continuously cold day, below the four-garment hide capacity of eight renewing prey. A regression derives this inequality from actual population, habitat and recipe quantities. It is a capacity check, not a claim that tools, travel or other inputs are free.

Validate recurring use beyond opening stock: an unchanged 192,000-tick simulation must produce and sell new garments after the first configured lifetime, replace a destroyed garment with a different equipped item, retain twelve living residents and conserve currency. Keep physical-host and full-workspace verification evidence distinct from this focused simulation check. Results live in [the validation note](../_notes/2026-11-01-living-economy-validation.md).
