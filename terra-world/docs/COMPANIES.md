# Transport companies, recovery and warehouse commerce

Terra's company layer connects owned or prepaid rental vehicles, physical road
movement, finite passenger queues, production inventories, public service funds,
employee payroll and private depots. Version 0.9 introduced private warehouse
stock on physical commercial routes and nearby shop trading. Buyers fund
actual goods purchases; source refund space and private arrival space are reserved
while goods travel. Loaded return/handover, employee departure and finite household
spending remain connected; no command creates money, replacement cargo or passengers.
Employees remain persistent driver contracts rather than separately animated NPCs.

## First company

Open **Fleet [Y]**. **Company**, **Finances**, **Drivers**, **Routes**, **Recovery** and **Depot**
share the same authority command boundary as multiplayer clients.

1. Walk within 20 metres of a garage. Name and register a company for **20 personal
   credits**. One company belongs to one player.
2. Deposit personal funds into the company account. The transfer selector offers
   10, 50 or 100 credits; 50 credits funds recruitment and initial payroll.
3. In **Drivers**, hire a named road, passenger and heavy-goods employee for
   **10 company credits**. Registration and recruitment payments go to the finite
   service treasury.
4. In **Routes**, use **Draft a local passenger loop**, edit its name or stops and
   create it. Town interchange → Hillside interchange starts with real waiting
   queues. A garage with no passengers supplies no immediate passenger job.
5. Exit the pickup, select the walking avatar, choose an employee, free vehicle
   and saved route, then assign them. The vehicle must be stopped and empty, with
   no other job, race or company assignment. A rental needs prepaid time.
6. Start the route. Physical arrival loads real passengers or goods. Public work
   reserves an issuer-funded fare; warehouse sales reserve the buying shop's
   money. Actual travel, arrival and unloading earn the reserved payment.
7. Retain company income for payroll or withdraw it into personal cash. **Pause
   safely** completes the current loaded delivery before parking; **Release
   parked empty vehicle** returns it to personal driving and jobs.

Opening a depot or renting another vehicle is optional. A new player can operate
the starter pickup company with the authored 250 personal credits; no developer
fund injection is required. A higher-class rental still requires its license.

## Review a route before spending

**Routes** shows a read-only economic preview for the draft. **Drivers → Before
dispatch** shows the selected saved route. Both describe the **first departure**:
the first two stops for passengers, or the first and last stops for freight.
They do not describe an employee's already loaded trip or promise that every leg
will earn the same amount. The employee's actual load and reserved payment remain
visible in its status above.

The preview shows the requested load, current visible pickup stock or queue,
the receipt for a complete load at current prices, and the destination's visible
stock and restocking target where applicable. A full target explains why another
destination or actual consumption is needed. Competing loads can reserve some
of the apparent demand; hidden source or buyer records are **unavailable**, not
assumed empty. Mixed-item depot capacity is not inferred from one item's count.

For warehouse sales, the preview names the cheapest currently visible retail
unit price with stock and compares it with the buyer's current purchase price.
It states the seller's available quantity; a seller with one item is not a
promise that a whole load can be bought. This is a comparison of visible prices,
not a global cheapest-source search or a reachable depot-trade quote. Visiting,
funding and storing a purchase still use the ordinary checks and the displayed
quoted transaction.

For example, eight water bought at **0.80 credits each** cost **6.40 credits**.
A buyer paying **0.30 each** returns **2.40 credits**: a **4.00-credit shortfall
before payroll or fuel**. A better route cannot recover that merchandise spread
by itself. Cost an actual harvested or crafted source, choose other funded work,
or wait for a suitable buyer; contributed goods and player time do not become
free inputs because their price is unknown.

The gross receipt divided by **1.20 credits per physical minute** states how
much payroll that receipt alone could cover. It is an upper bound **before all
other costs**, not an estimated trip duration or a profit forecast. Include
travel to pickup, empty freight returns, traffic delays, acquisition, fuel,
lease/vehicle costs, recruitment and registration in the decision. Bay transfers
explicitly earn no receipt while retaining payroll and fuel costs.

Public fares require finite issuer funding; warehouse payments require actual
buyer money. The preview does not inspect private issuer cash, reserve payment,
authorize dispatch or lock a price. The authoritative pickup/delivery rules
still decide what can be loaded and paid. Review actual cash results in
**Finances**, including known owner-paid costs and unpriced contributions.

The SDK API is `SandboxState::company_route_preview(owner, stops, service)`.
It derives a `CompanyRoutePreview` without mutating the world or adding saved
progress. [Planning regressions](../crates/terra-sandbox/tests/company_planning.rs)
cover the retail shortfall, exact first-leg fare arithmetic, private issuer
funding independence, changing prices, insufficient visible retail stock,
unknown versus known-empty inventories, no-fare transfers and rejected route
shapes without state changes.

