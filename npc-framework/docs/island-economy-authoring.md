# Authoring the coastal world's economy

The coastal world (`living_world`) reads its goods, recipes, professions, resource
patches, buildings, founding endowments and tuning from data. The block is `economy` inside
`living_world`, in both shipped files:

- `content/testbed_content/worlds/living_world.json` (the world preset)
- `content/testbed_content/scenarios/living_world.json` (the direct scenario)

Older island profiles do not read it and keep their Rust tables.

Like `island_ai` (see [Authoring Island Village life](island-ai-authoring.md)), the
block is validated before the world is built, copied into the save's immutable
construction data and covered by the state hash. Edit a source file and start a
new world to try a change. An existing save keeps the economy it was created with.

## Compatibility

`economy.version` is 1. A world file without an `economy` block loads with the
built-in economy, which equals the shipped block. A block may also omit whole
sections (`goods`, `recipes`, `professions`, `patches`, `buildings`, `seed`, `tuning`,
`demography`, `crops`, `weather`, `merchant`): each
missing section takes its built-in value. A section that is present replaces the
built-in section completely, so list every entry you want to keep. Unknown fields
are rejected. `buildings` is such a section too.

## Schema

```json
"economy": {
  "version": 1,
  "goods": [
    {"key": "log", "display": "logs", "base_price": 3, "stock_limit": 20,
     "tags": ["fuel"], "gather_tool": "axe"},
    {"key": "village_hoe", "display": "hoes", "base_price": 14, "stock_limit": 3,
     "tags": ["tool"], "durability": 120,
     "item": {"weight_g": 1200, "max_stack": 1, "tags": ["tool:hoe"]}}
  ],
  "recipes": [
    {"key": "village_saw", "display": "Saw planks", "station": "village_saw",
     "inputs": [{"good": "log", "count": 1}],
     "outputs": [{"good": "plank", "count": 4}], "duration": 160}
  ],
  "professions": [
    {"role": "carpenter", "station": "village_saw",
     "outputs": ["plank", "village_bowl", "village_fishing_rod"],
     "wants": ["log", "plank", "village_thread"],
     "recipes": ["village_saw", "village_carve", "village_make_rod"],
     "aptitudes": ["carpenter", "woodcutter", "miller"],
     "skill": {"name": "Carpentry", "level": 8}, "template": "carpenter",
     "endowment": [{"good": "plank", "count": 2}]}
  ],
  "patches": [
    {"good": "log", "dx": -32000, "dz": 0, "capacity": 240, "gather_ticks": 240,
     "regrowth": 4, "harvest_loss": 125}
  ],
  "buildings": [
    {"id": "stockpile", "display": "Stockpile", "cost": [{"good": "log", "count": 4}],
     "work_ticks": 200, "footprint": {"width": 4, "depth": 4},
     "kind": {"type": "storage", "capacity": 120}, "solid": false}
  ],
  "seed": {"coins": 200, "common": [{"good": "bread", "count": 2}], "founders": []},
  "tuning": {"levy_percent": 10}
}
```

### Goods

| Field | Meaning |
| --- | --- |
| `key` | Stable item kind. Renaming it is a new good |
| `display` | Lower-case noun in job summaries ("Gather logs") |
| `base_price` | Seed price, 1 to 10000 coins. The market learns from here |
| `stock_limit` | Stock ceiling of one seller, 1 to 10000 |
| `food_value` | Hunger restored by one unit; needs the `food` tag |
| `tags` | `food`, `tool`, `fuel`, `clothing`, `staple`. Every resident accepts and may buy a `staple` |
| `durability` | Uses before a made or founding unit breaks |
| `gather_tool` | Tool good that speeds gathering this good from a patch |
| `item` | Item kind to register: `weight_g`, `max_stack` (1 does not stack), engine `tags`, `equip_slot` |

Eight goods use item kinds owned by the engine content and must not declare
`item`: axe, bread, hammer, ingot, log, ore, pickaxe, plank. Every other good needs it.

### Recipes

`key`, `display` (job label), `station` (workstation kind), `inputs`, `outputs`,
`duration` in ticks (1 to 100000), and optionally `tool` and
`skill: {"name", "level"}` (the level is the roll modifier). Each recipe registers
the agent verb `craft:<key>`.

### Professions

`role` is one of the thirteen resident professions: farmer, miller, baker,
water_keeper, woodcutter, carpenter, miner, smith, innkeeper, hunter, tanner,
tailor, fisher. The set is closed: a profession's identity is a Rust enum, and
all of its properties are data.

| Field | Meaning |
| --- | --- |
| `station` | Workstation kind of the workplace; recipes must use the same one |
| `outputs` | Goods the profession makes and sells, in priority order |
| `wants` | Goods it buys, besides staples and its tool |
| `recipes` | Recipes it may work |
| `tool` | Gathering tool it is founded with and keeps one of |
| `required_tool` | Tool without which the job is not viable |
| `keeps` | Standing stock ordered through the funded plan: `good`, `count`, `reserve_inputs` |
| `aptitudes` | Trades a founder of this profession may ever take |
| `skill` | Minimum effective skill level to hold the job |
| `template`, `flexible_template` | Actor template of a founder |
| `essential` | The settlement keeps its last worker of this profession |
| `endowment` | Goods a founder starts with, besides `seed.common` and the tool |

