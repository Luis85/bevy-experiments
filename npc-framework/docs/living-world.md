# The coastal world

Normal entry selects `living_world`: a finite 256×256 metre island with Harbour,
Alderwood and Stone Ridge, 42 residents, distinct homes and workplaces, regional
terrain and a shared physical economy. It extends the one-window flow described
in [Island life](island-life.md). Survival and permanent death still apply.

At startup, the title shows **Preparing the island** until the terrain and its
render pipelines are ready. World-entry controls stay disabled during preparation;
How to play and Quit remain available. Preparation does not advance the world or
write a save. If preparation fails, the title offers a clear restart message.

## Explore and participate

Open **Explore** for the published settlement chart. Destinations are authored public
information; the live marker is your character. It does not reveal unseen people,
stock or private property. Travel uses ordinary walking. Carry food, water and warm
clothing on longer journeys.

**Craft** shows discovered sources and workshops. Fishing requires a real rod.
Make one at a carpenter's workshop from one plank and two thread, or buy one.
The rod is retained when catching fish. Two fish, one water and one log produce
two cooked meals at a bakery; raw fish cannot be eaten. Tailors can weave a warm
coat from eight thread and four fiber, a renewable alternative to leather clothing.

Harvesting depletes stocks and disturbs habitats. Renewable sites recover gradually;
ore does not regrow. Approach a disturbed site and open **Explore** to restore it.
Planting a tree cutting uses one log and one water; habitat restoration uses two
fiber and one water. Work takes 240 ticks. Ingredients and site are reserved, then
consumed only on completion. Stop releases unspent materials. Growth matures later;
restoration does not instantly refill stock. Wildlife uses a breeding refuge and
habitat-dependent capacity instead of a daily population reset.

Building, farming, hunting, trade and neighbours retain shared physical actions.
Housing and garden plots are distributed across the world. Colony commissions
require actual procurement and construction.

## Oversee and shape

Colony **Overview** offers settlement camera destinations. Select a resident under
**Residents** to inspect qualifications, assignment, reason, cooldown and pending
work. Requests are checked against capacities, qualifications and tools. An assignment
waits for a safe work boundary; it does not teleport the person, grant equipment or
erase their current batch. Automatic reconsideration responds to shortages and
disruptions, with essential coverage and cooldowns preventing constant switching.

Terraform uses the same recorded command on the larger heightfield. A painted
stroke can also be sent as one `TerraformCells` command: each listed cell carries its
own height and surface, the combined result is checked once, and either every cell
changes or none does. Dry elevations
range from 0 to 12 metres, water from −2 to 0, and brush radii from 0 to 8 metres.
Homes, workplaces, actors, access and connectivity constrain edits. Navigation is
on a ground heightfield, with no stacked floors or navigable tunnels.

## Prices, tools and hardship

These rules apply to the coastal world only; older island profiles keep theirs.

- **Levy, not a cap.** Each day a resident pays 10% of the coins held above 200
  into the community fund. Anyone below 40 coins is topped up to 40 while the fund
  can pay. The fund keeps 20 coins per resident in reserve and returns the rest as
  an equal dividend, less what open building sites still need (see below).
  Visitors pay and receive nothing. No coin is created or lost.
  The innkeeper no longer consumes logs, bowls and hammers on a schedule.
- **Margins move labour.** A profession's realised margin is the learned market
  price of its goods minus input cost, compared with the same margin at seed
  prices. Three of the fourteen residents in each settlement may take any trade.
  When a legal local vacancy out-earns a resident's job by 250‰ for 6,000 ticks,
  a flexible or surplus worker may take it, and the switching cooldown is 6,000
  ticks. A sole essential worker still stays unless blocked.
- **Tools.** Woodcutters start with an axe, miners with a pickaxe and farmers with
  a hoe. The matching tool cuts gathering time for logs, ore and grain to 75%;
  without it the work takes 150%. This applies to your character too. A tool lasts
  120 gathers, then breaks. Smiths forge an axe or hoe from one ingot and one
  plank, and a pickaxe from two ingots and one plank. A worker without a tool
  orders one through the ordinary funded plan and keeps working by hand meanwhile.