The passenger quantity selector includes **three seats** for a full compact or
pickup and **30 seats** for the existing bus. Choosing a load still does not
override a selected vehicle's actual seat capacity. A native action regression
creates the three-person draft and assigns a real compact rental through the
same authority commands used in play.

## Cash accounting and the Finances report

**Fleet → Finances** reports the company's recorded cash result, financing and
known personal support. Every company cash movement has an exact persisted
category. The journal also records attributable payments made directly from the
owner's personal wallet, keeping them outside the company cash balance.
It does not invent a price for contributed goods or a personally owned vehicle.

| Recorded movement | Cash class | Payer/recipient and scope |
| --- | --- | --- |
| Paid public fares and delivered/direct goods sales | Operating receipt | Issuer escrow or actual shop money enters company cash only on accepted payment. |
| Stock purchased into a depot | Operating payment | Company cash buys the quoted shop goods. This records acquisition cash, not a per-item cost-of-goods valuation. |
| Payroll, recruitment, fuel, repair and roadside callout | Operating payment | Each company payment is recorded separately; a compound service quote produces separate fuel, repair and callout receipts. |
| Public service contribution | Operating payment | A nonrefundable company contribution to the finite shared issuer. Any later paid fare remains a separate operating receipt. |
| Depot access | Capital payment | The nonrefundable bay-opening outlay is separated from recurring operations; no resale value or public-facility ownership is implied. |
| Owner deposit / withdrawal | Financing receipt / payment | Personal and company wallets transfer exact amounts. Deposits are not sales; withdrawals are not operating expenses. |
| Registration | Owner-paid capital outlay | A new company records its actual 20-credit personal setup payment without pretending it passed through company cash. |
| Fuel, repair, paint and lease renewal while a vehicle is assigned | Owner-paid operating outlay | The personal transaction's known amount is attributed at payment time. It does not also debit company cash. |
| Mechanical upgrade while a vehicle is assigned | Owner-paid capital outlay | The personal upgrade payment is separated from company cash and recurring service. |

All values use integer minor credits. The authoritative reconciliation is:

```text
opening cash + operating net + capital net + financing net = current company cash
cash result before financing = operating net + capital net
cash result after known owner costs = cash result before financing
                                    - recorded owner-paid operating/capital outlays
```

**Known owner funding** is gross recorded deposits plus recorded owner-paid
outlays. Withdrawals remain visible in financing. Employee household purchases
spend money already paid as wages, so they do not create another company expense.
Reserved fares, buyer payments and rental bonds remain escrow until the existing
delivery or return rules release them; a reservation is not company income or an
expense.

This is a cash-basis record, not all-in profit. It does not depreciate vehicles,
value unsold stock, price player time, or allocate a personal vehicle's purchase,
initial prepaid rental, earlier fuel or later unassigned use. Assignment marks
that a personal vehicle has been used. Physical stock deposits and withdrawals
retain exact aggregate unit counts and visibly unpriced transfers; the journal
does not guess their acquisition cost. Payment while assigned is the explicit
allocation rule for known personal vehicle costs, not a claim that every prepaid
second or repair benefit was consumed by this company.

### Existing saves and retained receipts

A newly created company records **SinceRegistration** coverage. An older save
without `TransportCompany.accounting` remains valid: its report says
**ImportedOpeningBalance**, with current cash and historical revenue/payroll
counters preserved as the opening state. The first successful recorded event
persists that opening and physical world time. No prior expenses, deposits,
registration charge or unpriced assets are reconstructed from the balance. An
imported company's new operating result covers only its recorded period; opening
revenue is not treated as fresh income or proof of earlier profit.
Compatibility means older worlds can be read by the current authority. An older
executable that rewrites a save may discard additive fields it does not know;
reopening that file cannot reconstruct its lost journal and must again report
imported history.

The latest **64 monetary entries** retain sequence, physical time, category,
amount and company cash after the entry. Older detailed entries retire while
exact lifetime category totals and the entry count remain. Totals cover the
stated recording period, not only the visible recent receipts. Save validation
checks the opening, totals, bounded recent sequence, cash, paid revenue and
current/departed payroll against one another. A malformed present ledger rejects;
it is not silently downgraded to an import.

Each lifetime category and the monetary-entry count is bounded at
**1,000,000,000,000**; current wallet bounds still apply. These are exact bounds,
not saturation thresholds. Commands preflight the entire record before changing
cash, stock, fuel or employees. Payroll/delivery that cannot record its payment
holds with **Company accounting limit reached**, preserving cash, fractional
wages, cargo/passengers and escrow. Even owner withdrawal needs a valid receipt.
No balance is confiscated or reset when a bound is reached.

A load already at its original pickup can still refund its issuer/shop directly,
and noncash actions such as closing an empty depot remain available. A distant
return still requires ordinary paid travel, so full payroll/event history does
not grant free movement. Further cash postings require a compatible accounting
limit or history migration that preserves exact prior totals; clearing the journal
or changing the opening to disguise prior activity is not a recovery rule.