### Patches

One patch per good is placed in every settlement at `dx`, `dz` millimetres from
its centre, or at a profession's workplace with `at_site`. `biomes` limits the
patch to settlements of those biomes. `capacity` and `gather_ticks` set the stock
and base gathering time. `regrowth` is units per 600-tick round at full biomass
and soil; `winter_regrowth` overrides it in winter. `harvest_loss` and `soil_loss`
are milli-units lost per harvested unit. `finite: true` never regrows.

A settlement keeps at least one patch of every listed good; it may hold up to
four of one good. `PlaceResourcePatch` adds one (25% to 400% of `capacity` in steps of 25%,
the same `gather_ticks` and regrowth) and `RemoveResourcePatch` removes one but
never the last; gatherers walk to the nearest reachable patch that still has
stock. An exported world (`living_world.resource_patches`) lists its patches
and replaces this list.

### Buildings

What residents and the overseer can build. Each entry is one building type.

```json
"buildings": [
  {"id": "cottage", "display": "Cottage",
   "cost": [{"good": "plank", "count": 8}, {"good": "log", "count": 4}],
   "work_ticks": 600, "footprint": {"width": 4, "depth": 5},
   "kind": {"type": "home", "beds": 2},
   "placement": {"surfaces": ["grass", "stone"], "max_slope_mm": 800, "clearance": 1}},
  {"id": "smithy", "display": "Smithy",
   "cost": [{"good": "plank", "count": 10}, {"good": "log", "count": 6}],
   "work_ticks": 800, "footprint": {"width": 5, "depth": 5},
   "kind": {"type": "workplace", "profession": "smith", "station": "village_smelt",
            "capacity": 2}},
  {"id": "stockpile", "display": "Stockpile", "cost": [{"good": "log", "count": 4}],
   "work_ticks": 200, "footprint": {"width": 4, "depth": 4},
   "kind": {"type": "storage", "capacity": 120}, "solid": false},
  {"id": "market_stall", "display": "Market stall",
   "cost": [{"good": "plank", "count": 6}, {"good": "log", "count": 2}],
   "work_ticks": 300, "footprint": {"width": 3, "depth": 2},
   "kind": {"type": "stall", "capacity": 60}, "solid": false, "civic": false}
]
```

| Field | Meaning |
| --- | --- |
| `id` | Stable key used by commands and saves. Renaming it is a new building |
| `display` | Name shown to players. A built one is labelled "Harbour cottage 1" |
| `cost` | Goods that must be carried to the site, each 1 to 1000. Tools and clothing are refused. May be empty |
| `work_ticks` | Builder ticks after the last delivery, 1 to 100000. Worked in stages of `tuning.build_stage_ticks` |
| `footprint` | Body in cells (metres): `width` across the entrance, `depth` behind it, each 1 to 12 |
| `kind` | What it becomes; see below |
| `solid` | `true` (default): the body blocks walking and sight. `false`: an open yard |
| `civic` | `true` (default): the civic planner may commission it. `false`: only a command places it |
| `placement` | Where it may stand; see below |