- **Cold.** Below 10,000 Warmth a resident loses 6 health per tick. In winter a
  hearth log lasts a third of a day instead of half.
- **Morale.** Gathering, and residents' own crafting, take between 90% (full
  morale) and 115% (no morale) of the normal time.
- **Winter fuel.** From two days before winter until it ends, each home keeps two
  hearth logs in reserve, so woodcutters cut ahead of the cold. A cold resident
  without a coat fetches fuel from a tree patch when it is nearer than a seller,
  stays by the hearth until warm (up to 1,500 ticks) and shelters at home when
  freezing even without fuel.
- **Essential trades.** The last farmer, miller, baker, well keeper or woodcutter
  of a settlement stays while the settlement still wants the goods. A settled
  specialist may cover such a trade at home when its workplace stands empty.
- **Settled labour.** A resident does not return to a profession left less than two
  days ago unless a persistent pay gap of twice the usual threshold pulls them back.
  A seat somebody just left stays open for 2,400 ticks, so two residents no longer
  swap trades in one pass. A worker whose own goods are short moves only towards a
  larger shortage. An empty essential workplace overrides these rules, and an
  explicit assignment from the inspector is not limited by them.
- **Price ceiling.** A learned market price never exceeds five times the base price.
  Units delivered in a market round answer as many unfilled orders, so a good made
  to order, such as bread, rises only while orders outrun deliveries. A price above
  base with no order left unfilled and nobody undercutting drifts back by a
  sixteenth per market round.
- **Food first.** Ingredients of food count as food when a producer ranks funded
  goals: a farmer cuts ordered grain before fibre, so the mill and the bakery are
  not starved by a long order book for cloth. Bakers keep four flour and millers
  four grain in hand (`keeps` in the economy data), so ordered bread is baked at
  once instead of waiting for a harvest, a milling and two walks.
- **Seeds.** The world seed chooses each founder's preferred food, one extra
  aptitude and up to one extra unit of each starting good, so worlds differ.