## Routes and finite demand

Routes have **2–8 distinct stops**, each consecutive leg, including the return,
**50 metres–2 kilometres** long. Passenger routes request **1–30 passengers** per
leg; freight routes request up to **240 units** per delivery. The assigned
vehicle must fit the route maximum, including any valid attached trailer.

| Service | Physical operation | Limiting resource |
| --- | --- | --- |
| Passengers | Board at a stop, travel to the next and add the riders to its queue; cycle around the saved route. | Real waiting people and vehicle seats. The system moves people between queues without generating them. |
| Public freight | Load a linked public producer/shop at the first stop, visit intermediate physical gates, deliver at the last and return empty. | Actual source goods, destination demand, competing incoming reservations, issuer cash and vehicle/storage capacity. |
| Warehouse sale | Load your private bay, drive to a facility linked to a buying shop and sell the delivered goods at the price reserved at loading. Return empty. | Your stock, the shop's actual cash and restocking demand, and vehicle capacity. No public fare. |
| Warehouse transfer | Load your private bay and deliver to another bay belonging to the same company. Return empty. | Your stock, destination mixed-item capacity and payroll/fuel. This internal stock movement earns no payment. |

A producer requests up to 16 recipe-input batches, capped at 256 units of the
item. A shop requests up to 64 units of each item it sells. Incoming company,
manual-job and courier cargo reduces demand. Returning company loads no longer
reserve space at their abandoned destination. Source goods promised to an NPC
courier remain unavailable to the fleet. Producer inventory has a **2,000-unit
aggregate** capacity, including mixed items; a full destination retains the load
and escrow until space exists.

For public passenger and public freight work, the issuer sets fares in minor credits:

```text
fare = 500 + rounded direct distance in metres + load component
passenger component = 200 × people carried
freight component = 30 × units carried
100 minor credits = 1 credit
```

The issuer debits the fare at pickup and holds it in the trip. Completion checks
elapsed physical time and actual odometer progress; repeated arrival calls cannot
pay twice. A nearby handover carries forward only previously admitted distance
and the original trip clock, rather than manufacturing a new full-distance proof.

## Payroll and employee departure

Dispatched travel costs **1.20 company credits per physical minute**, including
ordinary road delay. Waiting for stock, demand, passengers, issuer cash, repair,
fuel, buyer purchase funds or rental renewal has no payroll charge. Fifteen seconds without physical
progress holds a blocked route and its payroll; persistent intersection queues
can also reach this safeguard. Resume when the route can move again.

`wages_earned` is the employee's current finite money wallet. `wages_spent` records
money already used by their household and is not another wallet. The company
`payroll_paid()` total includes current and departed employee balances and their
recorded spending. `revenue` counts paid public fares and warehouse goods sales,
including nearby direct sales; it and delivery counts are historical, not spendable.
The accounting report reconciles recorded payroll and revenue with their retained
opening counters; neither counter alone is an expense ledger or profit measure.

To dismiss an employee, release their vehicle first, return to a garage and use
**Dismiss unassigned employee**, then confirm. A fractional final minor credit
is rounded up and paid from company cash. Their remaining money moves into the
company's durable `departed_wages` ledger; past spending remains in
`departed_spending`. Dismissal neither confiscates wages nor refunds recruitment.
The freed employee slot can be used for a new hire. Route deletion still requires
releasing every driver assigned to that route.

## Loaded vehicle recovery

The **Recovery** tab always refers to the selected employee's actual assignment.
A selected replacement vehicle is shown separately.

| Action | Result | Value and authority boundary |
| --- | --- | --- |
| **Pause safely** | Finish the current loaded delivery, then park. | Fare is paid only on valid physical delivery. |
| **Hold vehicle here** | Immediately park in its current position and suspend payroll. | Cargo, passengers, route, fare, mileage proof and employee wages remain recorded. |
| **Start / resume** | Resume a held or blocked trip; a return remains a return. | Payroll, fuel, lease and ordinary terrain/body checks still apply. |
| **Return load to original pickup** | Cancel onward delivery and drive physically back to pickup; confirmation explains that no payment will be earned. | Refund the issuer's fare or warehouse buyer's purchase escrow and return the exact cargo or people only when stopped at pickup. A full public source or refund account retains the commitment and retries. Private source space is reserved. |
| **Hand over load to relief crew** | Transfer the commitment to a free replacement vehicle and unassigned employee, then leave the relief driver held for explicit resume. | Both vehicles must be stopped and within 20 metres of one another; the owner's walking avatar must be within 20 metres of each. Capacity and ownership are checked, and neither body changes position. |
| **Release parked empty vehicle** | Clear driver/vehicle assignment. | Loaded or running work cannot be released, sold, rented back or silently abandoned. |