A building may also carry `"field": {"plots": 8, "crop": "grain"}`: crop plots
around it, worked by farmers (see [Crops and fields](#crops-and-fields)). Only an
open (`"solid": false`) building of type `storage` may have a field: the store is
the field's granary and takes only the seed and the harvest good of its crop. `plots` is 1 to 64 and `crop` a key of `crops`. With
`"founding": true` (the default) every settlement is founded with one such
building; set it to `false` for a field that is only ever built.

`kind` is one of:

| `type` | Fields | Finished building |
| --- | --- | --- |
| `home` | `beds` 1 to 8 | A household with that many beds |
| `workplace` | `profession`, `station`, `capacity` 1 to 8 (default 2) | A workplace residents can be assigned to. `station` must be the profession's own |
| `storage` | `capacity` 1 to 10000 units | A stockpile or barn. Sells to residents of any settlement, not to visitors |
| `stall` | `capacity` 1 to 10000 units | A market stall. Sells to anyone, visitors included |

`placement` (every field optional):

| Field | Default | Meaning |
| --- | --- | --- |
| `surfaces` | `["grass", "stone"]` | Ground every body cell must have: `grass`, `soil` (roads are soil) or `stone`. Water is never buildable |
| `max_slope_mm` | 800 | Largest height difference across the body and its entrance, up to 3000 |
| `clearance` | 1 | Free cells kept between the body and any other building, up to 8 |

Rules that are not data: the footprint stays inside the map; the entrance is a dry,
unblocked cell; the body does not cover another building, an entrance, a public
place or a reserved homestead plot, and keeps 3 cells from a resource site; nobody
stands where a solid body would rise; the entrance is at most 48 m from a
settlement centre (the building belongs to the nearest); the entrance can be
reached on foot and a solid body cuts off no ground; a settlement has at most 4
open sites.

The shipped list has the cottage, one workplace for each of the thirteen
professions, a stockpile, a barn, a family house (four beds), a field (eight plots
of grain with a granary of 80 units), the trading post (the merchant boat's stall)
and a market stall. The homes and workplaces a
world starts with are still listed under `settlements`; the buildings section
describes what can be added later, with the same beds, professions and stations.

### Seed

`coins` per founder; `common` goods for every founder; `founders`, the fourteen
`{"role", "flexible"}` founders of each settlement in home order;
`food_preferences`, the finished foods the world seed chooses from for each
resident; `endowment_bonus_max`, the seeded 0..=N extra units of each common good;
`extra_aptitudes`, the seeded extra trades of each founder who is not flexible.
The world seed therefore changes who prefers what, who can retrain into what, and
what each household starts with.

### Tuning

| Field | Default | Meaning |
| --- | --- | --- |
| `levy_threshold`, `levy_percent` | 200, 10 | Daily levy on coins above the threshold |
| `destitution_floor` | 40 | Top-up paid from the fund |
| `fund_reserve_per_resident` | 20 | Kept before the equal dividend |
| `margin_gap_permille`, `margin_persist_ticks`, `margin_cooldown_ticks` | 250, 6000, 6000 | When a better-paid vacancy moves a worker. 6000 (was 2400) since prices no longer collapse to one coin: with real margins the shorter hysteresis moved workers 200 times in the 14-day audit |
| `switch_cooldown_ticks` | 6000 | Ordinary cooldown after a job change |
| `tool_gather_percent`, `bare_gather_percent` | 75, 150 | Gathering time with and without the tool |
| `work_pace_slowest_percent`, `work_pace_fastest_percent` | 115, 90 | Work time at zero and at full morale |
| `cold_health_threshold`, `cold_health_drain` | 10000, 6 | Health lost per tick in deep cold |
| `winter_hearth_lightings` | 3 | Hearth logs a home burns per winter day |
| `price_ceiling_percent` | 500 | A learned price never exceeds this share of the base price |
| `winter_fuel_lead_days`, `winter_fuel_reserve` | 2, 2 | Hearth logs each home's steward stocks before and during winter |
| `profession_return_ticks` | 48000 | A resident does not return to a profession left more recently, unless an essential workplace stands empty or the pay gap is twice `margin_gap_permille` |
| `vacated_seat_ticks` | 2400 | A seat somebody just left is not refilled for this long, so two residents cannot swap trades in one pass |
| `depot_working_stock`, `depot_deposit_min` | 2, 4 | A producer keeps this many units of a good in hand and carries the rest to a store once the surplus is at least the minimum |
| `stall_restock_level` | 4 | A stall holding fewer units of a good is restocked from the settlement's storage |
| `haul_trip_units`, `haul_reservation_ticks` | 8, 1200 | Most units carried in one trip, and how long a hauler's claim on store goods or a site order lasts |
| `build_stage_ticks` | 200 | Builder ticks worked on a site in one go |
| `civic_reserve` | 250 | Coins the fund keeps back from buying building materials, for destitution grants: on a bad day the grants exceed the levy |
| `civic_budget_slack_percent` | 25 | Extra share of the open material cost the fund withholds from the dividend |
| `building_levy_percent` | 5 | When open sites and deferred projects need more than the fund may spend, every resident pays up to this share of the coins held above `destitution_floor`, earmarked for building. 0 switches the levy off |
| `relief_levy_percent` | 5 | When the fund cannot pay the destitution grants of the day, every resident pays up to this share of the coins held above `destitution_floor` to make up the difference. 0 switches the levy off |
| `gatherer_shelf_units` | 4 | While a gathered good sells above its base price, each gatherer who can work keeps this many units beyond its orders, so a price that rose in a shortage falls again once the gatherers keep up. 0 switches it off |
| `sleep_energy_per_tick` | 12 | Energy a resident asleep in a bed gains per tick, before the ordinary decay of 2. Sleep lasts to the end of the sleep block, so a night restores up to 72,000 |
| `price_floor_percent` | 0 | A safety switch: the lowest a price may fall, in percent of the base price and never below one coin. Reservation prices and stock targets keep prices up by themselves, so it is off |
| `reservation_prices`, `maker_margin_percent`, `raw_floor_percent` | true, 10, 25 | Residents and stores do not offer a good below its reservation price: a made good at the input cost of its cheapest recipe at today's quotes plus the margin, a raw good at this share of its base price (never below one coin). Stock offered below the quote's reservation is no supply for the market round, so a glut cannot push a quote below what the good costs to make. A played character's offer keeps its own minimum |
| `stock_target_days` | 2 | Makers start no new batch of a good while the settlement's unsold stock, store shelves and open offers of it exceed this many days of the settlement's use. Use is the need decay for water and food and the moving sum of recent sales for everything else; a crop's harvest good also keeps the field's `stock_per_resident`, and one batch of any recipe always counts as working stock. 0 switches it off |
| `stock_cover_pricing` | true | For goods made in batches (a recipe output) or grown once a season (a crop harvest), stock up to the island's stock target is working stock: a market round lowers a quote only for what is offered beyond it, and an order the stock on offer can fill is no scarcity. A quote above base with no glut and no shortage drifts back to base. Needs `stock_target_days` above 0 |
| `trade_window_rounds` | 24 | Market rounds (eight a day) of sales the daily use of goods that are not eaten or drunk is estimated from |
| `boat_duty_percent` | 5 | Share of the value of every merchant-boat trade (a purchase from the stores or a sale to the fund) the boat pays from its purse into the civic fund. Coins are conserved: the purse is part of the island's money |
| `inn_guest_beds` | 4 | Guest beds of an inn. A guest who rents one in its own sleep block sleeps there until morning |
| `nap_energy_per_tick` | 100 | Energy per tick of a short rest at home by day (300 ticks, or longer while warming up) |
| `planner_review_ticks`, `planner_sustain_reviews` | 1200, 3 | How often each settlement is reviewed, and how many reviews in a row a shortage must last |
| `planner_storage_overflow_units` | 40 | Surplus units without a store that count as overflow |
| `planner_demand_units` | 6 | Unmet units of a profession's goods that count as unmet demand |
| `planner_search_radius_cells` | 36 | How far from the centre the planner looks for a site |
| `planner_project_timeout_ticks` | 168000 | A civic site not finished after this long (a week) is cancelled |
| `planner_crop_shortage_units` | 4 | Unmet units of field crops that count as a grain shortage |
| `days_per_season` | 7 | Days in a season; a year has four seasons. Ages are counted in seasons, so a shorter year shortens every life with it |
| `founding_day` | -1 | Calendar day the world is founded on; day 0 is the first day of spring. -1 founds it two days before the first winter |
| `civic_reserve_grant_days`, `civic_reserve_window_days` | 3, 7 | The fund also keeps this many days of its average shortfall (destitution grants paid above the levy collected) back from building and importing, averaged over the last window of levy days, when that is more than `civic_reserve` |

### Demography

```json
"demography": {
  "enabled": true,
  "adult_age_seasons": 8, "elder_age_seasons": 36,
  "lifespan_min_seasons": 40, "lifespan_max_seasons": 52,
  "founder_max_age_seasons": 34,
  "pair_affinity": 12000, "courtship_days": 2,
  "birth_cooldown_days": 10, "birth_chance_permille": 300,
  "birth_food_units": 3, "max_children": 3,
  "child_ration_percent": 50, "child_starve_days": 3,
  "elder_speed_percent": 75, "elder_work_percent": 125,
  "arrivals_per_visit": 2, "arrival_grant_coins": 60,
  "arrival_food_per_head": 1, "max_population": 160,
  "names": ["Alba", "Beck", "Cora"]
}
```

Ages are counted in **seasons**. With the shipped seven-day season a child comes
of age at two years, an adult is an elder at nine and old age takes a resident
between ten and thirteen. Set `tuning.days_per_season` to 3 and the same lives
fit a twelve-day year.

| Field | Meaning |
| --- | --- |
| `adult_age_seasons`, `elder_age_seasons` | Life stages. A child is a member of its household, not an actor: it takes a bed, eats and does not work |
| `lifespan_min_seasons`, `lifespan_max_seasons` | Old age takes each resident on a day drawn from this band with the world seed when the resident is founded, born or lands |
| `founder_max_age_seasons` | Founders are between coming of age and this; settlers are in the younger half |
| `pair_affinity`, `courtship_days` | Two housed adults of one settlement, not close kin, court when each has this affinity for the other (a meeting adds 250) and become a couple when that has lasted this many days |
| `birth_chance_permille`, `birth_cooldown_days` | Daily chance of a birth for a couple that may have a child, and the pause after one |
| `birth_food_units`, `max_children` | Food units the parents carry between them before a birth; children one household raises at a time |
| `child_ration_percent`, `child_starve_days` | A child's hunger as a share of an adult's; days without any food before it dies. When the house has no food, the civic fund buys a hungry child one meal at a time from a store of the settlement |
| `elder_speed_percent`, `elder_work_percent` | An elder's walking speed and work duration, percent of an adult's |
| `arrivals_per_visit`, `arrival_grant_coins`, `arrival_food_per_head`, `max_population` | Settlers per boat, the grant the fund gives each (nobody lands while the fund cannot pay it), the food units per head a settlement must hold, and the headcount (children included) above which nobody lands |
| `names` | Given names of children and settlers, drawn with the seed |

A couple shares the home with room for both. A birth needs both parents adult
(not elder) under one roof, the food, and a bed: a free one in the household,
or a home the planner can build (it has a home in `buildings` and the fund can
pay for it). That second way allows one child more than the home has beds, in a
family no larger than the largest home the planner may build. A family in a home with too few beds counts as a bed shortage; the
planner builds the first home with beds for the whole family and the family
moves into any empty home that fits. Children eat from the packs of the
household's adults, who plan and buy food for them. On coming of age a child
takes a free seat (a parent's trade first), keeps its bed, inherits one aptitude
of a parent drawn with the seed and gets a quarter of each parent's coins, at
most 50 each. Without a free seat it waits, and the planner builds a workplace.

When a resident dies, the partner and the other adults of the household share
the coins and receive the plain goods that fit their packs; with nobody at home
the coins go to the civic fund. Tools, clothing and what does not fit are left
where the resident fell. Orphans are taken in by another household of the
settlement. Nothing is minted at a birth, a death or an arrival.

### Crops and fields

```json
"crops": [
  {"key": "grain", "display": "Grain",
   "seed": {"good": "village_grain", "count": 1},
   "harvest": {"good": "village_grain", "count": 6},
   "growth_ticks": 48000,
   "sow_seasons": ["spring", "summer"],
   "sow_ticks": 160, "tend_ticks": 120, "harvest_ticks": 200,
   "tend_interval_ticks": 24000, "untended_growth_percent": 60,
   "stock_per_resident": 4}
]
```

The built-in crop is the homestead's grain (same seed and harvest good) scaled to
a field plot: twice the yield and eight times the growing time of a garden bed.

| Field | Meaning |
| --- | --- |
| `seed`, `harvest` | Goods one plot takes to sow and yields when ripe, each 1 to 100. When both are the same good, the seed of the next sowing stays in the field at harvest and the farmer carries the rest |
| `growth_ticks` | Growing ticks from sowing to ripe on clear days (100 to 2000000) |
| `sow_seasons` | Seasons a plot may be sown in; never winter. A plot is only sown when `growth_ticks` remain before winter |
| `sow_ticks`, `tend_ticks`, `harvest_ticks` | Work ticks, 20 to 10000; the tool of the harvest good and morale shorten them |
| `tend_interval_ticks`, `untended_growth_percent` | A growing plot wants tending this often; one left for two intervals grows at this share |
| `stock_per_resident` | Farmers sow while the settlement holds, or has growing, fewer units of the harvest good than this per resident; 0 always sows. It keeps fields from growing a glut and makes them follow the headcount |

Growth runs in rounds of 200 ticks at today's rate: 100% on a clear or stormy
day, `weather.rain_growth_percent` in rain, nothing under snow, and scaled by
`weather.drought_growth_percent` in a drought. On the first day of winter every
plot is cleared, sown or ripe, and the field lies fallow until spring.

Every settlement is founded with one building that has a founding `field` (the
first in `buildings`), beside its farm, with seed for every plot. It costs nothing and is
not counted as built. The planner commissions another when field crops have
stayed short by `planner_crop_shortage_units` for `planner_sustain_reviews`
reviews outside winter with every plot of the settlement sown. Wild grain is a
finite founding reserve (`patches`: `"finite": true`); grain for the mill comes
from fields.

### Weather

```json
"weather": {
  "enabled": true,
  "spring": {"clear": 600, "rain": 330, "storm": 60, "snow": 10},
  "summer": {"clear": 700, "rain": 240, "storm": 60, "snow": 0},
  "autumn": {"clear": 560, "rain": 330, "storm": 90, "snow": 20},
  "winter": {"clear": 520, "rain": 50, "storm": 70, "snow": 360},
  "forecast_days": 3,
  "rain_growth_percent": 150, "rain_water_units": 4,
  "snow_warmth_loss_percent": 150,
  "drought_permille": 100, "blight_permille": 80, "harsh_winter_permille": 120,
  "shock_grace_seasons": 3,
  "drought_growth_percent": 50, "blight_loss_percent": 40,
  "harsh_warmth_loss_percent": 150, "harsh_extra_hearth_lightings": 1
}
```

The four season objects are relative odds of each kind of day. The weather of a
day is a function of the world seed and the calendar day, so a forecast is exact
and a saved world needs no weather history.

| Weather | Effect |
| --- | --- |
| clear | none |
| rain | crops grow at `rain_growth_percent`; every water source gains `rain_water_units` per 600 ticks |
| storm | the `Storm` branch of the resident tree keeps everybody from outdoor work; the boat does not land, and leaves if it is in; market prices do not move; the day before, residents plan one more day of food and water |
| snow | it is cold in the open all day, as in winter; warmth is lost at `snow_warmth_loss_percent`; nothing grows |

Shocks last one season and are rolled with the seed when it begins: a drought
(summer: no rain, growth at `drought_growth_percent`), a blight (spring or summer:
`blight_loss_percent` of the growing plots are destroyed on the first day) or a
harsh winter (rain turns to snow and half the clear days too, warmth is lost at
`harsh_warmth_loss_percent` on top of snow, and homes burn
`harsh_extra_hearth_lightings` more logs a day). `shock_grace_seasons` seasons
after the founding have none. `drought_permille` plus `blight_permille` must not
exceed 1000.

### Merchant boat

```json
"merchant": {
  "enabled": true, "settlement": "harbour", "post": "trading_post",
  "interval_days": 4, "first_visit_day": 2,
  "arrive_permille": 150, "stay_ticks": 9600,
  "purse": 1200,
  "buys": [{"good": "village_grain", "max_units": 30, "price_percent": 70, "keep_units": 40}],
  "sells": [{"good": "ingot", "units": 6, "price_percent": 130, "want_units": 3}]
}
```

The boat is the island's only external source and sink. `settlement` is the key
of the port; `post` names a `stall` building that is founded there as the trading
post. The boat lands at `arrive_permille` of the day `interval_days` after it
last sailed (the first time `first_visit_day` days after the founding) and stays
`stay_ticks`. A storm puts the landing off by a day.

- **`buys`**: from all stores and stalls of the port it buys what lies above
  `keep_units`, up to `max_units` a visit, at `price_percent` of the base price,
  and pays each owner at once (the fund for the settlement's own stock).
- **`sells`**: it brings `units` of each good. The civic fund buys while the port
  (its residents' packs and its stores) holds fewer than `want_units`, at
  `price_percent` of the base price, within what the fund may spend. The goods
  are laid in the trading post as the settlement's stock and sold on at the
  market price, which credits the fund.
- **`purse`**: the merchant's coins at the founding. Nothing refills it. Island
  coins plus purse are constant: include `village::merchant::purse(world)` in any
  conservation check. A merchant without coins buys nothing.

Only goods a store can hold are traded (no tools or clothing). Fixed prices
relative to the base price make the boat a floor under a glutted good and a
ceiling over a scarce one. Settlers land with the boat (see Demography) with two
bread and two water each, counted as imports.

## Building, storing and the civic planner

**Placing.** `{"type": "PlaceBuilding", "building": "cottage", "x": 12, "z": -34,
"rotation": 0}` is a recorded command. `x`, `z` is the entrance cell in metres;
`rotation` 0 to 3 puts the body towards +z, +x, -z or -x of the entrance. It is
checked like a terrain edit and opens a construction site. The view tests a spot
without submitting through `village::civic::placement_check(world, building, x, z,
rotation)`, which returns the same reason in words.
`{"type": "CancelBuilding", "site": 812}` abandons an unfinished site: delivered
materials become the settlement's stock in the nearest store with room, and what
does not fit is laid on the ground at the entrance.

**Who pays.** The village does, for the overseer's buildings and the planner's
alike. The civic fund pays for delivered materials. When the materials still to
come (with `civic_budget_slack_percent` on top) cost more than the fund may
spend, a **building levy** is raised at once and again at each daily levy: every
resident pays in proportion to the coins held above `destitution_floor`, never
more than `building_levy_percent` of them, and never more in total than is
needed. Those coins are earmarked: they buy building materials whatever the
reserve says, and what is left when no project remains returns with the
dividend. Destitution grants are paid first, from the whole fund. A site that
still cannot be paid for says so: `civic::sites(world)[i].funding` (also
`civic::funding`) is one sentence with the coins missing, what the fund may spend
and what the levy can raise. The journal records each levy as
`VillageBuildingLevy`. Grants have the same backing: when the fund cannot bring
everybody up to the floor at the daily levy, a **relief levy** raises the
shortfall the same way, bounded by `relief_levy_percent`
(`VillageReliefLevy`). A site view also carries `placed_at`, `last_delivery_at`
and `paid`; a building view carries `crop_plots`; a planner's last decision is a
sentence in `PlannerState.last_decision_text`; and `VillageSiteCancelled` names
the site's label, door position and cell.

**Supplying and building.** A site orders its materials like any buyer and the
civic fund pays on delivery, at the quoted price. Residents carry spare materials
there, or buy them at a store first. Spare means beyond what their funded plan
wants in the pack, and never the hearth fuel of their home (two logs). When
everything is delivered, a resident works the site in stages until `work_ticks`
are done. The finished building keeps the site's identity. Coins the fund paid
stay paid if the site is cancelled: the returned goods belong to the settlement,
and selling them pays the fund back.

**Stores and stalls.** Goods in a store still belong to whoever laid them there.
The store sells them at the quoted price while the owner is elsewhere and credits
the owner at once; goods of the settlement, or of an owner who has died, credit
the civic fund. Tools and clothing are not stored. A hauler never carries somebody
else's property: goods moved from storage to a stall or to a site are bought at
the quote and sold on at the quote.

**The planner.** Every `planner_review_ticks` each settlement is reviewed, one
settlement per tick at most. The planner first moves crowded families into an
empty home that fits and gives homeless residents a free bed.
Then, if nothing is under way, it commissions one building, in this order: a home
when residents have had no bed, or a family too few beds, for
`planner_sustain_reviews` reviews; a field on a lasting crop shortage with every
plot sown; storage when unstored surplus
has stayed above `planner_storage_overflow_units`; a workplace when a grown child
has waited for a seat; a workplace when a profession's
goods have stayed short by `planner_demand_units` with every seat taken. It takes
the first `civic` building of that kind in the list, picks the nearest valid spot
to the centre in a fixed order and waits when the fund cannot pay. The daily
dividend returns only what open sites do not need. Every decision is a
`VillageCivicProject` journal effect with its reason.

## Validation errors

Each message starts with `economy:` and names the entry. Examples:

- ``recipe `village_bake` references unknown good `village_rye` ``
- ``recipe `village_mill` is part of a cycle with no source: input `bread` can never be obtained``
- ``recipe `village_split` is part of a recipe cycle (a good is its own ingredient)``
- ``profession `miner` has no producible output: `ingot` has neither a recipe of this profession nor a resource patch``
- ``profession `baker` cannot buy `village_flour`, an input of its recipe `village_bake`; add it to wants``
- ``recipe `village_bake` duration must be 1..=100000 ticks`` (a negative duration fails while the JSON is parsed)
- ``duplicate good `axe` ``, ``duplicate recipe `village_mill` ``, ``duplicate profession `farmer` ``
- ``good `village_charcoal` needs an item section (it is not an engine item kind)``
- ``building `granary` cost references unknown good `marble` ``
- ``building `hall` cost `axe` is a tool or clothing and cannot be a building material``
- ``building `forge` is a workplace for `smith`, whose station is `village_smelt`, not `village_saw` ``
- ``building `shed` capacity must be 1..=10000 units``, ``duplicate building `cottage` ``

## Walkthrough: add a good

This adds charcoal, burned by carpenters and stocked by smiths, without touching Rust.
`tests/island_economy_data.rs` runs exactly this edit and checks that the good is
produced and traded.

1. Add the good to `goods`:

   ```json
   {"key": "village_charcoal", "display": "charcoal", "base_price": 3,
    "stock_limit": 20, "tags": ["fuel"], "item": {"weight_g": 300, "max_stack": 20}}
   ```

2. Add the recipe to `recipes`:

   ```json
   {"key": "village_burn_charcoal", "display": "Burn charcoal",
    "station": "village_saw", "inputs": [{"good": "log", "count": 1}],
    "outputs": [{"good": "village_charcoal", "count": 2}], "duration": 120}
   ```

3. In the `carpenter` profession, append `"village_charcoal"` to `outputs` and
   `"village_burn_charcoal"` to `recipes`.
4. Create demand. In the `smith` profession, append `"village_charcoal"` to
   `wants` and add `"keeps": [{"good": "village_charcoal", "count": 2}]`.
5. Apply the same edit to the scenario file if you start worlds from it, rebuild,
   and start a new world.

Production follows funded demand. A good that nobody keeps, eats or uses as a
recipe input is never ordered, so step 4 matters.

## Walkthrough: add a building

This adds a granary, a small store, without touching Rust.
`a_storage_building_defined_only_in_json_is_built_by_the_residents` in
`tests/island_buildings.rs` makes exactly this edit and watches residents build it.

1. Add the building to `buildings`:

   ```json
   {"id": "granary", "display": "Granary",
    "cost": [{"good": "log", "count": 2}], "work_ticks": 200,
    "footprint": {"width": 3, "depth": 3},
    "kind": {"type": "storage", "capacity": 80}, "solid": false}
   ```

2. Apply the same edit to the scenario file if you start worlds from it, rebuild
   and start a new world.
3. Place it. From the game, the overseer's build menu lists every entry of
   `buildings`. Headless, submit the command with an entrance cell that
   `placement_check` accepts:

   ```json
   {"type": "PlaceBuilding", "building": "granary", "x": 3, "z": -41, "rotation": 2}
   ```

4. Watch. The journal shows `VillageSitePlaced`, a `VillageSiteDelivery` for each
   load of logs a resident brings (the civic fund pays), `VillageSiteProgress` for
   each stage of work and `VillageBuildingCompleted`. Producers then lay surplus in
   it (`VillageGoodsStored`) and neighbours buy from it (`VillageTrade` with
   `"store": true`, and a `VillageStallSale` for the owner).

The fund must hold more than its reserve before it buys anything; a new
world's fund fills at the first daily levy. The reserve is `civic_reserve`, or
`civic_reserve_grant_days` days of the average shortfall (grants above the levy)
of the last `civic_reserve_window_days` levy days when that is more
(`village::civic::reserve(world)`), so a run of days on which the grants exceed
the levy raises it before building and importing can drain the fund. The daily dividend returns only what
is above the reserve, the open sites' needs and the cost of a project a planner
has decided on and cannot pay yet. To let the planner choose your building
by itself, list it before the other buildings of its kind and keep `civic` true.

## What is still Rust

- The set of professions (`Role`) and the fourteen founders per settlement.
- Mechanics that name a good: hearths burn `log`, residents drink `village_water`,
  wear `village_warm_clothes`, hunt with `village_spear`, fish with
  `village_fishing_rod`, and forage `village_berries` when food runs out.
- Hunting yields and wildlife; the crop beds of the player's homestead; the materials of the player's homestead
  cottage (buildings placed with `PlaceBuilding` take theirs from `buildings`).
- The kinds of building (`home`, `workplace`, `storage`, `stall`) and the fixed
  placement rules listed under Buildings. A field is the `field` attribute of a
  storage building, not a fifth kind.
- What the weather kinds and shocks do, the order of the planner's reasons, and
  the rules of pairing, birth and inheritance; their numbers are data.
- The homes and workplaces a world is founded with: they are authored under
  `settlements`, not spawned from `buildings`.
- Two player-panel lookups (the edible check and a fallback price) still read the
  older Rust tables, so a new food or good is traded but not fully labelled there.

## Live mode (`economy.live`)

Everything a played character does beyond work is tuned here. `enabled: false`
switches all of it off.

| Field | Default | Meaning |
|---|---|---|
| `tiers` | stranger 0, acquaintance 5,000, friend 20,000, close friend 50,000 | Bond tiers, ascending by `score` (0 to 100,000). `discount_percent` is taken off when the played character buys from the resident; `gift_back_days` makes the resident give one unit of what it sells when talked to, at most that often; `invites` announces gatherings; `helps` (friend and close friend) makes the resident help on the homestead, see below. There are no debts between residents and a played character, so a bond has no "unpaid debt" effect |
| `talk_points`, `trade_points` | 1,000, 1,500 | Points for the first talk and the first trade of a day with a resident |
| `gift_liked_points`, `gift_plain_points`, `gift_disliked_points` | 6,000, 2,000, -2,000 | Points for the first gift of a day. Each resident likes one good of `gift_goods` and dislikes another, by seed |
| `neglect_after_days`, `neglect_points_per_day` | 3, 500 | A bond without contact for that long loses points every morning |
| `resident_neglect_points_per_day` | 25 | Fondness between residents fades by this much a day; a meeting adds 250 |
| `greeting_permille`, `greetings` | 250, four lines | Every 300 ticks a free resident within three metres of a played character speaks first with this chance, once a day; the line is chosen by bond tier |
| `gatherings` | harvest feast (autumn day 2), midwinter fire (winter day 4) | `season`, `day` of the season from 1, `from_permille` to `to_permille` of the day, and the `morale` each attendant gains once. During a gathering every free resident whose needs are all above `gathering_needs_above` walks to the square of its settlement |
| `gathering_needs_above` | 50,000 | A resident goes to a gathering only while every need is above this; one in need sees to it first. Without it the midwinter fire drew hungry, cold residents away from food and fuel (14-day audit: Hunger low 33,214 without gatherings, 20,990 with them) |
| `help_needs_above`, `help_work_ticks` | 40,000, 300 | A resident at a helping tier, in its leisure block, in the settlement of the played character's cottage, not busy and with every need above `help_needs_above`, helps once a day: it hands over one unit it carries of a material the open upgrade misses (paid at today's quote from the upgrade's escrow), or works `help_work_ticks` on a supplied upgrade, or harvests a ripe bed of the owner's plots into the chest or the owner's pack. Journal `VillageFriendHelped` |
| `haul_fee_percent` | 20 | Fee on an upgrade's buy order, in percent of the goods' price; goods and fee are held in escrow from the wallet when the order is placed and paid to the resident who brings them |
| `builder_wage` | 4 | Coins a hired builder earns per stage of `tuning.build_stage_ticks`; the whole hire is held in escrow when the builder is hired |
| `mourning_days`, `mourning_morale` | 3, 15,000 | When somebody is laid to rest, the household and everybody fond of the dead lose that much morale and mourn for that long |
| `goals` | seven goals: a cottage, the storage chest, the second plot, the tool shed, the larger cottage, a friend at the helping tier, a gathering | The chain a played character works through, in order. `kind` is `own_cottage`, `harvest`, `gift`, `friend` (with `tier`, the tier index that counts), `sleep`, `gathering`, `trade`, `talk`, `upgrade` (tiers built), `coins` or `homestead` (with `homestead`, the tier key that must stand); `count` is how many |
| `homestead` | storage chest, second plot, tool shed, larger cottage | Upgrades of the cottage, each a private building site: `after` (a tier first), `goal` (a goal first), `cost` in goods delivered to the site, `work_ticks` of work once every good is in, and the effect: `beds` added, `plots` (further garden plots allowed), `gather_percent` of the owner's gathering time, `storage` (units of the homestead chest) and `building` (the catalogue home the cottage body grows into, which also sets its beds; the larger cottage grows into the 5 x 6 m family house with four beds) |
| `sleep_from_permille` | 600 | From this part of the day a played character may sleep until morning |

In the living world the homestead cottage is the catalogue `cottage` (4 x 5 m,
two beds) behind the plot's door, and a plot is accepted for a cottage only
when the larger body (the `building` of a tier) also fits there. Tool tiers act
through `gather_percent` of the tier, not through the tool goods: the tool goods
keep `tuning.tool_gather_percent` and their `durability`.

The commands are `LiveAct` (`sleep`, `wake`, `gift`, `repeat`, `stop_repeat`,
`upgrade`, `cancel_upgrade`, `deliver`, `order`, `hire`, `store`, `take`); see
`docs/living-world.md`. Journal effects: `VillageSleep`, `VillageDaySummary`,
`VillageGift`, `VillageGiftReturned`, `VillageBondTier`, `VillageGreeting`,
`VillageInvitation`, `VillageRepeatStopped`, `VillageHomesteadUpgradeStarted`,
`VillageHomesteadDelivered`, `VillageHomesteadOrder`,
`VillageHomesteadBuilderHired`, `VillageHomesteadBuilderWorked`,
`VillageHomesteadUpgradeCancelled`, `VillageHomesteadUpgraded`,
`VillageChestStored`, `VillageChestTaken`, `VillageFriendHelped`,
`VillageGatheringBegan`, `VillageGathering`, `VillageMourning`,
`VillageBoatDuty`.