- **Data.** Goods, recipes, professions, patches, endowments and these numbers are
  the `economy` block of the world file: see
  [Authoring the coastal world's economy](island-economy-authoring.md).
- **Behaviour.** Residents run a behaviour tree that is data
  (`content/testbed_content/trees/resident.json`): needs first, then chores, leisure
  and the profession. Each resident has four seeded traits. Diligent residents start
  their evening leisure later, sociable ones spend it meeting neighbours or strolling
  to the square, larks and owls shift their whole day by up to 600 ticks, and thrifty
  ones keep coins back instead of stocking provisions ahead. Work, meal, leisure and
  sleep windows are data too; a resident carrying food eats at meal times once
  Hunger is below 60,000. The journal records why a resident chose its current
  activity. See [Authoring resident behaviour](island-ai-authoring.md).
- **Planning and routes.** A resident reuses a searched plan while its first step
  is pending and still possible, for at most 600 ticks. After three failed detours
  a resident gives up on that destination, the journal records
  `VillageRouteAbandoned`, and the target is avoided for 1,200 ticks.

## A village that grows

These rules apply to the coastal world only.

- **Building sites.** The overseer places a building from the list in the world's
  data, and the civic planner places one when a settlement lacks beds, storage or
  a workplace. A site is real ground: it must fit, stand on dry buildable land,
  leave entrances free and stay reachable. Materials are carried there by
  residents and paid for by the community fund at the market price. When all are
  delivered, a resident builds in stages. Cancelling a site returns the materials
  to a nearby store, or lays them at the entrance.
- **Stores and stalls.** Producers lay surplus in a stockpile, barn or market
  stall. It stays theirs: the store sells it at the market price while they are
  away and they are paid at once. Storage serves residents; a stall serves
  anyone, visitors included. Tools and clothing are not stored. Buy at a
  store as you would from a person; sell to a store to consign your own goods, and
  sell to a building site to deliver what it needs.
- **Carrying.** Any resident whose needs are met may carry materials to a site,
  build, store surplus or restock a stall from storage during work hours. There is
  no builder or porter profession.
- **The planner.** Each settlement is reviewed every 1,200 ticks. Homeless
  residents get a free bed first. A shortage that lasts three reviews leads to one
  building: a home, a store or a workplace. One project per settlement at a time,
  and the fund must be able to pay. The daily dividend returns only the coins open
  sites do not need. Each decision is in the journal as `VillageCivicProject`.
- **Money.** The village pays for every building, the overseer's and the
  planner's. When materials cost more than the fund may spend, a building levy
  takes at most a twentieth of each resident's coins above 40, only as much as is
  needed, and those coins buy materials at once. The fund keeps 250 coins back
  for destitution grants, which are always paid first. No coin is created or lost by building, storing or selling
  through a store.

Buildings, their costs and the planner's thresholds are data: see
[Authoring the coastal world's economy](island-economy-authoring.md).

## A village that lives

These rules apply to the coastal world only.

- **The year.** A season has seven days and a year four seasons. The world is
  founded in late autumn, two days before the first winter.
- **Ages.** Every resident has an age. A child comes of age at two years, an
  adult is an elder at nine and old age takes a resident between ten and thirteen,
  on a day fixed by the world seed. Founders are between two and eight and a half.
  Elders walk at three quarters of their speed and take a quarter longer over
  their work.
- **Couples.** Two housed adults of one settlement who keep meeting grow fond of
  each other. When both are fond enough for two days they become a couple and
  share the home that has room for both.
- **Children.** A couple under one roof may have a child when there is food in
  the house and a bed: a free one, or a home the civic planner can pay for. A
  child takes a bed, eats from its parents' packs and does not work. A family
  with too few beds makes the planner build a family house and moves in when one
  stands. On coming of age the child takes a free workplace seat (a parent's
  trade first), keeps one aptitude of a parent and gets a start in coins from
  both. When the house has no food, the fund buys a hungry child a meal from a
  store of the settlement; a child without any food for three days dies.
- **Death and inheritance.** When a resident dies, the partner and the other
  adults of the household share the coins and the plain goods; with nobody at
  home the coins go to the community fund. Orphans are taken in by a neighbour.
- **Settlers.** The merchant boat lands up to two settlers a visit while a
  settlement has a free bed, a free workplace seat and food to spare, and the
  fund can give each 60 coins.
- **Fields.** Every settlement is founded with a field of eight plots and a small
  granary beside its farm. Farmers sow in spring and summer, tend what grows and
  harvest what is ripe; grain takes two clear days to ripen. A plot keeps one
  unit of each harvest as seed. They sow only while the settlement holds, or has
  growing, less than four units of grain per resident, so fields follow the
  headcount instead of growing a glut. On the first day of winter every plot is cleared,
  ripe or not, and the fields lie fallow until spring. Wild grain is a founding
  reserve that does not grow back: the mill's grain comes from the fields. The
  planner lays out another field when grain stays short with every plot sown.
- **Weather.** Each day is clear, rainy, stormy or snowy, by odds that follow the
  season; the world seed decides, so the three-day forecast is exact. Rain makes
  crops grow half as fast again and refills the wells. A storm keeps everybody
  from outdoor work (crafters work on) and keeps the boat away; the day before
  one, residents lay in a third day of food and water, and market prices rest
  while it lasts. Snow makes the
  whole day cold and half as cold again, and nothing grows.
- **Hard seasons.** Rarely, and never in the first three seasons, a season brings
  a drought (summer: no rain and half the growth), a blight (spring or summer:
  about four growing plots in ten are lost) or a harsh winter (more snow, half
  as much warmth lost again and one more hearth log a day).
- **The merchant boat.** Every fourth day a boat lands at the Harbour trading
  post in the morning and stays some hours. It buys what the Harbour's stores
  hold above a kept level (grain, fibre, thread, planks, leather, fish, bowls)
  and pays the owners at once. It sells the community fund what the Harbour is
  short of (grain, flour, ingots, logs); the goods lie in the trading post and
  residents buy them at the market price. The merchant has 1,200 coins and
  nothing refills them: island coins plus the merchant's purse never change.
- **Indoors.** A resident who stands at the door of the home it sleeps in, and
  is not walking, is inside that home. Sleeping, resting, warming at the hearth,
  eating at home and sheltering from a storm happen indoors. Inside, the home
  decides warmth (a lit hearth warms fast, an unlit home slowly) whatever the
  weather, a coat does not wear, and only somebody inside the same home can be
  met. Everybody with a bed sleeps in it; only a resident without a home rests
  in the open, and the planner houses or builds for them. Beds go to the adults
  of a home in order of identity, then to its children; a family larger than
  its beds shares until it is rehoused. A guest who rents a bed beside the inn
  is inside the inn while the rest lasts.
- **The fund keeps a reserve.** The fund holds back three days of its recent
  shortfall (destitution grants above the levy; at least 250 coins) before it
  buys building materials or imports, and the daily dividend returns only what is above that, the needs of
  open building sites and the cost of a project the planner cannot pay yet.

Every one of these numbers is data: see
[Authoring the coastal world's economy](island-economy-authoring.md). The journal
records births (`VillageBirth`), coming of age (`VillageComingOfAge`), couples
(`VillagePairing`), deaths with their cause (`VillageDeath`, `VillageChildDeath`,
`VillageInheritance`), settlers (`VillageArrival`), weather changes
(`VillageWeather`, `VillageSeasonShock`), field work (`VillageFieldSown`,
`VillageFieldRipe`, `VillageFieldHarvested`, `VillageFieldFallow`) and each boat
visit with its totals (`VillageMerchantArrived`, `VillageMerchantVisit`,
`VillageMerchantDelayed`).

- **Nights.** In its sleep block (the last three tenths of the day, a little
  earlier or later by chronotype) a resident with a bed goes home, goes to bed
  and sleeps until morning. Only its own hunger or thirst near danger, a child
  of the home with nothing left to eat, or a player order wakes it; it goes
  back to bed afterwards. A night restores Energy slowly; a rest at home by day
  is short and restores a little. Somebody without a bed still rests where it
  can.
- **Inns.** An inn has four guest beds. A guest who rents one is inside the
  inn in a bed of its own; a fifth guest is turned away until a bed is free.
- **The dead.** A resident who dies leaves coins and plain goods to the
  household, as before. A day later the body is removed: the bed is free, no
  roster names it, and the life stays on record with name, profession, age,
  cause, day, household and settlement (`view::departed`). Children who die
  are recorded the same way. Journal: `VillageLaidToRest`.
- **Hungry children.** When a child has eaten too little and nobody at home
  carries food, an adult of the home goes and buys some, without spare coins
  and without being hungry itself.

- **A played life.** The played character acts through recorded commands.
  `{"type":"LiveAct","unit":U,"verb":"sleep"}` at the door of its home, from
  six tenths of the day on, puts it to bed until the next morning; `wake` ends
  it. The simulation skips nothing: a host may run faster while
  `view::asleep_until` names a tick. Every morning it gets a day summary
  (`view::day_summary`): coins earned and spent, goods gathered, made and
  received, residents met, the low point of each need and whether it slept.
  `{"verb":"repeat","target":PATCH,"quantity":N}` gathers from the same
  resource N times; with `"recipe":KEY` and a workshop as `target` it crafts
  the recipe there N times (the character stands at the workshop); with
  `"task":"plant"`, `"water_crop"` or `"harvest"` and one of its plots as
  `target` it works the beds that need it, walking from bed to bed.
  `stop_repeat` ends it. `VillageRepeatStopped` says why it ended: `count_done`,
  `pack_full`, `inputs_gone`, `need_critical` (a need below the policy's
  critical level) or `target_gone`, with a `detail` sentence. Goods in the day
  summary are booked by the action that brought them: a gathering, harvest or
  hunt is `gathered`, a craft `made`, a purchase, a gift back or a friend's
  harvest `received`.
- **Bonds.** Talk, trade and gifts raise a bond with a resident, a disliked
  gift and neglect lower it (`{"verb":"gift","target":RESIDENT,"kind":GOOD,
  "quantity":1}`; one gift a day counts). A friend sells a tenth cheaper, a
  close friend a fifth; a friend now and then hands over something of its own
  when talked to; a close friend invites to gatherings. Residents sometimes
  speak first. Among residents fondness grows by meeting and fades slowly.
  There are no debts in the village, so a bond has no "unpaid debt" effect.
- **Friends help.** A friend or close friend (`helps` in the tier data) who is
  in its leisure block, in the settlement of the cottage and well in every need
  helps once a day: a material the open upgrade misses (paid at today's price
  from the upgrade's escrow), work on a supplied upgrade, or a ripe bed of the
  owner's plots harvested into the chest or the pack (`VillageFriendHelped`).
  The help is instant: the friend does not walk to the homestead.
- **Goals and the homestead.** A chain of goals from data (`view::goals`):
  a cottage, a storage chest, a second plot, a tool shed, the larger cottage,
  a friend who helps and a gathering. Each upgrade of the cottage
  (`view::homestead_tiers`) is a private building site (`view::upgrade_site`):
  `{"verb":"upgrade","kind":"storage"}` at the cottage door starts it, and
  carried materials go in by themselves while the owner stands there;
  `{"verb":"deliver","kind":G,"quantity":N}` hands over some explicitly;
  `{"verb":"order","kind":G,"quantity":N}` asks the residents of the
  settlement to bring them, and the price plus a carrying fee
  (`haul_fee_percent`) is held in escrow from the wallet and paid to whoever
  brings them; `{"verb":"hire"}` hires a builder (a carpenter first) for the
  remaining work at `builder_wage` coins a stage, also held in escrow;
  `{"verb":"cancel_upgrade"}` gives the delivered goods and the unspent
  escrow back. Work starts once every material is in; the owner works while
  at the door, and a hired builder or a friend works too. Escrow is part of
  every coin count (`village::live::escrow`). The storage chest
  (`view::chest`) holds 40 units: `{"verb":"store","kind":G,"quantity":N}`
  and `take` at the door. The second plot allows a second garden plot (the
  claim beyond the allowance is refused with the reason). The tool shed
  shortens the owner's gathering to 80%. The larger cottage grows the body
  into the 5 x 6 m family house with four beds.
- **Homestead cottages are real houses.** In the living world the cottage is
  the catalogue cottage, 4 x 5 m with two beds, behind the plot's door; a plot
  takes a cottage only when the family house also fits there (the claim's
  build is refused with the reason). `view::home` reports the body, door and
  beds. Older profiles keep the 2 x 2 m cottage with three beds.
- **Gatherings and grief.** On the harvest feast and at midwinter every free
  resident whose needs are all above 50,000 walks to the square of its
  settlement and gains heart (`view::gatherings`); a hungry or cold resident
  sees to that first. When somebody is laid to rest, the household and
  those fond of the dead mourn for three days (`view::mourning`).
- **Prices hold to what goods cost.** Residents and stores do not sell a made
  good below its input cost at today's prices plus 10%, nor a raw good below a
  quarter of its base price. Makers start no new batch while the settlement
  already offers more than two days of its use (one batch always counts as
  working stock; a crop keeps the field's own target), and stock up to that
  target does not lower a price: only stock beyond it does. The flat floor
  (`price_floor_percent`) stays as a switch and is 0.
- **Boat duty.** On every trade of the merchant boat 5% of the value goes
  from the boat's purse to the civic fund (`VillageBoatDuty`).
- **Shaping the world.** `{"type":"PlaceResourcePatch","good":G,"x":X,"z":Z,
  "richness":150}` (millimetres; richness 25 to 400 percent, in steps of 25, of the authored
  capacity) places a patch for the nearest settlement within 60 m, up to four
  of one good; `{"type":"RemoveResourcePatch","patch":ID}` removes one but
  never a settlement's last of a good. `frontier::export_world(world)` returns
  the current world as world JSON (`{"living_world": ...}` with `terrain`,
  `resource_patches` and `buildings` beside the authored blocks); the host
  merges it into the world document and writes the file, and building that
  file gives the same terrain, patches and buildings. Trees and rocks are
  view dressing, not simulation data.
- **Inns by night.** A guest who rents a bed in its sleep block sleeps there
  until morning; the number of guest beds is data.

Views read all of this through `testbed_content::village::view`: activity label,
latest decision, effective schedule, season effects, household and family,
weather and forecast, boat status, field plots, who is inside which building,
each home's entrance, footprint, facing and beds, and whether a resident is
awake, resting or asleep; inns with their guest beds (`inn`, `inns`); the dead
(`departed`); where a resident would buy a good (`supplier`); a day's use of
water and food per settlement (`daily_use`), who makes a good (`producers`)
and the building catalogue with a reason per entry (`catalogue`); and for a
played character the day summary, bonds (`relationships`, `gift_tastes`),
`goals` (with the step and the total), `homestead_tiers`, `upgrade_site`,
`chest`, `gatherings` and `mourning`. Workplaces, founding or built, give
`workplace`/`workplaces` (entrance, door, body, facing, built, solid).
`daily_use` also lists goods that are not eaten, from the units sold per day. For the inspector: `age` (the life stage and an age laid over a
human life, printed as `Adult · 34`; a year of four seasons is too short to
print), `yearly_census` (births, deaths and arrivals by year), `pack` (weight
and capacity in grams), `held_tool`, `activity_progress` and `need_reason`
(one sentence on why Hunger, Thirst, Energy or Warmth is low). Homes and inns
give `entrance` (where residents stand), `door` (the centre of the doorway on
the wall, in millimetres: draw the door there) and the blocked body both in
cells and in millimetres. A resident indoors keeps the position of the entrance:
the view hides it or draws it inside.

## Saves and authoring

Normal play uses `living-world.world.json` in the configured agents folder and
`living_world.json` in its worlds folder. The old `local-island.world.json` is
preserved. Open it explicitly with:

```powershell
.\target\debug\game_menu.exe --front-end --island-save "C:\path\local-island.world.json"
```

Continue reconstructs the saved immutable profile. New Game in that loaded session
uses the same profile. Editing preset files does not migrate existing saves.
Explicit lower-level legacy constructors remain legacy; select living-world rules
when creating a regional world.

The preset declares dimensions, arrival, settlements, homes and sites. Separate
content contexts own mechanics, as documented in [ADR 0037](adr/0037-regional-living-world.md).
Reference prices and the community fund are shared world institutions. There are
no separate village currencies or hidden deliveries. Research and test results
remain separate evidence; a larger world alone does not prove indefinite survival.

## Validation scope

One year of the shipped calendar (28 days, seed 7, the pricing pass) ended with
51 residents, 12 children, 9 settlers and 12 births, no death of any cause and a
population between 42 and 63; the lowest needs were Energy 56,590, Hunger 19,386,
Thirst 20,461 and Warmth 24,406; the planners built 17 buildings for 745 coins.
The fund was above the reserve of 250 in 784 of 1,120 samples and ended at 179;
the building levy fired 7 times (221 coins), the relief levy never. The same year
without the boat duty: fund above the reserve in 777 samples, ending at 209, the
building levy 7 times (223 coins). The boat trades a few hundred coins a year, so
a 5% duty moves a few tens of coins and does not change the fund's course; the
purse rose from 1,200 to 1,331 and is not starved. The forced blight and harsh
winter year (twelve days) ended with 48 residents, no death of want or cold, a
population never below 42 and lowest needs Energy 22,728, Hunger 17,082, Thirst
25,660 and Warmth 13,076.

With nights slept through, buying at fixed places, the dead laid to rest and
children fetched for (2026-11-11, later the same day) the seed-7 fourteen-day
audit kept all 42 founders alive and housed, with 46 residents and one child at
the end and 9,600 coins at every sample. Trades fell from 3,109 to 2,860, job
changes from 185 to 131 and meetings from 8,914 to 7,397: nobody trades, changes
work or meets a neighbour while asleep. The lowest sampled needs were Energy
18,566 (40,752 before), Hunger 33,214, Thirst 35,194 (47,128 before) and Warmth
24,036: Energy now comes back over a night instead of in one short rest, and
nobody drinks while asleep. In the middle of each of three nights 42 of 42
housed residents were asleep in their own bed (the view had counted six to nine
near midnight and none later); `tests/island_indoors.rs` keeps one night in the
suite and three as an explicit run.

Three years of the shipped calendar (28 days a year, 2,016,000 ticks, 94
minutes for three seeds in a release build) on seeds 7, 11 and 23 ended with
57, 60 and 63 residents and 15, 17 and 21 children, after 20, 24 and 32 births,
14, 12 and 13 settlers and 4, 1 and 3 deaths, all of old age. Nobody died of
want, nobody with a bed slept outside it, no resident fell below 10,000 in any
need (lowest: Warmth 10,672 on seed 11) and no child went unfed for 600 ticks.
The planners finished 26, 36 and 40 buildings; the boat called 21, 21 and 20
times; the fund ended at 7, 83 and 7 coins, the relief levy paying what it
could not. Of 3,360 price samples per seed, logs stood near their ceiling in
38, 31 and 45; on seed 23, with up to 85 people, grain did in 214 and bread in 131.
Seeds 7 and 11 had one blight each; no drought and no harsh winter occurred, so
a harsh winter is still covered only by `tests/island_weather.rs`. This is the
explicit test `the_village_lives_through_three_shipped_years_on_three_seeds`.

The shelf of a gatherer was checked against a control: two days in, with water
made five times dearer, the price was above twice its base in 2 of 16 samples
with the shelf and in 7 of 16 without it. Without the shelf the price also
comes back in that scene, about three times more slowly.

Prices and oversupply (2026-11-11, the pricing pass). The flat floor of the
live-mode pass was measured against a control on the same seed-7 fourteen-day
audit: with the floor at 50% the lowest Hunger was 14,996 and job changes 224,
with it at 0 Hunger 20,990 and 163. The rest of that regression came from the
gatherings: without a played character, the midwinter fire drew hungry and cold
residents to the square (with gatherings switched off: Hunger 33,214, Thirst
35,194, Warmth 24,036, 131 job changes, the figures before live mode). With
reservation prices, stock targets, stock cover for goods made in batches or grown,
gatherings only for residents above 50,000 in every need, the longer margin
hysteresis (6,000 ticks) and the boat duty, the same audit kept all 42 founders
alive and housed (46 residents at the end) and 9,600 coins at every sample, with
2,986 trades and 118 job changes; the lowest needs were Energy 59,086, Hunger
28,718, Thirst 34,165 and Warmth 24,406. Of 1,120 price samples bread, planks and
grain were at the lowest price the market allows in 0, 148 and 0 (before: 785,
1,090 and 967 with the 50% floor); logs were at one coin in 226. An earlier
variant of the cover rule (any open order pushed the quote a full step) gave
Hunger 34,258, Thirst 30,451, Warmth 25,900 and 103 job changes, but held bread at
its ceiling for two days after a player buy-out; the shipped rule lets bread
recover 15,600 ticks after the buy-out ends (`tests/island_player_shocks.rs`).
These are single runs of one seed: switching single rules off moved the lowest
Thirst between 21,000 and 38,000, so a few thousand either way is within what one
seed varies.

Simulation cost, release build, 3,600 ticks in autumn: 489, 293 and 153 ticks
per second with 42, 81 and 120 residents.

With demography, fields, the merchant boat, weather, the two levies and
residents indoors (2026-11-11) the seed-7 fourteen-day audit kept all 42 founders
alive and housed; six settlers landed and three children were born, so 48
residents and three children lived there at the end. Island coins plus the boat's
purse stayed at 9,600 (8,400 and the purse of 1,200) at every sample. There were
3,109 trades, 185 job changes and 8,914 social meetings; the lowest sampled needs
were Energy 40,752, Hunger 36,990, Thirst 47,128 and Warmth 24,916. Nine
buildings were finished for 342 coins and the fund ended at 437. The weather was
nine clear days, one of rain and four of snow. Warmth is higher than before
(14,536) because a resident at home is inside, under the hearth's rule, and a
chilled resident now goes home whether or not there is fuel to buy. Earlier
audits did not count meetings, so what the indoors rule (passers-by cannot meet
somebody indoors, housemates can) did to their number is not measured. The audit's last check changed: no garment
made in the final three spring days fails only while somebody has no coat or an
unfilled order for one, because a coat no longer wears indoors.

Three years of twelve days (`days_per_season` 3, 864,000 ticks) on seeds 7, 11
and 23 ended with 52, 59 and 52 residents and 16, 16 and 18 children, after 20,
21 and 18 births, 10, 13 and 13 settlers and 4, 1 and 3 deaths, all of old age.
Nobody died of want, nobody with a bed slept outside it, and the population
never fell below the 42 founders. The planners finished 26, 28 and 23 buildings
for 889, 857 and 641 coins; the boat called 17, 17 and 16 times. Of 1,440 price
samples per seed, water stood near its ceiling in 9, 7 and 4 and logs in 32, 13
and 22. The lowest needs on seeds 11 and 23 were Energy 43,994 and 42,850,
Hunger 40,932 and 34,866, Warmth 21,910 and 18,214. On seed 7 one elder miner
went down to Hunger 2,482 and Energy 186 in the third summer and recovered: with
37 coins and no food in his pack he walked after a food seller for about 6,000
ticks at an elder's pace before he could buy, then ran out of Energy on the way
home. Buying from somebody who is walking is not fixed. There was one blight on seeds 7 and 11 and no
harsh winter in any run, so a harsh winter is covered only by
`tests/island_weather.rs`. These are the explicit, ignored tests of
`tests/island_generations_soak.rs`; a year of the shipped length (28 days) was
not run.

Simulation cost, release build, 3,600 ticks in autumn: 344, 174 and 116 ticks
per second with 42, 81 and 120 residents (69, 71 and 72 microseconds per resident
and tick), against 206, 77 and 49 before the hot paths were rewritten.

With data-authored resident behaviour (2026-11-11) the seed-7 audit again kept all
42 residents alive and housed for 336,000 ticks and conserved 8,400 coins, with
2,891 trades, 132 job changes and a lowest sampled Warmth of 14,992. After a
one-day player buy-out, bread was back within twice its base price 17,400 ticks
later. These are single-seed results from the explicit, ignored tests.

With building sites, stores and the civic planner (2026-11-11) the same seed-7
audit kept all 42 residents alive and housed and conserved 8,400 coins, with
2,834 trades, 131 job changes and a lowest sampled Warmth of 14,536. The planner
commissioned five workplaces and all were finished: tailor shops in Alderwood and
Stone Ridge, farmsteads in Harbour and Alderwood, and a bakery in Alderwood. The
fund paid 394 coins for their materials, stayed between 217 and 330 coins through
the winter and ended at 602. Nobody lacked a bed and no store was needed, so no
home or store was built in that run; those paths are covered by
`tests/island_buildings.rs`. A six-day run on seeds 7, 11 and 23 also kept
everyone alive and housed, with two, one and no building finished. The buy-out
figure above was not measured again.

The [validation record](_notes/2026-11-09-living-world-validation.md) contains the
latest seed-7 audit: all 42 residents remained alive and housed for 336,000 ticks
(14 days through winter into spring), conserving 8,400 coins. Native confirmation
includes three real village journeys and 54 captures at both supported test sizes.
Physical input and GPU/FPS performance targets were not verified; a full year or
indefinite survival is not established. Six additional inspected images confirm
the shading and editor diagnostics corrections. The
[startup follow-up](_notes/2026-11-10-terrain-startup-polish.md) confirms preparation
gating, coherent terrain reveal and water/land chunk updates without empty-mesh
warnings. Its full gate returned `ok: true`, including 1,960 passing tests and all
architecture, replay, snapshot and dependency-policy stages. That record retains
skipped/ignored counts and limitations.