For a handover, hire or free a second employee, bring a suitable replacement
vehicle near the stranded one, exit to the walking avatar, choose the relief
employee and replacement, then submit the handover. The original employee keeps
all wages. The old empty body becomes personally controllable. The replacement
receives the cargo or passengers, original escrow, route cursor, return intent
and accumulated travel proof exactly once. A capacity-limited replacement must
fit the complete route load, not merely one temporarily smaller shipment.

These recovery commands preserve material and money, but cannot guarantee a path
through missing source coverage or impossible geometry. If both onward and return
routes are impassable, a physically reachable replacement may help. A completely
inaccessible vehicle still requires a game-owned world/terrain recovery policy;
there is no hidden teleport, remote cargo deletion or free refund.

### Roadside and depot service

Walk within **20 metres of the stopped assigned vehicle**. **Buy quoted roadside
service** recomputes the displayed quote and checks company funds, account caps,
finite source fuel, ownership and resident body before committing:

- **15 credits** callout, waived only when the vehicle is within 20 metres of the
  named garage where this company owns a depot bay.
- **1.50 credits per litre**, drawn from that garage's actual fuel stock.
- Needed repair costs the greater of 2 credits and 45% of model purchase price
  multiplied by missing condition, rounded up to a minor credit.

The native offer supplies up to 10 whole litres and any needed repair. The domain
accepts 0–500 litres with at least one service requested, bounded by actual tank
capacity. Servicing preserves route, load, clock, mileage, fare and held state.
A deliberately held or blocked driver requires resume after servicing.

This is a paid immediate support abstraction; it does not simulate a mechanic's
journey or a towing rigid body. Depot ownership does not refill fuel or waive the
fuel/repair charge. **Rental expired** instead directs the player to **Garage →
Renew selected rental**; renewal uses personal money and preserves the shipment.

## Private depot bays

The **Depot** tab opens up to **two bays per company** at existing garages or
freight facilities for **25 company credits each**. This purchases gameplay storage access; it
does not acquire the public map facility, its fuel or a political territory.
Other companies may hold their own private bays at the same facility. Garage bays
also waive the onsite mechanic callout; a freight facility does not become a garage.

Each bay stores **2,000 total units**, shared across item types. Walk within
20 metres, leave vehicles, choose an item and quantity, then **Store goods** or
**Take goods**. Transfers move exact amounts between the personal inventory and
private depot stock. Capacity, catalog item, physical reach and available stock
are checked before either inventory changes. Private warehouse goods are never
published through another player's view.

Loaded warehouse work reserves its source's refund space. Transfers also reserve
arrival space at the other private bay. Both count against the mixed-item capacity,
so a later player deposit or market purchase cannot fill a shipment's reserved
space. **Reserved delivery/return space** displays this amount. A returning transfer
releases its abandoned destination reservation while keeping its source reservation.

**Close empty depot** requires an empty bay and physical facility reach, with an
explicit nonrefundable-fee confirmation. Delete warehouse routes referencing the
bay first, after releasing their employees. Closing does not dismiss employees or
discard accounts. Normal player housing/building and territory mechanics remain
separate; warehouse bays are not arbitrary placeable buildings or vehicle factories.

### Stock, market and route workflow

1. Gather, farm, craft or buy the stock you intend to distribute. In **Fleet →
   Depot**, choose the bay, item and quantity, then **Store goods**. The stock moves
   from your personal inventory to the company; it does not copy public producer stock.
2. For a shop beside the bay, **Trade with a nearby shop** shows the actual shop
   stock, cash, current buy/sell totals and any blocking reason. Walk within
   **20 metres of both the bay and shop**, using your avatar. **Buy into depot**
   spends company money and removes shop goods; **Sell from depot** spends shop
   money and removes private goods. The domain supports 1–100 units per trade.
   Authority checks the displayed total again; changed quotes reject without mutation.
3. For a distant buyer, choose **Draft warehouse sale to a shop**. The draft
   selects an owned source and compatible buying shop, then opens **Routes** for
   review. Set its name, item, quantity and any intermediate stops, then create it.
   A usable source and destination must still satisfy the 50 m–2 km leg rules.
4. Assign a stopped empty vehicle and employee, fund payroll and start it. At
   loading, the buyer reserves `shop sell price × actual amount`. Available goods,
   the shop's affordable quantity and restocking need can reduce a batch below
   the route maximum. An empty buyer wallet shows **Destination shop needs purchase
   funds**. Customer purchases can restore its cash and shelf demand.
5. The truck carries real goods and retains the frozen purchase payment across
   hold, return, handover and save/resume. Delivery credits company cash. Returning
   physically restores the source goods and buyer's money without recording revenue.
6. To redistribute internally, open a second bay and choose **Draft transfer to
   your other bay**. It uses the same physical fleet and recovery controls, but
   neither bay nor the public issuer pays the company for moving its own goods.

