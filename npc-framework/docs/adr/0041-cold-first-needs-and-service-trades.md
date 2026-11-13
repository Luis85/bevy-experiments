# 0041: The lowest critical need first, and service trades

Date: 2026-11-12. Status: accepted for the living world (`living_world`).

## Context

In the shipped living world a resident without a coat could freeze to
Warmth 0 while buying food: the resident tree served Hunger and Thirst before
the cold, and a coatless resident started outdoor errands in snow at any
Warmth. Separately, the inn's keeper had no output, so the margin-based
labour choice moved every founding innkeeper to another trade and the inns
stood empty.

## Decision

**Need ordering is data.** The resident tree (`trees/resident.json`) opens
with a `Freezing` guard on `life.freezing`: Warmth below
`tuning.freezing_warmth` without a coat, unless Thirst or Hunger is below
`critical_need` and lower still. Its leaf is `life.warm_up` (always starts),
so it may abort lower-priority work. The critical Thirst and Hunger branches
follow. Together they serve the lowest critical need first. Later agents add
a new survival need the same way: a guard whose condition yields to any
lower critical need, placed in priority order in the tree, not as a special
case in an action.

**Exposure is a rule of the leaf boundary.** In the cold (night, winter or
snow) a resident without a coat starts none of the outdoor errands listed in
`resident::actions::OUTDOOR_ERRANDS` while Warmth is below
`tuning.exposure_warmth`. Buying food, water, fuel or a coat, eating, resting
and going home are not errands. `life.idle` waits at home instead of in the
open. From `tuning.coat_lead_days` before winter a resident without a coat
buys one (`life.coat_season`), the civic fund paying the price it cannot
(`VillageCoatGrant`), and tailors' coat target covers everybody without one.

**Service trades.** A profession with `service` in data (the innkeeper) is
not chosen by output margin. A service building with nobody is staffed by a
resident of its settlement who may take the trade, from a trade with more
than one worker at its workplace first. Its keeper stays while the building
stands. Guests' fees are its takings; once a day the civic fund tops them up
to `service.wage_floor` while the building is the only one of its trade in
its settlement and the fund may spend it (`VillageServiceWage`). An unstaffed
inn takes no guests, as before. A tired or cold resident at least
`tuning.inn_detour_m` from home rents a bed at a staffed inn with a free bed
that is at most half as far, when it can pay the bed and keep its destitution
floor: inns serve people caught far from home. In the living world the bed is
rented at the inn's entrance and the keeper is paid wherever it is (it is not
at the inn at night); renting from a keeper met elsewhere had guests sleeping
in the open beside it. Roles stay names in data; no
enum variant is added for a service.

**A coat grant buys a coat.** The fund pays a coat grant only when the
purchase starts; the grant is journaled then and otherwise the coins go
back. It may draw on the fund's destitution reserve (it is a survival grant)
but never on coins the building levy raised for sites. In the coat season a
resident whose plan listed no coat (it had no coins at the last refresh)
buys one directly.

**The market is not the trading post.** The port founds a market stall near
its centre (`merchant.market`, data) where the boat's imports are laid and
sold, and the trading post at the berth takes only what the boat buys.
Shopping at the waterfront 90 m from the homes cut meetings by about a fifth
and with them the couples and births of a year (16 against 32 births over
seeds 7, 11 and 23). Storage is not restocked into the trading post. What
the civic fund cannot pay for is laid on the market on consignment
(`merchant::MERCHANT_OWNER`): whoever buys it pays the boat's purse, so the
boat trades in seasons when the fund is empty, and it never buys its own
goods back. The boat does not buy fiber or thread, the tailors' winter stock.

**Labour does not chase idle trades by default.** A worker whose trade had no
work could leave it after `tuning.idle_trade_days`; with 2 days the 14-day
soak saw 319-364 job changes against 62-95 with the rule off, so it ships off
(0). Headcount follows work through the margin rule and the service staffing.

**Goods find the trade that uses them (pass 4).** A resident with no use for
a good (`market::no_use`: not food, water, clothing, its hearth's fuel or its
trade's tool, and its trade neither makes nor uses it) carries it whole to a
store, which sells it (`tuning.unused_goods_to_market`). The rule is not part
of the per-person seller test, which every market scan runs. A former
tailor's thread no longer sits in a miller's pack while the tailors idle.

**Coats are made to stock in every season.** While anybody of a settlement
lacks a coat, each coat maker keeps a shelf of `max(2, residents /
tuning.coat_shelf_residents)` coats, financed from its own purse: production
demand, not a grant. Outside the coat season a resident without a coat who
cannot pay one on offer is granted the price only from the fund's balance
above its destitution reserve (`tuning.coat_grants_any_season`,
`living::coat_grant_room`); in the coat season the grant may draw on the
reserve as before.

**The boat orders ahead.** As the boat leaves it announces what it will buy at
its next call (`merchant.order_goods` goods of `merchant.order_units` each,
at `merchant.order_price_percent` of the island's quote, at least one coin
above it), rotating over the goods of its buy list that a trade of the island
makes, never fiber or thread. The order is financed demand in the port's
production plan, like a homestead order, and is paid from the boat's purse at
the trading post; what is not delivered expires as the boat leaves, with
nothing reserved and no coins held. The boat's share of the island's trade is
measured from the second season on: the founding season carries the founding
trade burst (about twice a later season's trade).

**Floor grants leave the sites their coins.** The daily destitution-floor
grants may not take the coins the fund holds for open building sites
(`civic::fund_hold`); when the floor needs more, the relief levy raises it
from coins above the floor. Sites spend `civic::site_budget`: the budget, or
what is held for them, whichever is more. The grants had grown to about the
whole daily levy, so the fund sat at zero and sites waited for weeks.

**Wildlife moves (pass 4).** In the living world every animal carries a hashed
`WildlifeMotion`: it grazes, walks to a spot inside its habitat's home range
(`wildlife::RANGE_MM`), keeps `wildlife::SPACING_MM` from every other animal,
stays on open land (walkable, not water, no field or building), gallops away
from a resident within `wildlife::FLEE_REACH_MM` and settles; after a flight
it stays calm long enough for a hunter to follow, and an animal being hunted
stands still. A moving animal steps every `wildlife::STEP_TICKS` ticks.
Timings come from `sim_rand`; animals are processed by `SimId`.

**Settlers do not land naked a week before winter (pass 5).** A resident
who carries a coat and wears none (or a worn-out one) puts it on at once, in
any season (`living::wear_carried_coats`, `VillageCoatWorn`). Two founders in
three, in founding order across settlements, households and trades, come
wearing a coat (`tuning.founder_coats_per_three`). Each settlement is founded
with its coat makers' workshop built and each coat maker holding one batch
of the first coat recipe's inputs (`tuning.found_coat_workshops`).

**Standing orders are demand for the trade, not a reason to join it.** The
boat orders 3 goods a call (`merchant.order_goods`). Its purchases against an
order are not booked as island trades: they move neither the island's
quotes nor the realised margins that pull residents into a trade. The coat
shelf is a production target of the coat makers only. Neither by itself
triggers a trade change.

**The hunter stalks.** A hunter keeps the animal its hunt intent names while
it is still prey, and re-paths to it only once it is more than 3 m from
where the hunter is walking. That animal does not flee from that hunter; it
flees from everybody else, and from a hunter hunting another animal.

## Consequences

- The living world's content hash changes (tree, tuning, professions,
  `Employment` fields).
- Legacy profiles are unaffected: none of these conditions, rules or fields
  is registered or read there.