Warehouse sale destinations are public shops even if the company also owns a bay
at that facility. Warehouse transfers are explicitly private and cannot use a
different owner's stock. Public freight remains a separate service that reads the
linked public inventory. This prevents an owned bay from silently changing the
meaning or payer of an existing public route.

A warehouse is useful distribution storage, not an automatic source of profit.
Buying retail goods and reselling into the same market pays the shop's spread and
travel costs. Gathered/crafted goods still have acquisition costs and finite supply;
route distance, fuel, payroll and buyer demand determine whether a sale is worthwhile.

## Recurring retail demand and service funding

Opening the first depot enables an explicit bounded household abstraction. Once
per **60 physical seconds**, each off-duty employee may buy **one real water or
bread item**, plus a **0.50-credit transit pass**, from a real shop within
**200 metres** of that depot. An employee is off duty when unassigned, or parked
at that depot with no active trip. The departed employee wallet supplies one
aggregate household purchase per interval. Water and bread consumption alternate
according to the smaller recorded consumed count.

This purchase requires earned wallet money, actual retail stock, an available
shop money account, and service treasury capacity. It transfers the item price
to the shop, the pass payment to the finite public service treasury, and the item
to a bounded consumed-material counter. Missing money or goods means no purchase;
there is no debt, automatic shelf refill, extra passenger spawn or treasury grant.
`household_consumed` and `wages_spent` are receipts, not spendable stock/cash.

Shop selection first filters for an affordable complete item-and-pass purchase
and available receiving-account capacity, then chooses the nearest eligible
shop within 200 metres. An unaffordable or capped shop beside the depot cannot
suppress a feasible purchase at another local shop. The same rule applies to
current and departed employee wallets; a blocked public service treasury still
prevents the entire purchase without consuming stock or wages.

Retail consumption creates space for the existing producer → courier/fleet →
shop chains. Shop receipts feed the existing productive-resident wage and trade
systems. Spent employee money therefore circulates rather than accumulating
forever in inaccessible wage balances. Household time, consumption and all company
work freeze while the owner is dormant, with no reconnect catch-up.

The public issuer can still exhaust its capital. **Company → Fund public
services** transfers the selected amount from company cash into that shared
issuer, after an explicit contribution confirmation. It is nonrefundable; the
player is funding public transport work rather than minting new money. Existing
vehicle purchases, rents, licenses, servicing and pass payments also fund the
issuer. This establishes replenishment paths and finite recurring demand, not a
claim of indefinite profitable growth or a fully calibrated macroeconomy.

## Integration, persistence and privacy

The domain is under
[`transport/company`](../crates/terra-sandbox/src/transport/company/mod.rs).
`TransportCommand::Company { action }` wraps `CompanyCommand`. Public command
families cover registration, transfers and public-fund contributions; recruitment
and dismissal; routes and assignment; run/pause/hold/return/handover/release;
depot opening/closure, stock transfers and quote-checked shop trades; and quoted
roadside service. `FleetService::DepotFreight` and `DepotTransfer` distinguish the
two private-stock workflows from public `Freight`.

`prepare_company_drivers(physical_seconds)` runs before movement.
`advance_transport` handles physical arrivals, refunds, rental clocks and bounded
household spending after admitted motion. Hosts must preserve this order and use
actual accepted movement for odometer accounting. Commands validate their bounded
candidate ledgers before publishing a mutation.

`TransportWorld.companies` is authoritative. `TransportView.company` contains only
the recipient's ledger, including depots, households, wages, accounting and commitments.
Additive serde defaults leave old companies without depots, spent-wage records,
return/handover state or `FleetTrip.goods_payment`. That optional payment records
the buying shop, frozen unit price and exact total; save validation checks its
service variant, destination, product and cargo amount. Optional accounting uses
the imported-opening treatment above, retaining the existing domain save schema.
`TransportCompany::accounting_report()` returns `CompanyAccountingReport`: cash
reconciliation; `CompanyCashFlow` operating/capital/financing receipts, payments
and `net()`; separately recorded owner-paid costs; coverage, opening counters and
recording time; exact `CompanyCashCategory` totals; unpriced contributions and
personal-vehicle use; and the bounded `CompanyCashEntry` list. Category `label()`,
`class()`, `owner_paid()` and `receipt()` support presentation without reimplementing
classification. This is an owner-private report, never another spendable wallet.
Domain/save/catalog/source identities remain pinned.
Dormant bodies can retire while their global commercial obligations remain
validated and conserved. Rental bonds are documented in [TRANSPORT.md](TRANSPORT.md).

Limits remain one company per owner, four current employees, eight routes, eight
stops per route, two depot bays, 2,000 goods per bay, and existing global player,
body, vehicle and source limits. Names are 1–32 printable characters/64 UTF-8
bytes. Company and individual wage wallets cap at 1,000,000,000 minor credits;
departed aggregate wallet and historical spending/revenue cap at 1,000,000,000,000.
Consumed counts cap at one billion per essential item. Bounds never discard value
when reached; the affected transfer waits or rejects.

## Verification and remaining scope

[Company tests](../crates/terra-sandbox/tests/companies.rs) exercise physical
passenger/freight movement, blocked payroll, complete returns, atomic denial,
nearby replacement handovers and retained travel proof, final-pay dismissal,
private depot transfers, aggregate capacity, onsite quote changes, finite paid
household demand, service funding, cold restoration and owner privacy. [Rental
tests](../crates/terra-sandbox/tests/rentals.rs) cover expired loaded company work
alongside lease money/fuel/refund conservation. The release validation report
records the actual executed commands and native journeys.

[Warehouse commerce tests](../crates/terra-sandbox/tests/company_depots.rs) add
real market purchases/sales and stale-quote rollback; finite buyer funds and
partial batches; private source/destination reservations; physical paid delivery;
buyer cancellation refunds; dormant cold-region restoration; malicious escrow and
overfilled reservation rejection; and a deterministic six-session finite-stock
career combining physical distribution, retail purchases, payroll, household
consumption, fuel recovery, cancellation and seven save/resume checkpoints.
Its exact measured result is recorded after execution in the release validation.

### Finite career measurement

The 0.10 deterministic `six_session_warehouse_career_conserves_finite_goods_cash_and_recovers_scarcity`
fixture completed the following measured run with shared bounded autonomous
handling. It uses the ordinary physical living/transport update on an isolated
flat corridor, two private bays 100 metres apart and a shop another 80 metres
away. Production is paused to keep its explicit 96-water starting harvest
finite. It is a domain integration scenario, not a rendered terrain journey or
a production-server soak.

| Fixture observation | Result |
|---|---|
| Sessions / physical deliveries | 6 / 24: 12 private transfers and 12 shop sales |
| Save/resume checkpoints | 7, including loaded cold-region restore and payroll scarcity |
| Physical world time | 2,967.45 seconds, including dormant and off-duty intervals |
| Goods sold / customer purchases | 96 water / 96 water |
| Sales / wages / paid roadside service | 28.80 / 15.49 / 18.00 credits |
| Operating cash change excluding owner transfers | **-4.69 credits** |
| Conserved money and material, including consumption | 13,900.00 credits; 224 water; 40 bread |
| Exhausted-source result | WaitingGoods; no income or payroll on repeated idle ticks |

The scenario includes 1 paid finite-stock fuel recovery,
1 cancelled and retried transfer, and
replenishment through a real owner deposit. Earned wages fund household
consumption of 4 water and
3 bread. The physical clock includes
720 seconds of dormant-owner checks and 1,440 seconds of off-duty household time.

The operating loss is a concrete balance finding. Physical turning and
clearance change the time spent working and therefore the payroll due. The
figure excludes registration, hiring, bay fees, harvest acquisition and owner
transfers. This short finite route does not establish profitable long-career
balance or setup-cost recovery, and a retail purchase of the same source water
would cost more than its sale revenue. A game must choose acquisition costs,
productive supply, vehicle scale and route distances for its intended
progression. The test proves continuity and recovery under these finite
conditions; idle waiting, internal transfers and restarting do not create wealth.

The [final career log](verification/v0.10/career-report.log) retains the exact
`CAREER_REPORT` under the release source freeze. The previous 0.9 measurement is
historical and remains in [VALIDATION_v0.9.md](VALIDATION_v0.9.md).

### Paired cash-basis career evidence

The current [warehouse acceptance tests](../crates/terra-sandbox/tests/company_depots.rs)
extend that physical journey with persisted accounting and a second acquisition
case. The first owner transfers an authored, finite **96-water harvest** through
the ordinary depot transfer command; its acquisition cost is unknown. The second
company purchases those 96 water through the quoted shop trade for **76.80
credits**, then follows the same route, demand, recovery and save/resume rules.
Neither case receives stock or money grants after its starting budget is captured.

The focused comparison completed **24 physical deliveries in six sessions** in
both cases, with seven in-career restore checkpoints and a final exact regional
accounting round trip. It conserved **13,900.00 credits**, **224 water** and **40
bread**, including actual household consumption. Both reports retain the same
2,967.45-second physical clock and the same 28.80-credit sale revenue. These are
bounded domain scenarios; final release validation retains its own freshly run
reports and does not infer a production-server or rendered-terrain workload.

| Persisted cash observation | Contributed harvest | Purchased at retail |
| --- | ---: | ---: |
| Goods sale receipts | 28.80 | 28.80 |
| Recorded stock purchase payments | None; input remains unpriced | 76.80 |
| Payroll | 15.49 | 15.49 |
| Purchased recovery fuel / callout | 3.00 / 15.00 | 3.00 / 15.00 |
| Recruitment operating payment | 10.00 | 10.00 |
| Operating cash result since registration | **−14.69** | **−91.49** |
| Depot access capital payments | 50.00 | 50.00 |
| Cash result before owner financing | **−64.69** | **−141.49** |
| Known owner-paid registration | 20.00 | 20.00 |
| Cash result after known owner costs | **−84.69** | **−161.49** |
| Owner deposits / withdrawals | 205.00 / 126.38 | 205.00 / 49.58 |
| Final company cash | 13.93 | 13.93 |
| Lifetime monetary entries / recent entries retained | 1,570 / 64 | 1,571 / 64 |

All table money is in credits. The identical final wallet does not mean identical
performance: the purchasing company returns **76.80 fewer credits to its owner**.
Its complete journal explains the difference without treating a withdrawal as an
expense or excluding the stock purchase that happened before the delivery window.

For the contributed-harvest route, sales less payroll and purchased fuel leave
**10.31 credits** before the callout, recruitment, depot access, registration and
unpriced inputs. The fixture deliberately starts the vehicle with **0.04 litres**
to exercise a funded stranded-load recovery; its one **15.00-credit callout** turns
that delivery-window cash result into **−4.69 credits**. This is a recovery test
condition, not evidence that ordinary service requires a callout on every loop.
The fuel payment buys two actual litres, including any fuel still in the tank;
the cash report does not invent a consumed-fuel valuation. Recruitment, both bay
fees and registration add **80.00 credits** of known setup outlay to that window.
Their separation lets a player distinguish startup and exceptional recovery
costs from the observed receipts and paid work.

Retail acquisition is already losing before travel: **96 × 0.80 = 76.80** paid
for stock versus **96 × 0.30 = 28.80** received, a **48.00-credit merchandise cash
shortfall**. This is the existing retail/wholesale spread, not a routing bug or a
reason to change prices until a test becomes profitable. Gathering/crafting or
productive supply may change acquisition cash, but the harvest case does not
price player labor or inputs. Both cases use a personal vehicle whose original
capital and prepaid fuel are unpriced. Neither result is all-in profit or proof
of a sustainable long career; representative supply, demand, utilization and
career duration still need broader balance acceptance.

`CAREER_REPORT` retains the contributed-harvest result and
`PURCHASED_CAREER_REPORT` retains the purchased-stock result. Both include the
serialized `persisted_accounting` report. `source_acquisition_minor_credits` is
**null** for unpriced harvest and **7680** for the actual retail purchase; null is
not a zero-cost assertion. Their original delivery-window fields remain for
comparison with historical evidence, while the full report includes setup,
acquisition and owner support from the company's registration onward.

Run the evidence with:

```sh
cargo test -p terra-sandbox --test company_depots --test companies --test company_accounting --test rentals --offline --locked -j2 -- --nocapture
```

The run prints `CAREER_REPORT` with exact account transfers and measured totals.
Additional cases retain a capped buyer's refund and cargo until an actual retail
sale opens money-account capacity, and move the loaded buyer commitment and space
reservation to a relief crew exactly once. Failed buys/sales at stock, payer and
account caps leave the complete world state unchanged.

[Accounting regressions](../crates/terra-sandbox/tests/company_accounting.rs)
cover missing-field legacy import, malformed present history, receipt retention,
category and cash reconciliation, personally paid assigned-vehicle service and
lease renewal, and atomic rejection at lifetime bounds. A compound roadside
transaction fails without partial payment even when only its first receipt fits.
Payroll holds before accruing a fractional liability at a full event limit;
pending delivery retains passengers and escrow. An independent refund already at
pickup still completes, and a historical revenue limit does not block legitimate
stock purchases. Existing loaded-rental and dismissal tests also reconcile new
lease/final-pay receipts without reclassifying bond escrow or legacy wages.

Still outside this reference slice: playable employee contracts, separately
animated staff households, insurance and tax, scheduled individual passenger
itineraries, placed company headquarters, vehicle production/parts inventories,
physical towing or lifting, offline earnings and long-duration balance calibration
on production servers. The finite career fixture establishes conservation and
recovery for its explicit budget and route conditions; it does not establish
unlimited profitable growth or an individually scheduled passenger population. These distinctions
are retained in [MOTORTOWN_PARITY.md](MOTORTOWN_PARITY.md).

### A priced novice passenger career

The separate [novice passenger regression](../crates/terra-sandbox/tests/company_career.rs)
uses the unchanged authored settlement roads, queues and **250-credit personal
wallet**. It registers a company, deposits **100 credits once**, hires one driver
and rents the basic-license **Comet hatchback**. The company requests three
passengers per leg between Town and Hillside. It opens no depot and buys no
vehicle. There are no later stock grants, owner deposits, fare changes or fund
injections.

The acceptance window is **3,600 physical seconds**, split by six exact JSON
save/resume checkpoints at ten-minute intervals. The accelerated ecology clock
is held at zero to isolate the transport business: vehicle movement, traffic,
wear, rental time, fare reservations and payroll still advance normally. This
does not measure avatar food, water or housing costs. Passengers circulate
between the existing queues; they do not represent distinct newly generated
individual itineraries.

The harness pays each real lease renewal before expiry. When a route holds after
a sustained obstruction, it waits ten physical seconds and submits the ordinary
**Start / resume** command, with a finite limit on retries. Payroll stops during
the held interval; the rental clock continues. Holds and their times are retained
in the report, and the owner's management time remains unpriced. This is a
managed-service scenario, not unattended-autonomy evidence.

After the hour it pauses, completes the final passenger commitment, releases
the empty car and uses ordinary vehicle Autopilot to return to its original
delivery bay beside the garage. **Release** parks a vehicle; a later accepted
RTS move or Autopilot order releases its parking brake. The test then executes the checked rental
return and records actual bond, fuel refund and damage deductions. It includes
registration, recruitment, payroll, the initial rental and every renewal, net
fuel cash and rental damage in the career result. Refundable bond and owner
deposits are transfers, not earnings or expenses. Initial rental costs that
predate company assignment remain explicit in this external career measurement;
the persisted company ledger is not rewritten to claim a larger coverage period.

`NOVICE_PASSENGER_CAREER_REPORT` is the machine-readable measurement. The
acceptance test requires a positive cash change after all of those known costs,
exact cash/escrow and passenger conservation at every step, fuel reconciliation,
six exact in-flight state resumes and a final regional accounting round trip.
It retains every ten-minute checkpoint, total service/return time, odometer,
lease renewals, road holds and the persisted company accounting report.

The focused 0.13 run completed **65 paid deliveries**: the hour ended with 64
complete and one in flight, then the final delivery and actual rental return
finished at **3,649.5 physical seconds**. It traveled **15.726 km** and consumed
**1.582 litres**, with **five explicit wait/resume interventions**. All six
checkpoints and the final regional accounting round trip passed. Exact cash and
escrow remained **13,650.00 credits**, and existing passenger queues plus in-flight
loads remained **160 people** throughout. The unpriced starter vehicle's mileage,
fuel and condition were unchanged.

| Measured career cash movement | Credits |
| --- | ---: |
| Paid public fares | 821.60 |
| Employee payroll | −70.71 |
| Registration / recruitment | −20.00 / −10.00 |
| Initial lease plus 12 renewals | −23.40 |
| Fuel payment net of the actual return refund | −2.38 |
| Rental damage charged on return | −0.63 |
| **Cash change after all known costs** | **+694.48** |
| Initial personal wallet | 250.00 |
| Final personal wallet / company cash | 103.59 / 840.89 |
| One owner deposit / later deposits | 100.00 / 0.00 |
| Rental bond posted / returned | 45.00 / 44.37 |
| Initial / final finite service fund | 10,000.00 / 9,234.81 |

The final company journal reports **699.29 credits after its recorded known
owner costs**. The external career result is **4.81 credits lower** because it
also includes the **1.80 initial pre-assignment lease**, **2.38 net fuel cash**
and **0.63 damage** from the actual rental return. This difference is deliberate
coverage disclosure, not another withdrawal or a rewritten journal. Remaining
fuel and bond are refunded by the real return transaction; neither is counted
as new business revenue.

This finite public-service case does not establish indefinite treasury funding,
all-in profit after management/living costs, warehouse profitability, every
terrain layout or long human-player engagement. The paired warehouse loss cases
remain unchanged and continue to measure their own acquisition and recovery
conditions. Run the additional evidence with:

```sh
cargo test -p terra-sandbox --test company_career --test company_planning --offline --locked -- --nocapture
```

### Failed owner-paid vehicle services (0.18)

An assigned vehicle's personal fuel, repair, paint, engine/brake upgrade or
renewal charge prepares its company expense receipt before payment and commits
it only after the checked wallet/issuer transfer succeeds. Insufficient personal
funds, a full issuer treasury or exhausted accounting capacity rejects the whole
command without changing the vehicle, rental, funds or company journal. This
preserves cash-basis records; it does not change service prices or company cash.

## 0.24 exact operational choices

Native Fleet retains the selected driver, vehicle, route, relief driver, stop
and depot. A missing explicit selection needs deliberate browsing. Assignment,
run/pause, hold/release, handover, route drafting and stock/cash transfers retain
their reviewed targets, desired state and amount through activation.

Normal movement, payroll and rental countdown do not invalidate an otherwise
unchanged operation. Changed assignment/load/selection or lost eligibility
requires a fresh action and leaves value unchanged on rejection. Existing price
quotes and destructive confirmations continue to apply.

